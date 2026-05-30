use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(
    content: String,
    refs: Vec<String>,
    confidence: Option<f32>,
) -> Result<Value, CliError> {
    super::runtime::run_remember(content, refs, confidence)
}
