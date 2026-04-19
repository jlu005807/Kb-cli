use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(question: String, top: Option<u32>) -> Result<Value, CliError> {
    super::runtime::run_ask(question, top)
}
