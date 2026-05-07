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
  └─ .kb/search/*
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

### 6.2 三层配置优先级（已确认）

优先级从高到低：

1. **当前进程环境变量**（手动覆盖）
2. **当前知识库** `<kb_root>/.kb/config.json`
3. **全局配置**（默认 `~/.kb-cli/config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）

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
3. `.kb/search/*`
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

## 10. 非目标

当前阶段不做：

1. 让 LLM 直接改写 `kb.md` 或用户源文件。
2. 把 L5 当作唯一问答真值层。
3. 在未验证性能瓶颈前强制引入外部向量库。
4. 以“模型可用”为前提阻断 L0/L1 基础更新流程。
