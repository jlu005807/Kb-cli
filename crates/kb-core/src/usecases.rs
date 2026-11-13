//! Usecase 编排层。
//! 负责把领域规则组织成可执行业务流程，并保持输入/输出稳定契约。

use crate::domain::{
    memory_topic_slug, Alias, GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType,
    GraphSnapshot, IndexSnapshot, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SearchHit,
    SearchProjectionSnapshot, SemanticProviderInfo, WikiManifestEntry, WikiManifestSnapshot,
    WikiPageDocument, WikiPageType, WikiViewSnapshot,
};
use crate::ports::{
    AnswerEngine, AnswerMeta, FileContentReader, RetrievedContext, Retriever,
    SemanticExtractRequest, SemanticExtractor, StateSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};

// init 规划：只做“是否允许初始化/覆盖”的业务判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPlan {
    pub project: String,
    pub overwritten: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitPlanError {
    AlreadyInitialized,
}

pub fn plan_init(
    project_name_from_cwd: String,
    provided_name: Option<String>,
    already_initialized: bool,
    force: bool,
) -> Result<InitPlan, InitPlanError> {
    if already_initialized && !force {
        return Err(InitPlanError::AlreadyInitialized);
    }

    Ok(InitPlan {
        project: provided_name.unwrap_or(project_name_from_cwd),
        overwritten: force && already_initialized,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddModePlan {
    Link,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlanInput {
    pub normalized_path: String,
    pub alias: Option<String>,
    pub mode: AddModePlan,
    pub requested_category: Option<String>,
    pub available_categories: Vec<String>,
    pub existing_aliases: Vec<String>,
    pub existing_paths: Vec<String>,
    pub ignored: bool,
    pub source_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlan {
    pub alias: String,
    pub category: String,
    pub mode: AddModePlan,
    pub ignored: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddPlanError {
    SourcePathNotFound {
        path: String,
    },
    InvalidAlias {
        alias: String,
    },
    AliasConflict {
        alias: String,
    },
    PathAlreadyRegistered {
        path: String,
    },
    CategoryNotFound {
        category: String,
        available: Vec<String>,
    },
}

// add 规划：聚合输入校验（路径、别名、分类、冲突）并给出标准化执行参数。
pub fn plan_add(input: AddPlanInput) -> Result<AddPlan, AddPlanError> {
    if !input.source_exists {
        return Err(AddPlanError::SourcePathNotFound {
            path: input.normalized_path,
        });
    }

    let alias = input.alias.unwrap_or_else(|| input.normalized_path.clone());

    if input.ignored {
        return Ok(AddPlan {
            alias,
            category: input
                .requested_category
                .or_else(|| input.available_categories.last().cloned())
                .unwrap_or_else(|| "raw".to_string()),
            mode: input.mode,
            ignored: true,
            path: input.normalized_path,
        });
    }

    if Alias::parse(&alias).is_err() {
        return Err(AddPlanError::InvalidAlias { alias });
    }

    if input.existing_aliases.iter().any(|x| x == &alias) {
        return Err(AddPlanError::AliasConflict { alias });
    }

    if input
        .existing_paths
        .iter()
        .any(|x| x == &input.normalized_path)
    {
        return Err(AddPlanError::PathAlreadyRegistered {
            path: input.normalized_path,
        });
    }

    let mut categories = input.available_categories;
    if categories.is_empty() {
        categories.push("raw".to_string());
    }

    let category = match input.requested_category {
        Some(requested) => {
            if !categories.iter().any(|x| x == &requested) {
                return Err(AddPlanError::CategoryNotFound {
                    category: requested,
                    available: categories,
                });
            }
            requested
        }
        None => categories
            .last()
            .cloned()
            .unwrap_or_else(|| "raw".to_string()),
    };

    Ok(AddPlan {
        alias,
        category,
        mode: input.mode,
        ignored: false,
        path: input.normalized_path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovePlanError {
    InvalidAlias { alias: String },
    AliasConflict { alias: String },
    NotFoundAlias { alias: String },
}

// mv 只改别名，不改 path/category/perms。
pub fn apply_mv(
    state: &StateSnapshot,
    alias: &str,
    new_alias: &str,
) -> Result<StateSnapshot, MovePlanError> {
    if Alias::parse(new_alias).is_err() {
        return Err(MovePlanError::InvalidAlias {
            alias: new_alias.to_string(),
        });
    }
    if state.refs.iter().any(|r| r.alias == new_alias) {
        return Err(MovePlanError::AliasConflict {
            alias: new_alias.to_string(),
        });
    }

    let mut next = state.clone();
    let Some(item) = next.refs.iter_mut().find(|r| r.alias == alias) else {
        return Err(MovePlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    item.alias = new_alias.to_string();
    Ok(next)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovePlanError {
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovePlan {
    pub state: StateSnapshot,
    pub removed_alias: String,
    pub removed_path: String,
}

// rm 只移除引用关系，不触碰物理文件。
pub fn apply_rm(state: &StateSnapshot, alias: &str) -> Result<RemovePlan, RemovePlanError> {
    let mut next = state.clone();
    let Some(idx) = next.refs.iter().position(|r| r.alias == alias) else {
        return Err(RemovePlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    let removed = next.refs.remove(idx);
    Ok(RemovePlan {
        state: next,
        removed_alias: removed.alias,
        removed_path: removed.path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPlanError {
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPlan {
    pub state: Option<StateSnapshot>,
    pub alias: String,
    pub path: String,
    pub old_path: Option<String>,
}

// path 支持“查询”和“更新”两种模式：new_path=None 为查询。
pub fn apply_path(
    state: &StateSnapshot,
    alias: &str,
    new_path: Option<String>,
) -> Result<PathPlan, PathPlanError> {
    let Some(current) = state.refs.iter().find(|r| r.alias == alias) else {
        return Err(PathPlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };

    if let Some(new_path_value) = new_path {
        let mut next = state.clone();
        let item = next
            .refs
            .iter_mut()
            .find(|r| r.alias == alias)
            .expect("alias existence already checked");
        let old_path = item.path.clone();
        item.path = new_path_value.clone();
        Ok(PathPlan {
            state: Some(next),
            alias: alias.to_string(),
            path: new_path_value,
            old_path: Some(old_path),
        })
    } else {
        Ok(PathPlan {
            state: None,
            alias: alias.to_string(),
            path: current.path.clone(),
            old_path: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChmodPlanError {
    InvalidPermission { provided: String },
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChmodPlan {
    pub state: StateSnapshot,
    pub alias: String,
    pub permissions: String,
}

// chmod 在 core 层统一校验权限串合法性，CLI 仅做错误映射。
pub fn apply_chmod(
    state: &StateSnapshot,
    alias: &str,
    perms: &str,
) -> Result<ChmodPlan, ChmodPlanError> {
    if crate::domain::Permission::parse(perms).is_err() {
        return Err(ChmodPlanError::InvalidPermission {
            provided: perms.to_string(),
        });
    }

    let mut next = state.clone();
    let Some(item) = next.refs.iter_mut().find(|r| r.alias == alias) else {
        return Err(ChmodPlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    item.perms = perms.to_string();
    Ok(ChmodPlan {
        state: next,
        alias: alias.to_string(),
        permissions: perms.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryFilterError {
    CategoryNotFound { category: String },
}

pub fn validate_category_filter(
    requested: Option<&str>,
    categories: &[String],
) -> Result<(), CategoryFilterError> {
    if let Some(cat) = requested {
        if !categories.iter().any(|x| x == cat) {
            return Err(CategoryFilterError::CategoryNotFound {
                category: cat.to_string(),
            });
        }
    }
    Ok(())
}

// top 参数归一化：统一由 usecase 层决定默认值策略。
pub fn normalize_top(top: Option<u32>, default: u32) -> usize {
    top.unwrap_or(default) as usize
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub alias: String,
    pub path: String,
    pub perms: String,
    pub category: String,
    pub size: Option<u64>,
    pub mtime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPlan {
    pub items: Vec<ListRow>,
    pub categories: Option<Vec<String>>,
}

// list 输出规划：tree 模式附带分类目录，普通模式仅返回 items。
pub fn plan_list_output(items: Vec<ListRow>, tree: bool, categories: Vec<String>) -> ListPlan {
    ListPlan {
        items,
        categories: if tree { Some(categories) } else { None },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskPlanError {
    NoRelevantContext,
}

// ask 前置判定：先判断是否存在可检索上下文，再确定 top 限额。
pub fn plan_ask(has_context: bool, top: Option<u32>) -> Result<usize, AskPlanError> {
    if !has_context {
        return Err(AskPlanError::NoRelevantContext);
    }
    Ok(top.unwrap_or(1).max(1) as usize)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskUsecaseOutput {
    pub meta: AnswerMeta,
    pub retrieved_context: Vec<RetrievedContext>,
}

// ask 主流程编排：retrieve -> meta，任何一步失败都转成 NoRelevantContext。
pub fn execute_ask<R: Retriever + ?Sized, A: AnswerEngine + ?Sized>(
    retriever: &R,
    answer_engine: &A,
    snapshot: &IndexSnapshot,
    question: &str,
    top: Option<u32>,
) -> Result<AskUsecaseOutput, AskPlanError> {
    let has_context = snapshot.records.iter().any(|record| record.present);
    let limit = plan_ask(has_context, top)?;
    let retrieved_context = retriever
        .retrieve(question, limit, snapshot)
        .map_err(|_| AskPlanError::NoRelevantContext)?;
    if retrieved_context.is_empty() {
        return Err(AskPlanError::NoRelevantContext);
    }
    let meta = answer_engine
        .meta(question, &retrieved_context)
        .map_err(|_| AskPlanError::NoRelevantContext)?;
    Ok(AskUsecaseOutput {
        meta,
        retrieved_context,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileL2Issue {
    pub record_id: String,
    pub path: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompileL2Outcome {
    pub snapshot: KnowledgeSnapshot,
    pub processed: usize,
    pub reused: usize,
    pub missing: usize,
    pub failed: usize,
    pub issues: Vec<CompileL2Issue>,
}

pub fn compile_l2<R: FileContentReader + ?Sized, E: SemanticExtractor + ?Sized>(
    baseline: Option<&KnowledgeSnapshot>,
    index_snapshot: &IndexSnapshot,
    reader: &R,
    extractor: &E,
    generated_at: &str,
) -> CompileL2Outcome {
    let mut baseline_map: BTreeMap<&str, &KnowledgeEntry> = BTreeMap::new();
    if let Some(baseline_snapshot) = baseline {
        for entry in &baseline_snapshot.entries {
            baseline_map.insert(entry.record_id.as_str(), entry);
        }
    }

    // 第一阶段：消化"无需调用 extractor"的记录（missing / 命中 baseline /
    // 源文件读取失败），并为剩余需要抽取的记录预先 stage 占位条目 + 待发送的
    // request。这样第二阶段就可以把所有 request 一次性丢给 extractor.batch
    // —— LLM 适配器会在 tokio runtime 上并发地跑这一批。
    enum SlotInit {
        Final(KnowledgeEntry),
        Pending {
            record_id: String,
            path: String,
            input_signature: String,
            text: String,
        },
    }

    let mut slots: Vec<SlotInit> = Vec::with_capacity(index_snapshot.records.len());
    let mut issues = Vec::new();
    let mut processed = 0usize;
    let mut reused = 0usize;
    let mut missing = 0usize;

    for record in &index_snapshot.records {
        let record_id = record.alias.clone();
        let path = record.path.clone();
        let input_signature = format!("{}|{}", record.path, record.signature);

        if !record.present {
            missing += 1;
            issues.push(CompileL2Issue {
                record_id: record_id.clone(),
                path: path.clone(),
                code: "W-L2-SOURCE-MISSING".to_string(),
                message: "Index record marked as not present".to_string(),
                retryable: true,
            });
            slots.push(SlotInit::Final(KnowledgeEntry::from_file(
                record_id,
                path,
                input_signature,
                KnowledgeStatus::Missing,
                String::new(),
                vec![],
                vec![],
                vec![],
                SemanticProviderInfo {
                    kind: "system".to_string(),
                    name: "missing-source".to_string(),
                },
            )));
            continue;
        }

        if let Some(previous) = baseline_map.get(record.alias.as_str()) {
            if previous.path == record.path
                && previous.input_signature == input_signature
                && previous.status == KnowledgeStatus::Active
            {
                reused += 1;
                slots.push(SlotInit::Final((**previous).clone()));
                continue;
            }
        }

        processed += 1;
        let text = match reader.read_text(&path) {
            Ok(text) => text,
            Err(err) => {
                missing += 1;
                issues.push(CompileL2Issue {
                    record_id: record_id.clone(),
                    path: path.clone(),
                    code: "W-L2-SOURCE-MISSING".to_string(),
                    message: err.to_string(),
                    retryable: true,
                });
                slots.push(SlotInit::Final(KnowledgeEntry::from_file(
                    record_id,
                    path,
                    input_signature,
                    KnowledgeStatus::Missing,
                    String::new(),
                    vec![],
                    vec![],
                    vec![],
                    SemanticProviderInfo {
                        kind: "system".to_string(),
                        name: "missing-source".to_string(),
                    },
                )));
                continue;
            }
        };

        slots.push(SlotInit::Pending {
            record_id,
            path,
            input_signature,
            text,
        });
    }

    // 第二阶段：收集所有 pending request，批量交给 extractor。LlmSemanticExtractor
    // 覆写了 extract_batch，会在共享 tokio runtime 上用 semaphore 限制并发地跑；
    // 默认实现退化成串行 `extract`，对 rule-based extractor 无副作用。
    let pending_requests: Vec<SemanticExtractRequest> = slots
        .iter()
        .filter_map(|slot| match slot {
            SlotInit::Pending {
                record_id,
                path,
                input_signature,
                text,
            } => Some(SemanticExtractRequest {
                record_id: record_id.clone(),
                path: path.clone(),
                text: text.clone(),
                input_signature: input_signature.clone(),
            }),
            SlotInit::Final(_) => None,
        })
        .collect();
    let mut batch_results = extractor.extract_batch(&pending_requests).into_iter();

    let mut failed = 0usize;
    let mut entries = Vec::with_capacity(slots.len());
    for slot in slots {
        match slot {
            SlotInit::Final(entry) => entries.push(entry),
            SlotInit::Pending {
                record_id,
                path,
                input_signature,
                text,
            } => {
                let result = batch_results
                    .next()
                    .expect("extract_batch returned fewer results than requests");
                match result {
                    Ok(extracted) => {
                        entries.push(KnowledgeEntry::from_file(
                            record_id,
                            path,
                            input_signature,
                            KnowledgeStatus::Active,
                            extracted.summary,
                            extracted.tags,
                            extracted.key_concepts,
                            extracted.chunks,
                            extracted.provider,
                        ));
                    }
                    Err(err) => {
                        failed += 1;
                        issues.push(CompileL2Issue {
                            record_id: record_id.clone(),
                            path: path.clone(),
                            code: "W-L2-EXTRACT-FAILED".to_string(),
                            message: err.to_string(),
                            retryable: true,
                        });
                        entries.push(KnowledgeEntry::from_file(
                            record_id,
                            path,
                            input_signature,
                            KnowledgeStatus::Failed,
                            first_non_empty_line(&text).unwrap_or_default(),
                            vec![],
                            vec![],
                            vec![],
                            SemanticProviderInfo {
                                kind: "system".to_string(),
                                name: "extract-failed".to_string(),
                            },
                        ));
                    }
                }
            }
        }
    }
    debug_assert!(
        batch_results.next().is_none(),
        "extract_batch returned more results than requests"
    );

    let snapshot = KnowledgeSnapshot {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        base_index_signature: build_index_signature(index_snapshot),
        entries,
    };

    CompileL2Outcome {
        snapshot,
        processed,
        reused,
        missing,
        failed,
        issues,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompileL3Outcome {
    pub snapshot: SearchProjectionSnapshot,
    pub processed: usize,
    pub reused: usize,
}

pub fn compile_l3(
    baseline: Option<&SearchProjectionSnapshot>,
    knowledge_snapshot: &KnowledgeSnapshot,
    generated_at: &str,
) -> CompileL3Outcome {
    let mut baseline_map: BTreeMap<&str, &SearchHit> = BTreeMap::new();
    if let Some(baseline_snapshot) = baseline {
        for hit in &baseline_snapshot.hits {
            baseline_map.insert(hit.key.as_str(), hit);
        }
    }

    let mut hits = Vec::new();
    let mut processed = 0usize;
    let mut reused = 0usize;

    for entry in &knowledge_snapshot.entries {
        if entry.status != KnowledgeStatus::Active {
            continue;
        }
        for chunk in &entry.chunks {
            let key = format!(
                "{}|{}|{}",
                entry.record_id, chunk.chunk_id, chunk.content_signature
            );
            if let Some(previous) = baseline_map.get(key.as_str()) {
                reused += 1;
                hits.push((**previous).clone());
                continue;
            }

            processed += 1;
            let matched_terms = derive_matched_terms(entry, chunk);
            hits.push(SearchHit {
                key,
                record_id: entry.record_id.clone(),
                chunk_id: chunk.chunk_id.clone(),
                content_signature: chunk.content_signature.clone(),
                score: deterministic_search_score(&matched_terms),
                matched_terms,
            });
        }
    }
    hits.sort_by(|a, b| a.key.cmp(&b.key));

    let snapshot = SearchProjectionSnapshot {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        base_knowledge_signature: build_knowledge_signature(knowledge_snapshot),
        hits,
    };

    CompileL3Outcome {
        snapshot,
        processed,
        reused,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileL4Issue {
    pub edge_id: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileL4Outcome {
    pub snapshot: GraphSnapshot,
    pub processed: usize,
    pub reused: usize,
    pub invalid_edges: usize,
    pub issues: Vec<CompileL4Issue>,
}

pub fn compile_l4(
    baseline: Option<&GraphSnapshot>,
    knowledge_snapshot: &KnowledgeSnapshot,
    generated_at: &str,
) -> CompileL4Outcome {
    let mut baseline_map: BTreeMap<&str, &GraphEdge> = BTreeMap::new();
    if let Some(baseline_snapshot) = baseline {
        for edge in &baseline_snapshot.edges {
            baseline_map.insert(edge.edge_id.as_str(), edge);
        }
    }

    let mut nodes_map: BTreeMap<String, GraphNode> = BTreeMap::new();
    let mut candidates = Vec::new();
    let mut file_profiles = Vec::new();
    let mut issues = Vec::new();
    let mut invalid_edges = 0usize;

    for entry in &knowledge_snapshot.entries {
        if entry.status != KnowledgeStatus::Active {
            continue;
        }

        let file_node_id = format!("file:{}", entry.record_id);
        nodes_map
            .entry(file_node_id.clone())
            .or_insert_with(|| GraphNode {
                node_id: file_node_id.clone(),
                node_type: GraphNodeType::File,
                label: entry.record_id.clone(),
            });
        let default_chunk_id = entry.chunks.first().map(|chunk| chunk.chunk_id.clone());

        let tags = normalize_terms(&entry.tags);
        for tag in &tags {
            let tag_node_id = format!("tag:{}", tag);
            nodes_map
                .entry(tag_node_id.clone())
                .or_insert_with(|| GraphNode {
                    node_id: tag_node_id.clone(),
                    node_type: GraphNodeType::Tag,
                    label: tag.clone(),
                });
            candidates.push(EdgeCandidate {
                edge_id: format!("file_has_tag|{}|{}", file_node_id, tag_node_id),
                edge_type: GraphEdgeType::FileHasTag,
                from_node_id: file_node_id.clone(),
                to_node_id: tag_node_id,
                evidence: vec![GraphEvidence {
                    record_id: entry.record_id.clone(),
                    chunk_id: default_chunk_id.clone(),
                    reason: "tag derived from semantic extraction".to_string(),
                }],
            });
        }

        let concepts = normalize_terms(&entry.key_concepts);
        for concept in &concepts {
            let concept_node_id = format!("concept:{}", concept);
            nodes_map
                .entry(concept_node_id.clone())
                .or_insert_with(|| GraphNode {
                    node_id: concept_node_id.clone(),
                    node_type: GraphNodeType::Concept,
                    label: concept.clone(),
                });
            candidates.push(EdgeCandidate {
                edge_id: format!("file_mentions_concept|{}|{}", file_node_id, concept_node_id),
                edge_type: GraphEdgeType::FileMentionsConcept,
                from_node_id: file_node_id.clone(),
                to_node_id: concept_node_id,
                evidence: vec![GraphEvidence {
                    record_id: entry.record_id.clone(),
                    chunk_id: default_chunk_id.clone(),
                    reason: "concept extracted from key_concepts".to_string(),
                }],
            });
        }

        for i in 0..concepts.len() {
            for j in (i + 1)..concepts.len() {
                let from = format!("concept:{}", concepts[i]);
                let to = format!("concept:{}", concepts[j]);
                candidates.push(EdgeCandidate {
                    edge_id: format!("concept_related_to_concept|{}|{}", from, to),
                    edge_type: GraphEdgeType::ConceptRelatedToConcept,
                    from_node_id: from,
                    to_node_id: to,
                    evidence: vec![GraphEvidence {
                        record_id: entry.record_id.clone(),
                        chunk_id: default_chunk_id.clone(),
                        reason: "concepts co-occur in same file".to_string(),
                    }],
                });
            }
        }

        file_profiles.push(FileProfile {
            record_id: entry.record_id.clone(),
            file_node_id,
            tags: tags.into_iter().collect(),
            concepts: concepts.into_iter().collect(),
            chunk_id: default_chunk_id,
        });
    }

    for i in 0..file_profiles.len() {
        for j in (i + 1)..file_profiles.len() {
            let left = &file_profiles[i];
            let right = &file_profiles[j];

            let mut shared = left
                .tags
                .intersection(&right.tags)
                .cloned()
                .collect::<Vec<_>>();
            shared.extend(
                left.concepts
                    .intersection(&right.concepts)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            shared.sort();
            shared.dedup();
            if shared.is_empty() {
                continue;
            }

            let joined = shared
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            candidates.push(EdgeCandidate {
                edge_id: format!(
                    "file_related_to_file|{}|{}",
                    left.file_node_id, right.file_node_id
                ),
                edge_type: GraphEdgeType::FileRelatedToFile,
                from_node_id: left.file_node_id.clone(),
                to_node_id: right.file_node_id.clone(),
                evidence: vec![
                    GraphEvidence {
                        record_id: left.record_id.clone(),
                        chunk_id: left.chunk_id.clone(),
                        reason: format!(
                            "shares semantic terms with {}: {}",
                            right.record_id, joined
                        ),
                    },
                    GraphEvidence {
                        record_id: right.record_id.clone(),
                        chunk_id: right.chunk_id.clone(),
                        reason: format!(
                            "shares semantic terms with {}: {}",
                            left.record_id, joined
                        ),
                    },
                ],
            });
        }
    }

    let mut edges = Vec::new();
    let mut processed = 0usize;
    let mut reused = 0usize;
    let mut merged: BTreeMap<String, EdgeCandidate> = BTreeMap::new();
    for candidate in candidates {
        if let Some(existing) = merged.get_mut(&candidate.edge_id) {
            for evidence in candidate.evidence {
                if !existing.evidence.contains(&evidence) {
                    existing.evidence.push(evidence);
                }
            }
            continue;
        }
        merged.insert(candidate.edge_id.clone(), candidate);
    }

    for (_, mut candidate) in merged {
        candidate.evidence.sort_by(|a, b| {
            a.record_id
                .cmp(&b.record_id)
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
                .then_with(|| a.reason.cmp(&b.reason))
        });
        if candidate.evidence.is_empty() {
            invalid_edges += 1;
            issues.push(CompileL4Issue {
                edge_id: candidate.edge_id,
                code: "W-L4-MISSING-EVIDENCE".to_string(),
                message: "graph edge has no evidence".to_string(),
                retryable: false,
            });
            continue;
        }

        if let Some(previous) = baseline_map.get(candidate.edge_id.as_str()) {
            if previous.edge_type == candidate.edge_type
                && previous.from_node_id == candidate.from_node_id
                && previous.to_node_id == candidate.to_node_id
                && previous.evidence == candidate.evidence
            {
                reused += 1;
                edges.push((**previous).clone());
                continue;
            }
        }

        processed += 1;
        edges.push(GraphEdge {
            edge_id: candidate.edge_id,
            edge_type: candidate.edge_type,
            from_node_id: candidate.from_node_id,
            to_node_id: candidate.to_node_id,
            evidence: candidate.evidence,
        });
    }

    let mut nodes = nodes_map.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

    let snapshot = GraphSnapshot {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        base_knowledge_signature: build_knowledge_signature(knowledge_snapshot),
        nodes,
        edges,
    };

    CompileL4Outcome {
        snapshot,
        processed,
        reused,
        invalid_edges,
        issues,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileL5Issue {
    pub page_id: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileL5Outcome {
    pub snapshot: WikiViewSnapshot,
    pub processed: usize,
    pub reused: usize,
    pub removed_paths: Vec<String>,
    pub issues: Vec<CompileL5Issue>,
}

pub fn compile_l5(
    baseline: Option<&WikiManifestSnapshot>,
    knowledge_snapshot: &KnowledgeSnapshot,
    graph_snapshot: &GraphSnapshot,
    generated_at: &str,
) -> CompileL5Outcome {
    let mut baseline_map: BTreeMap<&str, &WikiManifestEntry> = BTreeMap::new();
    if let Some(baseline_manifest) = baseline {
        for page in &baseline_manifest.pages {
            baseline_map.insert(page.page_id.as_str(), page);
        }
    }

    let mut pages = Vec::new();
    let mut issues = Vec::new();
    let mut active_entries = knowledge_snapshot
        .entries
        .iter()
        .filter(|entry| entry.status == KnowledgeStatus::Active)
        .collect::<Vec<_>>();
    active_entries.sort_by(|a, b| a.record_id.cmp(&b.record_id));

    for entry in &active_entries {
        let page_id = format!("source:{}", entry.record_id);
        let title = entry.record_id.clone();
        let relative_path = format!("pages/source/{}.md", slugify_for_path(&entry.record_id));
        let source_chunks = entry
            .chunks
            .iter()
            .map(|chunk| chunk.chunk_id.clone())
            .collect::<Vec<_>>();
        let see_also = collect_related_sources(entry.record_id.as_str(), graph_snapshot);
        let related_edge_ids = collect_related_edge_ids(entry.record_id.as_str(), graph_snapshot);
        let edge_sig = format!(
            "fnv1a64:{:016x}",
            fnv1a64(related_edge_ids.join("|").as_bytes())
        );
        let input_signature = format!("{}|{}", entry.input_signature, edge_sig);
        let markdown = render_source_wiki_page(
            generated_at,
            &page_id,
            &title,
            &input_signature,
            entry,
            &source_chunks,
            &see_also,
        );

        if source_chunks.is_empty() {
            issues.push(CompileL5Issue {
                page_id: page_id.clone(),
                code: "W-L5-NO-CHUNK-EVIDENCE".to_string(),
                message: "source page has no chunk evidence".to_string(),
                retryable: false,
            });
        }

        pages.push(WikiPageDocument {
            page_id,
            page_type: WikiPageType::Source,
            title,
            relative_path,
            input_signature,
            source_records: vec![entry.record_id.clone()],
            source_chunks,
            markdown,
        });
    }

    let index_markdown = render_index_wiki_page(generated_at, &pages);
    let overview_markdown = render_overview_wiki_page(generated_at, &pages, graph_snapshot);
    let log_markdown = render_log_wiki_page(generated_at, &pages);

    pages.push(WikiPageDocument {
        page_id: "index".to_string(),
        page_type: WikiPageType::Index,
        title: "Knowledge Wiki".to_string(),
        relative_path: "index.md".to_string(),
        input_signature: format!("fnv1a64:{:016x}", fnv1a64(index_markdown.as_bytes())),
        source_records: pages
            .iter()
            .filter(|page| page.page_type == WikiPageType::Source)
            .flat_map(|page| page.source_records.clone())
            .collect(),
        source_chunks: vec![],
        markdown: index_markdown,
    });
    pages.push(WikiPageDocument {
        page_id: "overview".to_string(),
        page_type: WikiPageType::Overview,
        title: "Overview".to_string(),
        relative_path: "overview.md".to_string(),
        input_signature: format!("fnv1a64:{:016x}", fnv1a64(overview_markdown.as_bytes())),
        source_records: vec![],
        source_chunks: vec![],
        markdown: overview_markdown,
    });
    pages.push(WikiPageDocument {
        page_id: "log".to_string(),
        page_type: WikiPageType::Log,
        title: "Update Log".to_string(),
        relative_path: "log.md".to_string(),
        input_signature: format!("fnv1a64:{:016x}", fnv1a64(log_markdown.as_bytes())),
        source_records: vec![],
        source_chunks: vec![],
        markdown: log_markdown,
    });

    pages.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let manifest_pages = pages
        .iter()
        .map(|page| WikiManifestEntry {
            page_id: page.page_id.clone(),
            page_type: page.page_type.clone(),
            title: page.title.clone(),
            relative_path: page.relative_path.clone(),
            input_signature: page.input_signature.clone(),
            source_records: page.source_records.clone(),
            source_chunks: page.source_chunks.clone(),
        })
        .collect::<Vec<_>>();

    let manifest = WikiManifestSnapshot {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        base_graph_signature: build_graph_signature(graph_snapshot),
        pages: manifest_pages,
    };

    let mut processed = 0usize;
    let mut reused = 0usize;
    for page in &manifest.pages {
        if let Some(previous) = baseline_map.get(page.page_id.as_str()) {
            if previous.input_signature == page.input_signature
                && previous.relative_path == page.relative_path
                && previous.title == page.title
            {
                reused += 1;
                continue;
            }
        }
        processed += 1;
    }

    let current_page_ids = manifest
        .pages
        .iter()
        .map(|page| page.page_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut removed_paths = baseline
        .map(|snapshot| {
            snapshot
                .pages
                .iter()
                .filter(|page| !current_page_ids.contains(page.page_id.as_str()))
                .map(|page| page.relative_path.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    removed_paths.sort();
    removed_paths.dedup();

    CompileL5Outcome {
        snapshot: WikiViewSnapshot { manifest, pages },
        processed,
        reused,
        removed_paths,
        issues,
    }
}

fn build_index_signature(index_snapshot: &IndexSnapshot) -> String {
    let mut parts = index_snapshot
        .records
        .iter()
        .map(|record| {
            format!(
                "{}|{}|{}|{}",
                record.alias, record.path, record.signature, record.present
            )
        })
        .collect::<Vec<_>>();
    parts.sort();
    let payload = parts.join("\n");
    format!("fnv1a64:{:016x}", fnv1a64(payload.as_bytes()))
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Select a stable memory filename, reusing matching metadata before creating a hashed name.
/// `existing` maps each occupied filename to its parsed topic; an empty topic marks unknown metadata.
pub fn memory_file_name(
    topic: &str,
    existing: &BTreeMap<String, String>,
) -> Result<String, String> {
    if let Some((file_name, _)) = existing.iter().find(|(_, existing_topic)| *existing_topic == topic)
    {
        return Ok(file_name.clone());
    }

    let slug = memory_topic_slug(topic);
    let slug = if slug.is_empty() {
        let fallback = topic
            .chars()
            .map(|ch| if ch.is_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if fallback.is_empty() {
            "topic".to_string()
        } else {
            fallback
        }
    } else {
        slug
    };
    let hash = format!("{:016x}", fnv1a64(topic.as_bytes()));
    let short_name = format!("{}-{}.md", slug, &hash[..8]);
    if !existing.contains_key(&short_name) {
        return Ok(short_name);
    }

    let full_name = format!("{}-{}.md", slug, hash);
    if !existing.contains_key(&full_name) {
        return Ok(full_name);
    }

    Err(format!("memory filename hash collision for topic '{topic}'"))
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn build_knowledge_signature(snapshot: &KnowledgeSnapshot) -> String {
    let mut parts = snapshot
        .entries
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}",
                entry.record_id,
                entry.path,
                entry.input_signature,
                match entry.status {
                    KnowledgeStatus::Active => "active",
                    KnowledgeStatus::Missing => "missing",
                    KnowledgeStatus::Failed => "failed",
                    KnowledgeStatus::Stale => "stale",
                }
            )
        })
        .collect::<Vec<_>>();
    parts.sort();
    let payload = parts.join("\n");
    format!("fnv1a64:{:016x}", fnv1a64(payload.as_bytes()))
}

fn derive_matched_terms(
    entry: &KnowledgeEntry,
    chunk: &crate::domain::KnowledgeChunk,
) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for tag in &entry.tags {
        if !tag.trim().is_empty() {
            terms.insert(tag.trim().to_ascii_lowercase());
        }
    }
    for concept in &entry.key_concepts {
        for token in concept
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            terms.insert(token.to_string());
        }
    }
    for token in chunk
        .summary
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .take(6)
    {
        terms.insert(token.to_string());
    }

    terms.into_iter().take(16).collect()
}

fn deterministic_search_score(matched_terms: &[String]) -> f64 {
    let capped = matched_terms.len().min(10) as f64;
    (0.5 + capped / 20.0).min(1.0)
}

fn build_graph_signature(snapshot: &GraphSnapshot) -> String {
    let mut node_parts = snapshot
        .nodes
        .iter()
        .map(|node| format!("{}|{:?}|{}", node.node_id, node.node_type, node.label))
        .collect::<Vec<_>>();
    node_parts.sort();

    let mut edge_parts = snapshot
        .edges
        .iter()
        .map(|edge| {
            let mut evidence = edge
                .evidence
                .iter()
                .map(|e| {
                    format!(
                        "{}|{}|{}",
                        e.record_id,
                        e.chunk_id.as_deref().unwrap_or("-"),
                        e.reason
                    )
                })
                .collect::<Vec<_>>();
            evidence.sort();
            format!(
                "{}|{:?}|{}|{}|{}",
                edge.edge_id,
                edge.edge_type,
                edge.from_node_id,
                edge.to_node_id,
                evidence.join("||")
            )
        })
        .collect::<Vec<_>>();
    edge_parts.sort();

    let payload = format!("{}\n{}", node_parts.join("\n"), edge_parts.join("\n"));
    format!("fnv1a64:{:016x}", fnv1a64(payload.as_bytes()))
}

fn collect_related_sources(record_id: &str, graph_snapshot: &GraphSnapshot) -> Vec<String> {
    let file_node_id = format!("file:{}", record_id);
    let mut sources = BTreeSet::new();
    for edge in &graph_snapshot.edges {
        if edge.edge_type != GraphEdgeType::FileRelatedToFile {
            continue;
        }
        if edge.from_node_id == file_node_id && edge.to_node_id.starts_with("file:") {
            sources.insert(edge.to_node_id.trim_start_matches("file:").to_string());
        } else if edge.to_node_id == file_node_id && edge.from_node_id.starts_with("file:") {
            sources.insert(edge.from_node_id.trim_start_matches("file:").to_string());
        }
    }
    sources.into_iter().collect()
}

fn collect_related_edge_ids(record_id: &str, graph_snapshot: &GraphSnapshot) -> Vec<String> {
    let file_node_id = format!("file:{}", record_id);
    let mut edges = BTreeSet::new();
    for edge in &graph_snapshot.edges {
        let evidence_match = edge
            .evidence
            .iter()
            .any(|evidence| evidence.record_id == record_id);
        if evidence_match || edge.from_node_id == file_node_id || edge.to_node_id == file_node_id {
            edges.insert(edge.edge_id.clone());
        }
    }
    edges.into_iter().collect()
}

fn render_source_wiki_page(
    generated_at: &str,
    page_id: &str,
    title: &str,
    input_signature: &str,
    entry: &KnowledgeEntry,
    source_chunks: &[String],
    see_also: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", page_id));
    out.push_str("type: source\n");
    out.push_str(&format!("title: {}\n", title));
    out.push_str(&format!("updated_at: {}\n", generated_at));
    out.push_str("source_records:\n");
    out.push_str(&format!("- {}\n", entry.record_id));
    out.push_str("source_chunks:\n");
    if source_chunks.is_empty() {
        out.push_str("- -\n");
    } else {
        for chunk_id in source_chunks {
            out.push_str(&format!("- {}\n", chunk_id));
        }
    }
    out.push_str("generator: rule/wiki-view-v1\n");
    out.push_str(&format!("input_signature: {}\n", input_signature));
    out.push_str("---\n\n");

    out.push_str(&format!("# {}\n\n", title));
    out.push_str("## Summary\n");
    if entry.summary.trim().is_empty() {
        out.push_str("- (empty)\n\n");
    } else {
        out.push_str(&format!("{}\n\n", entry.summary.trim()));
    }

    out.push_str("## Key Points\n");
    let tags_text = if entry.tags.is_empty() {
        "(none)".to_string()
    } else {
        entry.tags.join(", ")
    };
    let concepts_text = if entry.key_concepts.is_empty() {
        "(none)".to_string()
    } else {
        entry.key_concepts.join(", ")
    };
    out.push_str(&format!("- Tags: {}\n", tags_text));
    out.push_str(&format!("- Concepts: {}\n", concepts_text));
    out.push_str(&format!("- Chunks: {}\n\n", entry.chunks.len()));

    out.push_str("## Evidence\n");
    if entry.chunks.is_empty() {
        out.push_str(&format!(
            "- record `{}` (no chunk evidence)\n\n",
            entry.record_id
        ));
    } else {
        for chunk in entry.chunks.iter().take(8) {
            out.push_str(&format!(
                "- `{}` / `{}`: {}\n",
                entry.record_id, chunk.chunk_id, chunk.summary
            ));
        }
        out.push('\n');
    }

    out.push_str("## See Also\n");
    if see_also.is_empty() {
        out.push_str("- (none)\n");
    } else {
        for source in see_also {
            out.push_str(&format!("- [[source:{}]]\n", source));
        }
    }
    out
}

fn render_index_wiki_page(generated_at: &str, pages: &[WikiPageDocument]) -> String {
    let mut out = String::new();
    out.push_str("# Knowledge Wiki\n\n");
    out.push_str("## Summary\n");
    out.push_str(&format!("- Generated at: {}\n", generated_at));
    out.push_str(&format!("- Source pages: {}\n\n", pages.len()));

    out.push_str("## Key Points\n");
    for page in pages {
        let summary_line = page
            .markdown
            .lines()
            .find(|line| {
                !line.trim().is_empty() && !line.starts_with('#') && !line.starts_with('-')
            })
            .unwrap_or("-")
            .trim();
        out.push_str(&format!(
            "- [{}]({}) — {}\n",
            page.title, page.relative_path, summary_line
        ));
    }
    out.push('\n');

    out.push_str("## Evidence\n");
    out.push_str("- Generated from L2 semantic snapshots and L4 graph edges.\n\n");
    out.push_str("## See Also\n");
    out.push_str("- [Overview](overview.md)\n");
    out
}

fn render_overview_wiki_page(
    generated_at: &str,
    pages: &[WikiPageDocument],
    graph_snapshot: &GraphSnapshot,
) -> String {
    let mut out = String::new();
    out.push_str("# Overview\n\n");
    out.push_str("## Summary\n");
    out.push_str(&format!("- Generated at: {}\n", generated_at));
    out.push_str(&format!("- Source pages: {}\n", pages.len()));
    out.push_str(&format!("- Graph nodes: {}\n", graph_snapshot.nodes.len()));
    out.push_str(&format!(
        "- Graph edges: {}\n\n",
        graph_snapshot.edges.len()
    ));

    let mut tags = BTreeSet::new();
    let mut concepts = BTreeSet::new();
    for node in &graph_snapshot.nodes {
        match node.node_type {
            GraphNodeType::Tag => {
                tags.insert(node.label.clone());
            }
            GraphNodeType::Concept => {
                concepts.insert(node.label.clone());
            }
            _ => {}
        }
    }

    out.push_str("## Key Points\n");
    out.push_str(&format!(
        "- Tags: {}\n",
        if tags.is_empty() {
            "(none)".to_string()
        } else {
            tags.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
        }
    ));
    out.push_str(&format!(
        "- Concepts: {}\n\n",
        if concepts.is_empty() {
            "(none)".to_string()
        } else {
            concepts
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));

    out.push_str("## Evidence\n");
    out.push_str(&format!(
        "- Graph signature: {}\n\n",
        build_graph_signature(graph_snapshot)
    ));

    out.push_str("## See Also\n");
    out.push_str("- [Knowledge Wiki](index.md)\n");
    out
}

fn render_log_wiki_page(generated_at: &str, pages: &[WikiPageDocument]) -> String {
    let mut out = String::new();
    out.push_str("# Update Log\n\n");
    out.push_str("## Summary\n");
    out.push_str(&format!("- Run at: {}\n\n", generated_at));

    out.push_str("## Key Points\n");
    out.push_str(&format!(
        "- Pages in current render: {}\n\n",
        pages.len() + 3
    ));

    out.push_str("## Evidence\n");
    out.push_str("- Generated by deterministic wiki-view compiler.\n\n");

    out.push_str("## See Also\n");
    out.push_str("- [Knowledge Wiki](index.md)\n");
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EdgeCandidate {
    edge_id: String,
    edge_type: GraphEdgeType,
    from_node_id: String,
    to_node_id: String,
    evidence: Vec<GraphEvidence>,
}

#[derive(Debug, Clone)]
struct FileProfile {
    record_id: String,
    file_node_id: String,
    tags: BTreeSet<String>,
    concepts: BTreeSet<String>,
    chunk_id: Option<String>,
}

fn normalize_terms(raw: &[String]) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for value in raw {
        let normalized = value
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        if !normalized.is_empty() {
            terms.insert(normalized);
        }
    }
    terms.into_iter().collect()
}

fn slugify_for_path(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in raw.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let slug = out.trim_matches('-');
    if slug.is_empty() {
        "page".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_mv, compile_l2, plan_add, plan_ask, plan_init, AddModePlan, AddPlanError,
        AddPlanInput, AskPlanError, InitPlanError, MovePlanError,
    };
    use crate::domain::{
        IndexMeta, IndexRecord, IndexSnapshot, KnowledgeChunk, SemanticProviderInfo,
    };
    use crate::error::DomainError;
    use crate::ports::{
        AnswerEngine, AnswerMeta, FileContentReader, RetrievedContext, Retriever,
        SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor, StateRef,
        StateSnapshot,
    };
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    struct TestRetriever;

    impl Retriever for TestRetriever {
        fn retrieve(
            &self,
            _question: &str,
            top: usize,
            _snapshot: &IndexSnapshot,
        ) -> Result<Vec<RetrievedContext>, DomainError> {
            Ok(vec![RetrievedContext {
                alias: "docs/config".to_string(),
                snippet: "Use kb config to change settings.".to_string(),
                score: 0.95,
                source_layer: Some("index".to_string()),
                record_id: None,
                chunk_id: None,
            }]
            .into_iter()
            .take(top)
            .collect())
        }
    }

    struct DisabledAnswerEngine;

    impl AnswerEngine for DisabledAnswerEngine {
        fn meta(
            &self,
            _question: &str,
            _contexts: &[RetrievedContext],
        ) -> Result<AnswerMeta, DomainError> {
            Ok(AnswerMeta {
                engine_status: "disabled".to_string(),
                provider: None,
                trace_id: None,
            })
        }
    }

    #[test]
    fn init_plan_blocks_non_force_overwrite() {
        let result = plan_init("demo".to_string(), None, true, false);
        assert_eq!(result, Err(InitPlanError::AlreadyInitialized));
    }

    #[test]
    fn memory_file_name_reuses_existing_topics_and_resolves_hash_collisions() {
        let topic = "docs/a";
        let hash = format!("{:016x}", super::fnv1a64(topic.as_bytes()));
        let mut existing = BTreeMap::new();
        existing.insert("docs-a.md".to_string(), topic.to_string());
        assert_eq!(super::memory_file_name(topic, &existing).unwrap(), "docs-a.md");

        existing.clear();
        let short_name = format!("docs-a-{}.md", &hash[..8]);
        assert_eq!(
            super::memory_file_name(topic, &existing).unwrap(),
            short_name
        );

        existing.insert(short_name.clone(), "other/topic".to_string());
        let full_name = format!("docs-a-{hash}.md");
        assert_eq!(
            super::memory_file_name(topic, &existing).unwrap(),
            full_name
        );

        existing.insert(full_name, "another/topic".to_string());
        assert!(super::memory_file_name(topic, &existing).is_err());
    }

    #[test]
    fn add_plan_rejects_unknown_category() {
        let result = plan_add(AddPlanInput {
            normalized_path: "./docs/a.md".to_string(),
            alias: Some("docs/a".to_string()),
            mode: AddModePlan::Link,
            requested_category: Some("unknown".to_string()),
            available_categories: vec!["raw".to_string()],
            existing_aliases: vec![],
            existing_paths: vec![],
            ignored: false,
            source_exists: true,
        });

        assert!(matches!(result, Err(AddPlanError::CategoryNotFound { .. })));
    }

    #[test]
    fn mv_plan_rejects_conflict() {
        let state = StateSnapshot {
            refs: vec![
                StateRef {
                    alias: "a".to_string(),
                    path: "./a".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                },
                StateRef {
                    alias: "b".to_string(),
                    path: "./b".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                },
            ],
        };

        let result = apply_mv(&state, "a", "b");
        assert!(matches!(result, Err(MovePlanError::AliasConflict { .. })));
    }

    #[test]
    fn ask_plan_requires_context() {
        let result = plan_ask(false, Some(3));
        assert_eq!(result, Err(AskPlanError::NoRelevantContext));
    }

    #[test]
    fn ask_usecase_returns_disabled_meta_and_context() {
        let snapshot = IndexSnapshot {
            records: vec![IndexRecord {
                alias: "docs/config".to_string(),
                path: "./docs/config.md".to_string(),
                category: "raw".to_string(),
                signature: "f:1".to_string(),
                present: true,
                excerpt: Some("Use kb config to change settings.".to_string()),
            }],
            errors: vec![],
            meta: IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 1,
            },
        };

        let out = super::execute_ask(
            &TestRetriever,
            &DisabledAnswerEngine,
            &snapshot,
            "hello",
            Some(1),
        )
        .expect("ask should succeed");

        assert_eq!(out.meta.engine_status, "disabled");
        assert_eq!(out.retrieved_context.len(), 1);
        assert_eq!(out.retrieved_context[0].alias, "docs/config");
    }

    struct MemReader<'a>(&'a [(&'a str, &'a str)]);
    impl<'a> FileContentReader for MemReader<'a> {
        fn read_text(&self, path: &str) -> Result<String, DomainError> {
            for (p, body) in self.0 {
                if *p == path {
                    return Ok((*body).to_string());
                }
            }
            Err(DomainError::io(format!("missing: {path}")))
        }
    }

    // 计数器型 extractor：默认 `extract_batch` 应该串行回退到 `extract`，
    // 且返回值顺序必须严格匹配 requests。
    struct CountingExtractor {
        seen_paths: RefCell<Vec<String>>,
    }
    impl SemanticExtractor for CountingExtractor {
        fn extract(
            &self,
            request: &SemanticExtractRequest,
        ) -> Result<SemanticExtractResponse, DomainError> {
            self.seen_paths.borrow_mut().push(request.path.clone());
            Ok(SemanticExtractResponse {
                summary: format!("sum:{}", request.record_id),
                tags: vec![],
                key_concepts: vec![],
                chunks: vec![KnowledgeChunk {
                    chunk_id: format!("{}:0001", request.record_id),
                    byte_range: [0, request.text.len() as u64],
                    content_signature: "sig".to_string(),
                    summary: request.text.clone(),
                    preview: request.text.clone(),
                }],
                provider: SemanticProviderInfo {
                    kind: "test".to_string(),
                    name: "counting".to_string(),
                },
            })
        }
    }

    #[test]
    fn compile_l2_routes_through_extract_batch_in_record_order() {
        let snapshot = IndexSnapshot {
            meta: IndexMeta {
                full: true,
                baseline_size: 0,
                tracked_size: 3,
            },
            errors: vec![],
            records: vec![
                IndexRecord {
                    alias: "a/one".to_string(),
                    path: "./docs/one.md".to_string(),
                    category: "raw".to_string(),
                    signature: "sig-one".to_string(),
                    present: true,
                    excerpt: None,
                },
                IndexRecord {
                    alias: "b/two".to_string(),
                    path: "./docs/two.md".to_string(),
                    category: "raw".to_string(),
                    signature: "sig-two".to_string(),
                    present: true,
                    excerpt: None,
                },
                IndexRecord {
                    alias: "c/missing".to_string(),
                    path: "./docs/missing.md".to_string(),
                    category: "raw".to_string(),
                    signature: "sig-missing".to_string(),
                    present: false,
                    excerpt: None,
                },
            ],
        };
        let reader = MemReader(&[
            ("./docs/one.md", "first body"),
            ("./docs/two.md", "second body"),
        ]);
        let extractor = CountingExtractor {
            seen_paths: RefCell::new(Vec::new()),
        };

        let outcome = compile_l2(None, &snapshot, &reader, &extractor, "now");

        assert_eq!(outcome.processed, 2, "two present non-baseline records");
        assert_eq!(outcome.missing, 1, "one missing record");
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.snapshot.entries.len(), 3);
        assert_eq!(outcome.snapshot.entries[0].record_id, "a/one");
        assert_eq!(outcome.snapshot.entries[0].summary, "sum:a/one");
        assert_eq!(outcome.snapshot.entries[1].record_id, "b/two");
        assert_eq!(outcome.snapshot.entries[1].summary, "sum:b/two");
        assert_eq!(outcome.snapshot.entries[2].record_id, "c/missing");
        // 默认 extract_batch 串行走 extract，且只对 present 记录调用——missing 跳过。
        let seen = extractor.seen_paths.borrow().clone();
        assert_eq!(seen, vec!["./docs/one.md", "./docs/two.md"]);
    }
}
