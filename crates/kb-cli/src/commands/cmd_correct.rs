use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(content: String, refs: Vec<String>) -> Result<Value, CliError> {
    super::runtime::run_correct(content, refs)
}
