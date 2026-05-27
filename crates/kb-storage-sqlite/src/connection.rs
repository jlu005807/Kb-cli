//! 连接管理：打开 / 配置 / 关闭 SQLite 数据库。
//!
//! 设计：用一个 `Mutex<Connection>` 包一下 rusqlite 的 `Connection`——SQLite
//! 本身是单写者，写并发用应用层互斥锁是惯例做法。读取在 WAL 模式下可以
//! 并发，但当前实现保守地走互斥，先把正确性做对，后续按需要再放开读路径。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use kb_core::error::DomainError;

/// 进程内对 SQLite 的封装。clone 通过 `Arc` 共享同一份 `Mutex<Connection>`。
#[derive(Debug, Clone)]
pub struct SqliteConnection {
    inner: std::sync::Arc<Mutex<Connection>>,
}

impl SqliteConnection {
    /// 创建一个内存数据库（主要用于测试 / 临时仓库）。
    pub fn in_memory() -> Result<Self, DomainError> {
        let conn = Connection::open_in_memory()
            .map_err(|err| DomainError::io(format!("open in-memory sqlite failed: {err}")))?;
        configure(&conn)?;
        Ok(Self {
            inner: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    /// 以给定路径打开（不存在则创建）。
    pub fn open(path: &Path) -> Result<Self, DomainError> {
        let conn = Connection::open(path)
            .map_err(|err| DomainError::io(format!("open sqlite '{}' failed: {err}", path.display())))?;
        configure(&conn)?;
        Ok(Self {
            inner: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    /// 在内部互斥锁保护下访问连接执行闭包。
    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> Result<R, DomainError>) -> Result<R, DomainError> {
        let guard = self
            .inner
            .lock()
            .map_err(|err| DomainError::internal(format!("sqlite mutex poisoned: {err}")))?;
        f(&guard)
    }

    /// 同 [`Self::with`]，但传入 `&mut Connection` 以便启用事务/可变接口。
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<R, DomainError>,
    ) -> Result<R, DomainError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|err| DomainError::internal(format!("sqlite mutex poisoned: {err}")))?;
        f(&mut guard)
    }
}

/// 应用 PRAGMA 配置：WAL 提升并发读、foreign_keys 强约束、busy_timeout 防止锁竞争立即失败。
fn configure(conn: &Connection) -> Result<(), DomainError> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|err| DomainError::io(format!("set journal_mode failed: {err}")))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|err| DomainError::io(format!("set synchronous failed: {err}")))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| DomainError::io(format!("set foreign_keys failed: {err}")))?;
    // 5 秒内的并发请求等待而不是立即 BUSY 出错。
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|err| DomainError::io(format!("set busy_timeout failed: {err}")))?;
    Ok(())
}

/// 顶层入口：打开数据库并保证 schema 已经初始化到当前版本。
pub fn open_database(path: &Path) -> Result<SqliteConnection, DomainError> {
    let conn = SqliteConnection::open(path)?;
    crate::schema::ensure_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_round_trips_pragmas() {
        let conn = SqliteConnection::in_memory().expect("open in-memory");
        conn.with(|c| {
            let mode: String = c
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .expect("read journal_mode");
            // 内存数据库的 journal_mode 不可能切到 WAL（SQLite 不支持），
            // 但 pragma 调用本身不应失败——本 case 主要验证锁/配置链路。
            assert!(!mode.is_empty());
            Ok(())
        })
        .expect("with closure");
    }
}
