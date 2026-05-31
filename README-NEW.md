# KB-CLI

> 本地知识库命令行工具 + 可插拔 Agent 记忆系统

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-production--ready-green.svg)]()

KB-CLI 是一个本地知识库命令行工具，用于把分散文件声明、索引、语义记忆和检索结果组织成可持续维护的知识库。同时提供**可插拔的外部 agent 记忆系统**，让任何支持 MCP 的 agent（Claude Desktop / Cursor / Cline）都能接入。

---

## 📑 目录

- [为什么选择 KB-CLI](#为什么选择-kb-cli)
- [快速开始](#快速开始)
- [核心功能](#核心功能)
- [Agent 记忆系统](#agent-记忆系统)
- [架构概览](#架构概览)
- [命令参考](#命令参考)
- [配置说明](#配置说明)
- [文档](#文档)
- [常见问题](#常见问题)

---

## 为什么选择 KB-CLI

### 🎯 核心优势

1. **本地优先，隐私可控**
   - 所有数据存储在本地（JSON 或 SQLite）
   - 无需上传到云端，完全掌控你的知识库
   - 可选择性启用 LLM 增强（支持任何 OpenAI-compatible API）

2. **分层记忆架构（L0-L5）**
   - L0 Episode：原始交互时间线（append-only）
   - L1 Index：文件索引
   - L2 Knowledge：语义记忆
   - L3 Search：检索索引
   - L4 Graph：知识图谱
   - L5 Wiki：可读视图

3. **可插拔 Agent 记忆系统**
   - 通过 MCP (Model Context Protocol) 暴露 8 个 tools
   - 支持 Claude Desktop / Cursor / Cline 等任何 MCP 兼容 agent
   - 多 agent 共享记忆，通过 `KB_AGENT_ID` 隔离

4. **反幻觉设计**
   - `verify_citation` 词法验证引用真实性
   - `UserCorrection` 最高优先级（confidence 1.0）
   - Provenance 链保留历史，冲突可见不覆写

5. **轻量无依赖**
   - 不依赖向量数据库（保持"无 LLM 也能跑"）
   - 手写 stdio JSON-RPC（零额外依赖）
   - 单二进制文件，开箱即用

### 🔄 典型使用场景

- **个人知识管理**：管理笔记、文档、代码片段
- **项目文档索引**：为代码项目建立可检索的知识库
- **Agent 外挂大脑**：让 AI agent 拥有持久化记忆
- **团队知识共享**：通过 git 同步知识库配置（`.kb/config.json` 可选 gitignore）

---

## 快速开始

### 安装

#### 方式 1：从 Release 下载（推荐）

1. 访问 [Releases](https://github.com/your-repo/kb-cli/releases) 页面
2. 下载对应平台的安装包：
   - Windows: `kb-cli-<version>-windows-x86_64.zip`
   - Linux: `kb-cli-<version>-linux-x86_64.tar.gz`
   - macOS: `kb-cli-<version>-darwin-x86_64.tar.gz`

3. 解压并安装：
   ```bash
   # Windows
   .\install.cmd
   
   # Linux/macOS
   bash ./install.sh
   ```

4. 验证安装：
   ```bash
   kb --version
   kb --help
   ```

#### 方式 2：从源码构建

```bash
# 克隆仓库
git clone https://github.com/your-repo/kb-cli.git
cd kb-cli

# 构建 release 版本
cargo build --release

# 二进制文件位于 target/release/kb-cli
```

### 5 分钟上手

```bash
# 1. 初始化知识库
kb init my-project
cd my-project

# 2. 添加文件到知识库
echo "# 测试文档" > test.md
kb add test.md

# 3. 更新索引
kb update

# 4. 检索知识
kb ask "测试文档的内容是什么"

# 5. (可选) 启用 Agent 记忆系统
export KB_AGENT_ID="my-agent"
kb remember "这是一个测试项目"
kb consolidate
```

### Agent 记忆系统快速集成

```bash
# 1. 启动 MCP server
kb serve --mcp

# 2. 配置 Claude Desktop (Windows)
# 编辑 %APPDATA%\Claude\claude_desktop_config.json
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

# 3. 重启 Claude Desktop，agent 会自动看到 8 个 tools
```

---

## 核心功能

### 📚 知识库管理

| 命令 | 功能 | 示例 |
|------|------|------|
| `kb init [name]` | 初始化知识库 | `kb init my-kb` |
| `kb add <path> [alias]` | 添加文件声明 | `kb add docs/a.md` |
| `kb rm <alias>` | 删除声明 | `kb rm docs/a` |
| `kb mv <alias> <new>` | 重命名声明 | `kb mv old new` |
| `kb list` | 列出登记项 | `kb list` |
| `kb check` | 检查差异 | `kb check` |
| `kb update` | 更新索引 | `kb update --llm` |

### 🔍 检索与问答

```bash
# 默认检索模式（返回上下文）
kb ask "项目如何更新索引"

# 启用 LLM 回答模式
kb ask "项目如何更新索引" --answer

# JSON 输出
kb ask "项目如何更新索引" --output json
```

### 🧠 Agent 记忆系统

```bash
# 记录观察（confidence 0.8）
kb remember "项目使用 Rust workspace"

# 记录用户纠错（confidence 1.0，最高优先级）
kb correct "存储后端默认是 fs"

# 固化记忆（dry-run 预览）
kb consolidate --dry-run

# 固化记忆（实际写入）
kb consolidate

# 启动 MCP server
kb serve --mcp
```

### 🛠️ 环境管理

```bash
# 列出已登记知识库
kb env --list

# 诊断配置和 keyring
kb env --doctor

# 生成 shell 补全
kb completion bash > ~/.kb-completion.bash
```

---

## Agent 记忆系统

KB-CLI 实现了一个**可插拔的外部 agent 记忆系统**，让任何 agent（Claude Desktop / Cursor / Cline / 自定义 agent）都能把这个项目当作外挂大脑使用。

### 🎯 设计理念

1. **反幻觉优先**
   - `verify_citation` 词法验证
   - `UserCorrection` 最高优先级
   - Provenance 链保留历史

2. **轻量无依赖**
   - 无向量嵌入
   - 手写 JSON-RPC
   - Append-only 存储

3. **多 agent 共享**
   - 通过 `KB_AGENT_ID` 隔离
   - `consolidate` 聚合贡献

4. **前沿研究**
   - 借鉴 arXiv:2404.13501
   - Mem0 冲突检测
   - Zep/Graphiti 双时态标注

### 📊 记忆分层模型（L0-L5）

```
L0 Episode (时间线)
  ↓ kb consolidate
.kb/raw/agent-memory/*.md (语义记忆文件)
  ↓ kb update
L1 Index → L2 Knowledge → L3 Search → L4 Graph → L5 Wiki
```

| 层级 | 存储位置 | 功能 |
|------|---------|------|
| L0 | `.kb/episodes.jsonl` | 原始交互时间线 |
| L1 | `.kb/index.json` | 文件索引 |
| L2 | `.kb/knowledge.json` | 语义记忆 |
| L3 | `.kb/search.json` | 检索索引 |
| L4 | `.kb/graph.json` | 知识图谱 |
| L5 | `.kb/wiki/*.md` | Wiki 视图 |

### 🔌 MCP 集成

#### 8 个可用 Tools

| Tool | 功能 | 用途 |
|------|------|------|
| `recall` | 检索记忆 | 分层检索，返回 ranked contexts |
| `remember` | 写入观察 | L0 episode，kind=Observation |
| `correct` | 写入纠错 | L0 episode，kind=UserCorrection |
| `verify_citation` | 验证引用 | 词法校验 quote 真实性 |
| `list_episodes` | 列出时间线 | 按 agent_id 过滤 |
| `consolidate` | 固化记忆 | 支持 dry_run 预览 |
| `read_summary` | 读取摘要 | L2 语义摘要 |
| `read_neighbors` | 读取图谱 | L4 邻接关系 |

#### 配置示例

<details>
<summary><b>Claude Desktop (Windows)</b></summary>

编辑 `%APPDATA%\Claude\claude_desktop_config.json`：

```json
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
```
</details>

<details>
<summary><b>Claude Desktop (macOS)</b></summary>

编辑 `~/Library/Application Support/Claude/claude_desktop_config.json`：

```json
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
```
</details>

<details>
<summary><b>Cursor</b></summary>

在项目根目录创建 `.cursorrules`：

```json
{
  "mcpServers": {
    "kb-memory": {
      "command": "kb",
      "args": ["serve", "--mcp"],
      "env": {
        "KB_AGENT_ID": "cursor"
      }
    }
  }
}
```
</details>

<details>
<summary><b>Cline (VSCode)</b></summary>

在 VSCode `settings.json` 中添加：

```json
{
  "cline.mcpServers": {
    "kb-memory": {
      "command": "kb",
      "args": ["serve", "--mcp"],
      "env": {
        "KB_AGENT_ID": "cline"
      }
    }
  }
}
```
</details>

#### 环境变量

- `KB_AGENT_ID`：标识调用方 agent（默认 `kb-cli`）
- `KB_SESSION_ID`：会话 ID（可选）

---

## 架构概览

KB-CLI 是一个 Rust workspace，遵循**六边形架构**（ports/adapters）：

```
┌─────────────────────────────────────────────┐
│              kb-cli (binary)                │
│         CLI 表层 + 命令编排                  │
└─────────────────┬───────────────────────────┘
                  │
    ┌─────────────┴─────────────┐
    │                           │
┌───▼────────┐          ┌───────▼──────┐
│  kb-core   │          │   kb-llm     │
│ 领域模型    │          │ LLM 适配器   │
│ Use Cases  │          │ + keyring    │
│  Ports     │          └──────────────┘
└───┬────────┘
    │
    ├──────────┬──────────┬──────────┐
    │          │          │          │
┌───▼────┐ ┌──▼──────┐ ┌─▼────────┐ ┌▼──────────┐
│kb-     │ │kb-      │ │kb-index- │ │kb-storage-│
│storage-│ │storage- │ │memory    │ │sqlite     │
│fs      │ │sqlite   │ │          │ │           │
└────────┘ └─────────┘ └──────────┘ └───────────┘
```

### 模块说明

- **kb-core**: 领域模型 + use cases + ports（不依赖具体存储或网络）
- **kb-storage-fs**: JSON 文件后端（便于人工查看和 diff）
- **kb-storage-sqlite**: SQLite 后端（原子性、并发、大仓库）
- **kb-llm**: OpenAI-compatible HTTP 适配器 + keyring 集成
- **kb-index-memory**: 内存中的分层检索器
- **kb-cli**: 唯一的二进制 crate（CLI 表层、命令编排）

### 存储后端切换

在 `.kb/config.json` 中配置：

```json
{
  "storage": { "backend": "fs" }  // 或 "sqlite"
}
```

- **fs**（默认）：所有数据存储为 JSON 文件，便于查看和版本控制
- **sqlite**：所有数据存储在 `.kb/store.sqlite3`，适合大仓库和并发场景

---

## 命令参考

### 完整命令列表

<details>
<summary><b>知识库管理</b></summary>

```bash
kb init [name]                    # 初始化知识库
kb add <path> [alias]             # 添加文件声明
kb rm <alias>                     # 删除声明
kb mv <alias> <new_alias>         # 重命名声明
kb path <alias> [new_path]        # 查看或修改路径
kb chmod <alias|category> <perms> # 修改权限
kb list                           # 列出登记项
kb check                          # 检查差异
kb update [--llm] [--strict]      # 更新索引
```
</details>

<details>
<summary><b>检索与问答</b></summary>

```bash
kb ask <question>                 # 检索模式
kb ask <question> --answer        # 回答模式
kb ask <question> --output json   # JSON 输出
kb ask <question> --top 10        # 返回 top 10 结果
```
</details>

<details>
<summary><b>Agent 记忆系统</b></summary>

```bash
kb remember <content>             # 记录观察
kb correct <content>              # 记录纠错
kb consolidate [--dry-run]        # 固化记忆
kb serve --mcp                    # 启动 MCP server
```
</details>

<details>
<summary><b>环境管理</b></summary>

```bash
kb env --list                     # 列出知识库
kb env --doctor                   # 诊断配置
kb completion <shell>             # 生成补全脚本
```
</details>

### 常用选项

```bash
--output <text|json>              # 输出格式
--quiet                           # 静默模式
--verbose, -v, -vv                # 详细日志
--config <path>                   # 指定配置文件
--no-color                        # 禁用颜色
```

---

## 配置说明

### 配置文件位置

KB-CLI 读取两份配置：

1. **项目级**：`./.kb/config.json`（由 `kb init` 生成）
2. **全局**：`~/.kb-cli/config.json`（可通过 `KB_GLOBAL_CONFIG` 覆盖）

### 配置优先级

```
环境变量 > 项目配置 > 全局配置 > 默认值
```

### LLM 配置

#### 密钥管理（推荐方式）

1. **环境变量**（最高优先级）
   ```bash
   export KB_LLM_KEY="sk-..."
   export KB_LLM_MODEL="gpt-4o-mini"
   export KB_LLM_BASE_URL="https://api.openai.com/v1"
   ```

2. **系统 keyring**（推荐）
   ```bash
   # KB-CLI 会自动从 keyring 读取
   # service: kb-cli
   # user: default
   ```

3. **配置文件**（不推荐，容易泄露）
   ```json
   {
     "llm": {
       "key": "sk-...",  // 不推荐写在这里
       "model": "gpt-4o-mini",
       "base_url": "https://api.openai.com/v1"
     }
   }
   ```

#### 启用 LLM 增强

```bash
# 环境变量方式
export KB_LLM_ENABLE_SEMANTIC=1
export KB_LLM_ENABLE_GRAPH=1
export KB_LLM_ENABLE_WIKI=1

# 或在配置文件中
{
  "llm": {
    "enable_semantic": true,
    "enable_graph": true,
    "enable_wiki": true
  }
}

# 或使用 --llm 标志（一次性启用）
kb update --llm
```

### 完整配置示例

<details>
<summary><b>查看完整配置示例</b></summary>

```json
{
  "llm": {
    "enable_semantic": true,
    "enable_graph": true,
    "enable_wiki": true,
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "semantic_flow": "two_step",
    "max_parallel": 4,
    "timeout_ms": 30000,
    "max_input_chars": 12000,
    "max_chunks": 12,
    "retry_max_attempts": 3,
    "retry_backoff_ms": 600,
    "agent_loop_max_rounds": 2,
    "agent_reflection_enabled": true,
    "agent_min_chunk_coverage": 0.6
  },
  "storage": {
    "backend": "fs"
  }
}
```
</details>

---

## 文档

### 📖 用户文档

- [MCP 集成指南](docs/mcp-integration-guide.md) - 详细的 MCP 配置和使用说明
- [命令手册](docs/cli/command-manual.md) - 完整的命令参考
- [命令示例](docs/cli/command-examples.md) - 实际使用示例
- [错误目录](docs/cli/error-catalog.md) - 错误码说明

### 🏗️ 架构文档

- [记忆模型](docs/architecture/memory-model.md) - L0-L5 分层架构设计
- [CLI 架构](docs/architecture/cli-architecture.md) - 六边形架构说明
- [后端集成](docs/architecture/backend-agent-integration.md) - 自定义后端接入

### 🔧 开发者文档

- [开发者工作流](docs/cli/developer-workflow.md) - 编译、测试、打包
- [安装与打包](docs/cli/install-and-package.md) - 安装脚本说明

---

## 常见问题

<details>
<summary><b>Q: KB-CLI 需要联网吗？</b></summary>

A: 不需要。KB-CLI 的核心功能（索引、检索）完全本地运行。只有在启用 LLM 增强功能时才需要访问 LLM API。
</details>

<details>
<summary><b>Q: 如何在不同设备间同步知识库？</b></summary>

A: 可以通过 git 同步知识库。建议将 `.kb/config.json` 加入 `.gitignore`（避免泄露密钥），只同步 `kb.md` 和 `.kb/raw/` 目录。
</details>

<details>
<summary><b>Q: 支持哪些 LLM 提供商？</b></summary>

A: 支持任何 OpenAI-compatible API，包括：
- OpenAI
- DeepSeek
- Anthropic (通过兼容层)
- 本地模型（Ollama、LM Studio 等）
</details>

<details>
<summary><b>Q: MCP server 如何停止？</b></summary>

A: 按 `Ctrl+C` 或发送 EOF 信号即可停止 MCP server。
</details>

<details>
<summary><b>Q: 如何查看当前使用的配置？</b></summary>

A: 运行 `kb env --doctor` 可以查看当前生效的配置和 keyring 状态。
</details>

<details>
<summary><b>Q: Episode 数据会占用多少空间？</b></summary>

A: Episode 使用 append-only 存储（JSONL 或 sqlite），每条 episode 约 200-500 字节。1000 条 episode 约 200-500 KB。
</details>

<details>
<summary><b>Q: 如何清理旧的 episodes？</b></summary>

A: 目前需要手动删除 `.kb/episodes.jsonl` 或清理 sqlite 数据库。未来版本会添加自动清理功能。
</details>

<details>
<summary><b>Q: 支持多语言吗？</b></summary>

A: 是的。KB-CLI 支持任何 UTF-8 编码的文本文件，包括中文、日文、韩文等。
</details>

---

## 贡献

欢迎贡献！请查看 [CONTRIBUTING.md](CONTRIBUTING.md)（待添加）了解详情。

### 开发环境

```bash
# 克隆仓库
git clone https://github.com/your-repo/kb-cli.git
cd kb-cli

# 安装依赖
cargo build

# 运行测试
cargo test

# 运行集成测试
bash test-mcp-basic.sh
```

---

## 许可证

[MIT License](LICENSE)

---

## 致谢

KB-CLI 的设计借鉴了以下前沿研究和项目：

- [arXiv:2404.13501](https://arxiv.org/abs/2404.13501) - Agent Memory Taxonomy
- [Mem0](https://github.com/mem0ai/mem0) - Conflict Detection
- [Zep](https://github.com/getzep/zep) / [Graphiti](https://github.com/getzep/graphiti) - Bi-temporal Stamping
- [MemGPT](https://github.com/cpacker/MemGPT) / [Letta](https://github.com/letta-ai/letta) - Layered Memory
- [Model Context Protocol](https://modelcontextprotocol.io) - MCP Specification

---

<p align="center">
  Made with ❤️ by the KB-CLI Team
</p>
