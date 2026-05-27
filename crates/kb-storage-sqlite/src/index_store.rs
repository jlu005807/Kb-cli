//! `IndexStore` 的 SQLite 实现。
//!
//! L1 索引层语义：把 `IndexSnapshot { records, errors, meta }` 三件套整体落库。
//! 与 JSON 版本一样，read 在"任何一个 part 缺失"时都返回 `None`——
//! SQLite 里没有 `index_meta` 行就视为索引未初始化。
//!
//! 写入策略：用事务做全量重写（DELETE + INSERT）。L1 不像 L2 那样有 chunk 级
//! 增量诉求，每次 `update` 都会产出整张快照，因此全量替换最直接也最安全。

use kb_core::domain::{IndexErrorRecord, IndexMeta, IndexRecord, IndexSnapshot};
use kb_core::error::DomainError;
use kb_core::ports::IndexStore;

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteIndexStore {
    conn: SqliteConnection,
}

impl SqliteIndexStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl IndexStore for SqliteIndexStore {
    fn read_index(&self) -> Result<Option<IndexSnapshot>, DomainError> {
        self.conn.with(|c| {
            let meta: Option<IndexMeta> = c
                .query_row(
                    "SELECT full, baseline_size, tracked_size FROM index_meta WHERE id = 1",
                    [],
                    |row| {
                        Ok(IndexMeta {
                            full: row.get::<_, i64>(0)? != 0,
                            baseline_size: row.get::<_, i64>(1)? as usize,
                            tracked_size: row.get::<_, i64>(2)? as usize,
                        })
                    },
                )
                .ok();

            let Some(meta) = meta else {
                return Ok(None);
            };

            let mut records_stmt = c
                .prepare(
                    "SELECT alias, path, category, signature, present, excerpt \
                     FROM index_records ORDER BY path ASC, alias ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare index_records failed: {err}")))?;

            let records = records_stmt
                .query_map([], |row| {
                    Ok(IndexRecord {
                        alias: row.get::<_, String>(0)?,
                        path: row.get::<_, String>(1)?,
                        category: row.get::<_, String>(2)?,
                        signature: row.get::<_, String>(3)?,
                        present: row.get::<_, i64>(4)? != 0,
                        excerpt: row.get::<_, Option<String>>(5)?,
                    })
                })
                .map_err(|err| DomainError::io(format!("query index_records failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    DomainError::io(format!("decode index_records row failed: {err}"))
                })?;

            let mut errors_stmt = c
                .prepare(
                    "SELECT alias, path, reason FROM index_errors ORDER BY path ASC, alias ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare index_errors failed: {err}")))?;
            let errors = errors_stmt
                .query_map([], |row| {
                    Ok(IndexErrorRecord {
                        alias: row.get::<_, String>(0)?,
                        path: row.get::<_, String>(1)?,
                        reason: row.get::<_, String>(2)?,
                    })
                })
                .map_err(|err| DomainError::io(format!("query index_errors failed: {err}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    DomainError::io(format!("decode index_errors row failed: {err}"))
                })?;

            Ok(Some(IndexSnapshot {
                records,
                errors,
                meta,
            }))
        })
    }

    fn write_index(&self, snapshot: &IndexSnapshot) -> Result<(), DomainError> {
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin write_index tx failed: {err}")))?;

            tx.execute("DELETE FROM index_records", [])
                .map_err(|err| DomainError::io(format!("clear index_records failed: {err}")))?;
            tx.execute("DELETE FROM index_errors", [])
                .map_err(|err| DomainError::io(format!("clear index_errors failed: {err}")))?;
            tx.execute("DELETE FROM index_meta", [])
                .map_err(|err| DomainError::io(format!("clear index_meta failed: {err}")))?;

            {
                let mut rec_stmt = tx
                    .prepare(
                        "INSERT INTO index_records (alias, path, category, signature, present, excerpt) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare index_records insert failed: {err}"))
                    })?;
                for r in &snapshot.records {
                    rec_stmt
                        .execute(rusqlite::params![
                            r.alias,
                            r.path,
                            r.category,
                            r.signature,
                            i64::from(r.present),
                            r.excerpt,
                        ])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert index_record '{}' failed: {err}",
                                r.alias
                            ))
                        })?;
                }

                let mut err_stmt = tx
                    .prepare(
                        "INSERT INTO index_errors (alias, path, reason) VALUES (?1, ?2, ?3)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare index_errors insert failed: {err}"))
                    })?;
                for e in &snapshot.errors {
                    err_stmt
                        .execute(rusqlite::params![e.alias, e.path, e.reason])
                        .map_err(|err| {
                            DomainError::io(format!(
                                "insert index_error for '{}' failed: {err}",
                                e.alias
                            ))
                        })?;
                }
            }

            tx.execute(
                "INSERT INTO index_meta (id, full, baseline_size, tracked_size) \
                 VALUES (1, ?1, ?2, ?3)",
                rusqlite::params![
                    i64::from(snapshot.meta.full),
                    snapshot.meta.baseline_size as i64,
                    snapshot.meta.tracked_size as i64,
                ],
            )
            .map_err(|err| DomainError::io(format!("insert index_meta failed: {err}")))?;

            tx.commit()
                .map_err(|err| DomainError::io(format!("commit write_index failed: {err}")))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;
    use kb_core::domain::{IndexErrorRecord, IndexMeta, IndexRecord, IndexSnapshot};

    fn fresh_store() -> SqliteIndexStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteIndexStore::new(conn)
    }

    fn rec(alias: &str, path: &str) -> IndexRecord {
        IndexRecord {
            alias: alias.to_string(),
            path: path.to_string(),
            category: "raw".to_string(),
            signature: format!("sig-{alias}"),
            present: true,
            excerpt: Some(format!("exc-{alias}")),
        }
    }

    #[test]
    fn empty_database_returns_none() {
        let store = fresh_store();
        assert_eq!(store.read_index().expect("read"), None);
    }

    #[test]
    fn write_then_read_roundtrip_preserves_records_and_meta() {
        let store = fresh_store();
        let snapshot = IndexSnapshot {
            records: vec![rec("docs/a", "./docs/a.md"), rec("docs/b", "./docs/b.md")],
            errors: vec![IndexErrorRecord {
                alias: "legacy".to_string(),
                path: "./legacy.md".to_string(),
                reason: "missing_path".to_string(),
            }],
            meta: IndexMeta {
                full: true,
                baseline_size: 2,
                tracked_size: 2,
            },
        };
        store.write_index(&snapshot).expect("write");
        let loaded = store.read_index().expect("read").expect("snapshot");
        assert_eq!(loaded.records.len(), 2);
        assert_eq!(loaded.records[0].alias, "docs/a");
        assert_eq!(loaded.records[0].excerpt.as_deref(), Some("exc-docs/a"));
        assert_eq!(loaded.errors.len(), 1);
        assert!(loaded.meta.full);
        assert_eq!(loaded.meta.tracked_size, 2);
    }

    #[test]
    fn second_write_overwrites_previous_state() {
        let store = fresh_store();
        store
            .write_index(&IndexSnapshot {
                records: vec![rec("old", "./old.md")],
                errors: vec![],
                meta: IndexMeta {
                    full: false,
                    baseline_size: 1,
                    tracked_size: 1,
                },
            })
            .expect("first write");
        store
            .write_index(&IndexSnapshot {
                records: vec![rec("new", "./new.md")],
                errors: vec![],
                meta: IndexMeta {
                    full: false,
                    baseline_size: 1,
                    tracked_size: 1,
                },
            })
            .expect("second write");
        let loaded = store.read_index().expect("read").expect("snapshot");
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].alias, "new");
    }
}
