use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use kb_core::domain::KnowledgeSnapshot;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KnowledgeFiles {
    pub knowledge_path: PathBuf,
}

impl KnowledgeFiles {
    pub fn under(base_dir: &Path) -> Self {
        let kb_dir = base_dir.join(".kb");
        Self {
            knowledge_path: kb_dir.join("knowledge.json"),
        }
    }
}

pub fn write_knowledge_file(
    base_dir: &Path,
    snapshot: &KnowledgeSnapshot,
) -> Result<KnowledgeFiles, PersistenceError> {
    let files = KnowledgeFiles::under(base_dir);
    let raw = serde_json::to_vec_pretty(snapshot).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.knowledge_path.clone(),
            },
            "failed to serialize knowledge snapshot",
            err,
        )
    })?;
    atomic_write_bytes(&files.knowledge_path, &raw)?;
    Ok(files)
}

pub fn read_knowledge_file(base_dir: &Path) -> Result<Option<KnowledgeSnapshot>, PersistenceError> {
    let files = KnowledgeFiles::under(base_dir);
    if !files.knowledge_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&files.knowledge_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.knowledge_path.clone(),
            },
            "failed to read knowledge.json",
            err,
        )
    })?;
    let parsed: KnowledgeSnapshot = serde_json::from_str(&raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.knowledge_path.clone(),
            },
            "failed to parse knowledge.json",
            err,
        )
    })?;
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::{read_knowledge_file, write_knowledge_file};
    use kb_core::domain::{
        KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
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
    fn roundtrip_knowledge_file() {
        let dir = unique_test_dir("kb-storage-knowledge-file");
        fs::create_dir_all(&dir).expect("create temp dir");
        let snapshot = KnowledgeSnapshot {
            schema_version: 1,
            generated_at: "2026-05-02T00:00:00+08:00".to_string(),
            base_index_signature: "index:demo".to_string(),
            entries: vec![KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "f:1".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["raw".to_string()],
                key_concepts: vec!["concept".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/a:0001".to_string(),
                    byte_range: [0, 32],
                    content_signature: "c:1".to_string(),
                    summary: "chunk".to_string(),
                    preview: "preview".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-based-v1".to_string(),
                },
                ..KnowledgeEntry::default()
            }],
        };

        write_knowledge_file(&dir, &snapshot).expect("write knowledge");
        let loaded = read_knowledge_file(&dir).expect("read knowledge");
        assert_eq!(loaded, Some(snapshot));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
