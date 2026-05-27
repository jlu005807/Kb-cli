// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// ask 子模块拆分 (#1 follow-up)：本文件负责"传输边界"——
// trace 输出、provider 响应文本/JSON 解析、长文本截断。
// 真正的 HTTP transport 在 #2 之后已经下沉到 `kb_llm::HttpClient`+`ask_client`，
// 这里只剩响应解析与 trace 出口（保留 http 文件名是为了语义稳定）。

fn emit_runtime_trace(prefix: &str, message: &str) {
    eprintln!("[{}] {}", prefix, message);
}

fn emit_ask_trace(message: &str) {
    emit_runtime_trace("ask-agent", message);
}

fn truncate_trace_text(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let mut out = input.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn push_agent_trace(
    trace: &mut Vec<Value>,
    round: Option<usize>,
    kind: &str,
    message: impl Into<String>,
    tool: Option<&str>,
) {
    let message = message.into();
    let mut item = json!({
        "kind": kind,
        "message": message,
    });
    if let Some(obj) = item.as_object_mut() {
        obj.insert(
            "round".to_string(),
            round.map(Value::from).unwrap_or(Value::Null),
        );
        obj.insert(
            "tool".to_string(),
            tool.map(Value::from).unwrap_or(Value::Null),
        );
    }
    trace.push(item);
}

fn extract_chat_message_content(message: &Value) -> Option<String> {
    let message_content = message.get("content")?;
    if let Some(text) = message_content.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(parts) = message_content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        let trimmed = joined.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

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

fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
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
        "ask agent content is not a valid json object; excerpt={:?}",
        truncate_trace_text(trimmed, 320)
    ))
}

