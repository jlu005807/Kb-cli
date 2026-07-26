//! Ask agent 的 HTTP 协议层：构造请求体、对 OpenAI-compatible provider 发起请求、
//! 解析响应。这是 #2 LLM 下沉的关键拼图——`kb-cli` 不应直接持有 HTTP 客户端，
//! 也不应该再写 `tools` schema 与 `tool_calls` 解析逻辑；所有协议细节都收敛到这里。
//!
//! 当前模块只关心传输与协议变体兼容（max_tokens/max_completion_tokens、
//! response_format 等），不关心 agent 编排（多轮调用、工具执行）——那部分由
//! `kb-cli` 端的 ask handler 暂留控制。
//!
//! 后续可以在此基础上引入 `AskAgent::run(executor)` 一体化 API，把 agent loop
//! 也移到 kb-llm。
//!
//! Stage 5 去重之后，纯协议逻辑（endpoint 规范化、兼容性错误分类、请求体的
//! 兼容字段写入）委托 `crate::protocol` 的共享实现；本模块保留的是 ask 路径
//! **有意**与 semantic/graph/wiki 路径不同的部分：reqwest 传输栈、相反的兼容
//! 变体顺序、以及 `DomainError` 错误分类的重试循环（详见 protocol.rs 模块文档）。

use std::thread;
use std::time::Duration;

use kb_core::error::DomainError;
use serde_json::{json, Value};

use crate::config::LlmRuntimeConfig;
use crate::http::{block_on_with_timeout, HttpClient};
use crate::protocol;

/// 把任意 base_url 规范化为 OpenAI-compatible chat completions endpoint。
///
/// - 用户配 `https://api.openai.com/v1` → 补 `/chat/completions`
/// - 用户配 `https://api.openai.com/v1/chat/completions` → 原样返回
/// - 末尾多余 `/` 会被去掉。
///
/// 实现与 semantic/graph/wiki 路径共享（`crate::protocol`）；此处保留公开
/// 包装以维持 `kb-cli` 依赖的既有 API。
pub fn ask_chat_completion_endpoint(base_url: &str) -> String {
    protocol::build_chat_completion_endpoint(base_url)
}

/// 判断 provider 返回的错误是否值得换一种请求变体重试（比如换 max_tokens 字段、
/// 去掉 response_format）。
///
/// 这个判定避免和"鉴权失败 / 限流 / 网络故障"等无法用变体绕过的错误混淆。
/// 分类关键字与 blocking 路径共享（`crate::protocol`）。
pub fn supports_ask_provider_fallback(error_text: &str) -> bool {
    protocol::supports_max_tokens_fallback(error_text)
}

/// `final_answer` 工具的 JSON Schema。
///
/// 把"最终回答"也建模成 LLM 的 function call（而不是要求模型在 content 里返回 JSON
/// 字符串），是 OpenAI 官方推荐的做法——让 agent 行为统一在 tool calling 协议里：
/// - 减少自由文本中混入 JSON 时的解析失败
/// - 让回答与中间工具调用共享同一套追溯/日志路径
///
/// 现有 ask handler 的 decode 路径已经会优先消费 `tool_calls`，因此 LLM 一旦选择
/// 调用 `final_answer`，CLI 就会通过 `parse_final_answer_tool_arguments` 读出来。
pub fn final_answer_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "final_answer",
            "description": "Submit the final answer back to the user. Use this when no further tool call is needed. Required: a concise natural-language `answer`. Optional: `citations` (list of {record_id, chunk_id?}) and a short `reasoning_summary`.",
            "parameters": {
                "type": "object",
                "properties": {
                    "answer": { "type": "string" },
                    "citations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "record_id": { "type": "string" },
                                "chunk_id": { "type": ["string", "null"] }
                            },
                            "required": ["record_id"]
                        }
                    },
                    "reasoning_summary": { "type": "string" }
                },
                "required": ["answer"]
            }
        }
    })
}

/// 从 `final_answer` tool call 的 arguments 抽出 answer + citations。
///
/// 容错策略：tool call 的 `arguments` 在 OpenAI 协议里有时是 JSON 字符串，有时是
/// 已经解析过的对象；这里两种都接受。answer 必填，缺失就返回 None 让上层走 fallback。
pub fn parse_final_answer_tool_arguments(arguments: &Value) -> Option<FinalAnswerPayload> {
    let parsed = match arguments {
        Value::String(raw) => serde_json::from_str::<Value>(raw).ok()?,
        Value::Object(_) => arguments.clone(),
        _ => return None,
    };
    let answer = parsed.get("answer").and_then(Value::as_str)?.trim();
    if answer.is_empty() {
        return None;
    }
    let citations = parsed
        .get("citations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reasoning_summary = parsed
        .get("reasoning_summary")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(FinalAnswerPayload {
        answer: answer.to_string(),
        citations,
        reasoning_summary,
    })
}

#[derive(Debug, Clone)]
pub struct FinalAnswerPayload {
    pub answer: String,
    pub citations: Vec<Value>,
    pub reasoning_summary: Option<String>,
}

/// 构造一次 ask 轮次的请求体；`max_tokens_key` 和 `include_response_format` 用于
/// 兼容不同 provider 对 OpenAI schema 的子集实现。
pub fn build_ask_chat_completion_body(
    model: &str,
    temperature: f64,
    max_output_tokens: u32,
    messages: &[Value],
    tools: &[Value],
    max_tokens_key: &str,
    include_response_format: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "tools": tools,
        "tool_choice": "auto",
    });
    protocol::apply_compat_body_fields(
        &mut body,
        max_tokens_key,
        max_output_tokens,
        include_response_format,
    );
    body
}

/// `final_answer` 路径的请求体——和工具轮次共享 max_tokens / response_format 兼容
/// 变体，但不传 `tools`，避免模型反复纠结要不要再调工具。
pub fn build_final_answer_body(
    model: &str,
    temperature: f64,
    max_output_tokens: u32,
    messages: &[Value],
    max_tokens_key: &str,
    include_response_format: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
    });
    protocol::apply_compat_body_fields(
        &mut body,
        max_tokens_key,
        max_output_tokens,
        include_response_format,
    );
    body
}

/// 对一次请求"轮询"四种 OpenAI 协议变体（max_completion_tokens / max_tokens 各两种
/// 是否带 response_format），只在 provider 明确表示"字段不认识"时切换；其他错误
/// 直接终止，避免无意义重试。
///
/// 注意：变体顺序与 `crate::protocol::call_openai_chat_once`（semantic/graph/wiki
/// 路径）**有意相反**——ask 路径优先不带 `response_format`，因为 tool calling 与
/// `json_object` response_format 的组合在部分 provider 上不兼容。此循环还使用
/// reqwest 传输栈与 `DomainError` 错误分类，因此整体留在本地，不并入 protocol。
fn send_with_compat_variants(
    endpoint: &str,
    api_key: &str,
    timeout_secs: u64,
    builder: impl Fn(&str, bool) -> Value,
) -> Result<Value, DomainError> {
    let variants = [
        ("max_completion_tokens", "max_completion_tokens", false),
        ("max_tokens", "max_tokens", false),
        (
            "max_completion_tokens+response_format",
            "max_completion_tokens",
            true,
        ),
        ("max_tokens+response_format", "max_tokens", true),
    ];

    let client = HttpClient::new(timeout_secs);
    let mut errors = Vec::new();
    for (label, max_tokens_key, include_response_format) in variants {
        let body = builder(max_tokens_key, include_response_format);
        // 用 `Duration::from_secs` * 2 给一个比单次 timeout 略宽的硬上限——保险绳。
        let request_timeout = Duration::from_secs(timeout_secs.saturating_mul(2).max(timeout_secs));
        let endpoint_owned = endpoint.to_string();
        let api_key_owned = api_key.to_string();
        let client_clone = client.clone();
        let result = block_on_with_timeout(request_timeout, async move {
            client_clone
                .post_json(&endpoint_owned, &api_key_owned, &body)
                .await
        });
        match result {
            Ok(value) => return Ok(value),
            Err(err) => {
                let text = err.to_string();
                let can_try_next = supports_ask_provider_fallback(&text);
                errors.push(format!("{label} failed: {text}"));
                if !can_try_next {
                    return Err(DomainError::external(errors.join(" | ")));
                }
            }
        }
    }
    Err(DomainError::external(format!(
        "ask llm compatibility fallbacks failed; {}",
        errors.join(" | ")
    )))
}

/// 工具调用轮次的发起。携带 `tools` schema + `tool_choice=auto`，并按配置做指数
/// 退避重试。错误统一为 `DomainError::external`，调用方可据此决定 fallback。
pub fn send_ask_chat_completion_with_retry(
    endpoint: &str,
    api_key: &str,
    llm: &LlmRuntimeConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, DomainError> {
    let max_attempts = llm.retry_max_attempts.max(1);
    let backoff_ms = llm.retry_backoff_ms;
    let mut errors = Vec::new();
    for attempt in 1..=max_attempts {
        let result = send_with_compat_variants(endpoint, api_key, llm.timeout_secs, |key, fmt| {
            build_ask_chat_completion_body(
                &llm.model,
                llm.temperature,
                llm.max_output_tokens,
                messages,
                tools,
                key,
                fmt,
            )
        });
        match result {
            Ok(value) => return Ok(value),
            Err(err) => {
                errors.push(format!("attempt {attempt}: {err}"));
                if attempt < max_attempts {
                    thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }
    Err(DomainError::external(format!(
        "ask llm exhausted retries: {}",
        errors.join(" | ")
    )))
}

/// `final_answer` 路径——和工具轮次共用 compat 变体逻辑，但不带 tools。
pub fn send_final_answer_completion_with_retry(
    endpoint: &str,
    api_key: &str,
    llm: &LlmRuntimeConfig,
    messages: &[Value],
) -> Result<Value, DomainError> {
    let max_attempts = llm.retry_max_attempts.max(1);
    let backoff_ms = llm.retry_backoff_ms;
    let mut errors = Vec::new();
    for attempt in 1..=max_attempts {
        let result = send_with_compat_variants(endpoint, api_key, llm.timeout_secs, |key, fmt| {
            build_final_answer_body(
                &llm.model,
                llm.temperature,
                llm.max_output_tokens,
                messages,
                key,
                fmt,
            )
        });
        match result {
            Ok(value) => return Ok(value),
            Err(err) => {
                errors.push(format!("attempt {attempt}: {err}"));
                if attempt < max_attempts {
                    thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }
    Err(DomainError::external(format!(
        "ask final-answer exhausted retries: {}",
        errors.join(" | ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_is_idempotent() {
        assert_eq!(
            ask_chat_completion_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            ask_chat_completion_endpoint("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            ask_chat_completion_endpoint("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn fallback_heuristic_recognizes_known_provider_errors() {
        for sample in [
            "unsupported parameter max_completion_tokens",
            "Unknown field response_format",
            "Unrecognized field 'json_object'",
            "Invalid request: max_tokens",
        ] {
            assert!(supports_ask_provider_fallback(sample), "missed: {sample}");
        }
        assert!(!supports_ask_provider_fallback("rate limit exceeded"));
        assert!(!supports_ask_provider_fallback("connection reset"));
    }

    #[test]
    fn final_answer_schema_advertises_function_tool() {
        let schema = final_answer_tool_schema();
        assert_eq!(schema.get("type").and_then(Value::as_str), Some("function"));
        assert_eq!(
            schema.pointer("/function/name").and_then(Value::as_str),
            Some("final_answer")
        );
    }

    #[test]
    fn parse_final_answer_handles_string_arguments() {
        let raw = Value::String(
            r#"{"answer":"hello","citations":[{"record_id":"r1"}],"reasoning_summary":"because"}"#
                .to_string(),
        );
        let parsed = parse_final_answer_tool_arguments(&raw).expect("should parse");
        assert_eq!(parsed.answer, "hello");
        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.reasoning_summary.as_deref(), Some("because"));
    }

    #[test]
    fn parse_final_answer_handles_object_arguments() {
        let value = json!({"answer": "ok"});
        let parsed = parse_final_answer_tool_arguments(&value).expect("should parse");
        assert_eq!(parsed.answer, "ok");
        assert!(parsed.citations.is_empty());
        assert!(parsed.reasoning_summary.is_none());
    }

    #[test]
    fn parse_final_answer_rejects_missing_answer() {
        let value = json!({"citations": []});
        assert!(parse_final_answer_tool_arguments(&value).is_none());
    }
}
