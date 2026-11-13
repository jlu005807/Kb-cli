// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 3：MCP 工具实现（续）——verify_citation / list_episodes /
// read_summary / read_neighbors。

/// verify_citation：找到 chunk -> 收集候选证据文本（chunk preview + 源文件
/// 切片）-> 调 kb_core::verify_citation 做词法校验。
fn mcp_tool_verify_citation(args: &Value) -> Result<Value, String> {
    let chunk_id = args
        .get("chunk_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "verify_citation requires chunk_id".to_string())?;
    let quote = args
        .get("quote")
        .and_then(Value::as_str)
        .ok_or_else(|| "verify_citation requires quote".to_string())?;
    let cwd = mcp_cwd()?;

    let record_id = kb_core::record_id_of_chunk(chunk_id);
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd.clone()));

    // 候选证据：优先源文件按 chunk 切出的片段；附带 preview 作为兜底。
    let mut candidates: Vec<String> = Vec::new();
    let mut stale = false;
    if let Some(raw) = toolbox.read_raw_chunk(record_id, Some(chunk_id)) {
        stale = raw.stale;
        candidates.push(raw.preview);
    }
    if let Some(summary) = toolbox.read_file_summary(record_id) {
        // summary item 可能带 chunk preview 列表；序列化后并入候选文本。
        if let Ok(text) = serde_json::to_string(&summary) {
            candidates.push(text);
        }
    }

    if candidates.is_empty() {
        return Ok(json!({
            "chunk_id": chunk_id,
            "matched": false,
            "stale": false,
            "reason": "chunk not found or source unreadable; run `kb update` first",
        }));
    }

    let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    let verdict = kb_core::verify_citation(quote, &candidate_refs);

    let mut result = json!({
        "chunk_id": chunk_id,
        "quote": quote,
        "matched": verdict.matched,
        "method": verdict.method,
        "coverage": verdict.coverage,
        "stale": stale,
    });
    if stale {
        result["warning"] = json!("Source content changed; run `kb update` to refresh citations.");
    }
    Ok(result)
}

/// list_episodes：按 agent_id（可选）+ limit 拉取最近 episode。
fn mcp_tool_list_episodes(args: &Value) -> Result<Value, String> {
    let cwd = mcp_cwd()?;
    let storage = StorageHandles::open(&cwd).map_err(|err| {
        let error = map_core_error(
            CoreErrorKind::StorageUnavailable,
            json!({ "reason": "open episode storage failed", "detail": err.to_string() }),
        );
        format!("{}: {}", error.code, error.message)
    })?;

    let mut filter = kb_core::EpisodeFilter::default();
    if let Some(agent) = args.get("agent_id").and_then(Value::as_str) {
        if let Ok(id) = kb_core::AgentId::new(agent) {
            filter.agent_id = Some(id);
        }
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(20);
    filter.limit = Some(limit);

    let episodes = storage
        .episode
        .query(&filter)
        .map_err(|err| {
            let error = map_core_error(
                CoreErrorKind::StorageUnavailable,
                json!({ "reason": "episode query failed", "detail": err.to_string() }),
            );
            format!("{}: {}", error.code, error.message)
        })?;

    let items: Vec<Value> = episodes
        .iter()
        .map(|ep| {
            json!({
                "id": ep.id,
                "timestamp": ep.timestamp.as_nanos(),
                "agent_id": ep.agent_id.as_str(),
                "session_id": ep.session_id,
                "kind": serde_json::to_value(&ep.kind).unwrap_or(Value::Null),
                "content": ep.content,
                "refs": ep.refs,
                "confidence": ep.confidence,
            })
        })
        .collect();

    Ok(json!({ "count": items.len(), "episodes": items }))
}

/// sqlite 后端下 SemanticAgentToolbox 直接读 .kb/*.json 文件布局，L2/L4/L5
/// 数据实际落在 store.sqlite3 —— 读不到时给出显式提示而不是让调用方猜。
fn mcp_sqlite_toolbox_warning(cwd: &std::path::Path) -> Option<String> {
    if matches!(
        crate::storage_backend::resolve_storage_backend(cwd),
        crate::storage_backend::StorageBackend::Sqlite
    ) {
        Some(
            "storage.backend=sqlite: this tool reads the fs JSON layout (.kb/knowledge.json etc.) and may report missing data"
                .to_string(),
        )
    } else {
        None
    }
}

fn mcp_tool_read_summary(args: &Value) -> Result<Value, String> {
    let target = args
        .get("record_id_or_path")
        .and_then(Value::as_str)
        .ok_or_else(|| "read_summary requires record_id_or_path".to_string())?;
    let cwd = mcp_cwd()?;
    let warning = mcp_sqlite_toolbox_warning(&cwd);
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd));
    match toolbox.read_file_summary(target) {
        Some(item) => serde_json::to_value(item).map_err(|e| e.to_string()),
        None => {
            let mut data = json!({ "found": false, "target": target });
            if let (Some(obj), Some(warning)) = (data.as_object_mut(), warning) {
                obj.insert("warning".to_string(), Value::String(warning));
            }
            Ok(data)
        }
    }
}

fn mcp_tool_read_neighbors(args: &Value) -> Result<Value, String> {
    let record_id = args
        .get("record_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "read_neighbors requires record_id".to_string())?;
    let cwd = mcp_cwd()?;
    let warning = mcp_sqlite_toolbox_warning(&cwd);
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd));
    let relations = toolbox.read_graph_neighbors(record_id);
    let mut data = json!({
        "record_id": record_id,
        "neighbors": relations,
    });
    if let (Some(obj), Some(warning)) = (data.as_object_mut(), warning) {
        obj.insert("warning".to_string(), Value::String(warning));
    }
    Ok(data)
}
