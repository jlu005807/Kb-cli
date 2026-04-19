use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(list: bool) -> Result<Value, CliError> {
    super::runtime::run_env(list)
}
