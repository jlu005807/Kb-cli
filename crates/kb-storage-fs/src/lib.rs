//! `kb-storage-fs` 提供文件系统持久化适配：
//! - 原子写
//! - 单写者锁
//! - schema 版本管理与迁移
//! - index 三文件读写

mod atomic_write;
mod error;
mod graph_files;
mod index_files;
mod knowledge_files;
mod lock_fs;
mod migration;
mod search_files;
mod schema;
mod search_files;
mod wiki_view_files;

pub use atomic_write::atomic_write_bytes;
pub use error::{PersistenceError, PersistenceErrorKind};
pub use graph_files::{read_graph_file, write_graph_file, GraphFiles};
pub use index_files::{read_index_files, write_index_files, IndexFiles};
pub use knowledge_files::{read_knowledge_file, write_knowledge_file, KnowledgeFiles};
pub use lock_fs::{SingleWriterLock, SingleWriterLockManager};
pub use migration::{
    ensure_schema, plan_migration, MigrationOutcome, MigrationPlan, MigrationStep,
    SCHEMA_VERSION_CURRENT,
};
pub use search_files::{read_search_file, write_search_file, SearchFiles};
pub use schema::{read_schema_version, write_schema_version, SchemaStatus};
pub use search_files::{read_search_file, write_search_file, SearchFiles};
pub use wiki_view_files::{read_wiki_manifest_file, write_wiki_view_files, WikiViewFiles};
