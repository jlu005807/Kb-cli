use serde::Serialize;
use serde_json::{Map, Value};

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
        render_root_text(&data);
    }
}

fn render_root_text(value: &Value) {
    match value {
        Value::Object(map) => render_object(map, 0),
        Value::Array(arr) => render_array(arr, 0),
        _ => println!("{}", scalar_to_text(value)),
    }
}

fn render_object(map: &Map<String, Value>, indent: usize) {
    for (key, value) in map {
        render_field(key, value, indent);
    }
}

fn render_field(key: &str, value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                println!("{pad}{key}: {{}}")
            } else {
                println!("{pad}{key}:");
                render_object(map, indent + 2);
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                println!("{pad}{key}: []");
            } else {
                println!("{pad}{key}:");
                render_array(arr, indent + 2);
            }
        }
        _ => println!("{pad}{key}: {}", scalar_to_text(value)),
    }
}

fn render_array(arr: &[Value], indent: usize) {
    for item in arr {
        render_array_item(item, indent);
    }
}

fn render_array_item(value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(path) = map.get("path").and_then(Value::as_str) {
                    println!("{pad}- {path}");
                    return;
                }
            }
            println!("{pad}-");
            render_object(map, indent + 2);
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                println!("{pad}- []");
            } else {
                println!("{pad}-");
                render_array(arr, indent + 2);
            }
        }
        _ => println!("{pad}- {}", scalar_to_text(value)),
    }
}

fn scalar_to_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => {
            if v.is_empty() {
                "\"\"".to_string()
            } else {
                v.clone()
            }
        }
        _ => String::new(),
    }
}

pub fn print_json_success(data: Value) {
    let mut data = data;
    if let Some(obj) = data.as_object_mut() {
        obj.remove("_raw_text");
    }

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
