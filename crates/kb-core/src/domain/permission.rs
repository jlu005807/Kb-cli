//! Permission 值对象与解释器。
//! 负责：
//! - 权限串解析（如 `rwx`、`-`）
//! - 规范化输出（canonical）
//! - 权限判定与可解释 trace

use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PermissionAction {
    Read,
    Write,
    Execute,
    Delete,
    Configure,
}

impl PermissionAction {
    pub const fn code(self) -> char {
        match self {
            Self::Read => 'r',
            Self::Write => 'w',
            Self::Execute => 'x',
            Self::Delete => 'd',
            Self::Configure => 'c',
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Execute => "execute",
            Self::Delete => "delete",
            Self::Configure => "configure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    actions: BTreeSet<PermissionAction>,
    explicit_none: bool,
    canonical: String,
}

impl Permission {
    /// 解析权限字符串并生成规范化 Permission。
    /// 规则：
    /// - `-` 表示显式无权限，且必须独立出现
    /// - 其余字符仅允许 `rwxdc`
    /// - 不允许重复字符
    pub fn parse(input: impl AsRef<str>) -> Result<Self, PermissionParseError> {
        let raw = input.as_ref();

        if raw.is_empty() {
            return Err(PermissionParseError::new(
                PermissionParseErrorKind::Empty,
                "permission string cannot be empty",
            ));
        }

        if raw == "-" {
            return Ok(Self {
                actions: BTreeSet::new(),
                explicit_none: true,
                canonical: "-".to_string(),
            });
        }

        if raw.contains('-') {
            return Err(PermissionParseError::new(
                PermissionParseErrorKind::DashMustBeStandalone,
                "'-' must be used alone",
            ));
        }

        let mut actions = BTreeSet::new();
        for ch in raw.chars() {
            let action = match ch {
                'r' => PermissionAction::Read,
                'w' => PermissionAction::Write,
                'x' => PermissionAction::Execute,
                'd' => PermissionAction::Delete,
                'c' => PermissionAction::Configure,
                _ => {
                    return Err(PermissionParseError::new(
                        PermissionParseErrorKind::UnknownCharacter { ch },
                        format!("unknown permission character '{}'", ch),
                    ))
                }
            };

            if !actions.insert(action) {
                return Err(PermissionParseError::new(
                    PermissionParseErrorKind::DuplicateCharacter { ch },
                    format!("duplicate permission character '{}'", ch),
                ));
            }
        }

        let canonical = canonicalize(&actions);
        Ok(Self {
            actions,
            explicit_none: false,
            canonical,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn allows(&self, action: PermissionAction) -> bool {
        if self.explicit_none {
            return false;
        }
        self.actions.contains(&action)
    }

    pub fn actions(&self) -> impl Iterator<Item = PermissionAction> + '_ {
        self.actions.iter().copied()
    }

    pub fn is_explicit_none(&self) -> bool {
        self.explicit_none
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn canonicalize(actions: &BTreeSet<PermissionAction>) -> String {
    // 固定顺序输出，确保同一权限集合的字符串表示稳定可比较。
    let mut out = String::new();
    for action in [
        PermissionAction::Read,
        PermissionAction::Write,
        PermissionAction::Execute,
        PermissionAction::Delete,
        PermissionAction::Configure,
    ] {
        if actions.contains(&action) {
            out.push(action.code());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionInterpreter;

impl PermissionInterpreter {
    /// 仅返回布尔判定结果，适合运行态快速检查。
    pub fn evaluate(permission: &Permission, action: PermissionAction) -> bool {
        permission.allows(action)
    }

    /// 返回判定结果和解释步骤，便于调试与审计。
    pub fn evaluate_with_trace(
        permission: &Permission,
        action: PermissionAction,
    ) -> PermissionExplainTrace {
        let allowed = Self::evaluate(permission, action);
        let mut steps = Vec::new();

        steps.push(format!(
            "permission={} action={}",
            permission.as_str(),
            action.name()
        ));

        if permission.is_explicit_none() {
            steps.push("decision=denied reason=explicit-none(-)".to_string());
        } else if allowed {
            steps.push(format!(
                "decision=allowed reason=action-present({})",
                action.code()
            ));
        } else {
            steps.push(format!(
                "decision=denied reason=action-missing({})",
                action.code()
            ));
        }

        PermissionExplainTrace {
            action,
            allowed,
            steps,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionExplainTrace {
    pub action: PermissionAction,
    pub allowed: bool,
    steps: Vec<String>,
}

impl PermissionExplainTrace {
    pub fn steps(&self) -> &[String] {
        &self.steps
    }

    pub fn render(&self) -> String {
        self.steps.join("\n")
    }

    pub fn contains_fragment(&self, fragment: &str) -> bool {
        self.steps.iter().any(|step| step.contains(fragment))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionParseError {
    pub kind: PermissionParseErrorKind,
    pub message: String,
}

impl PermissionParseError {
    // 内部构造器：统一解析错误的 kind/message 生成。
    fn new(kind: PermissionParseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for PermissionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PermissionParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionParseErrorKind {
    Empty,
    UnknownCharacter { ch: char },
    DuplicateCharacter { ch: char },
    DashMustBeStandalone,
}

#[cfg(test)]
mod tests {
    use super::{Permission, PermissionAction, PermissionInterpreter, PermissionParseErrorKind};

    #[test]
    fn parses_valid_permission_and_canonicalizes_order() {
        let perms = Permission::parse("xwr").expect("valid permission should parse");
        assert_eq!(perms.as_str(), "rwx");
        assert!(perms.allows(PermissionAction::Read));
        assert!(perms.allows(PermissionAction::Write));
        assert!(perms.allows(PermissionAction::Execute));
    }

    #[test]
    fn rejects_empty_permission() {
        let err = Permission::parse("").expect_err("empty permission must fail");
        assert_eq!(err.kind, PermissionParseErrorKind::Empty);
    }

    #[test]
    fn rejects_unknown_character() {
        let err = Permission::parse("rv").expect_err("unknown permission must fail");
        assert_eq!(
            err.kind,
            PermissionParseErrorKind::UnknownCharacter { ch: 'v' }
        );
    }

    #[test]
    fn rejects_duplicate_character() {
        let err = Permission::parse("rr").expect_err("duplicate permission must fail");
        assert_eq!(
            err.kind,
            PermissionParseErrorKind::DuplicateCharacter { ch: 'r' }
        );
    }

    #[test]
    fn rejects_dash_when_mixed_with_other_permissions() {
        let err = Permission::parse("r-").expect_err("mixed '-' must fail");
        assert_eq!(err.kind, PermissionParseErrorKind::DashMustBeStandalone);
    }

    #[test]
    fn explicit_none_denies_everything() {
        let perms = Permission::parse("-").expect("'-' must parse");
        for action in [
            PermissionAction::Read,
            PermissionAction::Write,
            PermissionAction::Execute,
            PermissionAction::Delete,
            PermissionAction::Configure,
        ] {
            assert!(!perms.allows(action));
        }
    }

    #[test]
    fn explain_trace_contains_decision_reason() {
        let perms = Permission::parse("r").expect("permission should parse");
        let trace = PermissionInterpreter::evaluate_with_trace(&perms, PermissionAction::Write);
        assert!(!trace.allowed);
        assert!(trace.contains_fragment("decision=denied"));
        assert!(trace.contains_fragment("reason=action-missing(w)"));
    }
}
