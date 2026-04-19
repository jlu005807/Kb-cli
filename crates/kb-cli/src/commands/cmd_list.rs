use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(long: bool, tree: bool, filter: Option<String>) -> Result<Value, CliError> {
    super::runtime::run_list(long, tree, filter)
}
