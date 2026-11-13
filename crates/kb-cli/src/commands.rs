use serde_json::Value;
use std::path::Path;

use crate::cli::{Cli, Commands};
use crate::errors::{map_core_error, CliError, CoreErrorKind};
use kb_storage_fs::{PersistenceErrorKind, SingleWriterLock, SingleWriterLockManager};

mod cmd_add;
mod cmd_ask;
mod cmd_check;
mod cmd_chmod;
mod cmd_completion;
mod cmd_consolidate;
mod cmd_correct;
mod cmd_env;
mod cmd_init;
mod cmd_list;
mod cmd_mv;
mod cmd_path;
mod cmd_remember;
mod cmd_rm;
mod cmd_serve;
mod cmd_update;
mod runtime;

pub(crate) struct WriterLockGuard {
    _lock: Option<SingleWriterLock>,
    pub unsupported: bool,
}

pub(crate) fn acquire_writer_lock(cwd: &Path) -> Result<WriterLockGuard, CliError> {
    match SingleWriterLockManager::new(cwd.join(".kb")).acquire() {
        Ok(lock) => Ok(WriterLockGuard {
            unsupported: lock.is_none(),
            _lock: lock,
        }),
        Err(err) => {
            if let PersistenceErrorKind::LockAlreadyHeld { lock_path } = &err.kind {
                let owner_path = lock_path.with_file_name("writer.lock.owner.lock");
                let pid = std::fs::read_to_string(&owner_path)
                    .or_else(|_| std::fs::read_to_string(lock_path))
                    .ok()
                    .and_then(|contents| {
                        contents.lines().find_map(|line| {
                            line.strip_prefix("pid=")?
                                .split_whitespace()
                                .next()?
                                .parse::<u32>()
                                .ok()
                        })
                    });
                return Err(map_core_error(
                    CoreErrorKind::WriterLockHeld,
                    serde_json::json!({
                        "pid": pid,
                        "lock_path": lock_path.to_string_lossy(),
                    }),
                ));
            }
            Err(map_core_error(
                CoreErrorKind::WriteConfigFailed,
                serde_json::json!({"reason": err.to_string()}),
            ))
        }
    }
}

pub fn dispatch(cli: Cli) -> Result<Value, CliError> {
    let verbose = cli.verbose;
    let requires_writer_lock = matches!(
        &cli.command,
        Commands::Add { .. }
            | Commands::Mv { .. }
            | Commands::Rm { .. }
            | Commands::Path { .. }
            | Commands::Chmod { .. }
            | Commands::Update { dry_run: false, .. }
            | Commands::Consolidate { dry_run: false, .. }
    );
    if requires_writer_lock {
        if let Ok(cwd) = std::env::current_dir() {
            if cwd.join("kb.md").is_file() && cwd.join(".kb").is_dir() {
                let writer_lock = acquire_writer_lock(&cwd)?;
                if writer_lock.unsupported && verbose > 0 {
                    eprintln!("warning: filesystem does not support writer locks; continuing without a lock");
                }
                let _writer_lock = writer_lock;
                return dispatch_unlocked(cli);
            }
        }
    }

    dispatch_unlocked(cli)
}

fn dispatch_unlocked(cli: Cli) -> Result<Value, CliError> {
    let verbose = cli.verbose;
    match cli.command {
        Commands::Init { name, force } => cmd_init::handle(name, force, verbose),
        Commands::Add {
            path,
            alias,
            mode,
            category,
        } => cmd_add::handle(path, alias, mode, category),
        Commands::Mv { alias, new_alias } => cmd_mv::handle(alias, new_alias),
        Commands::Rm { alias, force } => cmd_rm::handle(alias, force),
        Commands::Path { alias, new_path } => cmd_path::handle(alias, new_path),
        Commands::Chmod { alias, perms } => cmd_chmod::handle(alias, perms),
        Commands::List { long, tree, filter } => cmd_list::handle(long, tree, filter),
        Commands::Check {
            raw_only,
            category,
            top,
        } => cmd_check::handle(raw_only, category, top),
        Commands::Update {
            dry_run,
            prune,
            full,
            strict,
            llm,
        } => cmd_update::handle(dry_run, prune, full, strict, llm),
        Commands::Ask {
            question,
            top,
            answer,
        } => cmd_ask::handle(question, top, answer),
        Commands::Remember {
            content,
            refs,
            confidence,
        } => cmd_remember::handle(content, refs, confidence),
        Commands::Correct { content, refs } => cmd_correct::handle(content, refs),
        Commands::Consolidate { min, dry_run } => cmd_consolidate::handle(min, dry_run),
        Commands::Env { list, doctor } => cmd_env::handle(list, doctor),
        Commands::Serve { mcp } => cmd_serve::handle(mcp, verbose),
        Commands::Completion { shell } => cmd_completion::handle(shell),
    }
}
