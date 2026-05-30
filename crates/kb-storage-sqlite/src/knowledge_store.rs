//! `KnowledgeStore` 的 SQLite 实现。
//!
//! L2 语义层落库布局：
//! - `knowledge_meta`：单例行，存 schema_version / generated_at /
//!   base_index_signature——读端用它来判断"snapshot 是否存在"。
//! - `knowledge_entries`：每个 record_id 一行，tags/key_concepts 序列化为 JSON
//!   数组保存在 `_json` 列。这两类字段是变长 vec，没必要再单独建表，落 JSON
//!   足够高效。
//! - `knowledge_chunks`：每行一个 chunk，`(record_id, chunk_id)` 复合主键，
//!   `record_id` 外键 ON DELETE CASCADE，保证 entry 被替换/删除时 chunk 自动清。
//!
//! 写入策略：全量替换。L2 编译每次输出整张快照，diff 由上游 IndexEngine 完成，
//! 我们只关心"原子落最新版本"。

use kb_core::domain::{
    KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
};
use kb_core::error::DomainError;
use kb_core::ports::KnowledgeStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteKnowledgeStore {
    conn: SqliteConnection,
}

impl SqliteKnowledgeStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl KnowledgeStore for SqliteKnowledgeStore {
    fn read_knowledge(&self) -> Result<Option<KnowledgeSnapshot>, DomainError> {
        self.conn.with(|c| {
            let header: Option<(i64, String, String)> = c
                .query_row(
                    "SELECT schema_version, generated_at, base_index_signature \
                     FROM knowledge_meta WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            let Some((schema_version, generated_at, base_index_signature)) = header else {
                return Ok(None);
            };

            let mut entry_stmt = c
                .prepare(
                    "SELECT record_id, path, input_signature, status, summary, \
                            tags_json, key_concepts_json, provider_kind, provider_name \
                     FROM knowledge_entries ORDER BY record_id ASC",
                )
                .map_err(|err| {
                    DomainError::io(format!("prepare knowledge_entries failed: {err}"))
                })?;
            let entry_rows = entry_stmt
                .query_map([], |row| {
                    let status_raw: String = row.get(3)?;
                    Ok((
                        row.get::<_, String>(0)?, // record_id
                        row.get::<_, String>(1)?, // path
                        row.get::<_, String>(2)?, // input_signature
                        status_raw,
                        row.get::<_, String>(4)?, // summary
                        row.get::<_, String>(5)?, // tags_json
                        row.get::<_, String>(6)?, // key_concepts_json
                        row.get::<_, String>(7)?, // provider_kind
                        row.get::<_, String>(8)?, // provider_name
                    ))
                })
                .map_err(|err| {
                    DomainError::io(format!("query knowledge_entries failed: {err}"))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    DomainError::io(format!("decode knowledge_entries row failed: {err}"))
                })?;

            let mut entries = Vec::with_capacity(entry_rows.len());
            for (
                record_id,
                path,
                input_signature,
                status_raw,
                summary,
                tags_json,
                key_concepts_json,
                provider_kind,
                provider_name,
            ) in entry_rows
            {
                let status = decode_status(&status_raw)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|err| {
                    DomainError::serialization(format!(
                        "decode tags_json for '{record_id}' failed: {err}"
                    ))
                })?;
                let key_concepts: Vec<String> =
                    serde_json::from_str(&key_concepts_json).map_err(|err| {
                        DomainError::serialization(format!(
                            "decode key_concepts_json for '{record_id}' failed: {err}"
                        ))
                    })?;
                let chunks = read_chunks_for(c, &record_id)?;
                entries.push(KnowledgeEntry {
                    record_id,
                    path,
                    input_signature,
                    status,
                    summary,
                    tags,
                    key_concepts,
                    chunks,
                    provider: SemanticProviderInfo {
                        kind: provider_kind,
                        name: provider_name,
                    },
                    // Phase 0 schema bump：SQLite 后端尚未持久化 confidence /
                    // source / timestamps / provenance，临时走 default 占位。
                    // 等 Phase 1（Episode 层）一起做 schema migration。
                    ..KnowledgeEntry::default()
                });
            }

            Ok(Some(KnowledgeSnapshot {
                schema_version: schema_version as u32,
                generated_at,
                base_index_signature,
                entries,
            }))
        })
    }

    fn write_knowledge(&self, snapshot: &KnowledgeSnapshot) -> Result<(), DomainError> {
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin write_knowledge tx failed: {err}")))?;

            // 由 FK ON DELETE CASCADE 处理 chunks，但 SQLite 仅在 foreign_keys=ON
            // 时生效（已在 connection 层配置）。这里仍显式按顺序删除，避免任何
            // pragma 异常下漏清行。
            tx.execute("DELETE FROM knowledge_chunks", [])
                .map_err(|err| DomainError::io(format!("clear knowledge_chunks failed: {err}")))?;
            tx.execute("DELETE FROM knowledge_entries", [])
                .map_err(|err| DomainError::io(format!("clear knowledge_entries failed: {err}")))?;
            tx.execute("DELETE FROM knowledge_meta", [])
                .map_err(|err| DomainError::io(format!("clear knowledge_meta failed: {err}")))?;

            {
                let mut entry_stmt = tx
                    .prepare(
                        "INSERT INTO knowledge_entries \
                         (record_id, path, input_signature, status, summary, \
                          tags_json, key_concepts_json, provider_kind, provider_name) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!(
                            "prepare knowledge_entries insert failed: {err}"
                        ))
                    })?;
                let mut chunk_stmt = tx
                    .prepare(
                        "INSERT INTO knowledge_chunks \
                         (record_id, chunk_id, byte_start, byte_end, \
                          content_signature, summary, preview) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!(
                            "prepare knowledge_chunks insert failed: {err}"
                        ))
                    })?;

                for entry in &snapshot.entries {
                    let tags_json = serde_json::to_string(&entry.tags).map_err(|err| {
                        DomainError::serialization(format!(
                            "encode tags for '{}' failed: {err}",
                            entry.record_id
                        ))
                    })?;
                    let key_concepts_json =
                        serde_json::to_string(&entry.key_concepts).map_err(|err| {
                            DomainError::serialization(format!(
                                "encode key_concepts for '{}' failed: {err}",
                                entry.record_id
                            ))
                        })?;
                    entry_stmt
                        .execute(rusqlite::params![
                            entry.record_id,
                            entry.path,
                            entry.input_signature,
                            encode_status(&entry.status),
                            entry.summary,
                            tags_json,
                            key_concepts_json,
                            entry.provider.kind,
                            entry.provider.name,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert knowledge_entry '{}' failed: {err}",
                                entry.record_id
                            ))
                        })?;
                    for chunk in &entry.chunks {
                        chunk_stmt
                            .execute(rusqlite::params![
                                entry.record_id,
                                chunk.chunk_id,
                                chunk.byte_range[0] as i64,
                                chunk.byte_range[1] as i64,
                                chunk.content_signature,
                                chunk.summary,
                                chunk.preview,
                            ])
                            .map_err(|err| {
                                DomainError::conflict(format!(
                                    "insert knowledge_chunk '{}::{}' failed: {err}",
                                    entry.record_id, chunk.chunk_id
                                ))
                            })?;
                    }
                }
            }

            tx.execute(
                "INSERT INTO knowledge_meta \
                 (id, schema_version, generated_at, base_index_signature) \
                 VALUES (1, ?1, ?2, ?3)",
                rusqlite::params![
                    snapshot.schema_version as i64,
                    snapshot.generated_at,
                    snapshot.base_index_signature,
                ],
            )
            .map_err(|err| DomainError::io(format!("insert knowledge_meta failed: {err}")))?;

            tx.commit()
                .map_err(|err| DomainError::io(format!("commit write_knowledge failed: {err}")))?;
            Ok(())
        })
    }
}

fn read_chunks_for(
    c: &rusqlite::Connection,
    record_id: &str,
) -> Result<Vec<KnowledgeChunk>, DomainError> {
    let mut stmt = c
        .prepare(
            "SELECT chunk_id, byte_start, byte_end, content_signature, summary, preview \
             FROM knowledge_chunks WHERE record_id = ?1 \
             ORDER BY byte_start ASC, chunk_id ASC",
        )
        .map_err(|err| DomainError::io(format!("prepare knowledge_chunks query failed: {err}")))?;
    let rows = stmt
        .query_map(rusqlite::params![record_id], |row| {
            Ok(KnowledgeChunk {
                chunk_id: row.get::<_, String>(0)?,
                byte_range: [row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64],
                content_signature: row.get::<_, String>(3)?,
                summary: row.get::<_, String>(4)?,
                preview: row.get::<_, String>(5)?,
            })
        })
        .map_err(|err| DomainError::io(format!("query knowledge_chunks failed: {err}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| DomainError::io(format!("decode knowledge_chunks row failed: {err}")))?;
    Ok(rows)
}

fn encode_status(s: &KnowledgeStatus) -> &'static str {
    match s {
        KnowledgeStatus::Active => "active",
        KnowledgeStatus::Missing => "missing",
        KnowledgeStatus::Failed => "failed",
        KnowledgeStatus::Stale => "stale",
    }
}

fn decode_status(raw: &str) -> Result<KnowledgeStatus, DomainError> {
    match raw {
        "active" => Ok(KnowledgeStatus::Active),
        "missing" => Ok(KnowledgeStatus::Missing),
        "failed" => Ok(KnowledgeStatus::Failed),
        "stale" => Ok(KnowledgeStatus::Stale),
        other => Err(DomainError::serialization(format!(
            "unknown knowledge status '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;
    use kb_core::domain::{
        KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
    };

    fn fresh_store() -> SqliteKnowledgeStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteKnowledgeStore::new(conn)
    }

    fn sample_snapshot() -> KnowledgeSnapshot {
        KnowledgeSnapshot {
            schema_version: 1,
            generated_at: "2026-05-27T00:00:00+08:00".to_string(),
            base_index_signature: "idx-sig".to_string(),
            entries: vec![KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "input-1".to_string(),
                status: KnowledgeStatus::Active,
                summary: "summary".to_string(),
                tags: vec!["agent".to_string(), "memory".to_string()],
                key_concepts: vec!["graph".to_string()],
                chunks: vec![
                    KnowledgeChunk {
                        chunk_id: "docs/a:0001".to_string(),
                        byte_range: [0, 32],
                        content_signature: "c1".to_string(),
                        summary: "chunk one".to_string(),
                        preview: "preview one".to_string(),
                    },
                    KnowledgeChunk {
                        chunk_id: "docs/a:0002".to_string(),
                        byte_range: [32, 64],
                        content_signature: "c2".to_string(),
                        summary: "chunk two".to_string(),
                        preview: "preview two".to_string(),
                    },
                ],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule-v1".to_string(),
                },
                ..KnowledgeEntry::default()
            }],
        }
    }

    #[test]
    fn empty_database_returns_none() {
        let store = fresh_store();
        assert_eq!(store.read_knowledge().expect("read"), None);
    }

    #[test]
    fn write_then_read_roundtrip_preserves_entries_and_chunks() {
        let store = fresh_store();
        let snapshot = sample_snapshot();
        store.write_knowledge(&snapshot).expect("write");
        let loaded = store.read_knowledge().expect("read").expect("snapshot");
        assert_eq!(loaded, snapshot);
    }

    #[test]
    fn second_write_replaces_full_snapshot() {
        let store = fresh_store();
        store.write_knowledge(&sample_snapshot()).expect("first write");

        let replacement = KnowledgeSnapshot {
            schema_version: 1,
            generated_at: "2026-05-27T01:00:00+08:00".to_string(),
            base_index_signature: "idx-sig-2".to_string(),
            entries: vec![],
        };
        store.write_knowledge(&replacement).expect("second write");
        let loaded = store.read_knowledge().expect("read").expect("snapshot");
        assert!(loaded.entries.is_empty());
        assert_eq!(loaded.base_index_signature, "idx-sig-2");
    }

    #[test]
    fn unknown_status_decodes_as_serialization_error() {
        let err = decode_status("weird").expect_err("should fail");
        assert!(err.to_string().to_lowercase().contains("status"));
    }
}
