# KB-CLI 记忆模型设计文档（Memory Model）

> 状态：2026-05-02  
> 适用范围：定义 KB-CLI 的分层记忆架构、`kb update` 单命令编排语义、LLM 接入边界与对比优势。

## 1. 设计目标

KB-CLI 的目标不是“接管用户文件”，而是把用户声明的知识源编译成可重建、可解释、可增量更新的机器记忆。

核心原则：

1. **非侵入式**：用户文件默认留在原路径，除非显式 copy 到 `.kb/raw/`。
2. **声明优先**：`kb.md + .kb/raw/` 是声明源，`.kb/regs.json` 是登记快照。
3. **单命令编排**：`kb update` 是跨层更新入口，不要求用户记多条流水线命令。
4. **规则先行，LLM 增强**：没有 LLM 也能完成基础更新；LLM 用于语义质量提升。
5. **分层可退化**：高层失败不污染低层，并可按模式选择是否阻断命令。
6. **可追溯**：每个高层结论都能回指到 `record_id/chunk_id/path`。

## 2. 当前基线与目标演进

当前已稳定：

1. L0 声明解析（`kb.md`、`.kb/raw/`、外部路径）。
2. 登记快照（`.kb/regs.json`）。
3. L1 索引快照（`.kb/index.json`、`.kb/index.errors.json`、`.kb/index.meta.json`）。
4. `kb check`/`kb update`/`kb list` 基础行为。

目标演进：

1. L2 语义编译（`knowledge.json`）。
2. L3 搜索投影（词法优先，向量后置）。
3. L4 关系图谱（evidence 必须可解释）。
4. L5 Wiki Markdown 视图（`.kb/wiki/`，可删可重建）。

## 3. 分层模型总览

```text
Layer 0: User Declaration Layer
  ├─ kb.md
  ├─ .kb/raw/
  └─ external file paths
        |
        | kb update (single orchestration entrypoint)
        v
Registration Boundary
  ├─ .kb/regs.json       current registration snapshot
  └─ .kb/refs.json       legacy migration input only
        |
        | index persistence
        v
Layer 1: Canonical Snapshot Layer
  ├─ .kb/index.json
  ├─ .kb/index.errors.json
  └─ .kb/index.meta.json
        |
        | semantic compile
        v
Layer 2: Semantic Knowledge Layer
  └─ .kb/knowledge.json
        |
        | search projection compile
        v
Layer 3: Search Projection Layer
  └─ .kb/search.json
        |
        | graph compile
        v
Layer 4: Knowledge Graph Layer
  └─ .kb/graph.json
        |
        | wiki projection compile
        v
Layer 5: Wiki Markdown View Layer
  └─ .kb/wiki/*
```

说明：登记边界不是新语义层，而是 L0 声明与 L1 快照之间的稳定工作集。

## 4. 各层职责边界

### 4.1 L0（声明层）

1. 人工可维护、可审阅。
2. 不承载机器索引细节。
3. `kb add/rm/chmod` 只改声明，不直接改派生层。

### 4.2 登记边界（`.kb/regs.json`）

1. 作为当前登记真值快照。
2. 供 `check/list/update` 做稳定对比。
3. `.kb/refs.json` 只保留迁移兼容读取。

### 4.3 L1（权威索引快照）

1. 记录 path/signature/present/excerpt 等机器事实。
2. 必须可从上游重建。
3. 是 L2-L5 的输入基线。

### 4.4 L2（语义知识层）

1. 保存 `summary/tags/key_concepts/chunks` 等语义产物。
2. 记录 `input_signature` 支持增量重用。
3. 状态建议：`active | missing | failed | stale`。

### 4.5 L3（搜索投影层）

1. 只做检索性能投影，不定义新事实。
2. 先词法投影，再按 benchmark 决定向量。
3. 结果只回传 `record_id/chunk_id/score/matched_terms`。

### 4.6 L4（图谱层）

1. 显式建模 file/tag/concept/topic 关系。
2. 每条边必须包含 evidence。
3. LLM 可参与归一化建议，但最终合法性由规则校验。

### 4.7 L5（Wiki 视图层）

1. 把 L2/L4 投影为可读 markdown 页面。
2. 典型输出：`.kb/wiki/index.md`、`log.md`、`pages/*`、`manifest.json`。
3. 是派生视图，不反向覆盖 L0-L2。

## 5. `kb update` 单命令编排语义

`kb update` 编排阶段建议统一为：

1. `l0_declaration`
2. `l1_registry`
3. `l1_index`
4. `l2_semantic`
5. `l3_projection`
6. `l4_graph`
7. `l5_wiki_view`（未启用时可 `skipped`）

成功/失败策略：

1. **基础层（L0->L1）失败**：默认与严格模式都失败（非 0）。
2. **默认模式**：L2-L5 失败记入 `warnings`，命令可成功返回。
3. **严格模式（`--strict`）**：L2-L5 任一失败记入 `errors`，命令整体失败。

输出契约最小字段：

1. `strict`
2. `layer_status`
3. `warnings`
4. `errors`

`layer_status` 枚举：

1. `ok`
2. `skipped`
3. `warning`
4. `error`

## 6. LLM 接入策略与配置优先级

### 6.1 Provider 兼容策略

1. 运行时采用 OpenAI-compatible 请求形态。
2. provider 由 `base_url + model + key` 决定，不在 core 写死厂商分支。
3. DeepSeek、Mimo、OpenAI 等通过配置切换，不改核心流程。

### 6.2 配置优先级（已确认）

优先级从高到低：

1. **当前进程环境变量**（手动覆盖）
2. **系统 keyring**（仅 `key` 一个敏感字段；Windows Credential Manager /
   macOS Keychain / Secret Service，service 默认 `kb-cli`、user 默认
   `default`，可用 `KB_LLM_KEYRING_SERVICE`/`KB_LLM_KEYRING_USER` 覆盖，
   `KB_LLM_DISABLE_KEYRING=1` 关闭；不可用时静默降级到下层）
3. **当前知识库** `<kb_root>/.kb/config.json`
4. **全局配置**（默认 `~/.kb-cli/config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）

必需键：

1. `KB_LLM_KEY`
2. `KB_LLM_MODEL`
3. `KB_LLM_BASE_URL`
4. `KB_LLM_ENABLE_SEMANTIC`
5. `KB_LLM_ENABLE_GRAPH`
6. `KB_LLM_ENABLE_WIKI`

项目/全局配置结构建议：

```json
{
  "llm": {
    "enable_semantic": false,
    "enable_graph": false,
    "enable_wiki": false,
    "key": "",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1"
  }
}
```

补充约束：

1. 若进程环境已存在键值，不被项目/全局 config 覆盖。
2. 无有效 LLM 配置时，系统自动回退 deterministic 路径。
3. `--llm` 会在本次 `kb update` 强制尝试 L2/L4/L5 的 LLM 路径；缺 key 时按层回退规则路径并给 warning。
4. `key/model/base_url` 之外的超时/重试可放在 `.kb/config.json` 或全局 config。

调优参数中有两个语义不同的循环轮数（完整清单见 `kb init` 生成的
`.kb/.env.template`）：

- `KB_LLM_AGENT_LOOP_MAX_ROUNDS`（config 键 `agent_loop_max_rounds`，默认
  2，clamp `[1, 4]`）：L2/L4/L5 编译的反思轮数，覆盖率不达标时按缺失项
  重跑。
- `KB_LLM_ASK_LOOP_MAX_ROUNDS`（config 键 `ask_loop_max_rounds`，默认 8，
  clamp `[4, 12]`）：`kb ask --answer` 的工具调用轮数；证据步态
  （search → summary → raw → graph）至少需要 4 轮，故独立于上一项。

## 7. `kb ask` 的上下文连接与检索路径

检索顺序（已实现）：

1. `L3`：先用搜索投影命中候选 chunk。
2. `L2`：补足语义层 chunk（去重后继续填充 top-k）。
3. `L4`：补足图谱 evidence 命中（去重后继续填充 top-k）。
4. `L1`：最后从 index 做 fallback，保证高层缺失时仍可返回结果。
5. `AnswerEngine`：输出 `engine_status/provider/trace_id` 元信息。

上下文连接要点：

1. 每条命中都输出 `source_layer/record_id/chunk_id`。
2. `source_layer` 枚举：`l3_projection | l2_semantic | l4_graph | l1_index`。
3. `l1_index` 命中允许 `chunk_id = null`，其余层通常带 chunk 锚点。
4. 这保证了问答、图谱和 wiki 视图都能回指同一证据锚点。

## 8. 一致性与重建约束

可重建派生层：

1. `.kb/index*.json`
2. `.kb/knowledge.json`
3. `.kb/search.json`
4. `.kb/graph.json`
5. `.kb/wiki/*`

不可被重建流程删除：

1. 用户外部源文件
2. `kb.md`
3. `.kb/raw/` 原始资料（除非用户显式确认）
4. `.kb/regs.json`

## 9. 与 LLM Wiki / 传统 RAG 的差异与优势

### 9.1 对比主流 LLM Wiki

主要差异：

1. 本架构先有严格的 L0/L1 基础层，再做 L2-L5 编译；不是仅靠生成页面。
2. 页面层（L5）明确是派生层，不是事实源。
3. 默认提供 strict/default 双模式，方便开发与 CI 两类场景。

主要优势：

1. **可审计**：每层输入输出和失败位置都可定位。
2. **可退化**：高层失败不影响基础更新可用性。
3. **可迁移**：provider 无关，便于替换模型服务。

### 9.2 对比传统 RAG

主要差异：

1. 传统 RAG 常把“检索”作为中心；本架构把“分层编译记忆”作为中心。
2. 传统 RAG 多在查询时实时拼上下文；本架构强调离线/增量编译与跨层复用。
3. 本架构新增 L4 图谱与 L5 wiki 视图，覆盖“机器检索 + 人类审阅”双场景。

主要优势：

1. **查询稳定性更高**：依赖已编译层，减少每次临场抽取波动。
2. **知识累积更强**：L2-L5 随更新持续沉淀，不止“瞬时召回”。
3. **人机协同更好**：L5 让知识可读可审，而非只存在向量或临时上下文中。

## 10. Agent 记忆系统扩展（L0 Episode Layer）

> 状态：2026-05-31  
> 实现阶段：Phase 0-4 已完成并合并到主分支

### 10.1 设计动机

KB-CLI 的核心是“知识库编译”，但在 agent 交互场景中，还需要一个**时间线记忆层**来：

1. 记录 agent 观察到的事实（Observation）
2. 接受用户纠错（UserCorrection，最高优先级）
3. 固化为语义记忆文件（consolidation）
4. 让外部 agent（Claude Desktop / Cursor / Cline）通过 MCP 接入

这形成了一个**可插拔的外部 agent 记忆系统**，让任何 agent 都能把 KB-CLI 当作外挂大脑使用。

### 10.2 L0 Episode Layer 架构

L0 Episode Layer 位于现有 L0 声明层之下，是一个 **append-only 时间线存储**：

```text
L0 Episode (Time-series Memory)
  ├─ .kb/episodes.jsonl (fs backend)
  └─ episodes table (sqlite backend)
        |
        | kb consolidate
        v
  .kb/raw/agent-memory/*.md (semantic memory files)
        |
        | kb update (existing pipeline)
        v
L0 Declaration → L1 Index → L2-L5 (existing layers)
```

### 10.3 Episode 数据模型

每个 episode 包含：

- `episode_id`：唯一标识（`ep-<timestamp_nanos>-<hash16>`；unix 纳秒 + 内容稳定哈希，不依赖随机源，保证可重放）
- `agent_id`：调用方标识（从 `KB_AGENT_ID` 环境变量读取，默认 `kb-cli`）
- `session_id`：会话 ID（可选，从 `KB_SESSION_ID` 读取）
- `kind`：类型枚举（wire format 为 snake_case，如 `user_correction`）
  - `Query`：agent 发起了一次检索/提问（`kb ask` 自动记录）
  - `Answer`：agent 产出了一次回答（`kb ask --answer` 成功后记录，`refs` 挂 citations 命中的 chunk）
  - `ToolCall`：工具调用记录（保留，未启用）
  - `Observation`：agent 观察到的事实（`kb remember`，confidence 默认 0.8）
  - `UserCorrection`：用户纠错（`kb correct`，confidence 固定 1.0，最高优先级）
- `content`：记忆内容（纯文本）
- `refs`：引用的 `chunk_id` 列表（指向 L1/L2 的稳定内容地址，可为空）
- `confidence`：置信度（0.0-1.0）
- `timestamp`：Unix 纳秒时间戳

### 10.4 记忆命令

#### `kb remember <CONTENT>`
- 写入 Observation episode
- 默认 confidence 0.8
- 用于记录 agent 学到的事实

#### `kb correct <CONTENT>`
- 写入 UserCorrection episode
- 固定 confidence 1.0（最高优先级）
- 用于反幻觉和记忆冲突解决

#### `kb consolidate [--dry-run]`
- 将积累的 episodes 固化为 `.kb/raw/agent-memory/*.md` 文件
- 按 agent_id 聚类，提炼成机器可读的 skill/SOP 格式
- 固化后的文件会在下次 `kb update` 时编译进 L1-L5 层

### 10.5 MCP 集成（Model Context Protocol）

#### 启动 MCP Server
```bash
kb serve --mcp
```

通过 stdio 暴露 8 个 tools：

| Tool | 功能 |
|------|------|
| `recall` | 检索记忆（分层检索，返回 ranked contexts） |
| `remember` | 写入观察（L0 episode，kind=Observation） |
| `correct` | 写入纠错（L0 episode，kind=UserCorrection） |
| `verify_citation` | 验证引用（词法校验 quote 是否真实存在于 chunk_id） |
| `list_episodes` | 列出 episode 时间线（可按 agent_id 过滤） |
| `consolidate` | 触发记忆固化（支持 dry_run 预览） |
| `read_summary` | 读取文件语义摘要（L2） |
| `read_neighbors` | 读取知识图谱邻接（L4） |

#### 配置示例（Claude Desktop）
```json
{
  "mcpServers": {
    "kb-memory": {
      "command": "kb",
      "args": ["serve", "--mcp"],
      "env": {
        "KB_AGENT_ID": "claude-desktop"
      }
    }
  }
}
```

### 10.6 记忆系统设计原则

1. **反幻觉优先**
   - `verify_citation` 做词法验证（substring match + token overlap）
   - `UserCorrection` 最高优先级（confidence 1.0）
   - 冲突可见但不覆写（provenance 链保留历史）

2. **轻量无依赖**
   - 不用向量嵌入（保持“无 LLM 也能跑”）
   - 手写 stdio JSON-RPC（零额外依赖）
   - Append-only episode store（JSONL 或 sqlite）

3. **多 agent 共享**
   - 每个 episode 带 `agent_id` 归属
   - `consolidate` 聚合多 agent 贡献
   - 通过 `KB_AGENT_ID` 隔离不同调用方

4. **借鉴前沿研究**
   - 4 类记忆分层（Working / Episodic / Semantic / Procedural，arXiv:2404.13501）
   - 双时态标注（Zep/Graphiti：事实成立时间 + 观测时间）
   - Confidence decay + provenance chains（Mem0 冲突检测）

### 10.7 与现有分层模型的关系

L0 Episode Layer 是对现有 L0-L5 模型的**正交扩展**：

- **现有 L0-L5**：面向“知识库编译”，输入是用户声明的文件/目录
- **L0 Episode**：面向“agent 记忆”，输入是 agent 交互时间线
- **连接点**：`kb consolidate` 将 episodes 固化为 `.kb/raw/agent-memory/*.md`，然后通过 `kb update` 编译进 L1-L5

这样设计的好处：
1. 不破坏现有知识库编译流程
2. agent 记忆可以独立演进（append-only，无需重建）
3. 固化后的记忆文件与普通知识源一视同仁（统一编译、检索、图谱化）

### 10.8 实现状态

- ✅ Phase 0：基础类型（Timestamp / AgentId / KnowledgeSource / Episode）
- ✅ Phase 1：Episode store（fs + sqlite 后端）
- ✅ Phase 2：记忆命令（remember / correct / consolidate）
- ✅ Phase 3：MCP server（kb serve --mcp，8 tools）
- ✅ Phase 4：文档 + 真实集成验证（详见 `docs/phase-4-completion-report.md`）

## 11. 非目标

当前阶段不做：

1. 让 LLM 直接改写 `kb.md` 或用户源文件。
2. 把 L5 当作唯一问答真值层。
3. 在未验证性能瓶颈前强制引入外部向量库。
4. 以“模型可用”为前提阻断 L0/L1 基础更新流程。
