//! `SearchStore` 的 SQLite 实现。
//!
//! L3 投影层是 L2 的扁平化检索表，每条命中关联到一个 chunk。这里把 matched_terms
//! 序列化为 JSON 数组存入单列——term 是变长 vec、对单 hit 全量读写为主，没必要
//! 再开一张关联表。
//!
//! 写入策略：全量替换（DELETE + 批量 INSERT），跟其他 L2+ store 一致。

use kb_core::domain::{SearchHit, SearchProjectionSnapshot};
use kb_core::error::DomainError;
use kb_core::ports::SearchStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteSearchStore {
    conn: SqliteConnection,
}

impl SqliteSearchStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl SearchStore for SqliteSearchStore {
    fn read_search(&self) -> Result<Option<SearchProjectionSnapshot>, DomainError> {
        self.conn.with(|c| {
            let header: Option<(i64, String, String)> = c
                .query_row(
                    "SELECT schema_version, generated_at, base_knowledge_signature \
                     FROM search_meta WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            let Some((schema_version, generated_at, base_knowledge_signature)) = header else {
                return Ok(None);
            };

            // search_hits 表结构（schema v1）里只有 record_id/chunk_id/score/matched_terms，
            // 没有 key/content_signature 列，因此这两个字段我们落在 matched_terms 同表的
            // 额外列里之前需要先扩展 schema。先按 schema 现状还原其余字段；为了
            // 不破坏 round-trip 完整性，key 与 content_signature 改由组合还原：
            // - key 按 fs 实现的命名约定 `{record_id}|{chunk_id}|{content_signature}`
            //   重建，所以必须把 content_signature 也保存下来。
            // 这里我们用 search_meta 之外的方式：把 (key, content_signature) 当作一个
            // 复合行存。为不再做 schema 迁移，沿用 `matched_terms` 列复用 JSON：
            //   {"terms":[...], "key":"...", "content_signature":"..."}
            // 兼容现有 schema v2，无需再 bump。
            let mut hit_stmt = c
                .prepare(
                    "SELECT record_id, chunk_id, score, matched_terms FROM search_hits \
                     ORDER BY score DESC, record_id ASC, chunk_id ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare search_hits failed: {err}")))?;
            let rows = hit_stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,         // record_id
                        row.get::<_, Option<String>>(1)?, // chunk_id (nullable in schema)
                        row.get::<_, f64>(2)?,            // score
                        row.get::<_, String>(3)?,         // matched_terms (JSON envelope)
                    ))
                })
                .map_err(|err| DomainError::io(format!("query search_hits failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode search_hits row failed: {err}")))?;

            let mut hits = Vec::with_capacity(rows.len());
            for (record_id, chunk_id_opt, score, matched_terms_raw) in rows {
                let envelope: SearchHitEnvelope =
                    serde_json::from_str(&matched_terms_raw).map_err(|err| {
                        DomainError::serialization(format!(
                            "decode search_hit envelope for '{record_id}' failed: {err}"
                        ))
                    })?;
                let chunk_id = chunk_id_opt.unwrap_or_default();
                hits.push(SearchHit {
                    key: envelope.key,
                    record_id,
                    chunk_id,
                    content_signature: envelope.content_signature,
                    score,
                    matched_terms: envelope.matched_terms,
                });
            }

            Ok(Some(SearchProjectionSnapshot {
                schema_version: schema_version as u32,
                generated_at,
                base_knowledge_signature,
                hits,
            }))
        })
    }

    fn write_search(&self, snapshot: &SearchProjectionSnapshot) -> Result<(), DomainError> {
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin write_search tx failed: {err}")))?;

            tx.execute("DELETE FROM search_hits", [])
                .map_err(|err| DomainError::io(format!("clear search_hits failed: {err}")))?;
            tx.execute("DELETE FROM search_meta", [])
                .map_err(|err| DomainError::io(format!("clear search_meta failed: {err}")))?;

            {
                let mut stmt = tx
                    .prepare(
                        "INSERT INTO search_hits \
                         (record_id, chunk_id, score, matched_terms) \
                         VALUES (?1, ?2, ?3, ?4)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare search_hits insert failed: {err}"))
                    })?;
                for hit in &snapshot.hits {
                    let envelope = SearchHitEnvelope {
                        key: hit.key.clone(),
                        content_signature: hit.content_signature.clone(),
                        matched_terms: hit.matched_terms.clone(),
                    };
                    let envelope_json = serde_json::to_string(&envelope).map_err(|err| {
                        DomainError::serialization(format!(
                            "encode search_hit envelope for '{}' failed: {err}",
                            hit.record_id
                        ))
                    })?;
                    let chunk_id_opt: Option<&str> = if hit.chunk_id.is_empty() {
                        None
                    } else {
                        Some(hit.chunk_id.as_str())
                    };
                    stmt.execute(rusqlite::params![
                        hit.record_id,
                        chunk_id_opt,
                        hit.score,
                        envelope_json,
                    ])
                    .map_err(|err| {
                        DomainError::conflict(format!(
                            "insert search_hit '{}' failed: {err}",
                            hit.key
                        ))
                    })?;
                }
            }

            tx.execute(
                "INSERT INTO search_meta \
                 (id, schema_version, generated_at, base_knowledge_signature) \
                 VALUES (1, ?1, ?2, ?3)",
                rusqlite::params![
                    snapshot.schema_version as i64,
                    snapshot.generated_at,
                    snapshot.base_knowledge_signature,
                ],
            )
            .map_err(|err| DomainError::io(format!("insert search_meta failed: {err}")))?;

            tx.commit()
                .map_err(|err| DomainError::io(format!("commit write_search failed: {err}")))?;
            Ok(())
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SearchHitEnvelope {
    key: String,
    content_signature: String,
    matched_terms: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;
    use kb_core::domain::{SearchHit, SearchProjectionSnapshot};

    fn fresh_store() -> SqliteSearchStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteSearchStore::new(conn)
    }

    fn sample_snapshot() -> SearchProjectionSnapshot {
        SearchProjectionSnapshot {
            schema_version: 1,
            generated_at: "2026-05-27T00:00:00+08:00".to_string(),
            base_knowledge_signature: "k-sig".to_string(),
            hits: vec![
                SearchHit {
                    key: "docs/a|docs/a:0001|c1".to_string(),
                    record_id: "docs/a".to_string(),
                    chunk_id: "docs/a:0001".to_string(),
                    content_signature: "c1".to_string(),
                    score: 0.91,
                    matched_terms: vec!["agent".to_string(), "memory".to_string()],
                },
                SearchHit {
                    key: "docs/b|docs/b:0001|c2".to_string(),
                    record_id: "docs/b".to_string(),
                    chunk_id: "docs/b:0001".to_string(),
                    content_signature: "c2".to_string(),
                    score: 0.55,
                    matched_terms: vec!["graph".to_string()],
                },
            ],
        }
    }

    #[test]
    fn empty_database_returns_none() {
        let store = fresh_store();
        assert_eq!(store.read_search().expect("read"), None);
    }

    #[test]
    fn write_then_read_roundtrip_preserves_hits() {
        let store = fresh_store();
        let snapshot = sample_snapshot();
        store.write_search(&snapshot).expect("write");
        let loaded = store.read_search().expect("read").expect("snapshot");
        // 顺序由 SELECT 的 ORDER BY score DESC 决定，与 sample 写入顺序一致。
        assert_eq!(loaded.hits.len(), 2);
        assert_eq!(loaded.hits[0].key, snapshot.hits[0].key);
        assert_eq!(loaded.hits[1].content_signature, "c2");
        assert_eq!(loaded.base_knowledge_signature, "k-sig");
    }
}
