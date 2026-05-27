// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// ask 子模块拆分 (#1 follow-up)：本文件负责"工具协议"——
// tools schema、ask agent JSON 解码、tool 执行回路、follow-up 指令构造。

#[derive(Debug)]
struct AskToolResult {
    summary: String,
    json: String,
    retrieved_context: Vec<kb_core::RetrievedContext>,
}

#[derive(Debug)]
struct AskAgentFinalPayload {
    reasoning_summary: Option<String>,
    answer_text: String,
    citations: Vec<Value>,
}

#[derive(Debug)]
struct AskAgentTurn {
    raw_message: Value,
    reasoning_summary: Option<String>,
    tool_calls: Vec<AskToolCall>,
    final_payload: Option<AskAgentFinalPayload>,
}

#[derive(Debug)]
struct AskToolCall {
    id: String,
    name: String,
    arguments: Value,
}


fn ask_agent_tools_schema() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "search_context",
                "description": "Search the knowledge base for relevant records before reading files. Returns ranked retrieved contexts from the layered retriever.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "top_k": { "type": "integer" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file_summary",
                "description": "Read semantic summary, tags, and concepts for one record_id or path.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "record_id_or_path": { "type": "string" }
                    },
                    "required": ["record_id_or_path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_graph_overview",
                "description": "Read high-level knowledge graph stats and sample edges.",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_graph_neighbors",
                "description": "Read graph neighbors for one file record.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "record_id": { "type": "string" }
                    },
                    "required": ["record_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_related_file_summaries",
                "description": "Read semantic summaries for files connected to one record through the knowledge graph.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "record_id": { "type": "string" }
                    },
                    "required": ["record_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_graph_edge",
                "description": "Read one graph edge with evidence.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "edge_id": { "type": "string" }
                    },
                    "required": ["edge_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_wiki_page",
                "description": "Read one wiki page excerpt by page_id. Common page_ids include overview, index, log, and source:<record_id>.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "page_id": { "type": "string" }
                    },
                    "required": ["page_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_wiki_page_markdown",
                "description": "Read a fuller wiki markdown page by page_id. Common page_ids include overview, index, log, and source:<record_id>.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "page_id": { "type": "string" }
                    },
                    "required": ["page_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_raw_chunk",
                "description": "Read a bounded raw chunk for a record_id or path, optionally targeting one chunk_id.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "record_id_or_path": { "type": "string" },
                        "chunk_id": { "type": "string" }
                    },
                    "required": ["record_id_or_path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_raw_document",
                "description": "Read a larger raw source excerpt for a record_id or path.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "record_id_or_path": { "type": "string" }
                    },
                    "required": ["record_id_or_path"]
                }
            }
        }),
    ]
}

fn build_minimal_follow_up_tool_instruction(requirement: &str, tool_trace: &[Value]) -> String {
    if requirement.contains("search_context") {
        return "Do not answer yet. First call search_context with a refined or broader query, then continue."
            .to_string();
    }
    if requirement.contains("raw source evidence") {
        let target = tool_trace.iter().rev().find_map(|item| {
            if item.get("tool").and_then(Value::as_str) == Some("read_file_summary") {
                let summary = item.get("summary").and_then(Value::as_str)?;
                let start = summary.find('(')? + 1;
                let end = summary[start..].find(')')? + start;
                Some(summary[start..end].to_string())
            } else {
                None
            }
        });
        if let Some(record_id) = target {
            return format!(
                "Do not answer yet. Next call read_raw_document({}) or read_raw_chunk({}) before continuing.",
                record_id, record_id
            );
        }
        return "Do not answer yet. Next call read_raw_document or read_raw_chunk for the most relevant file before continuing."
            .to_string();
    }
    if requirement.contains("semantic summaries") {
        return "Do not answer yet. Next call read_file_summary for the most relevant retrieved file before continuing."
            .to_string();
    }
    if requirement.contains("graph relations") || requirement.contains("related file summaries") {
        return "Do not answer yet. Next call read_graph_neighbors or read_related_file_summaries for the main file before continuing."
            .to_string();
    }
    format!(
        "Do not answer yet. {} Continue by calling the necessary tool instead of summarizing.",
        requirement
    )
}

fn decode_ask_agent_final_payload(content: &str) -> Result<AskAgentFinalPayload, String> {
    let value = parse_json_object_from_text(content)?;
    let answer_text = value
        .get("answer_text")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .ok_or_else(|| "ask agent final payload is missing answer_text".to_string())?;
    let reasoning_summary = value
        .get("reasoning_summary")
        .or_else(|| value.get("thought"))
        .and_then(Value::as_str)
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty());
    let citations = value
        .get("citations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(AskAgentFinalPayload {
        reasoning_summary,
        answer_text,
        citations,
    })
}

fn decode_ask_agent_turn(response: &Value) -> Result<AskAgentTurn, String> {
    let raw_message = response
        .pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| "missing choices[0].message in ask agent response".to_string())?;
    let content = extract_chat_message_content(&raw_message);
    let tool_calls = raw_message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool_call")
                        .to_string();
                    let function = item
                        .get("function")
                        .ok_or_else(|| "tool_call missing function".to_string())?;
                    let name = function
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "tool_call missing function.name".to_string())?
                        .to_string();
                    let arguments = match function.get("arguments") {
                        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
                            .map_err(|err| format!("invalid tool_call arguments json: {}", err))?,
                        Some(value) => value.clone(),
                        None => json!({}),
                    };
                    Ok(AskToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let final_payload = if tool_calls.is_empty() {
        match content.as_deref() {
            Some(text) => match decode_ask_agent_final_payload(text) {
                Ok(payload) => Some(payload),
                Err(_) => Some(AskAgentFinalPayload {
                    reasoning_summary: None,
                    answer_text: text.trim().to_string(),
                    citations: vec![],
                }),
            },
            None => None,
        }
    } else {
        None
    };
    let reasoning_summary = if !tool_calls.is_empty() {
        content
    } else {
        final_payload
            .as_ref()
            .and_then(|payload| payload.reasoning_summary.clone())
    };

    Ok(AskAgentTurn {
        raw_message,
        reasoning_summary,
        tool_calls,
        final_payload,
    })
}

fn execute_ask_tool(
    cwd: &Path,
    toolbox: &kb_llm::SemanticAgentToolbox,
    call: &AskToolCall,
    default_search_top_k: usize,
) -> Result<AskToolResult, String> {
    match call.name.as_str() {
        "search_context" => {
            let query = call
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| "search_context requires query".to_string())?;
            let top_k = call
                .arguments
                .get("top_k")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(default_search_top_k)
                .max(1);
            let retrieved_context = search_context_for_agent(cwd, query, top_k)?;
            let json = serde_json::to_string_pretty(&json!({
                "query": query,
                "top_k": top_k,
                "results": contexts_json(&retrieved_context),
            }))
            .unwrap_or_else(|_| "null".to_string());
            Ok(AskToolResult {
                summary: format!(
                    "search_context({:?}, top_k={}) -> {} retrieved items",
                    query,
                    top_k,
                    retrieved_context.len()
                ),
                json,
                retrieved_context,
            })
        }
        "read_file_summary" => {
            let target = call
                .arguments
                .get("record_id_or_path")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_file_summary requires record_id_or_path".to_string())?;
            let value = toolbox.read_file_summary(target);
            Ok(render_tool_result(
                format!("read_file_summary({})", target),
                &value,
            ))
        }
        "read_graph_overview" => {
            let value = toolbox.read_graph_overview();
            Ok(render_tool_result("read_graph_overview".to_string(), &value))
        }
        "read_graph_neighbors" => {
            let target = call
                .arguments
                .get("record_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_graph_neighbors requires record_id".to_string())?;
            let value = toolbox.read_graph_neighbors(target);
            Ok(render_tool_result(
                format!("read_graph_neighbors({})", target),
                &value,
            ))
        }
        "read_related_file_summaries" => {
            let target = call
                .arguments
                .get("record_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_related_file_summaries requires record_id".to_string())?;
            let value = toolbox.read_related_file_summaries(target);
            Ok(render_tool_result(
                format!("read_related_file_summaries({})", target),
                &value,
            ))
        }
        "read_graph_edge" => {
            let edge_id = call
                .arguments
                .get("edge_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_graph_edge requires edge_id".to_string())?;
            let value = toolbox.read_graph_edge(edge_id);
            Ok(render_tool_result(
                format!("read_graph_edge({})", edge_id),
                &value,
            ))
        }
        "read_wiki_page" => {
            let page_id = call
                .arguments
                .get("page_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_wiki_page requires page_id".to_string())?;
            let value = toolbox.read_wiki_page(page_id);
            Ok(render_tool_result(
                format!("read_wiki_page({})", page_id),
                &value,
            ))
        }
        "read_wiki_page_markdown" => {
            let page_id = call
                .arguments
                .get("page_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_wiki_page_markdown requires page_id".to_string())?;
            let value = toolbox.read_wiki_page_markdown(page_id);
            Ok(render_tool_result(
                format!("read_wiki_page_markdown({})", page_id),
                &value,
            ))
        }
        "read_raw_chunk" => {
            let target = call
                .arguments
                .get("record_id_or_path")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_raw_chunk requires record_id_or_path".to_string())?;
            let chunk_id = call.arguments.get("chunk_id").and_then(Value::as_str);
            let value = toolbox.read_raw_chunk(target, chunk_id);
            Ok(render_tool_result(
                format!("read_raw_chunk({})", target),
                &value,
            ))
        }
        "read_raw_document" => {
            let target = call
                .arguments
                .get("record_id_or_path")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_raw_document requires record_id_or_path".to_string())?;
            let value = toolbox.read_raw_document(target);
            Ok(render_tool_result(
                format!("read_raw_document({})", target),
                &value,
            ))
        }
        other => Err(format!("unknown ask tool: {}", other)),
    }
}

fn render_tool_result<T: serde::Serialize>(summary: String, value: &T) -> AskToolResult {
    AskToolResult {
        summary,
        json: serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string()),
        retrieved_context: vec![],
    }
}

fn ask_trace_has_tool(tool_trace: &[Value], tool_name: &str) -> bool {
    tool_trace
        .iter()
        .any(|item| item.get("tool").and_then(Value::as_str) == Some(tool_name))
}

