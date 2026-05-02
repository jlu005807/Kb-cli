use serde_json::Value;
use std::fs;
use std::path::Path;

const KB_LLM_KEY_ENV: &str = "KB_LLM_KEY";
const KB_LLM_MODEL_ENV: &str = "KB_LLM_MODEL";
const KB_LLM_BASE_URL_ENV: &str = "KB_LLM_BASE_URL";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmConfigSource {
    ProcessEnv,
    LocalDotEnv,
    LocalConfig,
    GlobalDotEnv,
    GlobalConfig,
    Defaults,
}

impl LlmConfigSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessEnv => "process_env",
            Self::LocalDotEnv => "kb_local_env",
            Self::LocalConfig => "kb_local_config",
            Self::GlobalDotEnv => "global_env",
            Self::GlobalConfig => "global_config",
            Self::Defaults => "defaults",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmConfigSourceMap {
    pub key: LlmConfigSource,
    pub model: LlmConfigSource,
    pub base_url: LlmConfigSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRuntimeConfig {
    pub key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub source: LlmConfigSourceMap,
    pub trace: String,
}

#[derive(Debug, Default, Clone)]
struct LlmConfigPartial {
    key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
}

pub fn resolve_llm_config(cwd: &Path) -> LlmRuntimeConfig {
    let process_env = read_process_env();
    let local_env = load_dotenv_config(&cwd.join(".kb/.env"));
    let local_config = load_json_config(&cwd.join(".kb/config.json"));

    let global_config_path = crate::env_registry::resolve_global_config_path();
    let global_env = global_config_path
        .parent()
        .map(|parent| load_dotenv_config(&parent.join(".env")))
        .unwrap_or_default();
    let global_config = load_json_config(&global_config_path);

    resolve_from_layers(
        process_env,
        local_env,
        local_config,
        global_env,
        global_config,
    )
}

fn resolve_from_layers(
    process_env: LlmConfigPartial,
    local_env: LlmConfigPartial,
    local_config: LlmConfigPartial,
    global_env: LlmConfigPartial,
    global_config: LlmConfigPartial,
) -> LlmRuntimeConfig {
    let (key, key_source) = pick_optional(&[
        (&process_env.key, LlmConfigSource::ProcessEnv),
        (&local_env.key, LlmConfigSource::LocalDotEnv),
        (&local_config.key, LlmConfigSource::LocalConfig),
        (&global_env.key, LlmConfigSource::GlobalDotEnv),
        (&global_config.key, LlmConfigSource::GlobalConfig),
    ]);
    let (model, model_source) = pick_required(
        &[
            (&process_env.model, LlmConfigSource::ProcessEnv),
            (&local_env.model, LlmConfigSource::LocalDotEnv),
            (&local_config.model, LlmConfigSource::LocalConfig),
            (&global_env.model, LlmConfigSource::GlobalDotEnv),
            (&global_config.model, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_MODEL,
    );
    let (base_url, base_url_source) = pick_required(
        &[
            (&process_env.base_url, LlmConfigSource::ProcessEnv),
            (&local_env.base_url, LlmConfigSource::LocalDotEnv),
            (&local_config.base_url, LlmConfigSource::LocalConfig),
            (&global_env.base_url, LlmConfigSource::GlobalDotEnv),
            (&global_config.base_url, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_BASE_URL,
    );

    let source = LlmConfigSourceMap {
        key: key_source,
        model: model_source,
        base_url: base_url_source,
    };
    let trace = format!(
        "key={}; model={}; base_url={}",
        source.key.as_str(),
        source.model.as_str(),
        source.base_url.as_str()
    );

    LlmRuntimeConfig {
        key,
        model,
        base_url,
        source,
        trace,
    }
}

fn read_process_env() -> LlmConfigPartial {
    LlmConfigPartial {
        key: normalize_value(std::env::var(KB_LLM_KEY_ENV).ok()),
        model: normalize_value(std::env::var(KB_LLM_MODEL_ENV).ok()),
        base_url: normalize_value(std::env::var(KB_LLM_BASE_URL_ENV).ok()),
    }
}

fn pick_optional(candidates: &[(&Option<String>, LlmConfigSource)]) -> (Option<String>, LlmConfigSource) {
    for (value, source) in candidates {
        if let Some(v) = value.as_deref().and_then(|x| normalize_value(Some(x.to_string()))) {
            return (Some(v), *source);
        }
    }
    (None, LlmConfigSource::Defaults)
}

fn pick_required(candidates: &[(&Option<String>, LlmConfigSource)], default_value: &str) -> (String, LlmConfigSource) {
    for (value, source) in candidates {
        if let Some(v) = value.as_deref().and_then(|x| normalize_value(Some(x.to_string()))) {
            return (v, *source);
        }
    }
    (default_value.to_string(), LlmConfigSource::Defaults)
}

fn load_dotenv_config(path: &Path) -> LlmConfigPartial {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return LlmConfigPartial::default(),
    };

    let mut key = None;
    let mut model = None;
    let mut base_url = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((left, right)) = trimmed.split_once('=') else {
            continue;
        };

        let name = left.trim().trim_start_matches("export ").trim();
        let value = parse_dotenv_value(right);
        match name {
            KB_LLM_KEY_ENV => key = normalize_value(Some(value)),
            KB_LLM_MODEL_ENV => model = normalize_value(Some(value)),
            KB_LLM_BASE_URL_ENV => base_url = normalize_value(Some(value)),
            _ => {}
        }
    }

    LlmConfigPartial {
        key,
        model,
        base_url,
    }
}

fn parse_dotenv_value(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_inline_comment = strip_inline_comment(trimmed);
    if let Some(inner) = strip_wrapped_quotes(without_inline_comment) {
        return inner.to_string();
    }
    without_inline_comment.to_string()
}

fn strip_wrapped_quotes(raw: &str) -> Option<&str> {
    if raw.len() < 2 {
        return None;
    }
    let bytes = raw.as_bytes();
    if (bytes[0] == b'"' && bytes[raw.len() - 1] == b'"')
        || (bytes[0] == b'\'' && bytes[raw.len() - 1] == b'\'')
    {
        return Some(&raw[1..raw.len() - 1]);
    }
    None
}

fn strip_inline_comment(raw: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;

    for (idx, ch) in raw.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return raw[..idx].trim_end(),
            _ => {}
        }
    }
    raw
}

fn load_json_config(path: &Path) -> LlmConfigPartial {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return LlmConfigPartial::default(),
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return LlmConfigPartial::default(),
    };

    let llm_obj = parsed.get("llm");
    let key = llm_obj
        .and_then(|v| v.get("key"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get(KB_LLM_KEY_ENV)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });
    let model = llm_obj
        .and_then(|v| v.get("model"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get(KB_LLM_MODEL_ENV)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });
    let base_url = llm_obj
        .and_then(|v| v.get("base_url"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get(KB_LLM_BASE_URL_ENV)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });

    LlmConfigPartial {
        key: normalize_value(key),
        model: normalize_value(model),
        base_url: normalize_value(base_url),
    }
}

fn normalize_value(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        load_dotenv_config, load_json_config, parse_dotenv_value, resolve_from_layers, LlmConfigPartial,
        LlmConfigSource, DEFAULT_BASE_URL, DEFAULT_MODEL,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn picks_process_env_with_highest_precedence() {
        let resolved = resolve_from_layers(
            LlmConfigPartial {
                key: Some("env-key".to_string()),
                model: Some("env-model".to_string()),
                base_url: Some("https://env.example/v1".to_string()),
            },
            LlmConfigPartial {
                key: Some("local-key".to_string()),
                model: Some("local-model".to_string()),
                base_url: Some("https://local.example/v1".to_string()),
            },
            LlmConfigPartial::default(),
            LlmConfigPartial {
                key: Some("global-env-key".to_string()),
                model: Some("global-env-model".to_string()),
                base_url: Some("https://global-env.example/v1".to_string()),
            },
            LlmConfigPartial {
                key: Some("global-config-key".to_string()),
                model: Some("global-config-model".to_string()),
                base_url: Some("https://global-config.example/v1".to_string()),
            },
        );

        assert_eq!(resolved.key.as_deref(), Some("env-key"));
        assert_eq!(resolved.model, "env-model");
        assert_eq!(resolved.base_url, "https://env.example/v1");
        assert_eq!(resolved.source.key, LlmConfigSource::ProcessEnv);
        assert_eq!(resolved.source.model, LlmConfigSource::ProcessEnv);
        assert_eq!(resolved.source.base_url, LlmConfigSource::ProcessEnv);
    }

    #[test]
    fn falls_back_to_local_env_before_global_layers() {
        let resolved = resolve_from_layers(
            LlmConfigPartial::default(),
            LlmConfigPartial {
                key: Some("local-key".to_string()),
                model: Some("local-model".to_string()),
                base_url: Some("https://local.example/v1".to_string()),
            },
            LlmConfigPartial {
                key: Some("local-config-key".to_string()),
                model: Some("local-config-model".to_string()),
                base_url: Some("https://local-config.example/v1".to_string()),
            },
            LlmConfigPartial {
                key: Some("global-env-key".to_string()),
                model: Some("global-env-model".to_string()),
                base_url: Some("https://global-env.example/v1".to_string()),
            },
            LlmConfigPartial {
                key: Some("global-config-key".to_string()),
                model: Some("global-config-model".to_string()),
                base_url: Some("https://global-config.example/v1".to_string()),
            },
        );

        assert_eq!(resolved.key.as_deref(), Some("local-key"));
        assert_eq!(resolved.model, "local-model");
        assert_eq!(resolved.base_url, "https://local.example/v1");
        assert_eq!(resolved.source.key, LlmConfigSource::LocalDotEnv);
        assert_eq!(resolved.source.model, LlmConfigSource::LocalDotEnv);
        assert_eq!(resolved.source.base_url, LlmConfigSource::LocalDotEnv);
    }

    #[test]
    fn falls_back_to_defaults_when_no_values_present() {
        let resolved = resolve_from_layers(
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
        );

        assert!(resolved.key.is_none());
        assert_eq!(resolved.model, DEFAULT_MODEL);
        assert_eq!(resolved.base_url, DEFAULT_BASE_URL);
        assert_eq!(resolved.source.key, LlmConfigSource::Defaults);
        assert_eq!(resolved.source.model, LlmConfigSource::Defaults);
        assert_eq!(resolved.source.base_url, LlmConfigSource::Defaults);
    }

    #[test]
    fn dotenv_parser_handles_comments_and_quotes() {
        assert_eq!(parse_dotenv_value("  plain-value  # note"), "plain-value");
        assert_eq!(parse_dotenv_value("' quoted # keep '"), " quoted # keep ");
        assert_eq!(parse_dotenv_value("\"quoted # keep\""), "quoted # keep");
        assert_eq!(parse_dotenv_value("\"quoted\" # note"), "quoted");
    }

    #[test]
    fn dotenv_loader_reads_llm_keys() {
        let dir = unique_temp_dir("llm-dotenv");
        let env_path = dir.join(".env");
        fs::write(
            &env_path,
            "KB_LLM_KEY=dotenv-key\nKB_LLM_MODEL=dotenv-model\nKB_LLM_BASE_URL=https://dotenv.example/v1\n",
        )
        .expect("write .env");

        let loaded = load_dotenv_config(&env_path);
        assert_eq!(loaded.key.as_deref(), Some("dotenv-key"));
        assert_eq!(loaded.model.as_deref(), Some("dotenv-model"));
        assert_eq!(
            loaded.base_url.as_deref(),
            Some("https://dotenv.example/v1")
        );
    }

    #[test]
    fn json_loader_reads_nested_llm_block() {
        let dir = unique_temp_dir("llm-json");
        let config_path = dir.join("config.json");
        fs::write(
            &config_path,
            r#"{
  "llm": {
    "key": "json-key",
    "model": "json-model",
    "base_url": "https://json.example/v1"
  }
}"#,
        )
        .expect("write config");

        let loaded = load_json_config(&config_path);
        assert_eq!(loaded.key.as_deref(), Some("json-key"));
        assert_eq!(loaded.model.as_deref(), Some("json-model"));
        assert_eq!(
            loaded.base_url.as_deref(),
            Some("https://json.example/v1")
        );
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kb_cli_llm_config_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
