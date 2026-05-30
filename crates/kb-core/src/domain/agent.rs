//! `AgentId` 标识"哪个 agent 写入了这条记忆"。
//!
//! 多 agent 共享同一个项目知识库时，每条 Episode / agent 产生的 KnowledgeEntry
//! 都带 AgentId 归属，方便：
//! - `recall(filter_by_agent=Some("cursor"))` 只看某 agent 的记忆
//! - 多 agent 写入冲突时按 agent 优先级判定
//! - 审计每个 agent 的写入轨迹
//!
//! AgentId 是宽松字符串（允许 `kb-cli` / `cursor` / `claude-desktop` / 用户自定义），
//! 不限制具体格式，但去掉首尾空白并禁止空串。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(pub String);

impl AgentId {
    /// 当 episode / entry 没有显式 agent 归属时使用——例如 `kb update` 从文件
    /// 编译出的 KnowledgeEntry。
    pub const SYSTEM: &'static str = "system";

    pub fn system() -> Self {
        Self(Self::SYSTEM.to_string())
    }

    pub fn new<S: Into<String>>(value: S) -> Result<Self, AgentIdError> {
        let trimmed = value.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(AgentIdError::Empty);
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_system(&self) -> bool {
        self.0 == Self::SYSTEM
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::system()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentIdError {
    Empty,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_is_default() {
        assert_eq!(AgentId::default(), AgentId::system());
        assert!(AgentId::default().is_system());
    }

    #[test]
    fn trims_whitespace() {
        let id = AgentId::new("  cursor  ").unwrap();
        assert_eq!(id.as_str(), "cursor");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(AgentId::new(""), Err(AgentIdError::Empty));
        assert_eq!(AgentId::new("   "), Err(AgentIdError::Empty));
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        let id = AgentId::new("claude-desktop").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"claude-desktop\"");
        let back: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
