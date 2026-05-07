# 后端知识库与 Agent 接入指南

本文档说明如何把 `kb ask` 从默认本地检索栈，切换为“后端知识库检索 + Agent 问答”。

## 1. 现有接入点

当前项目已经预留以下稳定接口：

1. 检索接口（知识库）
   - `kb_core::KnowledgeBackend`
   - 输入：`BackendRetrieveRequest`
   - 输出：`Vec<BackendRetrievedItem>`
2. 问答接口（Agent）
   - `kb_core::AgentBackend`
   - 输入：`AgentAnswerRequest`
   - 输出：`AgentAnswerResponse`
3. 桥接器
   - `kb_core::BackendRetrieverBridge<T>`
   - `kb_core::BackendAnswerEngineBridge<T>`
4. CLI 注入点
   - `crates/kb-cli/src/commands.rs` 的 `build_ask_stack()`

## 2. 字段语义（建议遵守）

### BackendRetrieveRequest

- `question`：用户问题原文。
- `top_k`：需要返回的上下文条数上限。
- `trace_id`：链路追踪 ID（可选）。
- `session_id`：会话 ID（可选）。

### BackendRetrievedItem

- `alias`：命中文档标识（用于 CLI 回显和调试定位）。
- `snippet`：摘要片段（建议已做长度控制）。
- `score`：相关性分数（建议范围 `0.0 ~ 1.0`）。
- `source_layer`：命中来源层（`l3_projection|l2_semantic|l4_graph|l1_index`）。
- `record_id`：稳定记录锚点（建议与 `alias` 对齐或可映射）。
- `chunk_id`：分块锚点（`l1_index` fallback 可为空）。
- `source`：后端来源标识（可选，如向量库分片/索引名）。

### AgentAnswerRequest

- `question`：用户问题原文。
- `contexts`：检索出的上下文数组。
- `trace_id` / `session_id`：同上，用于链路一致性。

### AgentAnswerResponse

- `engine_status`：引擎状态（如 `enabled` / `degraded` / `fallback`）。
- `provider`：上游服务标识（如模型供应商/模型名）。
- `trace_id`：可回传后端链路 ID。
- `answer_text`：预留字段，当前 CLI 尚未输出正文。

## 3. 最小接入步骤

1. 在新 crate 或 `kb-cli/src` 实现 `KnowledgeBackend`。
2. 实现 `AgentBackend`。
3. 在 `build_ask_stack()` 中替换默认本地实现：

```rust
(
    Box::new(kb_core::BackendRetrieverBridge::new(MyKnowledgeBackend::new())),
    Box::new(kb_core::BackendAnswerEngineBridge::new(MyAgentBackend::new())),
)
```

4. 保持 `run_ask` 与 `kb_core::usecases::execute_ask` 不变。

这样可以在不改命令入参与 JSON 协议的前提下完成切换。

## 4. 推荐实现约束

1. `retrieve`/`answer` 失败时返回可诊断错误字符串（含服务名与关键参数）。
2. 保证 `top_k` 生效，避免后端过量返回。
3. `score` 缺失时建议回填默认值（例如 `0.0`），避免排序不稳定。
4. 对 `trace_id/session_id` 做透传，便于跨系统排障。
5. 避免在 bridge 层耦合 HTTP/SDK 细节，bridge 只负责协议映射。

## 5. 典型调用流

1. CLI 收到 `kb ask <question> --top <k>`。
2. `run_ask` 读取本地索引快照并调用 `build_ask_stack()`。
3. `execute_ask` 按 `L3 -> L2 -> L4 -> L1 fallback` 调用 `Retriever::retrieve`。
4. Bridge 将请求映射到 `KnowledgeBackend::retrieve`。
5. `execute_ask` 调用 `AnswerEngine::meta`。
6. Bridge 将请求映射到 `AgentBackend::answer`。
7. CLI 输出统一 JSON：`engine_status/provider/trace_id/retrieved_context`。

## 6. LLM 运行时配置优先级（已确认）

当接入外部 LLM（OpenAI / DeepSeek / Mimo / 其他 OpenAI-compatible 服务）时，配置读取顺序固定为：

1. 当前进程环境变量（最高优先级）
2. 当前知识库 `<kb_root>/.kb/config.json`
3. 全局配置（默认 `~/.kb-cli/config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）

关键配置键：

1. `KB_LLM_KEY`
2. `KB_LLM_MODEL`
3. `KB_LLM_BASE_URL`
4. `KB_LLM_ENABLE_SEMANTIC`
5. `KB_LLM_ENABLE_GRAPH`
6. `KB_LLM_ENABLE_WIKI`

项目/全局 config 的推荐结构：

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

约束：

1. 如果进程环境变量已存在键值，不允许被项目/全局 config 覆盖。
2. 未解析到可用 key 时，系统会回退到规则路径并返回可诊断 warning（不会崩溃）。

## 7. 最简 Agent 参与边界

第一阶段不要求完整 ReAct 或复杂工具调用循环，最简可用形态即可：

1. 输入：`question + retrieved_context`
2. 输出：`engine_status/provider/trace_id`（以及后续可扩展 answer_text）
3. 能力：支持“上传上下文 -> 返回结构化响应”的单轮模式

建议先完成以下能力再演进复杂 agent：

1. 可替换 provider（通过 `base_url + model + key` 切换）
2. 稳定结构化输出（字段校验）
3. 失败回退（不阻断基础命令）

## 8. 分层上下文建议（面向后续图谱与记忆）

为了让后续知识图谱和跨轮记忆更稳定，检索上下文建议携带：

1. `source_layer`
2. `record_id`
3. `chunk_id`

这样 Agent 回答和图谱更新都可以回到同一批证据锚点，减少“只靠自由文本”带来的漂移。
