use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::agent_tools::SemanticAgentToolbox;
use crate::openai_semantic::LlmSemanticConfig;
use kb_core::domain::{
    GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot,
    KnowledgeSnapshot, KnowledgeStatus, WikiManifestEntry, WikiManifestSnapshot, WikiPageDocument,
    WikiViewSnapshot,
};
use kb_core::usecases::{CompileL4Issue, CompileL4Outcome, CompileL5Issue, CompileL5Outcome};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct LlmLayerCoverage {
    pub covered: usize,
    pub total: usize,
    pub ratio: f64,
    pub target_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct LlmLayerMapReduce {
    pub batch_size: usize,
    pub batches_total: usize,
    pub calls_total: usize,
    pub rounds_used: usize,
    pub rounds_max: usize,
    pub reflection_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct LlmLayerMetrics {
    pub map_reduce: LlmLayerMapReduce,
    pub coverage: LlmLayerCoverage,
}

#[derive(Debug, Clone)]
pub struct LlmGraphEnrichmentResult {
    pub outcome: CompileL4Outcome,
    pub metrics: LlmLayerMetrics,
}

#[derive(Debug, Clone)]
pub struct LlmWikiEnrichmentResult {
    pub outcome: CompileL5Outcome,
    pub metrics: LlmLayerMetrics,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GraphFileSummary {
    record_id: String,
    path: String,
    summary: String,
    tags: Vec<String>,
    key_concepts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WikiPageSummary {
    page_id: String,
    page_type: String,
    title: String,
    source_records: Vec<String>,
    source_chunks: Vec<String>,
}

pub fn enrich_graph_with_llm(
    baseline: Option<&GraphSnapshot>,
    rule_outcome: &CompileL4Outcome,
    knowledge_snapshot: &KnowledgeSnapshot,
    config: &LlmSemanticConfig,
) -> Result<LlmGraphEnrichmentResult, String> {
    let Some(api_key) = config.key.as_deref() else {
        return Err("llm graph enrichment requires KB_LLM_KEY".to_string());
    };

    let target_ratio = config.agent_min_chunk_coverage.clamp(0.0, 1.0);
    let rounds_max = config.agent_loop_max_rounds.clamp(1, 4);
    let reflection_enabled = config.agent_reflection_enabled && rounds_max > 1;
    let toolbox = SemanticAgentToolbox::new(config.workspace_root.clone());
    let graph_context =
        toolbox.build_graph_context(knowledge_snapshot, Some(&rule_outcome.snapshot));
    let file_summaries = collect_graph_file_summaries(knowledge_snapshot);
    let total_records = file_summaries.len();
    let batch_size = normalize_batch_size(config.max_chunks, total_records);
    let batches_total = count_batches(total_records, batch_size);
    let mut calls_total = 0usize;
    let mut rounds_used = 0usize;
    let mut merged_payload = LlmGraphEnrichmentPayload::default();

    if total_records > 0 {
        let all_record_ids = file_summaries
            .iter()
            .map(|item| item.record_id.clone())
            .collect::<BTreeSet<_>>();

        let mut covered_record_ids = BTreeSet::new();
        for round in 1..=rounds_max {
            let candidate = if round == 1 {
                file_summaries.clone()
            } else {
                file_summaries
                    .iter()
                    .filter(|item| !covered_record_ids.contains(&item.record_id))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if candidate.is_empty() {
                break;
            }
            rounds_used = round;
            for batch in candidate.chunks(batch_size) {
                let batch_ids = batch
                    .iter()
                    .map(|item| item.record_id.as_str())
                    .collect::<BTreeSet<_>>();
                let batch_context = filter_graph_context(&graph_context, &batch_ids);
                let prompt = build_graph_enrichment_prompt(
                    batch,
                    &batch_context,
                    round,
                    &batch
                        .iter()
                        .map(|item| item.record_id.clone())
                        .collect::<Vec<_>>(),
                );
                let response = call_openai_chat_with_retry(
                    config,
                    api_key,
                    "You enrich a local knowledge graph using structured hints. Return JSON only.",
                    &prompt,
                )?;
                calls_total += 1;
                let payload = decode_graph_enrichment_payload(&response)?;
                merge_graph_payload(&mut merged_payload, &payload);
            }

            covered_record_ids = collect_graph_covered_record_ids(&merged_payload, &all_record_ids);
            let coverage_ratio = ratio(covered_record_ids.len(), total_records);
            if !reflection_enabled || coverage_ratio >= target_ratio {
                break;
            }
        }
    }

    let mut issues = rule_outcome.issues.clone();
    let mut nodes_map = rule_outcome
        .snapshot
        .nodes
        .iter()
        .map(|node| (node.node_id.clone(), node.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut edges_map = rule_outcome
        .snapshot
        .edges
        .iter()
        .map(|edge| (edge.edge_id.clone(), edge.clone()))
        .collect::<BTreeMap<_, _>>();
    let record_chunk_map = first_chunk_map(knowledge_snapshot);
    let active_records = knowledge_snapshot
        .entries
        .iter()
        .filter(|entry| entry.status == KnowledgeStatus::Active)
        .map(|entry| entry.record_id.as_str())
        .collect::<BTreeSet<_>>();

    apply_file_relations(
        &merged_payload.file_relations,
        &active_records,
        &record_chunk_map,
        &mut nodes_map,
        &mut edges_map,
        &mut issues,
    );
    apply_file_concepts(
        &merged_payload.file_concepts,
        &active_records,
        &record_chunk_map,
        &mut nodes_map,
        &mut edges_map,
        &mut issues,
    );
    apply_concept_relations(
        &merged_payload.concept_relations,
        &mut nodes_map,
        &mut edges_map,
        &mut issues,
    );
    apply_file_tags(
        &merged_payload.file_tags,
        &active_records,
        &record_chunk_map,
        &mut nodes_map,
        &mut edges_map,
        &mut issues,
    );

    let mut nodes = nodes_map.into_values().collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    let mut edges = edges_map.into_values().collect::<Vec<_>>();
    for edge in &mut edges {
        edge.evidence.sort_by(|a, b| {
            a.record_id
                .cmp(&b.record_id)
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
                .then_with(|| a.reason.cmp(&b.reason))
        });
    }
    edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

    let snapshot = GraphSnapshot {
        schema_version: 1,
        generated_at: rule_outcome.snapshot.generated_at.clone(),
        base_knowledge_signature: rule_outcome.snapshot.base_knowledge_signature.clone(),
        nodes,
        edges,
    };

    let (processed, reused) = recalc_graph_diff_counters(baseline, &snapshot);
    let coverage = ratio(
        collect_graph_covered_record_ids(
            &merged_payload,
            &file_summaries
                .iter()
                .map(|item| item.record_id.clone())
                .collect::<BTreeSet<_>>(),
        )
        .len(),
        total_records,
    );
    if total_records > 0 && coverage < target_ratio {
        issues.push(CompileL4Issue {
            edge_id: "llm_graph_coverage".to_string(),
            code: "W-L4-LLM-COVERAGE-LOW".to_string(),
            message: format!(
                "llm graph coverage {:.3} is below target {:.3} after {} rounds",
                coverage, target_ratio, rounds_used
            ),
            retryable: true,
        });
    }

    Ok(LlmGraphEnrichmentResult {
        outcome: CompileL4Outcome {
            snapshot,
            processed,
            reused,
            invalid_edges: rule_outcome.invalid_edges,
            issues,
        },
        metrics: LlmLayerMetrics {
            map_reduce: LlmLayerMapReduce {
                batch_size,
                batches_total,
                calls_total,
                rounds_used,
                rounds_max,
                reflection_enabled,
            },
            coverage: LlmLayerCoverage {
                covered: count_graph_covered_records(
                    &merged_payload,
                    &file_summaries
                        .iter()
                        .map(|item| item.record_id.clone())
                        .collect::<BTreeSet<_>>(),
                ),
                total: total_records,
                ratio: coverage,
                target_ratio,
            },
        },
    })
}

pub fn enrich_wiki_with_llm(
    baseline: Option<&WikiManifestSnapshot>,
    rule_outcome: &CompileL5Outcome,
    knowledge_snapshot: &KnowledgeSnapshot,
    graph_snapshot: &GraphSnapshot,
    generated_at: &str,
    config: &LlmSemanticConfig,
) -> Result<LlmWikiEnrichmentResult, String> {
    let Some(api_key) = config.key.as_deref() else {
        return Err("llm wiki enrichment requires KB_LLM_KEY".to_string());
    };

    let target_ratio = config.agent_min_chunk_coverage.clamp(0.0, 1.0);
    let rounds_max = config.agent_loop_max_rounds.clamp(1, 4);
    let reflection_enabled = config.agent_reflection_enabled && rounds_max > 1;
    let toolbox = SemanticAgentToolbox::new(config.workspace_root.clone());
    let wiki_context = toolbox.build_wiki_context(
        knowledge_snapshot,
        graph_snapshot,
        &rule_outcome.snapshot.pages,
    );
    let page_summaries = collect_wiki_page_summaries(&rule_outcome.snapshot.pages);
    let total_pages = page_summaries.len();
    let batch_size = normalize_batch_size(config.max_chunks, total_pages);
    let batches_total = count_batches(total_pages, batch_size);
    let mut calls_total = 0usize;
    let mut rounds_used = 0usize;
    let mut patch_map = BTreeMap::new();

    if total_pages > 0 {
        let all_page_ids = page_summaries
            .iter()
            .map(|item| item.page_id.clone())
            .collect::<BTreeSet<_>>();
        for round in 1..=rounds_max {
            let candidate = if round == 1 {
                page_summaries.clone()
            } else {
                page_summaries
                    .iter()
                    .filter(|item| !patch_map.contains_key(&item.page_id))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if candidate.is_empty() {
                break;
            }
            rounds_used = round;

            for batch in candidate.chunks(batch_size) {
                let batch_ids = batch
                    .iter()
                    .map(|item| item.page_id.as_str())
                    .collect::<BTreeSet<_>>();
                let batch_context = filter_wiki_context(&wiki_context, &batch_ids);
                let prompt = build_wiki_enrichment_prompt(
                    batch,
                    &batch_context,
                    round,
                    &batch
                        .iter()
                        .map(|item| item.page_id.clone())
                        .collect::<Vec<_>>(),
                );
                let response = call_openai_chat_with_retry(
                    config,
                    api_key,
                    "You enrich wiki pages from a local knowledge base. Return JSON only.",
                    &prompt,
                )?;
                calls_total += 1;
                let payload = decode_wiki_enrichment_payload(&response)?;
                merge_wiki_notes(&mut patch_map, payload.page_notes);
            }

            let coverage_ratio = ratio(patch_map.len(), total_pages);
            if !reflection_enabled || coverage_ratio >= target_ratio {
                break;
            }
            if patch_map.len() >= all_page_ids.len() {
                break;
            }
        }
    }

    let mut pages = Vec::new();
    let mut manifest_pages = Vec::new();
    let mut issues = rule_outcome.issues.clone();
    for page in &rule_outcome.snapshot.pages {
        let note = patch_map.get(&page.page_id);
        let markdown = render_page_with_llm_note(page, note);
        if note.is_none() {
            issues.push(CompileL5Issue {
                page_id: page.page_id.clone(),
                code: "W-L5-LLM-NOTE-MISSING".to_string(),
                message: "llm wiki note missing for page, kept rule-generated markdown".to_string(),
                retryable: true,
            });
        }
        let input_signature = format!("fnv1a64:{:016x}", fnv1a64(markdown.as_bytes()));
        pages.push(WikiPageDocument {
            page_id: page.page_id.clone(),
            page_type: page.page_type.clone(),
            title: page.title.clone(),
            relative_path: page.relative_path.clone(),
            input_signature: input_signature.clone(),
            source_records: page.source_records.clone(),
            source_chunks: page.source_chunks.clone(),
            markdown,
        });
        manifest_pages.push(WikiManifestEntry {
            page_id: page.page_id.clone(),
            page_type: page.page_type.clone(),
            title: page.title.clone(),
            relative_path: page.relative_path.clone(),
            input_signature,
            source_records: page.source_records.clone(),
            source_chunks: page.source_chunks.clone(),
        });
    }

    let manifest = WikiManifestSnapshot {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        base_graph_signature: rule_outcome.snapshot.manifest.base_graph_signature.clone(),
        pages: manifest_pages,
    };
    let snapshot = WikiViewSnapshot { manifest, pages };
    let (processed, reused) = recalc_wiki_diff_counters(baseline, &snapshot.manifest);
    let coverage_ratio = ratio(patch_map.len(), total_pages);
    if total_pages > 0 && coverage_ratio < target_ratio {
        issues.push(CompileL5Issue {
            page_id: "llm_wiki_coverage".to_string(),
            code: "W-L5-LLM-COVERAGE-LOW".to_string(),
            message: format!(
                "llm wiki coverage {:.3} is below target {:.3} after {} rounds",
                coverage_ratio, target_ratio, rounds_used
            ),
            retryable: true,
        });
    }

    Ok(LlmWikiEnrichmentResult {
        outcome: CompileL5Outcome {
            snapshot,
            processed,
            reused,
            removed_paths: rule_outcome.removed_paths.clone(),
            issues,
        },
        metrics: LlmLayerMetrics {
            map_reduce: LlmLayerMapReduce {
                batch_size,
                batches_total,
                calls_total,
                rounds_used,
                rounds_max,
                reflection_enabled,
            },
            coverage: LlmLayerCoverage {
                covered: patch_map.len(),
                total: total_pages,
                ratio: coverage_ratio,
                target_ratio,
            },
        },
    })
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct LlmGraphEnrichmentPayload {
    #[serde(default)]
    file_relations: Vec<LlmFileRelation>,
    #[serde(default)]
    file_concepts: Vec<LlmFileConcept>,
    #[serde(default)]
    concept_relations: Vec<LlmConceptRelation>,
    #[serde(default)]
    file_tags: Vec<LlmFileTag>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LlmFileRelation {
    from_record_id: String,
    to_record_id: String,
    reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LlmFileConcept {
    record_id: String,
    concept: String,
    reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LlmConceptRelation {
    concept_a: String,
    concept_b: String,
    reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LlmFileTag {
    record_id: String,
    tag: String,
    reason: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct LlmWikiEnrichmentPayload {
    #[serde(default)]
    page_notes: Vec<LlmWikiPageNote>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LlmWikiPageNote {
    page_id: String,
    summary: Option<String>,
    #[serde(default)]
    key_points: Vec<String>,
    #[serde(default)]
    see_also: Vec<String>,
}

fn collect_graph_file_summaries(knowledge_snapshot: &KnowledgeSnapshot) -> Vec<GraphFileSummary> {
    knowledge_snapshot
        .entries
        .iter()
        .filter(|entry| entry.status == KnowledgeStatus::Active)
        .filter_map(|entry| {
            let summary = entry.summary.trim();
            if summary.is_empty() {
                return None;
            }
            Some(GraphFileSummary {
                record_id: entry.record_id.clone(),
                path: entry.path.clone(),
                summary: stable_truncate(summary, 220),
                tags: entry
                    .tags
                    .iter()
                    .map(|tag| tag.trim().to_string())
                    .filter(|tag| !tag.is_empty())
                    .collect(),
                key_concepts: entry
                    .key_concepts
                    .iter()
                    .map(|concept| concept.trim().to_string())
                    .filter(|concept| !concept.is_empty())
                    .collect(),
            })
        })
        .collect()
}

fn collect_wiki_page_summaries(pages: &[WikiPageDocument]) -> Vec<WikiPageSummary> {
    pages
        .iter()
        .map(|page| WikiPageSummary {
            page_id: page.page_id.clone(),
            page_type: format!("{:?}", page.page_type).to_ascii_lowercase(),
            title: page.title.clone(),
            source_records: page.source_records.clone(),
            source_chunks: page.source_chunks.clone(),
        })
        .collect()
}

fn normalize_batch_size(configured: usize, total: usize) -> usize {
    if total == 0 {
        1
    } else {
        configured.clamp(1, total)
    }
}

fn count_batches(total: usize, batch_size: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let normalized = batch_size.max(1);
    ((total - 1) / normalized) + 1
}

fn filter_graph_context(graph_context: &Value, record_ids: &BTreeSet<&str>) -> Value {
    let summaries = graph_context
        .get("read_file_summary")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("record_id")
                        .and_then(Value::as_str)
                        .map(|record_id| record_ids.contains(record_id))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let neighbors = graph_context
        .get("read_graph_neighbors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("record_id")
                        .and_then(Value::as_str)
                        .map(|record_id| record_ids.contains(record_id))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "read_knowledge_graph": graph_context
            .get("read_knowledge_graph")
            .cloned()
            .unwrap_or_else(|| json!({ "node_count": 0, "edge_count": 0, "sample_edges": [] })),
        "read_file_summary": summaries,
        "read_graph_neighbors": neighbors,
    })
}

fn filter_wiki_context(wiki_context: &Value, page_ids: &BTreeSet<&str>) -> Value {
    let page_tools = wiki_context
        .get("read_wiki_page")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("page_id")
                        .and_then(Value::as_str)
                        .map(|page_id| page_ids.contains(page_id))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "read_knowledge_graph": wiki_context
            .get("read_knowledge_graph")
            .cloned()
            .unwrap_or_else(|| json!({ "node_count": 0, "edge_count": 0 })),
        "read_wiki_page": page_tools,
    })
}

fn build_graph_enrichment_prompt(
    batch: &[GraphFileSummary],
    graph_context: &serde_json::Value,
    round: usize,
    focus_record_ids: &[String],
) -> String {
    let summaries_json = serde_json::to_string_pretty(batch).unwrap_or_else(|_| "[]".to_string());
    let graph_json =
        serde_json::to_string_pretty(graph_context).unwrap_or_else(|_| "{}".to_string());
    let focus_json =
        serde_json::to_string_pretty(focus_record_ids).unwrap_or_else(|_| "[]".to_string());
    format!(
        "Task: enrich a local knowledge graph from semantic file summaries.\n\
LLM_FLOW:\n\
- map-reduce batch mode\n\
- current_round: {round}\n\
- focus_record_ids: {focus_json}\n\
ATOMIC_TOOLS_AVAILABLE:\n\
- read_file_summary(record_id_or_path)\n\
- read_knowledge_graph()\n\
- read_graph_neighbors(record_id)\n\
\n\
FILE_SUMMARIES_BATCH:\n\
{summaries_json}\n\
\n\
ATOMIC_TOOL_CONTEXT:\n\
{graph_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"file_relations\": [{{\"from_record_id\":\"docs/a\",\"to_record_id\":\"docs/b\",\"reason\":\"...\"}}],\n\
  \"file_concepts\": [{{\"record_id\":\"docs/a\",\"concept\":\"tokenized concept\",\"reason\":\"...\"}}],\n\
  \"concept_relations\": [{{\"concept_a\":\"token_a\",\"concept_b\":\"token_b\",\"reason\":\"...\"}}],\n\
  \"file_tags\": [{{\"record_id\":\"docs/a\",\"tag\":\"tokenized tag\",\"reason\":\"...\"}}]\n\
}}\n\
Rules:\n\
- Output JSON only.\n\
- record_id must exist in FILE_SUMMARIES_BATCH.\n\
- Prefer proposing at least one signal for each focus_record_id when evidence exists.\n\
- concept/tag should be lowercase short tokens.\n\
- Keep reason factual and short.\n\
- Prefer high-confidence relations only.",
        round = round,
        focus_json = focus_json,
        summaries_json = summaries_json,
        graph_json = graph_json
    )
}

fn build_wiki_enrichment_prompt(
    pages: &[WikiPageSummary],
    wiki_context: &serde_json::Value,
    round: usize,
    focus_page_ids: &[String],
) -> String {
    let page_json = serde_json::to_string_pretty(pages).unwrap_or_else(|_| "[]".to_string());
    let ctx_json = serde_json::to_string_pretty(wiki_context).unwrap_or_else(|_| "{}".to_string());
    let focus_json =
        serde_json::to_string_pretty(focus_page_ids).unwrap_or_else(|_| "[]".to_string());
    format!(
        "Task: improve wiki pages for readability and navigation.\n\
LLM_FLOW:\n\
- map-reduce batch mode\n\
- current_round: {round}\n\
- focus_page_ids: {focus_json}\n\
ATOMIC_TOOLS_AVAILABLE:\n\
- read_file_summary(record_id_or_path)\n\
- read_knowledge_graph()\n\
- read_wiki_page(page_id)\n\
\n\
WIKI_PAGES_BATCH:\n\
{page_json}\n\
\n\
ATOMIC_TOOL_CONTEXT:\n\
{ctx_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"page_notes\": [\n\
    {{\n\
      \"page_id\":\"source:docs/a\",\n\
      \"summary\":\"one concise summary\",\n\
      \"key_points\":[\"point 1\",\"point 2\"],\n\
      \"see_also\":[\"[[source:docs/b]]\"]\n\
    }}\n\
  ]\n\
}}\n\
Rules:\n\
- Output JSON only.\n\
- page_id must exist in WIKI_PAGES_BATCH.\n\
- Prefer generating one note for each focus_page_id.\n\
- Keep each key point short and factual.\n\
- Keep see_also concise.",
        round = round,
        focus_json = focus_json,
        page_json = page_json,
        ctx_json = ctx_json
    )
}

fn merge_graph_payload(
    target: &mut LlmGraphEnrichmentPayload,
    incoming: &LlmGraphEnrichmentPayload,
) {
    for item in &incoming.file_relations {
        if !target.file_relations.contains(item) {
            target.file_relations.push(item.clone());
        }
    }
    for item in &incoming.file_concepts {
        if !target.file_concepts.contains(item) {
            target.file_concepts.push(item.clone());
        }
    }
    for item in &incoming.concept_relations {
        if !target.concept_relations.contains(item) {
            target.concept_relations.push(item.clone());
        }
    }
    for item in &incoming.file_tags {
        if !target.file_tags.contains(item) {
            target.file_tags.push(item.clone());
        }
    }
}

fn collect_graph_covered_record_ids(
    payload: &LlmGraphEnrichmentPayload,
    known_record_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut covered = BTreeSet::new();
    for item in &payload.file_relations {
        if known_record_ids.contains(item.from_record_id.as_str()) {
            covered.insert(item.from_record_id.clone());
        }
        if known_record_ids.contains(item.to_record_id.as_str()) {
            covered.insert(item.to_record_id.clone());
        }
    }
    for item in &payload.file_concepts {
        if known_record_ids.contains(item.record_id.as_str()) {
            covered.insert(item.record_id.clone());
        }
    }
    for item in &payload.file_tags {
        if known_record_ids.contains(item.record_id.as_str()) {
            covered.insert(item.record_id.clone());
        }
    }
    covered
}

fn count_graph_covered_records(
    payload: &LlmGraphEnrichmentPayload,
    known_record_ids: &BTreeSet<String>,
) -> usize {
    collect_graph_covered_record_ids(payload, known_record_ids).len()
}

fn merge_wiki_notes(target: &mut BTreeMap<String, LlmWikiPageNote>, notes: Vec<LlmWikiPageNote>) {
    for mut note in notes {
        let page_id = note.page_id.trim().to_string();
        if page_id.is_empty() {
            continue;
        }
        note.page_id = page_id.clone();
        note.summary = normalize_text_field(note.summary.take());
        note.key_points = normalize_list(&note.key_points, 8);
        note.see_also = normalize_list(&note.see_also, 8);

        if let Some(existing) = target.get_mut(&page_id) {
            if existing.summary.is_none() {
                existing.summary = note.summary.clone();
            }
            for point in note.key_points {
                if !existing.key_points.contains(&point) {
                    existing.key_points.push(point);
                }
                if existing.key_points.len() >= 8 {
                    break;
                }
            }
            for item in note.see_also {
                if !existing.see_also.contains(&item) {
                    existing.see_also.push(item);
                }
                if existing.see_also.len() >= 8 {
                    break;
                }
            }
            continue;
        }
        target.insert(page_id, note);
    }
}

fn ratio(covered: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        covered as f64 / total as f64
    }
}

fn render_page_with_llm_note(page: &WikiPageDocument, note: Option<&LlmWikiPageNote>) -> String {
    let Some(note) = note else {
        return page.markdown.clone();
    };
    let summary = normalize_text_field(note.summary.clone())
        .unwrap_or_else(|| "No additional LLM summary.".to_string());
    let key_points = normalize_list(&note.key_points, 8);
    let see_also = normalize_list(&note.see_also, 8);

    let mut out = String::new();
    out.push_str(&page.markdown);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n## LLM Notes\n");
    out.push_str(&format!("{}\n\n", summary));
    out.push_str("### Key Points\n");
    if key_points.is_empty() {
        out.push_str("- (none)\n");
    } else {
        for point in key_points {
            out.push_str(&format!("- {}\n", point));
        }
    }
    out.push('\n');
    out.push_str("### See Also\n");
    if see_also.is_empty() {
        out.push_str("- (none)\n");
    } else {
        for item in see_also {
            out.push_str(&format!("- {}\n", item));
        }
    }
    out
}

fn apply_file_relations(
    items: &[LlmFileRelation],
    active_records: &BTreeSet<&str>,
    record_chunk_map: &BTreeMap<&str, Option<String>>,
    nodes_map: &mut BTreeMap<String, GraphNode>,
    edges_map: &mut BTreeMap<String, GraphEdge>,
    issues: &mut Vec<CompileL4Issue>,
) {
    for item in items {
        let from = item.from_record_id.trim();
        let to = item.to_record_id.trim();
        if from.is_empty() || to.is_empty() || from == to {
            continue;
        }
        if !active_records.contains(from) || !active_records.contains(to) {
            issues.push(CompileL4Issue {
                edge_id: format!("file_related_to_file|file:{}|file:{}", from, to),
                code: "W-L4-LLM-UNKNOWN-RECORD".to_string(),
                message: "llm proposed relation with unknown record_id".to_string(),
                retryable: true,
            });
            continue;
        }
        let (from_node, to_node) = canonical_file_pair(from, to);
        nodes_map
            .entry(from_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: from_node.clone(),
                node_type: GraphNodeType::File,
                label: from_node.trim_start_matches("file:").to_string(),
            });
        nodes_map
            .entry(to_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: to_node.clone(),
                node_type: GraphNodeType::File,
                label: to_node.trim_start_matches("file:").to_string(),
            });

        let edge_id = format!("file_related_to_file|{}|{}", from_node, to_node);
        let evidence = vec![
            GraphEvidence {
                record_id: from.to_string(),
                chunk_id: record_chunk_map.get(from).cloned().flatten(),
                reason: format!(
                    "llm inferred relation: {}",
                    stable_truncate(item.reason.trim(), 160)
                ),
            },
            GraphEvidence {
                record_id: to.to_string(),
                chunk_id: record_chunk_map.get(to).cloned().flatten(),
                reason: format!(
                    "llm inferred relation: {}",
                    stable_truncate(item.reason.trim(), 160)
                ),
            },
        ];
        upsert_edge(
            edges_map,
            GraphEdge {
                edge_id,
                edge_type: GraphEdgeType::FileRelatedToFile,
                from_node_id: from_node,
                to_node_id: to_node,
                evidence,
            },
        );
    }
}

fn apply_file_concepts(
    items: &[LlmFileConcept],
    active_records: &BTreeSet<&str>,
    record_chunk_map: &BTreeMap<&str, Option<String>>,
    nodes_map: &mut BTreeMap<String, GraphNode>,
    edges_map: &mut BTreeMap<String, GraphEdge>,
    issues: &mut Vec<CompileL4Issue>,
) {
    for item in items {
        let record_id = item.record_id.trim();
        if record_id.is_empty() || !active_records.contains(record_id) {
            issues.push(CompileL4Issue {
                edge_id: format!(
                    "file_mentions_concept|file:{}|concept:{}",
                    record_id, item.concept
                ),
                code: "W-L4-LLM-UNKNOWN-RECORD".to_string(),
                message: "llm proposed concept for unknown record_id".to_string(),
                retryable: true,
            });
            continue;
        }
        let concept = normalize_token(&item.concept);
        if concept.is_empty() {
            continue;
        }
        let file_node = format!("file:{}", record_id);
        let concept_node = format!("concept:{}", concept);
        nodes_map
            .entry(file_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: file_node.clone(),
                node_type: GraphNodeType::File,
                label: record_id.to_string(),
            });
        nodes_map
            .entry(concept_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: concept_node.clone(),
                node_type: GraphNodeType::Concept,
                label: concept.clone(),
            });
        let edge_id = format!("file_mentions_concept|{}|{}", file_node, concept_node);
        upsert_edge(
            edges_map,
            GraphEdge {
                edge_id,
                edge_type: GraphEdgeType::FileMentionsConcept,
                from_node_id: file_node,
                to_node_id: concept_node,
                evidence: vec![GraphEvidence {
                    record_id: record_id.to_string(),
                    chunk_id: record_chunk_map.get(record_id).cloned().flatten(),
                    reason: format!(
                        "llm inferred concept: {}",
                        stable_truncate(item.reason.trim(), 160)
                    ),
                }],
            },
        );
    }
}

fn apply_concept_relations(
    items: &[LlmConceptRelation],
    nodes_map: &mut BTreeMap<String, GraphNode>,
    edges_map: &mut BTreeMap<String, GraphEdge>,
    issues: &mut Vec<CompileL4Issue>,
) {
    for item in items {
        let concept_a = normalize_token(&item.concept_a);
        let concept_b = normalize_token(&item.concept_b);
        if concept_a.is_empty() || concept_b.is_empty() || concept_a == concept_b {
            continue;
        }
        let (left, right) = if concept_a <= concept_b {
            (concept_a, concept_b)
        } else {
            (concept_b, concept_a)
        };
        let from = format!("concept:{}", left);
        let to = format!("concept:{}", right);
        nodes_map.entry(from.clone()).or_insert_with(|| GraphNode {
            node_id: from.clone(),
            node_type: GraphNodeType::Concept,
            label: left.clone(),
        });
        nodes_map.entry(to.clone()).or_insert_with(|| GraphNode {
            node_id: to.clone(),
            node_type: GraphNodeType::Concept,
            label: right.clone(),
        });
        let reason = stable_truncate(item.reason.trim(), 160);
        if reason.is_empty() {
            issues.push(CompileL4Issue {
                edge_id: format!("concept_related_to_concept|{}|{}", from, to),
                code: "W-L4-LLM-MISSING-REASON".to_string(),
                message: "llm concept relation reason is empty".to_string(),
                retryable: true,
            });
            continue;
        }
        upsert_edge(
            edges_map,
            GraphEdge {
                edge_id: format!("concept_related_to_concept|{}|{}", from, to),
                edge_type: GraphEdgeType::ConceptRelatedToConcept,
                from_node_id: from,
                to_node_id: to,
                evidence: vec![GraphEvidence {
                    record_id: "llm".to_string(),
                    chunk_id: None,
                    reason: format!("llm inferred relation: {}", reason),
                }],
            },
        );
    }
}

fn apply_file_tags(
    items: &[LlmFileTag],
    active_records: &BTreeSet<&str>,
    record_chunk_map: &BTreeMap<&str, Option<String>>,
    nodes_map: &mut BTreeMap<String, GraphNode>,
    edges_map: &mut BTreeMap<String, GraphEdge>,
    issues: &mut Vec<CompileL4Issue>,
) {
    for item in items {
        let record_id = item.record_id.trim();
        if record_id.is_empty() || !active_records.contains(record_id) {
            issues.push(CompileL4Issue {
                edge_id: format!("file_has_tag|file:{}|tag:{}", record_id, item.tag),
                code: "W-L4-LLM-UNKNOWN-RECORD".to_string(),
                message: "llm proposed tag for unknown record_id".to_string(),
                retryable: true,
            });
            continue;
        }
        let tag = normalize_token(&item.tag);
        if tag.is_empty() {
            continue;
        }
        let file_node = format!("file:{}", record_id);
        let tag_node = format!("tag:{}", tag);
        nodes_map
            .entry(file_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: file_node.clone(),
                node_type: GraphNodeType::File,
                label: record_id.to_string(),
            });
        nodes_map
            .entry(tag_node.clone())
            .or_insert_with(|| GraphNode {
                node_id: tag_node.clone(),
                node_type: GraphNodeType::Tag,
                label: tag.clone(),
            });
        upsert_edge(
            edges_map,
            GraphEdge {
                edge_id: format!("file_has_tag|{}|{}", file_node, tag_node),
                edge_type: GraphEdgeType::FileHasTag,
                from_node_id: file_node,
                to_node_id: tag_node,
                evidence: vec![GraphEvidence {
                    record_id: record_id.to_string(),
                    chunk_id: record_chunk_map.get(record_id).cloned().flatten(),
                    reason: format!(
                        "llm inferred tag: {}",
                        stable_truncate(item.reason.trim(), 160)
                    ),
                }],
            },
        );
    }
}

fn canonical_file_pair(a: &str, b: &str) -> (String, String) {
    let left = format!("file:{}", a);
    let right = format!("file:{}", b);
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn upsert_edge(edges_map: &mut BTreeMap<String, GraphEdge>, edge: GraphEdge) {
    if let Some(existing) = edges_map.get_mut(&edge.edge_id) {
        for evidence in edge.evidence {
            if !existing.evidence.contains(&evidence) {
                existing.evidence.push(evidence);
            }
        }
        return;
    }
    edges_map.insert(edge.edge_id.clone(), edge);
}

fn first_chunk_map(knowledge_snapshot: &KnowledgeSnapshot) -> BTreeMap<&str, Option<String>> {
    knowledge_snapshot
        .entries
        .iter()
        .map(|entry| {
            let first = entry.chunks.first().map(|chunk| chunk.chunk_id.clone());
            (entry.record_id.as_str(), first)
        })
        .collect()
}

fn recalc_graph_diff_counters(
    baseline: Option<&GraphSnapshot>,
    snapshot: &GraphSnapshot,
) -> (usize, usize) {
    let mut baseline_map = BTreeMap::new();
    if let Some(baseline) = baseline {
        for edge in &baseline.edges {
            baseline_map.insert(edge.edge_id.as_str(), edge);
        }
    }

    let mut processed = 0usize;
    let mut reused = 0usize;
    for edge in &snapshot.edges {
        if let Some(previous) = baseline_map.get(edge.edge_id.as_str()) {
            if *previous == edge {
                reused += 1;
                continue;
            }
        }
        processed += 1;
    }
    (processed, reused)
}

fn recalc_wiki_diff_counters(
    baseline: Option<&WikiManifestSnapshot>,
    manifest: &WikiManifestSnapshot,
) -> (usize, usize) {
    let mut baseline_map = BTreeMap::new();
    if let Some(baseline) = baseline {
        for page in &baseline.pages {
            baseline_map.insert(page.page_id.as_str(), page);
        }
    }
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
    (processed, reused)
}

fn decode_graph_enrichment_payload(response: &Value) -> Result<LlmGraphEnrichmentPayload, String> {
    let content = extract_chat_message_content(response).ok_or_else(|| {
        "missing choices[0].message.content in graph enrichment response".to_string()
    })?;
    let payload_value = parse_json_object_from_text(&content)?;
    serde_json::from_value::<LlmGraphEnrichmentPayload>(payload_value)
        .map_err(|err| format!("invalid llm graph enrichment payload: {}", err))
}

fn decode_wiki_enrichment_payload(response: &Value) -> Result<LlmWikiEnrichmentPayload, String> {
    let content = extract_chat_message_content(response).ok_or_else(|| {
        "missing choices[0].message.content in wiki enrichment response".to_string()
    })?;
    let payload_value = parse_json_object_from_text(&content)?;
    serde_json::from_value::<LlmWikiEnrichmentPayload>(payload_value)
        .map_err(|err| format!("invalid llm wiki enrichment payload: {}", err))
}

fn call_openai_chat_with_retry(
    config: &LlmSemanticConfig,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Value, String> {
    let mut errors = Vec::new();
    let max_attempts = config.retry_max_attempts.max(1);
    for attempt in 1..=max_attempts {
        match call_openai_chat_once(config, api_key, system_prompt, user_prompt) {
            Ok(value) => return Ok(value),
            Err(err) => {
                errors.push(format!(
                    "attempt {}/{} failed: {}",
                    attempt, max_attempts, err
                ));
                if attempt < max_attempts && config.retry_backoff_ms > 0 {
                    std::thread::sleep(Duration::from_millis(config.retry_backoff_ms));
                }
            }
        }
    }
    Err(format!(
        "llm request failed after {} attempts; {}",
        max_attempts,
        errors.join(" | ")
    ))
}

fn call_openai_chat_once(
    config: &LlmSemanticConfig,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Value, String> {
    let body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": config.temperature,
        "max_completion_tokens": config.max_output_tokens,
        "response_format": { "type": "json_object" }
    });
    let endpoint = build_chat_completion_endpoint(&config.base_url);
    match send_openai_chat(api_key, &endpoint, config.timeout_secs, &body) {
        Ok(value) => Ok(value),
        Err(err) => {
            if supports_max_tokens_fallback(&err) {
                let fallback_body = json!({
                    "model": config.model,
                    "messages": [
                        { "role": "system", "content": system_prompt },
                        { "role": "user", "content": user_prompt }
                    ],
                    "temperature": config.temperature,
                    "max_tokens": config.max_output_tokens,
                    "response_format": { "type": "json_object" }
                });
                send_openai_chat(api_key, &endpoint, config.timeout_secs, &fallback_body).map_err(
                    |fallback_err| {
                        format!(
                            "primary(max_completion_tokens) failed: {}; fallback(max_tokens) failed: {}",
                            err, fallback_err
                        )
                    },
                )
            } else {
                Err(err)
            }
        }
    }
}

fn send_openai_chat(
    api_key: &str,
    endpoint: &str,
    timeout_secs: u64,
    body: &Value,
) -> Result<Value, String> {
    let request = ureq::post(endpoint)
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(timeout_secs));

    let raw = match request.send_string(&body.to_string()) {
        Ok(response) => response
            .into_string()
            .map_err(|err| format!("failed to read llm response: {}", err))?,
        Err(ureq::Error::Status(code, response)) => {
            let body_text = response
                .into_string()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(format!(
                "http status {} from llm provider: {}",
                code, body_text
            ));
        }
        Err(ureq::Error::Transport(err)) => {
            return Err(format!("llm transport error: {}", err));
        }
    };

    serde_json::from_str(&raw).map_err(|err| format!("invalid llm response json: {}", err))
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

fn extract_chat_message_content(response: &Value) -> Option<String> {
    let message_content = response.pointer("/choices/0/message/content")?;
    if let Some(text) = message_content.as_str() {
        return Some(text.to_string());
    }
    if let Some(parts) = message_content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}

fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Ok(value);
    }

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let candidate = &text[start..=end];
                if let Ok(value) = serde_json::from_str::<Value>(candidate) {
                    return Ok(value);
                }
            }
        }
    }

    Err("llm content is not a valid json object".to_string())
}

fn supports_max_tokens_fallback(error_text: &str) -> bool {
    let lowered = error_text.to_ascii_lowercase();
    lowered.contains("max_completion_tokens")
        || lowered.contains("unknown field")
        || lowered.contains("unrecognized field")
        || lowered.contains("invalid request")
        || lowered.contains("unexpected field")
}

fn normalize_text_field(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_list(items: &[String], max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(stable_truncate(trimmed, 180));
        }
        if out.len() >= max {
            break;
        }
    }
    out
}

fn normalize_token(input: &str) -> String {
    input
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        collect_graph_covered_record_ids, count_batches, merge_wiki_notes, normalize_batch_size,
        normalize_token, recalc_graph_diff_counters, LlmFileConcept, LlmFileRelation, LlmFileTag,
        LlmGraphEnrichmentPayload, LlmWikiPageNote,
    };
    use kb_core::domain::{GraphEdge, GraphEdgeType, GraphEvidence, GraphSnapshot};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn normalize_token_lowercases_and_joins() {
        assert_eq!(normalize_token("IoT Device"), "iot_device");
        assert_eq!(normalize_token("A/B+C"), "a_b_c");
    }

    #[test]
    fn recalc_graph_diff_counts_reuse_and_processed() {
        let edge = GraphEdge {
            edge_id: "e1".to_string(),
            edge_type: GraphEdgeType::FileHasTag,
            from_node_id: "file:a".to_string(),
            to_node_id: "tag:x".to_string(),
            evidence: vec![GraphEvidence {
                record_id: "a".to_string(),
                chunk_id: Some("a:1".to_string()),
                reason: "r".to_string(),
            }],
        };
        let baseline = GraphSnapshot {
            schema_version: 1,
            generated_at: "g1".to_string(),
            base_knowledge_signature: "k".to_string(),
            nodes: vec![],
            edges: vec![edge.clone()],
        };
        let current = GraphSnapshot {
            schema_version: 1,
            generated_at: "g2".to_string(),
            base_knowledge_signature: "k".to_string(),
            nodes: vec![],
            edges: vec![edge],
        };
        let (processed, reused) = recalc_graph_diff_counters(Some(&baseline), &current);
        assert_eq!(processed, 0);
        assert_eq!(reused, 1);
    }

    #[test]
    fn batch_helpers_normalize_and_count() {
        assert_eq!(normalize_batch_size(0, 8), 1);
        assert_eq!(normalize_batch_size(3, 8), 3);
        assert_eq!(normalize_batch_size(20, 8), 8);
        assert_eq!(count_batches(0, 4), 0);
        assert_eq!(count_batches(8, 3), 3);
    }

    #[test]
    fn graph_coverage_counts_known_records() {
        let payload = LlmGraphEnrichmentPayload {
            file_relations: vec![LlmFileRelation {
                from_record_id: "docs/a".to_string(),
                to_record_id: "docs/b".to_string(),
                reason: "link".to_string(),
            }],
            file_concepts: vec![LlmFileConcept {
                record_id: "docs/c".to_string(),
                concept: "agent".to_string(),
                reason: "reason".to_string(),
            }],
            concept_relations: vec![],
            file_tags: vec![LlmFileTag {
                record_id: "docs/z".to_string(),
                tag: "unknown".to_string(),
                reason: "ignore".to_string(),
            }],
        };
        let known = ["docs/a", "docs/b", "docs/c"]
            .into_iter()
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>();
        let covered = collect_graph_covered_record_ids(&payload, &known);
        assert_eq!(covered.len(), 3);
        assert!(covered.contains("docs/a"));
        assert!(covered.contains("docs/b"));
        assert!(covered.contains("docs/c"));
    }

    #[test]
    fn merge_wiki_notes_prefers_existing_summary_and_dedupes_lists() {
        let mut notes = BTreeMap::new();
        notes.insert(
            "source:docs/a".to_string(),
            LlmWikiPageNote {
                page_id: "source:docs/a".to_string(),
                summary: Some("primary summary".to_string()),
                key_points: vec!["point-a".to_string()],
                see_also: vec!["[[source:docs/b]]".to_string()],
            },
        );

        merge_wiki_notes(
            &mut notes,
            vec![LlmWikiPageNote {
                page_id: "source:docs/a".to_string(),
                summary: Some("secondary summary".to_string()),
                key_points: vec!["point-a".to_string(), "point-b".to_string()],
                see_also: vec![
                    "[[source:docs/b]]".to_string(),
                    "[[source:docs/c]]".to_string(),
                ],
            }],
        );

        let merged = notes
            .get("source:docs/a")
            .expect("merged note should exist");
        assert_eq!(merged.summary.as_deref(), Some("primary summary"));
        assert_eq!(
            merged.key_points,
            vec!["point-a".to_string(), "point-b".to_string()]
        );
        assert_eq!(
            merged.see_also,
            vec![
                "[[source:docs/b]]".to_string(),
                "[[source:docs/c]]".to_string()
            ]
        );
    }
}
