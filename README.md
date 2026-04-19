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

## 文档

- 规范：`docs/cli/kb-md-spec.md`
- 手册：`docs/cli/command-manual.md`
- 示例：`docs/cli/command-examples.md`
- 错误目录：`docs/cli/error-catalog.md`

## 说明

- 当前 `kb ask` 使用本地检索流程，`engine_status = "disabled"`。
- JSON 输出是稳定的机器契约；text 输出面向人类阅读。
