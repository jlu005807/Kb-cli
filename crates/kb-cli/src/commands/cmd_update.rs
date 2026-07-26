use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(
    dry_run: bool,
    prune: bool,
    full: bool,
    strict: bool,
    llm: bool,
) -> Result<Value, CliError> {
    super::runtime::run_update(dry_run, prune, full, strict, llm)
}
