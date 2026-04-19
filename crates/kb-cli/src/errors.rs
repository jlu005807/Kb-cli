//! CLI 错误模型与错误码映射。
//! `CoreErrorKind` 表示语义错误类型，`map_core_error` 负责映射为稳定的
//! `E-code + message + suggestion + exit_code`，供 text/json 输出层复用。

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreErrorKind {
    AlreadyInitialized,
    InvalidGlobPattern,
    NotInitialized,
    InvalidKbFormat,
    WriteConfigFailed,
    ConfigNotFound,
    Interrupted,
    AliasNotFound,
    AliasInvalid,
    AliasConflict,
    PathMissing,
    PathAlreadyRegistered,
    PathForbidden,
    InvalidPermission,
    CategoryNotFound,
    NoRelevantContext,
    PersistIndexFailed,
}

#[derive(Debug)]
pub struct CliError {
    /// 稳定错误码（例如 `E3001`），用于脚本与文档契约。
    pub code: &'static str,
    /// 人类可读错误消息。
    pub message: &'static str,
    /// 可选修复建议。
    pub suggestion: Option<&'static str>,
    /// 结构化上下文细节（JSON）。
    pub details: Value,
    /// 进程退出码。
    pub exit_code: u8,
}

/// 将核心错误种类映射为 CLI 对外错误协议。
/// 注意：此处属于稳定契约层，修改需同步文档与测试。
pub fn map_core_error(kind: CoreErrorKind, details: Value) -> CliError {
    match kind {
        CoreErrorKind::NotInitialized => CliError {
            code: "E3001",
            message: "Knowledge base is not initialized",
            suggestion: Some("Run `kb init` in current directory"),
            details,
            exit_code: 3,
        },
        CoreErrorKind::AlreadyInitialized => CliError {
            code: "E3002",
            message: "Knowledge base already initialized",
            suggestion: Some("Use --force to overwrite or run in a different directory"),
            details,
            exit_code: 3,
        },
        CoreErrorKind::InvalidGlobPattern => CliError {
            code: "E2002",
            message: "Invalid glob pattern",
            suggestion: Some("Verify the filter string is a valid glob pattern"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::InvalidKbFormat => CliError {
            code: "E3003",
            message: "Invalid kb.md format",
            suggestion: Some("Fix kb.md syntax and run kb check again"),
            details,
            exit_code: 3,
        },
        CoreErrorKind::WriteConfigFailed => CliError {
            code: "E1001",
            message: "Failed to write configuration",
            suggestion: Some("Check file permissions and disk space"),
            details,
            exit_code: 1,
        },
        CoreErrorKind::ConfigNotFound => CliError {
            code: "E2008",
            message: "Configuration file not found",
            suggestion: Some("Pass a valid --config path or remove the flag"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::Interrupted => CliError {
            code: "E9001",
            message: "Operation interrupted",
            suggestion: Some("Retry the command when ready"),
            details,
            exit_code: 130,
        },
        CoreErrorKind::AliasNotFound => CliError {
            code: "E5002",
            message: "Alias does not exist",
            suggestion: Some("Run 'kb list' to see available aliases"),
            details,
            exit_code: 5,
        },
        CoreErrorKind::AliasInvalid => CliError {
            code: "E2001",
            message: "Alias contains invalid characters",
            suggestion: Some("Use only letters, digits, -, _, ., / and length 1-128"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::AliasConflict => CliError {
            code: "E2003",
            message: "Alias conflict",
            suggestion: Some("Choose another alias or cancel"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::PathMissing => CliError {
            code: "E5001",
            message: "Path does not exist",
            suggestion: Some("Verify the file or directory exists at the provided path"),
            details,
            exit_code: 5,
        },
        CoreErrorKind::PathAlreadyRegistered => CliError {
            code: "E2006",
            message: "Path already registered",
            suggestion: Some("Use a different path or remove existing registration first"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::PathForbidden => CliError {
            code: "E2009",
            message: "Path is protected and cannot be indexed",
            suggestion: Some(
                "Use normal content paths or ./.kb/raw/*, avoid kb.md and .kb config files",
            ),
            details,
            exit_code: 2,
        },
        CoreErrorKind::InvalidPermission => CliError {
            code: "E2004",
            message: "Invalid permission string",
            suggestion: Some("Use a valid combination of r, w, x, d, c, or -"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::CategoryNotFound => CliError {
            code: "E2007",
            message: "Category not found",
            suggestion: Some("Run 'kb list --tree' to view available categories and try again"),
            details,
            exit_code: 2,
        },
        CoreErrorKind::NoRelevantContext => CliError {
            code: "E5003",
            message: "No relevant context found",
            suggestion: Some("Try a different question or add more files to the knowledge base"),
            details,
            exit_code: 5,
        },
        CoreErrorKind::PersistIndexFailed => CliError {
            code: "E1003",
            message: "Failed to persist index files",
            suggestion: Some("Check write permissions for .kb directory and retry"),
            details,
            exit_code: 1,
        },
    }
}
