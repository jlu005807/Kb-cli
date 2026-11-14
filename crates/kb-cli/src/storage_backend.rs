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
//! 3. 未配置时使用 FS；显式配置错误必须报告，避免静默切后端造成数据分叉。

use std::path::{Path, PathBuf};

use kb_core::ports::{
    EpisodeStore, GraphStore, IndexStore, KnowledgeStore, SearchStore, WikiStore,
};
use kb_storage_fs::{
    FsEpisodeStore, FsFileContentReader, FsGraphStore, FsIndexStore, FsKnowledgeStore,
    FsSearchStore, FsWikiStore,
};
use kb_storage_sqlite::{
    open_database, SqliteEpisodeStore, SqliteGraphStore, SqliteIndexStore, SqliteKnowledgeStore,
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

/// 缺少配置或 storage/backend 时默认 FS；显式非法配置不允许切换后端。
pub fn resolve_storage_backend(cwd: &Path) -> Result<StorageBackend, kb_core::DomainError> {
    use kb_core::DomainError;
    let config_path = cwd.join(".kb/config.json");
    let raw = match std::fs::read_to_string(&config_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(StorageBackend::Fs),
        Err(err) => {
            return Err(DomainError::io(format!(
                "read '{}': {err}",
                config_path.display()
            )))
        }
    };
    let value: serde_json::Value =
        serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(&raw)).map_err(|err| {
            DomainError::config(format!("parse '{}': {err}", config_path.display()))
        })?;
    let object = value
        .as_object()
        .ok_or_else(|| DomainError::config("project config must be an object"))?;
    let Some(storage) = object.get("storage") else {
        return Ok(StorageBackend::Fs);
    };
    let storage = storage
        .as_object()
        .ok_or_else(|| DomainError::config("storage must be an object"))?;
    let Some(backend) = storage.get("backend") else {
        return Ok(StorageBackend::Fs);
    };
    let backend = backend
        .as_str()
        .ok_or_else(|| DomainError::config("storage.backend must be a string"))?;
    match backend.trim().to_lowercase().as_str() {
        "sqlite" => Ok(StorageBackend::Sqlite),
        "fs" => Ok(StorageBackend::Fs),
        other => Err(DomainError::config(format!(
            "unknown storage backend: {other}"
        ))),
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
    pub index: Box<dyn IndexStore + Send + Sync>,
    pub knowledge: Box<dyn KnowledgeStore + Send + Sync>,
    pub search: Box<dyn SearchStore + Send + Sync>,
    pub graph: Box<dyn GraphStore + Send + Sync>,
    pub wiki: Box<dyn WikiStore + Send + Sync>,
    /// L0 Episodic 层：append-only 交互时间线。两个后端都接入——fs 写
    /// `.kb/episodes.jsonl`，sqlite 写 `episodes` 表。供 agent runtime 写入
    /// （ask / remember / correct）与 consolidation 读取。
    pub episode: Box<dyn EpisodeStore + Send + Sync>,
    /// L2 编译需要读原始文件；fs 后端就是文件系统读取，sqlite 后端也复用同一个
    /// reader（数据库里不存原文）。
    pub content_reader: FsFileContentReader,
}

impl StorageHandles {
    /// 按 cwd 下 `.kb/config.json` 的 `storage.backend` 构造一组 store。
    /// 配置或 SQLite 打开失败直接报告，禁止静默切换到另一后端写入。
    pub fn open(cwd: &Path) -> Result<Self, kb_core::error::DomainError> {
        let backend = resolve_storage_backend(cwd)?;
        match backend {
            StorageBackend::Fs => Ok(Self::open_fs(cwd)),
            StorageBackend::Sqlite => Self::open_sqlite(cwd),
        }
    }

    fn open_fs(cwd: &Path) -> Self {
        Self {
            index: Box::new(FsIndexStore::new(cwd.to_path_buf())),
            knowledge: Box::new(FsKnowledgeStore::new(cwd.to_path_buf())),
            search: Box::new(FsSearchStore::new(cwd.to_path_buf())),
            graph: Box::new(FsGraphStore::new(cwd.to_path_buf())),
            wiki: Box::new(FsWikiStore::new(cwd.to_path_buf())),
            episode: Box::new(FsEpisodeStore::new(cwd.to_path_buf())),
            content_reader: FsFileContentReader::new(cwd.to_path_buf()),
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
            index: Box::new(SqliteIndexStore::new(conn.clone())),
            knowledge: Box::new(SqliteKnowledgeStore::new(conn.clone())),
            search: Box::new(SqliteSearchStore::new(conn.clone())),
            graph: Box::new(SqliteGraphStore::new(conn.clone())),
            wiki: Box::new(SqliteWikiStore::new(conn.clone())),
            episode: Box::new(SqliteEpisodeStore::new(conn.clone())),
            content_reader: FsFileContentReader::new(cwd.to_path_buf()),
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
        assert_eq!(resolve_storage_backend(&tmp).unwrap(), StorageBackend::Fs);
        for body in ["{}", r#"{"storage":{}}"#] {
            write_config(&tmp, body);
            assert_eq!(resolve_storage_backend(&tmp).unwrap(), StorageBackend::Fs);
        }
    }

    #[test]
    fn explicit_sqlite_backend_is_parsed() {
        let tmp = unique_temp_dir("explicit-sqlite");
        for prefix in ["", "\u{feff}"] {
            write_config(
                &tmp,
                &format!("{prefix}{{\"storage\":{{\"backend\":\"sqlite\"}}}}"),
            );
            assert_eq!(
                resolve_storage_backend(&tmp).unwrap(),
                StorageBackend::Sqlite
            );
        }
    }

    #[test]
    fn explicit_fs_backend_is_parsed() {
        let tmp = unique_temp_dir("explicit-fs");
        write_config(&tmp, r#"{"storage":{"backend":"fs"}}"#);
        assert_eq!(resolve_storage_backend(&tmp).unwrap(), StorageBackend::Fs);
    }

    #[test]
    fn invalid_backend_config_is_rejected() {
        for body in [
            r#"{"storage":{"backend":"hbase"}}"#,
            r#"{"storage":{"backend":null}}"#,
            r#"{"storage":[]}"#,
            "[]",
            "null",
        ] {
            let tmp = unique_temp_dir("invalid-backend");
            write_config(&tmp, body);
            assert!(
                matches!(
                    StorageHandles::open(&tmp),
                    Err(kb_core::DomainError::Config(_))
                ),
                "{body}"
            );
        }
    }

    #[test]
    fn malformed_json_is_rejected() {
        let tmp = unique_temp_dir("malformed-json");
        write_config(&tmp, "not json");
        assert!(matches!(
            StorageHandles::open(&tmp),
            Err(kb_core::DomainError::Config(_))
        ));
    }

    #[test]
    fn fs_handles_open_without_filesystem_state() {
        let tmp = unique_temp_dir("fs-handles");
        let _handles = StorageHandles::open(&tmp).expect("open fs handles");
        assert!(!sqlite_database_path(&tmp).exists());
    }

    #[test]
    fn sqlite_handles_create_database_file() {
        let tmp = unique_temp_dir("sqlite-handles");
        write_config(&tmp, r#"{"storage":{"backend":"sqlite"}}"#);
        let _handles = StorageHandles::open(&tmp).expect("open sqlite handles");
        assert!(sqlite_database_path(&tmp).exists());
    }
}
