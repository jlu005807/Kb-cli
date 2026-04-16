use crate::error::{PersistenceError, PersistenceErrorKind};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct SingleWriterLock {
    lock_path: PathBuf,
    released: bool,
}

impl SingleWriterLock {
    pub fn release(mut self) -> Result<(), PersistenceError> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<(), PersistenceError> {
        if self.released {
            return Ok(());
        }

        fs::remove_file(&self.lock_path).map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::LockIo {
                    lock_path: self.lock_path.clone(),
                },
                format!("failed to remove lock file '{}'", self.lock_path.display()),
                err,
            )
        })?;

        self.released = true;
        Ok(())
    }
}

impl Drop for SingleWriterLock {
    // RAII 兜底：即使调用方忘记显式 release，也会在 drop 时尝试释放锁。
    fn drop(&mut self) {
        let _ = self.release_inner();
    }
}

#[derive(Debug, Clone)]
pub struct SingleWriterLockManager {
    lock_path: PathBuf,
}

impl SingleWriterLockManager {
    pub fn new(kb_dir: impl AsRef<Path>) -> Self {
        Self {
            lock_path: kb_dir.as_ref().join("writer.lock"),
        }
    }

    pub fn acquire(&self) -> Result<SingleWriterLock, PersistenceError> {
        if let Some(parent) = self.lock_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                PersistenceError::with_source(
                    PersistenceErrorKind::LockIo {
                        lock_path: self.lock_path.clone(),
                    },
                    format!("failed to create lock dir '{}'", parent.display()),
                    err,
                )
            })?;
        }

        // create_new=true 保证并发下只有一个持有者成功创建 lock 文件。
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.lock_path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    PersistenceError::with_source(
                        PersistenceErrorKind::LockAlreadyHeld {
                            lock_path: self.lock_path.clone(),
                        },
                        format!(
                            "single-writer lock already held at '{}'",
                            self.lock_path.display()
                        ),
                        err,
                    )
                } else {
                    PersistenceError::with_source(
                        PersistenceErrorKind::LockIo {
                            lock_path: self.lock_path.clone(),
                        },
                        format!("failed to create lock file '{}'", self.lock_path.display()),
                        err,
                    )
                }
            })?;

        file.write_all(b"kb-single-writer-lock\n")
            .and_then(|_| file.sync_all())
            .map_err(|err| {
                let _ = fs::remove_file(&self.lock_path);
                PersistenceError::with_source(
                    PersistenceErrorKind::LockIo {
                        lock_path: self.lock_path.clone(),
                    },
                    format!(
                        "failed to initialize lock file '{}'",
                        self.lock_path.display()
                    ),
                    err,
                )
            })?;

        Ok(SingleWriterLock {
            lock_path: self.lock_path.clone(),
            released: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SingleWriterLockManager;
    use crate::error::PersistenceErrorKind;
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
    fn second_acquisition_fails_deterministically_while_first_is_held() {
        let dir = unique_test_dir("kb-storage-fs-lock");
        fs::create_dir_all(&dir).expect("create temp dir");

        let manager = SingleWriterLockManager::new(&dir);
        let first = manager.acquire().expect("first lock should succeed");
        let second_err = manager
            .acquire()
            .expect_err("second lock must fail deterministically");

        match second_err.kind {
            PersistenceErrorKind::LockAlreadyHeld { .. } => {}
            other => panic!("unexpected lock error kind: {other:?}"),
        }

        first.release().expect("release lock");
        let third = manager
            .acquire()
            .expect("acquire should work after release");
        third.release().expect("release third lock");

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
