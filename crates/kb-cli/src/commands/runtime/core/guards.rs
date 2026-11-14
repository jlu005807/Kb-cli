fn build_ask_stack(cwd: &Path) -> (Box<dyn Retriever>, Box<dyn AnswerEngine>) {
    // 走 StorageHandles 的好处：sqlite 后端下也能让 ask 命中同一份数据；
    // 调用入口已成功打开存储并读取索引；此处保留派生层的 best-effort 降级。
    let snapshots = StorageHandles::open(cwd).ok();
    let (search_snap, knowledge_snap, graph_snap) = match snapshots {
        Some(handles) => (
            handles.search.read_search().ok().flatten(),
            handles.knowledge.read_knowledge().ok().flatten(),
            handles.graph.read_graph().ok().flatten(),
        ),
        None => (None, None, None),
    };
    let retriever: Box<dyn Retriever> =
        Box::new(LayeredRetriever::new(search_snap, knowledge_snap, graph_snap));

    let llm = resolve_llm_config(cwd);
    let has_llm_credentials = llm.key.is_some();
    if has_llm_credentials {
        let provider = format!("openai-compatible:{}@{}", llm.model, llm.base_url);
        return (
            retriever,
            Box::new(ConfiguredAnswerEngine::new(provider)),
        );
    }

    (retriever, Box::new(DisabledAnswerEngine))
}

fn ensure_initialized() -> Result<PathBuf, CliError> {
    let cwd = std::env::current_dir()
        .map_err(|_| map_core_error(CoreErrorKind::NotInitialized, json!({})))?;
    if !cwd.join("kb.md").exists() {
        return Err(map_core_error(CoreErrorKind::NotInitialized, json!({})));
    }
    Ok(cwd)
}

fn is_protected_index_target(cwd: &Path, absolute_candidate: &Path) -> bool {
    let kb_md_path = cwd.join("kb.md");
    if absolute_candidate == kb_md_path {
        return true;
    }

    let kb_dir = cwd.join(".kb");
    let raw_dir = cwd.join(RAW_DIR_REL.trim_start_matches("./"));
    absolute_candidate.starts_with(&kb_dir) && !absolute_candidate.starts_with(&raw_dir)
}
