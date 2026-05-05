use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use kb_core::domain::{
    GraphSnapshot, IndexRecord, IndexSnapshot, KnowledgeEntry, KnowledgeSnapshot,
};
use kb_core::{RetrievedContext, Retriever, SearchProjectionSnapshot};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultRetriever;

impl Retriever for DefaultRetriever {
    fn retrieve(
        &self,
        question: &str,
        top: usize,
        snapshot: &IndexSnapshot,
    ) -> Result<Vec<RetrievedContext>, String> {
        let normalized_question = question.to_ascii_lowercase();
        let question_tokens = normalized_tokens(question);

        let present_records: Vec<&IndexRecord> = snapshot
            .records
            .iter()
            .filter(|record| record.present)
            .collect();

        let mut ranked: Vec<(f64, &IndexRecord)> = present_records
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

#[derive(Debug, Clone)]
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

                for (score, _edge, evidence, summary) in ranked {
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

fn normalized_tokens(input: &str) -> BTreeSet<String> {
    input
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

fn score_record(
    record: &IndexRecord,
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

fn snippet_for(record: &IndexRecord, question_tokens: &BTreeSet<String>) -> String {
    if let Some(excerpt) = record.excerpt.as_deref() {
        let trimmed = excerpt.trim();
        if !trimmed.is_empty() {
            return stable_excerpt_window(trimmed, question_tokens, 180);
        }
    }
    format!("Matched {} at {}", record.alias, record.path)
}

fn snippet_from_chunk(
    entry: &KnowledgeEntry,
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
    entry: &KnowledgeEntry,
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
        return stable_truncate(text, max_chars);
    }

    let start_b = grapheme_starts[best_start];
    let end_b = if best_end < grapheme_len {
        grapheme_starts[best_end]
    } else {
        text.len()
    };

    let mut out = text[start_b..end_b].to_string();
    if best_start > 0 {
        out = format!("...{}", out);
    }
    if best_end < grapheme_len {
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
