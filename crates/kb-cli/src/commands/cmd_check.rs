use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(
    raw_only: bool,
    category: Option<String>,
    top: Option<u32>,
) -> Result<Value, CliError> {
    super::runtime::run_check(raw_only, category, top)
}
