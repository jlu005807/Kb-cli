mod cli;
mod commands;
mod core_adapters;
mod env_registry;
mod errors;
mod output;
mod paths;
mod runtime_config;
mod state;

use clap::Parser;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&interrupted);
    // 仅记录中断标记，不在 signal handler 中执行复杂逻辑。
    let _ = ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::SeqCst);
    });

    let cli = Cli::parse();
    // 提前校验显式传入的配置路径，避免进入命令执行后才报错。
    if let Some(config_path) = cli.config.as_ref() {
        if !std::path::Path::new(config_path).exists() {
            let err = map_core_error(
                CoreErrorKind::ConfigNotFound,
                serde_json::json!({"path": config_path}),
            );
            render_error(cli.output, &err);
            return ExitCode::from(err.exit_code);
        }
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let resolved = resolve_runtime_config(&cli, &cwd);

    // debug trace 只在 verbose 且非 quiet 时输出，避免污染标准输出契约。
    if resolved.dto.verbose > 0 && !resolved.dto.quiet {
        eprintln!(
            "verbose={} no_color={} config={} source={}\ntrace:\n{}",
            resolved.dto.verbose,
            resolved.dto.no_color,
            cli.config.as_deref().unwrap_or("<none>"),
            resolved.source,
            resolved.trace
        );
    }

    let output = cli.output;
    let quiet = resolved.dto.quiet;
    let result = dispatch(cli);
    if interrupted.load(Ordering::SeqCst) {
        let err = map_core_error(CoreErrorKind::Interrupted, serde_json::json!({}));
        render_error(output, &err);
        return ExitCode::from(err.exit_code);
    }
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
