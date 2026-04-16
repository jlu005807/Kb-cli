use serde::Serialize;
use serde_json::Value;

use crate::errors::CliError;

const SCHEMA_VERSION: &str = "1.0";

// 输出层只负责协议封装，不参与业务逻辑判断。
// 目标：保证 text/json 两种输出契约稳定。
pub fn print_text_success(data: Value) {
    if let Some(raw) = data
        .as_object()
        .and_then(|obj| obj.get("_raw_text"))
        .and_then(Value::as_str)
    {
        print!("{}", raw);
    } else {
        println!("{}", data);
    }
}

pub fn print_json_success(data: Value) {
    let envelope = SuccessEnvelope {
        schema_version: SCHEMA_VERSION,
        status: "success",
        data,
    };
    print_pretty_json(&envelope);
}

pub fn print_json_error(err: &CliError) {
    let envelope = ErrorEnvelope {
        schema_version: SCHEMA_VERSION,
        error: ErrorPayload {
            code: err.code,
            message: err.message,
            suggestion: err.suggestion,
            details: err.details.clone(),
        },
    };
    print_pretty_json(&envelope);
}

fn print_pretty_json<T: Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("json serialization")
    );
}

#[derive(Debug, Serialize)]
struct SuccessEnvelope {
    schema_version: &'static str,
    status: &'static str,
    data: Value,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    schema_version: &'static str,
    error: ErrorPayload,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: &'static str,
    suggestion: Option<&'static str>,
    details: Value,
}
