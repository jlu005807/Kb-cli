use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::errors::{map_core_error, CliError, CoreErrorKind};
use crate::paths::{normalize_path, RAW_PREFIX};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub path: String,
    #[serde(default = "default_perms")]
    pub perms: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegistryState {
    #[serde(default)]
    pub regs: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyRefEntry {
    alias: String,
    path: String,
    #[serde(default)]
    category: String,
    #[serde(default = "default_perms")]
    perms: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct LegacyState {
    #[serde(default)]
    refs: Vec<LegacyRefEntry>,
}

fn default_perms() -> String {
    "r".to_string()
}

fn default_source(path: &str) -> String {
    if path.starts_with(RAW_PREFIX) {
        "raw".to_string()
    } else {
        "kbmd".to_string()
    }
}

pub fn load_registry(cwd: &Path) -> Result<RegistryState, CliError> {
    let regs_path = cwd.join(".kb/regs.json");
    if regs_path.exists() {
        let raw = fs::read_to_string(&regs_path)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
        let mut state: RegistryState = serde_json::from_str(&raw)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
        normalize_registry_entries(&mut state.regs);
        return Ok(state);
    }

    let legacy_path = cwd.join(".kb/refs.json");
    if legacy_path.exists() {
        let raw = fs::read_to_string(&legacy_path)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
        let legacy: LegacyState = serde_json::from_str(&raw)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
        let mut regs = legacy
            .refs
            .into_iter()
            .map(|item| {
                let alias = if item.alias.trim().is_empty() {
                    None
                } else {
                    Some(item.alias.trim().to_string())
                };
                let path = normalize_path(&item.path);
                RegistryEntry {
                    id: alias.clone().unwrap_or_else(|| path.clone()),
                    alias,
                    path: path.clone(),
                    perms: item.perms,
                    source: default_source(&path),
                    category: item.category,
                }
            })
            .collect::<Vec<_>>();
        normalize_registry_entries(&mut regs);
        return Ok(RegistryState { regs });
    }

    Ok(RegistryState::default())
}

pub fn save_registry(cwd: &Path, state: &RegistryState) -> Result<(), CliError> {
    let path = cwd.join(".kb/regs.json");
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
    kb_storage_fs::atomic_write_bytes(&path, &bytes)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))
}

pub fn normalize_registry_entries(entries: &mut [RegistryEntry]) {
    for entry in entries.iter_mut() {
        let alias = entry
            .alias
            .as_deref()
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(ToString::to_string);
        entry.alias = alias;
        entry.path = normalize_path(&entry.path);
        if entry.perms.trim().is_empty() {
            entry.perms = default_perms();
        }
        if entry.source.trim().is_empty() {
            entry.source = default_source(&entry.path);
        }
        if entry.id.trim().is_empty() {
            entry.id = entry.alias.clone().unwrap_or_else(|| entry.path.clone());
        }
        if entry.category.trim().is_empty() {
            entry.category = "raw".to_string();
        }
    }
}
