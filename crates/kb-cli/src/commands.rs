//! CLI 命令编排层（application layer）。
//! 主要职责：
//! - 参数到 usecase 输入的映射
//! - 持久化适配调用
//! - 结果组织为统一 JSON 数据结构
//!
//! 不在此层实现复杂业务规则，规则尽量下沉到 `kb-core`。

use clap::CommandFactory;
use clap_complete::generate;
use kb_core::domain::{CheckResult, IndexConfig, IndexEngine, IndexRecord, IndexSnapshot};
use kb_core::usecases::{
    apply_chmod, apply_mv, apply_path, apply_rm, normalize_top, plan_add, plan_init,
    plan_list_output, validate_category_filter, AddModePlan, AddPlanError, AddPlanInput,
    CategoryFilterError, ChmodPlanError, InitPlanError, ListRow, MovePlanError, PathPlanError,
    RemovePlanError,
};
use kb_core::{AnswerEngine, IndexStore, Retriever, StateStore};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cli::{AddMode, Cli, Commands};
use crate::core_adapters::{DefaultRetriever, DisabledAnswerEngine, FsIndexStore, FsStateStore};
use crate::env_registry::{list_existing_kb_roots, register_kb_root};
use crate::errors::{map_core_error, CliError, CoreErrorKind};
use crate::kb_md::parse_kb_document;
use crate::paths::{
    is_ignored, load_ignore_rules, normalize_path, KBIGNORE_REL, RAW_DIR_REL, RAW_PREFIX,
};
use crate::registry::{load_registry, save_registry, RegistryEntry, RegistryState};
use crate::state::{load_state, save_state, RefEntry, State};

const DEFAULT_KBIGNORE_CONTENT: &str = "# KB internal files (auto-generated)\n./kb.md\n./.kb/.kbignore\n./.kb/refs.json\n./.kb/regs.json\n./.kb/index.json\n./.kb/index.errors.json\n./.kb/index.meta.json\n./.kb/schema.version\n./.kb/*.lock\n";

fn write_config_error() -> CliError {
    map_core_error(CoreErrorKind::WriteConfigFailed, json!({}))
}

fn alias_not_found_error(alias: String) -> CliError {
    map_core_error(CoreErrorKind::AliasNotFound, json!({ "alias": alias }))
}

fn alias_invalid_error(alias: String) -> CliError {
    map_core_error(CoreErrorKind::AliasInvalid, json!({ "alias": alias }))
}

fn alias_conflict_error(alias: String) -> CliError {
    map_core_error(CoreErrorKind::AliasConflict, json!({ "alias": alias }))
}

fn load_state_snapshot(store: &FsStateStore) -> Result<kb_core::StateSnapshot, CliError> {
    store.load_state().map_err(|_| write_config_error())
}

fn save_state_snapshot(
    store: &FsStateStore,
    state: &kb_core::StateSnapshot,
) -> Result<(), CliError> {
    store.save_state(state).map_err(|_| write_config_error())
}

fn with_text(mut data: Value, text: String) -> Value {
    if let Some(obj) = data.as_object_mut() {
        let mut text = text;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        obj.insert("_raw_text".to_string(), Value::String(text));
    }
    data
}

fn format_env_text(config_path: &str, recorded_total: usize, items: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Knowledge base registry");
    let _ = writeln!(out, "config_path: {}", config_path);
    let _ = writeln!(out, "recorded_total: {}", recorded_total);
    let _ = writeln!(out, "count: {}", items.len());
    out.push_str("items:\n");
    if items.is_empty() {
        out.push_str("  - (empty)\n");
    } else {
        for path in items {
            let _ = writeln!(out, "  - {}", path);
        }
    }
    out
}

fn format_list_text(items: &[ListRow], categories: Option<&Vec<String>>) -> String {
    if items.is_empty() {
        return "No references found.\n".to_string();
    }

    let alias_w = items
        .iter()
        .map(|r| r.alias.len())
        .max()
        .unwrap_or(5)
        .max("ALIAS".len());
    let perms_w = items
        .iter()
        .map(|r| r.perms.len())
        .max()
        .unwrap_or(5)
        .max("PERMS".len());
    let show_size = items.iter().any(|r| r.size.is_some());
    let show_mtime = items.iter().any(|r| r.mtime.is_some());

    let mut out = String::new();
    let _ = writeln!(out, "Tracked references: {}", items.len());

    if show_size || show_mtime {
        let _ = writeln!(
            out,
            "{:<alias_w$}  {:<perms_w$}  {:>10}  {:>12}  PATH",
            "ALIAS",
            "PERMS",
            "SIZE",
            "MTIME"
        );
    } else {
        let _ = writeln!(
            out,
            "{:<alias_w$}  {:<perms_w$}  PATH",
            "ALIAS",
            "PERMS"
        );
    }

    for row in items {
        if show_size || show_mtime {
            let size_text = row
                .size
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let mtime_text = row.mtime.clone().unwrap_or_else(|| "-".to_string());
            let _ = writeln!(
                out,
                "{:<alias_w$}  {:<perms_w$}  {:>10}  {:>12}  {}",
                row.alias,
                row.perms,
                size_text,
                mtime_text,
                row.path
            );
        } else {
            let _ = writeln!(
                out,
                "{:<alias_w$}  {:<perms_w$}  {}",
                row.alias,
                row.perms,
                row.path
            );
        }
    }

    if let Some(cats) = categories {
        if !cats.is_empty() {
            out.push_str("categories:\n");
            for c in cats {
                let _ = writeln!(out, "  - {}", c);
            }
        }
    }

    out
}

fn format_check_text(
    has_changes: bool,
    to_add: &[CheckResult],
    to_update: &[CheckResult],
    to_remove: &[CheckResult],
    limit: usize,
    duration_ms: u64,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Index diff");
    let _ = writeln!(out, "has_changes: {}", has_changes);
    let _ = writeln!(out, "to_add: {}", to_add.len());
    let _ = writeln!(out, "to_update: {}", to_update.len());
    let _ = writeln!(out, "to_remove: {}", to_remove.len());
    let _ = writeln!(out, "duration_ms: {}", duration_ms);
    out.push_str("suggested_command: kb update\n");

    if !to_add.is_empty() {
        out.push_str("\nTo add:\n");
        for item in to_add.iter().take(limit) {
            let source = if item.path.starts_with(RAW_PREFIX) {
                "raw"
            } else {
                "kbmd"
            };
            let _ = writeln!(out, "  - {} | {} [{}]", item.alias, item.path, source);
        }
        if to_add.len() > limit {
            let _ = writeln!(out, "  ... {} more", to_add.len() - limit);
        }
    }

    if !to_update.is_empty() {
        out.push_str("\nTo update:\n");
        for item in to_update.iter().take(limit) {
            let _ = writeln!(out, "  - {} | {} ({})", item.alias, item.path, item.reason);
        }
        if to_update.len() > limit {
            let _ = writeln!(out, "  ... {} more", to_update.len() - limit);
        }
    }

    if !to_remove.is_empty() {
        out.push_str("\nTo remove:\n");
        for item in to_remove.iter().take(limit) {
            let _ = writeln!(out, "  - {} | {} ({})", item.alias, item.path, item.reason);
        }
        if to_remove.len() > limit {
            let _ = writeln!(out, "  ... {} more", to_remove.len() - limit);
        }
    }

    out
}

struct UpdateTextSummary {
    added: usize,
    updated: usize,
    removed: usize,
    marked_invalid: usize,
    duration_ms: u64,
    dry_run: bool,
    prune: bool,
    full: bool,
}

fn format_update_text(summary: &UpdateTextSummary) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Index update complete");
    let _ = writeln!(out, "added: {}", summary.added);
    let _ = writeln!(out, "updated: {}", summary.updated);
    let _ = writeln!(out, "removed: {}", summary.removed);
    let _ = writeln!(out, "marked_invalid: {}", summary.marked_invalid);
    let _ = writeln!(out, "duration_ms: {}", summary.duration_ms);
    let _ = writeln!(out, "dry_run: {}", summary.dry_run);
    let _ = writeln!(out, "prune: {}", summary.prune);
    let _ = writeln!(out, "full: {}", summary.full);
    out
}

// 命令分发层保持“薄”：只做路由，不做复杂业务判断。
pub fn dispatch(cli: Cli) -> Result<Value, CliError> {
    match cli.command {
        Commands::Init { name, force } => run_init(name, force),
        Commands::Add {
            path,
            alias,
            mode,
            category,
        } => run_add(path, alias, mode, category),
        Commands::Mv { alias, new_alias } => run_mv(alias, new_alias),
        Commands::Rm { alias, force } => run_rm(alias, force),
        Commands::Path { alias, new_path } => run_path(alias, new_path),
        Commands::Chmod { alias, perms } => run_chmod(alias, perms),
        Commands::List { long, tree, filter } => run_list(long, tree, filter),
        Commands::Check {
            raw_only,
            category,
            top,
        } => run_check(raw_only, category, top),
        Commands::Update {
            dry_run,
            prune,
            yes,
            full,
        } => run_update(dry_run, prune, yes, full),
        Commands::Ask { question, top } => run_ask(question, top),
        Commands::Env { list } => run_env(list),
        Commands::Completion { shell } => run_completion(shell),
    }
}

fn run_completion(shell: clap_complete::Shell) -> Result<Value, CliError> {
    // completion 脚本既放在 script 字段，也放 _raw_text 以复用 text 渲染路径。
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

fn run_init(name: Option<String>, force: bool) -> Result<Value, CliError> {
    let cwd = std::env::current_dir().map_err(|_| write_config_error())?;
    let kb_md = cwd.join("kb.md");
    let already_initialized = kb_md.exists() && cwd.join(".kb").exists();

    // 初始化覆盖策略统一由 core usecase 决定，CLI 只做映射和落盘。
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
    // 全局登记失败不应阻断本地初始化，保证 init 核心语义稳定。
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

fn run_env(list: bool) -> Result<Value, CliError> {
    // 当前 env 命令聚焦全局知识库登记查询；未传 --list 时默认回退为 list 行为。
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

fn run_add(
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
    // 统一交给 core 侧 usecase 做输入校验，避免 CLI 层和 usecase 双重校验的冗余。
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
        // copy 模式：将源文件归档到 ./.kb/raw，后续由 update 纳入索引。
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

fn map_rm_plan_error(err: RemovePlanError) -> CliError {
    match err {
        RemovePlanError::NotFoundAlias { alias } => alias_not_found_error(alias),
    }
}

fn map_path_plan_error(err: PathPlanError) -> CliError {
    match err {
        PathPlanError::NotFoundAlias { alias } => alias_not_found_error(alias),
    }
}

fn map_chmod_plan_error(err: ChmodPlanError) -> CliError {
    match err {
        ChmodPlanError::InvalidPermission { provided } => map_core_error(
            CoreErrorKind::InvalidPermission,
            json!({"provided": provided}),
        ),
        ChmodPlanError::NotFoundAlias { alias } => alias_not_found_error(alias),
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

fn run_mv(alias: String, new_alias: String) -> Result<Value, CliError> {
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

fn run_rm(alias: String, _force: bool) -> Result<Value, CliError> {
    // _force 当前保留为兼容参数位，现阶段不改变删除语义（仅移除引用，不删物理文件）。
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;
    let plan = apply_rm(&state, &alias).map_err(map_rm_plan_error)?;
    save_state_snapshot(&store, &plan.state)?;
    let data = json!({"removed_alias": plan.removed_alias, "path": plan.removed_path});
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

fn run_path(alias: String, new_path: Option<String>) -> Result<Value, CliError> {
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

fn run_chmod(alias: String, perms: String) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;
    let plan = apply_chmod(&state, &alias, &perms).map_err(map_chmod_plan_error)?;
    save_state_snapshot(&store, &plan.state)?;
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

fn run_list(long: bool, tree: bool, filter: Option<String>) -> Result<Value, CliError> {
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

    // 预构建权限映射，避免每个 list item 在线性扫描 regs。
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

fn run_check(
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

fn run_update(dry_run: bool, prune: bool, _yes: bool, full: bool) -> Result<Value, CliError> {
    let start = Instant::now();
    let cwd = ensure_initialized()?;
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
        append_missing_raw_refs_to_kb_md(&cwd, &kb_doc, &desired)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        backend_sync.sync(&build_backend_sync_items(&diff)).map_err(|_| {
            map_core_error(CoreErrorKind::PersistIndexFailed, json!({"target":"backend"}))
        })?;
        save_registry(&cwd, &registry)?;
    }

    let refs = regs_to_records(&cwd, &registry.regs, &ignored, false, None);
    let effective_prune = prune || !diff.removed_paths.is_empty();
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
    });

    let data = json!({
        "added": diff.add_entries.len(),
        "updated": diff.to_update.len() + outcome.updated,
        "removed": diff.removed_paths.len(),
        "marked_invalid": 0,
        "duration_ms": duration_ms,
        "full": full,
        "raw_category_created": false
    });
    Ok(with_text(data, text))
}

fn run_ask(question: String, top: Option<u32>) -> Result<Value, CliError> {
    // ask 依赖已有索引快照；若不存在快照，直接返回“无可用上下文”。
    let cwd = ensure_initialized()?;
    let index_store = FsIndexStore::new(cwd);
    let snapshot = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let Some(snapshot) = snapshot else {
        return Err(map_core_error(
            CoreErrorKind::NoRelevantContext,
            json!({"query": question}),
        ));
    };

    let (retriever, answer_engine) = build_ask_stack();
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

fn build_ask_stack() -> (Box<dyn Retriever>, Box<dyn AnswerEngine>) {
    // 当前默认本地实现：
    // - Retriever: DefaultRetriever（本地索引检索）
    // - AnswerEngine: DisabledAnswerEngine（仅返回元信息）
    //
    // 接入后端知识库/Agent 时，可在此替换为：
    // - kb_core::BackendRetrieverBridge<YourKnowledgeBackend>
    // - kb_core::BackendAnswerEngineBridge<YourAgentBackend>
    (Box::new(DefaultRetriever), Box::new(DisabledAnswerEngine))
}

// 除 init 外其余命令都需要已初始化工作区。
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

#[derive(Debug, Clone)]
struct DesiredEntry {
    id: String,
    alias: Option<String>,
    path: String,
    perms: String,
    category: String,
    source: String,
    exists: bool,
}

impl DesiredEntry {
    fn to_registry_entry(&self) -> RegistryEntry {
        RegistryEntry {
            id: self.id.clone(),
            alias: self.alias.clone(),
            path: self.path.clone(),
            perms: self.perms.clone(),
            source: self.source.clone(),
            category: self.category.clone(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct RegistryDiff {
    to_add: Vec<CheckResult>,
    to_update: Vec<CheckResult>,
    to_remove: Vec<CheckResult>,
    add_entries: Vec<RegistryEntry>,
    update_entries: Vec<RegistryEntry>,
    removed_paths: Vec<String>,
}

#[derive(Debug, Clone)]
enum BackendSyncAction {
    Upsert,
    Delete,
}

#[derive(Debug, Clone)]
struct BackendSyncItem {
    action: BackendSyncAction,
    entry: RegistryEntry,
}

trait BackendSyncBackend {
    fn sync(&self, items: &[BackendSyncItem]) -> Result<(), String>;
}

#[derive(Debug, Default)]
struct NoopBackendSyncBackend;

impl BackendSyncBackend for NoopBackendSyncBackend {
    fn sync(&self, items: &[BackendSyncItem]) -> Result<(), String> {
        for item in items {
            match item.action {
                BackendSyncAction::Upsert => {
                    let _ = &item.entry.path;
                }
                BackendSyncAction::Delete => {
                    let _ = &item.entry.id;
                }
            }
        }
        Ok(())
    }
}

fn build_backend_sync_backend() -> Box<dyn BackendSyncBackend> {
    Box::new(NoopBackendSyncBackend)
}

fn build_backend_sync_items(diff: &RegistryDiff) -> Vec<BackendSyncItem> {
    let mut out = Vec::new();
    for item in &diff.add_entries {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Upsert,
            entry: item.clone(),
        });
    }
    for item in &diff.update_entries {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Upsert,
            entry: item.clone(),
        });
    }
    for path in &diff.removed_paths {
        out.push(BackendSyncItem {
            action: BackendSyncAction::Delete,
            entry: RegistryEntry {
                id: path.clone(),
                alias: None,
                path: path.clone(),
                perms: "r".to_string(),
                source: if path.starts_with(RAW_PREFIX) {
                    "raw".to_string()
                } else {
                    "kbmd".to_string()
                },
                category: "raw".to_string(),
            },
        });
    }
    out
}

fn regs_to_records(
    cwd: &Path,
    regs: &[RegistryEntry],
    ignored: &BTreeSet<String>,
    raw_only: bool,
    category: Option<&str>,
) -> Vec<IndexRecord> {
    regs.iter()
        .filter(|r| category.map(|c| r.category == c).unwrap_or(true))
        .filter(|r| !raw_only || r.path.starts_with(RAW_PREFIX))
        .filter(|r| !is_protected_index_target(cwd, &cwd.join(r.path.trim_start_matches("./"))))
        .filter(|r| !ignored.contains(&r.path))
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() || !abs.is_file() {
                return None;
            }
            Some(IndexRecord {
                alias: r.id.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: compute_signature(&abs),
                present: true,
                excerpt: compute_excerpt(&abs),
            })
        })
        .collect()
}

fn regs_to_list_records(cwd: &Path, regs: &[RegistryEntry]) -> Vec<IndexRecord> {
    regs.iter()
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() {
                return None;
            }
            Some(IndexRecord {
                alias: r.id.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: String::new(),
                present: true,
                excerpt: None,
            })
        })
        .collect()
}

fn parse_kb_document_file(kb_md: &Path) -> Result<crate::kb_md::KbDocument, CliError> {
    let raw = fs::read_to_string(kb_md)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    parse_kb_document(&raw).map_err(|err| {
        map_core_error(
            CoreErrorKind::InvalidKbFormat,
            json!({ "path": normalize_path(&kb_md.to_string_lossy()), "message": err }),
        )
    })
}

fn collect_desired_entries(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    ignored: &BTreeSet<String>,
    raw_only: bool,
    category: Option<&str>,
) -> Result<Vec<DesiredEntry>, CliError> {
    let mut by_path: BTreeMap<String, DesiredEntry> = BTreeMap::new();

    for item in collect_raw_entries(cwd) {
        by_path.insert(item.path.clone(), item);
    }

    for item in collect_kb_entries(cwd, kb_doc)? {
        by_path.insert(item.path.clone(), item);
    }

    let mut out = by_path.into_values().collect::<Vec<_>>();
    out.retain(|entry| {
        category.map(|c| entry.category == c).unwrap_or(true)
            && (!raw_only || entry.path.starts_with(RAW_PREFIX))
            && !ignored.contains(&entry.path)
            && !is_protected_index_target(cwd, &cwd.join(entry.path.trim_start_matches("./")))
    });
    Ok(out)
}

fn collect_raw_entries(cwd: &Path) -> Vec<DesiredEntry> {
    let raw_dir = cwd.join(RAW_DIR_REL.trim_start_matches("./"));
    collect_files_under(cwd, &raw_dir, true)
        .into_iter()
        .map(|path| DesiredEntry {
            id: path.clone(),
            alias: None,
            path: path.clone(),
            perms: "r".to_string(),
            category: "raw".to_string(),
            source: "raw".to_string(),
            exists: true,
        })
        .collect()
}

fn collect_kb_entries(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
) -> Result<Vec<DesiredEntry>, CliError> {
    let mut out = Vec::new();

    for r in &kb_doc.references {
        let resolved = resolve_reference_paths(cwd, &r.path);
        if resolved.is_empty() {
            continue;
        }

        let singular_plain = resolved.len() == 1 && !is_pattern_reference(&r.path);
        for (path, exists) in resolved {
            let alias = if singular_plain { r.alias.clone() } else { None };
            let id = alias.clone().unwrap_or_else(|| path.clone());
            out.push(DesiredEntry {
                id,
                alias,
                path: path.clone(),
                perms: r.perms.clone(),
                category: r.category.clone().unwrap_or_else(|| "raw".to_string()),
                source: if path.starts_with(RAW_PREFIX) {
                    "raw".to_string()
                } else {
                    "kbmd".to_string()
                },
                exists,
            });
        }
    }

    Ok(out)
}

fn is_pattern_reference(path_spec: &str) -> bool {
    path_spec.ends_with('/')
        || path_spec.ends_with("/*")
        || path_spec.ends_with("/**")
        || path_spec.contains('*')
        || path_spec.contains('?')
        || path_spec.contains('[')
        || path_spec.contains(']')
        || path_spec.contains('{')
        || path_spec.contains('}')
}

fn resolve_reference_paths(cwd: &Path, path_spec: &str) -> Vec<(String, bool)> {
    let normalized = normalize_path(path_spec);

    if normalized.ends_with("/**") {
        let base = normalized.trim_end_matches("/**");
        let dir = cwd.join(base.trim_start_matches("./"));
        return collect_files_under(cwd, &dir, true)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if normalized.ends_with("/*") {
        let base = normalized.trim_end_matches("/*");
        let dir = cwd.join(base.trim_start_matches("./"));
        return collect_files_under(cwd, &dir, false)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if normalized.ends_with('/') {
        let dir = cwd.join(normalized.trim_start_matches("./").trim_end_matches('/'));
        return collect_files_under(cwd, &dir, true)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if is_pattern_reference(&normalized) {
        return resolve_glob_matches(cwd, &normalized)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    let abs = cwd.join(normalized.trim_start_matches("./"));
    vec![(normalized, abs.exists())]
}

fn resolve_glob_matches(cwd: &Path, pattern: &str) -> Vec<String> {
    let candidates = walk_all_files(cwd);
    let expanded = expand_braces(pattern);
    let mut matched = candidates
        .into_iter()
        .filter(|path| {
            expanded
                .iter()
                .any(|rule| glob_like_extended_match(rule, path))
        })
        .collect::<Vec<_>>();
    matched.sort();
    matched.dedup();
    matched
}

fn walk_all_files(root: &Path) -> Vec<String> {
    collect_files_under(root, root, true)
}

fn collect_files_under(cwd: &Path, dir: &Path, recursive: bool) -> Vec<String> {
    if !dir.exists() || !dir.is_dir() {
        return vec![];
    }

    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    stack.push(path);
                }
                continue;
            }
            if path.is_file() {
                if let Ok(rel) = path.strip_prefix(cwd) {
                    out.push(normalize_path(&rel.to_string_lossy()));
                }
            }
        }
    }
    out
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some((open, close)) = find_first_brace(pattern) else {
        return vec![pattern.to_string()];
    };

    let prefix = &pattern[..open];
    let body = &pattern[open + 1..close];
    let suffix = &pattern[close + 1..];

    let variants = split_brace_variants(body);
    let mut out = Vec::new();
    for variant in variants {
        for expanded_suffix in expand_braces(suffix) {
            out.push(format!("{}{}{}", prefix, variant, expanded_suffix));
        }
    }
    out
}

fn find_first_brace(pattern: &str) -> Option<(usize, usize)> {
    let chars = pattern.char_indices().collect::<Vec<_>>();
    for (idx, ch) in &chars {
        if *ch != '{' {
            continue;
        }
        let mut depth = 0usize;
        for (jdx, inner) in chars.iter().skip_while(|(pos, _)| pos < idx) {
            match *inner {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((*idx, *jdx));
                    }
                }
                _ => {}
            }
        }
        return None;
    }
    None
}

fn split_brace_variants(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let chars = body.char_indices().collect::<Vec<_>>();
    for (idx, ch) in &chars {
        match *ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(body[start..*idx].to_string());
                start = *idx + 1;
            }
            _ => {}
        }
    }
    out.push(body[start..].to_string());
    out
}

#[derive(Debug, Clone)]
enum GlobClassPart {
    Single(char),
    Range(char, char),
}

#[derive(Debug, Clone)]
enum GlobToken {
    Literal(char),
    Star,
    DoubleStar,
    Question,
    CharClass(Vec<GlobClassPart>),
}

fn glob_like_extended_match(pattern: &str, candidate: &str) -> bool {
    let tokens = parse_glob_tokens(pattern);
    let text = candidate.chars().collect::<Vec<_>>();
    let mut memo: HashMap<(usize, usize), bool> = HashMap::new();
    match_glob_tokens(&tokens, 0, &text, 0, &mut memo)
}

fn parse_glob_tokens(pattern: &str) -> Vec<GlobToken> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push(GlobToken::DoubleStar);
                    i += 2;
                } else {
                    out.push(GlobToken::Star);
                    i += 1;
                }
            }
            '?' => {
                out.push(GlobToken::Question);
                i += 1;
            }
            '[' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    out.push(GlobToken::Literal('['));
                    i += 1;
                } else {
                    let content = chars[i + 1..j].to_vec();
                    out.push(GlobToken::CharClass(parse_char_class(&content)));
                    i = j + 1;
                }
            }
            ch => {
                out.push(GlobToken::Literal(ch));
                i += 1;
            }
        }
    }

    out
}

fn parse_char_class(content: &[char]) -> Vec<GlobClassPart> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < content.len() {
        if i + 2 < content.len() && content[i + 1] == '-' {
            out.push(GlobClassPart::Range(content[i], content[i + 2]));
            i += 3;
        } else {
            out.push(GlobClassPart::Single(content[i]));
            i += 1;
        }
    }
    out
}

fn match_glob_tokens(
    tokens: &[GlobToken],
    token_idx: usize,
    text: &[char],
    text_idx: usize,
    memo: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(cached) = memo.get(&(token_idx, text_idx)) {
        return *cached;
    }

    let answer = if token_idx == tokens.len() {
        text_idx == text.len()
    } else {
        match &tokens[token_idx] {
            GlobToken::Literal(ch) => {
                text_idx < text.len()
                    && text[text_idx] == *ch
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::Question => {
                text_idx < text.len()
                    && text[text_idx] != '/'
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::CharClass(parts) => {
                text_idx < text.len()
                    && text[text_idx] != '/'
                    && class_matches(parts, text[text_idx])
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::Star => {
                let mut idx = text_idx;
                loop {
                    if match_glob_tokens(tokens, token_idx + 1, text, idx, memo) {
                        break true;
                    }
                    if idx >= text.len() || text[idx] == '/' {
                        break false;
                    }
                    idx += 1;
                }
            }
            GlobToken::DoubleStar => {
                let mut idx = text_idx;
                loop {
                    if match_glob_tokens(tokens, token_idx + 1, text, idx, memo) {
                        break true;
                    }
                    if idx >= text.len() {
                        break false;
                    }
                    idx += 1;
                }
            }
        }
    };

    memo.insert((token_idx, text_idx), answer);
    answer
}

fn class_matches(parts: &[GlobClassPart], ch: char) -> bool {
    parts.iter().any(|part| match part {
        GlobClassPart::Single(x) => *x == ch,
        GlobClassPart::Range(start, end) => *start <= ch && ch <= *end,
    })
}

fn compute_registry_diff(
    cwd: &Path,
    regs: &[RegistryEntry],
    desired: &[DesiredEntry],
    baseline: Option<&IndexSnapshot>,
) -> RegistryDiff {
    let mut diff = RegistryDiff::default();

    let regs_by_path: BTreeMap<&str, &RegistryEntry> =
        regs.iter().map(|item| (item.path.as_str(), item)).collect();
    let desired_by_path: BTreeMap<&str, &DesiredEntry> =
        desired.iter().map(|item| (item.path.as_str(), item)).collect();

    let baseline_signature: BTreeMap<&str, &str> = baseline
        .map(|snapshot| {
            snapshot
                .records
                .iter()
                .filter(|record| record.present)
                .map(|record| (record.path.as_str(), record.signature.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut removed_paths = HashSet::new();
    for item in desired {
        if !item.exists {
            diff.to_remove.push(CheckResult {
                alias: item.id.clone(),
                path: item.path.clone(),
                reason: "not_found".to_string(),
            });
            if regs_by_path.contains_key(item.path.as_str()) {
                removed_paths.insert(item.path.clone());
            }
            continue;
        }

        match regs_by_path.get(item.path.as_str()) {
            None => {
                diff.to_add.push(CheckResult {
                    alias: item.id.clone(),
                    path: item.path.clone(),
                    reason: "new_path".to_string(),
                });
                diff.add_entries.push(item.to_registry_entry());
            }
            Some(existing) => {
                let metadata_changed = existing.id != item.id
                    || existing.alias != item.alias
                    || existing.perms != item.perms
                    || existing.source != item.source
                    || existing.category != item.category;
                if metadata_changed {
                    diff.to_update.push(CheckResult {
                        alias: item.id.clone(),
                        path: item.path.clone(),
                        reason: "metadata_changed".to_string(),
                    });
                    diff.update_entries.push(item.to_registry_entry());
                    continue;
                }

                let current_sig = compute_signature(&cwd.join(item.path.trim_start_matches("./")));
                if baseline_signature
                    .get(item.path.as_str())
                    .map(|sig| **sig != current_sig)
                    .unwrap_or(false)
                {
                    diff.to_update.push(CheckResult {
                        alias: item.id.clone(),
                        path: item.path.clone(),
                        reason: "content_changed".to_string(),
                    });
                }
            }
        }
    }

    for reg in regs {
        if !desired_by_path.contains_key(reg.path.as_str()) {
            diff.to_remove.push(CheckResult {
                alias: reg.id.clone(),
                path: reg.path.clone(),
                reason: "deleted".to_string(),
            });
            removed_paths.insert(reg.path.clone());
        }
    }

    diff.to_add
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));
    diff.to_update
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));
    diff.to_remove
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));

    let mut removed = removed_paths.into_iter().collect::<Vec<_>>();
    removed.sort();
    diff.removed_paths = removed;

    diff
}

fn apply_registry_diff(registry: &mut RegistryState, diff: &RegistryDiff) {
    let removed = diff.removed_paths.iter().cloned().collect::<HashSet<_>>();
    registry.regs.retain(|item| !removed.contains(&item.path));

    for item in &diff.update_entries {
        if let Some(existing) = registry.regs.iter_mut().find(|x| x.path == item.path) {
            *existing = item.clone();
        }
    }

    for item in &diff.add_entries {
        if !registry.regs.iter().any(|x| x.path == item.path) {
            registry.regs.push(item.clone());
        }
    }

    registry
        .regs
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
}

fn compute_signature(path: &Path) -> String {
    if !path.exists() {
        return "missing".to_string();
    }
    if path.is_file() {
        // 使用流式读取计算签名，避免大文件整块读入内存。
        if let Ok(mut file) = fs::File::open(path) {
            let mut h: u64 = 5381;
            let mut buf = [0u8; 8192];
            loop {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for byte in &buf[..n] {
                            h = ((h << 5).wrapping_add(h)).wrapping_add(*byte as u64);
                        }
                    }
                    Err(_) => return "unknown".to_string(),
                }
            }
            return format!("f:{h}");
        }
    }
    match fs::metadata(path) {
        Ok(meta) => format!("d:{}", meta.len()),
        Err(_) => "unknown".to_string(),
    }
}

fn compute_excerpt(path: &Path) -> Option<String> {
    if !path.exists() || !path.is_file() {
        return None;
    }

    // 只读取到首个非空行即可，降低大文件场景的 I/O 与内存占用。
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).ok()?;
        if read == 0 {
            return None;
        }
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.chars().take(480).collect());
        }
    }
}

fn parse_categories(kb_md: &Path) -> Result<Vec<String>, CliError> {
    parse_kb_document_file(kb_md).map(|doc| doc.categories)
}

fn append_to_kb_md(
    kb_md: &Path,
    category: &str,
    alias: &str,
    path: &str,
) -> Result<(), std::io::Error> {
    let mut content = fs::read_to_string(kb_md).unwrap_or_default();
    let has_category = parse_kb_document(&content)
        .ok()
        .map(|doc| doc.categories.iter().any(|x| x == category))
        .unwrap_or(false);

    if !has_category {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&format!("# {category}\n"));
    }
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&format!("[{alias}]({path})\n"));
    fs::write(kb_md, content)
}

fn append_missing_raw_refs_to_kb_md(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    desired: &[DesiredEntry],
) -> Result<usize, std::io::Error> {
    let kb_md = cwd.join("kb.md");
    let mut already_referenced = kb_doc
        .references
        .iter()
        .map(|r| normalize_path(&r.path))
        .collect::<HashSet<_>>();

    let mut appended = 0usize;
    for item in desired {
        if item.source != "raw" || !item.exists {
            continue;
        }
        if already_referenced.contains(&item.path) {
            continue;
        }
        append_to_kb_md(&kb_md, "raw", "", &item.path)?;
        already_referenced.insert(item.path.clone());
        appended += 1;
    }
    Ok(appended)
}

fn project_name(cwd: &Path) -> String {
    cwd.file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("demo")
        .to_string()
}

fn to_change_json(limit: usize, items: &[CheckResult]) -> Vec<Value> {
    items
        .iter()
        .take(limit)
        .map(|x| json!({"alias": x.alias, "path": x.path, "reason": x.reason}))
        .collect()
}

fn to_add_json(limit: usize, items: &[CheckResult]) -> Vec<Value> {
    items
        .iter()
        .take(limit)
        .map(|x| {
            json!({
                "alias": x.alias,
                "path": x.path,
                "source": if x.path.starts_with(RAW_PREFIX) { "raw" } else { "kbmd" }
            })
        })
        .collect()
}

fn metadata_summary(abs: &Path) -> (u64, String) {
    // mtime 输出 Unix 秒字符串，保持与当前 JSON fixture 契约一致。
    match fs::metadata(abs) {
        Ok(meta) => {
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|x| x.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|x| x.as_secs().to_string())
                .unwrap_or_else(|| "0".to_string());
            (size, mtime)
        }
        Err(_) => (0, "0".to_string()),
    }
}
