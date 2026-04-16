//! Alias 值对象与校验规则。
//! 约束包括：长度、保留关键字、前导字符、空白字符、允许字符集合。

use std::fmt;

const ALIAS_MIN_LEN: usize = 1;
const ALIAS_MAX_LEN: usize = 128;

const RESERVED_ALIASES: [&str; 11] = [
    "init", "add", "mv", "rm", "path", "chmod", "list", "check", "update", "ask", "help",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias(String);

impl Alias {
    /// 解析并校验 alias。
    /// 该函数是所有 alias 输入（CLI/usecase）的统一入口，保证规则一致。
    pub fn parse(input: impl AsRef<str>) -> Result<Self, AliasError> {
        let value = input.as_ref();

        if value.is_empty() {
            return Err(AliasError::new(
                AliasErrorKind::Empty,
                "alias cannot be empty",
            ));
        }

        // 空字符串已在前面单独处理；此处只需校验上限即可。
        if value.len() > ALIAS_MAX_LEN {
            return Err(AliasError::new(
                AliasErrorKind::LengthOutOfRange {
                    min: ALIAS_MIN_LEN,
                    max: ALIAS_MAX_LEN,
                    got: value.len(),
                },
                format!(
                    "alias length must be between {} and {} characters",
                    ALIAS_MIN_LEN, ALIAS_MAX_LEN
                ),
            ));
        }

        if value.starts_with('-') {
            return Err(AliasError::new(
                AliasErrorKind::StartsWithDash,
                "alias cannot start with '-'",
            ));
        }

        if value.chars().any(char::is_whitespace) {
            return Err(AliasError::new(
                AliasErrorKind::ContainsWhitespace,
                "alias cannot contain whitespace",
            ));
        }

        if RESERVED_ALIASES.iter().any(|reserved| reserved == &value) {
            return Err(AliasError::new(
                AliasErrorKind::ReservedKeyword,
                format!("alias '{}' is reserved", value),
            ));
        }

        // 仅允许 ASCII 字母数字与 `-_. /`，其余字符都作为非法字符拒绝。
        if let Some(ch) = value
            .chars()
            .find(|ch| !ch.is_ascii_alphanumeric() && !matches!(ch, '-' | '_' | '.' | '/'))
        {
            return Err(AliasError::new(
                AliasErrorKind::InvalidCharacter { ch },
                format!(
                    "alias contains invalid character '{}': allowed [a-zA-Z0-9-_. /]",
                    ch
                ),
            ));
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasError {
    pub kind: AliasErrorKind,
    pub message: String,
}

impl AliasError {
    // 内部构造器：统一错误 kind 与 message 的绑定方式。
    fn new(kind: AliasErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AliasError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasErrorKind {
    Empty,
    LengthOutOfRange { min: usize, max: usize, got: usize },
    StartsWithDash,
    ContainsWhitespace,
    ReservedKeyword,
    InvalidCharacter { ch: char },
}

#[cfg(test)]
mod tests {
    use super::{Alias, AliasErrorKind};

    #[test]
    fn accepts_valid_alias_characters() {
        let alias = Alias::parse("docs/guide_v2.1").expect("alias should be valid");
        assert_eq!(alias.as_str(), "docs/guide_v2.1");
    }

    #[test]
    fn rejects_empty_alias() {
        let err = Alias::parse("").expect_err("empty alias must fail");
        assert_eq!(err.kind, AliasErrorKind::Empty);
    }

    #[test]
    fn rejects_alias_starting_with_dash() {
        let err = Alias::parse("-docs").expect_err("alias must fail");
        assert_eq!(err.kind, AliasErrorKind::StartsWithDash);
    }

    #[test]
    fn rejects_alias_with_whitespace() {
        let err = Alias::parse("docs guide").expect_err("alias must fail");
        assert_eq!(err.kind, AliasErrorKind::ContainsWhitespace);
    }

    #[test]
    fn rejects_reserved_alias_keyword() {
        let err = Alias::parse("init").expect_err("reserved keyword must fail");
        assert_eq!(err.kind, AliasErrorKind::ReservedKeyword);
    }

    #[test]
    fn rejects_alias_with_invalid_character() {
        let err = Alias::parse("docs@guide").expect_err("invalid character must fail");
        assert_eq!(err.kind, AliasErrorKind::InvalidCharacter { ch: '@' });
    }

    #[test]
    fn rejects_alias_longer_than_max() {
        let too_long = "a".repeat(129);
        let err = Alias::parse(too_long).expect_err("length > 128 must fail");
        assert_eq!(
            err.kind,
            AliasErrorKind::LengthOutOfRange {
                min: 1,
                max: 128,
                got: 129
            }
        );
    }
}
