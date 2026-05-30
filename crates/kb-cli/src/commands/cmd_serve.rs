use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(mcp: bool) -> Result<Value, CliError> {
    super::runtime::run_serve(mcp)
}
