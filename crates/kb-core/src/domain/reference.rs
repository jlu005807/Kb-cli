//! Ref 领域实体：关联 alias/path/permission 三元组。
//! 该对象用于表达“知识库条目引用关系”的最小业务单元。

use super::{Alias, Permission};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    alias: Alias,
    path: String,
    permission: Permission,
}

impl Ref {
    /// 构造引用实体。
    /// 目前仅校验 path 非空；alias 与 permission 由各自值对象保证合法。
    pub fn new(
        alias: Alias,
        path: impl AsRef<str>,
        permission: Permission,
    ) -> Result<Self, RefError> {
        let path = path.as_ref();
        if path.trim().is_empty() {
            return Err(RefError::new("path cannot be empty"));
        }

        Ok(Self {
            alias,
            path: path.to_string(),
            permission,
        })
    }

    pub fn alias(&self) -> &Alias {
        &self.alias
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn permission(&self) -> &Permission {
        &self.permission
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefError {
    pub message: String,
}

impl RefError {
    // 内部构造器：统一 Ref 校验失败错误格式。
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RefError {}

#[cfg(test)]
mod tests {
    use crate::domain::{Alias, Permission, Ref};

    #[test]
    fn creates_ref_with_valid_components() {
        let alias = Alias::parse("docs/guide").expect("alias should parse");
        let perms = Permission::parse("r").expect("permission should parse");

        let reference = Ref::new(alias.clone(), "./docs/guide.md", perms.clone())
            .expect("reference should be valid");

        assert_eq!(reference.alias(), &alias);
        assert_eq!(reference.path(), "./docs/guide.md");
        assert_eq!(reference.permission(), &perms);
    }

    #[test]
    fn rejects_empty_path() {
        let alias = Alias::parse("docs/guide").expect("alias should parse");
        let perms = Permission::parse("r").expect("permission should parse");

        let err = Ref::new(alias, "   ", perms).expect_err("empty path should fail");
        assert_eq!(err.message, "path cannot be empty");
    }
}
