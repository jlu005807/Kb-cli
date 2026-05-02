fn build_ask_stack(cwd: &Path) -> (Box<dyn Retriever>, Box<dyn AnswerEngine>) {
    let llm = resolve_llm_config(cwd);
    let has_llm_credentials = llm.key.is_some();
    if has_llm_credentials {
        let provider = format!("openai-compatible:{}@{}", llm.model, llm.base_url);
        return (
            Box::new(DefaultRetriever),
            Box::new(ConfiguredAnswerEngine::new(provider)),
        );
    }

    (Box::new(DefaultRetriever), Box::new(DisabledAnswerEngine))
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
