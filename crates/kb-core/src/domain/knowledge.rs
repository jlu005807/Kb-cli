use serde::{Deserialize, Serialize};

use crate::domain::{KnowledgeSource, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticProviderInfo {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Active,
    Missing,
    Failed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub chunk_id: String,
    pub byte_range: [u64; 2],
    pub content_signature: String,
    pub summary: String,
    pub preview: String,
}

/// 语义记忆条目（L2 层产物 + agent runtime 写入）。
///
/// Phase 0 schema bump：在原有字段基础上加入 confidence / source / timestamps /
/// provenance，用于支撑 agent memory 场景。所有新增字段都带 `#[serde(default)]`，
/// 旧 `knowledge.json`（schema_version=2 之前）反序列化时会得到对应默认值，
/// 调用方据此可判断"这条条目是不是从老存储升级上来的"——例如 `created_at == 0`
/// 表示历史数据没有时间锚，写回时再补齐。
///
/// 注：因为 `confidence: f32` 不实现 `Eq`，本类型从 Phase 0 起只 derive `PartialEq`；
/// 上游测试断言用 `assert_eq!` 仍然可用（基于 `PartialEq`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub record_id: String,
    pub path: String,
    pub input_signature: String,
    pub status: KnowledgeStatus,
    pub summary: String,
    pub tags: Vec<String>,
    pub key_concepts: Vec<String>,
    pub chunks: Vec<KnowledgeChunk>,
    pub provider: SemanticProviderInfo,

    /// 0.0-1.0 的可信度评分。检索时可作权重，consolidation 时可做 decay。
    /// 默认 1.0 —— 文件编译产物天然 ground truth。
    #[serde(default = "default_confidence")]
    pub confidence: f32,

    /// 这条 entry 来自哪里（文件 / agent / 用户）。默认 FromFile。
    #[serde(default)]
    pub source: KnowledgeSource,

    /// 首次写入时间（unix nanos）。从旧 JSON 升级时为 0，重新编译会刷新。
    #[serde(default)]
    pub created_at: Timestamp,

    /// 上次被 `verify_citation` 或 `kb update` 验证通过的时间。
    /// `None` 表示从未验证过——retrieval 时可降权或主动触发验证。
    #[serde(default)]
    pub last_verified_at: Option<Timestamp>,

    /// 此条 entry 关联的 Episode id 列表（L0 层）。从文件编译来的 entry
    /// 这个字段为空；agent runtime 写入会带上原始 episode 引用，
    /// 用于反查"这条事实是从哪几次交互固化来的"。
    #[serde(default)]
    pub provenance: Vec<String>,
}

fn default_confidence() -> f32 {
    1.0
}

impl Default for KnowledgeEntry {
    /// 最小占位 entry：仅用于测试 / `..Default::default()` 填充。
    /// 业务路径请用 [`Self::from_file`]，会带上正确的 now 时间戳。
    fn default() -> Self {
        Self {
            record_id: String::new(),
            path: String::new(),
            input_signature: String::new(),
            status: KnowledgeStatus::Active,
            summary: String::new(),
            tags: Vec::new(),
            key_concepts: Vec::new(),
            chunks: Vec::new(),
            provider: SemanticProviderInfo {
                kind: String::new(),
                name: String::new(),
            },
            confidence: 1.0,
            source: KnowledgeSource::FromFile,
            created_at: Timestamp::ZERO,
            last_verified_at: None,
            provenance: Vec::new(),
        }
    }
}

impl KnowledgeEntry {
    /// 构造一个从文件编译来的 entry。沿用 Phase 0 之前的语义：
    /// confidence=1.0、source=FromFile、created_at=now、provenance 为空。
    ///
    /// 这是给现有 `compile_l2` / storage 测试构造 entry 的便捷入口——
    /// 业务调用者继续填基础字段（record_id / path / status / chunks / ...），
    /// 不必关心新加的记忆字段。需要写入 agent / user 来源时改用
    /// [`Self::with_source`]。
    // 9 个参数是历史签名，测试与 compile_l2 大量依赖；引入 builder 属于
    // 破坏性重构，收益不抵成本，这里定点豁免 clippy::too_many_arguments。
    #[allow(clippy::too_many_arguments)]
    pub fn from_file(
        record_id: String,
        path: String,
        input_signature: String,
        status: KnowledgeStatus,
        summary: String,
        tags: Vec<String>,
        key_concepts: Vec<String>,
        chunks: Vec<KnowledgeChunk>,
        provider: SemanticProviderInfo,
    ) -> Self {
        let now = Timestamp::now();
        Self {
            record_id,
            path,
            input_signature,
            status,
            summary,
            tags,
            key_concepts,
            chunks,
            provider,
            confidence: 1.0,
            source: KnowledgeSource::FromFile,
            created_at: now,
            last_verified_at: Some(now),
            provenance: Vec::new(),
        }
    }

    /// 链式覆盖 source（用于 agent runtime 写入 / 用户断言）。
    /// 同时根据 source 类型调整默认 confidence：
    /// - FromUser → 1.0（显式断言可信度满）
    /// - FromAgent → 0.6（需要后续 verification 提升）
    /// - FromFile  → 1.0（保持文件 ground truth）
    pub fn with_source(mut self, source: KnowledgeSource) -> Self {
        self.confidence = match &source {
            KnowledgeSource::FromUser => 1.0,
            KnowledgeSource::FromAgent { .. } => 0.6,
            KnowledgeSource::FromFile => 1.0,
        };
        self.source = source;
        self
    }

    pub fn with_provenance(mut self, episode_ids: Vec<String>) -> Self {
        self.provenance = episode_ids;
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// 标记本次验证通过（写入 last_verified_at）。
    pub fn mark_verified(&mut self, at: Timestamp) {
        self.last_verified_at = Some(at);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub base_index_signature: String,
    pub entries: Vec<KnowledgeEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> KnowledgeEntry {
        KnowledgeEntry::from_file(
            "docs/a".to_string(),
            "./docs/a.md".to_string(),
            "sig".to_string(),
            KnowledgeStatus::Active,
            "summary".to_string(),
            vec!["t".to_string()],
            vec!["k".to_string()],
            vec![],
            SemanticProviderInfo {
                kind: "rule".to_string(),
                name: "default".to_string(),
            },
        )
    }

    #[test]
    fn from_file_sets_file_source_and_confidence() {
        let entry = sample_entry();
        assert_eq!(entry.source, KnowledgeSource::FromFile);
        assert_eq!(entry.confidence, 1.0);
        assert!(entry.provenance.is_empty());
        assert!(entry.created_at.as_nanos() > 0);
        assert!(entry.last_verified_at.is_some());
    }

    #[test]
    fn with_source_adjusts_default_confidence() {
        let entry = sample_entry().with_source(KnowledgeSource::from_agent("cursor"));
        assert_eq!(entry.confidence, 0.6);
        assert_eq!(
            entry.source.agent_id().map(|id| id.as_str()),
            Some("cursor")
        );

        let entry = sample_entry().with_source(KnowledgeSource::FromUser);
        assert_eq!(entry.confidence, 1.0);
        assert!(entry.source.is_from_user());
    }

    #[test]
    fn confidence_is_clamped() {
        let entry = sample_entry().with_confidence(2.5);
        assert_eq!(entry.confidence, 1.0);
        let entry = sample_entry().with_confidence(-0.4);
        assert_eq!(entry.confidence, 0.0);
    }

    #[test]
    fn provenance_set_via_builder() {
        let entry = sample_entry().with_provenance(vec!["ep-1".to_string(), "ep-2".to_string()]);
        assert_eq!(entry.provenance, vec!["ep-1", "ep-2"]);
    }

    /// 关键兼容性测试：旧 JSON（没有 confidence/source/created_at/...）
    /// 必须能反序列化成默认值，不能因为 schema bump 把老 knowledge.json 整坏。
    #[test]
    fn deserialize_legacy_entry_uses_defaults() {
        let legacy_json = r#"{
            "record_id": "docs/a",
            "path": "./docs/a.md",
            "input_signature": "sig",
            "status": "active",
            "summary": "old",
            "tags": [],
            "key_concepts": [],
            "chunks": [],
            "provider": { "kind": "rule", "name": "default" }
        }"#;
        let entry: KnowledgeEntry = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(entry.confidence, 1.0);
        assert_eq!(entry.source, KnowledgeSource::FromFile);
        assert_eq!(entry.created_at, Timestamp::ZERO);
        assert_eq!(entry.last_verified_at, None);
        assert!(entry.provenance.is_empty());
    }

    /// 新 JSON roundtrip：写出新字段、读回来字段保留。
    #[test]
    fn new_entry_roundtrip_preserves_fields() {
        let entry = sample_entry()
            .with_source(KnowledgeSource::from_agent("cursor"))
            .with_provenance(vec!["ep-1".to_string()])
            .with_confidence(0.7);
        let json = serde_json::to_string(&entry).unwrap();
        let back: KnowledgeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn mark_verified_updates_timestamp() {
        let mut entry = sample_entry();
        let later = Timestamp(entry.created_at.as_nanos() + 1_000_000);
        entry.mark_verified(later);
        assert_eq!(entry.last_verified_at, Some(later));
    }
}
