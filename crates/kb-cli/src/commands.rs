use serde_json::Value;

use crate::cli::{Cli, Commands};
use crate::errors::CliError;

mod cmd_add;
mod cmd_ask;
mod cmd_check;
mod cmd_chmod;
mod cmd_completion;
mod cmd_env;
mod cmd_init;
mod cmd_list;
mod cmd_mv;
mod cmd_path;
mod cmd_rm;
mod cmd_update;
mod runtime;

pub fn dispatch(cli: Cli) -> Result<Value, CliError> {
    match cli.command {
        Commands::Init { name, force } => cmd_init::handle(name, force),
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
            yes,
            full,
        } => cmd_update::handle(dry_run, prune, yes, full),
        Commands::Ask { question, top } => cmd_ask::handle(question, top),
        Commands::Env { list } => cmd_env::handle(list),
        Commands::Completion { shell } => cmd_completion::handle(shell),
    }
}
