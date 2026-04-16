//! 持久化层错误模型。
//! 设计目标：保留结构化错误种类（kind）与人类可读 message，
//! 同时可选挂接底层 source error 便于排障。

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub struct PersistenceError {
    /// 结构化错误种类，供上层逻辑匹配处理。
    pub kind: PersistenceErrorKind,
    /// 人类可读错误消息。
    pub message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl PersistenceError {
    /// 构造不带底层 source 的错误。
    pub fn new(kind: PersistenceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    /// 构造并携带底层 source 错误，保留原始异常链。
    pub fn with_source(
        kind: PersistenceErrorKind,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|src| src.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceErrorKind {
    /// writer.lock 已被其他进程/线程持有。
    LockAlreadyHeld {
        lock_path: PathBuf,
    },
    LockIo {
        lock_path: PathBuf,
    },
    LockState {
        lock_path: PathBuf,
    },
    AtomicWriteIo {
        target_path: PathBuf,
    },
    SchemaIo {
        schema_path: PathBuf,
    },
    SchemaUnsupported {
        found: String,
        current: String,
        schema_path: PathBuf,
    },
    MigrationFailed {
        from: String,
        to: String,
    },
}
