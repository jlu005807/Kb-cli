//! 运行时配置解析入口。
//! 目标：将 flags/env/project/global/defaults 合并为单一配置 DTO，
//! 并输出“最终来源 + 可解释 trace”用于调试。

use kb_core::config::{ConfigLayers, ConfigSource, RuntimeConfigDto};
use serde::Deserialize;
use std::path::Path;

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// 合并后的最终配置值。
    pub dto: RuntimeConfigDto,
    /// 最终生效值来自哪一层。
    pub source: ConfigSource,
    /// 合并轨迹（用于 verbose 调试展示）。
    pub trace: String,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    quiet: Option<bool>,
    verbose: Option<u8>,
    no_color: Option<bool>,
}

/// 解析并合并运行时配置。
/// 优先级：flags > env > project > global > defaults。
pub fn resolve_runtime_config(cli: &Cli, cwd: &Path) -> RuntimeConfig {
    let defaults = RuntimeConfigDto::default();

    let flags = Some(RuntimeConfigDto {
        quiet: cli.quiet,
        verbose: cli.verbose,
        no_color: cli.no_color,
    });

    let env_cfg = RuntimeConfigDto {
        quiet: read_env_bool("KB_QUIET").unwrap_or(false),
        verbose: std::env::var("KB_VERBOSE")
            .ok()
            .and_then(|x| x.parse::<u8>().ok())
            .unwrap_or(0),
        no_color: read_env_bool("KB_NO_COLOR").unwrap_or(false),
    };
    let env = if env_cfg != RuntimeConfigDto::default() {
        Some(env_cfg)
    } else {
        None
    };

    let project = load_file_config(&cwd.join(".kb/config.json"));
    let global = std::env::var("KB_GLOBAL_CONFIG")
        .ok()
        .and_then(|p| load_file_config(Path::new(&p)));

    let merged = ConfigLayers::from_layers(flags, env, project, global, defaults).merge();
    RuntimeConfig {
        dto: merged.value,
        source: merged.source,
        trace: merged.trace.render(),
    }
}

// 读取 JSON 配置文件，字段缺失回退默认值；文件不存在或解析失败返回 None。
fn load_file_config(path: &Path) -> Option<RuntimeConfigDto> {
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: FileConfig = serde_json::from_str(&raw).ok()?;
    Some(RuntimeConfigDto {
        quiet: parsed.quiet.unwrap_or(false),
        verbose: parsed.verbose.unwrap_or(0),
        no_color: parsed.no_color.unwrap_or(false),
    })
}

// 统一布尔环境变量语义，避免各处手写大小写/同义词判断。
fn read_env_bool(key: &str) -> Option<bool> {
    match std::env::var(key).ok()?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
