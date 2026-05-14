pub(crate) fn run_completion(shell: clap_complete::Shell) -> Result<Value, CliError> {
    let mut cmd = Cli::command();
    let mut buf: Vec<u8> = Vec::new();
    generate(shell, &mut cmd, "kb", &mut buf);
    let script = String::from_utf8(buf).map_err(|_| write_config_error())?;
    Ok(json!({
        "shell": shell.to_string().to_ascii_lowercase(),
        "_raw_text": script,
        "script": script,
    }))
}

pub(crate) fn run_init(name: Option<String>, force: bool) -> Result<Value, CliError> {
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
    fs::create_dir_all(cwd.join(RAW_DIR_REL.trim_start_matches("./")))
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(KBIGNORE_REL), DEFAULT_KBIGNORE_CONTENT).map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/.env.template"), DEFAULT_LLM_ENV_TEMPLATE)
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/config.json"), DEFAULT_PROJECT_LLM_CONFIG)
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join(".kb/README.md"), default_kb_structure_guide())
        .map_err(|_| write_config_error())?;
    fs::write(&kb_md, "# raw\n").map_err(|_| write_config_error())?;
    save_state(&cwd, &State::default())?;
    save_registry(&cwd, &RegistryState::default())?;
    let global_register_error = register_kb_root(&cwd).err();
    let global_config_registered = global_register_error.is_none();

    let mut data = json!({
        "project": plan.project,
        "path": "./kb.md",
        "project_config_path": "./.kb/config.json",
        "kb_guide_path": kb_guide_rel,
        "global_config_path": global_config_path.to_string_lossy(),
        "global_config_registered": global_config_registered
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
    if force {
        let _ = writeln!(
            text,
            "overwritten: {}",
            data["overwritten"].as_bool().unwrap_or(false)
        );
    }
    Ok(with_text(data, text))
}

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

pub(crate) fn run_add(
    path: String,
    alias: Option<String>,
    mode: AddMode,
    category: Option<String>,
) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let normalized_path = normalize_path(&path);
    let src_path = cwd.join(path.trim_start_matches("./"));

    if is_protected_index_target(&cwd, &src_path) {
        return Err(map_core_error(
            CoreErrorKind::PathForbidden,
            json!({
                "path": normalized_path,
                "reason": "reserved_internal_path"
            }),
        ));
    }

    let source_exists = src_path.exists();

    let ignore_rules = load_ignore_rules(&cwd);
    if is_ignored(&ignore_rules, &normalized_path) {
        let data = json!({
            "alias": alias.clone().unwrap_or_else(|| normalized_path.clone()),
            "path": normalized_path,
            "ignored": true,
            "message": "skipped by .kbignore"
        });
        let mut text = String::new();
        text.push_str("Add skipped\n");
        let _ = writeln!(text, "reason: {}", data["message"].as_str().unwrap_or_default());
        let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
        let _ = writeln!(text, "path: {}", data["path"].as_str().unwrap_or_default());
        return Ok(with_text(data, text));
    }

    let mut state = load_state(&cwd)?;
    let categories =
        parse_categories(&cwd.join("kb.md")).unwrap_or_else(|_| vec!["raw".to_string()]);
    let plan = plan_add(AddPlanInput {
        normalized_path: normalized_path.clone(),
        alias,
        mode: if matches!(mode, AddMode::Copy) {
            AddModePlan::Copy
        } else {
            AddModePlan::Link
        },
        requested_category: category,
        available_categories: categories,
        existing_aliases: state.refs.iter().map(|r| r.alias.clone()).collect(),
        existing_paths: state.refs.iter().map(|r| r.path.clone()).collect(),
        ignored: false,
        source_exists,
    })
    .map_err(map_add_plan_error)?;

    let alias_value = plan.alias;
    let chosen_category = plan.category;

    let final_path = if matches!(mode, AddMode::Copy) {
    let file_name = src_path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("copied");
        let target_rel = format!("{RAW_PREFIX}{file_name}");
        let target_abs = cwd.join(target_rel.trim_start_matches("./"));
        if src_path.is_file() {
            fs::copy(&src_path, &target_abs).map_err(|_| write_config_error())?;
        }
        target_rel
    } else {
        normalized_path
    };

    append_to_kb_md(
        &cwd.join("kb.md"),
        &chosen_category,
        &alias_value,
        &final_path,
    )
    .map_err(|_| write_config_error())?;
    state.refs.push(RefEntry {
        alias: alias_value.clone(),
        path: final_path.clone(),
        category: chosen_category.clone(),
        perms: "rwx".to_string(),
    });
    save_state(&cwd, &state).map_err(|_| write_config_error())?;

    let data = json!({"alias": alias_value, "path": final_path});
    let mut text = String::new();
    text.push_str("Reference added\n");
    let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
    let _ = writeln!(text, "path: {}", data["path"].as_str().unwrap_or_default());
    Ok(with_text(data, text))
}

fn map_add_plan_error(err: AddPlanError) -> CliError {
    match err {
        AddPlanError::SourcePathNotFound { path } => {
            map_core_error(CoreErrorKind::PathMissing, json!({"path": path}))
        }
        AddPlanError::InvalidAlias { alias } => {
            map_core_error(CoreErrorKind::AliasInvalid, json!({"alias": alias}))
        }
        AddPlanError::AliasConflict { alias } => {
            map_core_error(CoreErrorKind::AliasConflict, json!({"alias": alias}))
        }
        AddPlanError::PathAlreadyRegistered { path } => {
            map_core_error(CoreErrorKind::PathAlreadyRegistered, json!({"path": path}))
        }
        AddPlanError::CategoryNotFound {
            category,
            available,
        } => map_core_error(
            CoreErrorKind::CategoryNotFound,
            json!({"category": category, "available": available}),
        ),
    }
}

fn map_mv_plan_error(err: MovePlanError) -> CliError {
    match err {
        MovePlanError::InvalidAlias { alias } => alias_invalid_error(alias),
        MovePlanError::AliasConflict { alias } => alias_conflict_error(alias),
        MovePlanError::NotFoundAlias { alias } => alias_not_found_error(alias),
    }
}

fn map_path_plan_error(err: PathPlanError) -> CliError {
    match err {
        PathPlanError::NotFoundAlias { alias } => alias_not_found_error(alias),
    }
}

fn map_category_filter_error(err: CategoryFilterError) -> CliError {
    match err {
        CategoryFilterError::CategoryNotFound { category } => map_core_error(
            CoreErrorKind::CategoryNotFound,
            json!({"category": category}),
        ),
    }
}

pub(crate) fn run_mv(alias: String, new_alias: String) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;
    let next = apply_mv(&state, &alias, &new_alias).map_err(map_mv_plan_error)?;
    save_state_snapshot(&store, &next)?;
    let data = json!({"old_alias": alias, "new_alias": new_alias});
    let mut text = String::new();
    text.push_str("Alias renamed\n");
    let _ = writeln!(
        text,
        "from: {}",
        data["old_alias"].as_str().unwrap_or_default()
    );
    let _ = writeln!(text, "to: {}", data["new_alias"].as_str().unwrap_or_default());
    Ok(with_text(data, text))
}

fn confirm_raw_source_delete(path: &str) -> bool {
    use std::io::Write as _;

    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "Raw source file detected: {path}\nDelete source file as well? [y/N]"
    );
    let _ = stderr.flush();

    let mut input = String::new();
    match std::io::stdin().read_line(&mut input) {
        Ok(0) => false,
        Ok(_) => matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        Err(_) => false,
    }
}

pub(crate) fn run_rm(alias: String, force: bool) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let kb_md = cwd.join("kb.md");
    let kb_doc = parse_kb_document_file(&kb_md)?;
    let Some(target_ref) = resolve_rm_target(&kb_doc, &alias) else {
        return Err(alias_not_found_error(alias));
    };
    let (removed_alias, removed_path) = target_ref.identity();

    if removed_path.starts_with(RAW_PREFIX) {
        let confirmed = if force {
            true
        } else {
            confirm_raw_source_delete(&removed_path)
        };
        if !confirmed {
            let data = json!({
                "ignored": true,
                "reason": "raw_delete_not_confirmed",
                "alias": removed_alias,
                "path": removed_path,
            });
            let mut text = String::new();
            text.push_str("Remove ignored\n");
            let _ = writeln!(text, "reason: raw_delete_not_confirmed");
            let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
            let _ = writeln!(text, "path: {}", data["path"].as_str().unwrap_or_default());
            return Ok(with_text(data, text));
        }

        let raw_abs = cwd.join(removed_path.trim_start_matches("./"));
        if raw_abs.is_file() {
            fs::remove_file(&raw_abs).map_err(|_| write_config_error())?;
        }
    }

    let removed = apply_rm_to_kb_md(&kb_md, &kb_doc, &alias)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
    let Some((removed_alias, removed_path)) = removed else {
        return Err(alias_not_found_error(alias));
    };
    let mut state = load_state(&cwd).map_err(|_| write_config_error())?;
    state
        .refs
        .retain(|item| normalize_path(&item.path) != removed_path);
    save_state(&cwd, &state).map_err(|_| write_config_error())?;

    let data = json!({"removed_alias": removed_alias, "path": removed_path});
    let mut text = String::new();
    text.push_str("Alias removed\n");
    let _ = writeln!(
        text,
        "alias: {}",
        data["removed_alias"].as_str().unwrap_or_default()
    );
    let _ = writeln!(text, "path: {}", data["path"].as_str().unwrap_or_default());
    Ok(with_text(data, text))
}

pub(crate) fn run_path(alias: String, new_path: Option<String>) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd.clone());
    let state = load_state_snapshot(&store)?;
    if let Some(new_path_value) = new_path {
        let normalized = normalize_path(&new_path_value);
        if !cwd.join(normalized.trim_start_matches("./")).exists() {
            return Err(map_core_error(
                CoreErrorKind::PathMissing,
                json!({"path": new_path_value}),
            ));
        }
        let plan =
            apply_path(&state, &alias, Some(normalized.clone())).map_err(map_path_plan_error)?;
        if let Some(next) = plan.state {
            save_state_snapshot(&store, &next)?;
        }
        let data =
            json!({"alias": alias, "old_path": plan.old_path.unwrap_or_default(), "new_path": normalized});
        let mut text = String::new();
        text.push_str("Alias path updated\n");
        let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
        let _ = writeln!(
            text,
            "old_path: {}",
            data["old_path"].as_str().unwrap_or_default()
        );
        let _ = writeln!(
            text,
            "new_path: {}",
            data["new_path"].as_str().unwrap_or_default()
        );
        Ok(with_text(data, text))
    } else {
        let plan = apply_path(&state, &alias, None).map_err(map_path_plan_error)?;
        let data = json!({"alias": alias, "path": plan.path});
        let mut text = String::new();
        text.push_str("Alias path\n");
        let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
        let _ = writeln!(text, "path: {}", data["path"].as_str().unwrap_or_default());
        Ok(with_text(data, text))
    }
}

pub(crate) fn run_chmod(alias: String, perms: String) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    if Permission::parse(&perms).is_err() {
        return Err(map_core_error(
            CoreErrorKind::InvalidPermission,
            json!({"provided": perms}),
        ));
    }

    let kb_md = cwd.join("kb.md");
    let kb_doc = parse_kb_document_file(&kb_md)?;
    let changed = apply_chmod_to_kb_md(&kb_md, &kb_doc, &alias, &perms)
        .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
    if !changed {
        return Err(alias_not_found_error(alias));
    }

    let data = json!({"alias": alias, "permissions": perms});
    let mut text = String::new();
    text.push_str("Permissions updated\n");
    let _ = writeln!(text, "alias: {}", data["alias"].as_str().unwrap_or_default());
    let _ = writeln!(
        text,
        "permissions: {}",
        data["permissions"].as_str().unwrap_or_default()
    );
    Ok(with_text(data, text))
}

pub(crate) fn run_list(long: bool, tree: bool, filter: Option<String>) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let registry = load_registry(&cwd)?;
    let records = regs_to_list_records(&cwd, &registry.regs);
    let snapshot = IndexSnapshot {
        records,
        errors: vec![],
        meta: kb_core::domain::IndexMeta {
            full: false,
            baseline_size: 0,
            tracked_size: registry.regs.len(),
        },
    };
    let view = IndexEngine
        .render_list(Some(&snapshot), filter.as_deref())
        .map_err(|_| {
            map_core_error(
                CoreErrorKind::InvalidGlobPattern,
                json!({"pattern": filter.unwrap_or_default()}),
            )
        })?;

    let perms_by_alias: HashMap<&str, &str> = registry
        .regs
        .iter()
        .map(|r| (r.id.as_str(), r.perms.as_str()))
        .collect();

    let mut rows = Vec::new();
    for item in view.items {
        let perms = perms_by_alias
            .get(item.alias.as_str())
            .copied()
            .unwrap_or("rwx")
            .to_string();
        let mut row = ListRow {
            alias: item.alias,
            path: item.path,
            perms,
            category: item.category,
            size: None,
            mtime: None,
        };
        if long {
            let abs = cwd.join(row.path.trim_start_matches("./"));
            let (size, mtime) = metadata_summary(&abs);
            row.size = Some(size);
            row.mtime = Some(mtime);
        }
        rows.push(row);
    }

    let plan = plan_list_output(rows, tree, view.categories.keys().cloned().collect());
    let text = format_list_text(&plan.items, plan.categories.as_ref());

    let mut items = Vec::new();
    for row in plan.items {
        let mut obj = json!({
            "alias": row.alias,
            "path": row.path,
            "perms": row.perms,
        });
        if let Some(size) = row.size {
            obj["size"] = Value::Number(size.into());
        }
        if let Some(mtime) = row.mtime {
            obj["mtime"] = Value::String(mtime);
        }
        items.push(obj);
    }

    let mut data = json!({"items": items});
    if let Some(categories) = plan.categories {
        data["categories"] = Value::Array(categories.into_iter().map(Value::String).collect());
    }
    Ok(with_text(data, text))
}

pub(crate) fn run_check(
    raw_only: bool,
    category: Option<String>,
    top: Option<u32>,
) -> Result<Value, CliError> {
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    let kb_doc = parse_kb_document_file(&cwd.join("kb.md"))?;
    let categories = kb_doc.categories.clone();
    validate_category_filter(category.as_deref(), &categories)
        .map_err(map_category_filter_error)?;

    let ignored = load_ignore_rules(&cwd).into_iter().collect::<BTreeSet<_>>();
    let desired = collect_desired_entries(
        &cwd,
        &kb_doc,
        &ignored,
        raw_only,
        category.as_deref(),
    )?;
    let registry = load_registry(&cwd)?;

    let index_store = FsIndexStore::new(cwd.clone());
    let baseline = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let diff = compute_registry_diff(&cwd, &registry.regs, &desired, baseline.as_ref());
    let limit = normalize_top(top, 50);

    let has_changes =
        !diff.to_add.is_empty() || !diff.to_update.is_empty() || !diff.to_remove.is_empty();
    let duration_ms = start.elapsed().as_millis() as u64;
    let text = format_check_text(
        has_changes,
        &diff.to_add,
        &diff.to_update,
        &diff.to_remove,
        limit,
        duration_ms,
    );

    let data = json!({
        "has_changes": has_changes,
        "to_add": to_add_json(limit, &diff.to_add),
        "to_update": to_change_json(limit, &diff.to_update),
        "to_remove": to_change_json(limit, &diff.to_remove),
        "suggested_command": "kb update",
        "duration_ms": duration_ms,
    });
    Ok(with_text(data, text))
}

const LAYER_L0_DECLARATION: &str = "l0_declaration";
const LAYER_L1_REGISTRY: &str = "l1_registry";
const LAYER_L1_INDEX: &str = "l1_index";
const LAYER_L2_SEMANTIC: &str = "l2_semantic";
const LAYER_L3_PROJECTION: &str = "l3_projection";
const LAYER_L4_GRAPH: &str = "l4_graph";
const LAYER_L5_WIKI_VIEW: &str = "l5_wiki_view";
const LAYER_L3_L4_PROJECTION_LEGACY: &str = "l3_l4_projection";
const UPDATE_LAYER_FAILURES_ENV: &str = "KB_UPDATE_SIMULATE_LAYER_FAILURES";

#[derive(Debug, Clone)]
struct LayerIssue {
    layer: &'static str,
    code: String,
    message: String,
    retryable: bool,
}

fn parse_simulated_layer_failures() -> HashSet<String> {
    std::env::var(UPDATE_LAYER_FAILURES_ENV)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

fn collect_progressive_layer_issues() -> Vec<LayerIssue> {
    let injected = parse_simulated_layer_failures();
    let mut issues = Vec::new();
    let legacy_l3_l4 = injected.contains(LAYER_L3_L4_PROJECTION_LEGACY);

    if injected.contains(LAYER_L2_SEMANTIC) {
        issues.push(LayerIssue {
            layer: LAYER_L2_SEMANTIC,
            code: "W-L2-EXTRACT-FAILED".to_string(),
            message: "Semantic extraction stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L3_PROJECTION) || legacy_l3_l4 {
        issues.push(LayerIssue {
            layer: LAYER_L3_PROJECTION,
            code: "W-L3-PROJECTION-FAILED".to_string(),
            message: "Search projection stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L4_GRAPH) || legacy_l3_l4 {
        issues.push(LayerIssue {
            layer: LAYER_L4_GRAPH,
            code: "W-L4-GRAPH-FAILED".to_string(),
            message: "Graph build stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L5_WIKI_VIEW) {
        issues.push(LayerIssue {
            layer: LAYER_L5_WIKI_VIEW,
            code: "W-L5-WIKI-VIEW-FAILED".to_string(),
            message: "Wiki markdown view stage failed".to_string(),
            retryable: true,
        });
    }
    issues
}

fn issue_json(issue: &LayerIssue) -> Value {
    json!({
        "layer": issue.layer,
        "code": issue.code,
        "message": issue.message,
        "retryable": issue.retryable,
    })
}

fn issue_text(issue: &LayerIssue) -> String {
    format!(
        "[{}] {}: {} ({})",
        issue.layer,
        issue.code,
        issue.message,
        if issue.retryable {
            "retryable"
        } else {
            "non-retryable"
        }
    )
}

fn generated_at_unix_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", secs)
}

#[derive(Debug)]
struct AskAgentAnswerOutput {
    agent_backend: String,
    agent_warning: Option<String>,
    agent_trace: Vec<Value>,
    tool_trace: Vec<Value>,
    answer_text: String,
    citations: Vec<Value>,
}

#[derive(Debug)]
struct AskToolResult {
    summary: String,
    json: String,
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
    out
}

fn ask_agent_tools_schema() -> Vec<Value> {
    vec![
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
                "description": "Read one wiki page excerpt by page_id.",
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
    ]
}

fn send_chat_completion(
    endpoint: &str,
    api_key: &str,
    timeout_secs: u64,
    body: &Value,
) -> Result<Value, String> {
    let request = ureq::post(endpoint)
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(timeout_secs));

    let raw = match request.send_string(&body.to_string()) {
        Ok(response) => response
            .into_string()
            .map_err(|err| format!("failed to read ask llm response: {}", err))?,
        Err(ureq::Error::Status(code, response)) => {
            let body_text = response
                .into_string()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(format!(
                "http status {} from ask llm provider: {}",
                code, body_text
            ));
        }
        Err(err) => return Err(format!("ask llm request failed: {}", err)),
    };
    serde_json::from_str(&raw).map_err(|err| format!("invalid ask llm json response: {}", err))
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

fn emit_runtime_trace(prefix: &str, message: &str) {
    eprintln!("[{}] {}", prefix, message);
}

fn emit_ask_trace(message: &str) {
    emit_runtime_trace("ask-agent", message);
}

fn truncate_trace_text(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let mut out = input.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn push_agent_trace(
    trace: &mut Vec<Value>,
    round: Option<usize>,
    kind: &str,
    message: impl Into<String>,
    tool: Option<&str>,
) {
    let message = message.into();
    let mut item = json!({
        "kind": kind,
        "message": message,
    });
    if let Some(obj) = item.as_object_mut() {
        obj.insert(
            "round".to_string(),
            round.map(Value::from).unwrap_or(Value::Null),
        );
        obj.insert(
            "tool".to_string(),
            tool.map(Value::from).unwrap_or(Value::Null),
        );
    }
    trace.push(item);
}

fn extract_chat_message_content(message: &Value) -> Option<String> {
    let message_content = message.get("content")?;
    if let Some(text) = message_content.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(parts) = message_content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        let trimmed = joined.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn extract_first_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in text[start..].char_indices() {
        let absolute_idx = start + idx;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&text[start..=absolute_idx]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim().trim_start_matches('\u{feff}').trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.is_object() {
            return Ok(value);
        }
    }

    if let Some(candidate) = extract_first_json_object(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            if value.is_object() {
                return Ok(value);
            }
        }
    }

    Err(format!(
        "ask agent content is not a valid json object; excerpt={:?}",
        truncate_trace_text(trimmed, 320)
    ))
}

fn supports_ask_provider_fallback(error_text: &str) -> bool {
    let lowered = error_text.to_ascii_lowercase();
    lowered.contains("max_completion_tokens")
        || lowered.contains("max_tokens")
        || lowered.contains("response_format")
        || lowered.contains("json_object")
        || lowered.contains("unknown field")
        || lowered.contains("unrecognized field")
        || lowered.contains("invalid request")
        || lowered.contains("unsupported parameter")
        || lowered.contains("unsupported field")
        || lowered.contains("unexpected field")
}

fn build_ask_chat_completion_body(
    model: &str,
    temperature: f64,
    max_output_tokens: u32,
    messages: &[Value],
    tools: &[Value],
    max_tokens_key: &str,
    include_response_format: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "tools": tools,
        "tool_choice": "auto",
    });
    if let Some(obj) = body.as_object_mut() {
        obj.insert(max_tokens_key.to_string(), json!(max_output_tokens));
        if include_response_format {
            obj.insert(
                "response_format".to_string(),
                json!({ "type": "json_object" }),
            );
        }
    }
    body
}

fn send_ask_chat_completion_once(
    endpoint: &str,
    api_key: &str,
    timeout_secs: u64,
    model: &str,
    temperature: f64,
    max_output_tokens: u32,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, String> {
    let variants = [
        (
            "max_completion_tokens+response_format",
            "max_completion_tokens",
            true,
        ),
        ("max_tokens+response_format", "max_tokens", true),
        ("max_completion_tokens", "max_completion_tokens", false),
        ("max_tokens", "max_tokens", false),
    ];
    let mut errors = Vec::new();
    for (label, max_tokens_key, include_response_format) in variants {
        let body = build_ask_chat_completion_body(
            model,
            temperature,
            max_output_tokens,
            messages,
            tools,
            max_tokens_key,
            include_response_format,
        );
        match send_chat_completion(endpoint, api_key, timeout_secs, &body) {
            Ok(value) => return Ok(value),
            Err(err) => {
                let can_try_next = supports_ask_provider_fallback(&err);
                errors.push(format!("{} failed: {}", label, err));
                if !can_try_next {
                    return Err(errors.join(" | "));
                }
            }
        }
    }
    Err(format!(
        "ask agent compatibility fallbacks failed; {}",
        errors.join(" | ")
    ))
}

fn send_ask_chat_completion_with_retry(
    endpoint: &str,
    api_key: &str,
    llm: &kb_llm::LlmRuntimeConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, String> {
    let max_attempts = llm.retry_max_attempts.max(1);
    let mut errors = Vec::new();
    for attempt in 1..=max_attempts {
        match send_ask_chat_completion_once(
            endpoint,
            api_key,
            llm.timeout_secs,
            &llm.model,
            llm.temperature,
            llm.max_output_tokens,
            messages,
            tools,
        ) {
            Ok(value) => return Ok(value),
            Err(err) => {
                errors.push(format!("attempt {}: {}", attempt, err));
                if attempt < max_attempts {
                    std::thread::sleep(std::time::Duration::from_millis(
                        llm.retry_backoff_ms,
                    ));
                }
            }
        }
    }
    Err(format!(
        "ask agent request failed after {} attempts; {}",
        max_attempts,
        errors.join(" | ")
    ))
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
    toolbox: &kb_llm::SemanticAgentToolbox,
    call: &AskToolCall,
) -> Result<AskToolResult, String> {
    match call.name.as_str() {
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
        other => Err(format!("unknown ask tool: {}", other)),
    }
}

fn render_tool_result<T: serde::Serialize>(summary: String, value: &T) -> AskToolResult {
    AskToolResult {
        summary,
        json: serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string()),
    }
}

fn run_minimal_ask_agent(
    cwd: &Path,
    question: &str,
    retrieved_context: &[kb_core::RetrievedContext],
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, CliError> {
    if std::env::var("KB_ASK_OFFLINE_STUB").ok().as_deref() == Some("1") {
        return run_stub_ask_agent(cwd, question, retrieved_context, provider);
    }
    match run_live_ask_agent(cwd, question, retrieved_context, provider) {
        Ok(value) => Ok(value),
        Err(err) => {
            emit_ask_trace(&format!(
                "live agent failed, falling back to offline stub: {}",
                truncate_trace_text(&err, 240)
            ));
            let mut fallback = run_stub_ask_agent(cwd, question, retrieved_context, provider)?;
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
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, CliError> {
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd.to_path_buf()));
    let mut agent_trace = Vec::new();
    let mut tool_trace = Vec::new();
    let mut answer_lines = Vec::new();
    let mut citations = Vec::new();

    push_agent_trace(
        &mut agent_trace,
        Some(1),
        "thought",
        format!(
            "Seeded {} retrieved records; inspect summaries, graph neighbors, and raw chunks for the best answer.",
            retrieved_context.len()
        ),
        None,
    );
    emit_ask_trace(&format!(
        "round 1 thought: seeded {} retrieved records",
        retrieved_context.len()
    ));

    tool_trace.push(json!({
        "tool": "search_context",
        "summary": format!("search_context seeded {} retrieved items", retrieved_context.len()),
    }));

    for item in retrieved_context.iter().take(3) {
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

    let answer_text = if answer_lines.is_empty() {
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
    provider: Option<&str>,
) -> Result<AskAgentAnswerOutput, String> {
    let llm = resolve_llm_config(cwd);
    let api_key = llm
        .key
        .as_deref()
        .ok_or_else(|| "missing KB_LLM_KEY for ask answer mode".to_string())?;
    let endpoint = build_chat_completion_endpoint(&llm.base_url);
    let toolbox = kb_llm::SemanticAgentToolbox::new(Some(cwd.to_path_buf()));

    let mut messages = vec![
        json!({
            "role": "system",
            "content": "You are a read-only knowledge-base question answering agent. Use the available tools to inspect compiled summaries, graph relations, wiki pages, and raw chunks before answering. Prefer graph-backed evidence when it helps connect related files. When you are ready to answer, stop calling tools and return a JSON object with keys reasoning_summary, answer_text, and citations. Keep reasoning_summary brief and high-level."
        }),
        json!({
            "role": "user",
            "content": format!(
                "Question: {}\nSeed contexts:\n{}\nYou may inspect graph neighbors and edge evidence to expand from the retrieved records before answering.",
                question,
                serde_json::to_string_pretty(&contexts_json(retrieved_context)).unwrap_or_else(|_| "[]".to_string())
            )
        }),
    ];

    let mut agent_trace = vec![];
    let mut tool_trace = vec![];
    let tools = ask_agent_tools_schema();
    emit_ask_trace(&format!(
        "start provider={} seed_records={}",
        provider.unwrap_or("unknown"),
        retrieved_context.len()
    ));

    for round in 1..=llm.agent_loop_max_rounds.max(1) {
        emit_ask_trace(&format!("round {} request", round));
        let response =
            send_ask_chat_completion_with_retry(&endpoint, api_key, &llm, &messages, &tools)?;
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
                let tool_result = execute_ask_tool(&toolbox, &call)?;
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
                agent_trace,
                tool_trace,
                answer_text: payload.answer_text,
                citations: payload.citations,
            });
        }
    }

    Err(format!(
        "ask agent did not produce final answer for question '{}' via {}",
        question,
        provider.unwrap_or("unknown")
    ))
}

pub(crate) fn run_update(
    dry_run: bool,
    prune: bool,
    _yes: bool,
    full: bool,
    strict: bool,
    llm: bool,
) -> Result<Value, CliError> {
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    // Load LLM runtime config at update-time so layered semantic stages can reuse it.
    let llm_runtime = resolve_llm_config(&cwd);
    let kb_doc = parse_kb_document_file(&cwd.join("kb.md"))?;
    let ignored = load_ignore_rules(&cwd).into_iter().collect::<BTreeSet<_>>();
    let desired = collect_desired_entries(&cwd, &kb_doc, &ignored, false, None)?;
    let mut registry = load_registry(&cwd)?;

    let index_store = FsIndexStore::new(cwd.clone());
    let effective_baseline = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let diff = compute_registry_diff(&cwd, &registry.regs, &desired, effective_baseline.as_ref());
    apply_registry_diff(&mut registry, &diff);

    let backend_sync = build_backend_sync_backend();
    if !dry_run {
        let kb_md = cwd.join("kb.md");
        let kb_doc_for_prune = parse_kb_document_file(&kb_md)?;
        prune_missing_refs_from_kb_md(&cwd, &kb_doc_for_prune, &diff)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        let kb_doc_for_dedupe = parse_kb_document_file(&kb_md)?;
        dedupe_raw_refs_in_kb_md(&cwd, &kb_doc_for_dedupe)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        let kb_doc_for_append = parse_kb_document_file(&kb_md)?;
        append_missing_raw_refs_to_kb_md(&cwd, &kb_doc_for_append, &desired)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        backend_sync.sync(&build_backend_sync_items(&diff)).map_err(|_| {
            map_core_error(CoreErrorKind::PersistIndexFailed, json!({"target":"backend"}))
        })?;
        save_registry(&cwd, &registry)?;
    }

    let refs = regs_to_records(&cwd, &registry.regs, &ignored, false, None);
    let effective_prune = prune || !diff.to_remove.is_empty();
    let outcome = IndexEngine.update(
        effective_baseline.as_ref(),
        &refs,
        &IndexConfig::new(ignored, effective_prune, full),
    );

    if !dry_run {
        index_store.write_index(&outcome.snapshot).map_err(|_| {
            map_core_error(
                CoreErrorKind::PersistIndexFailed,
                json!({"targets":[".kb/index.json",".kb/index.errors.json",".kb/index.meta.json"]}),
            )
        })?;
    }

    let mut progressive_issues = collect_progressive_layer_issues();

    let knowledge_store = FsKnowledgeStore::new(cwd.clone());
    let search_store = FsSearchStore::new(cwd.clone());
    let graph_store = FsGraphStore::new(cwd.clone());
    let wiki_store = FsWikiStore::new(cwd.clone());
    let content_reader = FsFileContentReader::new(cwd.clone());
    let semantic_llm_requested = llm || llm_runtime.semantic_enabled;
    let graph_llm_requested = llm || llm_runtime.graph_enabled;
    let wiki_llm_requested = llm || llm_runtime.wiki_enabled;

    let semantic_mode = if semantic_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM semantic extraction requested by --llm but KB_LLM_KEY is missing"
                        .to_string()
                } else {
                    "LLM semantic extraction enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };
    let graph_mode = if graph_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM graph enrichment requested by --llm but KB_LLM_KEY is missing".to_string()
                } else {
                    "LLM graph enrichment enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };
    let wiki_mode = if wiki_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM wiki enrichment requested by --llm but KB_LLM_KEY is missing".to_string()
                } else {
                    "LLM wiki enrichment enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };

    let llm_semantic_config = LlmSemanticConfig {
        key: llm_runtime.key.clone(),
        model: llm_runtime.model.clone(),
        base_url: llm_runtime.base_url.clone(),
        workspace_root: Some(cwd.clone()),
        semantic_flow: llm_runtime.semantic_flow.clone(),
        timeout_secs: llm_runtime.timeout_secs,
        temperature: llm_runtime.temperature,
        max_output_tokens: llm_runtime.max_output_tokens,
        analysis_max_output_tokens: llm_runtime.analysis_max_output_tokens,
        max_input_chars: llm_runtime.max_input_chars,
        max_chunks: llm_runtime.max_chunks,
        retry_max_attempts: llm_runtime.retry_max_attempts,
        retry_backoff_ms: llm_runtime.retry_backoff_ms,
        agent_loop_max_rounds: llm_runtime.agent_loop_max_rounds,
        agent_reflection_enabled: llm_runtime.agent_reflection_enabled,
        agent_min_chunk_coverage: llm_runtime.agent_min_chunk_coverage,
    };
    let extractor: Box<dyn SemanticExtractor> = if semantic_mode == "llm" {
        Box::new(LlmSemanticExtractor::new(llm_semantic_config.clone()))
    } else {
        Box::new(RuleBasedSemanticExtractor::new())
    };

    let baseline_knowledge = match knowledge_store.read_knowledge() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-READ-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
            None
        }
    };

    let generated_at = generated_at_unix_string();
    let l2_outcome = compile_l2(
        baseline_knowledge.as_ref(),
        &outcome.snapshot,
        &content_reader,
        extractor.as_ref(),
        &generated_at,
    );
    for issue in &l2_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L2_SEMANTIC,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }

    if !dry_run {
        if let Err(err) = knowledge_store.write_knowledge(&l2_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
        }
    }

    let baseline_search = match search_store.read_search() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L3_PROJECTION,
                code: "W-L3-READ-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
            None
        }
    };
    let l3_outcome = compile_l3(
        baseline_search.as_ref(),
        &l2_outcome.snapshot,
        &generated_at,
    );

    if !dry_run {
        if let Err(err) = search_store.write_search(&l3_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L3_PROJECTION,
                code: "W-L3-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
        }
    }

    let baseline_graph = match graph_store.read_graph() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-READ-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
            None
        }
    };
    let l4_rule_outcome = compile_l4(
        baseline_graph.as_ref(),
        &l2_outcome.snapshot,
        &generated_at,
    );
    let mut l4_metrics_json = Value::Null;
    let l4_outcome = if graph_mode == "llm" {
        let mut graph_trace = |line: &str| emit_runtime_trace("kb-llm", line);
        match enrich_graph_with_llm(
            baseline_graph.as_ref(),
            &l4_rule_outcome,
            &l2_outcome.snapshot,
            &llm_semantic_config,
            Some(&mut graph_trace),
        ) {
            Ok(enriched) => {
                l4_metrics_json = json!({
                    "map_reduce": {
                        "batch_size": enriched.metrics.map_reduce.batch_size,
                        "batches_total": enriched.metrics.map_reduce.batches_total,
                        "calls_total": enriched.metrics.map_reduce.calls_total,
                        "calls_succeeded": enriched.metrics.map_reduce.calls_succeeded,
                        "calls_failed": enriched.metrics.map_reduce.calls_failed,
                        "rounds_used": enriched.metrics.map_reduce.rounds_used,
                        "rounds_max": enriched.metrics.map_reduce.rounds_max,
                        "reflection_enabled": enriched.metrics.map_reduce.reflection_enabled,
                    },
                    "coverage": {
                        "covered": enriched.metrics.coverage.covered,
                        "total": enriched.metrics.coverage.total,
                        "ratio": enriched.metrics.coverage.ratio,
                        "target_ratio": enriched.metrics.coverage.target_ratio,
                    }
                });
                enriched.outcome
            }
            Err(err) => {
                progressive_issues.push(LayerIssue {
                    layer: LAYER_L4_GRAPH,
                    code: "W-L4-LLM-ENRICH-FAILED".to_string(),
                    message: err,
                    retryable: true,
                });
                l4_rule_outcome
            }
        }
    } else {
        l4_rule_outcome
    };
    for issue in &l4_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L4_GRAPH,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }
    if !dry_run {
        if let Err(err) = graph_store.write_graph(&l4_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err,
                retryable: true,
            });
        }
    }

    let baseline_wiki_manifest = match wiki_store.read_wiki_manifest() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-READ-MANIFEST-FAILED".to_string(),
                message: err,
                retryable: true,
            });
            None
        }
    };
    let l5_rule_outcome = compile_l5(
        baseline_wiki_manifest.as_ref(),
        &l2_outcome.snapshot,
        &l4_outcome.snapshot,
        &generated_at,
    );
    let mut l5_metrics_json = Value::Null;
    let l5_outcome = if wiki_mode == "llm" {
        let mut wiki_trace = |line: &str| emit_runtime_trace("kb-llm", line);
        match enrich_wiki_with_llm(
            baseline_wiki_manifest.as_ref(),
            &l5_rule_outcome,
            &l2_outcome.snapshot,
            &l4_outcome.snapshot,
            &generated_at,
            &llm_semantic_config,
            Some(&mut wiki_trace),
        ) {
            Ok(enriched) => {
                l5_metrics_json = json!({
                    "map_reduce": {
                        "batch_size": enriched.metrics.map_reduce.batch_size,
                        "batches_total": enriched.metrics.map_reduce.batches_total,
                        "calls_total": enriched.metrics.map_reduce.calls_total,
                        "calls_succeeded": enriched.metrics.map_reduce.calls_succeeded,
                        "calls_failed": enriched.metrics.map_reduce.calls_failed,
                        "rounds_used": enriched.metrics.map_reduce.rounds_used,
                        "rounds_max": enriched.metrics.map_reduce.rounds_max,
                        "reflection_enabled": enriched.metrics.map_reduce.reflection_enabled,
                    },
                    "coverage": {
                        "covered": enriched.metrics.coverage.covered,
                        "total": enriched.metrics.coverage.total,
                        "ratio": enriched.metrics.coverage.ratio,
                        "target_ratio": enriched.metrics.coverage.target_ratio,
                    }
                });
                enriched.outcome
            }
            Err(err) => {
                progressive_issues.push(LayerIssue {
                    layer: LAYER_L5_WIKI_VIEW,
                    code: "W-L5-LLM-ENRICH-FAILED".to_string(),
                    message: err,
                    retryable: true,
                });
                l5_rule_outcome
            }
        }
    } else {
        l5_rule_outcome
    };
    for issue in &l5_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L5_WIKI_VIEW,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }
    if !dry_run {
        if let Err(err) = wiki_store.write_wiki_view(&l5_outcome.snapshot, &l5_outcome.removed_paths)
        {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-WRITE-VIEW-FAILED".to_string(),
                message: err,
                retryable: true,
            });
        }
    }

    let mut l2_status = "ok";
    let mut l3_status = "ok";
    let mut l4_status = "ok";
    let mut l5_status = "ok";
    if progressive_issues
        .iter()
        .any(|issue| issue.layer == LAYER_L2_SEMANTIC)
    {
        l2_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues.iter().any(|issue| issue.layer == LAYER_L3_PROJECTION) {
        l3_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues.iter().any(|issue| issue.layer == LAYER_L4_GRAPH) {
        l4_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues
        .iter()
        .any(|issue| issue.layer == LAYER_L5_WIKI_VIEW)
    {
        l5_status = if strict { "error" } else { "warning" };
    }

    let layer_status = json!({
        LAYER_L0_DECLARATION: "ok",
        LAYER_L1_REGISTRY: "ok",
        LAYER_L1_INDEX: "ok",
        LAYER_L2_SEMANTIC: l2_status,
        LAYER_L3_PROJECTION: l3_status,
        LAYER_L4_GRAPH: l4_status,
        LAYER_L5_WIKI_VIEW: l5_status,
    });
    let l2_compilation = json!({
        "processed": l2_outcome.processed,
        "reused": l2_outcome.reused,
        "missing": l2_outcome.missing,
        "failed": l2_outcome.failed,
    });
    let warning_items: Vec<Value> = progressive_issues.iter().map(issue_json).collect();
    let warning_texts: Vec<String> = progressive_issues.iter().map(issue_text).collect();

    if strict && !progressive_issues.is_empty() {
        return Err(map_core_error(
            CoreErrorKind::StrictLayerFailure,
            json!({
                "strict": true,
                "layer_status": layer_status,
                "l2_compilation": l2_compilation,
                "warnings": [],
                "errors": warning_items,
                "failed_layers": progressive_issues.iter().map(|issue| issue.layer).collect::<Vec<_>>(),
            }),
        ));
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let text = format_update_text(&UpdateTextSummary {
        added: diff.add_entries.len(),
        updated: diff.to_update.len() + outcome.updated,
        removed: diff.removed_paths.len(),
        marked_invalid: 0,
        duration_ms,
        dry_run,
        prune: effective_prune,
        full,
        strict,
        layer_status: vec![
            (LAYER_L0_DECLARATION, "ok"),
            (LAYER_L1_REGISTRY, "ok"),
            (LAYER_L1_INDEX, "ok"),
            (LAYER_L2_SEMANTIC, l2_status),
            (LAYER_L3_PROJECTION, l3_status),
            (LAYER_L4_GRAPH, l4_status),
            (LAYER_L5_WIKI_VIEW, l5_status),
        ],
        l2_processed: l2_outcome.processed,
        l2_reused: l2_outcome.reused,
        l2_missing: l2_outcome.missing,
        l2_failed: l2_outcome.failed,
        warnings: warning_texts,
        errors: vec![],
    });

    let data = json!({
        "added": diff.add_entries.len(),
        "updated": diff.to_update.len() + outcome.updated,
        "removed": diff.removed_paths.len(),
        "marked_invalid": 0,
        "duration_ms": duration_ms,
        "full": full,
        "raw_category_created": false,
        "strict": strict,
        "layer_status": layer_status,
        "l2_compilation": l2_compilation,
        "semantic_extractor": {
            "mode": semantic_mode,
            "forced_by_cli": llm,
            "provider": if semantic_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "semantic_flow": llm_runtime.semantic_flow,
            "agent_tools": {
                "enabled": semantic_mode == "llm",
                "tools": [
                    "read_raw_file",
                    "read_knowledge_graph",
                    "read_file_summary"
                ]
            },
            "temperature": llm_runtime.temperature,
            "max_output_tokens": llm_runtime.max_output_tokens,
            "analysis_max_output_tokens": llm_runtime.analysis_max_output_tokens,
            "max_input_chars": llm_runtime.max_input_chars,
            "max_chunks": llm_runtime.max_chunks,
            "timeout_secs": llm_runtime.timeout_secs,
            "retry_max_attempts": llm_runtime.retry_max_attempts,
            "retry_backoff_ms": llm_runtime.retry_backoff_ms,
            "agent_loop_max_rounds": llm_runtime.agent_loop_max_rounds,
            "agent_reflection_enabled": llm_runtime.agent_reflection_enabled,
            "agent_min_chunk_coverage": llm_runtime.agent_min_chunk_coverage
        },
        "graph_builder": {
            "mode": graph_mode,
            "provider": if graph_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "agent_tools": {
                "enabled": graph_mode == "llm",
                "tools": [
                    "read_file_summary",
                    "read_knowledge_graph",
                    "read_graph_neighbors"
                ]
            },
            "llm_metrics": l4_metrics_json
        },
        "wiki_builder": {
            "mode": wiki_mode,
            "provider": if wiki_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "agent_tools": {
                "enabled": wiki_mode == "llm",
                "tools": [
                    "read_file_summary",
                    "read_knowledge_graph",
                    "read_wiki_page"
                ]
            },
            "llm_metrics": l5_metrics_json
        },
        "warnings": warning_items,
        "errors": [],
    });
    Ok(with_text(data, text))
}

pub(crate) fn run_ask(question: String, top: Option<u32>, answer: bool) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let index_store = FsIndexStore::new(cwd.clone());
    let snapshot = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let Some(snapshot) = snapshot else {
        return Err(map_core_error(
            CoreErrorKind::NoRelevantContext,
            json!({"query": question}),
        ));
    };

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

    if answer {
        if out.meta.engine_status == "disabled" {
            return Err(map_core_error(
                CoreErrorKind::NoRelevantContext,
                json!({
                    "query": question,
                    "mode": "answer",
                    "reason": "llm_not_configured"
                }),
            ));
        }
        let answer_out = run_minimal_ask_agent(
            &cwd,
            &question,
            &out.retrieved_context,
            out.meta.provider.as_deref(),
        )?;
        let text = format_answer_text(
            out.meta.engine_status.as_str(),
            out.meta.provider.as_deref(),
            out.meta.trace_id.as_deref(),
            &answer_out.agent_backend,
            answer_out.agent_warning.as_deref(),
            &retrieval_trace,
            &answer_out.agent_trace,
            &answer_out.tool_trace,
            &answer_out.answer_text,
            &answer_out.citations,
        );
        let data = json!({
            "mode": "answer",
            "engine_status": out.meta.engine_status,
            "provider": out.meta.provider,
            "trace_id": out.meta.trace_id,
            "agent_backend": answer_out.agent_backend,
            "agent_warning": answer_out.agent_warning,
            "retrieved_context": contexts_json(&out.retrieved_context),
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
    use super::decode_ask_agent_turn;
    use serde_json::json;

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
}

