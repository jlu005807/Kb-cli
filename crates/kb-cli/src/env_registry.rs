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
const LLM_CONFIG_KEY: &str = "llm";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvRegistryList {
    pub config_path: String,
    pub recorded_total: usize,
    pub existing_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalConfigSource {
    Environment,
    ExecutableDirectory,
    HomeDirectory,
}

impl GlobalConfigSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "KB_GLOBAL_CONFIG",
            Self::ExecutableDirectory => "executable_directory",
            Self::HomeDirectory => "home_directory",
        }
    }

    pub(crate) fn rule_number(self) -> u8 {
        match self {
            Self::Environment => 1,
            Self::ExecutableDirectory => 2,
            Self::HomeDirectory => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobalConfigSelection {
    pub path: PathBuf,
    pub source: GlobalConfigSource,
}

pub(crate) fn select_global_config_path(
    env_override: Option<&str>,
    exe_candidate: Option<&Path>,
    exe_candidate_exists: bool,
    home: Option<&Path>,
) -> GlobalConfigSelection {
    if let Some(path) = env_override.filter(|path| !path.trim().is_empty()) {
        return GlobalConfigSelection {
            path: PathBuf::from(path),
            source: GlobalConfigSource::Environment,
        };
    }

    if exe_candidate_exists {
        if let Some(path) = exe_candidate {
            return GlobalConfigSelection {
                path: path.to_path_buf(),
                source: GlobalConfigSource::ExecutableDirectory,
            };
        }
    }

    GlobalConfigSelection {
        path: home
            .unwrap_or_else(|| Path::new("."))
            .join(DEFAULT_GLOBAL_CONFIG_DIR)
            .join(DEFAULT_GLOBAL_CONFIG_FILE),
        source: GlobalConfigSource::HomeDirectory,
    }
}

pub fn register_kb_root(kb_root: &Path) -> Result<(), String> {
    let config_path = global_config_path();
    let mut config = read_global_config(&config_path);
    ensure_global_config_shape(&mut config);

    let root_abs = fs::canonicalize(kb_root).unwrap_or_else(|_| kb_root.to_path_buf());
    let root = normalize_registry_path(&root_abs);

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

pub(crate) fn resolve_global_config_selection() -> GlobalConfigSelection {
    let env_override = std::env::var(GLOBAL_CONFIG_ENV).ok();
    let exe_candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| executable_config_candidate(&exe));
    let exe_candidate_exists = exe_candidate
        .as_ref()
        .map(|path| path.is_file())
        .unwrap_or(false);
    let home = user_home_dir();
    select_global_config_path(
        env_override.as_deref(),
        exe_candidate.as_deref(),
        exe_candidate_exists,
        home.as_deref(),
    )
}

pub(crate) fn resolve_global_config_path() -> PathBuf {
    resolve_global_config_selection().path
}

fn global_config_path() -> PathBuf {
    resolve_global_config_selection().path
}

fn executable_config_candidate(exe: &Path) -> Option<PathBuf> {
    let exe_dir = exe.parent()?;

    let dir_name = exe_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase());
    if matches!(dir_name.as_deref(), Some("bin")) {
        if let Some(root_dir) = exe_dir.parent() {
            return Some(root_dir.join(DEFAULT_GLOBAL_CONFIG_FILE));
        }
    }

    Some(exe_dir.join(DEFAULT_GLOBAL_CONFIG_FILE))
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

fn ensure_global_config_shape(config: &mut Value) {
    if !config.is_object() {
        *config = json!({});
    }

    if !config
        .get(LLM_CONFIG_KEY)
        .map(Value::is_object)
        .unwrap_or(false)
    {
        config[LLM_CONFIG_KEY] = json!({});
    }

    let Some(llm) = config
        .get_mut(LLM_CONFIG_KEY)
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    if !llm.contains_key("enable_semantic") {
        llm.insert("enable_semantic".to_string(), Value::Bool(false));
    }
    if !llm.contains_key("enable_graph") {
        llm.insert("enable_graph".to_string(), Value::Bool(false));
    }
    if !llm.contains_key("enable_wiki") {
        llm.insert("enable_wiki".to_string(), Value::Bool(false));
    }
    if !llm.contains_key("model") {
        llm.insert(
            "model".to_string(),
            Value::String("gpt-4o-mini".to_string()),
        );
    }
    if !llm.contains_key("base_url") {
        llm.insert(
            "base_url".to_string(),
            Value::String("https://api.openai.com/v1".to_string()),
        );
    }
    if !llm.contains_key("semantic_flow") {
        llm.insert(
            "semantic_flow".to_string(),
            Value::String("two_step".to_string()),
        );
    }
    if !llm.contains_key("timeout_secs") {
        llm.insert("timeout_secs".to_string(), json!(60));
    }
    if !llm.contains_key("temperature") {
        llm.insert("temperature".to_string(), json!(0.1));
    }
    if !llm.contains_key("max_output_tokens") {
        llm.insert("max_output_tokens".to_string(), json!(900));
    }
    if !llm.contains_key("analysis_max_output_tokens") {
        llm.insert("analysis_max_output_tokens".to_string(), json!(700));
    }
    if !llm.contains_key("max_input_chars") {
        llm.insert("max_input_chars".to_string(), json!(12_000));
    }
    if !llm.contains_key("max_chunks") {
        llm.insert("max_chunks".to_string(), json!(12));
    }
    if !llm.contains_key("retry_max_attempts") {
        llm.insert("retry_max_attempts".to_string(), json!(3));
    }
    if !llm.contains_key("retry_backoff_ms") {
        llm.insert("retry_backoff_ms".to_string(), json!(600));
    }
    if !llm.contains_key("agent_loop_max_rounds") {
        llm.insert("agent_loop_max_rounds".to_string(), json!(2));
    }
    if !llm.contains_key("ask_loop_max_rounds") {
        llm.insert("ask_loop_max_rounds".to_string(), json!(8));
    }
    if !llm.contains_key("agent_reflection_enabled") {
        llm.insert("agent_reflection_enabled".to_string(), Value::Bool(true));
    }
    if !llm.contains_key("agent_min_chunk_coverage") {
        llm.insert("agent_min_chunk_coverage".to_string(), json!(0.6));
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
    let mut out = Vec::new();
    if let Some(arr) = value.get(REGISTRY_KEY).and_then(Value::as_array) {
        for raw in arr.iter().filter_map(Value::as_str) {
            let normalized = normalize_registry_path_str(raw);
            if !out.iter().any(|existing| existing == &normalized) {
                out.push(normalized);
            }
        }
    }
    out
}

fn normalize_registry_path(path: &Path) -> String {
    normalize_registry_path_str(&path.to_string_lossy())
}

fn normalize_registry_path_str(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = path.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }

    path.to_string()
}

fn is_existing_kb_root(path: &Path) -> bool {
    path.exists() && path.is_dir() && path.join("kb.md").exists() && path.join(".kb").exists()
}

#[cfg(test)]
mod tests {
    use super::{
        executable_config_candidate, normalize_registry_path_str, select_global_config_path,
        GlobalConfigSource,
    };
    use std::path::Path;

    #[test]
    fn global_config_selection_uses_environment_override_first() {
        let selected = select_global_config_path(
            Some("custom/global.json"),
            Some(Path::new("bin/config.json")),
            true,
            Some(Path::new("home")),
        );

        assert_eq!(selected.path, Path::new("custom/global.json"));
        assert_eq!(selected.source, GlobalConfigSource::Environment);
    }

    #[test]
    fn global_config_selection_uses_existing_executable_candidate() {
        let selected = select_global_config_path(
            None,
            Some(Path::new("install/config.json")),
            true,
            Some(Path::new("home")),
        );

        assert_eq!(selected.path, Path::new("install/config.json"));
        assert_eq!(selected.source, GlobalConfigSource::ExecutableDirectory);
    }

    #[test]
    fn global_config_selection_falls_back_to_home_when_executable_config_is_absent() {
        let selected = select_global_config_path(
            None,
            Some(Path::new("target/debug/config.json")),
            false,
            Some(Path::new("home")),
        );

        assert_eq!(selected.path, Path::new("home/.kb-cli/config.json"));
        assert_eq!(selected.source, GlobalConfigSource::HomeDirectory);
    }

    #[test]
    fn blank_environment_override_is_ignored() {
        let selected = select_global_config_path(
            Some("  "),
            Some(Path::new("target/debug/config.json")),
            false,
            Some(Path::new("home")),
        );

        assert_eq!(selected.path, Path::new("home/.kb-cli/config.json"));
        assert_eq!(selected.source, GlobalConfigSource::HomeDirectory);
    }

    #[test]
    fn executable_candidate_uses_install_root_for_bin_directory() {
        let executable = Path::new("install").join("bin").join("kb.exe");

        assert_eq!(
            executable_config_candidate(&executable),
            Some(Path::new("install").join("config.json"))
        );
    }

    #[test]
    fn executable_candidate_uses_executable_directory_outside_bin() {
        let executable = Path::new("target").join("debug").join("kb-cli.exe");

        assert_eq!(
            executable_config_candidate(&executable),
            Some(Path::new("target").join("debug").join("config.json"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn normalize_windows_verbatim_disk_prefix() {
        assert_eq!(
            normalize_registry_path_str(r"\\?\C:\kb\demo"),
            r"C:\kb\demo"
        );
    }

    #[test]
    #[cfg(windows)]
    fn normalize_windows_verbatim_unc_prefix() {
        assert_eq!(
            normalize_registry_path_str(r"\\?\UNC\server\share\kb"),
            r"\\server\share\kb"
        );
    }

    #[test]
    fn normalize_keeps_regular_path() {
        let path = if cfg!(windows) {
            r"C:\kb\demo"
        } else {
            "/tmp/kb/demo"
        };
        assert_eq!(normalize_registry_path_str(path), path);
    }
}
