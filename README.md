# KB-CLI

> 本地知识库 CLI + 可插拔 Agent 记忆系统

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-production--ready-green.svg)]()

**KB-CLI** 是一个 Rust 实现的本地知识库命令行工具，提供分层记忆架构（L0-L5）和可插拔的 Agent 记忆系统。

---

## ✨ 核心特性

- 🗂️ **本地优先**：所有数据存储在本地（JSON/SQLite），隐私可控
- 🧠 **分层记忆**：L0-L5 六层架构，从原始时间线到知识图谱
- 🔌 **MCP 集成**：通过 Model Context Protocol 暴露 8 个 tools
- 🛡️ **反幻觉**：词法验证、用户纠错优先、provenance 链
- ⚡ **轻量高效**：单二进制，无向量数据库依赖

---

## 🚀 快速开始

### 安装

```bash
# 从 Release 下载对应平台的安装包
# Windows: kb-cli-<version>-windows-x86_64.zip
# Linux: kb-cli-<version>-linux-x86_64.tar.gz

# 解压并安装
./install.sh  # Linux
.\install.cmd # Windows

# 验证
kb --version
```

### 5 分钟上手

```bash
# 1. 初始化知识库
kb init my-kb
cd my-kb

# 2. 添加文件
echo "# 测试文档" > test.md
kb add test.md

# 3. 更新索引
kb update

# 4. 检索
kb ask "测试文档的内容"

# 5. Agent 记忆（可选）
export KB_AGENT_ID="my-agent"
kb remember "这是一个测试项目"
kb consolidate
```

---

## 📚 核心功能

### 知识库管理

```bash
kb init [name]              # 初始化
kb add <path>               # 添加文件
kb update [--llm]           # 更新索引
kb ask <question>           # 检索
kb list                     # 列出文件
```

### Agent 记忆系统

```bash
kb remember <content>       # 记录观察
kb correct <content>        # 记录纠错（最高优先级）
kb consolidate              # 固化记忆
kb serve --mcp              # 启动 MCP server
```

**MCP Tools（8 个）：**
`recall` | `remember` | `correct` | `verify_citation` | `list_episodes` | `consolidate` | `read_summary` | `read_neighbors`

---

## 🧠 Agent 记忆系统

### 分层架构

```
L0 Episode (时间线) → kb consolidate
  ↓
.kb/raw/agent-memory/*.md (语义记忆)
  ↓ kb update
L1 Index → L2 Knowledge → L3 Search → L4 Graph → L5 Wiki
```

### MCP 集成

**启动 MCP Server：**
```bash
kb serve --mcp
```

**配置 Claude Desktop（Windows）：**
```json
{
  "mcpServers": {
    "kb-memory": {
      "command": "kb",
      "args": ["serve", "--mcp"],
      "env": { "KB_AGENT_ID": "claude-desktop" }
    }
  }
}
```

配置文件位置：`%APPDATA%\Claude\claude_desktop_config.json`

**其他 Agent：**
- **Cursor**: 在 `.cursorrules` 中配置
- **Cline**: 在 VSCode `settings.json` 中配置

详见：[MCP 集成指南](docs/mcp-integration-guide.md)

---

## 🏗️ 架构

```
kb-cli (binary)
    ↓
kb-core (领域模型)
    ↓
├─ kb-storage-fs (JSON 文件)
├─ kb-storage-sqlite (SQLite)
├─ kb-llm (LLM 适配器)
└─ kb-index-memory (检索器)
```

**存储后端切换：**
```json
// .kb/config.json
{ "storage": { "backend": "fs" } }  // 或 "sqlite"
```

---

## ⚙️ 配置

### LLM 配置（可选）

**环境变量（推荐）：**
```bash
export KB_LLM_KEY="sk-..."
export KB_LLM_MODEL="gpt-4o-mini"
export KB_LLM_ENABLE_SEMANTIC=1
```

**配置文件：**
```json
{
  "llm": {
    "enable_semantic": true,
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1"
  }
}
```

**优先级：** 环境变量 > 项目配置 > 全局配置

---

## 📖 文档

- **用户指南**
  - [MCP 集成指南](docs/mcp-integration-guide.md) - Agent 接入配置
  - [命令手册](docs/cli/command-manual.md) - 完整命令参考
  - [命令示例](docs/cli/command-examples.md) - 实际使用示例

- **架构文档**
  - [记忆模型](docs/architecture/memory-model.md) - L0-L5 设计
  - [CLI 架构](docs/architecture/cli-architecture.md) - 六边形架构

- **开发者**
  - [开发者工作流](docs/cli/developer-workflow.md) - 编译、测试、打包
  - [贡献指南](CONTRIBUTING.md) - 如何贡献

**完整文档索引：** [docs/README.md](docs/README.md)

---

## ❓ 常见问题

<details>
<summary><b>需要联网吗？</b></summary>

不需要。核心功能完全本地运行，只有启用 LLM 增强时才需要访问 API。
</details>

<details>
<summary><b>支持哪些 LLM？</b></summary>

支持任何 OpenAI-compatible API：OpenAI、DeepSeek、Anthropic（兼容层）、本地模型（Ollama、LM Studio）。
</details>

<details>
<summary><b>如何在设备间同步？</b></summary>

通过 git 同步。建议将 `.kb/config.json` 加入 `.gitignore`，只同步 `kb.md` 和 `.kb/raw/`。
</details>

<details>
<summary><b>MCP server 如何停止？</b></summary>

按 `Ctrl+C` 或发送 EOF 信号。
</details>

<details>
<summary><b>如何查看当前配置？</b></summary>

运行 `kb env --doctor` 查看生效的配置和 keyring 状态。
</details>

更多问题：[完整 FAQ](docs/README.md#常见任务)

---

## 🤝 贡献

欢迎贡献！请查看 [CONTRIBUTING.md](CONTRIBUTING.md)。

**开发环境：**
```bash
git clone https://github.com/your-repo/kb-cli.git
cd kb-cli
cargo build
cargo test
```

---

## 📄 许可证

[MIT License](LICENSE)

---

## 🙏 致谢

KB-CLI 借鉴了以下研究和项目：

- [arXiv:2404.13501](https://arxiv.org/abs/2404.13501) - Agent Memory Taxonomy
- [Mem0](https://github.com/mem0ai/mem0) - Conflict Detection
- [Zep](https://github.com/getzep/zep) / [Graphiti](https://github.com/getzep/graphiti) - Bi-temporal Stamping
- [MemGPT](https://github.com/cpacker/MemGPT) / [Letta](https://github.com/letta-ai/letta) - Layered Memory
- [Model Context Protocol](https://modelcontextprotocol.io) - MCP Specification

---

<p align="center">
  <a href="docs/README.md">📚 完整文档</a> •
  <a href="docs/mcp-integration-guide.md">🔌 MCP 集成</a> •
  <a href="CONTRIBUTING.md">🤝 贡献指南</a>
</p>
