//! L0 Episodic 层：agent 与知识库交互的原始时间线。
//!
//! Episode 是整个记忆系统的"事实来源"（source of truth for provenance）。
//! 与 L2 `KnowledgeEntry` 的关系：
//! - Episode 是**原始、append-only、永不覆写**的交互记录（谁、何时、做了什么）
//! - KnowledgeEntry 是从一批 Episode **提炼固化**出来的语义事实，
//!   其 `provenance: Vec<EpisodeId>` 指回这些原始 Episode
//!
//! 设计取舍（对齐前沿 agent memory 研究，但保持轻量）：
//! - append-only：对应 Zep/Graphiti 的"观测即记录、永不丢失"，矛盾事实靠后续
//!   consolidation 解决而不是覆盖删除
//! - `agent_id` 归属：多 agent 共享同一项目知识库时写入可审计、可过滤
//! - `session_id`：把同一轮对话/任务的 episodes 串起来
//! - `confidence`：agent 对自己这条观察的自评可信度，consolidation 时参与加权

use serde::{Deserialize, Serialize};

use crate::domain::{AgentId, Timestamp};

/// Episode 的种类。决定 consolidation 时的处理优先级与语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeKind {
    /// agent 发起了一次检索 / 提问。
    Query,
    /// agent 产出了一次回答（通常带 citations 的 chunk 引用）。
    Answer,
    /// agent 调用了某个工具。
    ToolCall,
    /// agent 主动记录的一条观察 / 推断（remember）。
    Observation,
    /// 用户显式纠错。consolidation 中优先级最高——用户说错就是错。
    UserCorrection,
}

impl EpisodeKind {
    /// consolidation 优先级（数字越大越优先固化、越能覆盖低优先级事实）。
    pub fn priority(&self) -> u8 {
        match self {
            Self::UserCorrection => 4,
            Self::Observation => 3,
            Self::Answer => 2,
            Self::Query => 1,
            Self::ToolCall => 1,
        }
    }

    /// 是否是"主动写入型"事实（值得进入 consolidation 候选）。
    /// Query / ToolCall 是过程痕迹，本身不构成可固化的事实。
    pub fn is_assertive(&self) -> bool {
        matches!(self, Self::Observation | Self::UserCorrection | Self::Answer)
    }
}

/// 一条 Episode：append-only 时间线上的一个原子记录。
///
/// `id` 由调用方生成（建议内容寻址或 UUID），核心层不假设具体格式，
/// 只要求在同一个 EpisodeStore 内唯一。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// 全局唯一 id。
    pub id: String,
    /// 写入时间（unix nanos）。
    pub timestamp: Timestamp,
    /// 写入者归属。文件编译等系统路径用 `AgentId::system()`。
    pub agent_id: AgentId,
    /// 会话 / 任务上下文 id。无会话语境时可为空串。
    pub session_id: String,
    /// 种类。
    pub kind: EpisodeKind,
    /// 原始内容，不做压缩。
    pub content: String,
    /// 引用的 chunk_id 列表（指向 L1/L2 的稳定内容地址）。
    #[serde(default)]
    pub refs: Vec<String>,
    /// agent 对这条记录的自评可信度 [0.0, 1.0]。默认 1.0。
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    1.0
}

impl Episode {
    /// 构造一条带当前时间戳的 Episode。`refs` / `confidence` 走默认，
    /// 之后用链式 builder 覆盖。
    pub fn new(
        id: impl Into<String>,
        agent_id: AgentId,
        session_id: impl Into<String>,
        kind: EpisodeKind,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            timestamp: Timestamp::now(),
            agent_id,
            session_id: session_id.into(),
            kind,
            content: content.into(),
            refs: Vec::new(),
            confidence: 1.0,
        }
    }

    pub fn with_refs(mut self, refs: Vec<String>) -> Self {
        self.refs = refs;
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// 查询 Episode 时间线的过滤条件。所有字段都是可选叠加（AND 语义）。
/// 留空（`Default`）表示"全部返回"。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EpisodeFilter {
    /// 只看某个 agent 写入的 episode。
    pub agent_id: Option<AgentId>,
    /// 只看某个 session 的 episode。
    pub session_id: Option<String>,
    /// 只看某些种类。空 = 不限种类。
    pub kinds: Vec<EpisodeKind>,
    /// 只看该时间点（含）之后的 episode。
    pub since: Option<Timestamp>,
    /// 最多返回多少条（按时间倒序取最近的 N 条）。None = 不限。
    pub limit: Option<usize>,
}

impl EpisodeFilter {
    /// 判断一条 episode 是否匹配本过滤器（`limit` 不在此判断，由 store 层应用）。
    pub fn matches(&self, ep: &Episode) -> bool {
        if let Some(agent) = &self.agent_id {
            if &ep.agent_id != agent {
                return false;
            }
        }
        if let Some(session) = &self.session_id {
            if &ep.session_id != session {
                return false;
            }
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&ep.kind) {
            return false;
        }
        if let Some(since) = self.since {
            if ep.timestamp < since {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: EpisodeKind, agent: &str, session: &str, ts: u64) -> Episode {
        Episode::new(
            format!("ep-{ts}"),
            AgentId::new(agent).unwrap(),
            session,
            kind,
            "content",
        )
        .with_timestamp(Timestamp(ts))
    }

    #[test]
    fn kind_priority_user_correction_highest() {
        assert!(EpisodeKind::UserCorrection.priority() > EpisodeKind::Observation.priority());
        assert!(EpisodeKind::Observation.priority() > EpisodeKind::Query.priority());
    }

    #[test]
    fn assertive_kinds_exclude_process_traces() {
        assert!(EpisodeKind::Observation.is_assertive());
        assert!(EpisodeKind::UserCorrection.is_assertive());
        assert!(!EpisodeKind::Query.is_assertive());
        assert!(!EpisodeKind::ToolCall.is_assertive());
    }

    #[test]
    fn builder_clamps_confidence() {
        let ep = sample(EpisodeKind::Observation, "a", "s", 1).with_confidence(2.0);
        assert_eq!(ep.confidence, 1.0);
        let ep = sample(EpisodeKind::Observation, "a", "s", 1).with_confidence(-1.0);
        assert_eq!(ep.confidence, 0.0);
    }

    #[test]
    fn filter_default_matches_everything() {
        let f = EpisodeFilter::default();
        assert!(f.matches(&sample(EpisodeKind::Query, "a", "s", 1)));
        assert!(f.matches(&sample(EpisodeKind::UserCorrection, "b", "t", 99)));
    }

    #[test]
    fn filter_by_agent() {
        let f = EpisodeFilter {
            agent_id: Some(AgentId::new("cursor").unwrap()),
            ..Default::default()
        };
        assert!(f.matches(&sample(EpisodeKind::Query, "cursor", "s", 1)));
        assert!(!f.matches(&sample(EpisodeKind::Query, "claude", "s", 1)));
    }

    #[test]
    fn filter_by_kind_and_since() {
        let f = EpisodeFilter {
            kinds: vec![EpisodeKind::Observation, EpisodeKind::UserCorrection],
            since: Some(Timestamp(10)),
            ..Default::default()
        };
        // 种类匹配 + 时间够新
        assert!(f.matches(&sample(EpisodeKind::Observation, "a", "s", 15)));
        // 种类匹配但太旧
        assert!(!f.matches(&sample(EpisodeKind::Observation, "a", "s", 5)));
        // 时间够新但种类不符
        assert!(!f.matches(&sample(EpisodeKind::Query, "a", "s", 15)));
    }

    #[test]
    fn serde_roundtrip_preserves_fields() {
        let ep = sample(EpisodeKind::Answer, "cursor", "sess-1", 42)
            .with_refs(vec!["docs/a:0001".to_string()])
            .with_confidence(0.8);
        let json = serde_json::to_string(&ep).unwrap();
        let back: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);
    }

    #[test]
    fn legacy_episode_without_refs_or_confidence_uses_defaults() {
        let legacy = r#"{
            "id": "ep-1",
            "timestamp": 100,
            "agent_id": "cursor",
            "session_id": "s",
            "kind": "query",
            "content": "hi"
        }"#;
        let ep: Episode = serde_json::from_str(legacy).unwrap();
        assert!(ep.refs.is_empty());
        assert_eq!(ep.confidence, 1.0);
    }

    #[test]
    fn episode_kind_wire_format_is_snake_case() {
        let json = serde_json::to_string(&EpisodeKind::UserCorrection).unwrap();
        assert_eq!(json, r#""user_correction""#);
    }
}
