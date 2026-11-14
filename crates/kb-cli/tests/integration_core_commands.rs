#[allow(dead_code)]
mod common;

use common::{kb_command_in, kb_command_with_env, kb_command_with_global_config};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_json(args: &[&str], current_dir: &Path) -> (bool, Value) {
    let assert = kb_command_in(current_dir)
        .arg("--output")
        .arg("json")
        .args(args)
        .assert();
    let success = assert.get_output().status.success();
    let body: Value = serde_json::from_slice(&assert.get_output().stdout).expect("json stdout");
    (success, body)
}

fn run_json_with_env(args: &[&str], current_dir: &Path, envs: &[(&str, &str)]) -> (bool, Value) {
    let assert = kb_command_with_env(current_dir, envs)
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
    let assert = kb_command_with_global_config(current_dir, global_config)
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

fn normalize_display_path(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = path.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    path.to_string()
}

fn canonical_display_path(path: &Path) -> String {
    let raw = fs::canonicalize(path)
        .expect("path should exist")
        .to_string_lossy()
        .to_string();
    normalize_display_path(&raw)
}

#[test]
fn init_creates_kbignore() {
    let dir = unique_temp_dir("init-kbignore");
    let (ok, body) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert!(dir.join(".kb/.kbignore").exists());
    assert!(dir.join(".kb/writer.lock").is_file());
    let ignore_raw = fs::read_to_string(dir.join(".kb/.kbignore")).expect("read .kbignore");
    assert!(ignore_raw.contains("./kb.md"));
    assert!(ignore_raw.contains("./.kb/refs.json"));
    assert!(dir.join(".kb/raw").exists());
    assert!(dir.join(".kb/.env.template").exists());
    let env_template =
        fs::read_to_string(dir.join(".kb/.env.template")).expect("read llm env template");
    assert!(env_template.contains("KB_LLM_SEMANTIC_FLOW=two_step"));
    assert!(env_template.contains("KB_LLM_RETRY_MAX_ATTEMPTS=3"));
    assert!(dir.join(".kb/config.json").exists());
    assert!(
        dir.join("global-config.json").exists(),
        "init should write the global registry into the test directory"
    );
    let local_cfg = read_json_file(&dir, "./.kb/config.json");
    assert!(local_cfg.get("llm").is_some());
    assert_eq!(local_cfg["llm"]["semantic_flow"], "two_step");
    assert_eq!(
        body["data"]["project_config_path"]
            .as_str()
            .unwrap_or_default(),
        "./.kb/config.json"
    );
    assert!(dir.join(".kb/README.md").exists());
    let guide_raw = fs::read_to_string(dir.join(".kb/README.md")).expect("read kb guide");
    assert!(guide_raw.contains(".kb/index.json"));
    assert!(guide_raw.contains(".kb/knowledge.json"));
    assert!(guide_raw.contains(".kb/search.json"));
    assert!(guide_raw.contains(".kb/graph.json"));
    assert!(guide_raw.contains(".kb/wiki/"));
    assert!(guide_raw.contains(".kb/.env.template"));
    assert!(guide_raw.contains(".kb/config.json"));
    assert_eq!(
        body["data"]["kb_guide_path"].as_str().unwrap_or_default(),
        "./.kb/README.md"
    );
}

#[test]
fn add_copy_mode_writes_into_hidden_raw_dir() {
    let dir = unique_temp_dir("add-copy-raw");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/copy.md", "COPY");
    let source = dir.join("docs/copy.md");
    let mut permissions = fs::metadata(&source).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&source, permissions.clone()).unwrap();

    let (ok, body) = run_json(
        &["add", "./docs/copy.md", "docs/copy", "--mode", "copy"],
        &dir,
    );
    assert!(ok);
    assert_eq!(body["data"]["path"], "./.kb/raw/copy.md");
    assert!(dir.join(".kb/raw/copy.md").exists());
    assert_eq!(fs::metadata(dir.join(".kb/raw/copy.md")).unwrap().permissions(), permissions);
}

#[test]
#[cfg(windows)]
fn add_copy_unreadable_source_does_not_create_target() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = unique_temp_dir("add-copy-locked-source");
    assert!(run_json(&["init"], &dir).0);
    mk_file(&dir, "./source.md", "original");
    let kb_before = fs::read(dir.join("kb.md")).unwrap();
    let refs_before = fs::read(dir.join(".kb/refs.json")).unwrap();
    let _held = fs::OpenOptions::new().read(true).share_mode(0).open(dir.join("source.md")).unwrap();
    let (ok, body) = run_json(&["add", "source.md", "locked", "--mode", "copy"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E1001");
    assert!(!dir.join(".kb/raw/source.md").exists());
    assert_eq!(fs::read(dir.join("kb.md")).unwrap(), kb_before);
    assert_eq!(fs::read(dir.join(".kb/refs.json")).unwrap(), refs_before);
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
fn add_inserts_before_child_heading_in_target_category() {
    let dir = unique_temp_dir("add-before-child-heading");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/new.md", "NEW");
    mk_file(&dir, "./docs/existing.md", "EXISTING");
    mk_file(&dir, "./docs/child.md", "CHILD");
    mk_file(
        &dir,
        "./kb.md",
        "# docs:r\n[existing](./docs/existing.md):r\n\n## child\n[child](./docs/child.md):r\n# notes:rw\n",
    );

    let (ok, _) = run_json(&["add", "./docs/new.md", "new", "--category", "docs"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    let new_line = kb_md.find("[new](./docs/new.md)").expect("new reference");
    let child_heading = kb_md.find("## child").expect("child heading");
    assert!(
        new_line < child_heading,
        "new reference must remain in docs"
    );

    let (ok, listed) = run_json(&["list", "--long", "--tree"], &dir);
    assert!(ok);
    let items = listed["data"]["items"].as_array().expect("items array");
    let new_item = items
        .iter()
        .find(|item| item["alias"] == "new")
        .expect("new alias should be indexed");
    assert_eq!(new_item["perms"], "r");
    let categories = listed["data"]["categories"]
        .as_array()
        .expect("categories array");
    assert!(categories.iter().any(|category| category == "docs"));
}

#[test]
fn writer_lock_blocks_cli_writes_but_not_read_or_episode_append_commands() {
    let dir = unique_temp_dir("writer-lock-cli-boundary");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let kb_md = fs::read(dir.join("kb.md")).expect("read initial kb.md");
    let lock = kb_storage_fs::SingleWriterLockManager::new(dir.join(".kb"))
        .acquire()
        .expect("acquire test writer lock")
        .expect("file locks should be supported");

    let (ok, list) = run_json(&["list"], &dir);
    assert!(
        ok,
        "list must remain available while writer lock is held: {list}"
    );
    let (ok, ask) = run_json(&["ask", "no context"], &dir);
    assert!(!ok);
    assert_eq!(ask["error"]["code"], "E5003");
    let (ok, remember) = run_json(&["remember", "append-only fact"], &dir);
    assert!(ok, "remember should remain available: {remember}");

    let (ok, add) = run_json(&["add", "./docs/a.md", "a"], &dir);
    assert!(!ok, "add must fail while another writer holds the lock");
    assert_eq!(add["error"]["code"], "E3004");
    assert_eq!(
        add["error"]["details"]["pid"],
        std::process::id(),
        "writer-lock error should include owner PID: {add}"
    );
    assert_eq!(
        fs::read(dir.join("kb.md")).expect("kb.md after rejected add"),
        kb_md
    );

    lock.release().expect("release test lock");
    let (ok, _) = run_json(&["add", "./docs/a.md", "a"], &dir);
    assert!(ok, "add should succeed after releasing writer lock");
}

#[test]
fn stale_writer_lock_file_does_not_block_cli_write() {
    let dir = unique_temp_dir("writer-lock-stale-file");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    fs::write(dir.join(".kb/writer.lock"), "pid=old started=1\n")
        .expect("write stale lock contents");

    let (ok, body) = run_json(&["add", "./docs/a.md", "a"], &dir);
    assert!(ok, "a lock file without an OS lock is reusable: {body}");
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
fn rm_raw_without_confirmation_is_ignored() {
    let dir = unique_temp_dir("rm-raw-no-confirm");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/raw/a.md", "RAW");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# raw:r
[raw-a](./.kb/raw/a.md):r
"#,
    );

    let (ok, removed) = run_json(&["rm", "raw-a"], &dir);
    assert!(ok);
    assert_eq!(removed["data"]["ignored"], true);
    assert_eq!(removed["data"]["reason"], "raw_delete_not_confirmed");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(
        kb_md.contains("[raw-a](./.kb/raw/a.md):r"),
        "rm should be ignored when raw delete is not confirmed"
    );
    assert!(
        dir.join(".kb/raw/a.md").is_file(),
        "raw source file should stay when rm is ignored"
    );
}

#[test]
fn rm_raw_with_force_removes_source_file_and_declaration() {
    let dir = unique_temp_dir("rm-raw-force");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./.kb/raw/a.md", "RAW");
    mk_file(
        &dir,
        "./kb.md",
        r#"
# raw:r
[raw-a](./.kb/raw/a.md):r
"#,
    );

    let (ok, removed) = run_json(&["rm", "raw-a", "--force"], &dir);
    assert!(ok);
    assert_eq!(removed["data"]["removed_alias"], "raw-a");
    assert_eq!(removed["data"]["path"], "./.kb/raw/a.md");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(
        !kb_md.contains("[raw-a](./.kb/raw/a.md):r"),
        "kb.md declaration should be removed after force rm"
    );
    assert!(
        !dir.join(".kb/raw/a.md").exists(),
        "raw source file should be deleted after force rm"
    );
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
fn update_keeps_missing_declarations_until_prune_is_requested() {
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

    let (ok, updated) = run_json(&["update"], &dir);
    assert!(ok);
    let missing = updated["data"]["missing_declarations"]
        .as_array()
        .expect("missing declarations array");
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["alias"], "missing");
    assert_eq!(missing[0]["path"], "./docs/missing.md");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(kb_md.contains("[missing](docs/missing.md):r"));
    assert!(kb_md.contains("[still](docs/still.md):r"));
    let (ok, listed) = run_json(&["list"], &dir);
    assert!(ok);
    let items = listed["data"]["items"].as_array().expect("items array");
    assert!(!items.iter().any(|item| item["path"] == "./docs/missing.md"));

    let regs = read_json_file(&dir, "./.kb/regs.json");
    assert!(!regs["regs"]
        .as_array()
        .expect("regs array")
        .iter()
        .any(|item| item["path"] == "./docs/missing.md"));

    let text_update = kb_command_in(&dir)
        .args(["update"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&text_update.get_output().stdout);
    assert!(text.contains(
        "1 declared path(s) not found; run 'kb update --prune' to remove them from kb.md"
    ));

    let (ok, pruned) = run_json(&["update", "--prune"], &dir);
    assert!(ok);
    assert!(pruned["data"]["missing_declarations"]
        .as_array()
        .expect("missing declarations array")
        .is_empty());
    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read pruned kb.md");
    assert!(!kb_md.contains("[missing](docs/missing.md):r"));
    assert!(kb_md.contains("[still](docs/still.md):r"));
}

#[test]
fn missing_declarations_do_not_fail_strict_and_dry_run_prune_does_not_write() {
    let dir = unique_temp_dir("update-missing-strict-dry-run");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./kb.md", "# docs:r\n[missing](docs/missing.md):r\n");

    let original = fs::read(dir.join("kb.md")).expect("read original kb.md");
    let (ok, dry_run) = run_json(&["update", "--dry-run", "--prune"], &dir);
    assert!(ok);
    assert_eq!(dry_run["data"]["missing_declarations"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read(dir.join("kb.md")).expect("read dry-run kb.md"), original);

    let (ok, strict) = run_json(&["update", "--strict"], &dir);
    assert!(ok, "missing declarations alone must not fail strict mode: {strict}");
    assert_eq!(strict["data"]["missing_declarations"].as_array().unwrap().len(), 1);
}

#[test]
fn strict_layer_failure_keeps_missing_declarations_in_error_details() {
    let dir = unique_temp_dir("update-strict-missing-details");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/live.md", "LIVE");
    mk_file(
        &dir,
        "./kb.md",
        "# docs:r\n[missing](docs/missing.md):r\n[live](docs/live.md):r\n",
    );

    let (ok, result) = run_json_with_env(
        &["update", "--strict"],
        &dir,
        &[("KB_UPDATE_SIMULATE_LAYER_FAILURES", "l2_semantic")],
    );

    assert!(!ok);
    assert_eq!(result["error"]["code"], "E1011");
    let missing = result["error"]["details"]["missing_declarations"]
        .as_array()
        .expect("missing declarations in strict error details");
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["alias"], "missing");
    assert_eq!(missing[0]["path"], "./docs/missing.md");
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
    assert_eq!(first["data"]["l2_compilation"]["processed"], 1);
    assert_eq!(first["data"]["l2_compilation"]["reused"], 0);
    assert!(dir.join(".kb/search.json").exists());
    assert!(dir.join(".kb/graph.json").exists());
    assert!(dir.join(".kb/wiki/manifest.json").exists());
    assert!(dir.join(".kb/wiki/index.md").exists());

    let (ok, second) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(second["data"]["added"], 0);
    assert_eq!(second["data"]["updated"], 0);
    assert_eq!(second["data"]["removed"], 0);
    assert_eq!(second["data"]["l2_compilation"]["processed"], 0);
    assert_eq!(second["data"]["l2_compilation"]["reused"], 1);

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
    assert!(!dir.join(".kb/search.json").exists());
    assert!(!dir.join(".kb/graph.json").exists());
    assert!(!dir.join(".kb/wiki/manifest.json").exists());

    let (ok, persisted) = run_json(&["update"], &dir);
    assert!(ok);
    assert_eq!(persisted["data"]["added"], 1);
    assert!(dir.join(".kb/index.json").exists());
    assert!(dir.join(".kb/index.errors.json").exists());
    assert!(dir.join(".kb/index.meta.json").exists());
    assert!(dir.join(".kb/search.json").exists());
    assert!(dir.join(".kb/graph.json").exists());
    assert!(dir.join(".kb/wiki/manifest.json").exists());
    assert!(dir.join(".kb/wiki/index.md").exists());
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
fn update_default_mode_succeeds_with_progressive_warning() {
    let dir = unique_temp_dir("update-default-progressive-warning");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["update"],
        &dir,
        &[("KB_UPDATE_SIMULATE_LAYER_FAILURES", "l2_semantic")],
    );
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["strict"], false);
    assert_eq!(body["data"]["layer_status"]["l0_declaration"], "ok");
    assert_eq!(body["data"]["layer_status"]["l1_registry"], "ok");
    assert_eq!(body["data"]["layer_status"]["l1_index"], "ok");
    assert_eq!(body["data"]["layer_status"]["l2_semantic"], "warning");
    assert_eq!(body["data"]["layer_status"]["l3_projection"], "ok");
    assert_eq!(body["data"]["layer_status"]["l4_graph"], "ok");
    assert_eq!(body["data"]["layer_status"]["l5_wiki_view"], "ok");
    assert_eq!(body["data"]["l2_compilation"]["processed"], 1);
    assert_eq!(body["data"]["l2_compilation"]["reused"], 0);
    assert_eq!(body["data"]["l2_compilation"]["missing"], 0);
    assert_eq!(body["data"]["l2_compilation"]["failed"], 0);
    let warnings = body["data"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["layer"], "l2_semantic");
    assert_eq!(warnings[0]["retryable"], true);
    assert_eq!(
        body["data"]["errors"]
            .as_array()
            .map(|x| x.len())
            .unwrap_or(0),
        0
    );
}

#[test]
fn update_semantic_enabled_without_key_falls_back_with_warning() {
    let dir = unique_temp_dir("update-llm-semantic-missing-key");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["update"],
        &dir,
        &[
            ("KB_LLM_ENABLE_SEMANTIC", "1"),
            ("KB_LLM_SEMANTIC_FLOW", "single-step"),
            ("KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS", "333"),
            ("KB_LLM_RETRY_MAX_ATTEMPTS", "5"),
            ("KB_LLM_RETRY_BACKOFF_MS", "900"),
        ],
    );
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["semantic_extractor"]["mode"], "rule");
    assert_eq!(
        body["data"]["semantic_extractor"]["agent_tools"]["enabled"],
        false
    );
    assert_eq!(
        body["data"]["semantic_extractor"]["semantic_flow"],
        "single_step"
    );
    assert_eq!(
        body["data"]["semantic_extractor"]["analysis_max_output_tokens"],
        333
    );
    assert_eq!(body["data"]["semantic_extractor"]["retry_max_attempts"], 5);
    assert_eq!(body["data"]["semantic_extractor"]["retry_backoff_ms"], 900);
    assert_eq!(body["data"]["layer_status"]["l2_semantic"], "warning");

    let warnings = body["data"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(warnings
        .iter()
        .any(|issue| issue["code"] == "W-L2-LLM-CONFIG-MISSING"));
}

#[test]
fn update_llm_flag_forces_semantic_path_and_warns_when_key_missing() {
    let dir = unique_temp_dir("update-llm-flag-missing-key");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["update", "--llm"], &dir);
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["semantic_extractor"]["forced_by_cli"], true);
    assert_eq!(body["data"]["semantic_extractor"]["mode"], "rule");
    assert_eq!(body["data"]["graph_builder"]["mode"], "rule");
    assert_eq!(body["data"]["wiki_builder"]["mode"], "rule");
    assert_eq!(
        body["data"]["semantic_extractor"]["agent_tools"]["enabled"],
        false
    );
    assert_eq!(
        body["data"]["graph_builder"]["agent_tools"]["enabled"],
        false
    );
    assert_eq!(
        body["data"]["wiki_builder"]["agent_tools"]["enabled"],
        false
    );
    assert_eq!(body["data"]["layer_status"]["l2_semantic"], "warning");
    assert_eq!(body["data"]["layer_status"]["l4_graph"], "warning");
    assert_eq!(body["data"]["layer_status"]["l5_wiki_view"], "warning");

    let warnings = body["data"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(warnings
        .iter()
        .any(|issue| issue["code"] == "W-L2-LLM-CONFIG-MISSING"));
    assert!(warnings.iter().any(|issue| {
        issue["message"]
            .as_str()
            .unwrap_or_default()
            .contains("--llm")
    }));
    assert!(warnings
        .iter()
        .any(|issue| issue["code"] == "W-L4-LLM-CONFIG-MISSING"));
    assert!(warnings
        .iter()
        .any(|issue| issue["code"] == "W-L5-LLM-CONFIG-MISSING"));
}

#[test]
fn update_strict_mode_fails_on_progressive_layer_error() {
    let dir = unique_temp_dir("update-strict-progressive-error");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["update", "--strict"],
        &dir,
        &[("KB_UPDATE_SIMULATE_LAYER_FAILURES", "l2_semantic")],
    );
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E1011");
    assert_eq!(body["error"]["details"]["strict"], true);
    assert_eq!(
        body["error"]["details"]["layer_status"]["l2_semantic"],
        "error"
    );
    assert_eq!(
        body["error"]["details"]["layer_status"]["l3_projection"],
        "ok"
    );
    assert_eq!(body["error"]["details"]["layer_status"]["l4_graph"], "ok");
    assert_eq!(
        body["error"]["details"]["layer_status"]["l5_wiki_view"],
        "ok"
    );
    assert_eq!(body["error"]["details"]["l2_compilation"]["processed"], 1);
    assert_eq!(body["error"]["details"]["l2_compilation"]["reused"], 0);
    assert_eq!(body["error"]["details"]["l2_compilation"]["missing"], 0);
    assert_eq!(body["error"]["details"]["l2_compilation"]["failed"], 0);
    let errors = body["error"]["details"]["errors"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["layer"], "l2_semantic");
    let failed_layers = body["error"]["details"]["failed_layers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(failed_layers.len(), 1);
    assert_eq!(failed_layers[0], "l2_semantic");
}

#[test]
fn update_default_mode_tracks_split_progressive_layers() {
    let dir = unique_temp_dir("update-default-progressive-split-layers");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["update"],
        &dir,
        &[(
            "KB_UPDATE_SIMULATE_LAYER_FAILURES",
            "l3_projection,l4_graph,l5_wiki_view",
        )],
    );
    assert!(ok);
    assert_eq!(body["data"]["layer_status"]["l2_semantic"], "ok");
    assert_eq!(body["data"]["layer_status"]["l3_projection"], "warning");
    assert_eq!(body["data"]["layer_status"]["l4_graph"], "warning");
    assert_eq!(body["data"]["layer_status"]["l5_wiki_view"], "warning");
    assert_eq!(body["data"]["l2_compilation"]["processed"], 1);
    assert_eq!(body["data"]["l2_compilation"]["reused"], 0);
    assert_eq!(body["data"]["l2_compilation"]["missing"], 0);
    assert_eq!(body["data"]["l2_compilation"]["failed"], 0);

    let warnings = body["data"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(warnings.len(), 3);
    assert!(warnings
        .iter()
        .any(|issue| issue["layer"] == "l3_projection"));
    assert!(warnings.iter().any(|issue| issue["layer"] == "l4_graph"));
    assert!(warnings
        .iter()
        .any(|issue| issue["layer"] == "l5_wiki_view"));
}

#[test]
fn update_default_mode_keeps_legacy_combined_projection_simulation() {
    let dir = unique_temp_dir("update-default-legacy-combined-simulation");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/a.md", "A");
    let (ok, _) = run_json(&["add", "./docs/a.md", "docs/a", "--category", "raw"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["update"],
        &dir,
        &[("KB_UPDATE_SIMULATE_LAYER_FAILURES", "l3_l4_projection")],
    );
    assert!(ok);
    assert_eq!(body["data"]["layer_status"]["l3_projection"], "warning");
    assert_eq!(body["data"]["layer_status"]["l4_graph"], "warning");
    assert_eq!(body["data"]["layer_status"]["l5_wiki_view"], "ok");

    let warnings = body["data"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(warnings.len(), 2);
    assert!(warnings
        .iter()
        .any(|issue| issue["layer"] == "l3_projection"));
    assert!(warnings.iter().any(|issue| issue["layer"] == "l4_graph"));
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
fn update_preserves_crlf_when_appending_raw_declarations() {
    let dir = unique_temp_dir("update-append-raw-crlf");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/guide.md", "GUIDE");
    mk_file(&dir, "./docs/notes.md", "NOTES");
    mk_file(&dir, "./.kb/raw/new-raw.md", "RAW NEW");

    let original = "# docs:r\r\n[guide](./docs/guide.md):r\r\n# notes:rw\r\n[notes](./docs/notes.md):rw\r\n";
    fs::write(dir.join("kb.md"), original.as_bytes()).expect("write CRLF kb.md");

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    assert_eq!(
        fs::read(dir.join("kb.md")).expect("read updated kb.md"),
        b"# docs:r\r\n[guide](./docs/guide.md):r\r\n# notes:rw\r\n[notes](./docs/notes.md):rw\r\n# raw\r\n[](./.kb/raw/new-raw.md)\r\n"
    );
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

    let assert = kb_command_in(&dir)
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
    assert_eq!(body["data"]["mode"], "retrieve");
    assert!(body["data"]["retrieval_trace"].is_array());
    assert!(body["data"]["reasoning_context"].is_object());

    let retrieved = body["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context must be array");
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0]["alias"], "docs/config");
    assert!(retrieved[0]["snippet"]
        .as_str()
        .unwrap_or_default()
        .contains("Use kb config to change settings."));
    assert!(retrieved[0]["source_layer"].is_string());
    assert_eq!(retrieved[0]["record_id"], "docs/config");
    assert!(retrieved[0]["chunk_id"].is_string() || retrieved[0]["chunk_id"].is_null());
    let score = retrieved[0]["score"].as_f64().unwrap_or_default();
    assert!(score > 0.0 && score <= 1.0);
}

#[test]
fn ask_answer_requires_llm_credentials() {
    let dir = unique_temp_dir("ask-answer-needs-llm");
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

    let (ok, body) = run_json(&["ask", "hello", "--top", "1", "--answer"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E5003");
}

#[test]
fn ask_reports_configured_engine_when_llm_env_is_present() {
    let dir = unique_temp_dir("ask-configured-llm");
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

    let assert = kb_command_with_env(
        &dir,
        &[
            ("KB_LLM_KEY", "demo-key"),
            ("KB_LLM_MODEL", "demo-model"),
            ("KB_LLM_BASE_URL", "https://api.example/v1"),
            ("KB_ASK_OFFLINE_STUB", "1"),
        ],
    )
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

    let body: Value =
        serde_json::from_slice(&output.stdout).expect("json stdout for configured ask success");
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["engine_status"], "configured");
    assert_eq!(body["data"]["mode"], "retrieve");
    assert_eq!(
        body["data"]["provider"],
        Value::String("openai-compatible:demo-model@https://api.example/v1".to_string())
    );
}

#[test]
fn ask_answer_reports_tool_trace_when_llm_env_is_present() {
    let dir = unique_temp_dir("ask-answer-configured-llm");
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

    let assert = kb_command_with_env(
        &dir,
        &[
            ("KB_LLM_KEY", "demo-key"),
            ("KB_LLM_MODEL", "demo-model"),
            ("KB_LLM_BASE_URL", "https://api.example/v1"),
        ],
    )
    .arg("ask")
    .arg("hello")
    .arg("--top")
    .arg("1")
    .arg("--answer")
    .arg("--output")
    .arg("json")
    .assert();
    let output = assert.get_output();
    assert!(output.status.success());

    let body: Value =
        serde_json::from_slice(&output.stdout).expect("json stdout for configured answer");
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["mode"], "answer");
    assert_eq!(body["data"]["engine_status"], "configured");
    assert!(body["data"]["tool_trace"].is_array());
    assert!(body["data"]["agent_trace"].is_array());
    assert!(body["data"]["answer"].is_object());
}

#[test]
fn ask_answer_stub_emits_agent_trace_and_stderr_progress() {
    let dir = unique_temp_dir("ask-answer-agent-progress");
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

    let assert = kb_command_with_env(
        &dir,
        &[
            ("KB_LLM_KEY", "demo-key"),
            ("KB_LLM_MODEL", "demo-model"),
            ("KB_LLM_BASE_URL", "https://api.example/v1"),
            ("KB_ASK_OFFLINE_STUB", "1"),
        ],
    )
    .arg("ask")
    .arg("hello")
    .arg("--top")
    .arg("1")
    .arg("--answer")
    .arg("--output")
    .arg("json")
    .assert();
    let output = assert.get_output();
    assert!(output.status.success());

    let body: Value =
        serde_json::from_slice(&output.stdout).expect("json stdout for configured answer");
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["mode"], "answer");
    assert_eq!(body["data"]["agent_backend"], "offline_stub");
    assert!(
        body["data"]["tool_trace"]
            .as_array()
            .expect("tool_trace should be array")
            .iter()
            .any(|item| item["tool"] == "search_context")
    );
    assert!(
        !body["data"]["retrieved_context"]
            .as_array()
            .expect("retrieved_context should be array")
            .is_empty()
    );
    let agent_trace = body["data"]["agent_trace"]
        .as_array()
        .expect("agent_trace should be array");
    assert!(!agent_trace.is_empty());
    assert!(agent_trace.iter().any(|item| item["kind"] == "thought"));
    assert!(agent_trace.iter().any(|item| item["kind"] == "tool_call"));

    let stderr = String::from_utf8(output.stderr.clone()).expect("stderr should be utf8");
    assert!(stderr.contains("[ask-agent]"));
    assert!(stderr.contains("read_file_summary"));
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
fn ask_layered_retrieval_prioritizes_l3_l2_l4_then_l1_fallback() {
    let dir = unique_temp_dir("ask-layered-retrieval-order");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(
        &dir,
        "./docs/l1.md",
        "This line is a layered target fallback anchor for index retrieval.",
    );
    let (ok, _) = run_json(
        &["add", "./docs/l1.md", "docs/l1", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    write_json_file(
        &dir,
        "./.kb/knowledge.json",
        &json!({
            "schema_version": 1,
            "generated_at": "2026-05-02T00:00:00+08:00",
            "base_index_signature": "index:test",
            "entries": [
                {
                    "record_id": "docs/l3",
                    "path": "./virtual/l3.md",
                    "input_signature": "sig-l3",
                    "status": "active",
                    "summary": "layered target entry for L3 projection",
                    "tags": ["layered"],
                    "key_concepts": ["target"],
                    "chunks": [
                        {
                            "chunk_id": "docs/l3:0001",
                            "byte_range": [0, 64],
                            "content_signature": "sig-l3-c1",
                            "summary": "l3 chunk summary",
                            "preview": "layered target from l3 preview"
                        }
                    ],
                    "provider": {
                        "kind": "rule",
                        "name": "rule-based-v1"
                    }
                },
                {
                    "record_id": "docs/l2",
                    "path": "./virtual/l2.md",
                    "input_signature": "sig-l2",
                    "status": "active",
                    "summary": "layered target entry for L2 semantic",
                    "tags": ["semantic"],
                    "key_concepts": ["layered", "target"],
                    "chunks": [
                        {
                            "chunk_id": "docs/l2:0001",
                            "byte_range": [0, 64],
                            "content_signature": "sig-l2-c1",
                            "summary": "l2 chunk summary with layered target",
                            "preview": "layered target from l2 preview"
                        }
                    ],
                    "provider": {
                        "kind": "rule",
                        "name": "rule-based-v1"
                    }
                }
            ]
        }),
    );

    write_json_file(
        &dir,
        "./.kb/search.json",
        &json!({
            "schema_version": 1,
            "generated_at": "2026-05-02T00:00:00+08:00",
            "base_knowledge_signature": "knowledge:test",
            "hits": [
                {
                    "key": "docs/l3|docs/l3:0001|sig-l3-c1",
                    "record_id": "docs/l3",
                    "chunk_id": "docs/l3:0001",
                    "content_signature": "sig-l3-c1",
                    "score": 0.98,
                    "matched_terms": ["layered", "target"]
                }
            ]
        }),
    );

    write_json_file(
        &dir,
        "./.kb/graph.json",
        &json!({
            "schema_version": 1,
            "generated_at": "2026-05-02T00:00:00+08:00",
            "base_knowledge_signature": "knowledge:test",
            "nodes": [
                {
                    "node_id": "file:docs/l4",
                    "node_type": "file",
                    "label": "docs/l4"
                },
                {
                    "node_id": "concept:target",
                    "node_type": "concept",
                    "label": "target"
                }
            ],
            "edges": [
                {
                    "edge_id": "file_mentions_concept|file:docs/l4|concept:target",
                    "edge_type": "file_mentions_concept",
                    "from_node_id": "file:docs/l4",
                    "to_node_id": "concept:target",
                    "evidence": [
                        {
                            "record_id": "docs/l4",
                            "chunk_id": "docs/l4:0001",
                            "reason": "layered target relation"
                        }
                    ]
                }
            ]
        }),
    );

    let (ok, body) = run_json(&["ask", "layered target", "--top", "4"], &dir);
    assert!(ok);
    let retrieved = body["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context must be array");
    assert_eq!(retrieved.len(), 4);

    assert_eq!(retrieved[0]["source_layer"], "l3_projection");
    assert_eq!(retrieved[0]["record_id"], "docs/l3");
    assert_eq!(retrieved[0]["chunk_id"], "docs/l3:0001");

    assert_eq!(retrieved[1]["source_layer"], "l2_semantic");
    assert_eq!(retrieved[1]["record_id"], "docs/l2");
    assert_eq!(retrieved[1]["chunk_id"], "docs/l2:0001");

    assert_eq!(retrieved[2]["source_layer"], "l4_graph");
    assert_eq!(retrieved[2]["record_id"], "docs/l4");
    assert_eq!(retrieved[2]["chunk_id"], "docs/l4:0001");

    assert_eq!(retrieved[3]["source_layer"], "l1_index");
    assert_eq!(retrieved[3]["record_id"], "docs/l1");
    assert!(retrieved[3]["chunk_id"].is_null());
}

#[test]
fn ask_layered_retrieval_falls_back_to_l1_when_compiled_layers_missing() {
    let dir = unique_temp_dir("ask-layered-retrieval-l1-fallback");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    mk_file(&dir, "./docs/l1.md", "fallback-only anchor keyword");
    let (ok, _) = run_json(
        &["add", "./docs/l1.md", "docs/l1", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let _ = fs::remove_file(dir.join(".kb/search.json"));
    let _ = fs::remove_file(dir.join(".kb/knowledge.json"));
    let _ = fs::remove_file(dir.join(".kb/graph.json"));

    let (ok, body) = run_json(&["ask", "anchor keyword", "--top", "1"], &dir);
    assert!(ok);
    let retrieved = body["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context must be array");
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0]["source_layer"], "l1_index");
    assert_eq!(retrieved[0]["record_id"], "docs/l1");
    assert!(retrieved[0]["chunk_id"].is_null());
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
    let assert = kb_command_in(&dir)
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
fn config_flag_is_rejected_by_clap() {
    let dir = unique_temp_dir("invalid-config");
    let output = kb_command_in(&dir)
        .args(["--config", "./config.json", "init", "demo"])
        .assert()
        .code(2)
        .get_output()
        .to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument"), "expected Clap rejection: {stderr}");
    assert!(stderr.contains("--config"));
}

#[test]
fn init_invalid_global_config_preserves_original_file() {
    for raw in [
        r#"{"llm":{"key":"test-only"},broken"#,
        "[]",
        "null",
        r#"{"llm":[]}"#,
        r#"{"llm":null}"#,
        r#"{"knowledge_bases":"old-kb"}"#,
        r#"{"knowledge_bases":["old-kb",42]}"#,
    ] {
        let dir = unique_temp_dir("init-invalid-global");
        let config = dir.join("global-config.json");
        fs::write(&config, raw).unwrap();
        let (ok, body) = run_json_with_global_config(&["init"], &dir, &config);
        assert!(ok, "local initialization should succeed: {body}");
        assert!(dir.join("kb.md").is_file());
        assert_eq!(body["data"]["global_config_registered"], false, "{raw}");
        assert!(body["data"]["global_config_error"].is_string());
        assert_eq!(fs::read(&config).unwrap(), raw.as_bytes());
    }
}

#[test]
fn init_bom_global_config_preserves_settings_and_registrations() {
    let dir = unique_temp_dir("init-bom-global");
    let config = dir.join("global-config.json");
    fs::write(&config, "\u{feff}{\"knowledge_bases\":[\"old-kb\"],\"llm\":{\"model\":\"kept-model\",\"key\":\"test-only\"},\"custom\":{\"keep\":true}}").unwrap();
    let (ok, body) = run_json_with_global_config(&["init"], &dir, &config);
    assert!(ok);
    assert_eq!(body["data"]["global_config_registered"], true);
    let updated: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(updated["llm"]["model"], "kept-model");
    assert_eq!(updated["llm"]["key"], "test-only");
    assert_eq!(updated["custom"]["keep"], true);
    assert_eq!(updated["knowledge_bases"], json!(["old-kb", canonical_display_path(&dir)]));
}

#[test]
fn env_list_shows_only_existing_registered_kb_roots() {
    let global_root = unique_temp_dir("env-global-root");
    let caller = unique_temp_dir("env-caller");
    let kb_one = unique_temp_dir("env-kb-one");
    let global_config = global_root.join("config.json");

    let (ok, _) = run_json_with_global_config(&["init", "demo"], &kb_one, &global_config);
    assert!(ok, "init should register kb root into global config");
    assert!(
        global_config.exists(),
        "global config should be created by init"
    );
    let global_cfg_raw = fs::read_to_string(&global_config).expect("read global config");
    let global_cfg: Value = serde_json::from_str(&global_cfg_raw).expect("parse global config");
    assert!(global_cfg.get("llm").is_some());
    assert!(global_cfg.get("knowledge_bases").is_some());

    let kb_one_abs = canonical_display_path(&kb_one);

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

    let assert = kb_command_with_global_config(&caller, &global_config)
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

#[test]
fn env_doctor_reports_selected_global_config_path_and_source() {
    let dir = unique_temp_dir("env-doctor-global-config-selection");
    write_json_file(
        &dir,
        "./global-config.json",
        &json!({ "llm": { "model": "selected-model" } }),
    );
    let (ok, doctor) = run_json(&["env", "--doctor"], &dir);
    assert!(ok);

    let expected = dir.join("global-config.json").to_string_lossy().to_string();
    assert_eq!(doctor["data"]["global_config"]["path"], expected);
    assert_eq!(doctor["data"]["global_config"]["rule"], 1);
    assert_eq!(doctor["data"]["global_config"]["source"], "KB_GLOBAL_CONFIG");
    assert_eq!(doctor["data"]["llm"]["model"], "selected-model");
}

#[test]
fn mv_renames_alias_in_kb_md_and_pipeline_converges() {
    let dir = unique_temp_dir("mv-kbmd");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./notes.md", "# Notes\n\nsome body text\n");
    let (ok, _) = run_json(&["add", "./notes.md", "notes"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["mv", "notes", "docs"], &dir);
    assert!(ok, "mv should succeed: {body}");
    assert_eq!(body["data"]["old_alias"], "notes");
    assert_eq!(body["data"]["new_alias"], "docs");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(
        kb_md.contains("[docs](./notes.md)"),
        "kb.md declaration should carry the new alias: {kb_md}"
    );
    assert!(
        !kb_md.contains("[notes](./notes.md)"),
        "old alias must be gone from kb.md: {kb_md}"
    );

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);
    let (ok, listed) = run_json(&["list"], &dir);
    assert!(ok);
    let items = listed["data"]["items"].as_array().expect("items array");
    assert!(
        items.iter().any(|x| x["alias"] == "docs"),
        "list should surface the renamed alias after update: {listed}"
    );
    assert!(
        !items.iter().any(|x| x["alias"] == "notes"),
        "old alias must not survive update: {listed}"
    );
}

#[test]
fn mv_preserves_explicit_perm_and_comment_in_kb_md() {
    let dir = unique_temp_dir("mv-perm-comment");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./secret.md", "classified\n");
    let kb_md_path = dir.join("kb.md");
    let mut kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    kb_md.push_str("[secret](./secret.md):rw # keep this comment\n");
    fs::write(&kb_md_path, kb_md).expect("write kb.md");

    let (ok, body) = run_json(&["mv", "secret", "hidden"], &dir);
    assert!(ok, "mv of kb.md-declared alias should succeed: {body}");

    let kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    assert!(
        kb_md.contains("[hidden](./secret.md):rw # keep this comment"),
        "explicit perm and comment must survive the rename: {kb_md}"
    );
}

#[test]
fn mv_missing_alias_returns_e5002_and_conflict_returns_e2003() {
    let dir = unique_temp_dir("mv-errors");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "A\n");
    mk_file(&dir, "./b.md", "B\n");
    let (ok, _) = run_json(&["add", "./a.md", "a"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["add", "./b.md", "b"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["mv", "ghost", "phantom"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E5002");

    let (ok, body) = run_json(&["mv", "a", "b"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E2003");
}

#[test]
fn path_updates_kb_md_declaration_and_supports_query() {
    let dir = unique_temp_dir("path-kbmd");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "old home\n");
    mk_file(&dir, "./b.md", "new home\n");
    let (ok, _) = run_json(&["add", "./a.md", "doc"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["path", "doc", "./b.md"], &dir);
    assert!(ok, "path update should succeed: {body}");
    assert_eq!(body["data"]["old_path"], "./a.md");
    assert_eq!(body["data"]["new_path"], "./b.md");

    let kb_md = fs::read_to_string(dir.join("kb.md")).expect("read kb.md");
    assert!(
        kb_md.contains("[doc](./b.md)"),
        "kb.md declaration should point at the new path: {kb_md}"
    );
    assert!(!kb_md.contains("[doc](./a.md)"));

    let (ok, body) = run_json(&["path", "doc"], &dir);
    assert!(ok);
    assert_eq!(body["data"]["path"], "./b.md");

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);
    let (ok, listed) = run_json(&["list"], &dir);
    assert!(ok);
    let items = listed["data"]["items"].as_array().expect("items array");
    assert!(
        items.iter().any(|x| x["path"] == "./b.md"),
        "pipeline should converge to the new path after update: {listed}"
    );
}

#[test]
fn path_to_missing_target_returns_e5001() {
    let dir = unique_temp_dir("path-missing-target");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "A\n");
    let (ok, _) = run_json(&["add", "./a.md", "doc"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["path", "doc", "./nowhere.md"], &dir);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "E5001");
}

#[test]
fn path_rejects_unrepresentable_characters_with_e2010() {
    let dir = unique_temp_dir("path-unrepresentable");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "A\n");
    mk_file(&dir, "./b (draft).md", "B\n");
    let (ok, _) = run_json(&["add", "./a.md", "doc"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["path", "doc", "./b (draft).md"], &dir);
    assert!(!ok, "path containing ')' must be rejected: {body}");
    assert_eq!(body["error"]["code"], "E2010");

    // kb.md 必须保持可解析
    let (ok, _) = run_json(&["check"], &dir);
    assert!(ok, "kb.md must remain parseable after rejected path update");
}

#[test]
fn add_rejects_unrepresentable_path_with_e2010() {
    let dir = unique_temp_dir("add-unrepresentable");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./report (final).md", "R\n");

    let (ok, body) = run_json(&["add", "./report (final).md"], &dir);
    assert!(!ok, "add of a ')' path must be rejected: {body}");
    assert_eq!(body["error"]["code"], "E2010");
}

#[test]
fn path_refuses_to_rewrite_pattern_declarations_with_e2011() {
    let dir = unique_temp_dir("path-pattern-guard");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./docs/one.md", "1\n");
    mk_file(&dir, "./docs/two.md", "2\n");
    mk_file(&dir, "./single.md", "S\n");
    let kb_md_path = dir.join("kb.md");
    let mut kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    kb_md.push_str("[docs](./docs/**)\n");
    fs::write(&kb_md_path, kb_md).expect("write kb.md");

    let (ok, body) = run_json(&["path", "docs", "./single.md"], &dir);
    assert!(!ok, "glob declaration must not be rewritten: {body}");
    assert_eq!(body["error"]["code"], "E2011");

    let kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    assert!(kb_md.contains("[docs](./docs/**)"), "pattern must be intact");
}

#[test]
fn path_to_occupied_path_returns_e2006() {
    let dir = unique_temp_dir("path-occupied");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./x.md", "X\n");
    mk_file(&dir, "./y.md", "Y\n");
    let (ok, _) = run_json(&["add", "./x.md", "a"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["add", "./y.md", "b"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["path", "b", "./x.md"], &dir);
    assert!(!ok, "pointing b at a's path must fail: {body}");
    assert_eq!(body["error"]["code"], "E2006");
}

#[test]
fn mv_by_path_target_syncs_workset_and_reports_real_alias() {
    let dir = unique_temp_dir("mv-by-path");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "A\n");
    let (ok, _) = run_json(&["add", "./a.md", "doc"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["mv", "./a.md", "renamed"], &dir);
    assert!(ok, "mv by path target should succeed: {body}");
    assert_eq!(
        body["data"]["old_alias"], "doc",
        "output must report the real old alias, not the path target"
    );

    // 工作集必须同步：旧别名不可再被幻影改名
    let (ok, body) = run_json(&["mv", "doc", "other"], &dir);
    assert!(!ok, "stale workset alias must not be phantom-renamed: {body}");
    assert_eq!(body["error"]["code"], "E5002");
}

#[test]
fn mv_alias_missing_from_kb_md_is_not_silently_renamed() {
    let dir = unique_temp_dir("mv-stale-workset");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./a.md", "A\n");
    let (ok, _) = run_json(&["add", "./a.md", "doc"], &dir);
    assert!(ok);

    // 手工把声明从 kb.md 删除，模拟 stale 工作集
    let kb_md_path = dir.join("kb.md");
    let kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    let filtered: String = kb_md
        .lines()
        .filter(|line| !line.contains("[doc]"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(&kb_md_path, filtered).expect("write kb.md");

    let (ok, body) = run_json(&["mv", "doc", "doc2"], &dir);
    assert!(!ok, "declaration truth must win over stale workset: {body}");
    assert_eq!(body["error"]["code"], "E5002");
}

#[test]
fn add_copy_conflicting_raw_target_returns_e2006() {
    for declared in [true, false] {
        let dir = unique_temp_dir("add-copy-conflict");
        assert!(run_json(&["init"], &dir).0);
        mk_file(&dir, "./sub/x.md", "second content\n");
        if declared {
            mk_file(&dir, "./x.md", "first content\n");
            assert!(run_json(&["add", "./x.md", "first", "--mode", "copy"], &dir).0);
        } else {
            mk_file(&dir, "./.kb/raw/x.md", "first content\n");
        }
        let kb_before = fs::read(dir.join("kb.md")).unwrap();
        let refs_before = fs::read(dir.join(".kb/refs.json")).unwrap();
        let output = kb_command_in(&dir).args(["add", "./sub/x.md", "second", "--mode", "copy", "--output", "json"])
            .assert().code(2).get_output().clone();
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["error"]["code"], "E2006");
        assert_eq!(fs::read(dir.join(".kb/raw/x.md")).unwrap(), b"first content\n");
        assert_eq!(fs::read(dir.join("kb.md")).unwrap(), kb_before);
        assert_eq!(fs::read(dir.join(".kb/refs.json")).unwrap(), refs_before);
    }
}

#[test]
fn add_detects_conflicts_against_manual_kb_md_declarations() {
    let dir = unique_temp_dir("add-kbmd-conflicts");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    mk_file(&dir, "./x.md", "X\n");
    mk_file(&dir, "./y.md", "Y\n");

    // 手工在 kb.md 声明一条引用（文档宣称的主工作流），不经过 kb add
    let kb_md_path = dir.join("kb.md");
    let mut kb_md = fs::read_to_string(&kb_md_path).expect("read kb.md");
    kb_md.push_str("[dup](./x.md)\n");
    fs::write(&kb_md_path, kb_md).expect("write kb.md");

    let (ok, body) = run_json(&["add", "./y.md", "dup"], &dir);
    assert!(!ok, "alias declared manually in kb.md must conflict: {body}");
    assert_eq!(body["error"]["code"], "E2003");

    let (ok, body) = run_json(&["add", "./x.md", "other"], &dir);
    assert!(!ok, "path declared manually in kb.md must conflict: {body}");
    assert_eq!(body["error"]["code"], "E2006");
}
