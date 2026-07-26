use serde_json::Value;
use std::fs;
use std::path::Path;

const KB_LLM_KEY_ENV: &str = "KB_LLM_KEY";
const KB_LLM_MODEL_ENV: &str = "KB_LLM_MODEL";
const KB_LLM_BASE_URL_ENV: &str = "KB_LLM_BASE_URL";
const KB_LLM_ENABLE_SEMANTIC_ENV: &str = "KB_LLM_ENABLE_SEMANTIC";
const KB_LLM_ENABLE_GRAPH_ENV: &str = "KB_LLM_ENABLE_GRAPH";
const KB_LLM_ENABLE_WIKI_ENV: &str = "KB_LLM_ENABLE_WIKI";
const KB_LLM_TIMEOUT_SECS_ENV: &str = "KB_LLM_TIMEOUT_SECS";
const KB_LLM_TEMPERATURE_ENV: &str = "KB_LLM_TEMPERATURE";
const KB_LLM_MAX_OUTPUT_TOKENS_ENV: &str = "KB_LLM_MAX_OUTPUT_TOKENS";
const KB_LLM_MAX_INPUT_CHARS_ENV: &str = "KB_LLM_MAX_INPUT_CHARS";
const KB_LLM_MAX_CHUNKS_ENV: &str = "KB_LLM_MAX_CHUNKS";
const KB_LLM_SEMANTIC_FLOW_ENV: &str = "KB_LLM_SEMANTIC_FLOW";
const KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV: &str = "KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS";
const KB_LLM_RETRY_MAX_ATTEMPTS_ENV: &str = "KB_LLM_RETRY_MAX_ATTEMPTS";
const KB_LLM_RETRY_BACKOFF_MS_ENV: &str = "KB_LLM_RETRY_BACKOFF_MS";
const KB_LLM_AGENT_LOOP_MAX_ROUNDS_ENV: &str = "KB_LLM_AGENT_LOOP_MAX_ROUNDS";
const KB_LLM_ASK_LOOP_MAX_ROUNDS_ENV: &str = "KB_LLM_ASK_LOOP_MAX_ROUNDS";
const KB_LLM_AGENT_REFLECTION_ENABLED_ENV: &str = "KB_LLM_AGENT_REFLECTION_ENABLED";
const KB_LLM_AGENT_MIN_CHUNK_COVERAGE_ENV: &str = "KB_LLM_AGENT_MIN_CHUNK_COVERAGE";

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_ENABLE_SEMANTIC: bool = false;
const DEFAULT_ENABLE_GRAPH: bool = false;
const DEFAULT_ENABLE_WIKI: bool = false;
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_TEMPERATURE: f64 = 0.1;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 900;
const DEFAULT_MAX_INPUT_CHARS: usize = 12_000;
const DEFAULT_MAX_CHUNKS: usize = 12;
const DEFAULT_SEMANTIC_FLOW: &str = "two_step";
const DEFAULT_ANALYSIS_MAX_OUTPUT_TOKENS: u32 = 700;
const DEFAULT_RETRY_MAX_ATTEMPTS: usize = 3;
const DEFAULT_RETRY_BACKOFF_MS: u64 = 600;
const DEFAULT_AGENT_LOOP_MAX_ROUNDS: usize = 2;
// ask agent 的工具循环与 semantic/graph/wiki 的反思循环量级不同：
// 证据步态（search -> summary -> raw -> graph）至少需要 4 轮，故独立成配置项，
// 消费端（kb ask --answer）再 clamp 到 [4, 12]。
const DEFAULT_ASK_LOOP_MAX_ROUNDS: usize = 8;
const DEFAULT_AGENT_REFLECTION_ENABLED: bool = true;
const DEFAULT_AGENT_MIN_CHUNK_COVERAGE: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmConfigSource {
    ProcessEnv,
    /// 系统密钥环（Windows Credential Manager / macOS Keychain / freedesktop Secret
    /// Service）。仅用于敏感字段（当前只有 `key`）；其他字段不会走 keyring。
    Keyring,
    ProjectConfig,
    GlobalConfig,
    Defaults,
}

impl LlmConfigSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessEnv => "process_env",
            Self::Keyring => "keyring",
            Self::ProjectConfig => "project_config",
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
    pub graph_enabled: bool,
    pub wiki_enabled: bool,
    pub semantic_flow: String,
    pub timeout_secs: u64,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub analysis_max_output_tokens: u32,
    pub max_input_chars: usize,
    pub max_chunks: usize,
    pub retry_max_attempts: usize,
    pub retry_backoff_ms: u64,
    pub agent_loop_max_rounds: usize,
    pub ask_loop_max_rounds: usize,
    pub agent_reflection_enabled: bool,
    pub agent_min_chunk_coverage: f64,
    pub source: LlmConfigSourceMap,
    pub trace: String,
}

#[derive(Debug, Default, Clone)]
struct LlmConfigPartial {
    key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    semantic_enabled: Option<bool>,
    graph_enabled: Option<bool>,
    wiki_enabled: Option<bool>,
    semantic_flow: Option<String>,
    timeout_secs: Option<u64>,
    temperature: Option<f64>,
    max_output_tokens: Option<u32>,
    analysis_max_output_tokens: Option<u32>,
    max_input_chars: Option<usize>,
    max_chunks: Option<usize>,
    retry_max_attempts: Option<usize>,
    retry_backoff_ms: Option<u64>,
    agent_loop_max_rounds: Option<usize>,
    ask_loop_max_rounds: Option<usize>,
    agent_reflection_enabled: Option<bool>,
    agent_min_chunk_coverage: Option<f64>,
}

pub fn resolve_llm_config(project_root: &Path, global_config_path: &Path) -> LlmRuntimeConfig {
    let process_env = read_process_env();
    let keyring_key = load_key_from_keyring();
    let project_config = load_json_config(&project_root.join(".kb/config.json"));
    let global_config = load_json_config(global_config_path);
    resolve_from_layers(process_env, keyring_key, project_config, global_config)
}

/// 默认 keyring service 名（可被 `KB_LLM_KEYRING_SERVICE` 覆盖，便于多环境隔离）。
const DEFAULT_KEYRING_SERVICE: &str = "kb-cli";
/// 默认 keyring username — 单用户场景下保持稳定即可。
const DEFAULT_KEYRING_USER: &str = "default";

const KB_LLM_KEYRING_SERVICE_ENV: &str = "KB_LLM_KEYRING_SERVICE";
const KB_LLM_KEYRING_USER_ENV: &str = "KB_LLM_KEYRING_USER";
const KB_LLM_DISABLE_KEYRING_ENV: &str = "KB_LLM_DISABLE_KEYRING";

fn resolve_keyring_service() -> String {
    std::env::var(KB_LLM_KEYRING_SERVICE_ENV)
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| DEFAULT_KEYRING_SERVICE.to_string())
}

fn resolve_keyring_user() -> String {
    std::env::var(KB_LLM_KEYRING_USER_ENV)
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| DEFAULT_KEYRING_USER.to_string())
}

fn keyring_disabled() -> bool {
    std::env::var(KB_LLM_DISABLE_KEYRING_ENV)
        .ok()
        .map(|v| parse_bool(&v).unwrap_or(false))
        .unwrap_or(false)
}

/// 当前生效的 keyring 后端探查结果。供 `kb env --doctor` 等可观测性场景使用，
/// 不影响 hot path：每次调用都会真正去碰一次平台后端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyringStatus {
    /// 经过 env override 解析后的 service 名。
    pub service: String,
    /// 经过 env override 解析后的 user 名。
    pub user: String,
    /// 用户是否通过 `KB_LLM_DISABLE_KEYRING=1` 主动禁用了 keyring。
    pub disabled: bool,
    /// 该 service/user 下是否存在一条非空记录。失败 / 缺失 / 平台不可用都归为 `false`。
    pub key_present: bool,
}

/// 探测当前 keyring 状态——不返回 key 本身，只回报"是否能拿到 key"。
/// 设计成 pub 是为了让 CLI 的诊断子命令复用 kb-llm 这套 env 解析 + 默认值，
/// 避免在两个 crate 里维护同样的常量。
pub fn probe_keyring_status() -> KeyringStatus {
    let disabled = keyring_disabled();
    let service = resolve_keyring_service();
    let user = resolve_keyring_user();

    let key_present = if disabled {
        false
    } else {
        matches!(
            keyring::Entry::new(&service, &user).and_then(|entry| entry.get_password()),
            Ok(value) if !value.trim().is_empty()
        )
    };

    KeyringStatus {
        service,
        user,
        disabled,
        key_present,
    }
}

/// 从系统密钥环读取 LLM API key。返回 `Some` 仅当：
/// 1. `KB_LLM_DISABLE_KEYRING=1` 未设置（运维 / 测试可关）
/// 2. 平台后端可用
/// 3. 对应 service/user 存在条目且非空
///
/// 任何失败都被吞掉——keyring 只是 best-effort 升级层，缺失时降级到配置文件 / env。
/// 这样在 CI / 容器 / 无 keyring 后端的服务器上不会破坏正常启动。
fn load_key_from_keyring() -> Option<String> {
    if keyring_disabled() {
        return None;
    }

    let service = resolve_keyring_service();
    let user = resolve_keyring_user();

    match keyring::Entry::new(&service, &user) {
        Ok(entry) => match entry.get_password() {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Err(_) => None,
        },
        Err(_) => None,
    }
}

/// 写入 LLM API key 到系统密钥环。供 CLI 配置子命令调用（例如未来的
/// `kb config set-key`）。该函数不在 hot path 上。
pub fn save_key_to_keyring(key: &str) -> Result<(), kb_core::DomainError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(kb_core::DomainError::validation(
            "refusing to store an empty key in keyring",
        ));
    }
    let service = resolve_keyring_service();
    let user = resolve_keyring_user();
    let entry = keyring::Entry::new(&service, &user)
        .map_err(|err| kb_core::DomainError::io(format!("keyring entry init failed: {err}")))?;
    entry
        .set_password(trimmed)
        .map_err(|err| kb_core::DomainError::io(format!("keyring set_password failed: {err}")))
}

fn resolve_from_layers(
    process_env: LlmConfigPartial,
    keyring_key: Option<String>,
    project_config: LlmConfigPartial,
    global_config: LlmConfigPartial,
) -> LlmRuntimeConfig {
    let (key, key_source) = pick_optional(&[
        (&process_env.key, LlmConfigSource::ProcessEnv),
        (&keyring_key, LlmConfigSource::Keyring),
        (&project_config.key, LlmConfigSource::ProjectConfig),
        (&global_config.key, LlmConfigSource::GlobalConfig),
    ]);
    let (model, model_source) = pick_required(
        &[
            (&process_env.model, LlmConfigSource::ProcessEnv),
            (&project_config.model, LlmConfigSource::ProjectConfig),
            (&global_config.model, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_MODEL,
    );
    let (base_url, base_url_source) = pick_required(
        &[
            (&process_env.base_url, LlmConfigSource::ProcessEnv),
            (&project_config.base_url, LlmConfigSource::ProjectConfig),
            (&global_config.base_url, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_BASE_URL,
    );

    let semantic_enabled = pick_bool(
        &[
            (&process_env.semantic_enabled, LlmConfigSource::ProcessEnv),
            (
                &project_config.semantic_enabled,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.semantic_enabled,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_ENABLE_SEMANTIC,
    );
    let graph_enabled = pick_bool(
        &[
            (&process_env.graph_enabled, LlmConfigSource::ProcessEnv),
            (
                &project_config.graph_enabled,
                LlmConfigSource::ProjectConfig,
            ),
            (&global_config.graph_enabled, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_ENABLE_GRAPH,
    );
    let wiki_enabled = pick_bool(
        &[
            (&process_env.wiki_enabled, LlmConfigSource::ProcessEnv),
            (&project_config.wiki_enabled, LlmConfigSource::ProjectConfig),
            (&global_config.wiki_enabled, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_ENABLE_WIKI,
    );
    let semantic_flow = normalize_semantic_flow(
        &pick_required(
            &[
                (&process_env.semantic_flow, LlmConfigSource::ProcessEnv),
                (
                    &project_config.semantic_flow,
                    LlmConfigSource::ProjectConfig,
                ),
                (&global_config.semantic_flow, LlmConfigSource::GlobalConfig),
            ],
            DEFAULT_SEMANTIC_FLOW,
        )
        .0,
    );
    let timeout_secs = pick_u64(
        &[
            (&process_env.timeout_secs, LlmConfigSource::ProcessEnv),
            (&project_config.timeout_secs, LlmConfigSource::ProjectConfig),
            (&global_config.timeout_secs, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_TIMEOUT_SECS,
    )
    .max(5);
    let temperature = pick_f64(
        &[
            (&process_env.temperature, LlmConfigSource::ProcessEnv),
            (&project_config.temperature, LlmConfigSource::ProjectConfig),
            (&global_config.temperature, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_TEMPERATURE,
    )
    .clamp(0.0, 2.0);
    let max_output_tokens = pick_u32(
        &[
            (&process_env.max_output_tokens, LlmConfigSource::ProcessEnv),
            (
                &project_config.max_output_tokens,
                LlmConfigSource::ProjectConfig,
            ),
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
                &project_config.analysis_max_output_tokens,
                LlmConfigSource::ProjectConfig,
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
            (
                &project_config.max_input_chars,
                LlmConfigSource::ProjectConfig,
            ),
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
            (&project_config.max_chunks, LlmConfigSource::ProjectConfig),
            (&global_config.max_chunks, LlmConfigSource::GlobalConfig),
        ],
        DEFAULT_MAX_CHUNKS,
    )
    .max(1);
    let retry_max_attempts = pick_usize(
        &[
            (&process_env.retry_max_attempts, LlmConfigSource::ProcessEnv),
            (
                &project_config.retry_max_attempts,
                LlmConfigSource::ProjectConfig,
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
            (
                &project_config.retry_backoff_ms,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.retry_backoff_ms,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_RETRY_BACKOFF_MS,
    )
    .min(30_000);
    let agent_loop_max_rounds = pick_usize(
        &[
            (
                &process_env.agent_loop_max_rounds,
                LlmConfigSource::ProcessEnv,
            ),
            (
                &project_config.agent_loop_max_rounds,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.agent_loop_max_rounds,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_AGENT_LOOP_MAX_ROUNDS,
    )
    .clamp(1, 4);
    let ask_loop_max_rounds = pick_usize(
        &[
            (
                &process_env.ask_loop_max_rounds,
                LlmConfigSource::ProcessEnv,
            ),
            (
                &project_config.ask_loop_max_rounds,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.ask_loop_max_rounds,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_ASK_LOOP_MAX_ROUNDS,
    )
    .clamp(4, 12);
    let agent_reflection_enabled = pick_bool(
        &[
            (
                &process_env.agent_reflection_enabled,
                LlmConfigSource::ProcessEnv,
            ),
            (
                &project_config.agent_reflection_enabled,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.agent_reflection_enabled,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_AGENT_REFLECTION_ENABLED,
    );
    let agent_min_chunk_coverage = pick_f64(
        &[
            (
                &process_env.agent_min_chunk_coverage,
                LlmConfigSource::ProcessEnv,
            ),
            (
                &project_config.agent_min_chunk_coverage,
                LlmConfigSource::ProjectConfig,
            ),
            (
                &global_config.agent_min_chunk_coverage,
                LlmConfigSource::GlobalConfig,
            ),
        ],
        DEFAULT_AGENT_MIN_CHUNK_COVERAGE,
    )
    .clamp(0.0, 1.0);

    let source = LlmConfigSourceMap {
        key: key_source,
        model: model_source,
        base_url: base_url_source,
    };
    let trace = format!(
        "key={}; model={}; base_url={}; semantic_enabled={}; graph_enabled={}; wiki_enabled={}; semantic_flow={}; timeout_secs={}; temperature={}; max_output_tokens={}; analysis_max_output_tokens={}; max_input_chars={}; max_chunks={}; retry_max_attempts={}; retry_backoff_ms={}; agent_loop_max_rounds={}; ask_loop_max_rounds={}; agent_reflection_enabled={}; agent_min_chunk_coverage={}",
        source.key.as_str(),
        source.model.as_str(),
        source.base_url.as_str(),
        semantic_enabled,
        graph_enabled,
        wiki_enabled,
        semantic_flow,
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
        agent_loop_max_rounds,
        ask_loop_max_rounds,
        agent_reflection_enabled,
        agent_min_chunk_coverage
    );

    LlmRuntimeConfig {
        key,
        model,
        base_url,
        semantic_enabled,
        graph_enabled,
        wiki_enabled,
        semantic_flow,
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
        agent_loop_max_rounds,
        ask_loop_max_rounds,
        agent_reflection_enabled,
        agent_min_chunk_coverage,
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
        graph_enabled: std::env::var(KB_LLM_ENABLE_GRAPH_ENV)
            .ok()
            .and_then(|value| parse_bool(&value)),
        wiki_enabled: std::env::var(KB_LLM_ENABLE_WIKI_ENV)
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
        agent_loop_max_rounds: std::env::var(KB_LLM_AGENT_LOOP_MAX_ROUNDS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
        ask_loop_max_rounds: std::env::var(KB_LLM_ASK_LOOP_MAX_ROUNDS_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
        agent_reflection_enabled: std::env::var(KB_LLM_AGENT_REFLECTION_ENABLED_ENV)
            .ok()
            .and_then(|value| parse_bool(&value)),
        agent_min_chunk_coverage: std::env::var(KB_LLM_AGENT_MIN_CHUNK_COVERAGE_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok()),
    }
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

    let key = read_string(llm_obj, &parsed, "key", KB_LLM_KEY_ENV);
    let model = read_string(llm_obj, &parsed, "model", KB_LLM_MODEL_ENV);
    let base_url = read_string(llm_obj, &parsed, "base_url", KB_LLM_BASE_URL_ENV);
    let semantic_enabled = read_bool(
        llm_obj,
        &parsed,
        "enable_semantic",
        KB_LLM_ENABLE_SEMANTIC_ENV,
    );
    let graph_enabled = read_bool(llm_obj, &parsed, "enable_graph", KB_LLM_ENABLE_GRAPH_ENV);
    let wiki_enabled = read_bool(llm_obj, &parsed, "enable_wiki", KB_LLM_ENABLE_WIKI_ENV);
    let semantic_flow = read_string(llm_obj, &parsed, "semantic_flow", KB_LLM_SEMANTIC_FLOW_ENV);
    let timeout_secs = read_u64(llm_obj, &parsed, "timeout_secs", KB_LLM_TIMEOUT_SECS_ENV);
    let temperature = read_f64(llm_obj, &parsed, "temperature", KB_LLM_TEMPERATURE_ENV);
    let max_output_tokens = read_u32(
        llm_obj,
        &parsed,
        "max_output_tokens",
        KB_LLM_MAX_OUTPUT_TOKENS_ENV,
    );
    let analysis_max_output_tokens = read_u32(
        llm_obj,
        &parsed,
        "analysis_max_output_tokens",
        KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS_ENV,
    );
    let max_input_chars = read_usize(
        llm_obj,
        &parsed,
        "max_input_chars",
        KB_LLM_MAX_INPUT_CHARS_ENV,
    );
    let max_chunks = read_usize(llm_obj, &parsed, "max_chunks", KB_LLM_MAX_CHUNKS_ENV);
    let retry_max_attempts = read_usize(
        llm_obj,
        &parsed,
        "retry_max_attempts",
        KB_LLM_RETRY_MAX_ATTEMPTS_ENV,
    );
    let retry_backoff_ms = read_u64(
        llm_obj,
        &parsed,
        "retry_backoff_ms",
        KB_LLM_RETRY_BACKOFF_MS_ENV,
    );
    let agent_loop_max_rounds = read_usize(
        llm_obj,
        &parsed,
        "agent_loop_max_rounds",
        KB_LLM_AGENT_LOOP_MAX_ROUNDS_ENV,
    );
    let ask_loop_max_rounds = read_usize(
        llm_obj,
        &parsed,
        "ask_loop_max_rounds",
        KB_LLM_ASK_LOOP_MAX_ROUNDS_ENV,
    );
    let agent_reflection_enabled = read_bool(
        llm_obj,
        &parsed,
        "agent_reflection_enabled",
        KB_LLM_AGENT_REFLECTION_ENABLED_ENV,
    );
    let agent_min_chunk_coverage = read_f64(
        llm_obj,
        &parsed,
        "agent_min_chunk_coverage",
        KB_LLM_AGENT_MIN_CHUNK_COVERAGE_ENV,
    );

    LlmConfigPartial {
        key: normalize_value(key),
        model: normalize_value(model),
        base_url: normalize_value(base_url),
        semantic_enabled,
        graph_enabled,
        wiki_enabled,
        semantic_flow: normalize_value(semantic_flow),
        timeout_secs,
        temperature,
        max_output_tokens,
        analysis_max_output_tokens,
        max_input_chars,
        max_chunks,
        retry_max_attempts,
        retry_backoff_ms,
        agent_loop_max_rounds,
        ask_loop_max_rounds,
        agent_reflection_enabled,
        agent_min_chunk_coverage,
    }
}

fn read_string(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<String> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            parsed
                .get(legacy_key)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn read_bool(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<bool> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_bool)
        .or_else(|| parsed.get(legacy_key).and_then(Value::as_bool))
}

fn read_u64(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<u64> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_u64)
        .or_else(|| parsed.get(legacy_key).and_then(Value::as_u64))
}

fn read_f64(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<f64> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_f64)
        .or_else(|| parsed.get(legacy_key).and_then(Value::as_f64))
}

fn read_u32(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<u32> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(legacy_key)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
        })
}

fn read_usize(
    llm_obj: Option<&Value>,
    parsed: &Value,
    nested_key: &str,
    legacy_key: &str,
) -> Option<usize> {
    llm_obj
        .and_then(|v| v.get(nested_key))
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .or_else(|| {
            parsed
                .get(legacy_key)
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
        })
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
    use super::{resolve_llm_config, LlmConfigSource};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    const ENV_KEYS: [&str; 6] = [
        "KB_LLM_MODEL",
        "KB_LLM_BASE_URL",
        "KB_LLM_ENABLE_SEMANTIC",
        "KB_LLM_ENABLE_GRAPH",
        "KB_LLM_ENABLE_WIKI",
        // 在测试中强制关闭 keyring，避免开发机上真实存在的 kb-cli/default 条目
        // 影响 precedence 断言。`EnvGuard::capture` 会保存 + 恢复原值。
        "KB_LLM_DISABLE_KEYRING",
    ];

    struct EnvGuard {
        saved: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn capture(keys: &[&str]) -> Self {
            let saved = keys
                .iter()
                .map(|key| (key.to_string(), std::env::var(key).ok()))
                .collect::<Vec<_>>();
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn precedence_is_env_then_project_then_global() {
        let _lock = ENV_LOCK.lock().expect("lock env");
        let _guard = EnvGuard::capture(&ENV_KEYS);
        let dir = unique_temp_dir("llm-config-priority");
        let project_config = dir.join(".kb").join("config.json");
        let global_config = dir.join(".global").join("config.json");
        fs::create_dir_all(project_config.parent().expect("project config parent"))
            .expect("create project dir");
        fs::create_dir_all(global_config.parent().expect("global config parent"))
            .expect("create global dir");

        fs::write(
            &project_config,
            r#"{"llm":{"model":"project-model","base_url":"https://project.example/v1","enable_semantic":true}}"#,
        )
        .expect("write project config");
        fs::write(
            &global_config,
            r#"{"llm":{"model":"global-model","base_url":"https://global.example/v1","enable_semantic":false}}"#,
        )
        .expect("write global config");

        std::env::set_var("KB_LLM_MODEL", "env-model");
        std::env::set_var("KB_LLM_BASE_URL", "https://env.example/v1");
        std::env::set_var("KB_LLM_ENABLE_SEMANTIC", "1");
        std::env::set_var("KB_LLM_ENABLE_GRAPH", "1");
        std::env::set_var("KB_LLM_ENABLE_WIKI", "0");
        std::env::set_var("KB_LLM_DISABLE_KEYRING", "1");

        let cfg = resolve_llm_config(&dir, &global_config);
        assert_eq!(cfg.model, "env-model");
        assert_eq!(cfg.base_url, "https://env.example/v1");
        assert!(cfg.semantic_enabled);
        assert!(cfg.graph_enabled);
        assert!(!cfg.wiki_enabled);
        assert_eq!(cfg.source.model, LlmConfigSource::ProcessEnv);
        assert_eq!(cfg.source.base_url, LlmConfigSource::ProcessEnv);
    }

    #[test]
    fn project_config_beats_global_when_env_missing() {
        let _lock = ENV_LOCK.lock().expect("lock env");
        let _guard = EnvGuard::capture(&ENV_KEYS);
        for key in ENV_KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("KB_LLM_DISABLE_KEYRING", "1");
        let dir = unique_temp_dir("llm-config-project-global");
        let project_config = dir.join(".kb").join("config.json");
        let global_config = dir.join(".global").join("config.json");
        fs::create_dir_all(project_config.parent().expect("project config parent"))
            .expect("create project dir");
        fs::create_dir_all(global_config.parent().expect("global config parent"))
            .expect("create global dir");

        fs::write(
            &project_config,
            r#"{"llm":{"model":"project-model","base_url":"https://project.example/v1","agent_loop_max_rounds":3}}"#,
        )
        .expect("write project config");
        fs::write(
            &global_config,
            r#"{"llm":{"model":"global-model","base_url":"https://global.example/v1","agent_loop_max_rounds":4}}"#,
        )
        .expect("write global config");

        let cfg = resolve_llm_config(&dir, &global_config);
        assert_eq!(cfg.model, "project-model");
        assert_eq!(cfg.base_url, "https://project.example/v1");
        assert_eq!(cfg.agent_loop_max_rounds, 3);
        assert!(!cfg.graph_enabled);
        assert!(!cfg.wiki_enabled);
        assert_eq!(cfg.source.model, LlmConfigSource::ProjectConfig);
        assert_eq!(cfg.source.base_url, LlmConfigSource::ProjectConfig);
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kb_llm_config_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
