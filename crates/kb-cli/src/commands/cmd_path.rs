use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(alias: String, new_path: Option<String>) -> Result<Value, CliError> {
    super::runtime::run_path(alias, new_path)
}
