//! 索引快照模型与差异引擎。
//! 提供三类核心能力：
//! - `update`：生成下一版快照
//! - `check`：只读差异预览
//! - `render_list`：面向展示的列表视图

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRecord {
    pub alias: String,
    pub path: String,
    pub category: String,
    pub signature: String,
    pub present: bool,
    #[serde(default)]
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexErrorRecord {
    pub alias: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexMeta {
    pub full: bool,
    pub baseline_size: usize,
    pub tracked_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub records: Vec<IndexRecord>,
    pub errors: Vec<IndexErrorRecord>,
    pub meta: IndexMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOutcome {
    pub snapshot: IndexSnapshot,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub marked_invalid: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub alias: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub to_add: Vec<CheckResult>,
    pub to_update: Vec<CheckResult>,
    pub to_remove: Vec<CheckResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexConfig {
    pub ignored_paths: BTreeSet<String>,
    pub prune: bool,
    pub full: bool,
}

impl IndexConfig {
    pub fn new(ignored_paths: BTreeSet<String>, prune: bool, full: bool) -> Self {
        Self {
            ignored_paths,
            prune,
            full,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub alias: String,
    pub path: String,
    pub category: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListView {
    pub items: Vec<ListItem>,
    pub categories: BTreeMap<String, Vec<ListItem>>,
}

#[derive(Debug, Default, Clone)]
pub struct IndexEngine;

impl IndexEngine {
    // update：基于 baseline 与当前 refs 计算下一个快照与统计信息。
    // - path 作为身份键
    // - prune=false 时缺失路径标记为 present=false 并写入 errors
    pub fn update(
        &self,
        baseline: Option<&IndexSnapshot>,
        refs: &[IndexRecord],
        config: &IndexConfig,
    ) -> UpdateOutcome {
        let mut next_records = refs
            .iter()
            .filter(|record| !config.ignored_paths.contains(&record.path))
            .cloned()
            .collect::<Vec<_>>();
        next_records.sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));

        let baseline_map = baseline
            .map(|snapshot| {
                snapshot
                    .records
                    .iter()
                    .map(|record| (record.path.clone(), record.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        let next_map = next_records
            .iter()
            .map(|record| (record.path.clone(), record.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut added = 0;
        let mut updated = 0;
        let mut removed = 0;
        let mut marked_invalid = 0;

        for (path, record) in &next_map {
            match baseline_map.get(path) {
                None => {
                    if record.present {
                        added += 1;
                    }
                }
                Some(prev) => {
                    if prev.signature != record.signature {
                        updated += 1;
                    }
                }
            }
        }

        let mut output_records = next_records;
        let mut errors = Vec::new();
        for (path, prev) in &baseline_map {
            if config.ignored_paths.contains(path) || next_map.contains_key(path) {
                continue;
            }
            if config.prune {
                removed += 1;
            } else {
                marked_invalid += 1;
                output_records.push(IndexRecord {
                    alias: prev.alias.clone(),
                    path: prev.path.clone(),
                    category: prev.category.clone(),
                    signature: prev.signature.clone(),
                    present: false,
                    excerpt: prev.excerpt.clone(),
                });
                errors.push(IndexErrorRecord {
                    alias: prev.alias.clone(),
                    path: prev.path.clone(),
                    reason: "missing_path".to_string(),
                });
            }
        }

        output_records.sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));
        errors.sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));

        let snapshot = IndexSnapshot {
            records: output_records,
            errors,
            meta: IndexMeta {
                full: config.full,
                baseline_size: baseline_map.len(),
                tracked_size: next_map.len(),
            },
        };

        UpdateOutcome {
            snapshot,
            added,
            updated,
            removed,
            marked_invalid,
        }
    }

    // check：仅做差异预览，不产出落盘快照。
    pub fn check(
        &self,
        baseline: Option<&IndexSnapshot>,
        refs: &[IndexRecord],
        ignored_paths: &BTreeSet<String>,
    ) -> CheckOutcome {
        let update_preview = self.update(
            baseline,
            refs,
            &IndexConfig::new(ignored_paths.clone(), false, false),
        );

        let baseline_map = baseline
            .map(|snapshot| {
                snapshot
                    .records
                    .iter()
                    .map(|record| (record.path.clone(), record.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        let next_map = refs
            .iter()
            .filter(|record| !ignored_paths.contains(&record.path))
            .map(|record| (record.path.clone(), record.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut to_add = Vec::new();
        let mut to_update = Vec::new();
        let mut to_remove = Vec::new();

        for (path, record) in &next_map {
            match baseline_map.get(path) {
                None => {
                    to_add.push(CheckResult {
                        alias: record.alias.clone(),
                        path: record.path.clone(),
                        reason: "new_path".to_string(),
                    });
                }
                Some(prev) => {
                    if prev.signature != record.signature {
                        to_update.push(CheckResult {
                            alias: record.alias.clone(),
                            path: record.path.clone(),
                            reason: "content_changed".to_string(),
                        });
                    }
                }
            }
        }

        for (path, prev) in &baseline_map {
            if ignored_paths.contains(path) || next_map.contains_key(path) {
                continue;
            }
            to_remove.push(CheckResult {
                alias: prev.alias.clone(),
                path: prev.path.clone(),
                reason: "missing_path".to_string(),
            });
        }

        debug_assert_eq!(update_preview.marked_invalid, to_remove.len());
        CheckOutcome {
            to_add,
            to_update,
            to_remove,
        }
    }

    // render_list：从快照生成可展示列表，并可按简化 glob 过滤。
    pub fn render_list(
        &self,
        snapshot: Option<&IndexSnapshot>,
        filter: Option<&str>,
    ) -> Result<ListView, crate::error::DomainError> {
        let mut items = snapshot
            .map(|x| x.records.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|record| record.present)
            .map(|record| ListItem {
                alias: record.alias,
                path: record.path,
                category: record.category,
                present: record.present,
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));

        if let Some(pattern) = filter {
            if pattern.is_empty() || pattern.contains('[') || pattern.contains(']') {
                return Err(crate::error::DomainError::validation(
                    "invalid glob pattern",
                ));
            }
            items.retain(|item| {
                glob_like_match(pattern, &item.alias) || glob_like_match(pattern, &item.path)
            });
        }

        let mut categories: BTreeMap<String, Vec<ListItem>> = BTreeMap::new();
        for item in &items {
            categories
                .entry(item.category.clone())
                .or_default()
                .push(item.clone());
        }

        Ok(ListView { items, categories })
    }
}

fn glob_like_match(pattern: &str, candidate: &str) -> bool {
    glob_match_bytes(pattern.as_bytes(), candidate.as_bytes())
}

// 轻量 glob 匹配（* 和 ?），用于 list/filter 与 ignore 子集场景。
fn glob_match_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star_idx = Some(p);
            match_idx = t;
            p += 1;
        } else if let Some(star) = star_idx {
            p = star + 1;
            match_idx += 1;
            t = match_idx;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }

    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{glob_like_match, IndexConfig, IndexEngine, IndexRecord, IndexSnapshot, ListView};
    use std::collections::BTreeSet;

    fn rec(alias: &str, path: &str, category: &str, signature: &str, present: bool) -> IndexRecord {
        IndexRecord {
            alias: alias.to_string(),
            path: path.to_string(),
            category: category.to_string(),
            signature: signature.to_string(),
            present,
            excerpt: None,
        }
    }

    #[test]
    fn update_is_idempotent_for_same_inputs() {
        let engine = IndexEngine;
        let refs = vec![rec("docs", "./docs/a.md", "raw", "s1", true)];
        let config = IndexConfig::new(BTreeSet::new(), false, false);

        let first = engine.update(None, &refs, &config);
        assert_eq!(first.added, 1);
        let second = engine.update(Some(&first.snapshot), &refs, &config);
        assert_eq!(second.added, 0);
        assert_eq!(second.updated, 0);
        assert_eq!(second.removed, 0);
        assert_eq!(second.marked_invalid, 0);
    }

    #[test]
    fn update_marks_missing_or_removes_with_prune() {
        let engine = IndexEngine;
        let baseline = IndexSnapshot {
            records: vec![rec("legacy", "./legacy.md", "raw", "s1", true)],
            errors: vec![],
            meta: super::IndexMeta {
                full: false,
                baseline_size: 1,
                tracked_size: 1,
            },
        };

        let marked = engine.update(
            Some(&baseline),
            &[],
            &IndexConfig::new(BTreeSet::new(), false, false),
        );
        assert_eq!(marked.marked_invalid, 1);
        assert_eq!(marked.removed, 0);

        let pruned = engine.update(
            Some(&baseline),
            &[],
            &IndexConfig::new(BTreeSet::new(), true, false),
        );
        assert_eq!(pruned.marked_invalid, 0);
        assert_eq!(pruned.removed, 1);
    }

    #[test]
    fn list_filter_works_with_glob_like_patterns() {
        let engine = IndexEngine;
        let snapshot = IndexSnapshot {
            records: vec![
                rec("docs/a", "./docs/a.md", "raw", "s1", true),
                rec("src/main", "./src/main.rs", "code", "s2", true),
            ],
            errors: vec![],
            meta: super::IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 2,
            },
        };

        let filtered = engine
            .render_list(Some(&snapshot), Some("docs/*"))
            .expect("filter should work");
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].alias, "docs/a");
    }

    #[test]
    fn glob_match_supports_wildcards() {
        assert!(glob_like_match("docs/*", "docs/a"));
        assert!(glob_like_match("*.md", "file.md"));
        assert!(!glob_like_match("docs/*", "src/a"));
    }

    #[test]
    fn list_invalid_filter_rejected() {
        let engine = IndexEngine;
        let out: Result<ListView, crate::error::DomainError> = engine.render_list(None, Some(""));
        assert!(out.is_err());
    }
}
