//! `EpisodeStore` 的文件系统实现：append-only 的 `.kb/episodes.jsonl`。
//!
//! 为什么用 JSONL 而不是单个 JSON 数组：
//! - **真 append**：新 episode 直接 `O_APPEND` 写一行，不需要把已有全部读出
//!   再整体重写。这正是 append-only 语义要的——写入成本 O(1) 而非 O(N)。
//! - **崩溃安全**：进程在写一行中途挂掉，最多损坏最后一行；读取时跳过
//!   解析失败的行即可，已有历史不受影响。整体 JSON 数组做不到这点
//!   （半写入会让整个文件 parse 失败）。
//! - **可 grep / 可 tail**：每行自包含，运维和调试友好。
//!
//! 并发：append 用 `OpenOptions::append(true)` 让系统选择追加位置。
//! 普通文件写入不受管道 PIPE_BUF 的原子性保证；读取按行跳过损坏记录。

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use kb_core::domain::{Episode, EpisodeFilter};
use kb_core::error::DomainError;
use kb_core::ports::EpisodeStore;

#[derive(Debug, Clone)]
pub struct FsEpisodeStore {
    episodes_path: PathBuf,
}

impl FsEpisodeStore {
    pub fn under(base_dir: &Path) -> Self {
        Self {
            episodes_path: base_dir.join(".kb").join("episodes.jsonl"),
        }
    }

    pub fn new(base_dir: PathBuf) -> Self {
        Self::under(&base_dir)
    }

    /// 读取并解析所有 episode，按文件内出现顺序；查询时再按时间排序。
    /// 解析失败的行被跳过（崩溃残留行容错），不会让整个读取失败。
    fn read_all(&self) -> Result<Vec<Episode>, DomainError> {
        if !self.episodes_path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read(&self.episodes_path).map_err(|err| {
            DomainError::io(format!(
                "read episodes.jsonl '{}' failed: {err}",
                self.episodes_path.display()
            ))
        })?;
        let mut out = Vec::new();
        for line in raw.split(|byte| *byte == b'\n') {
            // 容错：跳过解析失败的行（例如崩溃残留的半写入行）。
            if let Ok(ep) = serde_json::from_slice::<Episode>(line) {
                out.push(ep);
            }
        }
        Ok(out)
    }
}

impl EpisodeStore for FsEpisodeStore {
    fn append(&self, episode: &Episode) -> Result<(), DomainError> {
        if let Some(parent) = self.episodes_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                DomainError::io(format!(
                    "create episodes dir '{}' failed: {err}",
                    parent.display()
                ))
            })?;
        }
        let mut line = serde_json::to_string(episode).map_err(|err| {
            DomainError::serialization(format!("serialize episode '{}' failed: {err}", episode.id))
        })?;
        line.push('\n');

        let mut file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.episodes_path)
            .map_err(|err| {
                DomainError::io(format!(
                    "open episodes.jsonl '{}' for append failed: {err}",
                    self.episodes_path.display()
                ))
            })?;
        if file
            .metadata()
            .map_err(|err| {
                DomainError::io(format!(
                    "inspect episodes.jsonl '{}' before append failed: {err}",
                    self.episodes_path.display()
                ))
            })?
            .len()
            > 0
        {
            file.seek(SeekFrom::End(-1)).map_err(|err| {
                DomainError::io(format!(
                    "seek episodes.jsonl '{}' before append failed: {err}",
                    self.episodes_path.display()
                ))
            })?;
            let mut last_byte = [0u8; 1];
            file.read_exact(&mut last_byte).map_err(|err| {
                DomainError::io(format!(
                    "read episodes.jsonl '{}' tail before append failed: {err}",
                    self.episodes_path.display()
                ))
            })?;
            if last_byte[0] != b'\n' {
                file.write_all(b"\n").map_err(|err| {
                    DomainError::io(format!(
                        "terminate final line in episodes.jsonl '{}' failed: {err}",
                        self.episodes_path.display()
                    ))
                })?;
            }
        }
        file.write_all(line.as_bytes()).map_err(|err| {
            DomainError::io(format!(
                "append episode '{}' to '{}' failed: {err}",
                episode.id,
                self.episodes_path.display()
            ))
        })?;
        Ok(())
    }

    fn query(&self, filter: &EpisodeFilter) -> Result<Vec<Episode>, DomainError> {
        let all = self.read_all()?;
        let mut matched: Vec<Episode> = all.into_iter().filter(|ep| filter.matches(ep)).collect();
        matched.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
        // limit 取最近 N 条 = 取时间排序后的尾部，仍保持升序。
        if let Some(limit) = filter.limit {
            if matched.len() > limit {
                matched = matched.split_off(matched.len() - limit);
            }
        }
        Ok(matched)
    }

    fn count(&self, filter: &EpisodeFilter) -> Result<usize, DomainError> {
        let all = self.read_all()?;
        Ok(all.iter().filter(|ep| filter.matches(ep)).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_core::domain::{AgentId, EpisodeKind, Timestamp};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nonce));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn ep(id: &str, agent: &str, session: &str, kind: EpisodeKind, ts: u64) -> Episode {
        Episode::new(id, AgentId::new(agent).unwrap(), session, kind, "content")
            .with_timestamp(Timestamp(ts))
    }

    #[test]
    fn empty_store_returns_no_episodes() {
        let dir = unique_test_dir("kb-fs-ep-empty");
        let store = FsEpisodeStore::under(&dir);
        assert!(store.query(&EpisodeFilter::default()).unwrap().is_empty());
        assert_eq!(store.count(&EpisodeFilter::default()).unwrap(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn append_then_query_round_trips_in_order() {
        let dir = unique_test_dir("kb-fs-ep-order");
        let store = FsEpisodeStore::under(&dir);
        store
            .append(&ep("e1", "a", "s", EpisodeKind::Query, 1))
            .unwrap();
        store
            .append(&ep("e2", "a", "s", EpisodeKind::Answer, 2))
            .unwrap();
        store
            .append(&ep("e3", "b", "t", EpisodeKind::Observation, 3))
            .unwrap();

        let all = store.query(&EpisodeFilter::default()).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "e1");
        assert_eq!(all[1].id, "e2");
        assert_eq!(all[2].id, "e3");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn append_is_truly_additive_across_calls() {
        let dir = unique_test_dir("kb-fs-ep-additive");
        let store = FsEpisodeStore::under(&dir);
        for i in 0..5 {
            store
                .append(&ep(&format!("e{i}"), "a", "s", EpisodeKind::Query, i))
                .unwrap();
        }
        // 重新打开一个 store 指向同目录，确认历史持久化。
        let reopened = FsEpisodeStore::under(&dir);
        assert_eq!(reopened.count(&EpisodeFilter::default()).unwrap(), 5);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn append_separates_new_episode_from_truncated_final_line() {
        for tail in [
            b"{\"id\":\"half".as_slice(),
            b"{\"content\":\"\xe4\xb8".as_slice(),
        ] {
            let dir = unique_test_dir("kb-fs-ep-truncated-tail");
            let store = FsEpisodeStore::under(&dir);
            let path = dir.join(".kb").join("episodes.jsonl");
            store
                .append(&ep("before", "a", "s", EpisodeKind::Observation, 0))
                .unwrap();
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(tail)
                .unwrap();

            store
                .append(&ep("good", "a", "s", EpisodeKind::Observation, 1))
                .unwrap();

            let episodes = store.query(&EpisodeFilter::default()).unwrap();
            assert_eq!(
                episodes
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                ["before", "good"]
            );
            assert_eq!(store.count(&EpisodeFilter::default()).unwrap(), 2);
            fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn query_filters_by_agent() {
        let dir = unique_test_dir("kb-fs-ep-agent");
        let store = FsEpisodeStore::under(&dir);
        store
            .append(&ep("e1", "cursor", "s", EpisodeKind::Query, 1))
            .unwrap();
        store
            .append(&ep("e2", "claude", "s", EpisodeKind::Query, 2))
            .unwrap();

        let filter = EpisodeFilter {
            agent_id: Some(AgentId::new("cursor").unwrap()),
            ..Default::default()
        };
        let out = store.query(&filter).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "e1");
        assert_eq!(store.count(&filter).unwrap(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn query_limit_returns_most_recent_in_order() {
        let dir = unique_test_dir("kb-fs-ep-limit");
        let store = FsEpisodeStore::under(&dir);
        for i in [5, 1, 4, 2, 3] {
            store
                .append(&ep(&format!("e{i}"), "a", "s", EpisodeKind::Query, i))
                .unwrap();
        }
        let filter = EpisodeFilter {
            limit: Some(2),
            ..Default::default()
        };
        let out = store.query(&filter).unwrap();
        // 取最近 2 条（e4, e5），仍按时间升序。
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "e4");
        assert_eq!(out[1].id, "e5");
        // count 忽略 limit。
        assert_eq!(store.count(&filter).unwrap(), 5);
        let all = store.query(&EpisodeFilter::default()).unwrap();
        assert_eq!(
            all.iter()
                .map(|item| item.timestamp.as_nanos())
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(store
            .query(&EpisodeFilter {
                limit: Some(0),
                ..Default::default()
            })
            .unwrap()
            .is_empty());
        store
            .append(&ep("e0", "a", "s", EpisodeKind::Query, 4))
            .unwrap();
        let tied = store
            .query(&EpisodeFilter {
                limit: Some(3),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            tied.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["e0", "e4", "e5"]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupted_line_is_skipped_not_fatal() {
        let dir = unique_test_dir("kb-fs-ep-corrupt");
        let store = FsEpisodeStore::under(&dir);
        store
            .append(&ep("good1", "a", "s", EpisodeKind::Query, 1))
            .unwrap();
        // 手动追加一行损坏 JSON（模拟崩溃残留）。
        let path = dir.join(".kb").join("episodes.jsonl");
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{ this is not valid json\n").unwrap();
        drop(f);
        store
            .append(&ep("good2", "a", "s", EpisodeKind::Answer, 2))
            .unwrap();

        let out = store.query(&EpisodeFilter::default()).unwrap();
        // 损坏行被跳过，两条好记录都在。
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "good1");
        assert_eq!(out[1].id, "good2");
        fs::remove_dir_all(&dir).ok();
    }
}
