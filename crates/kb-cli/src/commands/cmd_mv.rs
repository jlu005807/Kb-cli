use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(alias: String, new_alias: String) -> Result<Value, CliError> {
    super::runtime::run_mv(alias, new_alias)
}
