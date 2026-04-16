use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use kb_core::domain::{IndexMeta, IndexSnapshot};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IndexFiles {
    pub index_path: PathBuf,
    pub errors_path: PathBuf,
    pub meta_path: PathBuf,
}

impl IndexFiles {
    // 统一索引文件布局，避免各调用方硬编码路径。
    pub fn under(base_dir: &Path) -> Self {
        let kb_dir = base_dir.join(".kb");
        Self {
            index_path: kb_dir.join("index.json"),
            errors_path: kb_dir.join("index.errors.json"),
            meta_path: kb_dir.join("index.meta.json"),
        }
    }
}

pub fn write_index_files(
    base_dir: &Path,
    snapshot: &IndexSnapshot,
) -> Result<IndexFiles, PersistenceError> {
    let files = IndexFiles::under(base_dir);

    // 序列化与落盘分离，便于分别定位“编码失败”与“IO失败”。
    let records = serde_json::to_vec_pretty(&snapshot.records).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.index_path.clone(),
            },
            "failed to serialize index records",
            err,
        )
    })?;
    let errors = serde_json::to_vec_pretty(&snapshot.errors).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.errors_path.clone(),
            },
            "failed to serialize index errors",
            err,
        )
    })?;
    let meta = serde_json::to_vec_pretty(&snapshot.meta).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.meta_path.clone(),
            },
            "failed to serialize index meta",
            err,
        )
    })?;

    atomic_write_bytes(&files.index_path, &records)?;
    atomic_write_bytes(&files.errors_path, &errors)?;
    atomic_write_bytes(&files.meta_path, &meta)?;

    Ok(files)
}

pub fn read_index_files(base_dir: &Path) -> Result<Option<IndexSnapshot>, PersistenceError> {
    let files = IndexFiles::under(base_dir);
    // 三文件任意缺失都视为“索引未初始化”，而不是格式错误。
    if !files.index_path.exists() || !files.errors_path.exists() || !files.meta_path.exists() {
        return Ok(None);
    }

    let records_raw = fs::read_to_string(&files.index_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.index_path.clone(),
            },
            "failed to read index.json",
            err,
        )
    })?;
    let errors_raw = fs::read_to_string(&files.errors_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.errors_path.clone(),
            },
            "failed to read index.errors.json",
            err,
        )
    })?;
    let meta_raw = fs::read_to_string(&files.meta_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.meta_path.clone(),
            },
            "failed to read index.meta.json",
            err,
        )
    })?;

    let records = serde_json::from_str(&records_raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.index_path.clone(),
            },
            "failed to parse index.json",
            err,
        )
    })?;
    let errors = serde_json::from_str(&errors_raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.errors_path.clone(),
            },
            "failed to parse index.errors.json",
            err,
        )
    })?;
    let meta: IndexMeta = serde_json::from_str(&meta_raw).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: files.meta_path.clone(),
            },
            "failed to parse index.meta.json",
            err,
        )
    })?;

    Ok(Some(IndexSnapshot {
        records,
        errors,
        meta,
    }))
}

#[cfg(test)]
mod tests {
    use super::{read_index_files, write_index_files};
    use kb_core::domain::{IndexMeta, IndexRecord, IndexSnapshot};
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
    fn roundtrip_index_files() {
        let dir = unique_test_dir("kb-storage-index-files");
        fs::create_dir_all(&dir).expect("create temp dir");
        let snapshot = IndexSnapshot {
            records: vec![IndexRecord {
                alias: "docs".to_string(),
                path: "./docs/a.md".to_string(),
                category: "raw".to_string(),
                signature: "s1".to_string(),
                present: true,
                excerpt: Some("hello".to_string()),
            }],
            errors: vec![],
            meta: IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 1,
            },
        };

        write_index_files(&dir, &snapshot).expect("write index files");
        let loaded = read_index_files(&dir).expect("read index files");
        assert_eq!(loaded, Some(snapshot));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
