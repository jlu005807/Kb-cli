use serde_json::Value;

use crate::cli::AddMode;
use crate::errors::CliError;

pub(crate) fn handle(
    path: String,
    alias: Option<String>,
    mode: AddMode,
    category: Option<String>,
) -> Result<Value, CliError> {
    super::runtime::run_add(path, alias, mode, category)
}
