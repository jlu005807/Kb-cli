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
