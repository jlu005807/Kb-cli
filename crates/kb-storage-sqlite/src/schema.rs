//! Schema 定义与迁移。
//!
//! 当前 schema_version = 1。
//!
//! 表结构概览（按 memory-model.md 的分层语义命名）：
//!
//! - `kb_meta`：单行的 schema version + 创建/最近迁移时间，幂等迁移的锚点。
//! - `state_refs`：L0 声明对应的 ref 条目（alias / path / category / perms）。
//!   对应 `kb_core::StateSnapshot`，由 [`crate::SqliteStateStore`] 读写。
//! - `index_records`：L1 index 行集；`alias` 主键，`signature` 与 `present` 配合
//!   做 check/update 增量计算。预留，未接入。
//! - `knowledge_entries` + `knowledge_chunks`：L2 语义层。前者携带 summary /
//!   tags / key_concepts JSON 列，后者每行一个 chunk。预留。
//! - `search_hits`：L3 投影。预留。
//! - `graph_nodes` + `graph_edges`：L4 图谱。预留。
//! - `wiki_pages` + `wiki_manifest`：L5 wiki 派生层。预留。
//!
//! 所有预留表都已建好，避免后续接入时再写一次 `CREATE TABLE`；Rust 端的 store
//! 实现按需逐步补齐。

use kb_core::error::DomainError;

use crate::connection::SqliteConnection;

/// 当前 schema 版本号。每次 DDL 变更必须 +1 并补一段 `up_v{N}` 迁移。
///
/// 版本历史：
/// - v1：初始 schema，建好 L0–L5 所有数据行表（state_refs / index_records /
///   knowledge_entries+chunks / search_hits / graph_nodes+edges /
///   wiki_pages+manifest）以及 index_meta 单例表。
/// - v2：为 L2/L3/L4/L5 各补一个单例 meta 表
///   （knowledge_meta / search_meta / graph_meta / wiki_meta），
///   存放 schema_version + generated_at + 上游 signature；至此每个
///   snapshot 都能在 SQLite 里完整 round-trip。
pub const SCHEMA_VERSION: i64 = 2;

/// 确保数据库处于最新 schema。幂等：多次调用安全。
///
/// 迁移策略：读取 `kb_meta.schema_version`，若不存在或低于当前版本就顺序
/// 执行对应的 `up_v{N}` 函数。所有 DDL 在单个事务里完成，部分失败可回滚。
pub fn ensure_schema(conn: &SqliteConnection) -> Result<(), DomainError> {
    conn.with_mut(|c| {
        let tx = c
            .transaction()
            .map_err(|err| DomainError::io(format!("begin schema tx failed: {err}")))?;

        // kb_meta 是 schema 的"自举"表——必须先于一切版本判断创建出来。
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS kb_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                schema_version INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                last_migrated_at TEXT NOT NULL
            );",
        )
        .map_err(|err| DomainError::io(format!("create kb_meta failed: {err}")))?;

        let current: Option<i64> = tx
            .query_row("SELECT schema_version FROM kb_meta WHERE id = 1", [], |row| {
                row.get(0)
            })
            .ok();

        let starting = current.unwrap_or(0);
        if starting < 1 {
            up_v1(&tx)?;
        }
        if starting < 2 {
            up_v2(&tx)?;
        }

        if starting < SCHEMA_VERSION {
            let now = now_string();
            if current.is_none() {
                tx.execute(
                    "INSERT INTO kb_meta (id, schema_version, created_at, last_migrated_at) \
                     VALUES (1, ?1, ?2, ?2)",
                    rusqlite::params![SCHEMA_VERSION, now],
                )
                .map_err(|err| DomainError::io(format!("insert kb_meta failed: {err}")))?;
            } else {
                tx.execute(
                    "UPDATE kb_meta SET schema_version = ?1, last_migrated_at = ?2 WHERE id = 1",
                    rusqlite::params![SCHEMA_VERSION, now],
                )
                .map_err(|err| DomainError::io(format!("update kb_meta failed: {err}")))?;
            }
        }

        tx.commit()
            .map_err(|err| DomainError::io(format!("commit schema tx failed: {err}")))?;
        Ok(())
    })
}

fn up_v1(tx: &rusqlite::Transaction<'_>) -> Result<(), DomainError> {
    tx.execute_batch(
        r#"
-- L0 声明 / refs 层
CREATE TABLE IF NOT EXISTS state_refs (
    alias    TEXT PRIMARY KEY,
    path     TEXT NOT NULL,
    category TEXT NOT NULL,
    perms    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_state_refs_path ON state_refs(path);

-- L1 权威索引快照（预留）
CREATE TABLE IF NOT EXISTS index_records (
    alias     TEXT PRIMARY KEY,
    path      TEXT NOT NULL,
    category  TEXT NOT NULL,
    signature TEXT NOT NULL,
    present   INTEGER NOT NULL,
    excerpt   TEXT
);
CREATE TABLE IF NOT EXISTS index_errors (
    alias  TEXT NOT NULL,
    path   TEXT NOT NULL,
    reason TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS index_meta (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    full           INTEGER NOT NULL,
    baseline_size  INTEGER NOT NULL,
    tracked_size   INTEGER NOT NULL
);

-- L2 语义层（预留）
CREATE TABLE IF NOT EXISTS knowledge_entries (
    record_id        TEXT PRIMARY KEY,
    path             TEXT NOT NULL,
    input_signature  TEXT NOT NULL,
    status           TEXT NOT NULL,
    summary          TEXT NOT NULL,
    tags_json        TEXT NOT NULL,
    key_concepts_json TEXT NOT NULL,
    provider_kind    TEXT NOT NULL,
    provider_name    TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS knowledge_chunks (
    record_id        TEXT NOT NULL,
    chunk_id         TEXT NOT NULL,
    byte_start       INTEGER NOT NULL,
    byte_end         INTEGER NOT NULL,
    content_signature TEXT NOT NULL,
    summary          TEXT NOT NULL,
    preview          TEXT NOT NULL,
    PRIMARY KEY (record_id, chunk_id),
    FOREIGN KEY (record_id) REFERENCES knowledge_entries(record_id) ON DELETE CASCADE
);

-- L3 搜索投影（预留）
CREATE TABLE IF NOT EXISTS search_hits (
    record_id    TEXT NOT NULL,
    chunk_id     TEXT,
    score        REAL NOT NULL,
    matched_terms TEXT NOT NULL
);

-- L4 图谱（预留）
CREATE TABLE IF NOT EXISTS graph_nodes (
    id        TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    label     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS graph_edges (
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    edge_type   TEXT NOT NULL,
    weight      REAL NOT NULL,
    evidence_json TEXT NOT NULL,
    PRIMARY KEY (source_id, target_id, edge_type),
    FOREIGN KEY (source_id) REFERENCES graph_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);

-- L5 wiki 视图（预留）
CREATE TABLE IF NOT EXISTS wiki_pages (
    page_id    TEXT PRIMARY KEY,
    page_type  TEXT NOT NULL,
    markdown   TEXT NOT NULL,
    metadata_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS wiki_manifest (
    page_id   TEXT PRIMARY KEY,
    summary   TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#,
    )
    .map_err(|err| DomainError::io(format!("up_v1 DDL failed: {err}")))?;
    Ok(())
}

fn up_v2(tx: &rusqlite::Transaction<'_>) -> Result<(), DomainError> {
    // 每个 layer 单例 meta 表：把原本只能落在 snapshot JSON 头部的
    // schema_version / generated_at / 上游 signature 字段固化到 SQL 里。
    // id = 1 约束保证至多一行，使读取语义跟 fs 实现一致："要么有完整 meta，
    // 要么视作未生成"。
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS knowledge_meta (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version        INTEGER NOT NULL,
    generated_at          TEXT NOT NULL,
    base_index_signature  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS search_meta (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version            INTEGER NOT NULL,
    generated_at              TEXT NOT NULL,
    base_knowledge_signature  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS graph_meta (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version            INTEGER NOT NULL,
    generated_at              TEXT NOT NULL,
    base_knowledge_signature  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS wiki_meta (
    id                     INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version         INTEGER NOT NULL,
    generated_at           TEXT NOT NULL,
    base_graph_signature   TEXT NOT NULL
);
"#,
    )
    .map_err(|err| DomainError::io(format!("up_v2 DDL failed: {err}")))?;
    Ok(())
}

fn now_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_schema_is_idempotent() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("first migrate");
        ensure_schema(&conn).expect("second migrate should be no-op");
        conn.with(|c| {
            let v: i64 = c
                .query_row("SELECT schema_version FROM kb_meta", [], |row| row.get(0))
                .expect("read version");
            assert_eq!(v, SCHEMA_VERSION);
            Ok(())
        })
        .expect("with");
    }

    #[test]
    fn ensure_schema_creates_expected_tables() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        conn.with(|c| {
            for table in [
                "state_refs",
                "index_records",
                "knowledge_entries",
                "knowledge_chunks",
                "knowledge_meta",
                "search_hits",
                "search_meta",
                "graph_nodes",
                "graph_edges",
                "graph_meta",
                "wiki_pages",
                "wiki_manifest",
                "wiki_meta",
                "kb_meta",
            ] {
                let count: i64 = c
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                        rusqlite::params![table],
                        |row| row.get(0),
                    )
                    .expect("table lookup");
                assert_eq!(count, 1, "missing table: {table}");
            }
            Ok(())
        })
        .expect("with");
    }
}
