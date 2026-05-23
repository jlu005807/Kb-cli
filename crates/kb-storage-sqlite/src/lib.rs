//! SQLite 持久化后端（#11）。
//!
//! 与 `kb-storage-fs` 的关系：两者都实现 `kb-core` 定义的存储 ports，可以
//! 在配置层选择一个挂入运行时。当前阶段：
//!
//! - `state` 表已落地，对应 [`SqliteStateStore`]，作为 SQLite 后端的"第一站"。
//!   其它 store（index/knowledge/search/graph/wiki）的表结构已在初始化时
//!   预先建好，但 Rust 端实现尚未补齐——参见 schema 内的注释。
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
pub mod schema;
pub mod state_store;

pub use connection::{open_database, SqliteConnection};
pub use schema::{ensure_schema, SCHEMA_VERSION};
pub use state_store::SqliteStateStore;
