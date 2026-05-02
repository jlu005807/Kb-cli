use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub key: String,
    pub record_id: String,
    pub chunk_id: String,
    pub content_signature: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchProjectionSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub base_knowledge_signature: String,
    pub hits: Vec<SearchHit>,
}
