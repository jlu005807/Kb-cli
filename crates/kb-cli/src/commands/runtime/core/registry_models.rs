#[derive(Debug, Clone)]
struct DesiredEntry {
    id: String,
    alias: Option<String>,
    path: String,
    perms: String,
    category: String,
    source: String,
    exists: bool,
}

impl DesiredEntry {
    fn to_registry_entry(&self) -> RegistryEntry {
        RegistryEntry {
            id: self.id.clone(),
            alias: self.alias.clone(),
            path: self.path.clone(),
            perms: self.perms.clone(),
            source: self.source.clone(),
            category: self.category.clone(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct RegistryDiff {
    to_add: Vec<CheckResult>,
    to_update: Vec<CheckResult>,
    to_remove: Vec<CheckResult>,
    add_entries: Vec<RegistryEntry>,
    update_entries: Vec<RegistryEntry>,
    removed_paths: Vec<String>,
}

#[derive(Debug, Clone)]
enum BackendSyncAction {
    Upsert,
    Delete,
}

#[derive(Debug, Clone)]
struct BackendSyncItem {
    action: BackendSyncAction,
    entry: RegistryEntry,
}

trait BackendSyncBackend {
    fn sync(&self, items: &[BackendSyncItem]) -> Result<(), String>;
}

#[derive(Debug, Default)]
struct NoopBackendSyncBackend;

impl BackendSyncBackend for NoopBackendSyncBackend {
    fn sync(&self, items: &[BackendSyncItem]) -> Result<(), String> {
        for item in items {
            match item.action {
                BackendSyncAction::Upsert => {
                    let _ = &item.entry.path;
                }
                BackendSyncAction::Delete => {
                    let _ = &item.entry.id;
                }
            }
        }
        Ok(())
    }
}

fn build_backend_sync_backend() -> Box<dyn BackendSyncBackend> {
    Box::new(NoopBackendSyncBackend)
}

fn build_backend_sync_items(diff: &RegistryDiff) -> Vec<BackendSyncItem> {
    let mut out = Vec::new();
    for item in &diff.add_entries {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Upsert,
            entry: item.clone(),
        });
    }
    for item in &diff.update_entries {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Upsert,
            entry: item.clone(),
        });
    }
    for path in &diff.removed_paths {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Delete,
            entry: RegistryEntry {
                id: path.clone(),
                alias: None,
                path: path.clone(),
                perms: "r".to_string(),
                source: if path.starts_with(RAW_PREFIX) {
                    "raw".to_string()
                } else {
                    "kbmd".to_string()
                },
                category: "raw".to_string(),
            },
        });
    }
    out
}

fn regs_to_records(
    cwd: &Path,
    regs: &[RegistryEntry],
    ignored: &BTreeSet<String>,
    raw_only: bool,
    category: Option<&str>,
) -> Vec<IndexRecord> {
    regs.iter()
        .filter(|r| category.map(|c| r.category == c).unwrap_or(true))
        .filter(|r| !raw_only || r.path.starts_with(RAW_PREFIX))
        .filter(|r| !is_protected_index_target(cwd, &cwd.join(r.path.trim_start_matches("./"))))
        .filter(|r| !ignored.contains(&r.path))
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() || !abs.is_file() {
                return None;
            }
            Some(IndexRecord {
                alias: r.id.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: compute_signature(&abs),
                present: true,
                excerpt: compute_excerpt(&abs),
            })
        })
        .collect()
}

fn regs_to_list_records(cwd: &Path, regs: &[RegistryEntry]) -> Vec<IndexRecord> {
    regs.iter()
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() {
                return None;
            }
            Some(IndexRecord {
                alias: r.id.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: String::new(),
                present: true,
                excerpt: None,
            })
        })
        .collect()
}

