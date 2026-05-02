use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiPageType {
    Index,
    Log,
    Overview,
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiManifestEntry {
    pub page_id: String,
    pub page_type: WikiPageType,
    pub title: String,
    pub relative_path: String,
    pub input_signature: String,
    pub source_records: Vec<String>,
    pub source_chunks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiManifestSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub base_graph_signature: String,
    pub pages: Vec<WikiManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPageDocument {
    pub page_id: String,
    pub page_type: WikiPageType,
    pub title: String,
    pub relative_path: String,
    pub input_signature: String,
    pub source_records: Vec<String>,
    pub source_chunks: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiViewSnapshot {
    pub manifest: WikiManifestSnapshot,
    pub pages: Vec<WikiPageDocument>,
}
