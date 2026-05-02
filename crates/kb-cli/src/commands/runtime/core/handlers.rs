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
    fs::write(&kb_md, "# raw\n").map_err(|_| write_config_error())?;
    save_state(&cwd, &State::default())?;
    save_registry(&cwd, &RegistryState::default())?;
    let _ = register_kb_root(&cwd);

    let mut data = json!({
        "project": plan.project,
        "path": "./kb.md"
    });
    if force {
        data["overwritten"] = Value::Bool(plan.overwritten);
    }

    let mut text = String::new();
    text.push_str("Knowledge base initialized\n");
    let _ = writeln!(text, "project: {}", data["project"].as_str().unwrap_or_default());
    text.push_str("path: ./kb.md\n");
    text.push_str("ignore_file: ./.kb/.kbignore\n");
    text.push_str("raw_dir: ./.kb/raw\n");
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

pub(crate) fn run_update(
    dry_run: bool,
    prune: bool,
    _yes: bool,
    full: bool,
    strict: bool,
) -> Result<Value, CliError> {
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    // Load LLM runtime config at update-time so layered semantic stages can reuse it.
    let _llm_runtime = resolve_llm_config(&cwd);
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
    let content_reader = FsFileContentReader::new(cwd.clone());
    let extractor = RuleBasedSemanticExtractor::new();

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
        &extractor,
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

    let mut l2_status = "ok";
    let mut l3_status = "ok";
    let mut l4_status = "skipped";
    let mut l5_status = "skipped";
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
        "warnings": warning_items,
        "errors": [],
    });
    Ok(with_text(data, text))
}

pub(crate) fn run_ask(question: String, top: Option<u32>) -> Result<Value, CliError> {
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

    let mut ask_text = String::new();
    ask_text.push_str("Ask result\n");
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

    let retrieved_context: Vec<Value> = out
        .retrieved_context
        .into_iter()
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
        .collect();

    let data = json!({
        "engine_status": out.meta.engine_status,
        "provider": out.meta.provider,
        "trace_id": out.meta.trace_id,
        "retrieved_context": retrieved_context,
    });
    Ok(with_text(data, ask_text))
}

