use clap::CommandFactory;
use clap_complete::generate;
use kb_core::domain::{CheckResult, IndexConfig, IndexEngine, IndexRecord, IndexSnapshot, Permission};
use kb_core::usecases::{
    apply_mv, apply_path, compile_l2, compile_l3, compile_l4, compile_l5, normalize_top,
    plan_add, plan_init, plan_list_output,
    validate_category_filter, AddModePlan, AddPlanError, AddPlanInput, CategoryFilterError,
    InitPlanError, ListRow, MovePlanError, PathPlanError,
};
use kb_core::{
    AnswerEngine, GraphStore, IndexStore, KnowledgeStore, Retriever, SearchStore,
    SemanticExtractor, StateStore, WikiStore,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::cli::{AddMode, Cli};
use crate::core_adapters::{ConfiguredAnswerEngine, DisabledAnswerEngine, FsStateStore};
use crate::env_registry::{list_existing_kb_roots, register_kb_root};
use crate::errors::{map_core_error, CliError, CoreErrorKind};
use crate::kb_md::parse_kb_document;
use crate::llm_config::resolve_llm_config;
use crate::paths::{
    is_ignored, load_ignore_rules, normalize_path, KBIGNORE_REL, RAW_DIR_REL, RAW_PREFIX,
};
use crate::registry::{load_registry, save_registry, RegistryEntry, RegistryState};
use crate::state::{load_state, save_state, RefEntry, State};
use kb_core::RuleBasedSemanticExtractor;
use kb_index_memory::LayeredRetriever;
use kb_llm::{LlmSemanticConfig, LlmSemanticExtractor};
use kb_storage_fs::{
    FsFileContentReader, FsGraphStore, FsIndexStore, FsKnowledgeStore, FsSearchStore, FsWikiStore,
};

const DEFAULT_KBIGNORE_CONTENT: &str = "# KB internal files (auto-generated)\n./kb.md\n./.kb/.kbignore\n./.kb/refs.json\n./.kb/regs.json\n./.kb/index.json\n./.kb/index.errors.json\n./.kb/index.meta.json\n./.kb/schema.version\n./.kb/*.lock\n";
const DEFAULT_LLM_ENV_TEMPLATE: &str = "# KB LLM runtime template\n# Enable LLM-based L2 semantic compilation during `kb update`\nKB_LLM_ENABLE_SEMANTIC=0\n\n# OpenAI-compatible endpoint settings\nKB_LLM_KEY=\nKB_LLM_MODEL=gpt-4o-mini\nKB_LLM_BASE_URL=https://api.openai.com/v1\n\n# LLM compile flow:\n#   two_step   -> analysis then memory generation (recommended)\n#   single_step -> direct generation in one request\nKB_LLM_SEMANTIC_FLOW=two_step\n\n# Tuning for multi-file semantic compilation\nKB_LLM_TIMEOUT_SECS=60\nKB_LLM_TEMPERATURE=0.1\nKB_LLM_MAX_OUTPUT_TOKENS=900\nKB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS=700\nKB_LLM_MAX_INPUT_CHARS=12000\nKB_LLM_MAX_CHUNKS=12\n\n# Retry strategy for transient provider/network failures\nKB_LLM_RETRY_MAX_ATTEMPTS=3\nKB_LLM_RETRY_BACKOFF_MS=600\n";

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
    strict: bool,
    layer_status: Vec<(&'static str, &'static str)>,
    l2_processed: usize,
    l2_reused: usize,
    l2_missing: usize,
    l2_failed: usize,
    warnings: Vec<String>,
    errors: Vec<String>,
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
    let _ = writeln!(out, "strict: {}", summary.strict);
    out.push_str("layer_status:\n");
    for (layer, status) in &summary.layer_status {
        let _ = writeln!(out, "  {}: {}", layer, status);
    }
    out.push_str("l2_compilation:\n");
    let _ = writeln!(out, "  processed: {}", summary.l2_processed);
    let _ = writeln!(out, "  reused: {}", summary.l2_reused);
    let _ = writeln!(out, "  missing: {}", summary.l2_missing);
    let _ = writeln!(out, "  failed: {}", summary.l2_failed);
    if !summary.warnings.is_empty() {
        out.push_str("warnings:\n");
        for line in &summary.warnings {
            let _ = writeln!(out, "  - {}", line);
        }
    }
    if !summary.errors.is_empty() {
        out.push_str("errors:\n");
        for line in &summary.errors {
            let _ = writeln!(out, "  - {}", line);
        }
    }
    out
}

