//! schema.version 读写与状态抽象。
//! 该模块只处理“版本文件本身”，不负责迁移策略判定（由 migration.rs 负责）。

use crate::atomic_write::atomic_write_bytes;
use crate::error::{PersistenceError, PersistenceErrorKind};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION_FILE: &str = "schema.version";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaStatus {
    /// 不存在 schema 文件（通常表示首次初始化）。
    Missing,
    /// 读取到 schema 版本字符串。
    Version(String),
}

/// 读取 schema 版本；文件缺失时返回 `SchemaStatus::Missing`。
pub fn read_schema_version(kb_dir: &Path) -> Result<SchemaStatus, PersistenceError> {
    let schema_path = schema_path(kb_dir);
    if !schema_path.exists() {
        return Ok(SchemaStatus::Missing);
    }

    let raw = fs::read_to_string(&schema_path).map_err(|err| {
        PersistenceError::with_source(
            PersistenceErrorKind::SchemaIo {
                schema_path: schema_path.clone(),
            },
            format!(
                "failed to read schema version file '{}'",
                schema_path.display()
            ),
            err,
        )
    })?;

    Ok(SchemaStatus::Version(raw.trim().to_string()))
}

/// 以原子写方式更新 schema 版本文件，避免半写入。
pub fn write_schema_version(kb_dir: &Path, version: &str) -> Result<(), PersistenceError> {
    let schema_path = schema_path(kb_dir);
    atomic_write_bytes(&schema_path, format!("{}\n", version).as_bytes()).map_err(|err| {
        PersistenceError::new(
            PersistenceErrorKind::SchemaIo { schema_path },
            format!("failed to write schema version: {}", err),
        )
    })
}

// 统一 schema 文件路径拼装，避免调用方散落硬编码。
fn schema_path(kb_dir: &Path) -> PathBuf {
    kb_dir.join(SCHEMA_VERSION_FILE)
}
