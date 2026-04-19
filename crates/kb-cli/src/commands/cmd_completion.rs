use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(shell: clap_complete::Shell) -> Result<Value, CliError> {
    super::runtime::run_completion(shell)
}
