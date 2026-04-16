use crate::error::{PersistenceError, PersistenceErrorKind};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// 原子写协议：写临时文件 -> flush -> rename 覆盖目标。
// 目标是避免进程中断后留下半写入文件。
pub fn atomic_write_bytes(target_path: &Path, bytes: &[u8]) -> Result<(), PersistenceError> {
    let parent = target_path
        .parent()
        .ok_or_else(|| {
            PersistenceError::new(
                PersistenceErrorKind::AtomicWriteIo {
                    target_path: target_path.to_path_buf(),
                },
                format!("target path '{}' has no parent", target_path.display()),
            )
        })?
        .to_path_buf();

    fs::create_dir_all(&parent).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::AtomicWriteIo {
                target_path: target_path.to_path_buf(),
            },
            format!("failed to create parent directory '{}'", parent.display()),
            err,
        )
    })?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = parent.join(format!(
        ".{}.{}.tmp",
        target_path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("target"),
        nonce
    ));

    let write_result = (|| -> Result<(), PersistenceError> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)
            .map_err(|err| {
                PersistenceError::with_source(
                    PersistenceErrorKind::AtomicWriteIo {
                        target_path: target_path.to_path_buf(),
                    },
                    format!("failed to create temp file '{}'", tmp_path.display()),
                    err,
                )
            })?;

        file.write_all(bytes).map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::AtomicWriteIo {
                    target_path: target_path.to_path_buf(),
                },
                format!("failed to write temp file '{}'", tmp_path.display()),
                err,
            )
        })?;

        file.sync_all().map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::AtomicWriteIo {
                    target_path: target_path.to_path_buf(),
                },
                format!("failed to flush temp file '{}'", tmp_path.display()),
                err,
            )
        })?;

        fs::rename(&tmp_path, target_path).map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::AtomicWriteIo {
                    target_path: target_path.to_path_buf(),
                },
                format!(
                    "failed to atomically rename '{}' -> '{}'",
                    tmp_path.display(),
                    target_path.display()
                ),
                err,
            )
        })?;

        Ok(())
    })();

    // 失败时尽力清理临时文件，避免目录污染。
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    write_result
}

#[cfg(test)]
mod tests {
    use super::atomic_write_bytes;
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
    fn atomic_write_overwrites_target_with_new_content() {
        let dir = unique_test_dir("kb-storage-fs-atomic");
        fs::create_dir_all(&dir).expect("create temp dir");

        let target = dir.join("index.json");
        atomic_write_bytes(&target, br#"{"v":1}"#).expect("first write should succeed");
        atomic_write_bytes(&target, br#"{"v":2}"#).expect("second write should succeed");

        let content = fs::read_to_string(&target).expect("must read target file");
        assert_eq!(content, "{\"v\":2}");

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
