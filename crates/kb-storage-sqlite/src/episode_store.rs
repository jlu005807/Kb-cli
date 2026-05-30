//! `EpisodeStore` 的 SQLite 实现：L0 Episodic 层。
//!
//! 与 fs 的 JSONL 实现等价语义，但充分利用 SQL：
//! - `append` = 单条 INSERT，靠 (timestamp, id) 索引维持时间序，无需重写。
//! - `query` 用 SQL WHERE 子句下推过滤（agent / session / kind / since），
//!   `limit` 走 "ORDER BY timestamp DESC LIMIT n 再反转" 拿最近 N 条。
//! - `count` 用 SELECT COUNT(*)，不把行读进内存。
//!
//! refs（chunk 引用列表）序列化为 JSON 数组存 `refs_json` 列——与
//! knowledge_entries 的 tags_json 同样取舍：变长 vec 落 JSON 足够高效，
//! 不值得再拆一张关联表。

use kb_core::domain::{AgentId, Episode, EpisodeFilter, EpisodeKind, Timestamp};
use kb_core::error::DomainError;
use kb_core::ports::EpisodeStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteEpisodeStore {
    conn: SqliteConnection,
}

impl SqliteEpisodeStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl EpisodeStore for SqliteEpisodeStore {
    fn append(&self, episode: &Episode) -> Result<(), DomainError> {
        let refs_json = serde_json::to_string(&episode.refs).map_err(|err| {
            DomainError::serialization(format!(
                "encode refs for episode '{}' failed: {err}",
                episode.id
            ))
        })?;
        self.conn.with(|c| {
            c.execute(
                "INSERT INTO episodes \
                 (id, timestamp, agent_id, session_id, kind, content, refs_json, confidence) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    episode.id,
                    episode.timestamp.as_nanos() as i64,
                    episode.agent_id.as_str(),
                    episode.session_id,
                    encode_kind(&episode.kind),
                    episode.content,
                    refs_json,
                    episode.confidence as f64,
                ],
            )
            .map_err(|err| {
                DomainError::conflict(format!("insert episode '{}' failed: {err}", episode.id))
            })?;
            Ok(())
        })
    }

    fn query(&self, filter: &EpisodeFilter) -> Result<Vec<Episode>, DomainError> {
        let (where_sql, params) = build_where(filter);
        self.conn.with(|c| {
            // limit 语义：取最近 N 条。用 DESC + LIMIT 拿尾部，再在内存里反转
            // 成时间升序，符合 trait 契约。无 limit 时直接 ASC。
            let order_limit = match filter.limit {
                Some(n) => format!("ORDER BY timestamp DESC, id DESC LIMIT {n}"),
                None => "ORDER BY timestamp ASC, id ASC".to_string(),
            };
            let sql = format!(
                "SELECT id, timestamp, agent_id, session_id, kind, content, refs_json, confidence \
                 FROM episodes {where_sql} {order_limit}"
            );
            let mut stmt = c
                .prepare(&sql)
                .map_err(|err| DomainError::io(format!("prepare episodes query failed: {err}")))?;
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            let rows = stmt
                .query_map(param_refs.as_slice(), decode_row)
                .map_err(|err| DomainError::io(format!("query episodes failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode episode row failed: {err}")))?;

            let mut episodes = Vec::with_capacity(rows.len());
            for row in rows {
                episodes.push(row?);
            }
            // limit 分支取的是 DESC，需反转回升序。
            if filter.limit.is_some() {
                episodes.reverse();
            }
            Ok(episodes)
        })
    }

    fn count(&self, filter: &EpisodeFilter) -> Result<usize, DomainError> {
        let (where_sql, params) = build_where(filter);
        self.conn.with(|c| {
            let sql = format!("SELECT COUNT(*) FROM episodes {where_sql}");
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            let count: i64 = c
                .query_row(&sql, param_refs.as_slice(), |row| row.get(0))
                .map_err(|err| DomainError::io(format!("count episodes failed: {err}")))?;
            Ok(count as usize)
        })
    }
}

/// 把 EpisodeFilter 翻译成 WHERE 子句 + 参数列表。kind 多值用 IN (...)。
/// 参数用拥有所有权的 Box<dyn ToSql> 以便跨语句边界传递。
fn build_where(filter: &EpisodeFilter) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(agent) = &filter.agent_id {
        clauses.push("agent_id = ?".to_string());
        params.push(Box::new(agent.as_str().to_string()));
    }
    if let Some(session) = &filter.session_id {
        clauses.push("session_id = ?".to_string());
        params.push(Box::new(session.clone()));
    }
    if !filter.kinds.is_empty() {
        let placeholders = vec!["?"; filter.kinds.len()].join(", ");
        clauses.push(format!("kind IN ({placeholders})"));
        for k in &filter.kinds {
            params.push(Box::new(encode_kind(k).to_string()));
        }
    }
    if let Some(since) = filter.since {
        clauses.push("timestamp >= ?".to_string());
        params.push(Box::new(since.as_nanos() as i64));
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (where_sql, params)
}

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Episode, DomainError>> {
    let id: String = row.get(0)?;
    let timestamp: i64 = row.get(1)?;
    let agent_raw: String = row.get(2)?;
    let session_id: String = row.get(3)?;
    let kind_raw: String = row.get(4)?;
    let content: String = row.get(5)?;
    let refs_json: String = row.get(6)?;
    let confidence: f64 = row.get(7)?;

    Ok((|| {
        let kind = decode_kind(&kind_raw)?;
        let refs: Vec<String> = serde_json::from_str(&refs_json).map_err(|err| {
            DomainError::serialization(format!("decode refs_json for episode '{id}' failed: {err}"))
        })?;
        // agent_id 落库时已经过校验；读取时宽松回退到 system，避免历史脏数据
        // 让整次查询失败。
        let agent_id = AgentId::new(agent_raw).unwrap_or_else(|_| AgentId::system());
        Ok(Episode {
            id,
            timestamp: Timestamp(timestamp as u64),
            agent_id,
            session_id,
            kind,
            content,
            refs,
            confidence: confidence as f32,
        })
    })())
}

fn encode_kind(k: &EpisodeKind) -> &'static str {
    match k {
        EpisodeKind::Query => "query",
        EpisodeKind::Answer => "answer",
        EpisodeKind::ToolCall => "tool_call",
        EpisodeKind::Observation => "observation",
        EpisodeKind::UserCorrection => "user_correction",
    }
}

fn decode_kind(raw: &str) -> Result<EpisodeKind, DomainError> {
    match raw {
        "query" => Ok(EpisodeKind::Query),
        "answer" => Ok(EpisodeKind::Answer),
        "tool_call" => Ok(EpisodeKind::ToolCall),
        "observation" => Ok(EpisodeKind::Observation),
        "user_correction" => Ok(EpisodeKind::UserCorrection),
        other => Err(DomainError::serialization(format!(
            "unknown episode kind '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;

    fn fresh_store() -> SqliteEpisodeStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteEpisodeStore::new(conn)
    }

    fn ep(id: &str, agent: &str, session: &str, kind: EpisodeKind, ts: u64) -> Episode {
        Episode::new(id, AgentId::new(agent).unwrap(), session, kind, "content")
            .with_timestamp(Timestamp(ts))
    }

    #[test]
    fn empty_store_returns_nothing() {
        let store = fresh_store();
        assert!(store.query(&EpisodeFilter::default()).unwrap().is_empty());
        assert_eq!(store.count(&EpisodeFilter::default()).unwrap(), 0);
    }

    #[test]
    fn append_then_query_in_time_order() {
        let store = fresh_store();
        store.append(&ep("e2", "a", "s", EpisodeKind::Answer, 2)).unwrap();
        store.append(&ep("e1", "a", "s", EpisodeKind::Query, 1)).unwrap();
        store.append(&ep("e3", "b", "t", EpisodeKind::Observation, 3)).unwrap();

        let all = store.query(&EpisodeFilter::default()).unwrap();
        assert_eq!(all.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["e1", "e2", "e3"]);
    }

    #[test]
    fn roundtrip_preserves_all_fields() {
        let store = fresh_store();
        let original = ep("e1", "cursor", "sess-1", EpisodeKind::Answer, 42)
            .with_refs(vec!["docs/a:0001".to_string(), "docs/b:0002".to_string()])
            .with_confidence(0.75);
        store.append(&original).unwrap();
        let loaded = store.query(&EpisodeFilter::default()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], original);
    }

    #[test]
    fn filter_by_agent_and_kind() {
        let store = fresh_store();
        store.append(&ep("e1", "cursor", "s", EpisodeKind::Query, 1)).unwrap();
        store.append(&ep("e2", "cursor", "s", EpisodeKind::Observation, 2)).unwrap();
        store.append(&ep("e3", "claude", "s", EpisodeKind::Observation, 3)).unwrap();

        let filter = EpisodeFilter {
            agent_id: Some(AgentId::new("cursor").unwrap()),
            kinds: vec![EpisodeKind::Observation],
            ..Default::default()
        };
        let out = store.query(&filter).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "e2");
        assert_eq!(store.count(&filter).unwrap(), 1);
    }

    #[test]
    fn filter_by_since() {
        let store = fresh_store();
        for i in 1..=5 {
            store.append(&ep(&format!("e{i}"), "a", "s", EpisodeKind::Query, i)).unwrap();
        }
        let filter = EpisodeFilter {
            since: Some(Timestamp(3)),
            ..Default::default()
        };
        let out = store.query(&filter).unwrap();
        assert_eq!(out.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["e3", "e4", "e5"]);
        assert_eq!(store.count(&filter).unwrap(), 3);
    }

    #[test]
    fn limit_returns_most_recent_in_ascending_order() {
        let store = fresh_store();
        for i in 1..=5 {
            store.append(&ep(&format!("e{i}"), "a", "s", EpisodeKind::Query, i)).unwrap();
        }
        let filter = EpisodeFilter {
            limit: Some(2),
            ..Default::default()
        };
        let out = store.query(&filter).unwrap();
        // 最近 2 条 = e4, e5，升序排列。
        assert_eq!(out.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["e4", "e5"]);
        // count 忽略 limit。
        assert_eq!(store.count(&filter).unwrap(), 5);
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let store = fresh_store();
        store.append(&ep("dup", "a", "s", EpisodeKind::Query, 1)).unwrap();
        let err = store.append(&ep("dup", "a", "s", EpisodeKind::Query, 2));
        assert!(err.is_err(), "duplicate episode id should conflict");
    }
}
