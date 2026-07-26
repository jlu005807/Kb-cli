//! Schema 定义与迁移。
//!
//! 当前 schema_version = 4（版本历史见 [`SCHEMA_VERSION`]）。
//!
//! 表结构概览（按 memory-model.md 的分层语义命名）：
//!
//! - `kb_meta`：单行的 schema version + 创建/最近迁移时间，幂等迁移的锚点。
//! - `state_refs`：L0 声明对应的 ref 条目（alias / path / category / perms）。
//!   对应 `kb_core::StateSnapshot`，由 [`crate::SqliteStateStore`] 读写
//!   （store 已实现，但 CLI 侧尚未挂接，L0 声明仍走 fs）。
//! - `episodes`：L0 Episodic 交互时间线，由 [`crate::SqliteEpisodeStore`] 读写。
//! - `index_records`：L1 index 行集；`alias` 主键，`signature` 与 `present` 配合
//!   做 check/update 增量计算。由 [`crate::SqliteIndexStore`] 读写。
//! - `knowledge_entries` + `knowledge_chunks`：L2 语义层。前者携带 summary /
//!   tags / key_concepts JSON 列，v4 起还带 confidence / source / timestamps /
//!   provenance 记忆列；后者每行一个 chunk。由 [`crate::SqliteKnowledgeStore`] 读写。
//! - `search_hits`：L3 投影。由 [`crate::SqliteSearchStore`] 读写。
//! - `graph_nodes` + `graph_edges`：L4 图谱。由 [`crate::SqliteGraphStore`] 读写。
//! - `wiki_pages` + `wiki_manifest`：L5 wiki 派生层。由 [`crate::SqliteWikiStore`] 读写。
//!
//! 各层 store（index/knowledge/search/graph/wiki/episode）均已实现，并在 CLI 侧
//! 通过 `StorageHandles` 接入 sqlite 后端；仅 state 层的 store 实现完毕但未挂接。

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
/// - v3：新增 L0 Episodic 层 `episodes` 表（append-only 交互时间线），
///   对应 `kb_core::Episode` / [`crate::SqliteEpisodeStore`]。refs 序列化为
///   JSON 数组列；按 (timestamp, id) 排序提供时间升序读取。
/// - v4：`knowledge_entries` 补齐 Phase 0 引入的记忆字段列（confidence /
///   source_json / created_at / last_verified_at / provenance_json），
///   SQLite 后端自此与 fs 后端一样完整 round-trip `KnowledgeEntry`。
///   默认值兼容 pre-v4 旧行：source_json='' 表示旧行（读端回退默认
///   source），时间戳 0 表示无时间锚 / 从未验证。
/// - v5：重建 `knowledge_entries`，把 last_verified_at 改为可空列——
///   NULL 表示从未验证（None），任何数值（含 0）精确还原为 Some，
///   与 fs 后端的 serde round-trip 语义对齐；旧数据的 0（v4 编码的
///   "从未验证"）转换为 NULL。历史 up_vN 定义保持不变，确保任何
///   版本的库沿同一迁移链收敛到同一 schema。
pub const SCHEMA_VERSION: i64 = 5;

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
        if starting < 3 {
            up_v3(&tx)?;
        }
        if starting < 4 {
            up_v4(&tx)?;
        }
        if starting < 5 {
            up_v5(&tx)?;
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

fn up_v3(tx: &rusqlite::Transaction<'_>) -> Result<(), DomainError> {
    // L0 Episodic 层：append-only 交互时间线。
    // - id 主键保证同一 store 内 episode 唯一（与 fs JSONL 语义一致）。
    // - refs_json 把 Vec<String> chunk 引用序列化为 JSON 数组列。
    // - 复合索引 (timestamp, id) 支撑"按时间升序读取 + limit 取最近 N 条"。
    // - agent_id / session_id / kind 上各建索引，支撑 EpisodeFilter 的等值过滤。
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS episodes (
    id          TEXT PRIMARY KEY,
    timestamp   INTEGER NOT NULL,
    agent_id    TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    content     TEXT NOT NULL,
    refs_json   TEXT NOT NULL,
    confidence  REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_episodes_time ON episodes(timestamp, id);
CREATE INDEX IF NOT EXISTS idx_episodes_agent ON episodes(agent_id);
CREATE INDEX IF NOT EXISTS idx_episodes_session ON episodes(session_id);
CREATE INDEX IF NOT EXISTS idx_episodes_kind ON episodes(kind);
"#,
    )
    .map_err(|err| DomainError::io(format!("up_v3 DDL failed: {err}")))?;
    Ok(())
}

fn up_v4(tx: &rusqlite::Transaction<'_>) -> Result<(), DomainError> {
    // Phase 0 记忆字段落库：knowledge_entries 增加 confidence / source /
    // timestamps / provenance 列。ensure_schema 的版本门槛保证 up_v4 至多
    // 执行一次，因此裸 ALTER TABLE ADD COLUMN 是安全的（不会重复加列）。
    //
    // 默认值与读端回退语义配套（v4 时点的历史定义，last_verified_at 的
    // 可空化在 up_v5 完成——历史迁移步骤不可原地改写，否则已迁到 v4 的库
    // 会与新库 schema 分叉）：
    // - source_json=''      → pre-v4 旧行，读端回退默认 source（FromFile）
    // - created_at=0        → 历史数据没有时间锚（与 serde default 一致）
    // - last_verified_at=0  → v4 编码的"从未验证"（up_v5 转换为 NULL）
    // 时间戳沿用 episodes.timestamp 的约定：unix nanos 存 i64。
    tx.execute_batch(
        r#"
ALTER TABLE knowledge_entries ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0;
ALTER TABLE knowledge_entries ADD COLUMN source_json TEXT NOT NULL DEFAULT '';
ALTER TABLE knowledge_entries ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_entries ADD COLUMN last_verified_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_entries ADD COLUMN provenance_json TEXT NOT NULL DEFAULT '[]';
"#,
    )
    .map_err(|err| DomainError::io(format!("up_v4 DDL failed: {err}")))?;
    Ok(())
}

fn up_v5(tx: &rusqlite::Transaction<'_>) -> Result<(), DomainError> {
    // SQLite 无法直接去掉列的 NOT NULL 约束，整表重建：
    // 1. 建新表（last_verified_at 可空），拷贝数据并把 v4 编码的 0（从未
    //    验证）转换为 NULL。窗口期内新代码写入的真实 Some(0)（时钟异常才
    //    会产生）会被一并转为 None——可达性趋零，接受。
    // 2. 先备份并删除子表 knowledge_chunks 再删旧父表，避免 RENAME 把
    //    子表外键改绑到旧表名、DROP 父表级联清空 chunks。
    // 3. 重命名新表后按原 DDL 重建 chunks 并回填。
    tx.execute_batch(
        r#"
CREATE TABLE knowledge_entries_v5 (
    record_id        TEXT PRIMARY KEY,
    path             TEXT NOT NULL,
    input_signature  TEXT NOT NULL,
    status           TEXT NOT NULL,
    summary          TEXT NOT NULL,
    tags_json        TEXT NOT NULL,
    key_concepts_json TEXT NOT NULL,
    provider_kind    TEXT NOT NULL,
    provider_name    TEXT NOT NULL,
    confidence       REAL NOT NULL DEFAULT 1.0,
    source_json      TEXT NOT NULL DEFAULT '',
    created_at       INTEGER NOT NULL DEFAULT 0,
    last_verified_at INTEGER,
    provenance_json  TEXT NOT NULL DEFAULT '[]'
);
INSERT INTO knowledge_entries_v5
    SELECT record_id, path, input_signature, status, summary, tags_json,
           key_concepts_json, provider_kind, provider_name, confidence,
           source_json, created_at,
           CASE WHEN last_verified_at = 0 THEN NULL ELSE last_verified_at END,
           provenance_json
    FROM knowledge_entries;
CREATE TABLE knowledge_chunks_backup AS SELECT * FROM knowledge_chunks;
DROP TABLE knowledge_chunks;
DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_v5 RENAME TO knowledge_entries;
CREATE TABLE knowledge_chunks (
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
INSERT INTO knowledge_chunks SELECT * FROM knowledge_chunks_backup;
DROP TABLE knowledge_chunks_backup;
"#,
    )
    .map_err(|err| DomainError::io(format!("up_v5 DDL failed: {err}")))?;
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

    /// PRAGMA table_info 拿到的列名集合。
    fn table_columns(c: &rusqlite::Connection, table: &str) -> Vec<String> {
        let mut stmt = c
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare pragma");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query pragma")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns")
    }

    #[test]
    fn ensure_schema_is_idempotent() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("first migrate");
        ensure_schema(&conn).expect("second migrate should be no-op");
        // 第三次也应无副作用——重点是 up_v4 的 ALTER TABLE 不会被重复执行
        // （否则 duplicate column 直接报错）。
        ensure_schema(&conn).expect("third migrate should be no-op");
        conn.with(|c| {
            let v: i64 = c
                .query_row("SELECT schema_version FROM kb_meta", [], |row| row.get(0))
                .expect("read version");
            assert_eq!(v, SCHEMA_VERSION);
            // 反复迁移后 v4 列仍然齐备且未重复。
            let columns = table_columns(c, "knowledge_entries");
            for col in [
                "confidence",
                "source_json",
                "created_at",
                "last_verified_at",
                "provenance_json",
            ] {
                assert_eq!(
                    columns.iter().filter(|name| name.as_str() == col).count(),
                    1,
                    "expected exactly one '{col}' column, got {columns:?}"
                );
            }
            Ok(())
        })
        .expect("with");
    }

    #[test]
    fn fresh_database_migrates_to_latest_with_memory_columns() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        conn.with(|c| {
            let v: i64 = c
                .query_row("SELECT schema_version FROM kb_meta WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .expect("read version");
            assert_eq!(v, SCHEMA_VERSION, "fresh db should land on latest schema");

            let columns = table_columns(c, "knowledge_entries");
            for col in [
                "confidence",
                "source_json",
                "created_at",
                "last_verified_at",
                "provenance_json",
            ] {
                assert!(
                    columns.iter().any(|name| name == col),
                    "missing memory column '{col}' in knowledge_entries: {columns:?}"
                );
            }
            // v5：last_verified_at 必须可空（notnull=0）
            let notnull: i64 = c
                .query_row(
                    "SELECT [notnull] FROM pragma_table_info('knowledge_entries') WHERE name = 'last_verified_at'",
                    [],
                    |row| row.get(0),
                )
                .expect("pragma notnull");
            assert_eq!(notnull, 0, "last_verified_at must be nullable after v5");
            Ok(())
        })
        .expect("with");
    }

    /// 模拟一个已迁到"旧 v4"（last_verified_at 仍为 NOT NULL DEFAULT 0）的库：
    /// up_v5 必须重建该表为可空列，把 0 转换为 NULL，并保留 chunks 数据。
    #[test]
    fn legacy_v4_database_is_rebuilt_by_v5() {
        let conn = SqliteConnection::in_memory().expect("open");
        // 手工执行到旧 v4 状态（up_v1..up_v4 均为历史定义，不含 up_v5）。
        conn.with_mut(|c| {
            let tx = c.transaction().expect("tx");
            tx.execute_batch(
                "CREATE TABLE kb_meta (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    schema_version INTEGER NOT NULL,
                    created_at TEXT NOT NULL,
                    last_migrated_at TEXT NOT NULL
                );",
            )
            .expect("kb_meta");
            up_v1(&tx).expect("v1");
            up_v2(&tx).expect("v2");
            up_v3(&tx).expect("v3");
            up_v4(&tx).expect("v4");
            tx.execute(
                "INSERT INTO kb_meta (id, schema_version, created_at, last_migrated_at) \
                 VALUES (1, 4, 'unix:0', 'unix:0')",
                [],
            )
            .expect("stamp v4");
            // 旧编码数据：0 = 从未验证；123 = 真实时间戳；附带一条 chunk。
            tx.execute(
                "INSERT INTO knowledge_entries \
                 (record_id, path, input_signature, status, summary, tags_json, \
                  key_concepts_json, provider_kind, provider_name, last_verified_at) \
                 VALUES ('a', './a.md', 'sig', 'active', 's', '[]', '[]', 'rule', 'r', 0)",
                [],
            )
            .expect("legacy row 0");
            tx.execute(
                "INSERT INTO knowledge_entries \
                 (record_id, path, input_signature, status, summary, tags_json, \
                  key_concepts_json, provider_kind, provider_name, last_verified_at) \
                 VALUES ('b', './b.md', 'sig', 'active', 's', '[]', '[]', 'rule', 'r', 123)",
                [],
            )
            .expect("legacy row 123");
            tx.execute(
                "INSERT INTO knowledge_chunks \
                 (record_id, chunk_id, byte_start, byte_end, content_signature, summary, preview) \
                 VALUES ('a', 'a:0001', 0, 1, 'sig', 's', 'p')",
                [],
            )
            .expect("legacy chunk");
            tx.commit().expect("commit");
            Ok(())
        })
        .expect("setup legacy v4");

        ensure_schema(&conn).expect("migrate to v5");

        conn.with(|c| {
            let notnull: i64 = c
                .query_row(
                    "SELECT [notnull] FROM pragma_table_info('knowledge_entries') WHERE name = 'last_verified_at'",
                    [],
                    |row| row.get(0),
                )
                .expect("pragma notnull");
            assert_eq!(notnull, 0, "legacy NOT NULL column must be rebuilt as nullable");

            let zero_row: Option<i64> = c
                .query_row(
                    "SELECT last_verified_at FROM knowledge_entries WHERE record_id = 'a'",
                    [],
                    |row| row.get(0),
                )
                .expect("row a");
            assert_eq!(zero_row, None, "legacy 0 (never verified) must become NULL");

            let real_row: Option<i64> = c
                .query_row(
                    "SELECT last_verified_at FROM knowledge_entries WHERE record_id = 'b'",
                    [],
                    |row| row.get(0),
                )
                .expect("row b");
            assert_eq!(real_row, Some(123), "real timestamps must survive rebuild");

            let chunk_count: i64 = c
                .query_row("SELECT COUNT(*) FROM knowledge_chunks", [], |row| row.get(0))
                .expect("chunks");
            assert_eq!(chunk_count, 1, "chunks must survive the parent rebuild");

            // 重建后写入 NULL（None）必须成功——这正是旧 v4 库会失败的场景。
            c.execute(
                "INSERT INTO knowledge_entries \
                 (record_id, path, input_signature, status, summary, tags_json, \
                  key_concepts_json, provider_kind, provider_name, last_verified_at) \
                 VALUES ('c', './c.md', 'sig', 'active', 's', '[]', '[]', 'rule', 'r', NULL)",
                [],
            )
            .expect("NULL insert must succeed after rebuild");
            Ok(())
        })
        .expect("assertions");
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
                "episodes",
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
