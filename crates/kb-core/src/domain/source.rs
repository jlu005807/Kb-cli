//! `KnowledgeSource` 描述一条 KnowledgeEntry 是从哪里来的。
//!
//! 三类来源对应不同的可信度基线和后续策略：
//! - `FromFile`：`kb update` 从声明文件编译，可信度高、可重生成
//! - `FromAgent(agent_id)`：agent 运行时从交互推断写入，需要 provenance 链
//! - `FromUser`：用户显式断言（包括 UserCorrection），优先级最高
//!
//! 检索时按 source 排序：UserCorrection > FromFile > FromAgent；
//! consolidation 时同 record_id 上 FromUser 会覆盖 FromAgent，
//! 但永远不会覆盖 FromFile（文件是 ground truth）。

use serde::{Deserialize, Serialize};

use crate::domain::AgentId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnowledgeSource {
    /// 文件编译产物，没有 agent 归属。
    #[default]
    FromFile,
    /// agent 运行时写入。携带 agent_id，便于审计与过滤。
    FromAgent { agent_id: AgentId },
    /// 用户显式断言（最高优先级）。
    FromUser,
}

impl KnowledgeSource {
    pub fn from_agent<S: Into<String>>(agent_id: S) -> Self {
        // 通过 AgentId 构造器走一遍 trim/non-empty 校验；构造失败回退 system。
        let agent_id =
            AgentId::new(agent_id).unwrap_or_else(|_| AgentId::system());
        Self::FromAgent { agent_id }
    }

    pub fn is_from_file(&self) -> bool {
        matches!(self, Self::FromFile)
    }

    pub fn is_from_user(&self) -> bool {
        matches!(self, Self::FromUser)
    }

    pub fn agent_id(&self) -> Option<&AgentId> {
        match self {
            Self::FromAgent { agent_id } => Some(agent_id),
            _ => None,
        }
    }

    /// 排序优先级（数字越大越可信）。检索 / consolidation 用。
    pub fn priority(&self) -> u8 {
        match self {
            Self::FromUser => 3,
            Self::FromFile => 2,
            Self::FromAgent { .. } => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_file_is_default() {
        assert_eq!(KnowledgeSource::default(), KnowledgeSource::FromFile);
        assert!(KnowledgeSource::default().is_from_file());
    }

    #[test]
    fn priority_ordering() {
        assert!(KnowledgeSource::FromUser.priority() > KnowledgeSource::FromFile.priority());
        assert!(
            KnowledgeSource::FromFile.priority()
                > KnowledgeSource::from_agent("cursor").priority()
        );
    }

    #[test]
    fn from_agent_carries_id() {
        let src = KnowledgeSource::from_agent("cursor");
        assert_eq!(src.agent_id().unwrap().as_str(), "cursor");
    }

    #[test]
    fn from_agent_empty_falls_back_to_system() {
        let src = KnowledgeSource::from_agent("   ");
        assert_eq!(src.agent_id().unwrap(), &AgentId::system());
    }

    #[test]
    fn serde_uses_tagged_kind() {
        let json = serde_json::to_string(&KnowledgeSource::FromFile).unwrap();
        assert_eq!(json, r#"{"kind":"from_file"}"#);

        let json = serde_json::to_string(&KnowledgeSource::FromUser).unwrap();
        assert_eq!(json, r#"{"kind":"from_user"}"#);

        let json =
            serde_json::to_string(&KnowledgeSource::from_agent("cursor")).unwrap();
        assert_eq!(json, r#"{"kind":"from_agent","agent_id":"cursor"}"#);
    }

    #[test]
    fn serde_roundtrip() {
        let cases = vec![
            KnowledgeSource::FromFile,
            KnowledgeSource::FromUser,
            KnowledgeSource::from_agent("cursor"),
        ];
        for case in cases {
            let json = serde_json::to_string(&case).unwrap();
            let back: KnowledgeSource = serde_json::from_str(&json).unwrap();
            assert_eq!(back, case);
        }
    }
}
