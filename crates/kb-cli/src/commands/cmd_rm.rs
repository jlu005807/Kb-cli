use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(alias: String, force: bool) -> Result<Value, CliError> {
    super::runtime::run_rm(alias, force)
}
