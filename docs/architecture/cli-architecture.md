# KB-CLI 架构文档（CLI Architecture）

## Overview

本文件描述 KB-CLI 当前**已落地实现**与稳定架构边界。

- 架构风格：Rust Workspace + Hexagonal（Ports / Adapters）
- 核心目标：低耦合、可测试、契约先行（docs/fixtures）
- 命令集合：`init/add/mv/rm/path/chmod/list/check/update/ask/completion`

---

## Current Implementation Status

当前仓库已进入**实现与收敛阶段**，`crates/*` 全部落地并通过 workspace 级验证。

已落地能力（摘要）：

1. Command routing 与 usecase 映射完成（Phase 1）
2. 配置管线（defaults/file/env/flags）与 trace 落地（Phase 2）
3. 错误模型收敛为单点映射（core error -> CLI E-code/exit code）（Phase 2）
4. ask 端口化（`Retriever` / `AnswerEngine`）与本地检索演进（Phase 3~3.6）
5. 文档/fixtures/实现一致性 gate 长期开启

最近维护（2026-04-15）：

1. 删除 `kb add` 入口层重复校验，统一由 core usecase 校验输入
2. `compute_signature` / `compute_excerpt` I/O 路径优化（流式读取 + 首行读取）
3. ask 摘要窗口从重复滑窗求和优化为“差分 + 前缀和”
4. 入口错误渲染路径去重，补齐核心链路中文注释
5. `kb list` 改为轻量记录渲染，不再在列表场景计算签名/摘要
6. 增补后端知识库与 Agent 接入文档：`docs/architecture/backend-agent-integration.md`

---

## Normative Sources

以下文件为契约规范源（Normative）：

- `docs/cli/command-manual.md`
- `docs/cli/command-examples.md`
- `docs/cli/error-catalog.md`
- `docs/cli/state-diff-contract.md`
- `docs/cli/kbignore-spec.md`
- `docs/cli/kb-md-spec.md`

若本文件与规范源冲突，以规范源为准。

---

## Dependency Direction Policy

依赖方向保持单向：

`kb-cli (app + adapters)  ->  kb-core (domain + usecases + ports)`

强约束：

1. `kb-core` 禁止依赖 `kb-cli` / `kb-storage-fs` / `kb-index-memory`
2. adapters 只实现 `kb-core` ports，不承载业务规则
3. `kb-cli` 负责参数解析、usecase 调用、输出渲染与退出码映射

---

## Command-to-Usecase Mapping

| Command | Usecase/Flow | 读写类型 |
|---|---|---|
| `kb init` | `usecases::plan_init` + CLI 资源初始化 | 写 |
| `kb add` | `usecases::plan_add` + 状态/索引更新 | 写 |
| `kb mv` | `usecases::apply_mv` | 写 |
| `kb rm` | `usecases::apply_rm` | 写 |
| `kb path` | `usecases::apply_path` | 读/写 |
| `kb chmod` | `usecases::apply_chmod` | 写 |
| `kb list` | `IndexEngine::render_list` + `usecases::plan_list_output` | 读 |
| `kb check` | `IndexEngine::check` | 读（不写索引） |
| `kb update` | `IndexEngine::update` | 写（写 `.kb/index*.json`） |
| `kb ask` | `usecases::execute_ask`（Retriever + AnswerEngine） | 读 |
| `kb completion` | clap completion 生成 | 读 |

补充说明（当前实现）：

- `kb rm --force`：参数已保留，当前不改变行为（无交互确认流程）
- `kb update --yes`：参数已保留，当前不改变行为（预留非交互扩展位）
- `kb list --long`：`mtime` 字段当前输出 Unix 秒字符串（而非 ISO 时间）
- `kb check` / `kb update`：当前快照来源为 `.kb/refs.json` 投影记录，而非目录全量扫描

---

## ask Retrieval Architecture (Phase 3~3.6)

当前 ask 路径：

1. `commands.rs::run_ask` 加载 snapshot
2. `kb-core::usecases::execute_ask` 编排
3. `DefaultRetriever` 执行本地检索（无外部 LLM）
4. `DisabledAnswerEngine` 返回 `engine_status="disabled"`

### 检索能力（当前）

- 基于 `IndexSnapshot.records` 的 top-k 结果
- `IndexRecord.excerpt` 持久化并参与检索
- query-token 命中密度窗口摘要
- 显式 tie-break：同分按 `start` 最小，再按 `end` 最小
- grapheme cluster 边界安全切片（`unicode-segmentation`）

### 当前约束

- 不接真实外部 LLM/provider（保持 `engine_status="disabled"`）
- 优先保证 deterministic 输出与可测试性

---

## Crate Contracts

### kb-cli

#### Responsibility
- CLI entry / clap 参数模型 / presenter（text/json）
- runtime config 合并接线
- core error 映射到 E-code / exit code

#### Invariants
- 不实现领域规则
- `--output=json` 时 stdout 为合法 JSON envelope
- 命令层主要做“路由 + 协议封装”，输入校验与规则尽量下沉到 `kb-core`

### kb-core

#### Responsibility
- domain models
- usecases
- ports traits（`StateStore` / `IndexStore` / `Retriever` / `AnswerEngine`）

#### Invariants
- 不依赖 CLI / FS 具体实现
- `check` / `update` 共享同一状态判定契约

### kb-storage-fs

#### Responsibility
- `kb-core` ports 的 FS 实现
- schema/migration/lock/index 文件持久化

#### Invariants
- 只做适配，不承载业务编排

### kb-index-memory

#### Responsibility
- 内存索引后端与辅助能力

#### Invariants
- 不依赖 CLI 逻辑

---

## Test & Quality Gates

以下 gate 任一失败即视为不可合并：

1. Workspace 构建/测试 gate
   - `cargo check --workspace`
   - `cargo test --workspace`
2. 静态质量 gate
   - `cargo clippy --workspace --all-targets -- -D warnings`
3. 文档/fixture 一致性 gate
   - `node tests/tools/validate-doc-fixture-diff.js`

---

## Out of Scope（当前）

1. 真实 LLM / 外部 AI provider 调用
2. 远程向量数据库强耦合
3. 动态插件系统
4. 为“性能优化”破坏现有契约稳定性
