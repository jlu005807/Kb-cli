// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// ask 子模块拆分 (#1 follow-up)：本文件负责"agent loop 编排"——
// search_context_for_agent + bootstrap_overview_context + 三档 agent runner
// （minimal / stub / live）以及对外的 `run_ask` 入口。
// agent loop 单独的"是否需要更多证据"判定、"是否概览类问题"分类也放在这里，
// 因为它们都和 agent 步态紧耦合。

#[derive(Debug)]
struct AskAgentAnswerOutput {
    agent_backend: String,
    agent_warning: Option<String>,
    retrieved_context: Vec<kb_core::RetrievedContext>,
    agent_trace: Vec<Value>,
    tool_trace: Vec<Value>,
    answer_text: String,
    citations: Vec<Value>,
}

fn search_context_for_agent(
    cwd: &Path,
    query: &str,
    top_k: usize,
) -> Result<Vec<kb_core::RetrievedContext>, String> {
    let storage = StorageHandles::open(cwd).map_err(|err| {
        format!(
            "E1004: Storage unavailable: open storage backend failed: {}",
            err
        )
    })?;
    let snapshot = storage
        .index
        .read_index()
        .map_err(|err| {
            format!("E1004: Storage unavailable: failed to read index snapshot: {}", err)
        })?
        .ok_or_else(|| "missing index snapshot; run `kb update` first".to_string())?;
    let (retriever, _) = build_ask_stack(cwd);
    retriever
        .retrieve(query, top_k.max(1), &snapshot)
        .map_err(|err| format!("search_context failed: {}", err))
}

// ask 命令的人类可读摘要需要把 engine/provider/trace/citations 等独立来源
// 全部拼到一段文本里；这里继续按字段平铺最直观，引入结构体会牵动调用方
// 多处签名。`ask.rs` 后续会整体拆分（part of #1 follow-up），届时再考虑分组。
#[allow(clippy::too_many_arguments)]
fn bootstrap_overview_context(
    toolbox: &kb_llm::SemanticAgentToolbox,
    question: &str,
    messages: &mut Vec<Value>,
    agent_trace: &mut Vec<Value>,
    tool_trace: &mut Vec<Value>,
) {
    if !question_seeks_kb_overview(question) {
        return;
    }

    let mut bootstrap_chunks = Vec::new();
    if let Some(graph) = Some(toolbox.read_graph_overview()) {
        tool_trace.push(json!({
            "tool": "read_graph_overview",
            "summary": format!("read_graph_overview -> nodes={}, edges={}", graph.node_count, graph.edge_count),
        }));
        push_agent_trace(
            agent_trace,
            Some(0),
            "bootstrap",
            "Bootstrap graph overview for a high-level knowledge-base question.".to_string(),
            Some("read_graph_overview"),
        );
        emit_ask_trace("bootstrap tool read_graph_overview");
        bootstrap_chunks.push(format!(
            "read_graph_overview:\n{}",
            serde_json::to_string_pretty(&graph).unwrap_or_else(|_| "null".to_string())
        ));
    }

    if let Some(page) = toolbox.read_wiki_page("overview") {
        tool_trace.push(json!({
            "tool": "read_wiki_page",
            "page_id": "overview",
            "summary": format!("read_wiki_page(overview) -> {}", page.title),
        }));
        push_agent_trace(
            agent_trace,
            Some(0),
            "bootstrap",
            "Bootstrap overview page metadata for a high-level knowledge-base question."
                .to_string(),
            Some("read_wiki_page"),
        );
        emit_ask_trace("bootstrap tool read_wiki_page(overview)");
        bootstrap_chunks.push(format!(
            "read_wiki_page(overview):\n{}",
            serde_json::to_string_pretty(&page).unwrap_or_else(|_| "null".to_string())
        ));
    }

    if let Some(markdown) = toolbox.read_wiki_page_markdown("overview") {
        tool_trace.push(json!({
            "tool": "read_wiki_page_markdown",
            "page_id": "overview",
            "summary": format!("read_wiki_page_markdown(overview) -> {} chars", markdown.markdown.chars().count()),
        }));
        push_agent_trace(
            agent_trace,
            Some(0),
            "bootstrap",
            "Bootstrap full overview markdown for a high-level knowledge-base question."
                .to_string(),
            Some("read_wiki_page_markdown"),
        );
        emit_ask_trace("bootstrap tool read_wiki_page_markdown(overview)");
        bootstrap_chunks.push(format!(
            "read_wiki_page_markdown(overview):\n{}",
            serde_json::to_string_pretty(&markdown).unwrap_or_else(|_| "null".to_string())
        ));
    }

    if let Some(markdown) = toolbox.read_wiki_page_markdown("index") {
        tool_trace.push(json!({
            "tool": "read_wiki_page_markdown",
            "page_id": "index",
            "summary": format!("read_wiki_page_markdown(index) -> {} chars", markdown.markdown.chars().count()),
        }));
        push_agent_trace(
            agent_trace,
            Some(0),
            "bootstrap",
            "Bootstrap index markdown for a high-level knowledge-base question.".to_string(),
            Some("read_wiki_page_markdown"),
        );
        emit_ask_trace("bootstrap tool read_wiki_page_markdown(index)");
        bootstrap_chunks.push(format!(
            "read_wiki_page_markdown(index):\n{}",
            serde_json::to_string_pretty(&markdown).unwrap_or_else(|_| "null".to_string())
        ));
    }

    if !bootstrap_chunks.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": format!(
                "Bootstrap context for this high-level knowledge-base question. Use it when synthesizing the overall KB content:\n\n{}",
                bootstrap_chunks.join("\n\n")
            )
        }));
    }
}

fn ask_agent_requires_more_evidence(
    retrieved_context: &[kb_core::RetrievedContext],
    tool_trace: &[Value],
) -> Option<String> {
    if !ask_trace_has_tool(tool_trace, "search_context") {
        return Some(
            "Search the knowledge base with search_context before answering.".to_string(),
        );
    }
    if retrieved_context.is_empty() {
        return Some(
            "Search returned no usable records yet; refine the query or inspect broader context before answering."
                .to_string(),
        );
    }
    if !ask_trace_has_tool(tool_trace, "read_file_summary") {
        return Some(
            "Read semantic summaries for the most relevant seed files before answering."
                .to_string(),
        );
    }
    if !ask_trace_has_tool(tool_trace, "read_raw_document")
        && !ask_trace_has_tool(tool_trace, "read_raw_chunk")
    {
        return Some(
            "Read raw source evidence for at least one highly relevant file before answering."
                .to_string(),
        );
    }
    if retrieved_context.len() > 1
        && !ask_trace_has_tool(tool_trace, "read_graph_neighbors")
        && !ask_trace_has_tool(tool_trace, "read_related_file_summaries")
    {
        return Some(
            "Expand through graph relations or related file summaries before answering."
                .to_string(),
        );
    }
    None
}

fn question_seeks_kb_overview(question: &str) -> bool {
    let lowered = question.to_ascii_lowercase();
    let patterns = [
        "知识库",
        "主要内容",
        "整体内容",
        "概览",
        "总览",
        "overview",
        "main content",
        "main contents",
        "what is this knowledge base",
        "what is the knowledge base about",
        "what does this knowledge base contain",
    ];
    patterns
        .iter()
        .any(|pattern| question.contains(pattern) || lowered.contains(pattern))
}

fn run_minimal_ask_agent(
    cwd: &Path,
    question: &str,
    retrieved_context: &[kb_core::RetrievedContext],
    default_search_top_k: usize,
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, CliError> {
    if std::env::var("KB_ASK_OFFLINE_STUB").ok().as_deref() == Some("1") {
        return run_stub_ask_agent(
            cwd,
            question,
            retrieved_context,
            default_search_top_k,
            provider,
        );
    }
    match run_live_ask_agent(
        cwd,
        question,
        retrieved_context,
        default_search_top_k,
        provider,
    ) {
        Ok(value) => Ok(value),
        Err(err) => {
            emit_ask_trace(&format!(
                "live agent failed, falling back to offline stub: {}",
                truncate_trace_text(&err, 240)
            ));
            let mut fallback = run_stub_ask_agent(
                cwd,
                question,
                retrieved_context,
                default_search_top_k,
                provider,
            )?;
            fallback.agent_backend = "fallback_stub".to_string();
            fallback.agent_warning = Some(err.clone());
            push_agent_trace(
                &mut fallback.agent_trace,
                None,
                "fallback",
                format!(
                    "Live ask agent was unavailable, so a deterministic offline stub answered instead: {}",
                    truncate_trace_text(&err, 180)
                ),
                None,
            );
            Ok(fallback)
        }
    }
}

fn run_stub_ask_agent(
    cwd: &Path,
    question: &str,
    retrieved_context: &[kb_core::RetrievedContext],
    default_search_top_k: usize,
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, CliError> {
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd.to_path_buf()));
    let wants_overview = question_seeks_kb_overview(question);
    let mut agent_trace = Vec::new();
    let mut tool_trace = Vec::new();
    let mut answer_lines = Vec::new();
    let mut citations = Vec::new();
    let mut overview_markdown = None;
    let mut index_markdown = None;
    let mut gathered_context = retrieved_context.to_vec();

    if gathered_context.is_empty() {
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "thought",
            "No seed records were provided; search the knowledge base before reading files."
                .to_string(),
            None,
        );
        emit_ask_trace("round 1 thought: no seed records, searching knowledge base");
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_call",
            format!("Search the knowledge base for {:?}", question),
            Some("search_context"),
        );
        emit_ask_trace(&format!(
            "round 1 tool search_context({:?}, top_k={})",
            question, default_search_top_k
        ));
        gathered_context = search_context_for_agent(cwd, question, default_search_top_k)
            .map_err(|err| map_core_error(CoreErrorKind::NoRelevantContext, json!({"query": question, "reason": err})))?;
        tool_trace.push(json!({
            "tool": "search_context",
            "summary": format!(
                "search_context({:?}, top_k={}) -> {} retrieved items",
                question,
                default_search_top_k,
                gathered_context.len()
            ),
        }));
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_result",
            format!("Search returned {} retrieved items", gathered_context.len()),
            Some("search_context"),
        );
    } else {
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "thought",
            format!(
                "Seeded {} retrieved records; inspect summaries, graph neighbors, and raw chunks for the best answer.",
                gathered_context.len()
            ),
            None,
        );
        emit_ask_trace(&format!(
            "round 1 thought: seeded {} retrieved records",
            gathered_context.len()
        ));
        tool_trace.push(json!({
            "tool": "search_context",
            "summary": format!("search_context seeded {} retrieved items", gathered_context.len()),
        }));
    }

    for item in gathered_context.iter().take(3) {
        let record_id = item.record_id.as_deref().unwrap_or(item.alias.as_str());
        if let Some(summary) = toolbox.read_file_summary(record_id) {
            let tool_summary = format!(
                "read_file_summary({}) -> tags={}, concepts={}",
                record_id,
                summary.tags.len(),
                summary.key_concepts.len()
            );
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                format!("Inspect semantic summary for {}", record_id),
                Some("read_file_summary"),
            );
            emit_ask_trace(&format!("round 1 tool read_file_summary({})", record_id));
            tool_trace.push(json!({
                "tool": "read_file_summary",
                "record_id": record_id,
                "summary": tool_summary,
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Loaded summary for {}", record_id),
                Some("read_file_summary"),
            );
            answer_lines.push(format!(
                "- {}: {}",
                summary.record_id, summary.summary
            ));
            citations.push(json!({
                "record_id": summary.record_id,
                "chunk_id": Value::Null,
                "source": "read_file_summary"
            }));
        }

        let neighbors = toolbox.read_graph_neighbors(record_id);
        if !neighbors.is_empty() {
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                format!("Expand {} through graph neighbors", record_id),
                Some("read_graph_neighbors"),
            );
            emit_ask_trace(&format!("round 1 tool read_graph_neighbors({})", record_id));
            tool_trace.push(json!({
                "tool": "read_graph_neighbors",
                "record_id": record_id,
                "summary": format!("read_graph_neighbors({}) -> {} relations", record_id, neighbors.len()),
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Graph expansion for {} found {} relations", record_id, neighbors.len()),
                Some("read_graph_neighbors"),
            );
            for relation in neighbors.iter().take(2) {
                citations.push(json!({
                    "record_id": record_id,
                    "edge_id": relation.edge_id,
                    "source": "read_graph_neighbors"
                }));
            }
        }

        if let Some(raw_chunk) = toolbox.read_raw_chunk(record_id, item.chunk_id.as_deref()) {
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                format!("Read raw evidence for {}", record_id),
                Some("read_raw_chunk"),
            );
            emit_ask_trace(&format!("round 1 tool read_raw_chunk({})", record_id));
            tool_trace.push(json!({
                "tool": "read_raw_chunk",
                "record_id": record_id,
                "chunk_id": raw_chunk.chunk_id,
                "summary": format!("read_raw_chunk({}) -> excerpt {} chars", record_id, raw_chunk.preview.chars().count()),
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Read raw chunk {} for {}", raw_chunk.chunk_id, record_id),
                Some("read_raw_chunk"),
            );
        }

        if let Some(raw_document) = toolbox.read_raw_document(record_id) {
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                format!("Read broader source document for {}", record_id),
                Some("read_raw_document"),
            );
            emit_ask_trace(&format!("round 1 tool read_raw_document({})", record_id));
            tool_trace.push(json!({
                "tool": "read_raw_document",
                "record_id": record_id,
                "summary": format!("read_raw_document({}) -> {} chars", record_id, raw_document.content.chars().count()),
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Loaded raw document for {}", record_id),
                Some("read_raw_document"),
            );
        }

        let related = toolbox.read_related_file_summaries(record_id);
        if !related.is_empty() {
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                format!("Inspect summaries for files related to {}", record_id),
                Some("read_related_file_summaries"),
            );
            emit_ask_trace(&format!(
                "round 1 tool read_related_file_summaries({})",
                record_id
            ));
            tool_trace.push(json!({
                "tool": "read_related_file_summaries",
                "record_id": record_id,
                "summary": format!("read_related_file_summaries({}) -> {} related files", record_id, related.len()),
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Loaded {} related file summaries for {}", related.len(), record_id),
                Some("read_related_file_summaries"),
            );
        }
    }

    if let Some(wiki_page) = toolbox.read_wiki_page("overview") {
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_call",
            "Check wiki overview for broader project context".to_string(),
            Some("read_wiki_page"),
        );
        emit_ask_trace("round 1 tool read_wiki_page(overview)");
        tool_trace.push(json!({
            "tool": "read_wiki_page",
            "page_id": "overview",
            "summary": format!("read_wiki_page(overview) -> {}", wiki_page.title),
        }));
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_result",
            format!("Loaded wiki overview page {}", wiki_page.title),
            Some("read_wiki_page"),
        );
    }

    if let Some(wiki_markdown) = toolbox.read_wiki_page_markdown("overview") {
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_call",
            "Read full overview markdown for synthesized context".to_string(),
            Some("read_wiki_page_markdown"),
        );
        emit_ask_trace("round 1 tool read_wiki_page_markdown(overview)");
        tool_trace.push(json!({
            "tool": "read_wiki_page_markdown",
            "page_id": "overview",
            "summary": format!("read_wiki_page_markdown(overview) -> {} chars", wiki_markdown.markdown.chars().count()),
        }));
        push_agent_trace(
            &mut agent_trace,
            Some(1),
            "tool_result",
            format!("Loaded full overview markdown {}", wiki_markdown.title),
            Some("read_wiki_page_markdown"),
        );
        citations.push(json!({
            "record_id": "wiki:overview",
            "chunk_id": Value::Null,
            "source": "read_wiki_page_markdown"
        }));
        overview_markdown = Some(wiki_markdown.markdown);
    }

    if wants_overview {
        if let Some(wiki_markdown) = toolbox.read_wiki_page_markdown("index") {
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_call",
                "Read index markdown for broader repository coverage".to_string(),
                Some("read_wiki_page_markdown"),
            );
            emit_ask_trace("round 1 tool read_wiki_page_markdown(index)");
            tool_trace.push(json!({
                "tool": "read_wiki_page_markdown",
                "page_id": "index",
                "summary": format!("read_wiki_page_markdown(index) -> {} chars", wiki_markdown.markdown.chars().count()),
            }));
            push_agent_trace(
                &mut agent_trace,
                Some(1),
                "tool_result",
                format!("Loaded full index markdown {}", wiki_markdown.title),
                Some("read_wiki_page_markdown"),
            );
            citations.push(json!({
                "record_id": "wiki:index",
                "chunk_id": Value::Null,
                "source": "read_wiki_page_markdown"
            }));
            index_markdown = Some(wiki_markdown.markdown);
        }
    }

    let answer_text = if let Some(overview_answer) = build_stub_overview_answer_text(
        question,
        overview_markdown.as_deref(),
        index_markdown.as_deref(),
        &answer_lines,
    ) {
        overview_answer
    } else if answer_lines.is_empty() {
        format!(
            "No compiled summary could be expanded for the question: {}. Provider: {}.",
            question,
            provider.unwrap_or("unknown")
        )
    } else {
        format!(
            "Based on the retrieved knowledge-base summaries and graph neighbors, here is the most relevant context for \"{}\":\n{}",
            question,
            answer_lines.join("\n")
        )
    };
    push_agent_trace(
        &mut agent_trace,
        Some(1),
        "final",
        "Compose the final answer from retrieved summaries and graph evidence.".to_string(),
        None,
    );
    emit_ask_trace("final answer composed from offline stub context");

    Ok(AskAgentAnswerOutput {
        agent_backend: "offline_stub".to_string(),
        agent_warning: None,
        retrieved_context: gathered_context,
        agent_trace,
        tool_trace,
        answer_text,
        citations,
    })
}

fn run_live_ask_agent(
    cwd: &Path,
    question: &str,
    retrieved_context: &[kb_core::RetrievedContext],
    default_search_top_k: usize,
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, String> {
    let llm = resolve_llm_config(cwd);
    let api_key = llm
        .key
        .as_deref()
        .ok_or_else(|| "missing KB_LLM_KEY for ask answer mode".to_string())?;
    let endpoint = kb_llm::ask_chat_completion_endpoint(&llm.base_url);
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd.to_path_buf()));

    let mut messages = vec![
        json!({
            "role": "system",
            "content": "You are a read-only knowledge-base question answering agent. Use the available tools to search the knowledge base, inspect compiled summaries, graph relations, wiki pages, and raw source evidence before answering. Prefer graph-backed evidence when it helps connect related files. Before finalizing, you must search for relevant records, inspect semantic summaries for the main candidate files, and read raw source evidence for at least one relevant file. If related files matter, inspect graph neighbors or related file summaries too. When you are ready to answer, stop calling tools and return a JSON object with keys reasoning_summary, answer_text, and citations. Keep reasoning_summary brief and high-level."
        }),
        json!({
            "role": "user",
            "content": format!(
                "Question: {}\nCurrent seed contexts:\n{}\nRecommended workflow:\n1. If there are no seed contexts or they are insufficient, call search_context(query=question, top_k={}) first.\n2. Read summaries for the top candidate files.\n3. Read raw source evidence for the most relevant file(s).\n4. Expand via graph neighbors / related file summaries when connections matter.\n5. Read wiki overview or full wiki markdown when repository-level synthesis is useful.\nYou may call search_context more than once if you need to refine or broaden the candidate set before answering.",
                question,
                serde_json::to_string_pretty(&contexts_json(retrieved_context)).unwrap_or_else(|_| "[]".to_string()),
                default_search_top_k
            )
        }),
    ];

    let mut agent_trace = vec![];
    let mut tool_trace = vec![];
    let mut gathered_context = retrieved_context.to_vec();
    let tools = ask_agent_tools_schema();
    bootstrap_overview_context(
        &toolbox,
        question,
        &mut messages,
        &mut agent_trace,
        &mut tool_trace,
    );
    emit_ask_trace(&format!(
        "start provider={} seed_records={}",
        provider.unwrap_or("unknown"),
        retrieved_context.len()
    ));

    // ask 工具循环使用独立的 ask_loop_max_rounds（config 已 clamp 到 [4,12]）；
    // agent_loop_max_rounds 只管 semantic/graph/wiki 的反思轮（clamp [1,4]），两者语义不同。
    let max_rounds = llm.ask_loop_max_rounds.clamp(4, 12);
    for round in 1..=max_rounds {
        emit_ask_trace(&format!("round {} request", round));
        let response =
            kb_llm::send_ask_chat_completion_with_retry(&endpoint, api_key, &llm, &messages, &tools)
                .map_err(|err| err.to_string())?;
        let turn = decode_ask_agent_turn(&response)?;

        if let Some(reasoning) = turn.reasoning_summary.clone() {
            push_agent_trace(
                &mut agent_trace,
                Some(round),
                "thought",
                reasoning.clone(),
                None,
            );
            emit_ask_trace(&format!(
                "round {} thought: {}",
                round,
                truncate_trace_text(&reasoning, 180)
            ));
        }

        if !turn.tool_calls.is_empty() {
            messages.push(turn.raw_message);
            for call in turn.tool_calls {
                let tool_result =
                    execute_ask_tool(cwd, &toolbox, &call, default_search_top_k)?;
                if !tool_result.retrieved_context.is_empty() {
                    merge_retrieved_contexts(
                        &mut gathered_context,
                        &tool_result.retrieved_context,
                    );
                }
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "tool_call",
                    format!("Invoke {}", tool_result.summary),
                    Some(call.name.as_str()),
                );
                emit_ask_trace(&format!("round {} tool {}", round, tool_result.summary));
                tool_trace.push(json!({
                    "tool": call.name,
                    "summary": tool_result.summary,
                }));
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "tool_result",
                    format!("Loaded {}", truncate_trace_text(&tool_result.summary, 180)),
                    Some(call.name.as_str()),
                );
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": tool_result.json,
                }));
            }
            continue;
        }

        if let Some(payload) = turn.final_payload {
            if let Some(requirement) = ask_agent_requires_more_evidence(&gathered_context, &tool_trace)
            {
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "reflection",
                    requirement.clone(),
                    None,
                );
                emit_ask_trace(&format!(
                    "round {} insufficient evidence: {}",
                    round,
                    truncate_trace_text(&requirement, 180)
                ));
                let follow_up = build_minimal_follow_up_tool_instruction(&requirement, &tool_trace);
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "reflection",
                    follow_up.clone(),
                    None,
                );
                messages.push(turn.raw_message);
                messages.push(json!({
                    "role": "user",
                    "content": format!(
                        "The answer is not ready yet. {}\n{}",
                        requirement,
                        follow_up
                    )
                }));
                continue;
            }
            if let Some(reasoning) = payload.reasoning_summary.clone() {
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "final",
                    reasoning,
                    None,
                );
            } else {
                push_agent_trace(
                    &mut agent_trace,
                    Some(round),
                    "final",
                    "Final answer returned after agent inspection.".to_string(),
                    None,
                );
            }
            emit_ask_trace(&format!("final answer returned in round {}", round));
            return Ok(AskAgentAnswerOutput {
                agent_backend: "live".to_string(),
                agent_warning: None,
                retrieved_context: gathered_context,
                agent_trace,
                tool_trace,
                answer_text: payload.answer_text,
                citations: payload.citations,
            });
        }
    }

    if let Some(requirement) = ask_agent_requires_more_evidence(&gathered_context, &tool_trace) {
        return Err(format!(
            "ask agent exhausted tool rounds before collecting required evidence: {}",
            requirement
        ));
    }

    emit_ask_trace("tool rounds exhausted, forcing final synthesis");
    push_agent_trace(
        &mut agent_trace,
        None,
        "reflection",
        "Tool rounds completed; request a final synthesized answer from the accumulated evidence."
            .to_string(),
        None,
    );
    messages.push(json!({
        "role": "user",
        "content": if question_seeks_kb_overview(question) {
            "Now stop calling tools and synthesize the main content of the whole knowledge base from all gathered evidence. Return JSON only with reasoning_summary, answer_text, and citations."
        } else {
            "Now stop calling tools and provide the final answer from all gathered evidence. Return JSON only with reasoning_summary, answer_text, and citations."
        }
    }));
    let final_response =
        kb_llm::send_final_answer_completion_with_retry(&endpoint, api_key, &llm, &messages)
            .map_err(|err| err.to_string())?;
    let final_content = extract_chat_message_content(
        final_response
            .pointer("/choices/0/message")
            .ok_or_else(|| "missing choices[0].message in final answer response".to_string())?,
    )
    .ok_or_else(|| "missing choices[0].message.content in final answer response".to_string())?;
    let payload = decode_ask_agent_final_payload(&final_content)?;
    push_agent_trace(
        &mut agent_trace,
        None,
        "final",
        payload
            .reasoning_summary
            .clone()
            .unwrap_or_else(|| "Final answer synthesized after tool rounds.".to_string()),
        None,
    );
    emit_ask_trace("forced final synthesis returned answer");
    return Ok(AskAgentAnswerOutput {
        agent_backend: "live".to_string(),
        agent_warning: None,
        retrieved_context: gathered_context,
        agent_trace,
        tool_trace,
        answer_text: payload.answer_text,
        citations: payload.citations,
    });

    #[allow(unreachable_code)]
    Err(format!(
        "ask agent did not produce final answer for question '{}' via {}",
        question,
        provider.unwrap_or("unknown")
    ))
}
pub(crate) fn run_ask(question: String, top: Option<u32>, answer: bool) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let storage = StorageHandles::open(&cwd)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let snapshot = storage
        .index
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let Some(snapshot) = snapshot else {
        return Err(map_core_error(
            CoreErrorKind::NoRelevantContext,
            json!({"query": question}),
        ));
    };

    // Phase 1.5：把这次提问记入 L0 时间线（best-effort，失败仅降级为 trace）。
    record_ask_query_episode(&storage, &question, answer);

    if answer {
        let (_, answer_engine) = build_ask_stack(&cwd);
        let meta = answer_engine
            .meta(&question, &[])
            .map_err(|_| map_core_error(CoreErrorKind::NoRelevantContext, json!({"query": question})))?;
        if meta.engine_status == "disabled" {
            return Err(map_core_error(
                CoreErrorKind::NoRelevantContext,
                json!({
                    "query": question,
                    "mode": "answer",
                    "reason": "llm_not_configured"
                }),
            ));
        }
        let default_search_top_k = normalize_top(top, 3);
        let mut answer_out = run_minimal_ask_agent(
            &cwd,
            &question,
            &[],
            default_search_top_k,
            meta.provider.as_deref(),
        )?;
        // sqlite 后端下 SemanticAgentToolbox 仍直接读 .kb/*.json 文件布局，
        // L2/L4/L5 落在 store.sqlite3 时读侧工具会拿不到数据——显式提醒而非静默退化。
        if matches!(
            crate::storage_backend::resolve_storage_backend(&cwd),
            crate::storage_backend::StorageBackend::Sqlite
        ) {
            let note = "storage.backend=sqlite: semantic toolbox reads the fs JSON layout (.kb/knowledge.json etc.); summary/graph/wiki read tools may report missing data".to_string();
            answer_out.agent_warning = Some(match answer_out.agent_warning.take() {
                Some(existing) => format!("{existing}; {note}"),
                None => note,
            });
        }
        let retrieval_trace = build_retrieval_trace(&answer_out.retrieved_context);
        let reasoning_context = build_reasoning_context(&question, &answer_out.retrieved_context);
        let text = format_answer_text(
            meta.engine_status.as_str(),
            meta.provider.as_deref(),
            meta.trace_id.as_deref(),
            &answer_out.agent_backend,
            answer_out.agent_warning.as_deref(),
            &retrieval_trace,
            &answer_out.agent_trace,
            &answer_out.tool_trace,
            &answer_out.answer_text,
            &answer_out.citations,
        );
        // Phase 1.5：把这次回答记入 L0 时间线，refs 挂上引用到的 chunk_id。
        record_ask_answer_episode(
            &storage,
            &question,
            &answer_out.answer_text,
            &answer_out.citations,
            &answer_out.retrieved_context,
        );
        let data = json!({
            "mode": "answer",
            "engine_status": meta.engine_status,
            "provider": meta.provider,
            "trace_id": meta.trace_id,
            "agent_backend": answer_out.agent_backend,
            "agent_warning": answer_out.agent_warning,
            "retrieved_context": contexts_json(&answer_out.retrieved_context),
            "retrieval_trace": retrieval_trace,
            "reasoning_context": reasoning_context,
            "agent_trace": answer_out.agent_trace,
            "tool_trace": answer_out.tool_trace,
            "answer": {
                "text": answer_out.answer_text,
                "citations": answer_out.citations,
            }
        });
        return Ok(with_text(data, text));
    }

    let (retriever, answer_engine) = build_ask_stack(&cwd);
    let out = kb_core::usecases::execute_ask(
        retriever.as_ref(),
        answer_engine.as_ref(),
        &snapshot,
        &question,
        top,
    )
    .map_err(|_| map_core_error(CoreErrorKind::NoRelevantContext, json!({"query": question})))?;

    let retrieval_trace = build_retrieval_trace(&out.retrieved_context);
    let reasoning_context = build_reasoning_context(&question, &out.retrieved_context);

    let mut ask_text = String::new();
    ask_text.push_str("Ask result\n");
    let _ = writeln!(ask_text, "mode: retrieve");
    let _ = writeln!(ask_text, "engine_status: {}", out.meta.engine_status);
    let _ = writeln!(
        ask_text,
        "provider: {}",
        out.meta.provider.as_deref().unwrap_or("null")
    );
    let _ = writeln!(
        ask_text,
        "trace_id: {}",
        out.meta.trace_id.as_deref().unwrap_or("null")
    );

    if out.retrieved_context.is_empty() {
        ask_text.push_str("retrieved_context: (empty)\n");
    } else {
        ask_text.push_str("retrieved_context:\n");
        for (idx, ctx) in out.retrieved_context.iter().enumerate() {
            let _ = writeln!(
                ask_text,
                "  {}. {} (score: {:.3})",
                idx + 1,
                ctx.alias,
                ctx.score
            );
            let _ = writeln!(
                ask_text,
                "     layer: {} record: {} chunk: {}",
                ctx.source_layer.as_deref().unwrap_or("unknown"),
                ctx.record_id.as_deref().unwrap_or(ctx.alias.as_str()),
                ctx.chunk_id.as_deref().unwrap_or("-")
            );
            let _ = writeln!(ask_text, "     {}", ctx.snippet);
        }
    }
    ask_text.push_str("retrieval_trace:\n");
    for item in &retrieval_trace {
        let _ = writeln!(ask_text, "  - {}", item["message"].as_str().unwrap_or_default());
    }

    let data = json!({
        "mode": "retrieve",
        "engine_status": out.meta.engine_status,
        "provider": out.meta.provider,
        "trace_id": out.meta.trace_id,
        "retrieved_context": contexts_json(&out.retrieved_context),
        "retrieval_trace": retrieval_trace,
        "reasoning_context": reasoning_context,
    });
    Ok(with_text(data, ask_text))
}

#[cfg(test)]
mod tests {
    use super::{
        ask_agent_requires_more_evidence, decode_ask_agent_turn, format_answer_text,
        question_seeks_kb_overview, run_stub_ask_agent,
    };
    use kb_core::domain::{
        KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
        WikiManifestEntry, WikiManifestSnapshot, WikiPageType,
    };
    use kb_core::RetrievedContext;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kb_cli_handlers_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_stub_overview_fixture(root: &Path) -> RetrievedContext {
        fs::create_dir_all(root.join(".kb/wiki")).expect("create wiki dir");
        fs::create_dir_all(root.join("docs")).expect("create docs dir");
        fs::write(
            root.join("docs/a.md"),
            "Greedy scheduling notes and energy constraints.",
        )
        .expect("write source doc");

        let knowledge = KnowledgeSnapshot {
            schema_version: 1,
            generated_at: "now".to_string(),
            base_index_signature: "idx".to_string(),
            entries: vec![KnowledgeEntry {
                record_id: "docs/a".to_string(),
                path: "./docs/a.md".to_string(),
                input_signature: "sig-a".to_string(),
                status: KnowledgeStatus::Active,
                summary: "A greedy scheduling note about minimum initial energy.".to_string(),
                tags: vec!["greedy".to_string()],
                key_concepts: vec!["scheduling".to_string(), "energy".to_string()],
                chunks: vec![KnowledgeChunk {
                    chunk_id: "docs/a:0001".to_string(),
                    byte_range: [0, 32],
                    content_signature: "chunk-a".to_string(),
                    summary: "Task ordering by slack.".to_string(),
                    preview: "Greedy scheduling".to_string(),
                }],
                provider: SemanticProviderInfo {
                    kind: "rule".to_string(),
                    name: "rule".to_string(),
                },
                ..KnowledgeEntry::default()
            }],
        };
        fs::write(
            root.join(".kb/knowledge.json"),
            serde_json::to_string_pretty(&knowledge).expect("serialize knowledge"),
        )
        .expect("write knowledge");

        let overview_markdown = concat!(
            "# Overview\n\n",
            "\u{8fd9}\u{4e2a}\u{77e5}\u{8bc6}\u{5e93}\u{4e3b}\u{8981}\u{6574}\u{7406}\u{7b97}\u{6cd5}\u{9898}\u{89e3}\u{3002}",
            "\u{5f53}\u{524d}\u{91cd}\u{70b9}\u{8986}\u{76d6}\u{8d2a}\u{5fc3}\u{3001}\u{52a8}\u{6001}\u{89c4}\u{5212}\u{548c}\u{56fe}\u{641c}\u{7d22}\u{ff0c}",
            "\u{5e76}\u{901a}\u{8fc7}\u{77e5}\u{8bc6}\u{56fe}\u{8c31}\u{8fde}\u{63a5}\u{76f8}\u{5173}\u{9898}\u{76ee}\u{548c}\u{89e3}\u{6cd5}\u{3002}\n\n",
            "## Topics\n",
            "- \u{8d2a}\u{5fc3}\n",
            "- \u{52a8}\u{6001}\u{89c4}\u{5212}\n",
            "- \u{56fe}\u{641c}\u{7d22}\n"
        );
        let index_markdown = concat!(
            "# Index\n\n",
            "- docs/a: \u{6700}\u{5c0f}\u{521d}\u{59cb}\u{80fd}\u{91cf}\u{4e0e}\u{4efb}\u{52a1}\u{6392}\u{5e8f}\n"
        );
        fs::write(root.join(".kb/wiki/overview.md"), overview_markdown).expect("write overview");
        fs::write(root.join(".kb/wiki/index.md"), index_markdown).expect("write index");

        let manifest = WikiManifestSnapshot {
            schema_version: 1,
            generated_at: "now".to_string(),
            base_graph_signature: "graph".to_string(),
            pages: vec![
                WikiManifestEntry {
                    page_id: "overview".to_string(),
                    page_type: WikiPageType::Overview,
                    title: "Overview".to_string(),
                    relative_path: "overview.md".to_string(),
                    input_signature: "overview".to_string(),
                    source_records: vec!["docs/a".to_string()],
                    source_chunks: vec!["docs/a:0001".to_string()],
                },
                WikiManifestEntry {
                    page_id: "index".to_string(),
                    page_type: WikiPageType::Index,
                    title: "Index".to_string(),
                    relative_path: "index.md".to_string(),
                    input_signature: "index".to_string(),
                    source_records: vec!["docs/a".to_string()],
                    source_chunks: vec!["docs/a:0001".to_string()],
                },
            ],
        };
        fs::write(
            root.join(".kb/wiki/manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");

        RetrievedContext {
            alias: "docs/a".to_string(),
            snippet: "Greedy scheduling".to_string(),
            score: 0.93,
            source_layer: Some("l3_projection".to_string()),
            record_id: Some("docs/a".to_string()),
            chunk_id: Some("docs/a:0001".to_string()),
        }
    }

    #[test]
    fn decode_ask_agent_turn_reads_openai_tool_calls() {
        let response = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Need to inspect the graph before answering.",
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "read_graph_neighbors",
                                    "arguments": "{\"record_id\":\"docs/config\"}"
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let turn = decode_ask_agent_turn(&response).expect("turn should decode");
        assert_eq!(turn.reasoning_summary.as_deref(), Some("Need to inspect the graph before answering."));
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert_eq!(turn.tool_calls[0].name, "read_graph_neighbors");
        assert_eq!(turn.tool_calls[0].arguments["record_id"], "docs/config");
    }

    #[test]
    fn decode_ask_agent_turn_reads_search_context_tool_calls() {
        let response = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Need to search before reading files.",
                        "tool_calls": [
                            {
                                "id": "call_search",
                                "type": "function",
                                "function": {
                                    "name": "search_context",
                                    "arguments": "{\"query\":\"知识库的主要内容\",\"top_k\":3}"
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let turn = decode_ask_agent_turn(&response).expect("turn should decode");
        assert_eq!(
            turn.reasoning_summary.as_deref(),
            Some("Need to search before reading files.")
        );
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "search_context");
        assert_eq!(
            turn.tool_calls[0].arguments["query"],
            "\u{77e5}\u{8bc6}\u{5e93}\u{7684}\u{4e3b}\u{8981}\u{5185}\u{5bb9}"
        );
        assert_eq!(turn.tool_calls[0].arguments["top_k"], 3);
    }

    #[test]
    fn ask_answer_validator_requires_raw_evidence_before_finalizing() {
        let retrieved = vec![RetrievedContext {
            alias: "docs/config".to_string(),
            snippet: "config snippet".to_string(),
            score: 0.9,
            source_layer: Some("l3_projection".to_string()),
            record_id: Some("docs/config".to_string()),
            chunk_id: Some("docs/config:0001".to_string()),
        }];
        let tool_trace = vec![
            json!({
                "tool": "search_context",
                "summary": "search_context(\"config\", top_k=1) -> 1 retrieved items"
            }),
            json!({
                "tool": "read_file_summary",
                "summary": "read_file_summary(docs/config)"
            }),
        ];

        let warning =
            ask_agent_requires_more_evidence(&retrieved, &tool_trace).expect("should require raw evidence");
        assert!(warning.contains("raw"));
    }

    #[test]
    fn ask_answer_validator_requires_search_before_finalizing_when_agent_starts_empty() {
        let tool_trace = vec![
            json!({
                "tool": "read_file_summary",
                "summary": "read_file_summary(docs/config)"
            }),
            json!({
                "tool": "read_raw_document",
                "summary": "read_raw_document(docs/config)"
            }),
        ];

        let warning = ask_agent_requires_more_evidence(&[], &tool_trace)
            .expect("should require search when no initial seeds are provided");
        assert!(warning.contains("search"));
    }

    #[test]
    fn overview_question_detector_handles_real_unicode_chinese() {
        assert!(question_seeks_kb_overview(
            "\u{77e5}\u{8bc6}\u{5e93}\u{7684}\u{4e3b}\u{8981}\u{5185}\u{5bb9}"
        ));
        assert!(question_seeks_kb_overview(
            "\u{8fd9}\u{4e2a}\u{77e5}\u{8bc6}\u{5e93}\u{4e3b}\u{8981}\u{8bb2}\u{4e86}\u{4ec0}\u{4e48}"
        ));
        assert!(!question_seeks_kb_overview(
            "1665 \u{8fd9}\u{9053}\u{9898}\u{600e}\u{4e48}\u{505a}"
        ));
    }

    #[test]
    fn format_answer_text_places_answer_before_traces() {
        let text = format_answer_text(
            "configured",
            Some("provider"),
            None,
            "live",
            None,
            &[json!({ "message": "hit docs/a" })],
            &[json!({ "message": "thought" })],
            &[json!({ "summary": "read_file_summary(docs/a)" })],
            "Final synthesized answer.",
            &[],
        );

        let answer_pos = text
            .find("answer:\nFinal synthesized answer.")
            .expect("answer block");
        let retrieval_pos = text.find("retrieval_trace:").expect("retrieval trace");
        assert!(answer_pos < retrieval_pos, "answer should appear before traces");
    }

    #[test]
    fn stub_ask_agent_uses_overview_for_kb_wide_questions() {
        let root = unique_temp_dir("stub-overview");
        let retrieved = vec![write_stub_overview_fixture(&root)];

        let answer = run_stub_ask_agent(
            &root,
            "\u{77e5}\u{8bc6}\u{5e93}\u{7684}\u{4e3b}\u{8981}\u{5185}\u{5bb9}",
            &retrieved,
            3,
            Some("test-provider"),
        )
        .expect("stub ask should succeed");

        assert!(answer.answer_text.contains(
            "\u{8fd9}\u{4e2a}\u{77e5}\u{8bc6}\u{5e93}\u{4e3b}\u{8981}\u{6574}\u{7406}\u{7b97}\u{6cd5}\u{9898}\u{89e3}"
        ));
    }

    #[test]
    fn overview_question_detector_handles_chinese_and_english() {
        assert!(question_seeks_kb_overview("知识库的主要内容"));
        assert!(question_seeks_kb_overview("What is this knowledge base about?"));
        assert!(!question_seeks_kb_overview("1665 这道题怎么做"));
    }
}
