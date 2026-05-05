use serde_json::Value;
use std::fs;
use std::path::Path;

const KB_LLM_KEY_ENV: &str = "KB_LLM_KEY";
const KB_LLM_MODEL_ENV: &str = "KB_LLM_MODEL";
const KB_LLM_BASE_URL_ENV: &str = "KB_LLM_BASE_URL";
const KB_LLM_ENABLE_SEMANTIC_ENV: &str = "KB_LLM_ENABLE_SEMANTIC";
const KB_LLM_TIMEOUT_SECS_ENV: &str = "KB_LLM_TIMEOUT_SECS";
const KB_LLM_TEMPERATURE_ENV: &str = "KB_LLM_TEMPERATURE";
const KB_LLM_MAX_OUTPUT_TOKENS_ENV: &str = "KB_LLM_MAX_OUTPUT_TOKENS";
const KB_LLM_MAX_INPUT_CHARS_ENV: &str = "KB_LLM_MAX_INPUT_CHARS";
const KB_LLM_MAX_CHUNKS_ENV: &str = "KB_LLM_MAX_CHUNKS";
const KB_LLM_SEMANTIC_FLOW_ENV: &str = "KB_LLM_SEMANTIC_FLOW";
const KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV: &str = "KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS";
const KB_LLM_RETRY_MAX_ATTEMPTS_ENV: &str = "KB_LLM_RETRY_MAX_ATTEMPTS";
const KB_LLM_RETRY_BACKOFF_MS_ENV: &str = "KB_LLM_RETRY_BACKOFF_MS";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_ENABLE_SEMANTIC: bool = false;
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEMPERATURE: f64 = 0.1;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 900;
const DEFAULT_MAX_INPUT_CHARS: usize = 12_000;
const DEFAULT_MAX_CHUNKS: usize = 12;
const DEFAULT_SEMANTIC_FLOW: &str = "two_step";
const DEFAULT_ANALYSIS_MAX_OUTPUT_TOKENS: u32 = 700;
const DEFAULT_RETRY_MAX_ATTEMPTS: usize = 3;
const DEFAULT_RETRY_BACKOFF_MS: u64 = 600;

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

#[derive(Debug, Clone, PartialEq)]
pub struct LlmRuntimeConfig {
    pub key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub semantic_enabled: bool,
    pub semantic_flow: String,
    pub timeout_secs: u64,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub analysis_max_output_tokens: u32,
    pub max_input_chars: usize,
    pub max_chunks: usize,
    pub retry_max_attempts: usize,
    pub retry_backoff_ms: u64,
    pub source: LlmConfigSourceMap,
    pub trace: String,
}

#[derive(Debug, Default, Clone)]
struct LlmConfigPartial {
    key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    semantic_enabled: Option<bool>,
    semantic_flow: Option<String>,
    timeout_secs: Option<u64>,
    temperature: Option<f64>,
    max_output_tokens: Option<u32>,
    analysis_max_output_tokens: Option<u32>,
    max_input_chars: Option<usize>,
    max_chunks: Option<usize>,
    retry_max_attempts: Option<usize>,
    retry_backoff_ms: Option<u64>,
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
    let semantic_enabled = pick_bool(
        &[
            (&process_env.semantic_enabled, LlmConfigSource::ProcessEnv),
            (&local_env.semantic_enabled, LlmConfigSource::LocalDotEnv),
            (&local_config.semantic_enabled, LlmConfigSource::LocalConfig),
            (&global_env.semantic_enabled, LlmConfigSource::GlobalDotEnv),
            (
                &global_config.semantic_enabled,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_ENABLE_SEMANTIC,
    );
    let semantic_flow = normalize_semantic_flow(
        &pick_required(
            &[
                (&process_env.semantic_flow, LlmConfigSource::ProcessEnv),
                (&local_env.semantic_flow, LlmConfigSource::LocalDotEnv),
                (&local_config.semantic_flow, LlmConfigSource::LocalConfig),
                (&global_env.semantic_flow, LlmConfigSource::GlobalDotEnv),
                (&global_config.semantic_flow, LlmConfigSource::GlobalConfig),
            ],
            DEFAULT_SEMANTIC_FLOW,
        )
        .0,
    );
    let timeout_secs = pick_u64(
        &[
            (&process_env.timeout_secs, LlmConfigSource::ProcessEnv),
            (&local_env.timeout_secs, LlmConfigSource::LocalDotEnv),
            (&local_config.timeout_secs, LlmConfigSource::LocalConfig),
            (&global_env.timeout_secs, LlmConfigSource::GlobalDotEnv),
            (&global_config.timeout_secs, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_TIMEOUT_SECS,
    )
    .max(5);
    let temperature = pick_f64(
        &[
            (&process_env.temperature, LlmConfigSource::ProcessEnv),
            (&local_env.temperature, LlmConfigSource::LocalDotEnv),
            (&local_config.temperature, LlmConfigSource::LocalConfig),
            (&global_env.temperature, LlmConfigSource::GlobalDotEnv),
            (&global_config.temperature, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_TEMPERATURE,
    )
    .clamp(0.0, 2.0);
    let max_output_tokens = pick_u32(
        &[
            (&process_env.max_output_tokens, LlmConfigSource::ProcessEnv),
            (&local_env.max_output_tokens, LlmConfigSource::LocalDotEnv),
            (
                &local_config.max_output_tokens,
                LlmConfigSource::LocalConfig,
            ),
            (&global_env.max_output_tokens, LlmConfigSource::GlobalDotEnv),
            (
                &global_config.max_output_tokens,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_MAX_OUTPUT_TOKENS,
    )
    .max(64);
    let analysis_max_output_tokens = pick_u32(
        &[
            (
                &process_env.analysis_max_output_tokens,
                LlmConfigSource::ProcessEnv,
            ),
            (
                &local_env.analysis_max_output_tokens,
                LlmConfigSource::LocalDotEnv,
            ),
            (
                &local_config.analysis_max_output_tokens,
                LlmConfigSource::LocalConfig,
            ),
            (
                &global_env.analysis_max_output_tokens,
                LlmConfigSource::GlobalDotEnv,
            ),
            (
                &global_config.analysis_max_output_tokens,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_ANALYSIS_MAX_OUTPUT_TOKENS,
    )
    .max(64);
    let max_input_chars = pick_usize(
        &[
            (&process_env.max_input_chars, LlmConfigSource::ProcessEnv),
            (&local_env.max_input_chars, LlmConfigSource::LocalDotEnv),
            (&local_config.max_input_chars, LlmConfigSource::LocalConfig),
            (&global_env.max_input_chars, LlmConfigSource::GlobalDotEnv),
            (
                &global_config.max_input_chars,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_MAX_INPUT_CHARS,
    )
    .max(1200);
    let max_chunks = pick_usize(
        &[
            (&process_env.max_chunks, LlmConfigSource::ProcessEnv),
            (&local_env.max_chunks, LlmConfigSource::LocalDotEnv),
            (&local_config.max_chunks, LlmConfigSource::LocalConfig),
            (&global_env.max_chunks, LlmConfigSource::GlobalDotEnv),
            (&global_config.max_chunks, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_MAX_CHUNKS,
    )
    .max(1);
    let retry_max_attempts = pick_usize(
        &[
            (&process_env.retry_max_attempts, LlmConfigSource::ProcessEnv),
            (&local_env.retry_max_attempts, LlmConfigSource::LocalDotEnv),
            (
                &local_config.retry_max_attempts,
                LlmConfigSource::LocalConfig,
            ),
            (
                &global_env.retry_max_attempts,
                LlmConfigSource::GlobalDotEnv,
            ),
            (
                &global_config.retry_max_attempts,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_RETRY_MAX_ATTEMPTS,
    )
    .clamp(1, 8);
    let retry_backoff_ms = pick_u64(
        &[
            (&process_env.retry_backoff_ms, LlmConfigSource::ProcessEnv),
            (&local_env.retry_backoff_ms, LlmConfigSource::LocalDotEnv),
            (&local_config.retry_backoff_ms, LlmConfigSource::LocalConfig),
            (&global_env.retry_backoff_ms, LlmConfigSource::GlobalDotEnv),
            (
                &global_config.retry_backoff_ms,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_RETRY_BACKOFF_MS,
    )
    .min(30_000);

    let source = LlmConfigSourceMap {
        key: key_source,
        model: model_source,
        base_url: base_url_source,
    };
    let trace = format!(
        "key={}; model={}; base_url={}; semantic_enabled={}; semantic_flow={}; timeout_secs={}; temperature={}; max_output_tokens={}; analysis_max_output_tokens={}; max_input_chars={}; max_chunks={}; retry_max_attempts={}; retry_backoff_ms={}",
        source.key.as_str(),
        source.model.as_str(),
        source.base_url.as_str(),
        semantic_enabled,
        semantic_flow,
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms
    );

    LlmRuntimeConfig {
        key,
        model,
        base_url,
        semantic_enabled,
        semantic_flow,
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
        source,
        trace,
    }
}

fn read_process_env() -> LlmConfigPartial {
    LlmConfigPartial {
        key: normalize_value(std::env::var(KB_LLM_KEY_ENV).ok()),
        model: normalize_value(std::env::var(KB_LLM_MODEL_ENV).ok()),
        base_url: normalize_value(std::env::var(KB_LLM_BASE_URL_ENV).ok()),
        semantic_enabled: std::env::var(KB_LLM_ENABLE_SEMANTIC_ENV)
            .ok()
            .and_then(|value| parse_bool(&value)),
        semantic_flow: normalize_value(std::env::var(KB_LLM_SEMANTIC_FLOW_ENV).ok()),
        timeout_secs: std::env::var(KB_LLM_TIMEOUT_SECS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok()),
        temperature: std::env::var(KB_LLM_TEMPERATURE_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok()),
        max_output_tokens: std::env::var(KB_LLM_MAX_OUTPUT_TOKENS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok()),
        analysis_max_output_tokens: std::env::var(KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok()),
        max_input_chars: std::env::var(KB_LLM_MAX_INPUT_CHARS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
        max_chunks: std::env::var(KB_LLM_MAX_CHUNKS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
        retry_max_attempts: std::env::var(KB_LLM_RETRY_MAX_ATTEMPTS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
        retry_backoff_ms: std::env::var(KB_LLM_RETRY_BACKOFF_MS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok()),
    }
}

fn pick_optional(
    candidates: &[(&Option<String>, LlmConfigSource)],
) -> (Option<String>, LlmConfigSource) {
    for (value, source) in candidates {
        if let Some(v) = value
            .as_deref()
            .and_then(|x| normalize_value(Some(x.to_string())))
        {
            return (Some(v), *source);
        }
    }
    (None, LlmConfigSource::Defaults)
}

fn pick_required(
    candidates: &[(&Option<String>, LlmConfigSource)],
    default_value: &str,
) -> (String, LlmConfigSource) {
    for (value, source) in candidates {
        if let Some(v) = value
            .as_deref()
            .and_then(|x| normalize_value(Some(x.to_string())))
        {
            return (v, *source);
        }
    }
    (default_value.to_string(), LlmConfigSource::Defaults)
}

fn pick_bool(candidates: &[(&Option<bool>, LlmConfigSource)], default_value: bool) -> bool {
    candidates
        .iter()
        .find_map(|(value, _source)| **value)
        .unwrap_or(default_value)
}

fn pick_u64(candidates: &[(&Option<u64>, LlmConfigSource)], default_value: u64) -> u64 {
    candidates
        .iter()
        .find_map(|(value, _source)| **value)
        .unwrap_or(default_value)
}

fn pick_u32(candidates: &[(&Option<u32>, LlmConfigSource)], default_value: u32) -> u32 {
    candidates
        .iter()
        .find_map(|(value, _source)| **value)
        .unwrap_or(default_value)
}

fn pick_usize(candidates: &[(&Option<usize>, LlmConfigSource)], default_value: usize) -> usize {
    candidates
        .iter()
        .find_map(|(value, _source)| **value)
        .unwrap_or(default_value)
}

fn pick_f64(candidates: &[(&Option<f64>, LlmConfigSource)], default_value: f64) -> f64 {
    candidates
        .iter()
        .find_map(|(value, _source)| **value)
        .unwrap_or(default_value)
}

fn load_dotenv_config(path: &Path) -> LlmConfigPartial {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return LlmConfigPartial::default(),
    };

    let mut key = None;
    let mut model = None;
    let mut base_url = None;
    let mut semantic_enabled = None;
    let mut semantic_flow = None;
    let mut timeout_secs = None;
    let mut temperature = None;
    let mut max_output_tokens = None;
    let mut analysis_max_output_tokens = None;
    let mut max_input_chars = None;
    let mut max_chunks = None;
    let mut retry_max_attempts = None;
    let mut retry_backoff_ms = None;

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
            KB_LLM_ENABLE_SEMANTIC_ENV => semantic_enabled = parse_bool(&value),
            KB_LLM_SEMANTIC_FLOW_ENV => semantic_flow = normalize_value(Some(value)),
            KB_LLM_TIMEOUT_SECS_ENV => timeout_secs = value.trim().parse::<u64>().ok(),
            KB_LLM_TEMPERATURE_ENV => temperature = value.trim().parse::<f64>().ok(),
            KB_LLM_MAX_OUTPUT_TOKENS_ENV => max_output_tokens = value.trim().parse::<u32>().ok(),
            KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV => {
                analysis_max_output_tokens = value.trim().parse::<u32>().ok()
            }
            KB_LLM_MAX_INPUT_CHARS_ENV => max_input_chars = value.trim().parse::<usize>().ok(),
            KB_LLM_MAX_CHUNKS_ENV => max_chunks = value.trim().parse::<usize>().ok(),
            KB_LLM_RETRY_MAX_ATTEMPTS_ENV => {
                retry_max_attempts = value.trim().parse::<usize>().ok()
            }
            KB_LLM_RETRY_BACKOFF_MS_ENV => retry_backoff_ms = value.trim().parse::<u64>().ok(),
            _ => {}
        }
    }

    LlmConfigPartial {
        key,
        model,
        base_url,
        semantic_enabled,
        semantic_flow,
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
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
    let semantic_enabled = llm_obj
        .and_then(|v| v.get("enable_semantic"))
        .and_then(Value::as_bool)
        .or_else(|| {
            parsed
                .get(KB_LLM_ENABLE_SEMANTIC_ENV)
                .and_then(Value::as_bool)
        });
    let semantic_flow = llm_obj
        .and_then(|v| v.get("semantic_flow"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get(KB_LLM_SEMANTIC_FLOW_ENV)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });
    let timeout_secs = llm_obj
        .and_then(|v| v.get("timeout_secs"))
        .and_then(Value::as_u64)
        .or_else(|| parsed.get(KB_LLM_TIMEOUT_SECS_ENV).and_then(Value::as_u64));
    let temperature = llm_obj
        .and_then(|v| v.get("temperature"))
        .and_then(Value::as_f64)
        .or_else(|| parsed.get(KB_LLM_TEMPERATURE_ENV).and_then(Value::as_f64));
    let max_output_tokens = llm_obj
        .and_then(|v| v.get("max_output_tokens"))
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(KB_LLM_MAX_OUTPUT_TOKENS_ENV)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
        });
    let analysis_max_output_tokens = llm_obj
        .and_then(|v| v.get("analysis_max_output_tokens"))
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
        });
    let max_input_chars = llm_obj
        .and_then(|v| v.get("max_input_chars"))
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(KB_LLM_MAX_INPUT_CHARS_ENV)
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
        });
    let max_chunks = llm_obj
        .and_then(|v| v.get("max_chunks"))
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(KB_LLM_MAX_CHUNKS_ENV)
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
        });
    let retry_max_attempts = llm_obj
        .and_then(|v| v.get("retry_max_attempts"))
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(KB_LLM_RETRY_MAX_ATTEMPTS_ENV)
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
        });
    let retry_backoff_ms = llm_obj
        .and_then(|v| v.get("retry_backoff_ms"))
        .and_then(Value::as_u64)
        .or_else(|| {
            parsed
                .get(KB_LLM_RETRY_BACKOFF_MS_ENV)
                .and_then(Value::as_u64)
        });

    LlmConfigPartial {
        key: normalize_value(key),
        model: normalize_value(model),
        base_url: normalize_value(base_url),
        semantic_enabled,
        semantic_flow: normalize_value(semantic_flow),
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
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

fn normalize_semantic_flow(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "single" | "single_step" | "single-step" | "single_pass" | "single-pass" => {
            "single_step".to_string()
        }
        "two" | "two_step" | "two-step" | "dual_step" | "dual-step" => "two_step".to_string(),
        _ => DEFAULT_SEMANTIC_FLOW.to_string(),
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        load_dotenv_config, load_json_config, parse_dotenv_value, resolve_from_layers,
        LlmConfigPartial, LlmConfigSource, DEFAULT_ANALYSIS_MAX_OUTPUT_TOKENS, DEFAULT_BASE_URL,
        DEFAULT_ENABLE_SEMANTIC, DEFAULT_MAX_CHUNKS, DEFAULT_MAX_INPUT_CHARS,
        DEFAULT_MAX_OUTPUT_TOKENS, DEFAULT_MODEL, DEFAULT_RETRY_BACKOFF_MS,
        DEFAULT_RETRY_MAX_ATTEMPTS, DEFAULT_SEMANTIC_FLOW, DEFAULT_TEMPERATURE,
        DEFAULT_TIMEOUT_SECS,
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
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial {
                key: Some("local-key".to_string()),
                model: Some("local-model".to_string()),
                base_url: Some("https://local.example/v1".to_string()),
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial::default(),
            LlmConfigPartial {
                key: Some("global-env-key".to_string()),
                model: Some("global-env-model".to_string()),
                base_url: Some("https://global-env.example/v1".to_string()),
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial {
                key: Some("global-config-key".to_string()),
                model: Some("global-config-model".to_string()),
                base_url: Some("https://global-config.example/v1".to_string()),
                ..LlmConfigPartial::default()
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
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial {
                key: Some("local-config-key".to_string()),
                model: Some("local-config-model".to_string()),
                base_url: Some("https://local-config.example/v1".to_string()),
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial {
                key: Some("global-env-key".to_string()),
                model: Some("global-env-model".to_string()),
                base_url: Some("https://global-env.example/v1".to_string()),
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial {
                key: Some("global-config-key".to_string()),
                model: Some("global-config-model".to_string()),
                base_url: Some("https://global-config.example/v1".to_string()),
                ..LlmConfigPartial::default()
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
        assert_eq!(resolved.semantic_enabled, DEFAULT_ENABLE_SEMANTIC);
        assert_eq!(resolved.semantic_flow, DEFAULT_SEMANTIC_FLOW);
        assert_eq!(resolved.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert_eq!(resolved.temperature, DEFAULT_TEMPERATURE);
        assert_eq!(resolved.max_output_tokens, DEFAULT_MAX_OUTPUT_TOKENS);
        assert_eq!(
            resolved.analysis_max_output_tokens,
            DEFAULT_ANALYSIS_MAX_OUTPUT_TOKENS
        );
        assert_eq!(resolved.max_input_chars, DEFAULT_MAX_INPUT_CHARS);
        assert_eq!(resolved.max_chunks, DEFAULT_MAX_CHUNKS);
        assert_eq!(resolved.retry_max_attempts, DEFAULT_RETRY_MAX_ATTEMPTS);
        assert_eq!(resolved.retry_backoff_ms, DEFAULT_RETRY_BACKOFF_MS);
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
    fn dotenv_loader_reads_semantic_tuning_keys() {
        let dir = unique_temp_dir("llm-dotenv-semantic");
        let env_path = dir.join(".env");
        fs::write(
            &env_path,
            "KB_LLM_ENABLE_SEMANTIC=true\nKB_LLM_SEMANTIC_FLOW=single-step\nKB_LLM_TIMEOUT_SECS=77\nKB_LLM_TEMPERATURE=0.3\nKB_LLM_MAX_OUTPUT_TOKENS=1234\nKB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS=555\nKB_LLM_MAX_INPUT_CHARS=9999\nKB_LLM_MAX_CHUNKS=7\nKB_LLM_RETRY_MAX_ATTEMPTS=4\nKB_LLM_RETRY_BACKOFF_MS=1400\n",
        )
        .expect("write .env");

        let loaded = load_dotenv_config(&env_path);
        assert_eq!(loaded.semantic_enabled, Some(true));
        assert_eq!(loaded.semantic_flow.as_deref(), Some("single-step"));
        assert_eq!(loaded.timeout_secs, Some(77));
        assert_eq!(loaded.temperature, Some(0.3));
        assert_eq!(loaded.max_output_tokens, Some(1234));
        assert_eq!(loaded.analysis_max_output_tokens, Some(555));
        assert_eq!(loaded.max_input_chars, Some(9999));
        assert_eq!(loaded.max_chunks, Some(7));
        assert_eq!(loaded.retry_max_attempts, Some(4));
        assert_eq!(loaded.retry_backoff_ms, Some(1400));
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
    "base_url": "https://json.example/v1",
    "enable_semantic": true,
    "semantic_flow": "single_step",
    "timeout_secs": 42,
    "temperature": 0.25,
    "max_output_tokens": 1500,
    "analysis_max_output_tokens": 700,
    "max_input_chars": 8000,
    "max_chunks": 9,
    "retry_max_attempts": 5,
    "retry_backoff_ms": 1200
  }
}"#,
        )
        .expect("write config");

        let loaded = load_json_config(&config_path);
        assert_eq!(loaded.key.as_deref(), Some("json-key"));
        assert_eq!(loaded.model.as_deref(), Some("json-model"));
        assert_eq!(loaded.base_url.as_deref(), Some("https://json.example/v1"));
        assert_eq!(loaded.semantic_enabled, Some(true));
        assert_eq!(loaded.semantic_flow.as_deref(), Some("single_step"));
        assert_eq!(loaded.timeout_secs, Some(42));
        assert_eq!(loaded.temperature, Some(0.25));
        assert_eq!(loaded.max_output_tokens, Some(1500));
        assert_eq!(loaded.analysis_max_output_tokens, Some(700));
        assert_eq!(loaded.max_input_chars, Some(8000));
        assert_eq!(loaded.max_chunks, Some(9));
        assert_eq!(loaded.retry_max_attempts, Some(5));
        assert_eq!(loaded.retry_backoff_ms, Some(1200));
    }

    #[test]
    fn resolve_normalizes_semantic_flow_and_retry_bounds() {
        let resolved = resolve_from_layers(
            LlmConfigPartial {
                semantic_flow: Some("single-pass".to_string()),
                retry_max_attempts: Some(99),
                retry_backoff_ms: Some(99_999),
                analysis_max_output_tokens: Some(12),
                ..LlmConfigPartial::default()
            },
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
            LlmConfigPartial::default(),
        );

        assert_eq!(resolved.semantic_flow, "single_step");
        assert_eq!(resolved.retry_max_attempts, 8);
        assert_eq!(resolved.retry_backoff_ms, 30_000);
        assert_eq!(resolved.analysis_max_output_tokens, 64);
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
