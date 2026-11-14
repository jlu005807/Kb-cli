use assert_cmd::Command;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};

static COMMAND_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const NON_LLM_ENV_VARS: &[&str] = &[
    "KB_AGENT_ID",
    "KB_SESSION_ID",
    "KB_ASK_OFFLINE_STUB",
    "KB_UPDATE_SIMULATE_LAYER_FAILURES",
    "KB_QUIET",
    "KB_VERBOSE",
    "KB_NO_COLOR",
];

pub fn kb_command() -> Command {
    let current_dir = next_test_dir();
    build_command(
        Some(&current_dir),
        current_dir.join("global-config.json"),
        &[],
    )
}

pub fn kb_command_in(current_dir: &Path) -> Command {
    build_command(
        Some(current_dir),
        current_dir.join("global-config.json"),
        &[],
    )
}

pub fn kb_command_with_env(current_dir: &Path, envs: &[(&str, &str)]) -> Command {
    build_command(
        Some(current_dir),
        current_dir.join("global-config.json"),
        envs,
    )
}

pub fn kb_command_with_global_config(current_dir: &Path, global_config: &Path) -> Command {
    build_command(Some(current_dir), global_config.to_path_buf(), &[])
}

pub fn kb_process_command(current_dir: &Path, envs: &[(&str, &str)]) -> ProcessCommand {
    let mut command = ProcessCommand::new(assert_cmd::cargo::cargo_bin("kb-cli"));
    command.current_dir(current_dir);
    apply_process_environment(&mut command, current_dir.join("global-config.json"), envs);
    command
}

fn build_command(
    current_dir: Option<&Path>,
    global_config: std::path::PathBuf,
    envs: &[(&str, &str)],
) -> Command {
    let mut command = Command::cargo_bin("kb-cli").expect("kb-cli binary should build");
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    apply_isolated_environment(&mut command, global_config, envs);
    command
}

fn apply_isolated_environment(
    command: &mut Command,
    global_config: impl AsRef<std::ffi::OsStr>,
    envs: &[(&str, &str)],
) {
    command.env("KB_GLOBAL_CONFIG", global_config);
    for key in inherited_isolation_vars() {
        command.env_remove(key);
    }
    command.env("KB_LLM_DISABLE_KEYRING", "1");
    for (key, value) in envs {
        command.env(key, value);
    }
}

fn apply_process_environment(
    command: &mut ProcessCommand,
    global_config: impl AsRef<std::ffi::OsStr>,
    envs: &[(&str, &str)],
) {
    command.env("KB_GLOBAL_CONFIG", global_config);
    for key in inherited_isolation_vars() {
        command.env_remove(key);
    }
    command.env("KB_LLM_DISABLE_KEYRING", "1");
    for (key, value) in envs {
        command.env(key, value);
    }
}

fn inherited_isolation_vars() -> Vec<OsString> {
    let mut keys = std::env::vars_os()
        .filter_map(|(key, _)| {
            key.to_str()
                .filter(|name| name.starts_with("KB_LLM_"))
                .map(OsString::from)
        })
        .collect::<Vec<_>>();
    keys.extend(NON_LLM_ENV_VARS.iter().map(OsString::from));
    keys
}

fn next_test_dir() -> std::path::PathBuf {
    let sequence = COMMAND_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("kb_cli_test_{}_{}", std::process::id(), sequence));
    std::fs::create_dir_all(&dir).expect("test directory should be created");
    dir
}
