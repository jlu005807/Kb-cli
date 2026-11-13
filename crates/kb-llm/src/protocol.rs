//! OpenAI-compatible Chat Completions 协议层共享助手（Stage 5 去重产物）。
//!
//! 此前 `openai_semantic.rs`、`openai_graph_wiki.rs`、`ask_client.rs` 各自持有
//! 一份几乎逐字相同的协议辅助函数（endpoint 规范化、请求体兼容变体、容错 JSON
//! 解析、provider 兼容性错误分类、重试循环等）。本模块把其中**行为完全一致**的
//! 实现收敛为唯一副本；行为有意分叉的部分（见下）仍留在各自调用方并就地注释。
//!
//! ## 传输层拆分是有意为之（不要在本模块"顺手"统一）
//!
//! - `openai_semantic` / `openai_graph_wiki` 走**阻塞 ureq**（本模块的
//!   `send_openai_chat`），每次请求短连接；
//! - `ask_client` 走 `http.rs` 的 **reqwest + 共享 tokio runtime**（HTTP/2 +
//!   连接复用）。
//!
//! 两套 HTTP 栈并存是当前刻意保留的现状：统一到 reqwest 需要与 `ask_client.rs`
//! 顶部注释提到的 `AskAgent::run(executor)` 编排下沉一起完成（未来迁移）。本次
//! 去重只合并纯逻辑助手，线上请求（请求体、变体顺序、重试语义、解析容差）与
//! 合并前逐字节一致。
//!
//! ## 有意保留在调用方的分叉
//!
//! - **兼容变体顺序**：semantic/graph/wiki 优先尝试带 `response_format` 的变体
//!   （见本模块 `call_openai_chat_once`）；`ask_client::send_with_compat_variants`
//!   则优先不带 `response_format`（tool calling 与 `json_object` 组合在部分
//!   provider 上不兼容）。
//! - **重试循环**：ask 路径的错误类型是 `DomainError`、错误消息格式不同、且
//!   backoff 为无条件 sleep；blocking 路径（本模块 `call_openai_chat_with_retry`）
//!   返回 `String` 错误并只在 `backoff_ms > 0` 时 sleep。
//! - **`stable_truncate` 的字素簇变体**：`openai_semantic` 保留基于
//!   unicode-segmentation 的字素簇截断（其结果写入用户可见的 chunk
//!   summary/preview）；本模块提供的 `stable_truncate` 是 char 级截断，与原
//!   `openai_graph_wiki` / `agent_tools` 的副本逐字一致。

use std::time::Duration;

use serde_json::{json, Value};

/// 把任意 base_url 规范化为 OpenAI-compatible chat completions endpoint。
///
/// - `https://api.openai.com/v1` → 补 `/chat/completions`
/// - `https://api.openai.com/v1/chat/completions` → 原样返回
/// - 末尾多余 `/` 会被去掉。
pub(crate) fn build_chat_completion_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

/// 判断 provider 返回的错误是否值得换一种请求变体重试（比如换 max_tokens 字段、
/// 去掉 response_format）。
///
/// 这个判定避免和"鉴权失败 / 限流 / 网络故障"等无法用变体绕过的错误混淆。
pub(crate) fn supports_max_tokens_fallback(error_text: &str) -> bool {
    let lowered = error_text.to_ascii_lowercase();
    lowered.contains("max_completion_tokens")
        || lowered.contains("max_tokens")
        || lowered.contains("response_format")
        || lowered.contains("json_object")
        || lowered.contains("unknown field")
        || lowered.contains("unrecognized field")
        || lowered.contains("invalid request")
        || lowered.contains("unsupported parameter")
        || lowered.contains("unsupported field")
        || lowered.contains("unexpected field")
}

/// 从 Chat Completions 响应中取出 `choices[0].message.content`。
///
/// 兼容两种协议形态：纯字符串 content，以及分段数组
/// `[{"type":"text","text":"..."}, ...]`（拼接所有 text 段）。
pub(crate) fn extract_chat_message_content(response: &Value) -> Option<String> {
    let message_content = response.pointer("/choices/0/message/content")?;
    if let Some(text) = message_content.as_str() {
        return Some(text.to_string());
    }
    if let Some(parts) = message_content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}

/// 从 LLM 回复的自由文本中容错地解析出一个 JSON object：
///
/// 1. 去掉首尾空白与 BOM 后先尝试整体解析；
/// 2. 失败则用配平大括号扫描抽出第一个完整的 `{...}` 再解析
///    （兼容 markdown 代码块包裹、前后缀解释文字等常见污染）。
///
/// 错误消息中的摘录用本模块 char 级 `stable_truncate`；对原 `openai_semantic`
/// 副本而言仅诊断文案在极端 Unicode 下的截断点可能不同，不影响任何解析结果。
pub(crate) fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim().trim_start_matches('\u{feff}').trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.is_object() {
            return Ok(value);
        }
    }

    if let Some(candidate) = extract_first_json_object(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            if value.is_object() {
                return Ok(value);
            }
        }
    }

    Err(format!(
        "llm content is not a valid json object; excerpt={:?}",
        stable_truncate(trimmed, 320)
    ))
}

/// 配平大括号扫描：返回文本中第一个完整的 `{...}` 切片。
///
/// 正确跳过字符串字面量内部的 `{`/`}`（含转义 `\"`），避免被内容里的花括号骗过。
fn extract_first_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in text[start..].char_indices() {
        let absolute_idx = start + idx;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&text[start..=absolute_idx]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 向请求体写入 provider 兼容字段：`max_tokens_key` 指定的输出上限字段
/// （`max_tokens` 或 `max_completion_tokens`），以及可选的
/// `response_format: {"type":"json_object"}`。
///
/// 四处请求体构造（semantic/graph-wiki 的 system+user 体、ask 的工具轮次体、
/// final_answer 体）此前各自内联这段逻辑，现共享唯一实现。
pub(crate) fn apply_compat_body_fields(
    body: &mut Value,
    max_tokens_key: &str,
    max_output_tokens: u32,
    include_response_format: bool,
) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(max_tokens_key.to_string(), json!(max_output_tokens));
        if include_response_format {
            obj.insert(
                "response_format".to_string(),
                json!({ "type": "json_object" }),
            );
        }
    }
}

/// 构造 system + user 两条消息的 Chat Completions 请求体
/// （semantic / graph / wiki 共用的形态；ask 路径的消息数组由调用方自组）。
pub(crate) fn build_chat_completion_body(
    model: &str,
    temperature: f64,
    max_output_tokens: u32,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens_key: &str,
    include_response_format: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": temperature,
    });
    apply_compat_body_fields(
        &mut body,
        max_tokens_key,
        max_output_tokens,
        include_response_format,
    );
    body
}

/// 一次 blocking Chat Completions 调用所需的连接与模型参数。
///
/// 把原先 `LlmSemanticExtractor` 方法隐式携带的 `self.config`/`self.endpoint`
/// 显式化，使 semantic 与 graph/wiki 两个调用方共享同一份变体与重试实现。
pub(crate) struct ChatProtocolSettings<'a> {
    pub(crate) endpoint: &'a str,
    pub(crate) api_key: &'a str,
    pub(crate) timeout_secs: u64,
    pub(crate) model: &'a str,
    pub(crate) temperature: f64,
    pub(crate) max_output_tokens: u32,
}

/// 带重试的 blocking Chat Completions 调用。
///
/// 语义与原 semantic/graph-wiki 副本逐字一致：
/// - `1..=max_attempts` 逐次尝试（`max_attempts` 的钳制由调用方决定——
///   `openai_semantic` 传原值，`openai_graph_wiki` 传 `.max(1)`，维持既有行为）；
/// - 仅在 `backoff_ms > 0` 且还有后续尝试时 sleep；
/// - 错误消息格式：`attempt {n}/{max} failed: ...` 与
///   `llm request failed after {max} attempts; ...`。
pub(crate) fn call_openai_chat_with_retry(
    settings: &ChatProtocolSettings<'_>,
    max_attempts: usize,
    backoff_ms: u64,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Value, String> {
    let mut errors = Vec::new();
    for attempt in 1..=max_attempts {
        match call_openai_chat_once(settings, system_prompt, user_prompt) {
            Ok(value) => return Ok(value),
            Err(err) => {
                errors.push(format!(
                    "attempt {}/{} failed: {}",
                    attempt, max_attempts, err
                ));
                if attempt < max_attempts && backoff_ms > 0 {
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }

    Err(format!(
        "llm request failed after {} attempts; {}",
        max_attempts,
        errors.join(" | ")
    ))
}

/// 单次调用：轮询四种 provider 兼容变体（max_completion_tokens / max_tokens ×
/// 是否带 response_format），只在 `supports_max_tokens_fallback` 判定为
/// "字段不被认识"类错误时切换到下一个变体。
///
/// 注意变体顺序为 **response_format 优先**——这是 semantic/graph/wiki 路径的
/// 既有顺序；ask 路径的顺序相反且留在 `ask_client.rs` 本地（见模块文档）。
fn call_openai_chat_once(
    settings: &ChatProtocolSettings<'_>,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Value, String> {
    let variants = [
        (
            "max_completion_tokens+response_format",
            "max_completion_tokens",
            true,
        ),
        ("max_tokens+response_format", "max_tokens", true),
        ("max_completion_tokens", "max_completion_tokens", false),
        ("max_tokens", "max_tokens", false),
    ];
    let mut errors = Vec::new();
    for (label, max_tokens_key, include_response_format) in variants {
        let body = build_chat_completion_body(
            settings.model,
            settings.temperature,
            settings.max_output_tokens,
            system_prompt,
            user_prompt,
            max_tokens_key,
            include_response_format,
        );
        match send_openai_chat(
            settings.api_key,
            settings.endpoint,
            settings.timeout_secs,
            &body,
        ) {
            Ok(value) => return Ok(value),
            Err(err) => {
                let can_try_next = supports_max_tokens_fallback(&err);
                errors.push(format!("{} failed: {}", label, err));
                if !can_try_next {
                    return Err(errors.join(" | "));
                }
            }
        }
    }
    Err(format!(
        "llm request compatibility fallbacks failed; {}",
        errors.join(" | ")
    ))
}

/// blocking ureq POST：semantic / graph / wiki 路径的传输层。
///
/// ask 路径不走这里（reqwest + 共享 runtime，见模块文档"传输层拆分"）。
fn send_openai_chat(
    api_key: &str,
    endpoint: &str,
    timeout_secs: u64,
    body: &Value,
) -> Result<Value, String> {
    let request = ureq::post(endpoint)
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(timeout_secs));

    let raw = match request.send_string(&body.to_string()) {
        Ok(response) => response
            .into_string()
            .map_err(|err| format!("failed to read llm response: {}", err))?,
        Err(ureq::Error::Status(code, response)) => {
            let body_text = response
                .into_string()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(format!(
                "http status {} from llm provider: {}",
                code, body_text
            ));
        }
        Err(ureq::Error::Transport(err)) => {
            return Err(format!("llm transport error: {}", err));
        }
    };

    serde_json::from_str(&raw).map_err(|err| format!("invalid llm response json: {}", err))
}

/// char 级稳定截断：超过 `max_chars` 个 `char` 时截断并追加 `...`。
///
/// 与原 `openai_graph_wiki` / `agent_tools` 副本逐字一致。`openai_semantic`
/// 保留其本地字素簇（grapheme）变体——那份结果写入用户可见的 chunk
/// summary/preview，不在此合并（见模块文档）。
pub(crate) fn stable_truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

/// trim 后为空则归一化为 `None`，否则返回 trim 过的字符串。
pub(crate) fn normalize_text_field(input: Option<String>) -> Option<String> {
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
    use super::*;
    use kb_core::usecases::fnv1a64;

    #[test]
    fn endpoint_normalization_appends_and_is_idempotent() {
        assert_eq!(
            build_chat_completion_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_chat_completion_endpoint("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        // 不带 /v1 的裸域名也直接补路径，不猜测版本段。
        assert_eq!(
            build_chat_completion_endpoint("https://api.openai.com"),
            "https://api.openai.com/chat/completions"
        );
        assert_eq!(
            build_chat_completion_endpoint("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_chat_completion_endpoint("https://api.openai.com/v1/chat/completions/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn response_format_errors_are_provider_compatibility_fallbacks() {
        assert!(supports_max_tokens_fallback(
            "http status 400 from llm provider: unsupported parameter: response_format"
        ));
    }

    #[test]
    fn fallback_heuristic_rejects_non_compat_errors() {
        assert!(!supports_max_tokens_fallback("rate limit exceeded"));
        assert!(!supports_max_tokens_fallback("connection reset"));
    }

    #[test]
    fn json_parse_error_includes_safe_excerpt() {
        let err = parse_json_object_from_text("not json at all").expect_err("expected parse error");
        assert!(err.contains("not json at all"));
        assert!(err.contains("valid json object"));
    }

    #[test]
    fn parse_json_object_accepts_markdown_wrapped_payload() {
        let text = "Sure, here is the JSON you asked for:\n```json\n{\"summary\": \"ok\", \"tags\": [\"a\", \"b\"]}\n```\nLet me know if you need more.";
        let value = parse_json_object_from_text(text).expect("should extract json from markdown");
        assert_eq!(value.pointer("/summary").and_then(Value::as_str), Some("ok"));
        assert_eq!(
            value.pointer("/tags/1").and_then(Value::as_str),
            Some("b")
        );
    }

    #[test]
    fn parse_json_object_strips_bom_prefix() {
        let value =
            parse_json_object_from_text("\u{feff}{\"a\": 1}").expect("bom-prefixed json parses");
        assert_eq!(value.pointer("/a").and_then(Value::as_i64), Some(1));
    }

    #[test]
    fn extract_first_json_object_balances_braces_and_ignores_string_braces() {
        let text = "prefix {\"a\": {\"b\": \"}\"}, \"c\": 2} suffix {\"d\": 3}";
        assert_eq!(
            extract_first_json_object(text),
            Some("{\"a\": {\"b\": \"}\"}, \"c\": 2}")
        );
        assert_eq!(extract_first_json_object("no braces here"), None);
        assert_eq!(extract_first_json_object("{\"unclosed\": true"), None);
    }

    #[test]
    fn extract_chat_message_content_supports_string_and_segments() {
        let plain = serde_json::json!({
            "choices": [{ "message": { "content": "plain text" } }]
        });
        assert_eq!(
            extract_chat_message_content(&plain).as_deref(),
            Some("plain text")
        );

        let segmented = serde_json::json!({
            "choices": [{ "message": { "content": [
                { "type": "text", "text": "hel" },
                { "type": "text", "text": "lo" }
            ] } }]
        });
        assert_eq!(
            extract_chat_message_content(&segmented).as_deref(),
            Some("hello")
        );

        let missing = serde_json::json!({ "choices": [] });
        assert!(extract_chat_message_content(&missing).is_none());
    }

    #[test]
    fn chat_body_applies_token_key_and_optional_response_format() {
        let body = build_chat_completion_body(
            "model-x",
            0.2,
            512,
            "sys",
            "user",
            "max_completion_tokens",
            true,
        );
        assert_eq!(
            body.get("max_completion_tokens").and_then(Value::as_u64),
            Some(512)
        );
        assert_eq!(
            body.pointer("/response_format/type").and_then(Value::as_str),
            Some("json_object")
        );
        assert_eq!(
            body.pointer("/messages/0/role").and_then(Value::as_str),
            Some("system")
        );
        assert_eq!(
            body.pointer("/messages/1/content").and_then(Value::as_str),
            Some("user")
        );

        let body = build_chat_completion_body("model-x", 0.2, 256, "sys", "user", "max_tokens", false);
        assert_eq!(body.get("max_tokens").and_then(Value::as_u64), Some(256));
        assert!(body.get("response_format").is_none());
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[test]
    fn stable_truncate_keeps_short_text_and_appends_ellipsis() {
        assert_eq!(stable_truncate("short", 10), "short");
        assert_eq!(stable_truncate("hello world", 5), "hello...");
    }

    #[test]
    fn normalize_text_field_trims_and_drops_blank() {
        assert_eq!(
            normalize_text_field(Some("  hi  ".to_string())),
            Some("hi".to_string())
        );
        assert_eq!(normalize_text_field(Some("   ".to_string())), None);
        assert_eq!(normalize_text_field(None), None);
    }

    #[test]
    fn fnv1a64_matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
    }
}
