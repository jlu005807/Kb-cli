// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 1.5：把 `kb ask` 接到 L0 Episode 时间线。
//
// 设计原则——episode 写入是**旁路观测**，绝不能因为写入失败而让 ask 本身失败：
// 所有 append 错误都被吞掉并降级成一条 ask trace（emit_ask_trace），
// 用户拿到的答案不受影响。这呼应"记忆是增强、不是依赖"的整体取舍。
//
// agent 归属与会话：
// - `KB_AGENT_ID` 环境变量决定 episode 的 agent_id；缺省 "kb-cli"（本项目
//   内置 agent 只是参考实现，外部 agent 通过 MCP/CLI 接入时各自传自己的 id）。
// - `KB_SESSION_ID` 环境变量把同一轮任务的多条 episode 串起来；缺省空串。
//
// episode id 生成：unix 纳秒 + (question, kind, role) 的稳定 hash。不依赖
// 随机源（保持可重放 / 测试确定性），同一纳秒内不同内容也不易碰撞。

use std::hash::{Hash, Hasher};

/// 从环境解析 agent 归属。空白或未设回退到内置 "kb-cli"。
fn ask_episode_agent_id() -> kb_core::AgentId {
    std::env::var("KB_AGENT_ID")
        .ok()
        .and_then(|raw| kb_core::AgentId::new(raw).ok())
        .unwrap_or_else(|| kb_core::AgentId::new("kb-cli").expect("static agent id is valid"))
}

/// 当前会话 id。未设则空串（表示"无显式会话语境"）。
fn ask_episode_session_id() -> String {
    std::env::var("KB_SESSION_ID").unwrap_or_default()
}

/// 为一条 episode 生成稳定 id：`ep-<nanos>-<hash16>`。
fn make_episode_id(now: kb_core::Timestamp, discriminant: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    discriminant.hash(&mut hasher);
    now.as_nanos().hash(&mut hasher);
    format!("ep-{}-{:016x}", now.as_nanos(), hasher.finish())
}

/// 记录一条 `Query` episode：每次 `kb ask` 进入时调用（retrieve / answer 两条
/// 路径共用）。失败仅降级为 trace，不影响 ask。返回写入的 episode id（供
/// Answer episode 串联），失败返回 None。
fn record_ask_query_episode(
    storage: &StorageHandles,
    question: &str,
    answer_mode: bool,
) -> Option<String> {
    let now = kb_core::Timestamp::now();
    let id = make_episode_id(now, &format!("query|{}|{}", answer_mode, question));
    let episode = kb_core::Episode::new(
        id.clone(),
        ask_episode_agent_id(),
        ask_episode_session_id(),
        kb_core::EpisodeKind::Query,
        question.to_string(),
    )
    .with_timestamp(now);

    match storage.episode.append(&episode) {
        Ok(()) => {
            emit_ask_trace(&format!("recorded query episode {}", id));
            Some(id)
        }
        Err(err) => {
            emit_ask_trace(&format!("episode append (query) skipped: {}", err));
            None
        }
    }
}

/// 记录一条 `Answer` episode：仅 answer 路径成功产出后调用。
/// `refs` 取本次回答引用到的 chunk_id（先 citations，回退 retrieved_context），
/// 让这条记录直接挂上证据链，后续 consolidation / verify_citation 能用。
fn record_ask_answer_episode(
    storage: &StorageHandles,
    question: &str,
    answer_text: &str,
    citations: &[Value],
    retrieved: &[kb_core::RetrievedContext],
) {
    let now = kb_core::Timestamp::now();
    let id = make_episode_id(now, &format!("answer|{}", question));
    let refs = collect_answer_refs(citations, retrieved);

    let episode = kb_core::Episode::new(
        id.clone(),
        ask_episode_agent_id(),
        ask_episode_session_id(),
        kb_core::EpisodeKind::Answer,
        answer_text.to_string(),
    )
    .with_timestamp(now)
    .with_refs(refs);

    match storage.episode.append(&episode) {
        Ok(()) => emit_ask_trace(&format!("recorded answer episode {}", id)),
        Err(err) => emit_ask_trace(&format!("episode append (answer) skipped: {}", err)),
    }
}

/// 收集 answer episode 的 chunk 引用。优先用 citations 里显式声明的 chunk_id，
/// 没有就退回检索命中的 chunk_id。去重并保持稳定顺序。
fn collect_answer_refs(
    citations: &[Value],
    retrieved: &[kb_core::RetrievedContext],
) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for citation in citations {
        if let Some(chunk_id) = citation.get("chunk_id").and_then(Value::as_str) {
            if !chunk_id.is_empty() && seen.insert(chunk_id.to_string()) {
                refs.push(chunk_id.to_string());
            }
        }
    }
    if refs.is_empty() {
        for ctx in retrieved {
            if let Some(chunk_id) = ctx.chunk_id.as_deref() {
                if !chunk_id.is_empty() && seen.insert(chunk_id.to_string()) {
                    refs.push(chunk_id.to_string());
                }
            }
        }
    }
    refs
}
