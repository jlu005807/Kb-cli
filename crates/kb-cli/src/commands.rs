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
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cli::{AddMode, Cli, Commands};
use crate::core_adapters::{DefaultRetriever, DisabledAnswerEngine, FsIndexStore, FsStateStore};
use crate::errors::{map_core_error, CliError, CoreErrorKind};
use crate::paths::{is_ignored, load_ignore_rules, normalize_path, RAW_DIR_REL, RAW_PREFIX};
use crate::state::{load_state, save_state, RefEntry, State};

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
        Commands::Completion { shell } => run_completion(shell),
    }
}

fn run_completion(shell: clap_complete::Shell) -> Result<Value, CliError> {
    // completion 脚本既放在 script 字段，也放 _raw_text 以复用 text 渲染路径。
    let mut cmd = Cli::command();
    let mut buf: Vec<u8> = Vec::new();
    generate(shell, &mut cmd, "kb-cli", &mut buf);
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
    fs::create_dir_all(cwd.join("kb")).map_err(|_| write_config_error())?;
    fs::create_dir_all(cwd.join(RAW_DIR_REL.trim_start_matches("./")))
        .map_err(|_| write_config_error())?;
    fs::write(cwd.join("kb/.kbignore"), "").map_err(|_| write_config_error())?;
    fs::write(&kb_md, "# raw\n").map_err(|_| write_config_error())?;
    save_state(&cwd, &State::default())?;

    let mut data = json!({
        "project": plan.project,
        "path": "./kb.md"
    });
    if force {
        data["overwritten"] = Value::Bool(plan.overwritten);
    }
    Ok(data)
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
    let source_exists = src_path.exists();

    let ignore_rules = load_ignore_rules(&cwd);
    if is_ignored(&ignore_rules, &normalized_path) {
        return Ok(json!({
            "alias": alias.clone().unwrap_or_else(|| normalized_path.clone()),
            "path": normalized_path,
            "ignored": true,
            "message": "skipped by .kbignore"
        }));
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

    Ok(json!({"alias": alias_value, "path": final_path}))
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
    Ok(json!({"old_alias": alias, "new_alias": new_alias}))
}

fn run_rm(alias: String, _force: bool) -> Result<Value, CliError> {
    // _force 当前保留为兼容参数位，现阶段不改变删除语义（仅移除引用，不删物理文件）。
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;
    let plan = apply_rm(&state, &alias).map_err(map_rm_plan_error)?;
    save_state_snapshot(&store, &plan.state)?;
    Ok(json!({"removed_alias": plan.removed_alias, "path": plan.removed_path}))
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
        Ok(
            json!({"alias": alias, "old_path": plan.old_path.unwrap_or_default(), "new_path": normalized}),
        )
    } else {
        let plan = apply_path(&state, &alias, None).map_err(map_path_plan_error)?;
        Ok(json!({"alias": alias, "path": plan.path}))
    }
}

fn run_chmod(alias: String, perms: String) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let store = FsStateStore::new(cwd);
    let state = load_state_snapshot(&store)?;
    let plan = apply_chmod(&state, &alias, &perms).map_err(map_chmod_plan_error)?;
    save_state_snapshot(&store, &plan.state)?;
    Ok(json!({"alias": alias, "permissions": perms}))
}

fn run_list(long: bool, tree: bool, filter: Option<String>) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let state = load_state(&cwd)?;
    let records = refs_to_list_records(&cwd, &state.refs);
    let snapshot = IndexSnapshot {
        records,
        errors: vec![],
        meta: kb_core::domain::IndexMeta {
            full: false,
            baseline_size: 0,
            tracked_size: state.refs.len(),
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

    // 预构建权限映射，避免每个 list item 在线性扫描 refs。
    let perms_by_alias: HashMap<&str, &str> = state
        .refs
        .iter()
        .map(|r| (r.alias.as_str(), r.perms.as_str()))
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
    Ok(data)
}

fn run_check(
    raw_only: bool,
    category: Option<String>,
    top: Option<u32>,
) -> Result<Value, CliError> {
    // check 只做差异预览：读取基线 + 当前状态，绝不写索引文件。
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    let state = load_state(&cwd)?;
    let categories = parse_categories(&cwd.join("kb.md")).unwrap_or_default();
    validate_category_filter(category.as_deref(), &categories)
        .map_err(map_category_filter_error)?;

    let ignored = load_ignore_rules(&cwd).into_iter().collect::<BTreeSet<_>>();
    let index_store = FsIndexStore::new(cwd.clone());
    let baseline = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let records = refs_to_records(&cwd, &state.refs, &ignored, raw_only, category.as_deref());
    let outcome = IndexEngine.check(baseline.as_ref(), &records, &ignored);
    let limit = normalize_top(top, 50);

    Ok(json!({
        "has_changes": !outcome.to_add.is_empty() || !outcome.to_update.is_empty() || !outcome.to_remove.is_empty(),
        "to_add": to_add_json(limit, &outcome.to_add),
        "to_update": to_change_json(limit, &outcome.to_update),
        "to_remove": to_change_json(limit, &outcome.to_remove),
        "suggested_command": "kb update",
        "duration_ms": start.elapsed().as_millis() as u64,
    }))
}

fn run_update(dry_run: bool, prune: bool, _yes: bool, full: bool) -> Result<Value, CliError> {
    // _yes 当前作为非交互模式保留位，行为由后续交互流程扩展。
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    let state = load_state(&cwd)?;
    let ignored = load_ignore_rules(&cwd).into_iter().collect::<BTreeSet<_>>();
    let refs = refs_to_records(&cwd, &state.refs, &ignored, false, None);

    let index_store = FsIndexStore::new(cwd.clone());
    let effective_baseline = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let outcome = IndexEngine.update(
        effective_baseline.as_ref(),
        &refs,
        &IndexConfig::new(ignored, prune, full),
    );

    // dry-run 只返回计划，不落盘。
    if !dry_run {
        index_store.write_index(&outcome.snapshot).map_err(|_| {
            map_core_error(
                CoreErrorKind::PersistIndexFailed,
                json!({"targets":[".kb/index.json",".kb/index.errors.json",".kb/index.meta.json"]}),
            )
        })?;
    }

    Ok(json!({
        "added": outcome.added,
        "updated": outcome.updated,
        "removed": outcome.removed,
        "marked_invalid": outcome.marked_invalid,
        "duration_ms": start.elapsed().as_millis() as u64,
        "full": full,
        "raw_category_created": false
    }))
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

    Ok(json!({
        "engine_status": out.meta.engine_status,
        "provider": out.meta.provider,
        "trace_id": out.meta.trace_id,
        "retrieved_context": retrieved_context,
    }))
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

fn refs_to_records(
    cwd: &Path,
    refs: &[RefEntry],
    ignored: &BTreeSet<String>,
    raw_only: bool,
    category: Option<&str>,
) -> Vec<IndexRecord> {
    // 将 refs 状态投影为可索引记录，同时统一应用 raw/category/ignore 过滤。
    refs.iter()
        .filter(|r| category.map(|c| r.category == c).unwrap_or(true))
        .filter(|r| !raw_only || r.path.starts_with(RAW_PREFIX))
        .filter(|r| !ignored.contains(&r.path))
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() {
                return None;
            }
            Some(IndexRecord {
                alias: r.alias.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: compute_signature(&abs),
                present: true,
                excerpt: compute_excerpt(&abs),
            })
        })
        .collect()
}

fn refs_to_list_records(cwd: &Path, refs: &[RefEntry]) -> Vec<IndexRecord> {
    // list 渲染仅依赖 alias/path/category/present，不需要签名与摘要，避免无效 I/O。
    refs.iter()
        .filter_map(|r| {
            let abs = cwd.join(r.path.trim_start_matches("./"));
            if !abs.exists() {
                return None;
            }
            Some(IndexRecord {
                alias: r.alias.clone(),
                path: r.path.clone(),
                category: r.category.clone(),
                signature: String::new(),
                present: true,
                excerpt: None,
            })
        })
        .collect()
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
    // MVP 分类来源：kb.md 中以 "# " 开头的标题行。
    let raw = fs::read_to_string(kb_md)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    let categories = raw
        .lines()
        .filter_map(|line| line.strip_prefix("# ").map(|x| x.trim().to_string()))
        .collect::<Vec<_>>();
    Ok(categories)
}

fn append_to_kb_md(
    kb_md: &Path,
    category: &str,
    alias: &str,
    path: &str,
) -> Result<(), std::io::Error> {
    // 追加写策略：不存在分类先补分类，再追加条目，保持文本可读性与简单性。
    let mut content = fs::read_to_string(kb_md).unwrap_or_default();
    if !content.lines().any(|l| l.trim() == format!("# {category}")) {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&format!("# {category}\n"));
    }
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&format!("- {alias} | {path}\n"));
    fs::write(kb_md, content)
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
