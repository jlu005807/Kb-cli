//! 存储后端选择层。
//!
//! `kb-cli` 历来直接构造 `Fs*Store`；引入 SQLite 后端（#11）后，需要在运行时
//! 根据 `.kb/config.json` 里的 `storage.backend` 字段决定走哪条链路：
//! - `"fs"` (默认，未配置时也走这条)：保留现有 JSON 文件持久化
//! - `"sqlite"`：使用 `kb-storage-sqlite` 提供的同名 store
//!
//! 设计准则：
//! 1. 切换点集中在本模块——handlers 只拿 `Box<dyn IndexStore>` 等抽象类型，
//!    不再 hard-code `FsIndexStore::new`，便于将来再插入更多后端。
//! 2. SQLite 后端共享 **同一份** `SqliteConnection`（内部 `Arc<Mutex>`）；
//!    所有 store 都 clone 该连接句柄，保证多 store 共用一个数据库文件 + 单写者
//!    互斥。FS 后端每个 store 拿独立 cwd 副本。
//! 3. 解析失败默认回退到 fs，并在 trace 字段里留痕，避免单字段拼错就拖垮启动。
//!
//! 当前 commit 只落地 `StorageHandles` factory；handlers 还在直接构造 `Fs*Store`，
//! 将在后续 commit 里逐步迁移到 `Box<dyn _>`。期间允许整个模块 `dead_code`，
//! 避免 CI `-D warnings` 因"尚未消费的桥接层"而失败。

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kb_core::ports::{GraphStore, IndexStore, KnowledgeStore, SearchStore, WikiStore};
use kb_storage_fs::{
    FsFileContentReader, FsGraphStore, FsIndexStore, FsKnowledgeStore, FsSearchStore, FsWikiStore,
};
use kb_storage_sqlite::{
    open_database, SqliteConnection, SqliteGraphStore, SqliteIndexStore, SqliteKnowledgeStore,
    SqliteSearchStore, SqliteWikiStore,
};

/// 用户在 `.kb/config.json` 里选择的存储后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// 默认：每层一个 JSON 文件 + wiki/ 目录。
    Fs,
    /// 单文件 SQLite，所有层共用一个 `.kb/store.sqlite3`。
    Sqlite,
}

impl StorageBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fs => "fs",
            Self::Sqlite => "sqlite",
        }
    }
}

/// 解析 `.kb/config.json::storage.backend`。出现以下任一情况返回 `Fs` 兜底：
/// - 文件不存在
/// - JSON 不合法
/// - 缺少 `storage` 节
/// - `backend` 非 "fs" / "sqlite"
///
/// 这种 best-effort 解析跟 LLM config 路径保持一致：CLI 的解析层永不因为
/// 一个字段拼错就完全拒绝启动。
pub fn resolve_storage_backend(cwd: &Path) -> StorageBackend {
    let config_path = cwd.join(".kb/config.json");
    let Ok(raw) = std::fs::read_to_string(&config_path) else {
        return StorageBackend::Fs;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return StorageBackend::Fs;
    };
    let Some(backend) = value
        .get("storage")
        .and_then(|s| s.get("backend"))
        .and_then(|b| b.as_str())
    else {
        return StorageBackend::Fs;
    };
    match backend.trim().to_lowercase().as_str() {
        "sqlite" => StorageBackend::Sqlite,
        _ => StorageBackend::Fs,
    }
}

/// SQLite 后端单文件路径，固定落在 `.kb/store.sqlite3`。
pub fn sqlite_database_path(cwd: &Path) -> PathBuf {
    cwd.join(".kb/store.sqlite3")
}

/// 一组按 backend 构造好的 L1+ store 句柄。
///
/// state 不在这里——state 仍走 CLI 自己的 [`crate::core_adapters::FsStateStore`]，
/// 因为它桥接的是 `crate::state::State` 而不是 `kb_core::StateSnapshot` 的纯
/// 文件序列化路径。SQLite 化 state 是单独的迁移项，不在本模块范围内。
pub struct StorageHandles {
    pub backend: StorageBackend,
    pub index: Box<dyn IndexStore + Send + Sync>,
    pub knowledge: Box<dyn KnowledgeStore + Send + Sync>,
    pub search: Box<dyn SearchStore + Send + Sync>,
    pub graph: Box<dyn GraphStore + Send + Sync>,
    pub wiki: Box<dyn WikiStore + Send + Sync>,
    /// L2 编译需要读原始文件；fs 后端就是文件系统读取，sqlite 后端也复用同一个
    /// reader（数据库里不存原文）。
    pub content_reader: FsFileContentReader,
    /// 仅在 SQLite 后端下出现，供需要直接读取 wiki 页面正文等"trait 之外"的
    /// 便利方法使用。FS 后端时为 `None`。
    pub sqlite_conn: Option<SqliteConnection>,
}

impl StorageHandles {
    /// 按 cwd 下 `.kb/config.json` 的 `storage.backend` 构造一组 store。
    /// SQLite 打开失败时会自动回退到 FS，并把错误冒泡——上游需要决定是 panic
    /// 还是降级警告。这里选择"失败即冒泡"，因为静默回退会让用户配错 storage
    /// 后端后毫无察觉地继续往 JSON 写入。
    pub fn open(cwd: &Path) -> Result<Self, kb_core::error::DomainError> {
        let backend = resolve_storage_backend(cwd);
        match backend {
            StorageBackend::Fs => Ok(Self::open_fs(cwd)),
            StorageBackend::Sqlite => Self::open_sqlite(cwd),
        }
    }

    fn open_fs(cwd: &Path) -> Self {
        Self {
            backend: StorageBackend::Fs,
            index: Box::new(FsIndexStore::new(cwd.to_path_buf())),
            knowledge: Box::new(FsKnowledgeStore::new(cwd.to_path_buf())),
            search: Box::new(FsSearchStore::new(cwd.to_path_buf())),
            graph: Box::new(FsGraphStore::new(cwd.to_path_buf())),
            wiki: Box::new(FsWikiStore::new(cwd.to_path_buf())),
            content_reader: FsFileContentReader::new(cwd.to_path_buf()),
            sqlite_conn: None,
        }
    }

    fn open_sqlite(cwd: &Path) -> Result<Self, kb_core::error::DomainError> {
        let db_path = sqlite_database_path(cwd);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                kb_core::error::DomainError::io(format!(
                    "create sqlite parent dir '{}' failed: {err}",
                    parent.display()
                ))
            })?;
        }
        let conn = open_database(&db_path)?;
        Ok(Self {
            backend: StorageBackend::Sqlite,
            index: Box::new(SqliteIndexStore::new(conn.clone())),
            knowledge: Box::new(SqliteKnowledgeStore::new(conn.clone())),
            search: Box::new(SqliteSearchStore::new(conn.clone())),
            graph: Box::new(SqliteGraphStore::new(conn.clone())),
            wiki: Box::new(SqliteWikiStore::new(conn.clone())),
            content_reader: FsFileContentReader::new(cwd.to_path_buf()),
            sqlite_conn: Some(conn),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kb_cli_storage_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_config(cwd: &Path, body: &str) {
        let kb = cwd.join(".kb");
        std::fs::create_dir_all(&kb).expect("mkdir .kb");
        std::fs::write(kb.join("config.json"), body).expect("write config");
    }

    #[test]
    fn missing_config_defaults_to_fs() {
        let tmp = unique_temp_dir("missing-config");
        assert_eq!(resolve_storage_backend(&tmp), StorageBackend::Fs);
    }

    #[test]
    fn explicit_sqlite_backend_is_parsed() {
        let tmp = unique_temp_dir("explicit-sqlite");
        write_config(&tmp, r#"{"storage":{"backend":"sqlite"}}"#);
        assert_eq!(resolve_storage_backend(&tmp), StorageBackend::Sqlite);
    }

    #[test]
    fn explicit_fs_backend_is_parsed() {
        let tmp = unique_temp_dir("explicit-fs");
        write_config(&tmp, r#"{"storage":{"backend":"fs"}}"#);
        assert_eq!(resolve_storage_backend(&tmp), StorageBackend::Fs);
    }

    #[test]
    fn unknown_backend_falls_back_to_fs() {
        let tmp = unique_temp_dir("unknown-backend");
        write_config(&tmp, r#"{"storage":{"backend":"hbase"}}"#);
        assert_eq!(resolve_storage_backend(&tmp), StorageBackend::Fs);
    }

    #[test]
    fn malformed_json_falls_back_to_fs() {
        let tmp = unique_temp_dir("malformed-json");
        write_config(&tmp, "not json");
        assert_eq!(resolve_storage_backend(&tmp), StorageBackend::Fs);
    }

    #[test]
    fn fs_handles_open_without_filesystem_state() {
        let tmp = unique_temp_dir("fs-handles");
        let handles = StorageHandles::open(&tmp).expect("open fs handles");
        assert_eq!(handles.backend, StorageBackend::Fs);
        assert!(handles.sqlite_conn.is_none());
    }

    #[test]
    fn sqlite_handles_create_database_file() {
        let tmp = unique_temp_dir("sqlite-handles");
        write_config(&tmp, r#"{"storage":{"backend":"sqlite"}}"#);
        let handles = StorageHandles::open(&tmp).expect("open sqlite handles");
        assert_eq!(handles.backend, StorageBackend::Sqlite);
        assert!(handles.sqlite_conn.is_some());
        assert!(sqlite_database_path(&tmp).exists());
    }
}
