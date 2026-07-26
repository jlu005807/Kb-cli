// 注意：本文件通过 `include!` 注入到 `commands/runtime/core.rs` 的扁平命名空间，
// 而**不是**一个独立 module。所以这里不写 `use` / `mod`，复用 ask handler 的
// 全部上层符号（Value、json!、CliError、kb_core::RetrievedContext 等）。
//
// ask 子模块拆分 (#1 follow-up)：本文件负责"格式化/可读输出"——
// 把 RetrievedContext / engine meta / markdown 渲染成 CLI text + JSON。

fn contexts_json(items: &[kb_core::RetrievedContext]) -> Vec<Value> {
    items.iter()
        .map(|ctx| {
            json!({
                "alias": ctx.alias,
                "snippet": ctx.snippet,
                "score": ctx.score,
                "source_layer": ctx.source_layer,
                "record_id": ctx.record_id,
                "chunk_id": ctx.chunk_id,
            })
        })
        .collect()
}

fn build_retrieval_trace(items: &[kb_core::RetrievedContext]) -> Vec<Value> {
    items.iter()
        .enumerate()
        .map(|(idx, item)| {
            json!({
                "step": idx + 1,
                "layer": item.source_layer.clone().unwrap_or_else(|| "unknown".to_string()),
                "record_id": item.record_id.clone().unwrap_or_else(|| item.alias.clone()),
                "chunk_id": item.chunk_id,
                "message": format!(
                    "{} hit {} via {}",
                    idx + 1,
                    item.record_id.as_deref().unwrap_or(item.alias.as_str()),
                    item.source_layer.as_deref().unwrap_or("unknown")
                )
            })
        })
        .collect()
}

fn build_reasoning_context(question: &str, items: &[kb_core::RetrievedContext]) -> Value {
    json!({
        "question": question,
        "seed_records": items
            .iter()
            .map(|item| item.record_id.clone().unwrap_or_else(|| item.alias.clone()))
            .collect::<Vec<_>>(),
        "seed_layers": items
            .iter()
            .map(|item| item.source_layer.clone().unwrap_or_else(|| "unknown".to_string()))
            .collect::<Vec<_>>(),
        "context_count": items.len()
    })
}

fn retrieved_context_key(item: &kb_core::RetrievedContext) -> String {
    format!(
        "{}|{}",
        item.record_id.clone().unwrap_or_else(|| item.alias.clone()),
        item.chunk_id.clone().unwrap_or_else(|| "-".to_string())
    )
}

fn merge_retrieved_contexts(
    existing: &mut Vec<kb_core::RetrievedContext>,
    incoming: &[kb_core::RetrievedContext],
) {
    let mut seen = existing
        .iter()
        .map(retrieved_context_key)
        .collect::<HashSet<_>>();
    for item in incoming {
        let key = retrieved_context_key(item);
        if seen.insert(key) {
            existing.push(item.clone());
        }
    }
}

// 10 个参数：answer 摘要要拼接的独立来源本来就多（engine/provider/trace/
// citations/...）。与 agent.rs::bootstrap_overview_context 同理，等 ask.rs
// 整体拆分时再分组，这里定点豁免。
#[allow(clippy::too_many_arguments)]
fn format_answer_text(
    engine_status: &str,
    provider: Option<&str>,
    trace_id: Option<&str>,
    agent_backend: &str,
    agent_warning: Option<&str>,
    retrieval_trace: &[Value],
    agent_trace: &[Value],
    tool_trace: &[Value],
    answer_text: &str,
    citations: &[Value],
) -> String {
    let mut out = String::new();
    out.push_str("Ask result\n");
    out.push_str("mode: answer\n");
    let _ = writeln!(out, "engine_status: {}", engine_status);
    let _ = writeln!(out, "provider: {}", provider.unwrap_or("null"));
    let _ = writeln!(out, "trace_id: {}", trace_id.unwrap_or("null"));
    let _ = writeln!(out, "agent_backend: {}", agent_backend);
    if let Some(warning) = agent_warning {
        let _ = writeln!(out, "agent_warning: {}", warning);
    }
    out.push_str("answer:\n");
    let _ = writeln!(out, "{}", answer_text);
    if !citations.is_empty() {
        out.push_str("citations:\n");
        for citation in citations {
            let _ = writeln!(
                out,
                "  - {}",
                citation["record_id"].as_str().unwrap_or_default()
            );
        }
    }
    out.push_str("retrieval_trace:\n");
    for item in retrieval_trace {
        let _ = writeln!(out, "  - {}", item["message"].as_str().unwrap_or_default());
    }
    if !agent_trace.is_empty() {
        out.push_str("agent_trace:\n");
        for item in agent_trace {
            let _ = writeln!(out, "  - {}", item["message"].as_str().unwrap_or_default());
        }
    }
    out.push_str("tool_trace:\n");
    for item in tool_trace {
        let _ = writeln!(
            out,
            "  - {}",
            item["summary"].as_str().unwrap_or("tool call")
        );
    }
    out
}

fn text_contains_cjk(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        )
    })
}

fn push_markdown_answer_points(points: &mut Vec<String>, markdown: &str, limit: usize) {
    for raw_line in markdown.lines() {
        if points.len() >= limit {
            break;
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
            continue;
        }
        let normalized = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
            .unwrap_or(line)
            .trim();
        if normalized.is_empty() || points.iter().any(|item| item == normalized) {
            continue;
        }
        points.push(normalized.to_string());
    }
}

fn build_stub_overview_answer_text(
    question: &str,
    overview_markdown: Option<&str>,
    index_markdown: Option<&str>,
    representative_lines: &[String],
) -> Option<String> {
    if !question_seeks_kb_overview(question) {
        return None;
    }

    let mut points = Vec::new();
    if let Some(markdown) = overview_markdown {
        push_markdown_answer_points(&mut points, markdown, 6);
    }
    if points.len() < 6 {
        if let Some(markdown) = index_markdown {
            push_markdown_answer_points(&mut points, markdown, 6);
        }
    }
    if points.is_empty() {
        return None;
    }

    let chinese = text_contains_cjk(question);
    let intro = if chinese {
        "\u{6839}\u{636e}\u{77e5}\u{8bc6}\u{5e93}\u{7684} overview\u{3001}index \u{548c}\u{5df2}\u{68c0}\u{7d22}\u{5230}\u{7684}\u{8d44}\u{6599}\u{ff0c}\u{5f53}\u{524d}\u{77e5}\u{8bc6}\u{5e93}\u{7684}\u{4e3b}\u{8981}\u{5185}\u{5bb9}\u{53ef}\u{4ee5}\u{6982}\u{62ec}\u{4e3a}\u{ff1a}"
    } else {
        "Based on the knowledge-base overview, index, and retrieved evidence, the main content is:"
    };
    let representative_label = if chinese {
        "\u{4ee3}\u{8868}\u{6027}\u{8d44}\u{6599}\u{5305}\u{62ec}\u{ff1a}"
    } else {
        "Representative retrieved records:"
    };

    let mut lines = vec![intro.to_string()];
    lines.extend(points.into_iter().map(|item| format!("- {}", item)));
    if !representative_lines.is_empty() {
        lines.push(representative_label.to_string());
        lines.extend(representative_lines.iter().take(3).cloned());
    }
    Some(lines.join("\n"))
}

