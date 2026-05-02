use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use kb_core::domain::SearchProjectionSnapshot;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SearchFiles {
    pub search_path: PathBuf,
}

impl SearchFiles {
    pub fn under(base_dir: &Path) -> Self {
        let kb_dir = base_dir.join(".kb");
        Self {
            search_path: kb_dir.join("search.json"),
        }
    }
}

pub fn write_search_file(
    base_dir: &Path,
    snapshot: &SearchProjectionSnapshot,
) -> Result<SearchFiles, PersistenceError> {
    let files = SearchFiles::under(base_dir);
    let raw = serde_json::to_vec_pretty(snapshot).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.search_path.clone(),
            },
            "failed to serialize search projection snapshot",
            err,
        )
    })?;
    atomic_write_bytes(&files.search_path, &raw)?;
    Ok(files)
}

pub fn read_search_file(
    base_dir: &Path,
) -> Result<Option<SearchProjectionSnapshot>, PersistenceError> {
    let files = SearchFiles::under(base_dir);
    if !files.search_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&files.search_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.search_path.clone(),
            },
            "failed to read search.json",
            err,
        )
    })?;
    let parsed: SearchProjectionSnapshot = serde_json::from_str(&raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.search_path.clone(),
            },
            "failed to parse search.json",
            err,
        )
    })?;
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::{read_search_file, write_search_file};
    use kb_core::domain::{SearchHit, SearchProjectionSnapshot};
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
    fn roundtrip_search_file() {
        let dir = unique_test_dir("kb-storage-search-file");
        fs::create_dir_all(&dir).expect("create temp dir");
        let snapshot = SearchProjectionSnapshot {
            schema_version: 1,
            generated_at: "2026-05-02T00:00:00+08:00".to_string(),
            base_knowledge_signature: "knowledge:demo".to_string(),
            hits: vec![SearchHit {
                key: "docs/a|docs/a:0001|c:1".to_string(),
                record_id: "docs/a".to_string(),
                chunk_id: "docs/a:0001".to_string(),
                content_signature: "c:1".to_string(),
                score: 0.91,
                matched_terms: vec!["agent".to_string(), "memory".to_string()],
            }],
        };

        write_search_file(&dir, &snapshot).expect("write search");
        let loaded = read_search_file(&dir).expect("read search");
        assert_eq!(loaded, Some(snapshot));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
