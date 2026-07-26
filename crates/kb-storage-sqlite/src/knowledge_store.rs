//! `KnowledgeStore` 的 SQLite 实现。
//!
//! L2 语义层落库布局：
//! - `knowledge_meta`：单例行，存 schema_version / generated_at /
//!   base_index_signature——读端用它来判断"snapshot 是否存在"。
//! - `knowledge_entries`：每个 record_id 一行，tags/key_concepts 序列化为 JSON
//!   数组保存在 `_json` 列。这两类字段是变长 vec，没必要再单独建表，落 JSON
//!   足够高效。schema v4 起同一行还携带 Phase 0 记忆字段：confidence（REAL）、
//!   source_json（KnowledgeSource 的 serde_json）、created_at（unix nanos 存
//!   i64，与 episodes.timestamp 同一约定；0 表示无时间锚）、last_verified_at
//!   （可空列，NULL 表示从未验证，数值含 0 均精确还原）、provenance_json
//!   （episode id 的 JSON 数组）。
//! - `knowledge_chunks`：每行一个 chunk，`(record_id, chunk_id)` 复合主键，
//!   `record_id` 外键 ON DELETE CASCADE，保证 entry 被替换/删除时 chunk 自动清。
//!
//! 写入策略：全量替换。L2 编译每次输出整张快照，diff 由上游 IndexEngine 完成，
//! 我们只关心"原子落最新版本"。

use kb_core::domain::{
    KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeSource, KnowledgeStatus,
    SemanticProviderInfo, Timestamp,
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
                            tags_json, key_concepts_json, provider_kind, provider_name, \
                            confidence, source_json, created_at, last_verified_at, \
                            provenance_json \
                     FROM knowledge_entries ORDER BY record_id ASC",
                )
                .map_err(|err| {
                    DomainError::io(format!("prepare knowledge_entries failed: {err}"))
                })?;
            let entry_rows = entry_stmt
                .query_map([], |row| {
                    Ok(EntryRow {
                        record_id: row.get(0)?,
                        path: row.get(1)?,
                        input_signature: row.get(2)?,
                        status_raw: row.get(3)?,
                        summary: row.get(4)?,
                        tags_json: row.get(5)?,
                        key_concepts_json: row.get(6)?,
                        provider_kind: row.get(7)?,
                        provider_name: row.get(8)?,
                        confidence: row.get(9)?,
                        source_json: row.get(10)?,
                        created_at: row.get(11)?,
                        last_verified_at: row.get(12)?,
                        provenance_json: row.get(13)?,
                    })
                })
                .map_err(|err| {
                    DomainError::io(format!("query knowledge_entries failed: {err}"))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    DomainError::io(format!("decode knowledge_entries row failed: {err}"))
                })?;

            let mut entries = Vec::with_capacity(entry_rows.len());
            for row in entry_rows {
                let status = decode_status(&row.status_raw)?;
                let record_id = row.record_id;
                let tags: Vec<String> = serde_json::from_str(&row.tags_json).map_err(|err| {
                    DomainError::serialization(format!(
                        "decode tags_json for '{record_id}' failed: {err}"
                    ))
                })?;
                let key_concepts: Vec<String> =
                    serde_json::from_str(&row.key_concepts_json).map_err(|err| {
                        DomainError::serialization(format!(
                            "decode key_concepts_json for '{record_id}' failed: {err}"
                        ))
                    })?;
                let chunks = read_chunks_for(c, &record_id)?;
                // 记忆字段的宽松解码：pre-v4 旧行（source_json=''）或历史脏
                // 数据不应让整次读取失败，统一回退到 serde default 同款语义。
                let source = decode_source(&row.source_json);
                let provenance: Vec<String> =
                    serde_json::from_str(&row.provenance_json).unwrap_or_default();
                let created_at = Timestamp(row.created_at as u64);
                // 可空列：NULL = 从未验证（None）；任何数值（含 0）都精确还原。
                let last_verified_at = row
                    .last_verified_at
                    .map(|nanos| Timestamp(nanos as u64));
                entries.push(KnowledgeEntry {
                    record_id,
                    path: row.path,
                    input_signature: row.input_signature,
                    status,
                    summary: row.summary,
                    tags,
                    key_concepts,
                    chunks,
                    provider: SemanticProviderInfo {
                        kind: row.provider_kind,
                        name: row.provider_name,
                    },
                    confidence: row.confidence as f32,
                    source,
                    created_at,
                    last_verified_at,
                    provenance,
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
                          tags_json, key_concepts_json, provider_kind, provider_name, \
                          confidence, source_json, created_at, last_verified_at, \
                          provenance_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                    let source_json = serde_json::to_string(&entry.source).map_err(|err| {
                        DomainError::serialization(format!(
                            "encode source for '{}' failed: {err}",
                            entry.record_id
                        ))
                    })?;
                    let provenance_json =
                        serde_json::to_string(&entry.provenance).map_err(|err| {
                            DomainError::serialization(format!(
                                "encode provenance for '{}' failed: {err}",
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
                            entry.confidence as f64,
                            source_json,
                            entry.created_at.as_nanos() as i64,
                            // None（从未验证）落 NULL，Some(0) 与 None 不合流。
                            entry.last_verified_at.map(|t| t.as_nanos() as i64),
                            provenance_json,
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

/// `knowledge_entries` 单行的原始列值（解码前）。
/// 列多到超过 tuple 可读性的临界点，用具名结构体承接。
struct EntryRow {
    record_id: String,
    path: String,
    input_signature: String,
    status_raw: String,
    summary: String,
    tags_json: String,
    key_concepts_json: String,
    provider_kind: String,
    provider_name: String,
    confidence: f64,
    source_json: String,
    created_at: i64,
    last_verified_at: Option<i64>,
    provenance_json: String,
}

/// 宽松解码 source_json：
/// - `''`（up_v4 列默认值，即 pre-v4 旧行）→ 默认 source（FromFile）
/// - 反序列化失败（历史脏数据）→ 同样回退默认，不让整次读取失败
fn decode_source(raw: &str) -> KnowledgeSource {
    if raw.is_empty() {
        return KnowledgeSource::default();
    }
    serde_json::from_str(raw).unwrap_or_default()
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
        KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeSource, KnowledgeStatus,
        SemanticProviderInfo, Timestamp,
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

    /// Phase 0 记忆字段（confidence / source / timestamps / provenance）
    /// 必须在 SQLite 后端完整 round-trip——此前它们在读端被 default 覆盖，
    /// 造成 storage.backend=sqlite 下静默丢数据。
    #[test]
    fn memory_fields_roundtrip_exactly() {
        let store = fresh_store();
        let mut snapshot = sample_snapshot();
        snapshot.entries[0] = KnowledgeEntry {
            confidence: 0.6,
            source: KnowledgeSource::from_agent("cursor"),
            created_at: Timestamp(1_748_300_000_000_000_123),
            last_verified_at: Some(Timestamp(1_748_300_100_000_000_456)),
            provenance: vec!["ep-1".to_string(), "ep-2".to_string()],
            ..snapshot.entries[0].clone()
        };

        store.write_knowledge(&snapshot).expect("write");
        let loaded = store.read_knowledge().expect("read").expect("snapshot");

        assert_eq!(loaded.entries.len(), 1);
        let got = &loaded.entries[0];
        let want = &snapshot.entries[0];
        // 逐字段断言，直接暴露具体丢的是哪一个字段。
        assert_eq!(got.confidence, 0.6);
        assert_eq!(got.source, KnowledgeSource::from_agent("cursor"));
        assert_eq!(
            got.source.agent_id().map(|id| id.as_str()),
            Some("cursor")
        );
        assert_eq!(got.created_at, Timestamp(1_748_300_000_000_000_123));
        assert_eq!(
            got.last_verified_at,
            Some(Timestamp(1_748_300_100_000_000_456))
        );
        assert_eq!(got.provenance, vec!["ep-1", "ep-2"]);
        // 基础字段没有被顺带弄坏。
        assert_eq!(got, want);
        assert_eq!(loaded, snapshot);
    }

    /// pre-v4 旧行（source_json='' 等列默认值）读出来应等于 serde default 语义，
    /// 而不是报错——对应从 v3 库迁移上来的存量数据。
    #[test]
    fn pre_v4_row_reads_back_with_defaults() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        conn.with(|c| {
            // 只写 v1 时代就存在的列，让 v4 新列吃 ALTER TABLE 的默认值。
            c.execute(
                "INSERT INTO knowledge_entries \
                 (record_id, path, input_signature, status, summary, \
                  tags_json, key_concepts_json, provider_kind, provider_name) \
                 VALUES ('docs/old', './docs/old.md', 'sig', 'active', 's', \
                         '[]', '[]', 'rule', 'default')",
                [],
            )
            .expect("insert legacy row");
            c.execute(
                "INSERT INTO knowledge_meta \
                 (id, schema_version, generated_at, base_index_signature) \
                 VALUES (1, 1, 'unix:0', 'idx-sig')",
                [],
            )
            .expect("insert meta");
            Ok(())
        })
        .expect("with");

        let store = SqliteKnowledgeStore::new(conn);
        let loaded = store.read_knowledge().expect("read").expect("snapshot");
        assert_eq!(loaded.entries.len(), 1);
        let entry = &loaded.entries[0];
        assert_eq!(entry.confidence, 1.0);
        assert_eq!(entry.source, KnowledgeSource::FromFile);
        assert_eq!(entry.created_at, Timestamp::ZERO);
        assert_eq!(entry.last_verified_at, None);
        assert!(entry.provenance.is_empty());
    }

    #[test]
    fn some_zero_last_verified_at_survives_roundtrip() {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        let store = SqliteKnowledgeStore::new(conn);

        let mut snapshot = sample_snapshot();
        snapshot.entries[0].last_verified_at = Some(Timestamp::ZERO);
        store.write_knowledge(&snapshot).expect("write");

        let got = store.read_knowledge().expect("read").expect("some");
        assert_eq!(
            got.entries[0].last_verified_at,
            Some(Timestamp::ZERO),
            "Some(ZERO) must not collapse into None (nullable column semantics)"
        );
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
