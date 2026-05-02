use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(
    dry_run: bool,
    prune: bool,
    yes: bool,
    full: bool,
    strict: bool,
) -> Result<Value, CliError> {
    super::runtime::run_update(dry_run, prune, yes, full, strict)
}
