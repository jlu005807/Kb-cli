# Developer Workflow

本文档面向开发者，包含仓库结构、编译测试、打包与发布流程。

## 1. 克隆与环境准备

```bash
git clone <repo-url>
cd Kb-cli

rustc --version
cargo --version
```

## 2. 仓库结构（简版）

```text
crates/
  kb-cli/          # CLI 入口、命令编排、运行时装配
  kb-core/         # 领域模型、端口、核心用例（L2-L5 编译等）
  kb-storage-fs/   # 文件系统存储实现
  kb-index-memory/ # 内存检索实现
  kb-llm/          # LLM 配置解析与 OpenAI-compatible L2/L4/L5 增强

docs/cli/          # 命令与安装文档
scripts/           # 打包脚本
packaging/         # 安装脚本模板（Windows/Linux）
```

## 3. 编译与运行

```bash
# 全工作区检查
cargo check --workspace

# 只构建 CLI
cargo build -p kb-cli

# 运行 CLI
cargo run -p kb-cli -- --help
cargo run -p kb-cli -- init demo
```

## 4. 测试与质量检查

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## 5. 本地安装（开发者可选）

```bash
# 默认安装位置（通常是 ~/.cargo/bin 或 %USERPROFILE%\\.cargo\\bin）
cargo install --path crates/kb-cli --force

# 安装到独立目录（推荐）
cargo install --path crates/kb-cli --force --root C:\Users\<you>\.kb-cli
```

## 6. LLM 链路快速自检（开发者）

配置优先级（高到低）：

1. 环境变量
2. 项目 `./.kb/config.json`
3. 全局 `~/.kb-cli/config.json`（或 `KB_GLOBAL_CONFIG`）

建议最小测试：

```bash
kb init demo
kb add ./docs/a.md docs/a
kb update --llm --output json
```

检查返回关键字段：

- `data.semantic_extractor.mode`
- `data.graph_builder.mode`
- `data.wiki_builder.mode`

## 7. 本地打包

Windows：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

Linux：

```bash
bash ./scripts/package.sh --target x86_64-unknown-linux-gnu
```

## 8. Release 发布（GitHub Actions）

工作流：`.github/workflows/release.yml`

建议流程：

```bash
git push origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

推送 main 后先确认对应提交的 CI 全部通过，再推送与 workspace 版本一致的标签。工作流创建 draft，构建并验收 Windows/Linux 安装包，两平台都成功后才公开 Release。有 `docs/releases/<tag>.md` 时使用该文件作为发布说明；否则生成提交说明。已有同名公开 Release 时拒绝覆盖。
