use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticProviderInfo {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Active,
    Missing,
    Failed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub chunk_id: String,
    pub byte_range: [u64; 2],
    pub content_signature: String,
    pub summary: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub record_id: String,
    pub path: String,
    pub input_signature: String,
    pub status: KnowledgeStatus,
    pub summary: String,
    pub tags: Vec<String>,
    pub key_concepts: Vec<String>,
    pub chunks: Vec<KnowledgeChunk>,
    pub provider: SemanticProviderInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub base_index_signature: String,
    pub entries: Vec<KnowledgeEntry>,
}
