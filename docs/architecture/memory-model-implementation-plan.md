# KB-CLI Memory Model (L0-L5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在保持现有 CLI 兼容性的前提下，实现 `kb update` 单命令编排的 L0-L5 分层记忆流水线（默认可退化、`--strict` 可强一致），并完成 provider-neutral 的最简 LLM 参与能力。  
**Architecture:** `kb-cli` 负责命令和输出契约；`kb-core` 负责分层 domain/port/usecase；`kb-storage-fs` 负责 `.kb/*` 持久化。LLM 只通过 adapter 进入，核心层不绑定厂商。  
**Tech Stack:** Rust workspace, `clap`, `serde/serde_json`, FS atomic writes, OpenAI-compatible HTTP shape (adapter layer only).

---

## 0. 约束冻结（先确认，再动手）

与当前设计文档一致的冻结项：

1. 命令形态采用 **方案 B：`kb update` + `--strict`**。
2. 成功语义：
   1. 默认模式：L0->L1 成功即命令成功；L2-L5 失败记 warning。
   2. `--strict`：L2-L5 任一失败则命令失败（非 0）。
3. LLM 配置优先级固定为：
   1. 进程环境变量
   2. 当前知识库 `<kb_root>/.kb/config.json`
   3. 全局 `config.json`（默认 `~/.kb-cli/config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）
4. 必需配置键：`KB_LLM_KEY`、`KB_LLM_MODEL`、`KB_LLM_BASE_URL`。
5. L5 视图层输出目录固定为 `.kb/wiki/`。

对应 spec：

1. `docs/superpowers/specs/2026-05-02-kb-update-strict-layered-pipeline-design.md`
2. `docs/superpowers/specs/2026-05-02-l2-l3-l4-generation-flow-design.md`
3. `docs/superpowers/specs/2026-05-02-l5-wiki-markdown-view-design.md`

## 1. 文件结构与职责

### 1.1 必改文件

- `crates/kb-cli/src/cli.rs`
- `crates/kb-cli/src/commands.rs`
- `crates/kb-cli/src/commands/cmd_update.rs`
- `crates/kb-cli/src/commands/runtime/core/handlers.rs`
- `crates/kb-cli/src/commands/runtime/core/guards.rs`
- `crates/kb-cli/src/core_adapters.rs`
- `crates/kb-cli/src/llm_config.rs`
- `crates/kb-cli/src/errors.rs`
- `crates/kb-core/src/ports.rs`
- `crates/kb-core/src/usecases.rs`
- `crates/kb-core/src/domain/mod.rs`
- `crates/kb-storage-fs/src/lib.rs`

### 1.2 建议新增文件

- `crates/kb-core/src/domain/knowledge.rs`
- `crates/kb-core/src/domain/search.rs`
- `crates/kb-core/src/domain/graph.rs`
- `crates/kb-core/src/domain/wiki_view.rs`
- `crates/kb-storage-fs/src/knowledge_files.rs`
- `crates/kb-storage-fs/src/search_files.rs`
- `crates/kb-storage-fs/src/graph_files.rs`
- `crates/kb-storage-fs/src/wiki_view_files.rs`
- `crates/kb-core/tests/layered_memory.rs`
- `crates/kb-cli/tests/layered_update_contract.rs`

## 2. 分阶段实施路线

### Task 1: 固化 `kb update --strict` 契约与阶段状态

**Files:**
- Modify: `crates/kb-cli/src/cli.rs`
- Modify: `crates/kb-cli/src/commands/runtime/core/handlers.rs`
- Modify: `crates/kb-cli/src/errors.rs`
- Test: `crates/kb-cli/tests/layered_update_contract.rs`

- [ ] Step 1: 增加并透传 `--strict` 参数。
- [ ] Step 2: 定义统一阶段键：
  `l0_declaration/l1_registry/l1_index/l2_semantic/l3_projection/l4_graph/l5_wiki_view`。
- [ ] Step 3: 实现默认/严格模式分流：
  1. 默认：高层失败进 `warnings`。
  2. 严格：高层失败进 `errors` 并返回 `E1011`。
- [ ] Step 4: 输出新增字段 `strict/layer_status/warnings/errors`（保持向后兼容）。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-cli layered_update_contract`

### Task 2: 实现 LLM 配置三层优先级解析

**Files:**
- Modify: `crates/kb-cli/src/llm_config.rs`
- Modify: `crates/kb-cli/src/commands/runtime/core/guards.rs`
- Test: `crates/kb-cli/tests/layered_update_contract.rs`

- [ ] Step 1: 解析优先级链：`process env > .kb/config.json > global config.json`。
- [ ] Step 2: 对 `KB_LLM_KEY/MODEL/BASE_URL` 做统一字段映射。
- [ ] Step 3: 保证进程环境变量不被 `.env` 覆盖。
- [ ] Step 4: 输出配置来源诊断信息（仅来源，不泄露密钥）。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-cli llm_config_precedence`

### Task 3: 落地 L2 语义编译（deterministic first）

**Files:**
- Create: `crates/kb-core/src/domain/knowledge.rs`
- Create: `crates/kb-storage-fs/src/knowledge_files.rs`
- Modify: `crates/kb-core/src/ports.rs`
- Modify: `crates/kb-core/src/usecases.rs`
- Test: `crates/kb-core/tests/layered_memory.rs`

- [ ] Step 1: 定义 `KnowledgeSnapshot/KnowledgeEntry/KnowledgeChunk`。
- [ ] Step 2: 定义 `SemanticExtractor` 与 `KnowledgeStore` port。
- [ ] Step 3: 实现增量规则：
  1. `input_signature` 未变化则复用。
  2. 文件缺失标记 `missing`。
  3. 单条失败标记 `failed`，整轮继续。
- [ ] Step 4: 实现 deterministic 抽取器（可无 LLM 运行）。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-core layered_memory -- --nocapture`

### Task 4: 落地 L3 搜索投影（词法优先）

**Files:**
- Create: `crates/kb-core/src/domain/search.rs`
- Create: `crates/kb-storage-fs/src/search_files.rs`
- Modify: `crates/kb-core/src/ports.rs`
- Modify: `crates/kb-core/src/usecases.rs`
- Test: `crates/kb-core/tests/layered_memory.rs`

- [ ] Step 1: 定义 `SearchProjectionSnapshot/SearchHit`。
- [ ] Step 2: 构建增量 key：`record_id + chunk_id + content_signature`。
- [ ] Step 3: 删除缺失条目对应的投影数据（prune）。
- [ ] Step 4: 保证 L3 仅输出检索命中，不生成新事实。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-core search_projection_`

### Task 5: 落地 L4 图谱构建（evidence 必填）

**Files:**
- Create: `crates/kb-core/src/domain/graph.rs`
- Create: `crates/kb-storage-fs/src/graph_files.rs`
- Modify: `crates/kb-core/src/ports.rs`
- Modify: `crates/kb-core/src/usecases.rs`
- Test: `crates/kb-core/tests/layered_memory.rs`

- [ ] Step 1: 定义 node/edge/evidence 结构。
- [ ] Step 2: 实现 deterministic builder：
  `file_has_tag/file_mentions_concept/concept_related_to_concept/file_related_to_file`。
- [ ] Step 3: 强制无 evidence 边非法。
- [ ] Step 4: 图谱输出稳定排序（便于快照和 diff）。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-core graph_builder_`

### Task 6: 落地 L5 Wiki Markdown 视图层

**Files:**
- Create: `crates/kb-core/src/domain/wiki_view.rs`
- Create: `crates/kb-storage-fs/src/wiki_view_files.rs`
- Modify: `crates/kb-core/src/ports.rs`
- Modify: `crates/kb-core/src/usecases.rs`
- Test: `crates/kb-core/tests/layered_memory.rs`

- [ ] Step 1: 定义 `.kb/wiki/` 页面与 manifest 模型。
- [ ] Step 2: 实现页面增量规划（create/update/delete）。
- [ ] Step 3: 实现 deterministic 渲染骨架：
  `Summary/Key Points/Evidence/See Also`。
- [ ] Step 4: 接入 `l5_wiki_view` 阶段状态并遵守 strict/default 语义。
- [ ] Step 5: 运行测试：
  `cargo test -p kb-core wiki_view_`

### Task 7: 让 `kb ask` 消费分层上下文并可追溯

**Files:**
- Modify: `crates/kb-cli/src/core_adapters.rs`
- Modify: `crates/kb-cli/src/commands/runtime/core/handlers.rs`
- Test: `crates/kb-cli/tests/integration_core_commands.rs`

- [x] Step 1: 检索顺序升级为 `L3 -> L2 -> L4 -> L1 fallback`。
- [x] Step 2: 命中上下文输出 `source_layer/record_id/chunk_id`。
- [x] Step 3: L2-L4 缺失时保持旧行为可用。
- [x] Step 4: 运行测试：
  `cargo test -p kb-cli ask_layered_retrieval_`

### Task 8: 文档、快照与回归门禁

**Files:**
- Modify: `docs/cli/command-manual.md`
- Modify: `docs/cli/command-examples.md`
- Modify: `docs/architecture/memory-model.md`
- Modify: `docs/architecture/wiki-markdown-view-layer.md`
- Modify: `docs/architecture/backend-agent-integration.md`

- [x] Step 1: 对齐阶段键命名（不再混用 `l3_l4_projection`）。
- [x] Step 2: 对齐 `--strict` 与默认模式语义描述。
- [x] Step 3: 对齐 LLM 配置优先级（进程 env > `.kb/config.json` > 全局 `config.json`）。
- [x] Step 4: 运行验证：
  1. `cargo test -p kb-cli root_help_matches_full_snapshot`
  2. `node tests/tools/validate-doc-fixture-diff.js`

## 3. 里程碑与验收标准

### M1（基础契约就绪）

满足：

1. `kb update --strict` 行为与错误码稳定。
2. `layer_status/warnings/errors` 字段固定可解析。
3. LLM 配置优先级可验证。

### M2（语义与检索层就绪）

满足：

1. L2、L3、L4 可增量构建。
2. 高层失败不污染 L1。
3. `kb ask` 能优先消费编译层并保留回退。

### M3（wiki 视图层就绪）

满足：

1. `.kb/wiki/` 可稳定生成并支持增量更新。
2. 页面可回指来源 chunk。
3. `l5_wiki_view` 纳入 strict/default 统一契约。

## 4. 风险与缓解

1. 风险：高层增量逻辑复杂，容易出现脏状态。  
   缓解：所有层采用 snapshot+manifest+原子写；阶段失败不覆盖上一版。

2. 风险：LLM 输出不稳定导致 schema 漂移。  
   缓解：LLM 响应先做严格 schema 校验，不合法即回退 deterministic。

3. 风险：文档与输出契约脱节。  
   缓解：help snapshot + docs fixture gate 作为合并门禁。

## 5. 执行建议

推荐执行顺序：

1. 先做 Task 1+2（契约与配置基线）。
2. 再做 Task 3+4+5（L2-L4 数据层）。
3. 然后做 Task 6（L5 视图层）。
4. 最后做 Task 7+8（ask 路径与文档收口）。

每个 Task 完成后最少运行：

```bash
cargo test -p kb-cli
cargo test -p kb-core
```

阶段合并前运行：

```bash
cargo test --workspace
node tests/tools/validate-doc-fixture-diff.js
```
