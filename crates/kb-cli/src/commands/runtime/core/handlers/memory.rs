// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 2：记忆沉淀的写入侧 —— `kb remember` / `kb correct` / `kb consolidate`。
//
// 三个命令的分工：
// - remember : agent 主动记录一条 Observation（自评 confidence，默认 0.8）
// - correct  : 用户显式纠错，写 UserCorrection（confidence 固定 1.0，最高优先级）
// - consolidate : 把累积的 assertive episodes 聚类提炼成 .kb/raw/agent-memory/*.md，
//                 再 kb add 声明，从而下一次 kb update 进入 L1-L5 获得稳定 chunk_id
//
// 与 ask 的 episode 写入（Phase 1.5）的关键区别：remember / correct 是**显式
// 写入命令**，写失败必须冒泡为错误让用户知道——不像 ask 的旁路观测可以静默降级。
//
// 复用 ask/episodes.rs 的 helper（include! 平铺命名空间）：
//   ask_episode_agent_id / ask_episode_session_id / make_episode_id

/// consolidate 产物的落点目录（相对仓库根）。放在 `.kb/raw/` 下而不是 `.kb/`
/// 根，是因为 `.kb/` 被 `is_protected_index_target` 保护、无法 `kb add`；而
/// `.kb/raw/`（RAW_DIR_REL）是声明文件的正常归属地，能被 add + update 正常编译。
const AGENT_MEMORY_SUBDIR: &str = "raw/agent-memory";

/// `kb remember <content> [--ref <chunk_id>...] [--confidence <f32>]`
///
/// 写一条 Observation episode。confidence 缺省 0.8（agent 自评，需后续 verify
/// 才升到更高）。写入失败冒泡为错误。
pub(crate) fn run_remember(
    content: String,
    refs: Vec<String>,
    confidence: Option<f32>,
) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let storage = StorageHandles::open(&cwd)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let now = kb_core::Timestamp::now();
    let id = make_episode_id(now, &format!("observation|{}", content));
    let conf = confidence.unwrap_or(0.8);
    let episode = kb_core::Episode::new(
        id.clone(),
        ask_episode_agent_id(),
        ask_episode_session_id(),
        kb_core::EpisodeKind::Observation,
        content.clone(),
    )
    .with_timestamp(now)
    .with_refs(refs.clone())
    .with_confidence(conf);

    storage.episode.append(&episode).map_err(|err| {
        map_core_error(
            CoreErrorKind::InvalidKbFormat,
            json!({ "reason": "episode append failed", "detail": err.to_string() }),
        )
    })?;

    let data = json!({
        "command": "remember",
        "episode_id": id,
        "kind": "observation",
        "agent_id": ask_episode_agent_id().as_str(),
        "confidence": conf,
        "refs": refs,
    });
    let text = format!("remembered observation {} (confidence {:.2})", id, conf);
    Ok(with_text(data, text))
}

/// `kb correct <content> [--ref <chunk_id>...]`
///
/// 写一条 UserCorrection episode（confidence 固定 1.0）。这是反幻觉里最强的
/// 信号——用户说错就是错，consolidation 时优先级最高、会排在产物最前并打标。
pub(crate) fn run_correct(content: String, refs: Vec<String>) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let storage = StorageHandles::open(&cwd)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let now = kb_core::Timestamp::now();
    let id = make_episode_id(now, &format!("correction|{}", content));
    let episode = kb_core::Episode::new(
        id.clone(),
        ask_episode_agent_id(),
        ask_episode_session_id(),
        kb_core::EpisodeKind::UserCorrection,
        content.clone(),
    )
    .with_timestamp(now)
    .with_refs(refs.clone())
    .with_confidence(1.0);

    storage.episode.append(&episode).map_err(|err| {
        map_core_error(
            CoreErrorKind::InvalidKbFormat,
            json!({ "reason": "episode append failed", "detail": err.to_string() }),
        )
    })?;

    let data = json!({
        "command": "correct",
        "episode_id": id,
        "kind": "user_correction",
        "agent_id": ask_episode_agent_id().as_str(),
        "refs": refs,
    });
    let text = format!("recorded user correction {}", id);
    Ok(with_text(data, text))
}

/// `kb consolidate [--min <n>] [--dry-run]`
///
/// 读全部 episode → `plan_consolidation` 提炼 → 写 `.kb/raw/agent-memory/*.md`
/// 并 `kb add` 声明。dry-run 只返回计划、不落盘。
pub(crate) fn run_consolidate(min: usize, dry_run: bool) -> Result<Value, CliError> {
    let cwd = ensure_initialized()?;
    let storage = StorageHandles::open(&cwd)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let episodes = storage
        .episode
        .query(&kb_core::EpisodeFilter::default())
        .map_err(|err| {
            map_core_error(
                CoreErrorKind::InvalidKbFormat,
                json!({ "reason": "episode query failed", "detail": err.to_string() }),
            )
        })?;

    let now = kb_core::Timestamp::now();
    let plan = kb_core::plan_consolidation(&episodes, min, now);

    let mut written: Vec<Value> = Vec::new();
    let mut added: Vec<Value> = Vec::new();

    if !dry_run && !plan.memories.is_empty() {
        let mem_dir = cwd.join(".kb").join(AGENT_MEMORY_SUBDIR);
        std::fs::create_dir_all(&mem_dir).map_err(|err| {
            map_core_error(
                CoreErrorKind::InvalidKbFormat,
                json!({ "reason": "create agent-memory dir failed", "detail": err.to_string() }),
            )
        })?;

        for mem in &plan.memories {
            let stem = mem.file_stem();
            let file_name = format!("{stem}.md");
            let abs_path = mem_dir.join(&file_name);
            let doc = render_memory_document(mem);
            std::fs::write(&abs_path, doc).map_err(|err| {
                map_core_error(
                    CoreErrorKind::InvalidKbFormat,
                    json!({ "reason": "write memory file failed", "file": file_name, "detail": err.to_string() }),
                )
            })?;

            let rel_path = format!("./.kb/{}/{}", AGENT_MEMORY_SUBDIR, file_name);
            let alias = format!("agent-memory/{stem}");
            written.push(json!({
                "topic": mem.topic,
                "file": rel_path,
                "episodes": mem.source_episode_ids.len(),
                "confidence": mem.confidence,
                "has_user_correction": mem.has_user_correction,
            }));

            // 登记声明：mode=Link（文件已在 raw/ 下，不需要再复制），category=raw。
            // 已存在同名声明时 add 会报冲突——这里吞掉冲突错误，视作"已登记"。
            match run_add(
                rel_path.clone(),
                Some(alias.clone()),
                AddMode::Link,
                Some("raw".to_string()),
            ) {
                Ok(_) => added.push(json!({ "alias": alias, "file": rel_path })),
                Err(_) => { /* 已登记或被忽略——产物文件已写，update 仍会编译 */ }
            }
        }
    }

    let data = json!({
        "command": "consolidate",
        "dry_run": dry_run,
        "min_episodes_per_topic": min.max(1),
        "considered": plan.considered,
        "skipped_non_assertive": plan.skipped_non_assertive,
        "skipped_below_threshold": plan.skipped_below_threshold,
        "memories_planned": plan.memories.len(),
        "memories": plan.memories.iter().map(memory_summary_json).collect::<Vec<_>>(),
        "written": written,
        "added": added,
    });

    let text = if dry_run {
        format!(
            "consolidate (dry-run): {} memory file(s) would be written from {} episode(s)",
            plan.memories.len(),
            plan.considered
        )
    } else {
        format!(
            "consolidate: wrote {} memory file(s) from {} episode(s); run `kb update` to compile into L1-L5",
            written.len(),
            plan.considered
        )
    };
    Ok(with_text(data, text))
}

/// 一条 memory 的精简 JSON 视图（dry-run / 落盘都返回，便于外部 agent 消费）。
fn memory_summary_json(mem: &kb_core::ConsolidatedMemory) -> Value {
    json!({
        "topic": mem.topic,
        "file_stem": mem.file_stem(),
        "confidence": mem.confidence,
        "has_user_correction": mem.has_user_correction,
        "source_episode_ids": mem.source_episode_ids,
        "contributor_agents": mem.contributor_agents,
        "refs": mem.refs,
    })
}

/// 渲染落盘的记忆文档。正文（mem.body）面向 agent 检索；顶部加一段
/// machine-readable 的 frontmatter-ish 元信息块，方便外部消费 provenance。
fn render_memory_document(mem: &kb_core::ConsolidatedMemory) -> String {
    let meta = json!({
        "topic": mem.topic,
        "confidence": mem.confidence,
        "has_user_correction": mem.has_user_correction,
        "source_episode_ids": mem.source_episode_ids,
        "contributor_agents": mem.contributor_agents,
        "refs": mem.refs,
        "generated_at": mem.generated_at.as_nanos(),
    });
    let meta_str = serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".to_string());
    format!(
        "<!-- kb-agent-memory\n{}\n-->\n\n{}",
        meta_str, mem.body
    )
}
