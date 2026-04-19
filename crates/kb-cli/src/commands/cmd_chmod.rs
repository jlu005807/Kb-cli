use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(alias: String, perms: String) -> Result<Value, CliError> {
    super::runtime::run_chmod(alias, perms)
}
