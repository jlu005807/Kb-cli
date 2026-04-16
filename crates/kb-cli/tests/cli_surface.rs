use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn help_displays_full_command_surface() {
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(
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
                .and(contains("completion")),
        );
}

#[test]
fn help_includes_required_global_flags() {
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(
            contains("--output")
                .and(contains("--quiet"))
                .and(contains("--verbose"))
                .and(contains("--config"))
                .and(contains("--no-color")),
        );
}

#[test]
fn add_help_contains_mode_and_category() {
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(contains("--mode").and(contains("--category")));
}

#[test]
fn update_help_contains_full_flag() {
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(contains("--full"));
}

#[test]
fn check_help_contains_category_filter_and_top() {
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
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
    Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .args(["completion", "--help"])
        .assert()
        .success()
        .stdout(contains("SHELL").and(contains("bash")).and(contains("zsh")));
}
