// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 3：MCP server 的 stdio JSON-RPC 传输层。
//
// 为什么手写而非引入 rmcp：MCP 的 stdio 传输本质就是 JSON-RPC 2.0 over
// 换行分隔的 stdin/stdout。手写 initialize / tools/list / tools/call 几个方法
// 即可让 Claude Desktop / Cursor / Cline 接入，复用现成 serde_json，零额外重
// 依赖，契合"无 LLM 也能跑"的轻量取舍。将来需要完整能力协商 / resources /
// prompts 再升级到 rmcp。
//
// 协议：每行一个 JSON-RPC 请求对象；每个有 id 的请求回一行 JSON-RPC 响应。
// notification（无 id，如 `notifications/initialized`）只消费、不回复。
// stdin EOF = 客户端断开 = 正常退出。

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// `kb serve --mcp` 入口。读 stdin 的 JSON-RPC 行、派发、写 stdout 响应。
/// EOF 时 `process::exit(0)`，绕开 main 的输出协议（serve 是终端态命令，
/// stdout 已被 JSON-RPC 流占用，不能再让 main 追加 success 包）。
pub(crate) fn run_serve(mcp: bool) -> Result<Value, CliError> {
    if !mcp {
        return Err(map_core_error(
            CoreErrorKind::InvalidKbFormat,
            json!({
                "reason": "kb serve currently supports only --mcp (stdio JSON-RPC)",
                "hint": "run `kb serve --mcp`",
            }),
        ));
    }

    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin 读错误，按断开处理
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_mcp_line(trimmed);
        if let Some(resp) = response {
            let serialized =
                serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
            // 写一行响应并 flush——stdio 传输要求逐条可读，不能缓冲到退出。
            if writeln!(out, "{}", serialized).is_err() {
                break;
            }
            let _ = out.flush();
        }
    }

    // EOF：客户端断开，正常退出，不回到 main 的输出协议。
    std::process::exit(0);
}

/// 解析一行 JSON-RPC 请求并派发。返回 `None` 表示这是 notification（不回复）
/// 或无法解析（无 id 可回）。
fn handle_mcp_line(line: &str) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(err) => {
            // 解析失败：无 id 可关联，按 JSON-RPC 规范回 id=null 的 parse error。
            return Some(jsonrpc_error(
                Value::Null,
                -32700,
                &format!("parse error: {}", err),
            ));
        }
    };

    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(json!({}));

    // notification（无 id）：消费但不回复。
    let id = id?;

    match method {
        "initialize" => Some(jsonrpc_result(id, mcp_initialize_result())),
        "tools/list" => Some(jsonrpc_result(id, json!({ "tools": mcp_tools_schema() }))),
        "tools/call" => Some(mcp_dispatch_tool_call(id, &params)),
        "ping" => Some(jsonrpc_result(id, json!({}))),
        other => Some(jsonrpc_error(
            id,
            -32601,
            &format!("method not found: {}", other),
        )),
    }
}

/// initialize 响应：声明协议版本 + 能力 + server 信息。
fn mcp_initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "kb-cli",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

/// 构造 JSON-RPC 2.0 成功响应。
fn jsonrpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// 构造 JSON-RPC 2.0 错误响应。
fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

/// 把一次工具调用的结果包成 MCP `tools/call` 响应体（content + isError）。
/// MCP 约定：工具产出放在 `content: [{type:"text", text:...}]`，
/// 业务级失败用 `isError: true` 而非 JSON-RPC error（后者留给协议级错误）。
fn mcp_tool_content(payload: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

#[cfg(test)]
mod mcp_protocol_tests {
    // 注意：本测试模块随 protocol.rs 一起 include! 进 core.rs，与其它 MCP
    // 片段共享平铺命名空间，因此能直接调用 handle_mcp_line / mcp_tools_schema。
    use super::*;

    #[test]
    fn initialize_returns_protocol_version_and_server_info() {
        let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = handle_mcp_line(req).expect("initialize must reply");
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "kb-cli");
    }

    #[test]
    fn tools_list_exposes_all_tools() {
        let req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = handle_mcp_line(req).expect("tools/list must reply");
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in [
            "recall",
            "remember",
            "correct",
            "verify_citation",
            "list_episodes",
            "consolidate",
            "read_summary",
            "read_neighbors",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
    }

    #[test]
    fn notification_without_id_is_not_answered() {
        let req = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(handle_mcp_line(req).is_none());
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let req = r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#;
        let resp = handle_mcp_line(req).expect("must reply");
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let resp = handle_mcp_line("{not json").expect("must reply");
        assert_eq!(resp["error"]["code"], -32700);
        assert_eq!(resp["id"], serde_json::Value::Null);
    }

    #[test]
    fn tools_call_missing_name_is_invalid_params() {
        let req = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{}}"#;
        let resp = handle_mcp_line(req).expect("must reply");
        assert_eq!(resp["error"]["code"], -32602);
    }

    #[test]
    fn tools_call_unknown_tool_is_method_not_found() {
        let req = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#;
        let resp = handle_mcp_line(req).expect("must reply");
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn mcp_guard_converts_tool_panics_to_tool_errors() {
        let response = mcp_guard(Value::from(7), || panic!("simulated tool panic"));

        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["isError"], true);
    }
}
