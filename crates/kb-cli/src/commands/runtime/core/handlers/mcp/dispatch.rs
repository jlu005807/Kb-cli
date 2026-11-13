// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 3：MCP tools/call 派发。把每个工具调用翻译成已有 use case，
// 复用 include! 平铺命名空间里的 run_remember / run_correct / run_consolidate /
// search_context_for_agent / SemanticAgentToolbox 等。

/// 派发一次 `tools/call`。协议级失败（缺 name / 未知工具）回 JSON-RPC error；
/// 业务级失败（缺参数 / 未初始化）回 `isError:true` 的 tool content。
fn mcp_dispatch_tool_call(id: Value, params: &Value) -> Value {
    let failure_id = id.clone();
    mcp_guard(failure_id, || mcp_dispatch_tool_call_inner(id, params))
}

fn mcp_guard(id: Value, dispatch: impl FnOnce() -> Value) -> Value {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(dispatch)).unwrap_or_else(|_| {
        jsonrpc_result(
            id,
            mcp_tool_content(&json!({ "error": "tool handler panicked" }), true),
        )
    })
}

fn mcp_dispatch_tool_call_inner(id: Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    if name.is_empty() {
        return jsonrpc_error(id, -32602, "tools/call missing name");
    }

    let outcome: Result<Value, String> = match name {
        "recall" => mcp_tool_recall(&args),
        "remember" => mcp_tool_remember(&args),
        "correct" => mcp_tool_correct(&args),
        "verify_citation" => mcp_tool_verify_citation(&args),
        "list_episodes" => mcp_tool_list_episodes(&args),
        "consolidate" => mcp_tool_consolidate(&args),
        "read_summary" => mcp_tool_read_summary(&args),
        "read_neighbors" => mcp_tool_read_neighbors(&args),
        other => return jsonrpc_error(id, -32601, &format!("unknown tool: {}", other)),
    };

    match outcome {
        Ok(payload) => jsonrpc_result(id, mcp_tool_content(&payload, false)),
        Err(msg) => jsonrpc_result(id, mcp_tool_content(&json!({ "error": msg }), true)),
    }
}

fn mcp_cwd() -> Result<std::path::PathBuf, String> {
    ensure_initialized().map_err(|e| format!("{}: {}", e.code, e.message))
}

/// 把 CLI handler 返回的 `Result<Value, CliError>` 转成工具结果。
/// run_* 通过 with_text 在 data 对象里塞了一个面向人类的 `_raw_text` 字段；
/// 这里把它剥掉，只把结构化数据交给 MCP 客户端。
fn mcp_unwrap_cli(result: Result<Value, CliError>) -> Result<Value, String> {
    match result {
        Ok(mut value) => {
            if let Some(obj) = value.as_object_mut() {
                obj.remove("_raw_text");
            }
            Ok(value)
        }
        Err(err) => Err(format!("{}: {}", err.code, err.message)),
    }
}

fn mcp_tool_recall(args: &Value) -> Result<Value, String> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "recall requires query".to_string())?;
    let top_k = args
        .get("top_k")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(5)
        .max(1);
    let cwd = mcp_cwd()?;
    let contexts = search_context_for_agent(&cwd, query, top_k)?;
    Ok(json!({
        "query": query,
        "top_k": top_k,
        "results": contexts_json(&contexts),
    }))
}

fn mcp_tool_remember(args: &Value) -> Result<Value, String> {
    let content = args
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "remember requires content".to_string())?;
    let refs = mcp_string_array(args, "refs");
    let confidence = args.get("confidence").and_then(Value::as_f64).map(|v| v as f32);
    mcp_unwrap_cli(run_remember(content.to_string(), refs, confidence))
}

fn mcp_tool_correct(args: &Value) -> Result<Value, String> {
    let content = args
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "correct requires content".to_string())?;
    let refs = mcp_string_array(args, "refs");
    mcp_unwrap_cli(run_correct(content.to_string(), refs))
}

fn mcp_tool_consolidate(args: &Value) -> Result<Value, String> {
    let min = args
        .get("min")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(2);
    let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
    mcp_unwrap_cli(run_consolidate(min, dry_run))
}

/// 收集字符串数组参数（缺省空）。
fn mcp_string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
