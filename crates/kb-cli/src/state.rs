use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::errors::{map_core_error, CliError, CoreErrorKind};

// 本地引用状态持久化文件：.kb/refs.json
// 该文件是 CLI 的“当前工作集”来源，用于 list/check/update 等命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefEntry {
    pub alias: String,
    pub path: String,
    pub category: String,
    pub perms: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    pub refs: Vec<RefEntry>,
}

pub fn load_state(cwd: &Path) -> Result<State, CliError> {
    let path = cwd.join(".kb/refs.json");
    // 首次初始化后还未写入 refs.json 时，按空状态处理。
    if !path.exists() {
        return Ok(State::default());
    }
    let raw = fs::read_to_string(path)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
    serde_json::from_str(&raw)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))
}

pub fn save_state(cwd: &Path, state: &State) -> Result<(), CliError> {
    let path = cwd.join(".kb/refs.json");
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))?;
    // 原子写避免中断/崩溃导致状态文件半写入。
    kb_storage_fs::atomic_write_bytes(&path, &bytes)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, serde_json::json!({})))
}
