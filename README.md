# KB-CLI

KB-CLI 是一个本地知识库命令行项目，基于 Rust Workspace + Ports/Adapters 实现，采用「契约先行（docs/fixtures）」开发方式。

## 当前能力（2026-04）

### 命令面（12 个）

- `init`
- `add`
- `mv`
- `rm`
- `path`
- `chmod`
- `list`
- `check`
- `update`
- `ask`
- `env`
- `completion`

### ask 能力现状

- `ask` 已通过 `kb-core` ports/usecase 编排：`Retriever` + `AnswerEngine`
- 当前 `engine_status = "disabled"`（不接外部 LLM）
- 已实现本地检索增强：
  - 基于 `IndexSnapshot` 的检索与 top-k
  - 基于持久化 `excerpt` 字段的摘要
  - query-token 命中密度窗口
  - 显式 tie-break（同分按最小 start、再最小 end）
  - grapheme cluster（`unicode-segmentation`）边界安全摘要，避免 ZWJ/组合字符切割异常

> 说明：ask 算法级语义（排序/窗口/Unicode 边界）以 `docs/cli/command-manual.md` 中 `kb ask` 章节为准。

### 最近完成（2026-04）

- 代码结构维护：
  - 删除 `kb add` 中与 core usecase 重复的路径存在性校验（统一由 core 侧校验）
  - 抽取 `main.rs` 公共错误渲染逻辑，减少重复分支
- 性能优化：
  - `compute_signature` 改为流式读取，避免大文件整块读入内存
  - `compute_excerpt` 改为逐行读取首个非空行，降低 I/O 与内存占用
  - ask 摘要窗口评分改为“差分 + 前缀和”实现，降低滑窗求和复杂度
- 可维护性：
  - 对 `kb-cli` 核心链路补充中文注释（入口、命令编排、检索算法、状态与路径策略）
  - 对 `kb-core` / `kb-storage-fs` / `kb-index-memory` 核心模块补充中文注释（端口、用例、索引引擎、原子写、锁、迁移）
- 本轮继续优化（2026-04-15）：
  - `kb list` 的权限查找从“逐项线性查找”优化为“预构建映射查找”，降低大列表场景开销
  - `kb list` 渲染改为轻量记录，不再为列表场景计算文件签名/摘要，减少无效磁盘 I/O
  - `commands.rs` 中写配置失败错误映射收敛为单点函数，减少重复代码并降低维护成本
- 新增全局知识库管理（2026-04-16）：
  - 新增 `kb env --list` 命令，可列出全局登记且仍存在的知识库路径
  - `kb init` 成功后自动尝试登记知识库根目录到全局配置（失败不阻断 init）
- CLI 交互体验增强（2026-04-16）：
  - 根命令帮助改为 `kb` 形态，`Usage` 统一为 `kb [OPTIONS] <COMMAND>`
  - `kb -h` 增加分组命令概览（类似 git 的 command group）
  - text 模式改为命令专属的多行可读输出（不再是单行 JSON 风格）
  - `--output=json` 自动剔除内部 `_raw_text` 字段，保持 JSON 契约稳定
- 文档与质量门禁同步（2026-04-17）：
  - `docs/cli/command-manual.md` 补充 `kb completion` 与 `kb -h` 附录示例
  - `docs/cli/command-examples.md` 补充 `env` 默认模式与 `completion` 示例
  - 通过 `cargo clippy --workspace --all-targets -- -D warnings` 清理冗余参数传递
- 索引安全策略增强（2026-04-16）：
  - `.kbignore` 固定迁移到 `./.kb/.kbignore`
  - `kb init` 会生成默认忽略规则，自动排除 `kb.md` 与 `.kb` 内部配置文件
  - 禁止将 `kb.md` 与 `./.kb/` 下（除 `./.kb/raw/` 外）文件加入索引

---

## 项目结构（最新）

```text
.
├─ crates/
│  ├─ kb-cli/
│  │  ├─ src/
│  │  │  ├─ main.rs            # CLI 入口：参数解析、配置接线、输出协议封装
│  │  │  ├─ cli.rs             # clap 参数模型（全局选项 + 12 子命令）
│  │  │  ├─ commands.rs        # 命令编排层（run_*）
│  │  │  ├─ core_adapters.rs   # ports 适配：StateStore/IndexStore/Retriever/AnswerEngine
│  │  │  ├─ env_registry.rs    # 全局知识库注册表读写（KB_GLOBAL_CONFIG / ~/.kb-cli/config.json）
│  │  │  ├─ runtime_config.rs  # defaults/env/project/global/flags 合并
│  │  │  ├─ output.rs          # text/json 输出 envelope
│  │  │  ├─ errors.rs          # CoreErrorKind -> E-code/exit code
│  │  │  ├─ state.rs           # .kb/refs.json 读写
│  │  │  └─ paths.rs           # 路径规范化 + .kbignore 匹配
│  │  └─ tests/
│  │     ├─ cli_surface.rs             # 命令面与 help 契约
│  │     └─ integration_core_commands.rs # 端到端命令行为测试
│  ├─ kb-core/
│  │  └─ src/
│  │     ├─ domain/
│  │     │  ├─ alias.rs       # 别名规则
│  │     │  ├─ permission.rs  # 权限规则
│  │     │  ├─ reference.rs   # 引用模型
│  │     │  └─ index.rs       # 索引差异与列表引擎（IndexEngine）
│  │     ├─ usecases.rs       # 业务用例（plan/add/mv/rm/path/chmod/check/update/ask）
│  │     ├─ ports.rs          # 核心端口定义
│  │     └─ config.rs         # 配置合并模型与 trace
│  ├─ kb-storage-fs/
│  │  └─ src/
│  │     ├─ atomic_write.rs   # 原子写
│  │     ├─ index_files.rs    # .kb/index*.json 读写
│  │     ├─ lock_fs.rs        # 单写者锁
│  │     ├─ schema.rs         # schema.version 管理
│  │     ├─ migration.rs      # schema 迁移策略
│  │     ├─ error.rs          # 持久化错误模型
│  │     └─ lib.rs
│  └─ kb-index-memory/
│     └─ src/
│        ├─ memory_backend.rs # 内存索引存储
│        └─ lib.rs
├─ docs/
│  ├─ architecture/
│  │  ├─ cli-architecture.md
│  │  └─ backend-agent-integration.md
│  └─ cli/
│     ├─ command-manual.md
│     ├─ command-examples.md
│     ├─ error-catalog.md
│     ├─ state-diff-contract.md
│     ├─ kb-md-spec.md
│     └─ kbignore-spec.md
├─ tests/
│  ├─ fixtures/cli/           # 文档示例 JSON 快照
│  ├─ common/mod.rs           # 测试公共工具
│  ├─ doc_fixture_diff.rs     # docs 与 fixtures 一致性校验
│  ├─ tools/validate-doc-fixture-diff.js
│  └─ scripts/
│     ├─ run-tests.ps1
│     └─ run-tests.sh
└─ README.md
```

### 模块职责总览（建议阅读顺序）

1. `kb-cli`：先看“怎么接命令、怎么渲染输出”  
2. `kb-core`：再看“业务规则在哪里”  
3. `kb-storage-fs`：最后看“规则如何落盘”  

### 调用关系（简化）

`main.rs` → `commands.rs::run_*` → `kb-core::usecases/domain` → `kb-storage-fs`（索引持久化）

其中 `kb env` 与 `kb init` 的全局注册能力由 `env_registry.rs` 提供。

### 后端知识库与 Agent 接口（详细接入位）

当前项目已预留统一接口，支持把 ask 从“本地索引检索”切换为“后端知识库 + Agent 问答”：

- 详细文档：`docs/architecture/backend-agent-integration.md`

1. 后端知识库接口（检索）
   - `kb_core::KnowledgeBackend`
   - 请求/响应模型：
     - `BackendRetrieveRequest`
     - `BackendRetrievedItem`
2. Agent 问答接口（应答）
   - `kb_core::AgentBackend`
   - 请求/响应模型：
     - `AgentAnswerRequest`
     - `AgentAnswerResponse`
3. 桥接适配器（直接复用）
   - `kb_core::BackendRetrieverBridge<T>`
   - `kb_core::BackendAnswerEngineBridge<T>`
4. CLI 接入点
   - `crates/kb-cli/src/commands.rs` 中 `build_ask_stack()`

接入步骤（推荐）：

1. 在新 crate（或 `kb-cli/src`）实现 `KnowledgeBackend` 和 `AgentBackend`。
2. 在 `build_ask_stack()` 中替换默认本地实现：

```rust
(
  Box::new(kb_core::BackendRetrieverBridge::new(MyKnowledgeBackend::new())),
  Box::new(kb_core::BackendAnswerEngineBridge::new(MyAgentBackend::new())),
)
```

3. 保持 `run_ask` 与 `execute_ask` 不变，完成无侵入切换。

### 典型改动入口（实战）

- 新增/调整命令参数：`crates/kb-cli/src/cli.rs`
- 调整命令行为编排：`crates/kb-cli/src/commands.rs`
- 修改业务规则（校验/状态转换）：`crates/kb-core/src/usecases.rs`
- 修改索引差异算法：`crates/kb-core/src/domain/index.rs`
- 修改落盘格式或原子写策略：`crates/kb-storage-fs/src/index_files.rs` / `atomic_write.rs`

---

## 本地知识库目录约定

执行 `kb init` 后，目录中会生成：

- `kb.md`：引用与分类主清单
- `.kb/`：内部状态目录（索引与状态）
- `./.kb/raw/`：`kb add --mode copy` 的归档目标目录
- `./.kb/.kbignore`：忽略规则文件（固定在 `.kb/` 下，`kb init` 自动写入默认规则）

索引保护规则（实现）：

- `kb.md` 不允许加入索引
- `./.kb/` 下仅 `./.kb/raw/` 可加入索引，其余内部配置文件均被拦截

## 全局知识库登记（新增）

为支持“全局安装后在多目录使用”，CLI 提供全局知识库登记机制：

- `kb init` 成功后会尝试把当前知识库根目录登记到全局配置
- `kb env --list` 可列出“已登记且仍存在”的知识库路径
- 全局配置文件路径优先级：
  1. 环境变量 `KB_GLOBAL_CONFIG`
  2. 默认 `~/.kb-cli/config.json`（Windows 下使用 `USERPROFILE`）

当前注册表字段：

- `knowledge_bases`: `string[]`（知识库绝对路径数组）

---

## 质量门禁与常用命令

```bash
# 编译/测试
cargo check --workspace
cargo test --workspace

# 静态检查
cargo clippy --workspace --all-targets -- -D warnings

# 文档与 fixtures 一致性
node tests/tools/validate-doc-fixture-diff.js

# 根帮助全量快照（防止 help 文案回归）
cargo test -p kb-cli root_help_matches_full_snapshot

# 查看命令面
cargo run -p kb-cli -- --help
```

---

## 打包与 Release 发布

详细安装与打包文档见：`docs/cli/install-and-package.md`。

### 本地打包

Windows：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

Linux：

```bash
bash ./scripts/package.sh --target x86_64-unknown-linux-gnu
```

说明：

- `scripts/package.ps1` 仅用于 Windows 产物（`.zip`）。
- `scripts/package.sh` 仅用于 Linux 产物（`.tar.gz`）。
- 在 Git Bash/MSYS/Cygwin 环境中执行 `package.sh` 时，必须显式传入 Linux `--target`。

### GitHub Actions 自动发布

工作流文件：`.github/workflows/release.yml`

当前触发规则：

- 仅在“新建 tag”事件触发（`on: create`）
- 且仅处理 tag 名以 `v` 开头（如 `v0.1.2`）
- 推送分支（例如 `main`）不会触发发布任务

发布行为：

1. 先校验 tag 格式并创建（或复用）GitHub Release。
2. Linux runner 构建并打包 `.tar.gz`，直接上传到该 Release。
3. Windows runner 构建并打包 `.zip`，直接上传到该 Release。

推荐发布命令：

```bash
git push -u origin main
git tag <新的版本号>
git push -u origin <新的版本号>
```

> 注意：已存在的旧 tag 重新 push 通常不会触发 `create` 事件，建议使用新版本 tag。

---
