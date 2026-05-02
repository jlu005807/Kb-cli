use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeType {
    File,
    Tag,
    Concept,
    Topic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub node_id: String,
    pub node_type: GraphNodeType,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeType {
    FileHasTag,
    FileMentionsConcept,
    ConceptRelatedToConcept,
    FileRelatedToFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEvidence {
    pub record_id: String,
    pub chunk_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub edge_id: String,
    pub edge_type: GraphEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    pub evidence: Vec<GraphEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub base_knowledge_signature: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
