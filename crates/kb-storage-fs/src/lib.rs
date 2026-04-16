//! `kb-storage-fs` 提供文件系统持久化适配：
//! - 原子写
//! - 单写者锁
//! - schema 版本管理与迁移
//! - index 三文件读写

mod atomic_write;
mod error;
mod index_files;
mod lock_fs;
mod migration;
mod schema;

pub use atomic_write::atomic_write_bytes;
pub use error::{PersistenceError, PersistenceErrorKind};
pub use index_files::{read_index_files, write_index_files, IndexFiles};
pub use lock_fs::{SingleWriterLock, SingleWriterLockManager};
pub use migration::{
    ensure_schema, plan_migration, MigrationOutcome, MigrationPlan, MigrationStep,
    SCHEMA_VERSION_CURRENT,
};
pub use schema::{read_schema_version, write_schema_version, SchemaStatus};
