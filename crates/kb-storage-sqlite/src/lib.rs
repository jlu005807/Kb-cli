//! SQLite 持久化后端（#11）。
//!
//! 与 `kb-storage-fs` 的关系：两者都实现 `kb-core` 定义的存储 ports，可以
//! 在配置层选择一个挂入运行时。当前阶段：
//!
//! - 所有 store 均已实现：[`SqliteIndexStore`] / [`SqliteKnowledgeStore`] /
//!   [`SqliteSearchStore`] / [`SqliteGraphStore`] / [`SqliteWikiStore`] /
//!   [`SqliteEpisodeStore`]，并已在 CLI 侧通过 `StorageHandles` 按
//!   `storage.backend=sqlite` 配置挂入运行时；[`SqliteStateStore`] 也已实现，
//!   但 CLI 侧尚未挂接（L0 声明仍走 fs），是单独的迁移项。
//!
//! 选择 SQLite 的动机详见 `docs/architecture/memory-model.md` 与项目根 README：
//!
//! 1. JSON 全量重写在大 KB 下 O(N) IO 成本明显，SQLite 行级更新更划算。
//! 2. 跨表 JOIN（例如 graph 邻接 + knowledge 摘要）在 JSON 下需要把所有
//!    层文件全部读入内存才能做，SQLite 可以原生 SQL 描述。
//! 3. 单文件、零运维、bundle 进可执行文件，无外部依赖。
//!
//! 本 crate 严格遵循"adapters 只实现 ports"约束：业务规则仍住在 `kb-core`，
//! 这里只做行↔领域结构的 marshal/unmarshal + 事务管理。

pub mod connection;
pub mod episode_store;
pub mod graph_store;
pub mod index_store;
pub mod knowledge_store;
pub mod schema;
pub mod search_store;
pub mod state_store;
pub mod wiki_store;

pub use connection::{open_database, SqliteConnection};
pub use episode_store::SqliteEpisodeStore;
pub use graph_store::SqliteGraphStore;
pub use index_store::SqliteIndexStore;
pub use knowledge_store::SqliteKnowledgeStore;
pub use schema::{ensure_schema, SCHEMA_VERSION};
pub use search_store::SqliteSearchStore;
pub use state_store::SqliteStateStore;
pub use wiki_store::SqliteWikiStore;
