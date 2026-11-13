pub(crate) fn run_init(name: Option<String>, force: bool, verbose: u8) -> Result<Value, CliError> {
    let cwd = std::env::current_dir().map_err(|_| write_config_error())?;
    let kb_md = cwd.join("kb.md");
    let already_initialized = kb_md.exists() && cwd.join(".kb").exists();
    let global_config_path = resolve_global_config_path();
    let kb_guide_rel = "./.kb/README.md";

    let plan =
        plan_init(project_name(&cwd), name, already_initialized, force).map_err(
            |err| match err {
                InitPlanError::AlreadyInitialized => {
                    map_core_error(CoreErrorKind::AlreadyInitialized, json!({}))
                }
            },
        )?;

    fs::create_dir_all(cwd.join(".kb")).map_err(|_| write_config_error())?;
    let writer_lock = crate::commands::acquire_writer_lock(&cwd)?;
    if writer_lock.unsupported && verbose > 0 {
        eprintln!("warning: filesystem does not support writer locks; continuing without a lock");
    }
    let _writer_lock = writer_lock;
    if !force && kb_md.exists() && cwd.join(".kb").is_dir() {
        return Err(map_core_error(CoreErrorKind::AlreadyInitialized, json!({})));
    }
    fs::create_dir_all(cwd.join(RAW_DIR_REL.trim_start_matches("./")))
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(KBIGNORE_REL), DEFAULT_KBIGNORE_CONTENT).map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/.env.template"), DEFAULT_LLM_ENV_TEMPLATE)
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/config.json"), DEFAULT_PROJECT_LLM_CONFIG)
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/README.md"), default_kb_structure_guide())
        .map_err(|_| write_config_error())?;
    // kb.md 与 .kb/*.json 一样走原子写，避免 init 中断留下半写入的 kb.md。
    kb_storage_fs::atomic_write_bytes(&kb_md, b"# raw\n").map_err(|_| write_config_error())?;
    save_state(&cwd, &State::default())?;
    save_registry(&cwd, &RegistryState::default())?;
    // 维护 .gitignore：把派生层 + LLM key 配置文件加入忽略列表。
    // 失败不阻塞 init（用户的 git 工作区不一定可写、也可能根本不是 git 仓库）。
    let gitignore_updated = ensure_kb_gitignore_entries(&cwd).ok();
    let global_register_error = register_kb_root(&cwd).err();
    let global_config_registered = global_register_error.is_none();

    let mut data = json!({
        "project": plan.project,
        "path": "./kb.md",
        "project_config_path": "./.kb/config.json",
        "kb_guide_path": kb_guide_rel,
        "global_config_path": global_config_path.to_string_lossy(),
        "global_config_registered": global_config_registered,
        "gitignore_updated": gitignore_updated
    });
    if let Some(message) = global_register_error.as_ref() {
        data["global_config_error"] = Value::String(message.clone());
    }
    if force {
        data["overwritten"] = Value::Bool(plan.overwritten);
    }

    let mut text = String::new();
    text.push_str("Knowledge base initialized\n");
    let _ = writeln!(text, "project: {}", data["project"].as_str().unwrap_or_default());
    text.push_str("path: ./kb.md\n");
    text.push_str("ignore_file: ./.kb/.kbignore\n");
    text.push_str("raw_dir: ./.kb/raw\n");
    text.push_str("kb_guide: ./.kb/README.md\n");
    text.push_str("llm_env_template: ./.kb/.env.template\n");
    text.push_str("llm_project_config: ./.kb/config.json\n");
    let _ = writeln!(
        text,
        "llm_global_config: {}",
        global_config_path.to_string_lossy()
    );
    let _ = writeln!(
        text,
        "llm_global_config_registered: {}",
        global_config_registered
    );
    if let Some(message) = global_register_error {
        let _ = writeln!(text, "llm_global_config_error: {}", message);
    }
    if let Some(updated) = gitignore_updated {
        let _ = writeln!(
            text,
            "gitignore_updated: {}",
            if updated { "true (entries appended)" } else { "true (already up to date)" }
        );
    } else {
        text.push_str("gitignore_updated: false (write failed, ignored)\n");
    }
    if force {
        let _ = writeln!(
            text,
            "overwritten: {}",
            data["overwritten"].as_bool().unwrap_or(false)
        );
    }
    Ok(with_text(data, text))
}
