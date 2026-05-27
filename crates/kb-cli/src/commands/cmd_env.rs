use serde_json::Value;

use crate::errors::CliError;

pub(crate) fn handle(list: bool, doctor: bool) -> Result<Value, CliError> {
    // doctor 优先：当用户想排查"key 是从哪里读到的"时，列表信息没有意义。
    if doctor {
        super::runtime::run_env_doctor()
    } else {
        super::runtime::run_env(list)
    }
}
