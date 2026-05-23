//! `StateStore` 的 SQLite 实现。
//!
//! 选这个 store 作为 SQLite 后端的"第一站"原因：
//! - 数据量小、schema 稳定；任何错误都更容易诊断。
//! - 行为完全同步、不涉及外部依赖（LLM 等），可以无副作用地跑测试。
//! - 是 add/mv/rm/path/chmod 路径上的最频繁读写者，能直接验证写入吞吐改进。

use kb_core::error::DomainError;
use kb_core::ports::{StateRef, StateSnapshot, StateStore};

use crate::connection::SqliteConnection;

#[derive(Debug, Clone)]
pub struct SqliteStateStore {
    conn: SqliteConnection,
}

impl SqliteStateStore {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl StateStore for SqliteStateStore {
    fn load_state(&self) -> Result<StateSnapshot, DomainError> {
        self.conn.with(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT alias, path, category, perms FROM state_refs ORDER BY alias ASC",
                )
                .map_err(|err| DomainError::io(format!("prepare load_state failed: {err}")))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(StateRef {
                        alias: row.get::<_, String>(0)?,
                        path: row.get::<_, String>(1)?,
                        category: row.get::<_, String>(2)?,
                        perms: row.get::<_, String>(3)?,
                    })
                })
                .map_err(|err| DomainError::io(format!("query state_refs failed: {err}")))?;

            let mut refs = Vec::new();
            for row in rows {
                refs.push(
                    row.map_err(|err| {
                        DomainError::io(format!("decode state_ref row failed: {err}"))
                    })?,
                );
            }
            Ok(StateSnapshot { refs })
        })
    }

    fn save_state(&self, state: &StateSnapshot) -> Result<(), DomainError> {
        // 用事务做"删旧->插新"，保证读端永远看到一致快照。
        // 对 state_refs 这种小表来说，全量重写比 diff 逻辑简单且足够快。
        self.conn.with_mut(|c| {
            let tx = c
                .transaction()
                .map_err(|err| DomainError::io(format!("begin save_state tx failed: {err}")))?;
            tx.execute("DELETE FROM state_refs", [])
                .map_err(|err| DomainError::io(format!("clear state_refs failed: {err}")))?;
            {
                let mut stmt = tx
                    .prepare(
                        "INSERT INTO state_refs (alias, path, category, perms) \
                         VALUES (?1, ?2, ?3, ?4)",
                    )
                    .map_err(|err| {
                        DomainError::io(format!("prepare state_refs insert failed: {err}"))
                    })?;
                for r in &state.refs {
                    stmt.execute(rusqlite::params![r.alias, r.path, r.category, r.perms])
                        .map_err(|err| {
                            DomainError::conflict(format!(
                                "insert state_ref '{}' failed: {err}",
                                r.alias
                            ))
                        })?;
                }
            }
            tx.commit()
                .map_err(|err| DomainError::io(format!("commit save_state failed: {err}")))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;

    fn fresh_store() -> SqliteStateStore {
        let conn = SqliteConnection::in_memory().expect("open");
        ensure_schema(&conn).expect("migrate");
        SqliteStateStore::new(conn)
    }

    #[test]
    fn empty_database_loads_empty_snapshot() {
        let store = fresh_store();
        let snap = store.load_state().expect("load");
        assert!(snap.refs.is_empty());
    }

    #[test]
    fn save_then_load_round_trips_in_alias_order() {
        let store = fresh_store();
        let snap = StateSnapshot {
            refs: vec![
                StateRef {
                    alias: "zebra".to_string(),
                    path: "./docs/z.md".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                },
                StateRef {
                    alias: "apple".to_string(),
                    path: "./docs/a.md".to_string(),
                    category: "docs".to_string(),
                    perms: "r--".to_string(),
                },
            ],
        };
        store.save_state(&snap).expect("save");
        let loaded = store.load_state().expect("load");
        // 按 alias 升序回排。
        assert_eq!(loaded.refs.len(), 2);
        assert_eq!(loaded.refs[0].alias, "apple");
        assert_eq!(loaded.refs[1].alias, "zebra");
        assert_eq!(loaded.refs[0].category, "docs");
        assert_eq!(loaded.refs[1].perms, "rwx");
    }

    #[test]
    fn save_overwrites_previous_state_atomically() {
        let store = fresh_store();
        store
            .save_state(&StateSnapshot {
                refs: vec![StateRef {
                    alias: "a".to_string(),
                    path: "./a".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                }],
            })
            .expect("first save");
        store
            .save_state(&StateSnapshot {
                refs: vec![StateRef {
                    alias: "b".to_string(),
                    path: "./b".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                }],
            })
            .expect("second save");
        let loaded = store.load_state().expect("load");
        assert_eq!(loaded.refs.len(), 1);
        assert_eq!(loaded.refs[0].alias, "b");
    }
}
