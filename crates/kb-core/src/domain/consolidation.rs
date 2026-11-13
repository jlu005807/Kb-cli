//! L2 固化（consolidation）：把 L0 Episode 时间线提炼成可检索的语义记忆。
//!
//! 这是 agent memory "沉淀"的核心环节：agent 在交互中积累的 Observation /
//! UserCorrection / Answer，定期被聚类提炼成 `.kb/agent-memory/*.md`，再走
//! `kb update` 进入 L1-L5——从而获得稳定 `chunk_id` 与可被 `verify_citation`
//! 校验的能力。生成的文件面向 agent 检索，不要求人类可读。
//!
//! 本模块只做**纯逻辑**（输入 `&[Episode]`，输出 `ConsolidationPlan`），完全
//! 不碰 IO。实际写文件 / `kb add` / `kb update` 由 CLI handler 负责。这样
//! 聚类与提炼规则可以被单元测试穷尽覆盖。
//!
//! 规则路径（无 LLM）的"提炼" = 按 refs 的 record_id 聚类 + 按 kind 优先级
//! 排序 + 结构化拼接。LLM 路径以后只需替换 `build_memory` 里的正文生成步骤，
//! 聚类与 provenance 逻辑不变。
//!
//! 对齐前沿 agent memory 研究的取舍：
//! - **冲突可见而非覆盖**（Zep/Graphiti）：同一 topic 下 UserCorrection 排在
//!   最前并打标，但不删除被纠正的 Observation——矛盾留痕，由读取方按优先级裁决
//! - **provenance 链**（反幻觉）：每条产物回指其来源 episode id
//! - **可信度加权**：按 kind 优先级加权，UserCorrection 权重最高

use std::collections::BTreeMap;

use crate::domain::{Episode, EpisodeKind, Timestamp};

const GENERAL_TOPIC: &str = "general";

/// 一条固化产物：从一组相关 Episode 提炼出的语义记忆。
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidatedMemory {
    /// 主题键。取自 refs 的 record_id；无 refs 的 episode 归入 `"general"`。
    pub topic: String,
    /// 提炼出的正文（规则路径 = 结构化 markdown，agent 可读可检索）。
    pub body: String,
    /// 参与提炼的 episode id（provenance 链，按展示顺序）。
    pub source_episode_ids: Vec<String>,
    /// 贡献过的 agent id（去重，按首次出现顺序）。
    pub contributor_agents: Vec<String>,
    /// 关联的 chunk_id（去重，按首次出现顺序）。
    pub refs: Vec<String>,
    /// 加权可信度 [0,1]：按 kind 优先级加权平均。
    pub confidence: f32,
    /// 是否包含用户纠错（最高可信信号）。
    pub has_user_correction: bool,
    /// 生成时间。
    pub generated_at: Timestamp,
}

impl ConsolidatedMemory {
    /// 把 topic 转成安全的文件名主干：非字母数字字符统一替换为 `-`，
    /// 折叠连续 `-`，去掉首尾 `-`。空结果回退到 `"general"`。
    /// 例如 `"docs/a"` -> `"docs-a"`，`"src/api.rs:0001"` -> `"src-api-rs-0001"`。
    pub fn file_stem(&self) -> String {
        let mut out = String::with_capacity(self.topic.len());
        let mut prev_dash = false;
        for ch in self.topic.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
        let trimmed = out.trim_matches('-').to_string();
        if trimmed.is_empty() {
            GENERAL_TOPIC.to_string()
        } else {
            trimmed
        }
    }
}

/// 固化计划：一次 consolidate 的全部产物 + 统计。
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationPlan {
    /// 达到阈值、已提炼的记忆产物（按 topic 字典序）。
    pub memories: Vec<ConsolidatedMemory>,
    /// 考察的 episode 总数。
    pub considered: usize,
    /// 因非 assertive（Query / ToolCall）被跳过的 episode 数。
    pub skipped_non_assertive: usize,
    /// 因未达每主题最小阈值而未生成产物的主题数。
    pub skipped_below_threshold: usize,
}

/// 从 chunk id 提取 record id；只移除末尾非空的 ASCII 数字序号。
pub fn record_id_of_chunk(chunk_id: &str) -> &str {
    match chunk_id.rsplit_once(':') {
        Some((record_id, sequence))
            if !sequence.is_empty() && sequence.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            record_id
        }
        _ => chunk_id,
    }
}

/// 把一批 episode 提炼成固化计划。
///
/// - 只考虑 assertive episode（Observation / UserCorrection / Answer）；
///   Query / ToolCall 是过程痕迹，计入 `skipped_non_assertive`。
/// - 按 refs 的 record_id 聚类；一条引用多个 record 的 episode 会进入多个 topic。
/// - 无 refs 的 assertive episode 归入 `"general"`。
/// - 每个 topic 至少要积累 `min_episodes_per_topic`（下限 1）条才生成产物。
pub fn plan_consolidation(
    episodes: &[Episode],
    min_episodes_per_topic: usize,
    now: Timestamp,
) -> ConsolidationPlan {
    let min = min_episodes_per_topic.max(1);
    let mut considered = 0usize;
    let mut skipped_non_assertive = 0usize;

    // topic -> 该 topic 下的 assertive episodes。BTreeMap 保证 topic 字典序稳定。
    let mut buckets: BTreeMap<String, Vec<&Episode>> = BTreeMap::new();

    for ep in episodes {
        considered += 1;
        if !ep.kind.is_assertive() {
            skipped_non_assertive += 1;
            continue;
        }

        let mut topics: Vec<String> = Vec::new();
        for chunk_id in &ep.refs {
            let rid = record_id_of_chunk(chunk_id).to_string();
            if !rid.is_empty() && !topics.contains(&rid) {
                topics.push(rid);
            }
        }
        if topics.is_empty() {
            topics.push(GENERAL_TOPIC.to_string());
        }
        for topic in topics {
            buckets.entry(topic).or_default().push(ep);
        }
    }

    let mut memories = Vec::new();
    let mut skipped_below_threshold = 0usize;

    for (topic, eps) in buckets {
        if eps.len() < min {
            skipped_below_threshold += 1;
            continue;
        }
        memories.push(build_memory(&topic, &eps, now));
    }

    ConsolidationPlan {
        memories,
        considered,
        skipped_non_assertive,
        skipped_below_threshold,
    }
}

fn build_memory(topic: &str, eps: &[&Episode], now: Timestamp) -> ConsolidatedMemory {
    // 按 kind 优先级 desc、再 timestamp desc 排序：UserCorrection 最前、最新优先。
    let mut sorted: Vec<&Episode> = eps.to_vec();
    sorted.sort_by(|a, b| {
        b.kind
            .priority()
            .cmp(&a.kind.priority())
            .then(b.timestamp.cmp(&a.timestamp))
    });

    let has_user_correction = sorted
        .iter()
        .any(|e| e.kind == EpisodeKind::UserCorrection);

    let mut body = String::new();
    body.push_str(&format!("# memory: {topic}\n\n"));
    if has_user_correction {
        body.push_str("> contains user correction (authoritative)\n\n");
    }
    body.push_str("## observations\n\n");
    for ep in &sorted {
        body.push_str(&format!(
            "- [{}] (agent={}, confidence={:.2}) {}\n",
            kind_label(&ep.kind),
            ep.agent_id.as_str(),
            ep.confidence,
            ep.content.trim()
        ));
    }

    let source_episode_ids: Vec<String> = sorted.iter().map(|e| e.id.clone()).collect();

    let mut contributor_agents: Vec<String> = Vec::new();
    for ep in &sorted {
        let agent = ep.agent_id.as_str().to_string();
        if !contributor_agents.contains(&agent) {
            contributor_agents.push(agent);
        }
    }

    let mut refs: Vec<String> = Vec::new();
    for ep in &sorted {
        for chunk_id in &ep.refs {
            if !refs.contains(chunk_id) {
                refs.push(chunk_id.clone());
            }
        }
    }

    ConsolidatedMemory {
        topic: topic.to_string(),
        body,
        source_episode_ids,
        contributor_agents,
        refs,
        confidence: weighted_confidence(&sorted),
        has_user_correction,
        generated_at: now,
    }
}

/// 按 kind 优先级加权平均 confidence。UserCorrection 权重最高（priority=4）。
fn weighted_confidence(eps: &[&Episode]) -> f32 {
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for ep in eps {
        let weight = ep.kind.priority() as f32;
        num += ep.confidence * weight;
        den += weight;
    }
    if den == 0.0 {
        0.0
    } else {
        (num / den).clamp(0.0, 1.0)
    }
}

fn kind_label(kind: &EpisodeKind) -> &'static str {
    match kind {
        EpisodeKind::Query => "query",
        EpisodeKind::Answer => "answer",
        EpisodeKind::ToolCall => "tool",
        EpisodeKind::Observation => "observation",
        EpisodeKind::UserCorrection => "correction",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AgentId;

    fn ep(id: &str, kind: EpisodeKind, agent: &str, ts: u64, refs: &[&str], conf: f32) -> Episode {
        Episode::new(
            id,
            AgentId::new(agent).unwrap(),
            "sess",
            kind,
            format!("content of {id}"),
        )
        .with_timestamp(Timestamp(ts))
        .with_refs(refs.iter().map(|s| s.to_string()).collect())
        .with_confidence(conf)
    }

    #[test]
    fn empty_input_yields_empty_plan() {
        let plan = plan_consolidation(&[], 1, Timestamp(1));
        assert!(plan.memories.is_empty());
        assert_eq!(plan.considered, 0);
    }

    #[test]
    fn non_assertive_episodes_are_skipped() {
        let eps = vec![
            ep("q1", EpisodeKind::Query, "a", 1, &[], 1.0),
            ep("t1", EpisodeKind::ToolCall, "a", 2, &[], 1.0),
        ];
        let plan = plan_consolidation(&eps, 1, Timestamp(9));
        assert_eq!(plan.considered, 2);
        assert_eq!(plan.skipped_non_assertive, 2);
        assert!(plan.memories.is_empty());
    }

    #[test]
    fn groups_by_record_id_from_refs() {
        let eps = vec![
            ep("o1", EpisodeKind::Observation, "a", 1, &["docs/a:0001"], 0.8),
            ep("o2", EpisodeKind::Observation, "b", 2, &["docs/a:0002"], 0.6),
        ];
        let plan = plan_consolidation(&eps, 2, Timestamp(9));
        assert_eq!(plan.memories.len(), 1);
        let mem = &plan.memories[0];
        assert_eq!(mem.topic, "docs/a");
        assert_eq!(mem.source_episode_ids.len(), 2);
        assert_eq!(mem.contributor_agents, vec!["b", "a"]); // sorted by ts desc
        assert_eq!(mem.refs, vec!["docs/a:0002", "docs/a:0001"]);
    }

    #[test]
    fn consolidation_extracts_record_ids_only_from_numeric_chunk_suffixes() {
        for (chunk_id, expected_topic) in [
            ("docs/a:0001", "docs/a"),
            ("D:/notes/a.md:0002", "D:/notes/a.md"),
            ("docs/a", "docs/a"),
            ("a:b", "a:b"),
            ("x:", "x:"),
        ] {
            let episode = ep(
                "o1",
                EpisodeKind::Observation,
                "a",
                1,
                &[chunk_id],
                0.8,
            );
            let plan = plan_consolidation(&[episode], 1, Timestamp(9));
            assert_eq!(
                plan.memories[0].topic, expected_topic,
                "unexpected topic for chunk id {chunk_id}"
            );
        }
    }

    #[test]
    fn below_threshold_topics_are_skipped() {
        let eps = vec![ep("o1", EpisodeKind::Observation, "a", 1, &["docs/a:0001"], 1.0)];
        let plan = plan_consolidation(&eps, 2, Timestamp(9));
        assert!(plan.memories.is_empty());
        assert_eq!(plan.skipped_below_threshold, 1);
    }

    #[test]
    fn refless_episodes_go_to_general_topic() {
        let eps = vec![
            ep("o1", EpisodeKind::Observation, "a", 1, &[], 1.0),
            ep("o2", EpisodeKind::Observation, "a", 2, &[], 1.0),
        ];
        let plan = plan_consolidation(&eps, 1, Timestamp(9));
        assert_eq!(plan.memories.len(), 1);
        assert_eq!(plan.memories[0].topic, "general");
    }

    #[test]
    fn user_correction_is_flagged_and_sorted_first() {
        let eps = vec![
            ep("o1", EpisodeKind::Observation, "a", 1, &["docs/a:0001"], 0.5),
            ep("c1", EpisodeKind::UserCorrection, "user", 2, &["docs/a:0001"], 1.0),
        ];
        let plan = plan_consolidation(&eps, 1, Timestamp(9));
        let mem = &plan.memories[0];
        assert!(mem.has_user_correction);
        // correction (priority 4) sorts before observation (priority 3)
        assert_eq!(mem.source_episode_ids[0], "c1");
        assert!(mem.body.contains("user correction (authoritative)"));
    }

    #[test]
    fn confidence_is_priority_weighted() {
        // correction(conf=1.0, w=4) + observation(conf=0.0, w=3) -> 4.0/7 ~= 0.571
        let eps = vec![
            ep("c1", EpisodeKind::UserCorrection, "user", 2, &["docs/a:0001"], 1.0),
            ep("o1", EpisodeKind::Observation, "a", 1, &["docs/a:0001"], 0.0),
        ];
        let plan = plan_consolidation(&eps, 1, Timestamp(9));
        let c = plan.memories[0].confidence;
        assert!((c - (4.0 / 7.0)).abs() < 1e-5, "got {c}");
    }

    #[test]
    fn episode_referencing_multiple_records_joins_each_topic() {
        let eps = vec![
            ep("o1", EpisodeKind::Observation, "a", 1, &["docs/a:0001", "docs/b:0001"], 1.0),
            ep("o2", EpisodeKind::Observation, "a", 2, &["docs/a:0002"], 1.0),
            ep("o3", EpisodeKind::Observation, "a", 3, &["docs/b:0002"], 1.0),
        ];
        let plan = plan_consolidation(&eps, 2, Timestamp(9));
        // docs/a: o1+o2, docs/b: o1+o3 -> both reach threshold 2
        assert_eq!(plan.memories.len(), 2);
        let topics: Vec<&str> = plan.memories.iter().map(|m| m.topic.as_str()).collect();
        assert_eq!(topics, vec!["docs/a", "docs/b"]);
    }

    #[test]
    fn file_stem_sanitizes_topic() {
        let mem = ConsolidatedMemory {
            topic: "src/api.rs:0001".to_string(),
            body: String::new(),
            source_episode_ids: vec![],
            contributor_agents: vec![],
            refs: vec![],
            confidence: 1.0,
            has_user_correction: false,
            generated_at: Timestamp(1),
        };
        assert_eq!(mem.file_stem(), "src-api-rs-0001");

        let general = ConsolidatedMemory {
            topic: "general".to_string(),
            ..mem.clone()
        };
        assert_eq!(general.file_stem(), "general");
    }
}
