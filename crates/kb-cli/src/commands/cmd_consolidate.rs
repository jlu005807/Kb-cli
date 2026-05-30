use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(min: usize, dry_run: bool) -> Result<Value, CliError> {
    super::runtime::run_consolidate(min, dry_run)
}
