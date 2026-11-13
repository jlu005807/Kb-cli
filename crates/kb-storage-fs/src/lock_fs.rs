use crate::error::{PersistenceError, PersistenceErrorKind};
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct SingleWriterLock {
    file: File,
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

        self.file.unlock().map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::LockIo {
                    lock_path: self.lock_path.clone(),
                },
                format!("failed to unlock '{}'", self.lock_path.display()),
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

    pub fn acquire(&self) -> Result<Option<SingleWriterLock>, PersistenceError> {
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

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(|err| {
                PersistenceError::with_source(
                    PersistenceErrorKind::LockIo {
                        lock_path: self.lock_path.clone(),
                    },
                    format!("failed to open lock file '{}'", self.lock_path.display()),
                    err,
                )
            })?;

        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::Error(err))
                if err.kind() == std::io::ErrorKind::Unsupported =>
            {
                return Ok(None);
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(PersistenceError::with_source(
                    PersistenceErrorKind::LockAlreadyHeld {
                        lock_path: self.lock_path.clone(),
                    },
                    format!(
                        "single-writer lock already held at '{}'",
                        self.lock_path.display()
                    ),
                    std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        "writer lock is held by another process",
                    ),
                ));
            }
            Err(std::fs::TryLockError::Error(err)) => {
                return Err(PersistenceError::with_source(
                    PersistenceErrorKind::LockIo {
                        lock_path: self.lock_path.clone(),
                    },
                    format!("failed to acquire lock file '{}'", self.lock_path.display()),
                    err,
                ));
            }
        }

        let mut lock = SingleWriterLock {
            file,
            lock_path: self.lock_path.clone(),
            released: false,
        };
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let diagnostic = format!("pid={} started={}\n", std::process::id(), started);
        let write_result = lock
            .file
            .set_len(0)
            .and_then(|_| lock.file.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|_| lock.file.write_all(diagnostic.as_bytes()))
            .and_then(|_| lock.file.sync_all());
        if let Err(err) = write_result {
            return Err(PersistenceError::with_source(
                PersistenceErrorKind::LockIo {
                    lock_path: self.lock_path.clone(),
                },
                format!(
                    "failed to initialize lock file '{}'",
                    self.lock_path.display()
                ),
                err,
            ));
        }
        // Windows may deny reading a locked file, so keep a readable diagnostic copy.
        let owner_path = self.lock_path.with_file_name("writer.lock.owner.lock");
        crate::atomic_write_bytes(&owner_path, diagnostic.as_bytes()).map_err(|err| {
            PersistenceError::with_source(
                PersistenceErrorKind::LockIo {
                    lock_path: owner_path.clone(),
                },
                format!("failed to write lock owner file '{}'", owner_path.display()),
                std::io::Error::other(err.to_string()),
            )
        })?;

        Ok(Some(lock))
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
        let first = manager
            .acquire()
            .expect("first acquire should succeed")
            .expect("file locks should be supported");
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
            .expect("acquire after release should work")
            .expect("file locks should be supported");
        third.release().expect("release third lock");

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn drop_releases_os_lock_but_keeps_lock_file_for_reuse() {
        let dir = unique_test_dir("kb-storage-fs-lock-drop");
        fs::create_dir_all(&dir).expect("create temp dir");
        let manager = SingleWriterLockManager::new(&dir);
        let lock_path = dir.join("writer.lock");

        let first = manager
            .acquire()
            .expect("first acquire should succeed")
            .expect("file locks should be supported");
        drop(first);

        assert!(lock_path.is_file(), "lock file should remain on disk");
        let second = manager
            .acquire()
            .expect("acquire after drop should succeed")
            .expect("file locks should be supported");
        second.release().expect("release second lock");

        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn stale_lock_file_does_not_prevent_acquisition() {
        let dir = unique_test_dir("kb-storage-fs-lock-stale");
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("writer.lock"), "pid=old started=1\n").expect("write stale lock file");
        let manager = SingleWriterLockManager::new(&dir);

        let lock = manager
            .acquire()
            .expect("stale file has no held OS lock")
            .expect("file locks should be supported");
        lock.release().expect("release lock");
        let contents = std::fs::read_to_string(dir.join("writer.lock")).unwrap();
        assert!(contents.starts_with(&format!("pid={} started=", std::process::id())));
        fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
