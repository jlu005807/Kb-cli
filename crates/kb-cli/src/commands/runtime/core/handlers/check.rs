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

    let storage = StorageHandles::open(&cwd)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let baseline = storage
        .index
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
