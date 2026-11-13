#[allow(dead_code)]
mod common;

use common::kb_command;
use insta::assert_snapshot;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn help_displays_full_command_surface() {
    kb_command().arg("--help").assert().success().stdout(
        contains("init")
            .and(contains("add"))
            .and(contains("mv"))
            .and(contains("rm"))
            .and(contains("path"))
            .and(contains("chmod"))
            .and(contains("list"))
            .and(contains("check"))
            .and(contains("update"))
            .and(contains("ask"))
            .and(contains("env"))
            .and(contains("completion")),
    );
}

#[test]
fn help_includes_required_global_flags() {
    kb_command().arg("--help").assert().success().stdout(
        contains("--output")
            .and(contains("--quiet"))
            .and(contains("--verbose"))
            .and(contains("--no-color")),
    );
}

#[test]
fn root_help_does_not_advertise_config_flag() {
    let assert = kb_command().arg("--help").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(!stdout.contains("--config"));
}

#[test]
fn add_help_contains_mode_and_category() {
    kb_command()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(contains("--mode").and(contains("--category")));
}

#[test]
fn update_help_contains_full_and_strict_flags() {
    kb_command()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(
            contains("--full")
                .and(contains("--strict"))
                .and(contains("--llm")),
        );
}

#[test]
fn check_help_contains_category_filter_and_top() {
    kb_command()
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(
            contains("--category")
                .and(contains("--top"))
                .and(contains("--raw-only")),
        );
}

#[test]
fn completion_help_contains_shell_argument() {
    kb_command()
        .args(["completion", "--help"])
        .assert()
        .success()
        .stdout(contains("SHELL").and(contains("bash")).and(contains("zsh")));
}

#[test]
fn env_help_contains_list_flag() {
    kb_command()
        .args(["env", "--help"])
        .assert()
        .success()
        .stdout(contains("--list"));
}

#[test]
fn ask_help_contains_answer_flag() {
    kb_command()
        .args(["ask", "--help"])
        .assert()
        .success()
        .stdout(contains("--answer"));
}

#[test]
fn root_help_matches_full_snapshot() {
    let assert = kb_command().arg("--help").assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("help utf8");
    let normalized = stdout.replace("\r\n", "\n");
    assert_snapshot!("kb_root_help_full", normalized);
}
