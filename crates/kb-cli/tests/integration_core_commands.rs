use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_json(args: &[&str], current_dir: &Path) -> (bool, Value) {
    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(current_dir)
        .arg("--output")
        .arg("json")
        .args(args)
        .assert();
    let success = assert.get_output().status.success();
    let body: Value = serde_json::from_slice(&assert.get_output().stdout).expect("json stdout");
    (success, body)
}

fn run_json_with_global_config(
    args: &[&str],
    current_dir: &Path,
    global_config: &Path,
) -> (bool, Value) {
    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(current_dir)
        .env("KB_GLOBAL_CONFIG", global_config)
        .arg("--output")
        .arg("json")
        .args(args)
        .assert();
    let success = assert.get_output().status.success();
    let body: Value = serde_json::from_slice(&assert.get_output().stdout).expect("json stdout");
    (success, body)
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "kb_cli_it_{}_{}_{}",
        tag,
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn mk_file(root: &Path, rel: &str, content: &str) {
    let abs = root.join(rel.trim_start_matches("./"));
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(abs, content).expect("write file");
}

fn write_json_file(root: &Path, rel: &str, value: &Value) {
    let abs = root.join(rel.trim_start_matches("./"));
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    let raw = serde_json::to_string_pretty(value).expect("serialize json");
    fs::write(abs, raw).expect("write json file");
}

fn read_json_file(root: &Path, rel: &str) -> Value {
    let abs = root.join(rel.trim_start_matches("./"));
    let raw = fs::read_to_string(abs).expect("read json file");
    serde_json::from_str(&raw).expect("parse json file")
}

#[test]
fn init_creates_kbignore() {
    let dir = unique_temp_dir("init-kbignore");
    let (ok, body) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert!(dir.join(".kb/.kbignore").exists());
    let ignore_raw = fs::read_to_string(dir.join(".kb/.kbignore")).expect("read .kbignore");
    assert!(ignore_raw.contains("./kb.md"));
    assert!(ignore_raw.contains("./.kb/refs.json"));
    assert!(dir.join(".kb/raw").exists());
}

#[test]
fn add_copy_mode_writes_into_hidden_raw_dir() {
    let dir = unique_temp_dir("add-copy-raw");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/copy.md", "COPY");

    let (ok, body) = run_json(
        &["add", "./docs/copy.md", "docs/copy", "--mode", "copy"],
        &dir,
    );
    assert!(ok);
    assert_eq!(body["data"]["path"], "./.kb/raw/copy.md");
    assert!(dir.join(".kb/raw/copy.md").exists());
}

#[test]
fn add_ignored_path_is_non_error_skip() {
    let dir = unique_temp_dir("add-ignore");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/.kbignore", "ignored/*\n");
    mk_file(&dir, "./ignored/a.md", "hello");

    let (ok, body) = run_json(&["add", "./ignored/a.md", "ignored/a"], &dir);
    assert!(ok, "ignored add should not be an error");
    assert_eq!(body["data"]["ignored"], true);
}

#[test]
fn add_rejects_protected_internal_paths() {
    let dir = unique_temp_dir("add-protected-path");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, kb_md) = run_json(&["add", "./kb.md", "meta/kbmd"], &dir);
    assert!(!ok);
    assert_eq!(kb_md["error"]["code"], "E2009");

    mk_file(&dir, "./.kb/index.local.json", "{}");
    let (ok, kb_cfg) = run_json(&["add", "./.kb/index.local.json", "meta/index"], &dir);
    assert!(!ok);
    assert_eq!(kb_cfg["error"]["code"], "E2009");

    mk_file(&dir, "./.kb/raw/manual.md", "RAW");
    let (ok, raw_ok) = run_json(&["add", "./.kb/raw/manual.md", "raw/manual"], &dir);
    assert!(ok);
    assert_eq!(raw_ok["data"]["path"], "./.kb/raw/manual.md");
}

#[test]
fn add_default_append_and_category() {
    let dir = unique_temp_dir("add-category");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");

    let (ok, body) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);
    assert_eq!(body["data"]["alias"], "docs/a");
    assert_eq!(body["data"]["path"], "./docs/a.md");
}

#[test]
fn rm_updates_kb_md_only_and_check_detects_deletion_until_update() {
    let dir = unique_temp_dir("rm-kbmd-only");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[test](docs/a.md):r
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "test",
                    "alias": "test",
                    "path": "./docs/a.md",
                    "perms": "r",
                    "source": "kbmd",
                    "category": "docs"
                }
            ]
        }),
    );

    let (ok, removed) = run_json(&["rm", "test"], &dir);
    assert!(ok);
    assert_eq!(removed["data"]["removed_alias"], "test");
    assert_eq!(removed["data"]["path"], "./docs/a.md");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(!kb_md.contains("[test](docs/a.md):r"));

    let regs_before_update = read_json_file(&dir, "./.kb/regs.json");
    assert_eq!(regs_before_update["regs"][0]["path"], "./docs/a.md");

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_remove = check["data"]["to_remove"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(to_remove
        .iter()
        .any(|x| x["path"] == "./docs/a.md" && x["reason"] == "deleted"));

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let regs_after_update = read_json_file(&dir, "./.kb/regs.json");
    let items = regs_after_update["regs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!items.iter().any(|x| x["path"] == "./docs/a.md"));
}

#[test]
fn rm_supports_path_identifier_for_empty_alias_reference() {
    let dir = unique_temp_dir("rm-empty-alias-path-id");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[](docs/a.md):r
"#,
    );

    let (ok, removed) = run_json(&["rm", "docs/a.md"], &dir);
    assert!(ok);
    assert_eq!(removed["data"]["removed_alias"], "./docs/a.md");
    assert_eq!(removed["data"]["path"], "./docs/a.md");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(!kb_md.contains("[](docs/a.md):r"));
}

#[test]
fn chmod_updates_kb_md_reference_and_check_detects_permission_change() {
    let dir = unique_temp_dir("chmod-kbmd-reference");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[test](docs/a.md):r
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "test",
                    "alias": "test",
                    "path": "./docs/a.md",
                    "perms": "r",
                    "source": "kbmd",
                    "category": "docs"
                }
            ]
        }),
    );

    let (ok, chmod) = run_json(&["chmod", "test", "rw"], &dir);
    assert!(ok);
    assert_eq!(chmod["data"]["permissions"], "rw");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(kb_md.contains("[test](docs/a.md):rw"));

    let regs_before = read_json_file(&dir, "./.kb/regs.json");
    assert_eq!(regs_before["regs"][0]["perms"], "r");

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_update = check["data"]["to_update"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(to_update
        .iter()
        .any(|x| x["path"] == "./docs/a.md" && x["reason"] == "metadata_changed"));

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);
    let regs_after = read_json_file(&dir, "./.kb/regs.json");
    assert_eq!(regs_after["regs"][0]["perms"], "rw");
}

#[test]
fn chmod_updates_category_permission_and_check_detects_changes() {
    let dir = unique_temp_dir("chmod-kbmd-category");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[test](docs/a.md)
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "test",
                    "alias": "test",
                    "path": "./docs/a.md",
                    "perms": "r",
                    "source": "kbmd",
                    "category": "docs"
                }
            ]
        }),
    );

    let (ok, chmod) = run_json(&["chmod", "docs", "rw"], &dir);
    assert!(ok);
    assert_eq!(chmod["data"]["alias"], "docs");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(kb_md.contains("# docs:rw"));

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_update = check["data"]["to_update"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(to_update
        .iter()
        .any(|x| x["path"] == "./docs/a.md" && x["reason"] == "metadata_changed"));
}

#[test]
fn check_detects_additions_from_kb_md_and_raw_against_regs() {
    let dir = unique_temp_dir("check-kbmd-raw-regs-add");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(&dir, "./.kb/raw/raw-a.md", "RAW A");
    mk_file(
        &dir,
        "./kb.md",
        r#"---
kb: demo
default_perm: r
---

# docs:rw
[docs/a](docs/a.md):rw
"#,
    );
    write_json_file(&dir, "./.kb/regs.json", &json!({ "regs": [] }));

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);

    let to_add = check["data"]["to_add"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        to_add
            .iter()
            .any(|x| x["path"] == "./docs/a.md" && x["source"] == "kbmd"),
        "kb.md file should be recognized as add"
    );
    assert!(
        to_add
            .iter()
            .any(|x| x["path"] == "./.kb/raw/raw-a.md" && x["source"] == "raw"),
        "raw file should be recognized as add"
    );
}

#[test]
fn check_counts_directory_and_file_declarations_separately() {
    let dir = unique_temp_dir("check-dir-and-file-separate-add");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[docs-dir](docs/):r
[docs-file](docs/a.md):r
"#,
    );
    write_json_file(&dir, "./.kb/regs.json", &json!({ "regs": [] }));

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);

    let to_add = check["data"]["to_add"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        to_add.len(),
        2,
        "directory declaration and file declaration should be counted separately"
    );
    assert!(
        to_add
            .iter()
            .any(|x| x["path"] == "./docs/" && x["source"] == "kbmd"),
        "directory declaration should appear in to_add"
    );
    assert!(
        to_add
            .iter()
            .any(|x| x["path"] == "./docs/a.md" && x["source"] == "kbmd"),
        "file declaration should appear in to_add"
    );
}

#[test]
fn check_marks_deleted_and_not_found_against_regs() {
    let dir = unique_temp_dir("check-regs-delete-notfound");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/live.md", "LIVE");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[missing](docs/missing.md):r
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "live",
                    "alias": "live",
                    "path": "./docs/live.md",
                    "perms": "r",
                    "source": "kbmd"
                },
                {
                    "id": "missing",
                    "alias": "missing",
                    "path": "./docs/missing.md",
                    "perms": "r",
                    "source": "kbmd"
                }
            ]
        }),
    );

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_remove = check["data"]["to_remove"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    assert!(
        to_remove
            .iter()
            .any(|x| x["path"] == "./docs/live.md" && x["reason"] == "deleted"),
        "registry entry missing from kb.md/raw should be marked deleted"
    );
    assert!(
        to_remove
            .iter()
            .any(|x| x["path"] == "./docs/missing.md" && x["reason"] == "not_found"),
        "kb.md entry pointing to a missing file should be marked not_found"
    );
}

#[test]
fn update_applies_add_and_remove_to_regs_file() {
    let dir = unique_temp_dir("update-regs-apply");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/new.md", "NEW");
    mk_file(&dir, "./docs/stale.md", "STALE");
    mk_file(&dir, "./.kb/raw/raw-new.md", "RAW NEW");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:rw
[docs/new](docs/new.md):rw
[missing](docs/missing.md):r
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "docs/stale",
                    "alias": "docs/stale",
                    "path": "./docs/stale.md",
                    "perms": "rw",
                    "source": "kbmd"
                },
                {
                    "id": "missing",
                    "alias": "missing",
                    "path": "./docs/missing.md",
                    "perms": "r",
                    "source": "kbmd"
                }
            ]
        }),
    );

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let regs = read_json_file(&dir, "./.kb/regs.json");
    let items = regs["regs"].as_array().cloned().unwrap_or_default();
    let paths = items
        .iter()
        .filter_map(|x| x["path"].as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    assert!(
        paths.iter().any(|x| x == "./docs/new.md"),
        "new kb.md file should be added into regs"
    );
    assert!(
        paths.iter().any(|x| x == "./.kb/raw/raw-new.md"),
        "raw file should be added into regs"
    );
    assert!(
        !paths.iter().any(|x| x == "./docs/stale.md"),
        "deleted file should be removed from regs"
    );
    assert!(
        !paths.iter().any(|x| x == "./docs/missing.md"),
        "not_found file should be removed from regs"
    );
}

#[test]
fn update_prunes_missing_kb_md_references() {
    let dir = unique_temp_dir("update-prune-kbmd-missing");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[missing](docs/missing.md):r
[still](docs/still.md):r
"#,
    );
    mk_file(&dir, "./docs/still.md", "STILL");
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "missing",
                    "alias": "missing",
                    "path": "./docs/missing.md",
                    "perms": "r",
                    "source": "kbmd",
                    "category": "docs"
                },
                {
                    "id": "still",
                    "alias": "still",
                    "path": "./docs/still.md",
                    "perms": "r",
                    "source": "kbmd",
                    "category": "docs"
                }
            ]
        }),
    );

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(!kb_md.contains("[missing](docs/missing.md):r"));
    assert!(kb_md.contains("[still](docs/still.md):r"));
}

#[test]
fn update_idempotency_and_prune_behavior() {
    let dir = unique_temp_dir("update-idempotent");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, first) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(first["data"]["added"], 1);

    let (ok, second) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(second["data"]["added"], 0);
    assert_eq!(second["data"]["updated"], 0);
    assert_eq!(second["data"]["removed"], 0);

    fs::remove_file(dir.join("docs/a.md")).expect("remove file for missing path test");
    let (ok, marked) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(marked["data"]["marked_invalid"], 0);
    assert_eq!(marked["data"]["removed"], 1);

    let (ok, pruned) = run_json(&["update", "--prune"], &dir);
    assert!(ok);
    assert_eq!(pruned["data"]["removed"], 0);
}

#[test]
fn update_dry_run_does_not_persist_baseline() {
    let dir = unique_temp_dir("update-dryrun");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, dry) = run_json(&["update", "--dry-run"], &dir);
    assert!(ok);
    assert_eq!(dry["data"]["added"], 1);
    assert!(!dir.join(".kb/index.json").exists());

    let (ok, persisted) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(persisted["data"]["added"], 1);
    assert!(dir.join(".kb/index.json").exists());
    assert!(dir.join(".kb/index.errors.json").exists());
    assert!(dir.join(".kb/index.meta.json").exists());
}

#[test]
fn update_prunes_index_entries_confirmed_by_check_even_if_not_in_regs() {
    let dir = unique_temp_dir("update-prune-check-confirmed-removals");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    fs::remove_file(dir.join("docs/a.md")).expect("remove file");
    write_json_file(&dir, "./.kb/regs.json", &json!({ "regs": [] }));

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_remove = check["data"]["to_remove"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        to_remove
            .iter()
            .any(|x| x["path"] == "./docs/a.md" && x["reason"] == "not_found"),
        "check should confirm missing kb.md entry as to_remove/not_found"
    );

    let (ok, update) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(update["status"], "success");

    let index_raw = fs::read_to_string(dir.join(".kb/index.json")).expect("read index");
    assert!(
        !index_raw.contains("./docs/a.md"),
        "removed/not_found entry should be deleted from index snapshot"
    );
}

#[test]
fn check_and_update_apply_kbignore_before_scan() {
    let dir = unique_temp_dir("check-ignore");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/.kbignore", "./docs/skip.md\n");
    mk_file(&dir, "./docs/skip.md", "SKIP");
    mk_file(&dir, "./docs/keep.md", "KEEP");

    let (ok, _) = run_json(
        &["add", "./docs/keep.md", "docs/keep", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (_ok, _ignored) = run_json(
        &["add", "./docs/skip.md", "docs/skip", "--category", "raw"],
        &dir,
    );

    let (ok, check) = run_json(&["check"], &dir);
    assert!(ok);
    let to_add = check["data"]["to_add"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(to_add
        .iter()
        .all(|x| x["path"].as_str().unwrap_or_default() != "./docs/skip.md"));

    let (ok, update) = run_json(&["update"], &dir);
    assert!(ok);
    let raw = fs::read_to_string(dir.join(".kb/index.json")).expect("read index");
    assert!(!raw.contains("./docs/skip.md"));
    assert!(update["data"]["added"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn update_appends_new_raw_files_into_kb_md_raw_category() {
    let dir = unique_temp_dir("update-append-raw-into-kbmd");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/raw/new-raw.md", "RAW NEW");

    let (ok, first) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(first["data"]["added"], 1);

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(
        kb_md.contains("# raw"),
        "kb.md should keep raw category heading"
    );
    assert!(
        kb_md.contains("[](./.kb/raw/new-raw.md)"),
        "kb update should append raw file into kb.md using markdown reference format"
    );

    let (ok, second) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(second["data"]["added"], 0);

    let kb_md_again = fs::read_to_string(dir.join("kb.md")).expect("read kb.md again");
    let count = kb_md_again.matches("[](./.kb/raw/new-raw.md)").count();
    assert_eq!(count, 1, "raw reference should not be duplicated");
}

#[test]
fn update_dedupes_duplicate_raw_refs_in_kb_md() {
    let dir = unique_temp_dir("update-dedupe-raw-kbmd");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/raw/a.md", "A");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# raw:r
[](./.kb/raw/a.md)
[](./.kb/raw/a.md)
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "./.kb/raw/a.md",
                    "alias": null,
                    "path": "./.kb/raw/a.md",
                    "perms": "r",
                    "source": "raw",
                    "category": "raw"
                }
            ]
        }),
    );

    let (ok, update) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(update["status"], "success");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    let count = kb_md.matches("[](./.kb/raw/a.md)").count();
    assert_eq!(count, 1, "duplicate raw references should be deduplicated");
}

#[test]
fn update_ignores_protected_paths_even_if_state_contains_them() {
    let dir = unique_temp_dir("update-ignore-protected");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let refs_path = dir.join(".kb/refs.json");
    let mut refs_doc: Value =
        serde_json::from_str(&fs::read_to_string(&refs_path).expect("read refs"))
            .expect("parse refs");
    refs_doc["refs"]
        .as_array_mut()
        .expect("refs array")
        .push(json!({
            "alias": "meta/kbmd",
            "path": "./kb.md",
            "category": "raw",
            "perms": "rwx"
        }));
    refs_doc["refs"]
        .as_array_mut()
        .expect("refs array")
        .push(json!({
            "alias": "meta/refs",
            "path": "./.kb/refs.json",
            "category": "raw",
            "perms": "rwx"
        }));
    fs::write(
        &refs_path,
        serde_json::to_string_pretty(&refs_doc).expect("serialize refs"),
    )
    .expect("write refs");

    let (ok, update) = run_json(&["update"], &dir);
    assert!(ok);
    assert!(update["data"]["added"].as_u64().unwrap_or(0) >= 1);

    let index_raw = fs::read_to_string(dir.join(".kb/index.json")).expect("read index");
    assert!(!index_raw.contains("./kb.md"));
    assert!(!index_raw.contains("./.kb/refs.json"));
    assert!(index_raw.contains("./docs/a.md"));
}

#[test]
fn list_filter_long_tree_support() {
    let dir = unique_temp_dir("list-filter");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    mk_file(&dir, "./src/b.rs", "B");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["add", "./src/b.rs", "src/b", "--category", "raw"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, filtered) = run_json(&["list", "--filter", "docs/*", "--long", "--tree"], &dir);
    assert!(ok);
    let items = filtered["data"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["alias"], "docs/a");
    assert!(items[0].get("size").is_some());
    assert!(items[0].get("mtime").is_some());
    assert!(filtered["data"].get("categories").is_some());
}

#[test]
fn list_uses_regs_json_as_source_of_truth() {
    let dir = unique_temp_dir("list-from-regs");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/in-kb.md", "KB");
    mk_file(&dir, "./docs/in-regs.md", "REGS");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# docs:r
[only-kb](docs/in-kb.md):r
"#,
    );
    write_json_file(
        &dir,
        "./.kb/regs.json",
        &json!({
            "regs": [
                {
                    "id": "only-regs",
                    "alias": "only-regs",
                    "path": "./docs/in-regs.md",
                    "perms": "r",
                    "source": "kbmd"
                }
            ]
        }),
    );

    let (ok, listed) = run_json(&["list"], &dir);
    assert!(ok);

    let items = listed["data"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        items.iter().any(|x| x["path"] == "./docs/in-regs.md"),
        "list should include records from regs.json"
    );
    assert!(
        !items.iter().any(|x| x["path"] == "./docs/in-kb.md"),
        "list should not read directly from kb.md"
    );
}

#[test]
fn list_invalid_filter_returns_e2002() {
    let dir = unique_temp_dir("list-invalid-filter");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["list", "--filter", "[invalid"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E2002");
}

#[test]
fn ask_returns_disabled_engine_and_retrieval_structure() {
    let dir = unique_temp_dir("ask-success");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(
        &dir,
        "./docs/config.md",
        "Use kb config to change settings.\nMore details",
    );
    let (ok, _) = run_json(
        &[
            "add",
            "./docs/config.md",
            "docs/config",
            "--category",
            "raw",
        ],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(&dir)
        .arg("ask")
        .arg("hello")
        .arg("--top")
        .arg("1")
        .arg("--output")
        .arg("json")
        .assert();
    let output = assert.get_output();
    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));

    let body: Value = serde_json::from_slice(&output.stdout).expect("json stdout for ask success");
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["engine_status"], "disabled");
    assert!(body["data"]["provider"].is_null());
    assert!(body["data"]["trace_id"].is_null());

    let retrieved = body["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context must be array");
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0]["alias"], "docs/config");
    assert_eq!(retrieved[0]["snippet"], "Use kb config to change settings.");
    let score = retrieved[0]["score"].as_f64().unwrap_or_default();
    assert!(score > 0.0 && score <= 1.0);
}

#[test]
fn ask_top_k_order_is_deterministic_for_multiple_docs() {
    let dir = unique_temp_dir("ask-top-k-deterministic");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/beta.md", "beta topic reference");
    mk_file(&dir, "./docs/alpha.md", "alpha topic reference");

    let (ok, _) = run_json(
        &["add", "./docs/beta.md", "docs/beta", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(
        &["add", "./docs/alpha.md", "docs/alpha", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["ask", "topic", "--top", "2"], &dir);
    assert!(ok);
    let retrieved = body["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context must be array");
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0]["alias"], "docs/alpha");
    assert_eq!(retrieved[1]["alias"], "docs/beta");
}

#[test]
fn ask_query_density_window_snippet_golden() {
    let dir = unique_temp_dir("ask-density-window-golden");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(
        &dir,
        "./docs/window.md",
        "alpha lead noise before target section with beta and target and gamma plus tail",
    );
    let (ok, _) = run_json(
        &[
            "add",
            "./docs/window.md",
            "docs/window",
            "--category",
            "raw",
        ],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["ask", "beta target gamma", "--top", "1"], &dir);
    assert!(ok);
    let snippet = body["data"]["retrieved_context"][0]["snippet"]
        .as_str()
        .unwrap_or_default();
    assert!(snippet.contains("beta"));
    assert!(snippet.contains("target"));
    assert!(snippet.contains("gamma"));
}

#[test]
fn ask_query_density_unicode_multibyte_boundary_golden() {
    let dir = unique_temp_dir("ask-unicode-boundary-golden");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(
        &dir,
        "./docs/unicode.md",
        "前置噪声🙂🙂🙂 alpha βeta 命中片段 γamma 尾部文本",
    );
    let (ok, _) = run_json(
        &[
            "add",
            "./docs/unicode.md",
            "docs/unicode",
            "--category",
            "raw",
        ],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["ask", "alpha βeta γamma", "--top", "1"], &dir);
    assert!(ok);
    let snippet = body["data"]["retrieved_context"][0]["snippet"]
        .as_str()
        .unwrap_or_default();
    assert!(snippet.contains("alpha"));
    assert!(snippet.contains("βeta"));
    assert!(snippet.contains("γamma"));
}

#[test]
fn ask_query_density_grapheme_cluster_boundary_golden() {
    let dir = unique_temp_dir("ask-grapheme-cluster-golden");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(
        &dir,
        "./docs/grapheme.md",
        "前置片段 👨‍👩‍👧‍👦 family e\u{0301} 组合字符 and emoji 👍🏽 tail",
    );
    let (ok, _) = run_json(
        &[
            "add",
            "./docs/grapheme.md",
            "docs/grapheme",
            "--category",
            "raw",
        ],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["ask", "family e\u{0301} 👍🏽", "--top", "1"], &dir);
    assert!(ok);
    let snippet = body["data"]["retrieved_context"][0]["snippet"]
        .as_str()
        .unwrap_or_default();
    assert!(snippet.contains("👨‍👩‍👧‍👦"));
    assert!(snippet.contains("e\u{0301}") || snippet.contains("é"));
    assert!(snippet.contains("👍🏽"));
}

#[test]
fn ask_without_context_returns_e5003() {
    let dir = unique_temp_dir("ask-failure");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["ask", "unrelated question"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E5003");
    assert_eq!(
        body["error"]["details"]["query"],
        Value::String("unrelated question".to_string())
    );
}

#[test]
fn completion_bash_outputs_script_in_text_mode() {
    let dir = unique_temp_dir("completion-bash");
    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(&dir)
        .args(["completion", "bash"])
        .assert();
    let output = assert.get_output();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_kb()") || stdout.contains(" kb"));
}

#[test]
fn completion_json_contains_script_field() {
    let dir = unique_temp_dir("completion-json");
    let (ok, body) = run_json(&["completion", "bash"], &dir);
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["shell"], "bash");
    assert!(body["data"]["script"]
        .as_str()
        .unwrap_or_default()
        .contains("kb"));
}

#[test]
fn invalid_config_path_returns_e2008() {
    let dir = unique_temp_dir("invalid-config");
    let (ok, body) = run_json(&["--config", "./no/such/config.toml", "init", "demo"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E2008");
}

#[test]
fn env_list_shows_only_existing_registered_kb_roots() {
    let global_root = unique_temp_dir("env-global-root");
    let caller = unique_temp_dir("env-caller");
    let kb_one = unique_temp_dir("env-kb-one");
    let global_config = global_root.join("config.json");

    let (ok, _) = run_json_with_global_config(&["init", "demo"], &kb_one, &global_config);
    assert!(ok, "init should register kb root into global config");

    let kb_one_abs = fs::canonicalize(&kb_one)
        .expect("kb root should exist")
        .to_string_lossy()
        .to_string();

    let (ok, listed) = run_json_with_global_config(&["env", "--list"], &caller, &global_config);
    assert!(ok);
    assert_eq!(listed["status"], "success");
    let items = listed["data"]["items"]
        .as_array()
        .expect("items must be array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["path"].as_str().unwrap_or_default(), kb_one_abs);
    assert_eq!(listed["data"]["recorded_total"].as_u64().unwrap_or(0), 1);
    assert_eq!(listed["data"]["count"].as_u64().unwrap_or(0), 1);

    fs::remove_dir_all(&kb_one).expect("remove kb root to simulate missing");
    let (ok, listed_after_remove) =
        run_json_with_global_config(&["env", "--list"], &caller, &global_config);
    assert!(ok);
    assert_eq!(
        listed_after_remove["data"]["recorded_total"]
            .as_u64()
            .unwrap_or(0),
        1
    );
    assert_eq!(
        listed_after_remove["data"]["count"].as_u64().unwrap_or(0),
        0
    );
    let items = listed_after_remove["data"]["items"]
        .as_array()
        .expect("items must be array");
    assert!(items.is_empty());
}

#[test]
fn env_list_text_is_human_readable_multiline() {
    let global_root = unique_temp_dir("env-text-global-root");
    let caller = unique_temp_dir("env-text-caller");
    let kb_one = unique_temp_dir("env-text-kb-one");
    let global_config = global_root.join("config.json");

    let (ok, _) = run_json_with_global_config(&["init", "demo"], &kb_one, &global_config);
    assert!(ok);

    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(&caller)
        .env("KB_GLOBAL_CONFIG", &global_config)
        .args(["env", "--list"])
        .assert();

    let output = assert.get_output();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("config_path:"));
    assert!(stdout.contains("items:"));
    assert!(stdout.contains("count:"));
    assert!(stdout.contains("recorded_total:"));
    assert!(stdout.contains("- "));
    assert!(
        !stdout.trim_start().starts_with('{'),
        "text output should not be rendered as raw JSON"
    );
}
