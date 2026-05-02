//! `kb-cli` 到 `kb-core` 的适配层：
//! - 将 CLI 本地状态模型映射到 core ports
//! - 连接文件存储索引实现
//! - 提供默认本地检索器与占位应答引擎

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

use kb_core::domain::{
    GraphSnapshot, IndexSnapshot, KnowledgeChunk, KnowledgeSnapshot, SearchProjectionSnapshot,
    SemanticProviderInfo,
};
use kb_core::{
    AnswerEngine, AnswerMeta, FileContentReader, GraphStore, IndexStore, KnowledgeStore,
    RetrievedContext, Retriever, SearchStore, SemanticExtractRequest, SemanticExtractResponse,
    SemanticExtractor, StateRef, StateSnapshot, StateStore, WikiStore,
};

use crate::state::{load_state, save_state, RefEntry, State};

pub struct FsStateStore {
    cwd: PathBuf,
}

impl FsStateStore {
    /// 基于当前工作目录创建状态存储适配器。
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl StateStore for FsStateStore {
    // 适配层负责在 CLI State 与 core StateSnapshot 之间做结构转换。
    fn load_state(&self) -> Result<StateSnapshot, String> {
        let cli_state = load_state(&self.cwd).map_err(|err| err.message.to_string())?;
        Ok(StateSnapshot {
            refs: cli_state
                .refs
                .into_iter()
                .map(|r| StateRef {
                    alias: r.alias,
                    path: r.path,
                    category: r.category,
                    perms: r.perms,
                })
                .collect(),
        })
    }

    fn save_state(&self, state: &StateSnapshot) -> Result<(), String> {
        let cli_state = State {
            refs: state
                .refs
                .iter()
                .map(|r| RefEntry {
                    alias: r.alias.clone(),
                    path: r.path.clone(),
                    category: r.category.clone(),
                    perms: r.perms.clone(),
                })
                .collect(),
        };
        save_state(&self.cwd, &cli_state).map_err(|err| err.message.to_string())
    }
}

pub struct FsIndexStore {
    cwd: PathBuf,
}

impl FsIndexStore {
    /// 基于当前工作目录创建索引存储适配器。
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl IndexStore for FsIndexStore {
    fn read_index(&self) -> Result<Option<IndexSnapshot>, String> {
        kb_storage_fs::read_index_files(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_index(&self, snapshot: &IndexSnapshot) -> Result<(), String> {
        kb_storage_fs::write_index_files(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

pub struct DefaultRetriever;

impl Retriever for DefaultRetriever {
    /// 默认本地检索流程：
    /// 1) 计算 query token
    /// 2) 对 present 记录打分并排序
    /// 3) 生成稳定摘要片段
    fn retrieve(
        &self,
        question: &str,
        top: usize,
        snapshot: &IndexSnapshot,
    ) -> Result<Vec<RetrievedContext>, String> {
        // 查询文本统一降小写，便于后续 token 匹配与包含匹配。
        let normalized_question = question.to_ascii_lowercase();
        let question_tokens = normalized_tokens(question);

        let present_records: Vec<&kb_core::domain::IndexRecord> = snapshot
            .records
            .iter()
            .filter(|record| record.present)
            .collect();

        let mut ranked: Vec<(f64, &kb_core::domain::IndexRecord)> = present_records
            .iter()
            .map(|record| {
                (
                    score_record(record, &normalized_question, &question_tokens),
                    *record,
                )
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        ranked.sort_by(|(score_a, rec_a), (score_b, rec_b)| {
            score_b
                .partial_cmp(score_a)
                .unwrap_or(Ordering::Equal)
                .then_with(|| rec_a.alias.cmp(&rec_b.alias))
                .then_with(|| rec_a.path.cmp(&rec_b.path))
        });

        if ranked.is_empty() {
            // 零命中时返回稳定兜底结果，保证 ask 输出结构可预测。
            ranked = present_records
                .into_iter()
                .map(|record| (0.01, record))
                .collect();
            ranked.sort_by(|(_, rec_a), (_, rec_b)| {
                rec_a
                    .alias
                    .cmp(&rec_b.alias)
                    .then_with(|| rec_a.path.cmp(&rec_b.path))
            });
        }

        Ok(ranked
            .into_iter()
            .take(top)
            .map(|(score, record)| RetrievedContext {
                alias: record.alias.clone(),
                snippet: snippet_for(record, &question_tokens),
                score,
                source_layer: Some("l1_index".to_string()),
                record_id: Some(record.alias.clone()),
                chunk_id: None,
            })
            .collect())
    }
}

pub struct LayeredRetriever {
    search_snapshot: Option<SearchProjectionSnapshot>,
    knowledge_snapshot: Option<KnowledgeSnapshot>,
    graph_snapshot: Option<GraphSnapshot>,
    fallback: DefaultRetriever,
}

impl LayeredRetriever {
    pub fn new(
        search_snapshot: Option<SearchProjectionSnapshot>,
        knowledge_snapshot: Option<KnowledgeSnapshot>,
        graph_snapshot: Option<GraphSnapshot>,
    ) -> Self {
        Self {
            search_snapshot,
            knowledge_snapshot,
            graph_snapshot,
            fallback: DefaultRetriever,
        }
    }
}

impl Retriever for LayeredRetriever {
    fn retrieve(
        &self,
        question: &str,
        top: usize,
        snapshot: &IndexSnapshot,
    ) -> Result<Vec<RetrievedContext>, String> {
        let limit = top.max(1);
        let normalized_question = question.to_ascii_lowercase();
        let question_tokens = normalized_tokens(question);
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();

        let mut knowledge_chunk_map = BTreeMap::new();
        if let Some(knowledge) = &self.knowledge_snapshot {
            for entry in &knowledge.entries {
                if entry.status != kb_core::KnowledgeStatus::Active {
                    continue;
                }
                for chunk in &entry.chunks {
                    knowledge_chunk_map.insert(
                        format!("{}|{}", entry.record_id, chunk.chunk_id),
                        (entry, chunk),
                    );
                }
            }
        }

        if let Some(search) = &self.search_snapshot {
            let mut ranked = search
                .hits
                .iter()
                .filter_map(|hit| {
                    let chunk_key = format!("{}|{}", hit.record_id, hit.chunk_id);
                    let (entry, chunk) = knowledge_chunk_map.get(chunk_key.as_str())?;
                    let score = score_search_hit(hit, &normalized_question, &question_tokens);
                    if score <= 0.0 {
                        return None;
                    }
                    Some((score, hit, *entry, *chunk))
                })
                .collect::<Vec<_>>();
            ranked.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.1.record_id.cmp(&b.1.record_id))
                    .then_with(|| a.1.chunk_id.cmp(&b.1.chunk_id))
            });

            for (score, hit, entry, chunk) in ranked {
                if out.len() >= limit {
                    break;
                }
                let key = format!("{}|{}", hit.record_id, hit.chunk_id);
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);
                out.push(RetrievedContext {
                    alias: hit.record_id.clone(),
                    snippet: snippet_from_chunk(entry, chunk, &question_tokens),
                    score,
                    source_layer: Some("l3_projection".to_string()),
                    record_id: Some(hit.record_id.clone()),
                    chunk_id: Some(hit.chunk_id.clone()),
                });
            }
        }

        if out.len() < limit {
            if let Some(knowledge) = &self.knowledge_snapshot {
                let mut ranked = Vec::new();
                for entry in &knowledge.entries {
                    if entry.status != kb_core::KnowledgeStatus::Active {
                        continue;
                    }
                    for chunk in &entry.chunks {
                        let key = format!("{}|{}", entry.record_id, chunk.chunk_id);
                        if seen.contains(&key) {
                            continue;
                        }
                        let score = score_knowledge_chunk(
                            entry,
                            chunk,
                            &normalized_question,
                            &question_tokens,
                        );
                        if score <= 0.0 {
                            continue;
                        }
                        ranked.push((score, entry, chunk));
                    }
                }
                ranked.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.1.record_id.cmp(&b.1.record_id))
                        .then_with(|| a.2.chunk_id.cmp(&b.2.chunk_id))
                });

                for (score, entry, chunk) in ranked {
                    if out.len() >= limit {
                        break;
                    }
                    let key = format!("{}|{}", entry.record_id, chunk.chunk_id);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.insert(key);
                    out.push(RetrievedContext {
                        alias: entry.record_id.clone(),
                        snippet: snippet_from_chunk(entry, chunk, &question_tokens),
                        score,
                        source_layer: Some("l2_semantic".to_string()),
                        record_id: Some(entry.record_id.clone()),
                        chunk_id: Some(chunk.chunk_id.clone()),
                    });
                }
            }
        }

        if out.len() < limit {
            if let Some(graph) = &self.graph_snapshot {
                let mut ranked = graph
                    .edges
                    .iter()
                    .filter_map(|edge| {
                        let evidence = edge.evidence.first()?;
                        let summary = format!(
                            "{} -> {} | {}",
                            edge.from_node_id,
                            edge.to_node_id,
                            edge.evidence
                                .iter()
                                .map(|item| item.reason.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        );
                        let score =
                            score_graph_edge(&summary, &normalized_question, &question_tokens);
                        if score <= 0.0 {
                            return None;
                        }
                        Some((score, edge, evidence, summary))
                    })
                    .collect::<Vec<_>>();
                ranked.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.1.edge_id.cmp(&b.1.edge_id))
                });

                for (score, edge, evidence, summary) in ranked {
                    if out.len() >= limit {
                        break;
                    }
                    let key = format!(
                        "{}|{}",
                        evidence.record_id,
                        evidence.chunk_id.as_deref().unwrap_or("-")
                    );
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.insert(key);
                    out.push(RetrievedContext {
                        alias: evidence.record_id.clone(),
                        snippet: stable_excerpt_window(&summary, &question_tokens, 180),
                        score,
                        source_layer: Some("l4_graph".to_string()),
                        record_id: Some(evidence.record_id.clone()),
                        chunk_id: evidence.chunk_id.clone(),
                    });
                    let _ = edge;
                }
            }
        }

        if out.len() < limit {
            let fallback = self.fallback.retrieve(question, limit, snapshot)?;
            for mut item in fallback {
                if out.len() >= limit {
                    break;
                }
                let key = format!(
                    "{}|{}",
                    item.record_id.clone().unwrap_or_else(|| item.alias.clone()),
                    item.chunk_id.clone().unwrap_or_else(|| "-".to_string())
                );
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);
                item.source_layer = Some("l1_index".to_string());
                if item.record_id.is_none() {
                    item.record_id = Some(item.alias.clone());
                }
                out.push(item);
            }
        }

        Ok(out.into_iter().take(limit).collect())
    }
}

pub struct FsKnowledgeStore {
    cwd: PathBuf,
}

impl FsKnowledgeStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl KnowledgeStore for FsKnowledgeStore {
    fn read_knowledge(&self) -> Result<Option<kb_core::KnowledgeSnapshot>, String> {
        kb_storage_fs::read_knowledge_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_knowledge(&self, snapshot: &kb_core::KnowledgeSnapshot) -> Result<(), String> {
        kb_storage_fs::write_knowledge_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

pub struct FsSearchStore {
    cwd: PathBuf,
}

impl FsSearchStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl SearchStore for FsSearchStore {
    fn read_search(&self) -> Result<Option<kb_core::SearchProjectionSnapshot>, String> {
        kb_storage_fs::read_search_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_search(&self, snapshot: &kb_core::SearchProjectionSnapshot) -> Result<(), String> {
        kb_storage_fs::write_search_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

pub struct FsGraphStore {
    cwd: PathBuf,
}

impl FsGraphStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl GraphStore for FsGraphStore {
    fn read_graph(&self) -> Result<Option<kb_core::GraphSnapshot>, String> {
        kb_storage_fs::read_graph_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_graph(&self, snapshot: &kb_core::GraphSnapshot) -> Result<(), String> {
        kb_storage_fs::write_graph_file(&self.cwd, snapshot)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

pub struct FsWikiStore {
    cwd: PathBuf,
}

impl FsWikiStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl WikiStore for FsWikiStore {
    fn read_wiki_manifest(&self) -> Result<Option<kb_core::WikiManifestSnapshot>, String> {
        kb_storage_fs::read_wiki_manifest_file(&self.cwd).map_err(|err| err.to_string())
    }

    fn write_wiki_view(
        &self,
        snapshot: &kb_core::WikiViewSnapshot,
        removed_paths: &[String],
    ) -> Result<(), String> {
        kb_storage_fs::write_wiki_view_files(&self.cwd, snapshot, removed_paths)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

pub struct FsFileContentReader {
    cwd: PathBuf,
}

impl FsFileContentReader {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl FileContentReader for FsFileContentReader {
    fn read_text(&self, path: &str) -> Result<String, String> {
        let abs = self.cwd.join(path.trim_start_matches("./"));
        fs::read_to_string(&abs).map_err(|err| format!("failed to read {}: {}", path, err))
    }
}

pub struct RuleBasedSemanticExtractor;

impl RuleBasedSemanticExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl SemanticExtractor for RuleBasedSemanticExtractor {
    fn extract(&self, request: &SemanticExtractRequest) -> Result<SemanticExtractResponse, String> {
        let summary = request
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(|line| stable_truncate(line, 240))
            .unwrap_or_else(|| format!("Content from {}", request.path));

        let mut tags = BTreeSet::new();
        for token in request
            .path
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            tags.insert(token.to_ascii_lowercase());
        }

        let mut key_concepts = request
            .text
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix('#'))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .take(4)
            .collect::<Vec<_>>();
        if key_concepts.is_empty() {
            key_concepts = tags.iter().take(3).cloned().collect();
        }

        let mut chunks = Vec::new();
        for (idx, raw_chunk) in request.text.split("\n\n").enumerate().take(8) {
            let chunk_text = raw_chunk.trim();
            if chunk_text.is_empty() {
                continue;
            }
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:{:04}", request.record_id, idx + 1),
                byte_range: [0, chunk_text.len() as u64],
                content_signature: format!("len:{}", chunk_text.len()),
                summary: stable_truncate(chunk_text, 120),
                preview: stable_truncate(chunk_text, 200),
            });
        }
        if chunks.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, request.text.len() as u64],
                content_signature: format!("len:{}", request.text.len()),
                summary: summary.clone(),
                preview: stable_truncate(&request.text, 200),
            });
        }

        Ok(SemanticExtractResponse {
            summary,
            tags: tags.into_iter().take(8).collect(),
            key_concepts,
            chunks,
            provider: SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "rule-based-v1".to_string(),
            },
        })
    }
}

fn normalized_tokens(input: &str) -> BTreeSet<String> {
    input
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

fn score_record(
    record: &kb_core::domain::IndexRecord,
    normalized_question: &str,
    question_tokens: &BTreeSet<String>,
) -> f64 {
    if question_tokens.is_empty() {
        return 0.0;
    }

    let record_tokens = normalized_tokens(&format!(
        "{} {} {} {}",
        record.alias,
        record.path,
        record.category,
        record.excerpt.as_deref().unwrap_or("")
    ));
    if record_tokens.is_empty() {
        return 0.0;
    }

    let matched = question_tokens
        .iter()
        .filter(|q| {
            record_tokens
                .iter()
                .any(|d| d == *q || d.contains(q.as_str()) || q.contains(d.as_str()))
        })
        .count();

    if matched == 0 {
        return 0.0;
    }

    // 评分模型：
    // overlap 为查询词覆盖率，density 为命中密度，alias/path 命中提供轻量加权。
    let overlap = matched as f64 / question_tokens.len() as f64;
    let density = matched as f64 / record_tokens.len() as f64;
    let alias_hit = if !normalized_question.trim().is_empty()
        && record
            .alias
            .to_ascii_lowercase()
            .contains(normalized_question)
    {
        0.1
    } else {
        0.0
    };
    let path_hit = if !normalized_question.trim().is_empty()
        && record
            .path
            .to_ascii_lowercase()
            .contains(normalized_question)
    {
        0.05
    } else {
        0.0
    };

    (0.75 * overlap + 0.15 * density + alias_hit + path_hit).min(1.0)
}

fn snippet_for(
    record: &kb_core::domain::IndexRecord,
    question_tokens: &BTreeSet<String>,
) -> String {
    if let Some(excerpt) = record.excerpt.as_deref() {
        let trimmed = excerpt.trim();
        if !trimmed.is_empty() {
            return stable_excerpt_window(trimmed, question_tokens, 180);
        }
    }
    // excerpt 缺失时使用可解释的兜底文案，避免返回空字符串。
    format!("Matched {} at {}", record.alias, record.path)
}

fn snippet_from_chunk(
    entry: &kb_core::KnowledgeEntry,
    chunk: &kb_core::KnowledgeChunk,
    question_tokens: &BTreeSet<String>,
) -> String {
    let base = if chunk.preview.trim().is_empty() {
        if chunk.summary.trim().is_empty() {
            entry.summary.as_str()
        } else {
            chunk.summary.as_str()
        }
    } else {
        chunk.preview.as_str()
    };
    if base.trim().is_empty() {
        return format!("Matched {} at {}", entry.record_id, entry.path);
    }
    stable_excerpt_window(base.trim(), question_tokens, 180)
}

fn score_search_hit(
    hit: &kb_core::SearchHit,
    normalized_question: &str,
    question_tokens: &BTreeSet<String>,
) -> f64 {
    let matched = question_tokens
        .iter()
        .filter(|token| {
            hit.matched_terms.iter().any(|term| {
                term == *token
                    || term.contains(token.as_str())
                    || token.contains(term.as_str())
                    || hit.record_id.to_ascii_lowercase().contains(token.as_str())
            })
        })
        .count();
    let overlap = if question_tokens.is_empty() {
        0.0
    } else {
        matched as f64 / question_tokens.len() as f64
    };
    let alias_hit = if !normalized_question.trim().is_empty()
        && hit
            .record_id
            .to_ascii_lowercase()
            .contains(normalized_question)
    {
        0.05
    } else {
        0.0
    };
    (0.7 * hit.score + 0.25 * overlap + alias_hit).min(1.0)
}

fn score_knowledge_chunk(
    entry: &kb_core::KnowledgeEntry,
    chunk: &kb_core::KnowledgeChunk,
    normalized_question: &str,
    question_tokens: &BTreeSet<String>,
) -> f64 {
    let doc = format!(
        "{} {} {} {} {}",
        entry.record_id,
        entry.summary,
        entry.tags.join(" "),
        entry.key_concepts.join(" "),
        chunk.summary
    )
    .to_ascii_lowercase();
    let doc_tokens = normalized_tokens(&doc);
    let matched = question_tokens
        .iter()
        .filter(|q| {
            doc_tokens
                .iter()
                .any(|d| d == *q || d.contains(q.as_str()) || q.contains(d.as_str()))
        })
        .count();
    if matched == 0 {
        return 0.0;
    }
    let overlap = matched as f64 / question_tokens.len().max(1) as f64;
    let density = matched as f64 / doc_tokens.len().max(1) as f64;
    let alias_hit = if !normalized_question.trim().is_empty()
        && entry
            .record_id
            .to_ascii_lowercase()
            .contains(normalized_question)
    {
        0.05
    } else {
        0.0
    };
    (0.8 * overlap + 0.15 * density + alias_hit).min(1.0)
}

fn score_graph_edge(
    summary: &str,
    normalized_question: &str,
    question_tokens: &BTreeSet<String>,
) -> f64 {
    let lowered = summary.to_ascii_lowercase();
    let doc_tokens = normalized_tokens(&lowered);
    let matched = question_tokens
        .iter()
        .filter(|q| {
            doc_tokens
                .iter()
                .any(|d| d == *q || d.contains(q.as_str()) || q.contains(d.as_str()))
        })
        .count();
    if matched == 0 {
        return 0.0;
    }
    let overlap = matched as f64 / question_tokens.len().max(1) as f64;
    let direct = if !normalized_question.trim().is_empty() && lowered.contains(normalized_question)
    {
        0.1
    } else {
        0.0
    };
    (0.85 * overlap + direct).min(1.0)
}

fn stable_excerpt_window(
    text: &str,
    question_tokens: &BTreeSet<String>,
    max_chars: usize,
) -> String {
    // 按 grapheme cluster 切片，避免把 emoji/组合字符截断成乱码。
    let grapheme_starts: Vec<usize> = UnicodeSegmentation::grapheme_indices(text, true)
        .map(|(idx, _)| idx)
        .collect();
    let grapheme_len = grapheme_starts.len();
    if grapheme_len <= max_chars {
        return text.to_string();
    }

    let mut hit_diff = vec![0i32; grapheme_len + 1];
    let lowered = text.to_ascii_lowercase();

    for token in question_tokens {
        if token.is_empty() {
            continue;
        }
        let mut search_from = 0usize;
        while let Some(pos) = lowered[search_from..].find(token) {
            let start_b = search_from + pos;
            let end_b = start_b + token.len();
            let start_g = grapheme_floor_index(&grapheme_starts, start_b);
            let end_g = grapheme_end_index(&grapheme_starts, end_b, text.len());
            if start_g < end_g && end_g <= grapheme_len {
                hit_diff[start_g] += 1;
                hit_diff[end_g] -= 1;
            }
            search_from = end_b;
            if search_from >= lowered.len() {
                break;
            }
        }
    }

    // 用差分 + 前缀和做滑窗打分，避免每个窗口重复 O(window) 求和。
    let mut token_hits = vec![0usize; grapheme_len];
    let mut running = 0i32;
    for i in 0..grapheme_len {
        running += hit_diff[i];
        token_hits[i] = running.max(0) as usize;
    }
    let mut prefix_hits = vec![0usize; grapheme_len + 1];
    for i in 0..grapheme_len {
        prefix_hits[i + 1] = prefix_hits[i] + token_hits[i];
    }

    let mut best_start = 0usize;
    let mut best_end = max_chars.min(grapheme_len);
    let mut best_score = 0usize;
    for start in 0..=(grapheme_len - max_chars) {
        let end = start + max_chars;
        let score = prefix_hits[end] - prefix_hits[start];
        if score > best_score
            || (score == best_score
                && (start < best_start || (start == best_start && end < best_end)))
        {
            best_score = score;
            best_start = start;
            best_end = end;
        }
    }

    if best_score == 0 {
        // 无命中时走稳定截断，确保输出长度和展示行为一致。
        return stable_truncate(text, max_chars);
    }

    let start = best_start;
    let end = best_end.min(grapheme_len);

    let start_b = grapheme_starts[start];
    let end_b = if end < grapheme_len {
        grapheme_starts[end]
    } else {
        text.len()
    };

    let mut out = text[start_b..end_b].to_string();
    if start > 0 {
        out = format!("...{}", out);
    }
    if end < grapheme_len {
        out.push_str("...");
    }
    out
}

fn stable_truncate(text: &str, max_chars: usize) -> String {
    let grapheme_starts: Vec<usize> = UnicodeSegmentation::grapheme_indices(text, true)
        .map(|(idx, _)| idx)
        .collect();
    if grapheme_starts.len() <= max_chars {
        return text.to_string();
    }
    let end_b = grapheme_starts
        .get(max_chars)
        .copied()
        .unwrap_or(text.len());
    let mut out = text[..end_b].to_string();
    out.push_str("...");
    out
}

fn grapheme_floor_index(starts: &[usize], byte_index: usize) -> usize {
    starts
        .partition_point(|&s| s <= byte_index)
        .saturating_sub(1)
}

fn grapheme_end_index(starts: &[usize], byte_end: usize, text_len: usize) -> usize {
    if byte_end >= text_len {
        return starts.len();
    }
    if byte_end == 0 {
        return 0;
    }
    let last_byte = byte_end - 1;
    starts.partition_point(|&s| s <= last_byte)
}

pub struct DisabledAnswerEngine;

impl AnswerEngine for DisabledAnswerEngine {
    // 当前阶段不接外部 LLM，只返回元信息占位。
    fn meta(&self, _question: &str, _contexts: &[RetrievedContext]) -> Result<AnswerMeta, String> {
        Ok(AnswerMeta {
            engine_status: "disabled".to_string(),
            provider: None,
            trace_id: None,
        })
    }
}

pub struct ConfiguredAnswerEngine {
    provider: String,
}

impl ConfiguredAnswerEngine {
    pub fn new(provider: String) -> Self {
        Self { provider }
    }
}

impl AnswerEngine for ConfiguredAnswerEngine {
    fn meta(&self, _question: &str, _contexts: &[RetrievedContext]) -> Result<AnswerMeta, String> {
        Ok(AnswerMeta {
            engine_status: "configured".to_string(),
            provider: Some(self.provider.clone()),
            trace_id: None,
        })
    }
}
