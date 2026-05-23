//! 领域错误类型。
//!
//! `DomainError` 是 `kb-core` ports 与 usecases 的统一错误形态，
//! 取代以前的 `Result<_, String>`。
//!
//! 设计原则：
//! - 按语义分类（io/serialization/not_found/conflict/validation/config/external/internal），
//!   让上层能做"重试 / 终止 / 用户提示"分流。
//! - 不强依赖 `serde_json` / `std::io::Error`，避免把适配层细节侵入 core。
//!   适配器在自己一侧把内部错误格式化为字符串并归类到对应变体。
//! - 提供 `From<String> / From<&str>` 以便迁移期与遗留 `String` 错误平滑过渡。
//!   新代码应优先使用语义构造器（`io / config / not_found ...`）。

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// 文件系统、网络等 I/O 失败。
    #[error("io: {0}")]
    Io(String),

    /// JSON / 配置 / 自定义格式的序列化或解析失败。
    #[error("serialization: {0}")]
    Serialization(String),

    /// 目标对象不存在（alias / path / record 等）。
    #[error("not found: {0}")]
    NotFound(String),

    /// 状态冲突（alias 冲突、path 重复登记等）。
    #[error("conflict: {0}")]
    Conflict(String),

    /// 输入或领域规则校验失败。
    #[error("validation: {0}")]
    Validation(String),

    /// 配置缺失或非法。
    #[error("config: {0}")]
    Config(String),

    /// 外部依赖失败（LLM provider、远端服务等）。
    #[error("external: {0}")]
    External(String),

    /// 不期望的内部状态（应视为 bug）。
    #[error("internal: {0}")]
    Internal(String),
}

impl DomainError {
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }

    pub fn serialization(msg: impl Into<String>) -> Self {
        Self::Serialization(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn external(msg: impl Into<String>) -> Self {
        Self::External(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// 稳定的小写分类标签，便于 CLI 错误映射或 telemetry。
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Serialization(_) => "serialization",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Validation(_) => "validation",
            Self::Config(_) => "config",
            Self::External(_) => "external",
            Self::Internal(_) => "internal",
        }
    }

    /// 是否可能因为瞬时因素重试后成功（仅作建议，不强约束）。
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Io(_) | Self::External(_))
    }
}

/// 兼容历史 `Result<_, String>` 调用点。新代码应避免依赖该转换。
impl From<String> for DomainError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}

impl From<&str> for DomainError {
    fn from(value: &str) -> Self {
        Self::Internal(value.to_string())
    }
}

pub type DomainResult<T> = std::result::Result<T, DomainError>;

#[cfg(test)]
mod tests {
    use super::DomainError;

    #[test]
    fn kind_labels_are_stable() {
        assert_eq!(DomainError::io("x").kind(), "io");
        assert_eq!(DomainError::serialization("x").kind(), "serialization");
        assert_eq!(DomainError::not_found("x").kind(), "not_found");
        assert_eq!(DomainError::conflict("x").kind(), "conflict");
        assert_eq!(DomainError::validation("x").kind(), "validation");
        assert_eq!(DomainError::config("x").kind(), "config");
        assert_eq!(DomainError::external("x").kind(), "external");
        assert_eq!(DomainError::internal("x").kind(), "internal");
    }

    #[test]
    fn transient_classification() {
        assert!(DomainError::io("x").is_transient());
        assert!(DomainError::external("x").is_transient());
        assert!(!DomainError::validation("x").is_transient());
        assert!(!DomainError::config("x").is_transient());
    }

    #[test]
    fn from_string_falls_back_to_internal() {
        let err: DomainError = String::from("boom").into();
        assert_eq!(err.kind(), "internal");
        assert_eq!(err.to_string(), "internal: boom");
    }
}
