//! 运行时配置解析入口。
//! 目标：将 flags/env/project/global/defaults 合并为单一配置 DTO，
//! 并输出“最终来源 + 可解释 trace”用于调试。

use kb_core::config::{ConfigLayers, RuntimeConfigDto};
use serde::Deserialize;
use std::path::Path;

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// 合并后的最终配置值。
    pub dto: RuntimeConfigDto,
    /// 合并轨迹（用于 verbose 调试展示）。
    pub trace: String,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    quiet: Option<bool>,
    verbose: Option<u8>,
    no_color: Option<bool>,
}

/// 解析并合并运行时配置。
/// 优先级：flags > env > project > global > defaults。
pub fn resolve_runtime_config(cli: &Cli, cwd: &Path) -> RuntimeConfig {
    let global_path = crate::env_registry::resolve_global_config_selection().path;
    resolve_runtime_config_with_global_path(cli, cwd, &global_path)
}

fn resolve_runtime_config_with_global_path(
    cli: &Cli,
    cwd: &Path,
    global_path: &Path,
) -> RuntimeConfig {
    let flags = FileConfig {
        quiet: cli.quiet.then_some(true),
        verbose: (cli.verbose > 0).then_some(cli.verbose),
        no_color: cli.no_color.then_some(true),
    };
    let env = FileConfig {
        quiet: read_env_bool("KB_QUIET"),
        verbose: std::env::var("KB_VERBOSE")
            .ok()
            .and_then(|x| x.parse::<u8>().ok()),
        no_color: read_env_bool("KB_NO_COLOR"),
    };
    resolve_from_layers(
        flags,
        env,
        load_file_config(&cwd.join(".kb/config.json")).unwrap_or_default(),
        load_file_config(global_path).unwrap_or_default(),
    )
}

fn resolve_from_layers(
    flags: FileConfig,
    env: FileConfig,
    project: FileConfig,
    global: FileConfig,
) -> RuntimeConfig {
    let quiet =
        ConfigLayers::from_layers(flags.quiet, env.quiet, project.quiet, global.quiet, false)
            .merge();
    let verbose = ConfigLayers::from_layers(
        flags.verbose,
        env.verbose,
        project.verbose,
        global.verbose,
        0,
    )
    .merge();
    let no_color = ConfigLayers::from_layers(
        flags.no_color,
        env.no_color,
        project.no_color,
        global.no_color,
        false,
    )
    .merge();
    RuntimeConfig {
        dto: RuntimeConfigDto {
            quiet: quiet.value,
            verbose: verbose.value,
            no_color: no_color.value,
        },
        trace: format!(
            "quiet: {}\nverbose: {}\nno_color: {}",
            quiet.trace.render(),
            verbose.trace.render(),
            no_color.trace.render()
        ),
    }
}

// 保留字段缺失；文件不存在或解析失败返回 None。
fn load_file_config(path: &Path) -> Option<FileConfig> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(&raw)).ok()
}

// 统一布尔环境变量语义，避免各处手写大小写/同义词判断。
fn read_env_bool(key: &str) -> Option<bool> {
    match std::env::var(key).ok()?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{load_file_config, resolve_from_layers, FileConfig};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_config_reads_bom_global_fields_without_process_environment() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "kb-runtime-config-global-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create test directory");
        let global_path: PathBuf = directory.join("selected-global.json");
        fs::write(&global_path, "\u{feff}{\"quiet\":true}").expect("write global config");
        let resolved = resolve_from_layers(
            FileConfig::default(),
            FileConfig::default(),
            FileConfig::default(),
            load_file_config(&global_path).unwrap(),
        );

        assert!(resolved.dto.quiet);
        assert!(resolved.trace.contains("source=global decision=selected"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn runtime_layers_merge_fields_without_losing_false_or_zero() {
        for (layers, expected) in [
            (
                [
                    "{}",
                    "{}",
                    "{}",
                    r#"{"quiet":true,"verbose":2,"no_color":true}"#,
                ],
                (true, 2, true),
            ),
            (
                [
                    "{}",
                    r#"{"quiet":false,"verbose":0}"#,
                    "{}",
                    r#"{"quiet":true,"verbose":2,"no_color":true}"#,
                ],
                (false, 0, true),
            ),
            (
                [
                    r#"{"verbose":3}"#,
                    "{}",
                    r#"{"no_color":false}"#,
                    r#"{"quiet":true,"no_color":true}"#,
                ],
                (true, 3, false),
            ),
            (["{}"; 4], (false, 0, false)),
        ] {
            let [flags, env, project, global]: [FileConfig; 4] =
                layers.map(|raw| serde_json::from_str(raw).unwrap());
            let resolved = resolve_from_layers(flags, env, project, global);
            assert_eq!(
                (
                    resolved.dto.quiet,
                    resolved.dto.verbose,
                    resolved.dto.no_color
                ),
                expected
            );
        }
    }
}
