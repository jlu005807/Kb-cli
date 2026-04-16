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
3. `execute_ask` 调用 `Retriever::retrieve`。
4. Bridge 将请求映射到 `KnowledgeBackend::retrieve`。
5. `execute_ask` 调用 `AnswerEngine::meta`。
6. Bridge 将请求映射到 `AgentBackend::answer`。
7. CLI 输出统一 JSON：`engine_status/provider/trace_id/retrieved_context`。

