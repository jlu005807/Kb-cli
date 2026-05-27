//! `WikiStore` 的 SQLite 实现。
//!
//! L5 wiki 视图层：每个 page_id 一行，markdown 全文 + 元数据 JSON envelope。
//! 在 fs 实现里每个 page 落一份独立 .md 文件，因此 trait 上还要传
//! `removed_paths` 用来删过期文件；SQLite 实现没有这层文件副本，全量
//! 替换天然消除旧行，`removed_paths` 仅作信号——保留参数是为了
//! port trait 兼容。
//!
//! `read_wiki_manifest` 只返回 manifest（不含 markdown），跟 fs 行为对齐。

use kb_core::domain::{
    WikiManifestEntry, WikiManifestSnapshot, WikiPageDocument, WikiPageType, WikiViewSnapshot,
};
use kb_core::error::DomainError;
use kb_core::ports::WikiStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteWikiStore {
    conn: SqliteConnection,
}

impl SqliteWikiStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl WikiStore for SqliteWikiStore {
    fn read_wiki_manifest(&self) -> Result<Option<WikiManifestSnapshot>, DomainError> {
        self.conn.with(|c| {
            let header: Option<(i64, String, String)> = c
                .query_row(
                    "SELECT schema_version, generated_at, base_graph_signature \
                     FROM wiki_meta WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            let Some((schema_version, generated_at, base_graph_signature)) = header else {
                return Ok(None);
            };

            // wiki_pages 是页面正文 + 元数据，wiki_manifest 单独维护 summary 行。
            // 读 manifest 时只需 wiki_pages 的元数据 envelope（不需要 markdown）,
            // 跟 fs 版的"只读 manifest.json"语义保持一致。
            let mut stmt = c
                .prepare(
                    "SELECT page_id, page_type, metadata_json \
                     FROM wiki_pages ORDER BY page_id ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare wiki_pages failed: {err}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|err| DomainError::io(format!("query wiki_pages failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode wiki_pages row failed: {err}")))?;

            let mut pages = Vec::with_capacity(rows.len());
            for (page_id, type_raw, metadata_raw) in rows {
                let meta: WikiPageMetaEnvelope =
                    serde_json::from_str(&metadata_raw).map_err(|err| {
                        DomainError::serialization(format!(
                            "decode wiki_page metadata for '{page_id}' failed: {err}"
                        ))
                    })?;
                pages.push(WikiManifestEntry {
                    page_id,
                    page_type: decode_page_type(&type_raw)?,
                    title: meta.title,
                    relative_path: meta.relative_path,
                    input_signature: meta.input_signature,
                    source_records: meta.source_records,
                    source_chunks: meta.source_chunks,
                });
            }

            Ok(Some(WikiManifestSnapshot {
                schema_version: schema_version as u32,
                generated_at,
                base_graph_signature,
                pages,
            }))
        })
    }

    fn write_wiki_view(
        &self,
        snapshot: &WikiViewSnapshot,
        _removed_paths: &[String],
    ) -> Result<(), DomainError> {
        // SQLite 后端没有独立的 markdown 文件副本，全量替换自然清理旧行；
        // 因此 `_removed_paths` 在这里不做额外处理。保留参数以匹配 port 签名。
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin write_wiki tx failed: {err}")))?;

            tx.execute("DELETE FROM wiki_manifest", [])
                .map_err(|err| DomainError::io(format!("clear wiki_manifest failed: {err}")))?;
            tx.execute("DELETE FROM wiki_pages", [])
                .map_err(|err| DomainError::io(format!("clear wiki_pages failed: {err}")))?;
            tx.execute("DELETE FROM wiki_meta", [])
                .map_err(|err| DomainError::io(format!("clear wiki_meta failed: {err}")))?;

            {
                let mut page_stmt = tx
                    .prepare(
                        "INSERT INTO wiki_pages (page_id, page_type, markdown, metadata_json) \
                         VALUES (?1, ?2, ?3, ?4)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare wiki_pages insert failed: {err}"))
                    })?;
                let mut manifest_stmt = tx
                    .prepare(
                        "INSERT INTO wiki_manifest (page_id, summary, updated_at) \
                         VALUES (?1, ?2, ?3)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare wiki_manifest insert failed: {err}"))
                    })?;

                for page in &snapshot.pages {
                    let meta = WikiPageMetaEnvelope {
                        title: page.title.clone(),
                        relative_path: page.relative_path.clone(),
                        input_signature: page.input_signature.clone(),
                        source_records: page.source_records.clone(),
                        source_chunks: page.source_chunks.clone(),
                    };
                    let meta_json = serde_json::to_string(&meta).map_err(|err| {
                        DomainError::serialization(format!(
                            "encode wiki_page meta for '{}' failed: {err}",
                            page.page_id
                        ))
                    })?;
                    page_stmt
                        .execute(rusqlite::params![
                            page.page_id,
                            encode_page_type(&page.page_type),
                            page.markdown,
                            meta_json,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert wiki_page '{}' failed: {err}",
                                page.page_id
                            ))
                        })?;
                    manifest_stmt
                        .execute(rusqlite::params![
                            page.page_id,
                            page.title,
                            snapshot.manifest.generated_at,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert wiki_manifest '{}' failed: {err}",
                                page.page_id
                            ))
                        })?;
                }
            }

            tx.execute(
                "INSERT INTO wiki_meta \
                 (id, schema_version, generated_at, base_graph_signature) \
                 VALUES (1, ?1, ?2, ?3)",
                rusqlite::params![
                    snapshot.manifest.schema_version as i64,
                    snapshot.manifest.generated_at,
                    snapshot.manifest.base_graph_signature,
                ],
            )
            .map_err(|err| DomainError::io(format!("insert wiki_meta failed: {err}")))?;

            tx.commit()
                .map_err(|err| DomainError::io(format!("commit write_wiki failed: {err}")))?;
            Ok(())
        })
    }
}

impl SqliteWikiStore {
    /// 读出完整 page 正文 + 元数据。port trait 上没有这个能力，
    /// 这里作为 SQLite 后端的便利方法暴露（fs 实现读 markdown 文件，
    /// SQLite 直接读行）。
    pub fn read_wiki_pages(&self) -> Result<Vec<WikiPageDocument>, DomainError> {
        self.conn.with(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT page_id, page_type, markdown, metadata_json \
                     FROM wiki_pages ORDER BY page_id ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare wiki_pages read failed: {err}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|err| DomainError::io(format!("query wiki_pages failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DomainError::io(format!("decode wiki_pages row failed: {err}")))?;
            let mut pages = Vec::with_capacity(rows.len());
            for (page_id, type_raw, markdown, metadata_raw) in rows {
                let meta: WikiPageMetaEnvelope =
                    serde_json::from_str(&metadata_raw).map_err(|err| {
                        DomainError::serialization(format!(
                            "decode wiki_page metadata for '{page_id}' failed: {err}"
                        ))
                    })?;
                pages.push(WikiPageDocument {
                    page_id,
                    page_type: decode_page_type(&type_raw)?,
                    title: meta.title,
                    relative_path: meta.relative_path,
                    input_signature: meta.input_signature,
                    source_records: meta.source_records,
                    source_chunks: meta.source_chunks,
                    markdown,
                });
            }
            Ok(pages)
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WikiPageMetaEnvelope {
    title: String,
    relative_path: String,
    input_signature: String,
    source_records: Vec<String>,
    source_chunks: Vec<String>,
}

fn encode_page_type(t: &WikiPageType) -> &'static str {
    match t {
        WikiPageType::Index => "index",
        WikiPageType::Log => "log",
        WikiPageType::Overview => "overview",
        WikiPageType::Source => "source",
    }
}

fn decode_page_type(raw: &str) -> Result<WikiPageType, DomainError> {
    match raw {
        "index" => Ok(WikiPageType::Index),
        "log" => Ok(WikiPageType::Log),
        "overview" => Ok(WikiPageType::Overview),
        "source" => Ok(WikiPageType::Source),
        other => Err(DomainError::serialization(format!(
            "unknown wiki page type '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;
    use kb_core::domain::{
        WikiManifestEntry, WikiManifestSnapshot, WikiPageDocument, WikiPageType, WikiViewSnapshot,
    };

    fn fresh_store() -> SqliteWikiStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteWikiStore::new(conn)
    }

    fn sample_snapshot() -> WikiViewSnapshot {
        WikiViewSnapshot {
            manifest: WikiManifestSnapshot {
                schema_version: 1,
                generated_at: "2026-05-27T00:00:00+08:00".to_string(),
                base_graph_signature: "g-sig".to_string(),
                pages: vec![WikiManifestEntry {
                    page_id: "index".to_string(),
                    page_type: WikiPageType::Index,
                    title: "Knowledge Wiki".to_string(),
                    relative_path: "index.md".to_string(),
                    input_signature: "sig-index".to_string(),
                    source_records: vec!["docs/a".to_string()],
                    source_chunks: vec!["docs/a:0001".to_string()],
                }],
            },
            pages: vec![WikiPageDocument {
                page_id: "index".to_string(),
                page_type: WikiPageType::Index,
                title: "Knowledge Wiki".to_string(),
                relative_path: "index.md".to_string(),
                input_signature: "sig-index".to_string(),
                source_records: vec!["docs/a".to_string()],
                source_chunks: vec!["docs/a:0001".to_string()],
                markdown: "# Wiki\n\nbody".to_string(),
            }],
        }
    }

    #[test]
    fn empty_database_returns_none_for_manifest() {
        let store = fresh_store();
        assert_eq!(store.read_wiki_manifest().expect("read"), None);
    }

    #[test]
    fn write_then_read_manifest_roundtrip_preserves_entries() {
        let store = fresh_store();
        let snapshot = sample_snapshot();
        store.write_wiki_view(&snapshot, &[]).expect("write");
        let manifest = store.read_wiki_manifest().expect("read").expect("present");
        assert_eq!(manifest, snapshot.manifest);
    }

    #[test]
    fn read_wiki_pages_returns_markdown_payload() {
        let store = fresh_store();
        store
            .write_wiki_view(&sample_snapshot(), &[])
            .expect("write");
        let pages = store.read_wiki_pages().expect("read pages");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].markdown, "# Wiki\n\nbody");
        assert_eq!(pages[0].source_records, vec!["docs/a".to_string()]);
    }

    #[test]
    fn second_write_replaces_full_snapshot() {
        let store = fresh_store();
        store
            .write_wiki_view(&sample_snapshot(), &[])
            .expect("first write");

        let replacement = WikiViewSnapshot {
            manifest: WikiManifestSnapshot {
                schema_version: 1,
                generated_at: "2026-05-27T01:00:00+08:00".to_string(),
                base_graph_signature: "g-sig-2".to_string(),
                pages: vec![],
            },
            pages: vec![],
        };
        store
            .write_wiki_view(&replacement, &["index.md".to_string()])
            .expect("second write");
        let manifest = store.read_wiki_manifest().expect("read").expect("present");
        assert!(manifest.pages.is_empty());
        assert_eq!(manifest.base_graph_signature, "g-sig-2");
        assert!(store.read_wiki_pages().expect("pages").is_empty());
    }
}
