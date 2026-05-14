use kb_core::domain::{
    GraphEdge, GraphSnapshot, KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus,
    WikiManifestSnapshot, WikiPageDocument,
};
use kb_core::SemanticExtractRequest;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_RAW_EXCERPT_MAX_CHARS: usize = 6_000;
const DEFAULT_RELATED_SUMMARY_LIMIT: usize = 6;
const DEFAULT_GRAPH_RELATION_LIMIT: usize = 10;
const DEFAULT_WIKI_MARKDOWN_EXCERPT_MAX_CHARS: usize = 500;

#[derive(Debug, Clone)]
pub struct SemanticAgentToolbox {
    workspace_root: Option<PathBuf>,
    raw_excerpt_max_chars: usize,
    related_summary_limit: usize,
    graph_relation_limit: usize,
    wiki_markdown_excerpt_max_chars: usize,
}

impl SemanticAgentToolbox {
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self {
            workspace_root,
            raw_excerpt_max_chars: DEFAULT_RAW_EXCERPT_MAX_CHARS,
            related_summary_limit: DEFAULT_RELATED_SUMMARY_LIMIT,
            graph_relation_limit: DEFAULT_GRAPH_RELATION_LIMIT,
            wiki_markdown_excerpt_max_chars: DEFAULT_WIKI_MARKDOWN_EXCERPT_MAX_CHARS,
        }
    }

    pub fn build_context(
        &self,
        request: &SemanticExtractRequest,
        fallback_source_text: &str,
    ) -> SemanticAgentContext {
        let mut context = SemanticAgentContext::default();

        context.raw_source_excerpt = self.tool_read_raw_file(request, fallback_source_text);
        let knowledge = self.load_knowledge_snapshot();
        let graph = self.load_graph_snapshot();

        context.current_file_summary = self.tool_read_file_summary(request, knowledge.as_ref());
        context.related_file_summaries =
            self.collect_related_summaries(request, knowledge.as_ref(), graph.as_ref());
        context.graph_relations = self.tool_read_knowledge_graph(request, graph.as_ref());

        context
    }

    pub fn build_graph_context(
        &self,
        knowledge: &KnowledgeSnapshot,
        graph: Option<&GraphSnapshot>,
    ) -> Value {
        let summaries = knowledge
            .entries
            .iter()
            .filter_map(|entry| summary_item_from_entry(entry, 1.0))
            .collect::<Vec<_>>();

        let graph_overview = graph
            .map(|snapshot| {
                json!({
                    "node_count": snapshot.nodes.len(),
                    "edge_count": snapshot.edges.len(),
                    "sample_edges": snapshot
                        .edges
                        .iter()
                        .take(self.graph_relation_limit)
                        .map(|edge| json!({
                            "edge_id": edge.edge_id,
                            "edge_type": format!("{:?}", edge.edge_type).to_ascii_lowercase(),
                            "from_node_id": edge.from_node_id,
                            "to_node_id": edge.to_node_id,
                        }))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| json!({ "node_count": 0, "edge_count": 0, "sample_edges": [] }));

        let neighbors = summaries
            .iter()
            .map(|item| {
                json!({
                    "record_id": item.record_id,
                    "path": item.path,
                    "relations": self.tool_read_graph_neighbors_by_record_id(&item.record_id, graph),
                })
            })
            .collect::<Vec<_>>();

        json!({
            "read_knowledge_graph": graph_overview,
            "read_file_summary": summaries,
            "read_graph_neighbors": neighbors,
        })
    }

    pub fn build_wiki_context(
        &self,
        knowledge: &KnowledgeSnapshot,
        graph: &GraphSnapshot,
        pages: &[WikiPageDocument],
    ) -> Value {
        let page_tools = pages
            .iter()
            .map(|page| {
                let source_summaries = page
                    .source_records
                    .iter()
                    .filter_map(|record_id| {
                        self.find_summary_by_record_id(knowledge, record_id)
                            .or_else(|| self.find_summary_by_path(knowledge, record_id))
                    })
                    .collect::<Vec<_>>();

                let mut graph_relations = Vec::new();
                for record_id in &page.source_records {
                    for relation in
                        self.tool_read_graph_neighbors_by_record_id(record_id, Some(graph))
                    {
                        if !graph_relations.contains(&relation) {
                            graph_relations.push(relation);
                        }
                        if graph_relations.len() >= self.graph_relation_limit {
                            break;
                        }
                    }
                    if graph_relations.len() >= self.graph_relation_limit {
                        break;
                    }
                }

                json!({
                    "page_id": page.page_id,
                    "title": page.title,
                    "page_type": format!("{:?}", page.page_type).to_ascii_lowercase(),
                    "source_records": page.source_records,
                    "source_summaries": source_summaries,
                    "graph_relations": graph_relations,
                    "markdown_excerpt": stable_truncate(page.markdown.trim(), self.wiki_markdown_excerpt_max_chars),
                })
            })
            .collect::<Vec<_>>();

        json!({
            "read_knowledge_graph": {
                "node_count": graph.nodes.len(),
                "edge_count": graph.edges.len(),
            },
            "read_wiki_page": page_tools,
        })
    }

    pub fn read_file_summary(&self, record_id_or_path: &str) -> Option<AgentSummaryItem> {
        let knowledge = self.load_knowledge_snapshot()?;
        self.find_summary_by_record_id(&knowledge, record_id_or_path)
            .or_else(|| self.find_summary_by_path(&knowledge, record_id_or_path))
    }

    pub fn read_graph_overview(&self) -> AgentGraphOverview {
        match self.load_graph_snapshot() {
            Some(graph) => AgentGraphOverview {
                node_count: graph.nodes.len(),
                edge_count: graph.edges.len(),
                sample_edges: graph
                    .edges
                    .iter()
                    .take(self.graph_relation_limit)
                    .map(graph_edge_item)
                    .collect(),
            },
            None => AgentGraphOverview {
                node_count: 0,
                edge_count: 0,
                sample_edges: vec![],
            },
        }
    }

    pub fn read_graph_neighbors(&self, record_id: &str) -> Vec<AgentGraphRelation> {
        let graph = self.load_graph_snapshot();
        self.tool_read_graph_neighbors_by_record_id(record_id, graph.as_ref())
    }

    pub fn read_graph_edge(&self, edge_id: &str) -> Option<AgentGraphEdgeDetail> {
        let graph = self.load_graph_snapshot()?;
        let edge = graph.edges.iter().find(|item| item.edge_id == edge_id)?;
        Some(AgentGraphEdgeDetail {
            edge_id: edge.edge_id.clone(),
            edge_type: format!("{:?}", edge.edge_type).to_ascii_lowercase(),
            from_node_id: edge.from_node_id.clone(),
            to_node_id: edge.to_node_id.clone(),
            evidence: edge
                .evidence
                .iter()
                .map(|item| AgentGraphEdgeEvidence {
                    record_id: item.record_id.clone(),
                    chunk_id: item.chunk_id.clone(),
                    reason: item.reason.clone(),
                })
                .collect(),
        })
    }

    pub fn read_wiki_page(&self, page_id: &str) -> Option<AgentWikiPageItem> {
        let wiki = self.load_wiki_snapshot()?;
        let page = wiki.pages.iter().find(|item| item.page_id == page_id)?;
        Some(AgentWikiPageItem {
            page_id: page.page_id.clone(),
            title: page.title.clone(),
            relative_path: page.relative_path.clone(),
            page_type: format!("{:?}", page.page_type).to_ascii_lowercase(),
            source_records: page.source_records.clone(),
            markdown_excerpt: stable_truncate(
                page.markdown.trim(),
                self.wiki_markdown_excerpt_max_chars,
            ),
        })
    }

    pub fn read_raw_chunk(
        &self,
        record_id_or_path: &str,
        chunk_id: Option<&str>,
    ) -> Option<AgentRawChunkItem> {
        let knowledge = self.load_knowledge_snapshot()?;
        let entry = knowledge
            .entries
            .iter()
            .find(|item| item.record_id == record_id_or_path || item.path == record_id_or_path)?;
        let root = self.workspace_root.as_ref()?;
        let source_path = resolve_source_path(root, &entry.path);
        let raw = fs::read_to_string(source_path).ok()?;
        let chunk = selected_chunk(entry, chunk_id)?;
        let excerpt = excerpt_for_chunk(&raw, chunk, self.raw_excerpt_max_chars);
        Some(AgentRawChunkItem {
            record_id: entry.record_id.clone(),
            path: entry.path.clone(),
            chunk_id: chunk.chunk_id.clone(),
            preview: excerpt,
        })
    }

    fn tool_read_raw_file(
        &self,
        request: &SemanticExtractRequest,
        fallback_source_text: &str,
    ) -> String {
        let Some(root) = self.workspace_root.as_ref() else {
            return stable_truncate(fallback_source_text, self.raw_excerpt_max_chars);
        };
        let abs = resolve_source_path(root, &request.path);
        match fs::read_to_string(&abs) {
            Ok(raw) => stable_truncate(raw.trim(), self.raw_excerpt_max_chars),
            Err(_) => stable_truncate(fallback_source_text, self.raw_excerpt_max_chars),
        }
    }

    fn tool_read_file_summary(
        &self,
        request: &SemanticExtractRequest,
        knowledge: Option<&KnowledgeSnapshot>,
    ) -> Option<AgentSummaryItem> {
        let Some(knowledge) = knowledge else {
            return None;
        };
        knowledge
            .entries
            .iter()
            .find(|entry| entry.record_id == request.record_id || entry.path == request.path)
            .and_then(|entry| summary_item_from_entry(entry, 1.0))
    }

    fn collect_related_summaries(
        &self,
        request: &SemanticExtractRequest,
        knowledge: Option<&KnowledgeSnapshot>,
        graph: Option<&GraphSnapshot>,
    ) -> Vec<AgentSummaryItem> {
        let Some(knowledge) = knowledge else {
            return vec![];
        };
        let Some(current) = self.tool_read_file_summary(request, Some(knowledge)) else {
            return vec![];
        };

        let related_by_graph = collect_related_record_ids_from_graph(request, graph);
        let current_tags = current
            .tags
            .iter()
            .map(|x| x.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let current_concepts = current
            .key_concepts
            .iter()
            .map(|x| x.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();

        let mut ranked = knowledge
            .entries
            .iter()
            .filter(|entry| entry.record_id != request.record_id)
            .filter_map(|entry| {
                let mut score = 0.0;
                if related_by_graph.contains(entry.record_id.as_str()) {
                    score += 2.0;
                }
                for tag in &entry.tags {
                    if current_tags.contains(&tag.to_ascii_lowercase()) {
                        score += 0.5;
                    }
                }
                for concept in &entry.key_concepts {
                    if current_concepts.contains(&concept.to_ascii_lowercase()) {
                        score += 0.75;
                    }
                }
                summary_item_from_entry(entry, score)
            })
            .collect::<Vec<_>>();

        ranked.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.record_id.cmp(&b.record_id))
        });
        ranked.truncate(self.related_summary_limit);
        ranked
    }

    fn tool_read_knowledge_graph(
        &self,
        request: &SemanticExtractRequest,
        graph: Option<&GraphSnapshot>,
    ) -> Vec<AgentGraphRelation> {
        self.tool_read_graph_neighbors_by_record_id(&request.record_id, graph)
    }

    fn tool_read_graph_neighbors_by_record_id(
        &self,
        record_id: &str,
        graph: Option<&GraphSnapshot>,
    ) -> Vec<AgentGraphRelation> {
        let Some(graph) = graph else {
            return vec![];
        };
        let current_file_node = format!("file:{}", record_id);
        let mut relations = Vec::new();
        for edge in &graph.edges {
            let mut matched = edge.evidence.iter().any(|ev| ev.record_id == record_id);
            if !matched {
                matched =
                    edge.from_node_id == current_file_node || edge.to_node_id == current_file_node;
            }
            if !matched {
                continue;
            }
            let related = if edge.from_node_id == current_file_node {
                edge.to_node_id.clone()
            } else if edge.to_node_id == current_file_node {
                edge.from_node_id.clone()
            } else {
                edge.to_node_id.clone()
            };
            let reason = edge
                .evidence
                .first()
                .map(|ev| ev.reason.clone())
                .unwrap_or_default();
            relations.push(AgentGraphRelation {
                edge_id: edge.edge_id.clone(),
                edge_type: format!("{:?}", edge.edge_type).to_ascii_lowercase(),
                related_node_id: related,
                reason: stable_truncate(reason.trim(), 160),
            });
        }
        relations.truncate(self.graph_relation_limit);
        relations
    }

    fn find_summary_by_record_id(
        &self,
        knowledge: &KnowledgeSnapshot,
        record_id: &str,
    ) -> Option<AgentSummaryItem> {
        knowledge
            .entries
            .iter()
            .find(|entry| entry.record_id == record_id)
            .and_then(|entry| summary_item_from_entry(entry, 1.0))
    }

    fn find_summary_by_path(
        &self,
        knowledge: &KnowledgeSnapshot,
        path: &str,
    ) -> Option<AgentSummaryItem> {
        knowledge
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .and_then(|entry| summary_item_from_entry(entry, 1.0))
    }

    fn load_knowledge_snapshot(&self) -> Option<KnowledgeSnapshot> {
        let root = self.workspace_root.as_ref()?;
        let path = root.join(".kb").join("knowledge.json");
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn load_graph_snapshot(&self) -> Option<GraphSnapshot> {
        let root = self.workspace_root.as_ref()?;
        let path = root.join(".kb").join("graph.json");
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn load_wiki_snapshot(&self) -> Option<AgentWikiSnapshot> {
        let root = self.workspace_root.as_ref()?;
        let manifest_raw =
            fs::read_to_string(root.join(".kb").join("wiki").join("manifest.json")).ok()?;
        let manifest = serde_json::from_str::<WikiManifestSnapshot>(&manifest_raw).ok()?;
        let mut pages = Vec::new();
        for entry in &manifest.pages {
            let page_raw =
                fs::read_to_string(root.join(".kb").join("wiki").join(&entry.relative_path))
                    .ok()?;
            pages.push(WikiPageDocument {
                page_id: entry.page_id.clone(),
                page_type: entry.page_type.clone(),
                title: entry.title.clone(),
                relative_path: entry.relative_path.clone(),
                input_signature: entry.input_signature.clone(),
                source_records: entry.source_records.clone(),
                source_chunks: entry.source_chunks.clone(),
                markdown: page_raw,
            });
        }
        Some(AgentWikiSnapshot { pages })
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SemanticAgentContext {
    pub raw_source_excerpt: String,
    pub current_file_summary: Option<AgentSummaryItem>,
    pub related_file_summaries: Vec<AgentSummaryItem>,
    pub graph_relations: Vec<AgentGraphRelation>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentSummaryItem {
    pub record_id: String,
    pub path: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub key_concepts: Vec<String>,
    pub relevance_score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentGraphRelation {
    pub edge_id: String,
    pub edge_type: String,
    pub related_node_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentGraphOverview {
    pub node_count: usize,
    pub edge_count: usize,
    pub sample_edges: Vec<AgentGraphEdgeItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentGraphEdgeItem {
    pub edge_id: String,
    pub edge_type: String,
    pub from_node_id: String,
    pub to_node_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentGraphEdgeDetail {
    pub edge_id: String,
    pub edge_type: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub evidence: Vec<AgentGraphEdgeEvidence>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentGraphEdgeEvidence {
    pub record_id: String,
    pub chunk_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWikiPageItem {
    pub page_id: String,
    pub title: String,
    pub relative_path: String,
    pub page_type: String,
    pub source_records: Vec<String>,
    pub markdown_excerpt: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentRawChunkItem {
    pub record_id: String,
    pub path: String,
    pub chunk_id: String,
    pub preview: String,
}

#[derive(Debug, Clone)]
struct AgentWikiSnapshot {
    pages: Vec<WikiPageDocument>,
}

fn graph_edge_item(edge: &GraphEdge) -> AgentGraphEdgeItem {
    AgentGraphEdgeItem {
        edge_id: edge.edge_id.clone(),
        edge_type: format!("{:?}", edge.edge_type).to_ascii_lowercase(),
        from_node_id: edge.from_node_id.clone(),
        to_node_id: edge.to_node_id.clone(),
    }
}

fn selected_chunk<'a>(
    entry: &'a KnowledgeEntry,
    chunk_id: Option<&str>,
) -> Option<&'a KnowledgeChunk> {
    if let Some(chunk_id) = chunk_id {
        return entry.chunks.iter().find(|item| item.chunk_id == chunk_id);
    }
    entry.chunks.first()
}

fn excerpt_for_chunk(raw: &str, chunk: &KnowledgeChunk, max_chars: usize) -> String {
    let raw_len_u64 = raw.len() as u64;
    let start = chunk.byte_range[0].min(raw_len_u64) as usize;
    let end = chunk.byte_range[1].min(raw_len_u64) as usize;
    if start >= end {
        return stable_truncate(raw.trim(), max_chars);
    }
    stable_truncate(raw[start..end].trim(), max_chars)
}

fn summary_item_from_entry(entry: &KnowledgeEntry, score: f64) -> Option<AgentSummaryItem> {
    if !matches!(entry.status, KnowledgeStatus::Active) {
        return None;
    }
    let summary = entry.summary.trim();
    if summary.is_empty() {
        return None;
    }
    Some(AgentSummaryItem {
        record_id: entry.record_id.clone(),
        path: entry.path.clone(),
        summary: stable_truncate(summary, 220),
        tags: entry.tags.iter().map(|x| x.trim().to_string()).collect(),
        key_concepts: entry
            .key_concepts
            .iter()
            .map(|x| x.trim().to_string())
            .collect(),
        relevance_score: score,
    })
}

fn collect_related_record_ids_from_graph(
    request: &SemanticExtractRequest,
    graph: Option<&GraphSnapshot>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(graph) = graph else {
        return out;
    };
    let current_file_node = format!("file:{}", request.record_id);
    for edge in &graph.edges {
        let touches_current =
            edge.from_node_id == current_file_node || edge.to_node_id == current_file_node;
        if !touches_current {
            continue;
        }
        if let Some(record_id) = edge.from_node_id.strip_prefix("file:") {
            if record_id != request.record_id {
                out.insert(record_id.to_string());
            }
        }
        if let Some(record_id) = edge.to_node_id.strip_prefix("file:") {
            if record_id != request.record_id {
                out.insert(record_id.to_string());
            }
        }
    }
    out
}

fn resolve_source_path(root: &Path, raw_path: &str) -> PathBuf {
    let candidate = PathBuf::from(raw_path);
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(raw_path.trim_start_matches("./"))
    }
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
    use super::SemanticAgentToolbox;
    use kb_core::domain::{
        GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot,
        KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
        WikiPageDocument, WikiPageType,
    };
    use kb_core::SemanticExtractRequest;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kb_llm_agent_tools_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn builds_context_from_source_knowledge_and_graph() {
        let root = unique_temp_dir("context-build");
        fs::create_dir_all(root.join(".kb")).expect("create .kb");
        fs::create_dir_all(root.join("docs")).expect("create docs");

        fs::write(root.join("docs/a.md"), "A raw source line").expect("write source");
        let knowledge = KnowledgeSnapshot {
            schema_version: 1,
            generated_at: "now".to_string(),
            base_index_signature: "idx".to_string(),
            entries: vec![
                KnowledgeEntry {
                    record_id: "docs/a".to_string(),
                    path: "./docs/a.md".to_string(),
                    input_signature: "sig-a".to_string(),
                    status: KnowledgeStatus::Active,
                    summary: "summary-a".to_string(),
                    tags: vec!["rust".to_string()],
                    key_concepts: vec!["agent".to_string()],
                    chunks: vec![KnowledgeChunk {
                        chunk_id: "docs/a:0001".to_string(),
                        byte_range: [0, 10],
                        content_signature: "c-a".to_string(),
                        summary: "chunk-a".to_string(),
                        preview: "preview-a".to_string(),
                    }],
                    provider: SemanticProviderInfo {
                        kind: "rule".to_string(),
                        name: "rule".to_string(),
                    },
                },
                KnowledgeEntry {
                    record_id: "docs/b".to_string(),
                    path: "./docs/b.md".to_string(),
                    input_signature: "sig-b".to_string(),
                    status: KnowledgeStatus::Active,
                    summary: "summary-b".to_string(),
                    tags: vec!["rust".to_string()],
                    key_concepts: vec!["memory".to_string()],
                    chunks: vec![KnowledgeChunk {
                        chunk_id: "docs/b:0001".to_string(),
                        byte_range: [0, 10],
                        content_signature: "c-b".to_string(),
                        summary: "chunk-b".to_string(),
                        preview: "preview-b".to_string(),
                    }],
                    provider: SemanticProviderInfo {
                        kind: "rule".to_string(),
                        name: "rule".to_string(),
                    },
                },
            ],
        };
        fs::write(
            root.join(".kb/knowledge.json"),
            serde_json::to_string_pretty(&knowledge).expect("serialize knowledge"),
        )
        .expect("write knowledge");

        let graph = GraphSnapshot {
            schema_version: 1,
            generated_at: "now".to_string(),
            base_knowledge_signature: "k".to_string(),
            nodes: vec![
                GraphNode {
                    node_id: "file:docs/a".to_string(),
                    node_type: GraphNodeType::File,
                    label: "docs/a".to_string(),
                },
                GraphNode {
                    node_id: "file:docs/b".to_string(),
                    node_type: GraphNodeType::File,
                    label: "docs/b".to_string(),
                },
            ],
            edges: vec![GraphEdge {
                edge_id: "file_related_to_file|file:docs/a|file:docs/b".to_string(),
                edge_type: GraphEdgeType::FileRelatedToFile,
                from_node_id: "file:docs/a".to_string(),
                to_node_id: "file:docs/b".to_string(),
                evidence: vec![GraphEvidence {
                    record_id: "docs/a".to_string(),
                    chunk_id: Some("docs/a:0001".to_string()),
                    reason: "shared term".to_string(),
                }],
            }],
        };
        fs::write(
            root.join(".kb/graph.json"),
            serde_json::to_string_pretty(&graph).expect("serialize graph"),
        )
        .expect("write graph");

        let request = SemanticExtractRequest {
            record_id: "docs/a".to_string(),
            path: "./docs/a.md".to_string(),
            text: "fallback".to_string(),
            input_signature: "sig".to_string(),
        };

        let toolbox = SemanticAgentToolbox::new(Some(root.clone()));
        let ctx = toolbox.build_context(&request, "fallback");
        assert!(ctx.raw_source_excerpt.contains("A raw source line"));
        assert_eq!(
            ctx.current_file_summary
                .as_ref()
                .map(|x| x.record_id.as_str()),
            Some("docs/a")
        );
        assert!(ctx
            .related_file_summaries
            .iter()
            .any(|item| item.record_id == "docs/b"));
        assert!(ctx
            .graph_relations
            .iter()
            .any(|rel| rel.related_node_id == "file:docs/b"));

        let graph_ctx = toolbox.build_graph_context(&knowledge, Some(&graph));
        assert!(graph_ctx["read_file_summary"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false));
        assert!(graph_ctx["read_graph_neighbors"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false));

        let pages = vec![WikiPageDocument {
            page_id: "source:docs/a".to_string(),
            page_type: WikiPageType::Source,
            title: "docs/a".to_string(),
            relative_path: "pages/source/docs_a.md".to_string(),
            input_signature: "sig".to_string(),
            source_records: vec!["docs/a".to_string()],
            source_chunks: vec!["docs/a:0001".to_string()],
            markdown: "# docs/a\n\nsummary-a".to_string(),
        }];
        let wiki_ctx = toolbox.build_wiki_context(&knowledge, &graph, &pages);
        assert_eq!(
            wiki_ctx["read_wiki_page"][0]["page_id"]
                .as_str()
                .unwrap_or_default(),
            "source:docs/a"
        );
    }
}
