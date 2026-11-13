mod cli;
mod commands;
mod core_adapters;
mod env_registry;
mod errors;
mod kb_md;
mod llm_config;
mod output;
mod paths;
mod registry;
mod runtime_config;
mod state;
mod storage_backend;

use clap::Parser;
use std::process::ExitCode;

use crate::cli::Cli;
use crate::commands::dispatch;
use crate::errors::{map_core_error, CoreErrorKind};
use crate::output::{print_json_error, print_json_success, print_text_success};
use crate::runtime_config::resolve_runtime_config;

// text 模式下的人类可读错误渲染。
fn print_text_error(err: &crate::errors::CliError) {
    eprintln!("{}: {}", err.code, err.message);
    if let Some(suggestion) = err.suggestion {
        eprintln!("hint: {}", suggestion);
    }
}

fn render_error(output: cli::OutputFormat, err: &crate::errors::CliError) {
    match output {
        cli::OutputFormat::Json => print_json_error(err),
        cli::OutputFormat::Text => print_text_error(err),
    }
}

// 入口层职责：
// 1) 解析参数与运行时配置
// 2) 调用命令分发
// 3) 将结果统一渲染为 text/json 协议
fn main() -> ExitCode {
    let cli = Cli::parse();
    let output = cli.output;
    let _ = ctrlc::set_handler(move || {
        let error = map_core_error(CoreErrorKind::Interrupted, serde_json::json!({}));
        match output {
            cli::OutputFormat::Json => {
                let envelope = serde_json::json!({
                    "schema_version": "1.0",
                    "error": {
                        "code": error.code,
                        "message": error.message,
                        "suggestion": error.suggestion,
                        "details": error.details,
                    }
                });
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&envelope).unwrap_or_default()
                );
            }
            cli::OutputFormat::Text => print_text_error(&error),
        }
        std::process::exit(130);
    });

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let resolved = resolve_runtime_config(&cli, &cwd);

    // debug trace 只在 verbose 且非 quiet 时输出，避免污染标准输出契约。
    if resolved.dto.verbose > 0 && !resolved.dto.quiet {
        eprintln!(
            "verbose={} no_color={} source={}\ntrace:\n{}",
            resolved.dto.verbose,
            resolved.dto.no_color,
            resolved.source,
            resolved.trace
        );
    }

    let output = cli.output;
    let quiet = resolved.dto.quiet;
    let result = dispatch(cli);
    // 统一在入口层封装输出协议，命令层只关心业务数据本身。
    match result {
        Ok(data) => match output {
            cli::OutputFormat::Json => {
                print_json_success(data);
                ExitCode::from(0)
            }
            cli::OutputFormat::Text => {
                if !quiet {
                    print_text_success(data);
                }
                ExitCode::from(0)
            }
        },
        Err(err) => {
            render_error(output, &err);
            ExitCode::from(err.exit_code)
        }
    }
}
