# KB-CLI 文档中心

欢迎来到 KB-CLI 文档中心！这里汇集了所有相关文档，帮助你快速上手和深入了解 KB-CLI。

---

## 📚 快速导航

### 🚀 新手入门

- [README](../README.md) - 项目概览和快速开始
- [安装与打包指南](cli/install-and-package.md) - Release 安装、源码构建、打包脚本与发布流程
- [5 分钟教程](#5-分钟教程) - 快速上手指南

### 📖 用户指南

#### 命令参考
- [命令手册](cli/command-manual.md) - 完整的命令参考文档
- [命令示例](cli/command-examples.md) - 实际使用示例和输出格式
- [错误目录](cli/error-catalog.md) - 错误码说明和解决方案

#### 功能说明
- [MCP 集成指南](mcp-integration-guide.md) - Agent 记忆系统集成
- [kb.md 规范](cli/kb-md-spec.md) - 知识库声明文件格式
- [.kbignore 规范](cli/kbignore-spec.md) - 忽略文件配置

### 🏗️ 架构文档

- [记忆模型](architecture/memory-model.md) - L0-L5 分层架构设计
- [CLI 架构](architecture/cli-architecture.md) - 六边形架构说明
- [后端集成](architecture/backend-agent-integration.md) - 自定义后端接入
- [Wiki 视图层](architecture/wiki-markdown-view-layer.md) - L5 Wiki 层设计

### 🔧 开发者文档

- [开发者工作流](cli/developer-workflow.md) - 编译、测试、打包、发布
- [状态差异契约](cli/state-diff-contract.md) - 状态管理规范

### 📊 项目报告与历史资料

- [Phase 4 完成报告](phase-4-completion-report.md) - Agent 记忆系统实现总结
- [文档工作日志](history/documentation-work-log.md) - 文档优化与清理的合并纪要（历史记录）
- [superpowers 规格与计划](superpowers/) - 各阶段设计规格与实施计划（历史文档，按当时状态保留）

---

## 🎯 按主题浏览

### Agent 记忆系统

Agent 记忆系统是 KB-CLI 的核心特性之一，让任何 agent 都能拥有持久化记忆。

**相关文档：**
- [MCP 集成指南](mcp-integration-guide.md) - 如何配置 Claude Desktop / Cursor / Cline
- [记忆模型](architecture/memory-model.md) - L0 Episode Layer 设计
- [命令手册 - 记忆命令](cli/command-manual.md#411-kb-remember-content) - remember / correct / consolidate

**快速开始：**
```bash
# 1. 记录观察
kb remember "项目使用 Rust workspace"

# 2. 记录纠错
kb correct "存储后端默认是 fs"

# 3. 固化记忆
kb consolidate

# 4. 启动 MCP server
kb serve --mcp
```

### 知识库管理

管理你的知识源，包括添加、删除、更新和检索。

**相关文档：**
- [命令手册 - 知识库管理](cli/command-manual.md#41-kb-init-name) - init / add / rm / update
- [kb.md 规范](cli/kb-md-spec.md) - 声明文件格式
- [命令示例](cli/command-examples.md) - 实际使用示例

**快速开始：**
```bash
# 1. 初始化
kb init my-project

# 2. 添加文件
kb add docs/

# 3. 更新索引
kb update

# 4. 检索
kb ask "如何使用"
```

### LLM 集成

启用 LLM 增强功能，提升语义理解和知识图谱质量。

**相关文档：**
- [README - LLM 配置](../README.md#llm-配置可选) - 配置说明
- [记忆模型 - LLM 接入](architecture/memory-model.md#6-llm-接入策略与配置优先级) - 接入策略

**快速开始：**
```bash
# 1. 设置环境变量
export KB_LLM_KEY="sk-..."
export KB_LLM_ENABLE_SEMANTIC=1

# 2. 更新索引（启用 LLM）
kb update --llm

# 3. 检查状态
kb env --doctor
```

### 存储后端

选择合适的存储后端（fs 或 sqlite）。

**相关文档：**
- [README - 架构（含存储后端切换）](../README.md) - 配置说明（`.kb/config.json` 的 `storage.backend`，取值 `fs` / `sqlite`）
- [CLI 架构](architecture/cli-architecture.md) - 架构设计（历史文档，含时点标注）

**快速开始：**
```json
// .kb/config.json
{
  "storage": { "backend": "sqlite" }
}
```

---

## 📋 文档列表

### CLI 文档 (`docs/cli/`)

| 文档 | 描述 |
|------|------|
| [command-manual.md](cli/command-manual.md) | 完整的命令参考手册 |
| [command-examples.md](cli/command-examples.md) | 命令使用示例和输出格式 |
| [error-catalog.md](cli/error-catalog.md) | 错误码目录和解决方案 |
| [developer-workflow.md](cli/developer-workflow.md) | 开发者工作流程 |
| [install-and-package.md](cli/install-and-package.md) | 安装和打包说明 |
| [kb-md-spec.md](cli/kb-md-spec.md) | kb.md 文件格式规范 |
| [kbignore-spec.md](cli/kbignore-spec.md) | .kbignore 文件规范 |
| [state-diff-contract.md](cli/state-diff-contract.md) | 状态差异契约 |

### 架构文档 (`docs/architecture/`)

| 文档 | 描述 |
|------|------|
| [memory-model.md](architecture/memory-model.md) | L0-L5 分层记忆模型设计 |
| [cli-architecture.md](architecture/cli-architecture.md) | CLI 六边形架构说明 |
| [backend-agent-integration.md](architecture/backend-agent-integration.md) | 后端和 Agent 集成指南 |
| [wiki-markdown-view-layer.md](architecture/wiki-markdown-view-layer.md) | L5 Wiki 视图层设计 |
| [memory-model-implementation-plan.md](architecture/memory-model-implementation-plan.md) | 记忆模型实现计划 |

### 其他文档 (`docs/`)

| 文档 | 描述 |
|------|------|
| [mcp-integration-guide.md](mcp-integration-guide.md) | MCP 集成完整指南 |
| [phase-4-completion-report.md](phase-4-completion-report.md) | Phase 4 完成报告 |

### 历史文档 (`docs/history/`、`docs/superpowers/`)

| 目录 / 文档 | 描述 |
|------|------|
| [history/documentation-work-log.md](history/documentation-work-log.md) | 文档优化与项目清理工作日志（由 4 份历史报告合并而来） |
| [superpowers/specs/](superpowers/specs/) | 历史设计规格（2026-05，L2-L5 / 记忆模型 / wiki 视图等，按当时状态保留，不再随代码更新） |
| [superpowers/plans/](superpowers/plans/) | 历史实施计划（2026-05，与上述规格配套，已执行完毕） |

---

## 🔍 常见任务

### 如何...

<details>
<summary><b>初始化一个新的知识库？</b></summary>

```bash
kb init my-project
cd my-project
```

参考：[命令手册 - kb init](cli/command-manual.md#41-kb-init-name)
</details>

<details>
<summary><b>添加文件到知识库？</b></summary>

```bash
# 添加单个文件
kb add docs/readme.md

# 添加整个目录
kb add docs/

# 添加并指定别名
kb add docs/readme.md my-readme
```

参考：[命令手册 - kb add](cli/command-manual.md#42-kb-add-path-alias)
</details>

<details>
<summary><b>配置 LLM？</b></summary>

```bash
# 方式 1: 环境变量（推荐）
export KB_LLM_KEY="sk-..."
export KB_LLM_MODEL="gpt-4o-mini"

# 方式 2: 配置文件
# 编辑 .kb/config.json
{
  "llm": {
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1"
  }
}
```

参考：[README - LLM 配置](../README.md#llm-配置可选)
</details>

<details>
<summary><b>集成到 Claude Desktop？</b></summary>

1. 编辑 `%APPDATA%\Claude\claude_desktop_config.json`
2. 添加 MCP server 配置
3. 重启 Claude Desktop

参考：[MCP 集成指南](mcp-integration-guide.md#claude-desktop)
</details>

<details>
<summary><b>排查错误？</b></summary>

```bash
# 1. 查看详细日志
kb update -vv

# 2. 诊断配置
kb env --doctor

# 3. 查看错误码说明
```

参考：[错误目录](cli/error-catalog.md)
</details>

---

## 🆘 获取帮助

### 命令行帮助

```bash
# 查看所有命令
kb --help

# 查看特定命令帮助
kb add --help
kb update --help
```

### 文档搜索

使用 KB-CLI 自身来搜索文档：

```bash
# 初始化文档知识库
kb init kb-docs
cd kb-docs
kb add /path/to/kb-cli/docs/

# 搜索文档
kb ask "如何配置 MCP"
kb ask "什么是 L0 Episode Layer"
```

### 社区支持

- **GitHub Issues**: [报告 Bug 或提出功能请求](https://github.com/your-repo/kb-cli/issues)
- **Discussions**: [参与讨论](https://github.com/your-repo/kb-cli/discussions)

---

## 📝 5 分钟教程

### 场景：为代码项目建立知识库

```bash
# 1. 进入项目目录
cd my-project

# 2. 初始化知识库
kb init my-project-kb

# 3. 添加文档
kb add README.md
kb add docs/

# 4. 更新索引
kb update

# 5. 测试检索
kb ask "项目如何安装"

# 6. (可选) 启用 LLM 增强
export KB_LLM_KEY="sk-..."
kb update --llm

# 7. (可选) 启用 Agent 记忆
export KB_AGENT_ID="my-agent"
kb remember "这是一个 Rust 项目"
kb consolidate
```

### 场景：集成到 Claude Desktop

```bash
# 1. 确保 kb 在 PATH 中
kb --version

# 2. 编辑 Claude Desktop 配置
# Windows: %APPDATA%\Claude\claude_desktop_config.json
# macOS: ~/Library/Application Support/Claude/claude_desktop_config.json

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

# 3. 重启 Claude Desktop

# 4. 在对话中测试
# Agent 会自动看到 8 个 tools (recall, remember, correct, etc.)
```

---

## 🔄 文档更新日志

- **2026-07-27**: 文档一致性修复（episode id / `search.json` / `regs.json`），重写安装与打包指南，历史优化报告合并至 `docs/history/`
- **2026-05-31**: Phase 4 完成，添加 MCP 集成指南和完成报告
- **2026-05-02**: 添加记忆模型架构文档
- **2026-04-19**: 更新命令手册和示例
- **2026-04-18**: 添加 kb.md 和 .kbignore 规范

---

## 📧 反馈

发现文档问题或有改进建议？欢迎：

- [提交 Issue](https://github.com/your-repo/kb-cli/issues/new)
- [编辑文档](https://github.com/your-repo/kb-cli/edit/main/docs/)
- [参与讨论](https://github.com/your-repo/kb-cli/discussions)

---

<p align="center">
  <a href="../README.md">返回主页</a> •
  <a href="mcp-integration-guide.md">MCP 集成</a> •
  <a href="cli/command-manual.md">命令手册</a> •
  <a href="architecture/memory-model.md">架构设计</a>
</p>
