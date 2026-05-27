pub(crate) fn run_env(list: bool) -> Result<Value, CliError> {
    let mode = if list { "list" } else { "list(default)" };
    let out = list_existing_kb_roots();
    let items = out
        .existing_paths
        .iter()
        .map(|path| json!({ "path": path }))
        .collect::<Vec<_>>();

    let data = json!({
        "mode": mode,
        "items": items,
        "count": out.existing_paths.len(),
        "recorded_total": out.recorded_total,
        "config_path": out.config_path
    });

    let text = format_env_text(&out.config_path, out.recorded_total, &out.existing_paths);
    Ok(with_text(data, text))
}

/// `kb env --doctor` 的实现。
///
/// 目的：把 LLM 配置解析这条"看不见的链路"显性化，方便用户排查
/// "key 究竟从哪里读到 / 为什么 LLM 没启用"。
///
/// 输出包含：
/// - key / model / base_url 各自最终生效的来源（process_env / keyring /
///   project_config / global_config / defaults）；
/// - keyring 当前 service / user 名（含 env override 影响），以及是否被
///   `KB_LLM_DISABLE_KEYRING=1` 关闭；
/// - keyring 里那条记录是否真的存在（不打印 key 本身）；
/// - 已解析的 trace（kb-llm 自己记的层级摘要）。
pub(crate) fn run_env_doctor() -> Result<Value, CliError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let llm_runtime = resolve_llm_config(&cwd);
    let keyring = probe_keyring_status();

    let data = json!({
        "mode": "doctor",
        "cwd": cwd.to_string_lossy(),
        "llm": {
            "model": llm_runtime.model,
            "base_url": llm_runtime.base_url,
            "key_present": llm_runtime.key.is_some(),
            "semantic_enabled": llm_runtime.semantic_enabled,
            "graph_enabled": llm_runtime.graph_enabled,
            "wiki_enabled": llm_runtime.wiki_enabled,
        },
        "key_source": llm_runtime.source.key.as_str(),
        "source_map": {
            "key": llm_runtime.source.key.as_str(),
            "model": llm_runtime.source.model.as_str(),
            "base_url": llm_runtime.source.base_url.as_str(),
        },
        "keyring": {
            "service": keyring.service,
            "user": keyring.user,
            "disabled": keyring.disabled,
            "key_present": keyring.key_present,
        },
        "trace": llm_runtime.trace,
    });

    let text = format_env_doctor_text(&cwd, &llm_runtime, &keyring);
    Ok(with_text(data, text))
}

fn format_env_doctor_text(
    cwd: &Path,
    runtime: &kb_llm::LlmRuntimeConfig,
    keyring: &KeyringStatus,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "KB LLM doctor");
    let _ = writeln!(out, "cwd: {}", cwd.display());
    let _ = writeln!(out, "model: {}", runtime.model);
    let _ = writeln!(out, "base_url: {}", runtime.base_url);
    let _ = writeln!(
        out,
        "key_present: {} (source: {})",
        runtime.key.is_some(),
        runtime.source.key.as_str()
    );
    let _ = writeln!(out, "model_source: {}", runtime.source.model.as_str());
    let _ = writeln!(out, "base_url_source: {}", runtime.source.base_url.as_str());
    let _ = writeln!(out, "semantic_enabled: {}", runtime.semantic_enabled);
    let _ = writeln!(out, "graph_enabled: {}", runtime.graph_enabled);
    let _ = writeln!(out, "wiki_enabled: {}", runtime.wiki_enabled);
    out.push_str("keyring:\n");
    let _ = writeln!(out, "  service: {}", keyring.service);
    let _ = writeln!(out, "  user: {}", keyring.user);
    let _ = writeln!(out, "  disabled: {}", keyring.disabled);
    let _ = writeln!(out, "  key_present: {}", keyring.key_present);
    if !runtime.trace.is_empty() {
        out.push_str("trace:\n");
        for line in runtime.trace.lines() {
            let _ = writeln!(out, "  {}", line);
        }
    }
    out
}

