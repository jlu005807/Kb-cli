use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use kb_core::domain::GraphSnapshot;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GraphFiles {
    pub graph_path: PathBuf,
}

impl GraphFiles {
    pub fn under(base_dir: &Path) -> Self {
        let kb_dir = base_dir.join(".kb");
        Self {
            graph_path: kb_dir.join("graph.json"),
        }
    }
}

pub fn write_graph_file(
    base_dir: &Path,
    snapshot: &GraphSnapshot,
) -> Result<GraphFiles, PersistenceError> {
    let files = GraphFiles::under(base_dir);
    let raw = serde_json::to_vec_pretty(snapshot).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.graph_path.clone(),
            },
            "failed to serialize graph snapshot",
            err,
        )
    })?;
    atomic_write_bytes(&files.graph_path, &raw)?;
    Ok(files)
}

pub fn read_graph_file(base_dir: &Path) -> Result<Option<GraphSnapshot>, PersistenceError> {
    let files = GraphFiles::under(base_dir);
    if !files.graph_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&files.graph_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.graph_path.clone(),
            },
            "failed to read graph.json",
            err,
        )
    })?;
    let parsed: GraphSnapshot = serde_json::from_str(&raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.graph_path.clone(),
            },
            "failed to parse graph.json",
            err,
        )
    })?;
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::{read_graph_file, write_graph_file};
    use kb_core::domain::{
        GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}", prefix, nonce))
    }

    #[test]
    fn roundtrip_graph_file() {
        let dir = unique_test_dir("kb-storage-graph-file");
        fs::create_dir_all(&dir).expect("create temp dir");
        let snapshot = GraphSnapshot {
            schema_version: 1,
            generated_at: "2026-05-02T00:00:00+08:00".to_string(),
            base_knowledge_signature: "knowledge:demo".to_string(),
            nodes: vec![
                GraphNode {
                    node_id: "file:docs/a".to_string(),
                    node_type: GraphNodeType::File,
                    label: "docs/a".to_string(),
                },
                GraphNode {
                    node_id: "tag:agent".to_string(),
                    node_type: GraphNodeType::Tag,
                    label: "agent".to_string(),
                },
            ],
            edges: vec![GraphEdge {
                edge_id: "file_has_tag|file:docs/a|tag:agent".to_string(),
                edge_type: GraphEdgeType::FileHasTag,
                from_node_id: "file:docs/a".to_string(),
                to_node_id: "tag:agent".to_string(),
                evidence: vec![GraphEvidence {
                    record_id: "docs/a".to_string(),
                    chunk_id: Some("docs/a:0001".to_string()),
                    reason: "tag derived from semantic extract".to_string(),
                }],
            }],
        };

        write_graph_file(&dir, &snapshot).expect("write graph");
        let loaded = read_graph_file(&dir).expect("read graph");
        assert_eq!(loaded, Some(snapshot));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
