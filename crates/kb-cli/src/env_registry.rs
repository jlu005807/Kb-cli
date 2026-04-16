//! 全局知识库注册表管理。
//! 目标：
//! - 在 `kb init` 后登记知识库根目录
//! - 支持 `kb env --list` 列出“已记录且仍存在”的知识库路径

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const GLOBAL_CONFIG_ENV: &str = "KB_GLOBAL_CONFIG";
const DEFAULT_GLOBAL_CONFIG_DIR: &str = ".kb-cli";
const DEFAULT_GLOBAL_CONFIG_FILE: &str = "config.json";
const REGISTRY_KEY: &str = "knowledge_bases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvRegistryList {
    pub config_path: String,
    pub recorded_total: usize,
    pub existing_paths: Vec<String>,
}

pub fn register_kb_root(kb_root: &Path) -> Result<(), String> {
    let config_path = global_config_path();
    let mut config = read_global_config(&config_path);
    if !config.is_object() {
        config = json!({});
    }

    let root_abs = fs::canonicalize(kb_root).unwrap_or_else(|_| kb_root.to_path_buf());
    let root = root_abs.to_string_lossy().to_string();

    let mut paths = read_recorded_paths(&config);
    if !paths.iter().any(|p| p == &root) {
        paths.push(root);
    }
    config[REGISTRY_KEY] = Value::Array(paths.into_iter().map(Value::String).collect());
    write_global_config(&config_path, &config)
}

pub fn list_existing_kb_roots() -> EnvRegistryList {
    let config_path = global_config_path();
    let config = read_global_config(&config_path);
    if !config.is_object() {
        return EnvRegistryList {
            config_path: config_path.to_string_lossy().to_string(),
            recorded_total: 0,
            existing_paths: vec![],
        };
    }

    let recorded = read_recorded_paths(&config);
    let existing_paths = recorded
        .iter()
        .filter(|path| is_existing_kb_root(Path::new(path)))
        .cloned()
        .collect::<Vec<_>>();

    EnvRegistryList {
        config_path: config_path.to_string_lossy().to_string(),
        recorded_total: recorded.len(),
        existing_paths,
    }
}

fn global_config_path() -> PathBuf {
    if let Ok(path) = std::env::var(GLOBAL_CONFIG_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    user_home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_GLOBAL_CONFIG_DIR)
        .join(DEFAULT_GLOBAL_CONFIG_FILE)
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn read_global_config(path: &Path) -> Value {
    if !path.exists() {
        return json!({});
    }

    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return json!({}),
    };
    match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => json!({}),
    }
}

fn write_global_config(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create global config dir '{}': {}",
                parent.display(),
                err
            )
        })?;
    }

    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("failed to serialize global config: {}", err))?;
    bytes.push(b'\n');

    kb_storage_fs::atomic_write_bytes(path, &bytes).map_err(|err| {
        format!(
            "failed to write global config '{}': {}",
            path.display(),
            err
        )
    })
}

fn read_recorded_paths(value: &Value) -> Vec<String> {
    value
        .get(REGISTRY_KEY)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn is_existing_kb_root(path: &Path) -> bool {
    path.exists() && path.is_dir() && path.join("kb.md").exists() && path.join(".kb").exists()
}
