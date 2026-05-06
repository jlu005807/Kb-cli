# KB-CLI

KB-CLI 是一个基于 Rust Workspace 构建的本地知识库命令行工具。

当前实现采用“声明优先”模型：

- 声明源：`kb.md + ./.kb/raw/`
- 登记快照：`./.kb/regs.json`
- 索引快照：`./.kb/index*.json`

## 命令列表

- `kb init`
- `kb add`
- `kb mv`
- `kb rm`
- `kb path`
- `kb chmod`
- `kb list`
- `kb check`
- `kb update`
- `kb ask`
- `kb env`
- `kb completion`

## 行为边界

- `kb add`：写入 `kb.md` 声明项（不直接写 `regs.json`）。
- `kb rm`：删除 `kb.md` 声明项（不直接写 `regs.json`）。
- `kb chmod`：修改 `kb.md` 中的声明权限（不直接写 `regs.json`）。
- `kb check`：比较声明源与 `regs.json` 的差异。
- `kb update`：把差异落盘到 `regs.json` 并刷新后端索引快照。
- `kb list`：展示 `regs.json` 中的登记结果。

`kb update` 还会做 raw 与声明的对齐：

- 把新增的 `./.kb/raw/*` 文件回填到 `# raw` 分类
- 删除已确认为 `not_found` 的声明行
- 按规范化路径对重复 raw 声明去重

## 运行时解耦（2026-04-19）

命令实现已完成解耦：

- `crates/kb-cli/src/commands.rs`：仅负责分发
- `crates/kb-cli/src/commands/cmd_*.rs`：各命令连接器
- `crates/kb-cli/src/commands/runtime.rs`：运行时导出层
- `crates/kb-cli/src/commands/runtime/core.rs`：运行时组装层
- `crates/kb-cli/src/commands/runtime/core/*.rs`：拆分后的运行时逻辑（handlers/diff/resolve/kbmd_ops 等）

该重构不改变命令契约与输出 schema。

## 项目结构

```text
crates/
  kb-cli/
    src/
      main.rs
      cli.rs
      commands.rs
      commands/
        cmd_*.rs
        runtime.rs
        runtime/
          core.rs
          core/
            shared.rs
            handlers.rs
            guards.rs
            registry_models.rs
            resolve.rs
            diff.rs
            kbmd_ops.rs
      core_adapters.rs
      env_registry.rs
      errors.rs
      output.rs
      paths.rs
      registry.rs
      state.rs
  kb-core/
  kb-storage-fs/
  kb-index-memory/
  kb-llm/

docs/
  cli/
    kb-md-spec.md
    command-manual.md
    command-examples.md
    error-catalog.md
    state-diff-contract.md
    kbignore-spec.md
```

## 快速开始

```bash
# 初始化
kb init demo

# 添加声明（写入 kb.md）
kb add ./docs docs

# 查看声明与登记差异
kb check

# 同步差异到 regs/index
kb update

# 列出当前登记项
kb list --long
```

## 开发命令

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# 文档与 fixtures 一致性
node tests/tools/validate-doc-fixture-diff.js
```

## LLM 模块化架构（2026-05-05）

当前 LLM 语义编译已按职责拆分为独立 crate，便于维护和替换 provider：

- `crates/kb-cli`：只负责配置解析、命令编排、输出结构。
- `crates/kb-core`：定义语义提取 port（`SemanticExtractor`）和默认规则提取器。
- `crates/kb-llm`：实现 OpenAI-compatible 语义编译器（`LlmSemanticExtractor`）。
- `crates/kb-storage-fs`：负责持久化（L2/L3/L4/L5 快照写入）。
- `crates/kb-index-memory`：负责分层检索实现。

`kb update` 的 L2 语义流程：

1. `kb-cli` 调用 `kb-llm::config` 解析 LLM 运行时配置（独立模块）。
2. 配置优先级固定为：`进程环境变量 > 项目配置(.kb/config.json) > 全局配置($KB_GLOBAL_CONFIG 或 ~/.kb-cli/config.json)`。
3. 若 `KB_LLM_ENABLE_SEMANTIC=1` 且存在 `KB_LLM_KEY`，装配 `kb-llm::LlmSemanticExtractor`。
4. 通过 `kb-core::compile_l2` 对每个文件执行语义提取（支持增量复用）。
5. 写入 `.kb/knowledge.json`，后续继续编译 L3/L4/L5。

支持两种语义编译模式：

- `KB_LLM_SEMANTIC_FLOW=two_step`（推荐）：分析 -> 生成。
- `KB_LLM_SEMANTIC_FLOW=single_step`：单请求直出结构化记忆。

并支持轻量级 agent 式反思修复循环（参考 GenericAgent 的最小循环思想）：

- `KB_LLM_AGENT_LOOP_MAX_ROUNDS`：最多反思轮次（1-4）。
- `KB_LLM_AGENT_REFLECTION_ENABLED`：是否开启反思修复。
- `KB_LLM_AGENT_MIN_CHUNK_COVERAGE`：最小 chunk 覆盖率阈值（0.0-1.0）。

并内置 provider 兼容与稳定性策略：

- OpenAI 标准 Chat Completions 请求格式。
- `max_completion_tokens` 与 `max_tokens` 回退兼容。
- 可配置重试与退避：`KB_LLM_RETRY_MAX_ATTEMPTS`、`KB_LLM_RETRY_BACKOFF_MS`。

## LLM 配置与测试实操

### 1) 项目级配置（推荐）

在知识库根目录创建 `./.kb/config.json`：

```json
{
  "llm": {
    "enable_semantic": true,
    "key": "YOUR_OPENAI_KEY",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "semantic_flow": "two_step",
    "timeout_secs": 60,
    "temperature": 0.1,
    "max_output_tokens": 900,
    "analysis_max_output_tokens": 700,
    "max_input_chars": 12000,
    "max_chunks": 12,
    "retry_max_attempts": 3,
    "retry_backoff_ms": 600,
    "agent_loop_max_rounds": 2,
    "agent_reflection_enabled": true,
    "agent_min_chunk_coverage": 0.6
  }
}
```

### 2) 全局配置（可选）

默认路径为 `~/.kb-cli/config.json`，或用环境变量 `KB_GLOBAL_CONFIG` 指向自定义路径。

```json
{
  "llm": {
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1"
  }
}
```

### 3) 环境变量覆盖（最高优先级）

PowerShell 示例：

```powershell
$env:KB_LLM_KEY="YOUR_OPENAI_KEY"
$env:KB_LLM_MODEL="gpt-4o-mini"
$env:KB_LLM_BASE_URL="https://api.openai.com/v1"
$env:KB_LLM_ENABLE_SEMANTIC="1"
```

当三层同时存在时，最终生效顺序是：  
`环境变量 > 项目 .kb/config.json > 全局 config.json`。

### 4) 多文件记忆编译验证

```bash
# 1. 初始化
kb init demo

# 2. 添加多文件
kb add ./docs/a.md a
kb add ./docs/b.md b
kb add ./docs/c.md c

# 3. 触发编译
kb update
```

验证点：

- `kb update` 返回 `semantic_extractor.mode = "llm"`。
- 输出中包含 `semantic_flow`、`agent_loop_max_rounds`、`agent_min_chunk_coverage` 等字段。
- 产物文件存在：`./.kb/knowledge.json`、`./.kb/search.json`、`./.kb/graph.json`、`./.kb/wiki/`。

### 5) 本仓库开发测试命令

```bash
cargo fmt --all
cargo test --workspace
```

## 文档

- 规范：`docs/cli/kb-md-spec.md`
- 手册：`docs/cli/command-manual.md`
- 示例：`docs/cli/command-examples.md`
- 错误目录：`docs/cli/error-catalog.md`

## 说明

- 当前 `kb ask` 使用本地检索流程，`engine_status = "disabled"`。
- JSON 输出是稳定的机器契约；text 输出面向人类阅读。


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
