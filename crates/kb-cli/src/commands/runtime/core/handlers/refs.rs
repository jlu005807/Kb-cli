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

    if path_declaration_unrepresentable(&normalized_path) {
        return Err(map_core_error(
            CoreErrorKind::PathUnrepresentable,
            json!({"path": normalized_path}),
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
    // 冲突检测以声明真值 kb.md 为准（手工维护的引用同样参与），refs.json 工作集作补充。
    let kb_doc = parse_kb_document_file(&cwd.join("kb.md")).ok();
    let categories = kb_doc
        .as_ref()
        .map(|doc| doc.categories.clone())
        .unwrap_or_else(|| vec!["raw".to_string()]);
    let mut existing_aliases: Vec<String> = state.refs.iter().map(|r| r.alias.clone()).collect();
    let mut existing_paths: Vec<String> =
        state.refs.iter().map(|r| normalize_path(&r.path)).collect();
    if let Some(doc) = &kb_doc {
        for item in &doc.references {
            if let Some(alias_value) = &item.alias {
                existing_aliases.push(alias_value.clone());
            }
            existing_paths.push(normalize_path(&item.path));
        }
    }
    let declared_paths: HashSet<String> = existing_paths.iter().cloned().collect();
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
        existing_aliases,
        existing_paths,
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
        // copy 模式的落点也参与冲突检测：同名文件已被声明时拒绝，
        // 避免静默覆盖 ./.kb/raw/ 内容并留下重复声明。
        if declared_paths.contains(&normalize_path(&target_rel)) {
            return Err(map_core_error(
                CoreErrorKind::PathAlreadyRegistered,
                json!({"path": target_rel}),
            ));
        }
        let target_abs = cwd.join(target_rel.trim_start_matches("./"));
        if src_path.is_file() {
            let mut input = fs::File::open(&src_path).map_err(|_| write_config_error())?;
            let permissions = input.metadata().map_err(|_| write_config_error())?.permissions();
            let mut output = fs::OpenOptions::new().write(true).create_new(true)
                .open(&target_abs).map_err(|err| {
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        map_core_error(CoreErrorKind::PathAlreadyRegistered, json!({"path": target_rel}))
                    } else {
                        write_config_error()
                    }
                })?;
            let copied = std::io::copy(&mut input, &mut output)
                .and_then(|_| output.set_permissions(permissions));
            drop(output);
            if copied.is_err() {
                let _ = fs::remove_file(&target_abs);
                return Err(write_config_error());
            }
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
    if Alias::parse(&new_alias).is_err() {
        return Err(alias_invalid_error(new_alias));
    }

    let kb_md = cwd.join("kb.md");
    let kb_doc = parse_kb_document_file(&kb_md)?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;

    // 冲突命名空间 = kb.md 显式 alias ∪ 无别名引用的 path 派生 id ∪ 工作集 alias。
    let conflict_in_kb_md = kb_doc.references.iter().any(|item| match &item.alias {
        Some(existing) => existing == &new_alias,
        None => normalize_path(&item.path) == new_alias,
    });
    let conflict_in_state = state.refs.iter().any(|r| r.alias == new_alias);
    if conflict_in_kb_md || conflict_in_state {
        return Err(alias_conflict_error(new_alias));
    }

    // 声明真值必须命中：kb.md 无该目标时报 E5002，不做工作集独走的幻影改名。
    let Some((old_alias, target_path)) = apply_mv_to_kb_md(&kb_md, &kb_doc, &alias, &new_alias)
        .map_err(|_| write_config_error())?
    else {
        return Err(alias_not_found_error(alias));
    };

    // refs.json 工作集 best-effort 同步：按解析出的旧 alias 或 path 匹配，
    // kb.md 已生效，同步失败不再冒硬错误（与 chmod 策略一致）。
    let mut next = state.clone();
    let mut touched = false;
    for item in next.refs.iter_mut() {
        if item.alias == old_alias || normalize_path(&item.path) == target_path {
            item.alias = new_alias.clone();
            touched = true;
        }
    }
    if touched {
        let _ = save_state_snapshot(&store, &next);
    }

    let data = json!({"old_alias": old_alias, "new_alias": new_alias});
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
    let kb_md = cwd.join("kb.md");
    let kb_doc = parse_kb_document_file(&kb_md)?;
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
        if path_declaration_unrepresentable(&normalized) {
            return Err(map_core_error(
                CoreErrorKind::PathUnrepresentable,
                json!({"path": normalized}),
            ));
        }

        // 声明真值必须命中：kb.md 无该目标时报 E5002。
        let Some(target_ref) = resolve_rm_target(&kb_doc, &alias) else {
            return Err(alias_not_found_error(alias));
        };
        // 目录/glob 声明会展开成多条记录，单行改写等于静默注销全部展开条目。
        if is_pattern_reference(&target_ref.path) {
            return Err(map_core_error(
                CoreErrorKind::PatternNotRewritable,
                json!({"alias": target_ref.alias, "pattern": target_ref.path}),
            ));
        }
        // 与 add 一致的路径占用检查：新路径已被其他声明持有时拒绝，
        // 避免 update 时 by_path 收敛静默丢掉其中一个别名。
        let occupied_in_kb_md = kb_doc
            .references
            .iter()
            .any(|item| item.line != target_ref.line && normalize_path(&item.path) == normalized);
        let occupied_in_state = state
            .refs
            .iter()
            .any(|r| r.alias != target_ref.alias && normalize_path(&r.path) == normalized);
        if occupied_in_kb_md || occupied_in_state {
            return Err(map_core_error(
                CoreErrorKind::PathAlreadyRegistered,
                json!({"path": normalized}),
            ));
        }

        let Some((resolved_alias, old_path)) =
            apply_path_to_kb_md(&kb_md, &kb_doc, &alias, &normalized)
                .map_err(|_| write_config_error())?
        else {
            return Err(alias_not_found_error(alias));
        };

        // refs.json 工作集 best-effort 同步：按解析出的 alias 或旧 path 匹配。
        let mut next = state.clone();
        let mut touched = false;
        for item in next.refs.iter_mut() {
            if item.alias == resolved_alias || normalize_path(&item.path) == old_path {
                item.path = normalized.clone();
                touched = true;
            }
        }
        if touched {
            let _ = save_state_snapshot(&store, &next);
        }

        let data = json!({"alias": alias, "old_path": old_path, "new_path": normalized});
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
        // 查询模式：kb.md 声明优先，refs.json 工作集兜底。
        let path_value = if let Some(reference) = resolve_rm_target(&kb_doc, &alias) {
            reference.identity().1
        } else {
            apply_path(&state, &alias, None)
                .map_err(map_path_plan_error)?
                .path
        };
        let data = json!({"alias": alias, "path": path_value});
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

    // refs.json 工作集的 perms 同步为 best-effort：kb.md 已是权限真值。
    // 按解析出的引用身份（alias 或 path）匹配，覆盖以路径/id 寻址的情形；
    // 分类标题 chmod 解析不到单条引用，跳过同步。
    if let Some(target_ref) = resolve_rm_target(&kb_doc, &alias) {
        if let Ok(mut state) = load_state(&cwd) {
            let mut touched = false;
            for item in state.refs.iter_mut() {
                if item.alias == target_ref.alias || normalize_path(&item.path) == target_ref.path {
                    item.perms = perms.clone();
                    touched = true;
                }
            }
            if touched {
                let _ = save_state(&cwd, &state);
            }
        }
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
