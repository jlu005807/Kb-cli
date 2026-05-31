# KB-CLI

KB-CLI 是一个本地知识库命令行工具，用于把分散文件声明、索引、语义记忆和检索结果组织成可持续维护的知识库。

## 项目作用

- 统一管理知识源声明（`kb.md` + `./.kb/raw/`）
- 持续增量更新索引与语义层（L1-L5）
- 提供可脚本化的命令行工作流（检查、更新、问答、环境查看）

## 核心功能

- 声明管理：`add` / `rm` / `mv` / `path` / `chmod`
- 差异检查：`check` 对比声明与登记状态
- 分层编译：`update` 生成并更新 `.kb/index*`、`.kb/knowledge.json`、`.kb/search.json`、`.kb/graph.json`、`.kb/wiki/`
- 本地问答：`ask` 基于分层检索返回上下文结果
- 环境管理：`env --list` 查看系统已登记知识库

## 架构概览

KB-CLI 是一个 Rust workspace，遵循六边形架构（ports/adapters）：

- `kb-core`：领域模型 + use cases + ports；不依赖具体存储或网络。
- `kb-storage-fs`：JSON 文件后端的 Index / Knowledge / Search / Graph / Wiki / State store。
- `kb-storage-sqlite`：等价的 SQLite 后端（schema 版本化、原子写入），通过相同 trait 与 core 对接。
- `kb-llm`：OpenAI-compatible HTTP 适配器 + keyring 集成 + 共享 tokio runtime（reqwest HTTP/2 池）。
- `kb-index-memory`：内存中的分层检索器实现。
- `kb-cli`：唯一的二进制 crate，负责 CLI 表层、命令编排、装配适配器。

### 存储后端切换

项目级 `./.kb/config.json` 支持：

```json
{
  "storage": { "backend": "fs" }
}
```

或：

```json
{
  "storage": { "backend": "sqlite" }
}
```

- `fs`（默认，未配置或字段拼错都走这条）：所有 `.kb/*.json` 与 `.kb/wiki/*.md` 直接落盘，便于人工查看与 diff。
- `sqlite`：所有快照写入 `./.kb/store.sqlite3`，适合需要更稳定原子性、并发读取或大仓库场景。
- 两个后端共用同一组 `*Store` trait，CLI 通过 `StorageHandles::open(cwd)` 路由，runtime 行为完全等价。

## 命令速览

### 知识库管理
- `kb init [name]`：初始化知识库目录与配置
- `kb add <path> [alias]`：添加声明
- `kb rm <alias>`：删除声明
- `kb mv <alias> <new_alias>`：重命名声明
- `kb path <alias> [new_path]`：查看或修改路径
- `kb chmod <alias|category> <perms>`：修改权限
- `kb list`：列出当前登记项
- `kb check`：检查声明与登记差异
- `kb update [--llm]`：执行分层更新（可用 `--llm` 强制启用本次 LLM 语义/图谱/wiki 构建）

### 检索与问答
- `kb ask <question>`：默认返回检索包；加 `--answer` 启用最小 LLM agent 最终回答

### Agent 记忆系统
- `kb remember <content>`：记录 agent 观察到的事实（写入 L0 episode 时间线）
- `kb correct <content>`：记录用户纠错（最高优先级记忆信号）
- `kb consolidate`：将积累的 episodes 固化为语义记忆文件（`.kb/raw/agent-memory/*.md`）
- `kb serve --mcp`：启动 MCP server，让外部 agent（Claude Desktop / Cursor / Cline）接入记忆系统

### 环境管理
- `kb env --list`：列出已登记知识库
- `kb env --doctor`：诊断配置与 keyring 状态
- `kb completion <shell>`：生成补全脚本

## 安装快速开始

1. 打开仓库 **Releases** 页面，下载与你系统匹配的安装包：
- Windows：`kb-cli-<version>-windows-x86_64.zip`
- Linux：`kb-cli-<version>-linux-x86_64.tar.gz`

2. 解压后安装：
- Windows：双击 `install.cmd`（或执行 `install.ps1`）
- Linux：执行 `bash ./install.sh`

Windows 默认安装位置：

- 可执行文件：`%USERPROFILE%\.kb-cli\bin\kb.exe`
- 全局配置：`%USERPROFILE%\.kb-cli\config.json`

3. 验证：

```bash
kb --version
kb --help
```

## 使用快速开始

```bash
# 1) 初始化
kb init demo

# 2) 添加文件声明
kb add ./docs/a.md docs/a

# 3) 检查差异
kb check

# 4) 生成/更新索引与记忆层
kb update

# 5) 发起问答
kb ask "这个项目如何更新索引？"
```

## LLM 配置说明（简版）

`kb init` 会自动生成：

- 项目级配置：`./.kb/config.json`
- 环境模板：`./.kb/.env.template`
- 结构说明：`./.kb/README.md`
- 全局配置：默认 `kb-cli` 可执行文件同目录 `config.json`（Windows 安装脚本默认对应 `%USERPROFILE%\.kb-cli\config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）

`.kb/README.md` 会说明 `.kb/index.json`、`.kb/knowledge.json`、`.kb/search.json`、`.kb/graph.json`、`.kb/wiki/` 等文件的含义，方便用户把自定义 agent 接到知识库上，而不必只能使用内置最小 agent。

LLM 生效优先级：

`环境变量 KB_LLM_KEY > 系统 keyring > 项目 .kb/config.json > 全局 config.json`

> 推荐使用系统 keyring 存储密钥（默认 service=`kb-cli`、user=`default`，可通过 `KB_LLM_KEYRING_SERVICE` / `KB_LLM_KEYRING_USER` 覆盖；设置 `KB_LLM_DISABLE_KEYRING=1` 可完全停用 keyring 路径）。`kb env --doctor` 会打印当前生效的 key 来源以及 keyring 探测结果。
>
> 从 #15 起，`kb init` 生成的 `./.kb/config.json` 默认不再写入 `key` 字段，并将该文件加入 `.gitignore`，避免误把明文密钥提交进仓库。

LLM 参与构建（`kb update`）：

- 满足 `KB_LLM_ENABLE_SEMANTIC=1` 且配置 `KB_LLM_KEY` 后，L2 语义编译将进入 `mode=llm`
- 可额外启用：
- `KB_LLM_ENABLE_GRAPH=1`：L4 图谱在规则构建后进入 LLM 增强
- `KB_LLM_ENABLE_WIKI=1`：L5 wiki 在规则构建后进入 LLM 增强
- `--llm` 会在本次 `update` 中同时尝试启用 semantic/graph/wiki 的 LLM 路径
- 默认使用 OpenAI-compatible Chat Completions（标准 `messages` 请求格式）
- 内置轻量 agent 风格原子工具上下文：
- `read_raw_file`：读取当前源文件原文片段
- `read_knowledge_graph`：读取 `.kb/graph.json` 关联关系
- `read_file_summary`：读取 `.kb/knowledge.json` 中历史摘要
- `read_graph_neighbors`：读取指定文件节点邻接关系（L4）
- `read_wiki_page`：读取 wiki 页面上下文（L5）

推荐项目配置（`./.kb/config.json`，不写 `key`，密钥走环境变量或 keyring）：

```json
{
  "llm": {
    "enable_semantic": true,
    "enable_graph": true,
    "enable_wiki": true,
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "semantic_flow": "two_step"
  }
}
```

并行编译调优：`KB_LLM_MAX_PARALLEL`（默认 4，钳制 `[1, 32]`）控制 L2 语义编译时的并发 LLM 请求数；大仓库且 provider 容忍度高时可以适当上调。底层使用共享 tokio runtime + Semaphore 限流，HTTP/2 连接由 reqwest 池复用。

快速验证：

```bash
kb update --llm --output json
```

检查返回字段：

- `data.semantic_extractor.mode = "llm"`
- `data.semantic_extractor.agent_tools.enabled = true`
- `data.semantic_extractor.agent_tools.tools` 包含上述 3 个工具名
- `data.graph_builder.mode`（`llm`/`rule`）
- `data.wiki_builder.mode`（`llm`/`rule`）
- `data.graph_builder.agent_tools.enabled`（L4 agent 原子工具上下文）
- `data.wiki_builder.agent_tools.enabled`（L5 agent 原子工具上下文）

如果未配置 `KB_LLM_KEY`，会自动回退规则路径，并在 `warnings` 中看到：

- `W-L2-LLM-CONFIG-MISSING`
- `W-L4-LLM-CONFIG-MISSING`
- `W-L5-LLM-CONFIG-MISSING`

## 配置参数完整参考（`config.json`）

KB-CLI 读取两份 JSON 配置：

- **项目级**：`./.kb/config.json`（由 `kb init` 写入推荐模板，被 `.gitignore` 排除）
- **全局**：`%USERPROFILE%\.kb-cli\config.json`（Linux/macOS：`~/.kb-cli/config.json`），可通过 `KB_GLOBAL_CONFIG` 覆盖路径

两份文件 schema 完全一致；同名字段按下面"优先级"小节合并。无效 JSON、缺失字段、类型不匹配都会**静默回退到默认值**而不是拒绝启动。

### `llm.*` —— LLM 适配器

| 字段 | 环境变量 | 类型 | 默认值 | 取值范围 / 说明 |
|---|---|---|---|---|
| `llm.key` | `KB_LLM_KEY` | string | — | OpenAI-compatible API key。**不建议**写在配置里；优先使用环境变量或 keyring。 |
| `llm.model` | `KB_LLM_MODEL` | string | `gpt-4o-mini` | 模型 ID。 |
| `llm.base_url` | `KB_LLM_BASE_URL` | string | `https://api.openai.com/v1` | OpenAI-compatible 端点（DashScope / Together / 本地 vLLM 等都可填）。 |
| `llm.enable_semantic` | `KB_LLM_ENABLE_SEMANTIC` | bool | `false` | 是否在 `kb update` 中启用 L2 语义编译（LLM 路径）。 |
| `llm.enable_graph` | `KB_LLM_ENABLE_GRAPH` | bool | `false` | 是否在规则构建后用 LLM 增强 L4 知识图谱。 |
| `llm.enable_wiki` | `KB_LLM_ENABLE_WIKI` | bool | `false` | 是否在规则构建后用 LLM 增强 L5 wiki。 |
| `llm.semantic_flow` | `KB_LLM_SEMANTIC_FLOW` | string | `two_step` | `two_step`（先分析后生成，推荐）或 `single_step`（一次生成）。非法值回退为 `two_step`。 |
| `llm.timeout_secs` | `KB_LLM_TIMEOUT_SECS` | u64 (秒) | `60` | 单次 HTTP 请求超时，下限 5。 |
| `llm.temperature` | `KB_LLM_TEMPERATURE` | f64 | `0.1` | 采样温度，钳制 `[0.0, 2.0]`。 |
| `llm.max_output_tokens` | `KB_LLM_MAX_OUTPUT_TOKENS` | u32 | `900` | 单次响应最大输出 token，下限 64。 |
| `llm.analysis_max_output_tokens` | `KB_LLM_ANALYSIS_MAX_OUTPUT_TOKENS` | u32 | `700` | `two_step` 分析阶段的输出 token 上限，下限 64。 |
| `llm.max_input_chars` | `KB_LLM_MAX_INPUT_CHARS` | usize | `12000` | 单次请求拼接的输入字符上限，下限 1200。 |
| `llm.max_chunks` | `KB_LLM_MAX_CHUNKS` | usize | `12` | 每次语义编译请求最多传入的 chunk 数量，下限 1。 |
| `llm.retry_max_attempts` | `KB_LLM_RETRY_MAX_ATTEMPTS` | usize | `3` | 暂态失败（429 / 5xx / 超时）的重试次数，钳制 `[1, 8]`。 |
| `llm.retry_backoff_ms` | `KB_LLM_RETRY_BACKOFF_MS` | u64 (毫秒) | `600` | 重试初始退避，上限 30000。 |
| `llm.agent_loop_max_rounds` | `KB_LLM_AGENT_LOOP_MAX_ROUNDS` | usize | `2` | L2/L4/L5 agent 风格细化循环最大轮数，钳制 `[1, 4]`（即使写更大也会被截到 4）。 |
| `llm.agent_reflection_enabled` | `KB_LLM_AGENT_REFLECTION_ENABLED` | bool | `true` | 是否启用 agent loop 自反思阶段。 |
| `llm.agent_min_chunk_coverage` | `KB_LLM_AGENT_MIN_CHUNK_COVERAGE` | f64 | `0.6` | 触发 agent 细化的 chunk 覆盖率阈值，钳制 `[0.0, 1.0]`。 |

> bool 解析接受 `1` / `true` / `yes` / `on`（真）和 `0` / `false` / `no` / `off`（假），都不区分大小写。

### `storage.*` —— 存储后端

| 字段 | 类型 | 默认值 | 取值 |
|---|---|---|---|
| `storage.backend` | string | `fs` | `fs` 或 `sqlite`。其它值（包括拼错）静默回退为 `fs`。 |

> 存储后端**只读项目级配置**，不接受环境变量覆盖。详见前文"存储后端切换"。

### 仅环境变量（不在 config.json 里）

| 环境变量 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `KB_LLM_MAX_PARALLEL` | usize | `4` | L2 并行编译的最大并发请求数，钳制 `[1, 32]`。 |
| `KB_LLM_KEYRING_SERVICE` | string | `kb-cli` | 系统 keyring 的 service 名（用于多环境隔离）。 |
| `KB_LLM_KEYRING_USER` | string | `default` | 系统 keyring 的 user 名。 |
| `KB_LLM_DISABLE_KEYRING` | bool | `false` | 设为 `1` / `true` 完全停用 keyring 路径（CI / 容器场景）。 |
| `KB_GLOBAL_CONFIG` | path | 见下 | 覆盖全局 config.json 文件路径。 |

> 全局 config 默认查找顺序：`KB_GLOBAL_CONFIG` → `kb` 可执行文件同目录下的 `config.json` → `%USERPROFILE%\.kb-cli\config.json`（或 Linux/macOS 下 `~/.kb-cli/config.json`）。

### 优先级（高 → 低）

对 `llm.key` 字段：

```
process env (KB_LLM_KEY)  >  系统 keyring  >  项目 .kb/config.json  >  全局 config.json  >  defaults
```

对其它 `llm.*` 字段：

```
process env (KB_LLM_*)  >  项目 .kb/config.json  >  全局 config.json  >  defaults
```

对 `storage.backend`：

```
项目 .kb/config.json  >  defaults("fs")
```

任何层缺失或解析失败都会跳到下一层，永不破坏启动。`kb env --doctor` 会逐字段打印当前生效来源，便于排查"为什么我的 timeout 没生效"。

### 全局 config.json 的额外字段

全局 config 文件除 `llm` 节外还会维护一个 `knowledge_bases` 数组（由 `kb init` 写入、`kb env --list` 读取）：

```json
{
  "llm": { "model": "gpt-4o-mini" },
  "knowledge_bases": [
    "C:\\path\\to\\kb-one",
    "C:\\path\\to\\kb-two"
  ]
}
```

该数组由 CLI 自维护，**不要手工编辑**（路径会在 `kb init` / `kb env --list` 时被规范化）。

### 全字段示例（项目级 `./.kb/config.json`）

```json
{
  "llm": {
    "enable_semantic": true,
    "enable_graph": true,
    "enable_wiki": true,
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
  },
  "storage": {
    "backend": "fs"
  }
}
```

> 真正需要时密钥可以写在 `llm.key` 里，但 `kb init` 默认不写、`.kb/config.json` 默认被 `.gitignore` 排除——优先选择环境变量或 keyring。

## Agent 记忆系统

KB-CLI 实现了一个**可插拔的外部 agent 记忆系统**，让任何 agent（Claude Desktop / Cursor / Cline / 自定义 agent）都能把这个项目当作外挂大脑使用。

### 记忆分层模型（L0-L5）

- **L0 Episode**：原始交互时间线（append-only，`.kb/episodes.jsonl` 或 sqlite `episodes` 表）
- **L1 Index**：文件索引（`.kb/index.json`）
- **L2 Knowledge**：语义记忆（`.kb/knowledge.json` + `.kb/raw/agent-memory/*.md`）
- **L3 Search**：检索索引（`.kb/search.json`）
- **L4 Graph**：知识图谱（`.kb/graph.json`）
- **L5 Wiki**：wiki 页面（`.kb/wiki/*.md`）

### 记忆写入流程

1. **实时写入 L0**：agent 通过 `kb remember` / `kb correct` 或 MCP tools 写入 episode
2. **固化为语义记忆**：`kb consolidate` 将 episodes 聚类/提炼成 `.kb/raw/agent-memory/*.md` 文件（机器可读，类似 skill/SOP）
3. **编译到上层**：`kb update` 将 agent-memory 文件编译进 L1-L5，供检索使用

### 记忆命令

```bash
# 记录观察（默认 confidence 0.8）
kb remember "项目使用 Rust workspace + 六边形架构"

# 记录用户纠错（最高优先级，confidence 1.0）
kb correct "存储后端默认是 fs 不是 sqlite"

# 固化记忆（dry-run 预览）
kb consolidate --dry-run

# 固化记忆（实际写入）
kb consolidate

# 查看 episode 时间线
kb ask "list recent episodes" --output json
```

### MCP 集成：让外部 agent 接入

KB-CLI 通过 [Model Context Protocol (MCP)](https://modelcontextprotocol.io) 暴露记忆系统，任何支持 MCP 的 agent 都能无缝接入。

#### 启动 MCP server

```bash
kb serve --mcp
```

Server 通过 stdio 接受 JSON-RPC 2.0 请求，暴露 8 个 tools：

| Tool | 功能 |
|------|------|
| `recall` | 检索记忆（分层检索，返回 ranked contexts） |
| `remember` | 写入观察（L0 episode，kind=Observation） |
| `correct` | 写入纠错（L0 episode，kind=UserCorrection，最高优先级） |
| `verify_citation` | 验证引用（词法校验 quote 是否真实存在于 chunk_id） |
| `list_episodes` | 列出 episode 时间线（可按 agent_id 过滤） |
| `consolidate` | 触发记忆固化（支持 dry_run 预览） |
| `read_summary` | 读取文件语义摘要（L2） |
| `read_neighbors` | 读取知识图谱邻接（L4） |

#### Claude Desktop 配置

编辑 `%APPDATA%\Claude\claude_desktop_config.json`（Windows）或 `~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）：

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

重启 Claude Desktop，在对话中 agent 会自动看到 8 个 tools，可以：
- 用 `recall` 检索项目知识
- 用 `remember` 记录学到的事实
- 用 `correct` 接受用户纠错
- 用 `verify_citation` 验证引用真实性（反幻觉）

#### Cursor 配置

在项目根目录创建 `.cursorrules` 或编辑 Cursor 设置中的 MCP servers：

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

#### Cline (VSCode) 配置

在 VSCode 设置中添加 MCP server（`settings.json`）：

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

#### 环境变量说明

- `KB_AGENT_ID`：标识调用方 agent（用于 episode 归属和审计），默认 `kb-cli`
- `KB_SESSION_ID`：会话 ID（可选，用于按会话过滤 episodes）

### 记忆系统设计原则

1. **反幻觉优先**：
   - `verify_citation` 做词法验证（substring match + token overlap）
   - `UserCorrection` 最高优先级（confidence 1.0）
   - 冲突可见但不覆写（provenance 链保留历史）

2. **轻量无依赖**：
   - 不用向量嵌入（保持"无 LLM 也能跑"）
   - 手写 stdio JSON-RPC（零额外依赖）
   - Append-only episode store（JSONL 或 sqlite）

3. **多 agent 共享**：
   - 每个 episode 带 `agent_id` 归属
   - `consolidate` 聚合多 agent 贡献
   - 通过 `KB_AGENT_ID` 隔离不同调用方

4. **借鉴前沿研究**：
   - 4 类记忆分层（Working / Episodic / Semantic / Procedural，arXiv:2404.13501）
   - 双时态标注（Zep/Graphiti：事实成立时间 + 观测时间）
   - Confidence decay + provenance chains（Mem0 冲突检测）

## Ask 模式说明

默认检索包模式：

```bash
kb ask "这个项目如何更新索引？" --output json
```

重点字段：

- `data.mode = "retrieve"`
- `data.retrieved_context`
- `data.retrieval_trace`
- `data.reasoning_context`

显式最终回答模式：

```bash
kb ask "这个项目如何更新索引？" --answer --output json
```

重点字段：

- `data.mode = "answer"`
- `data.tool_trace`
- `data.answer.text`
- `data.answer.citations`

当前 `--answer` 会最大化复用 `.kb/search.json`、`.kb/knowledge.json`、`.kb/graph.json`、`.kb/wiki/*` 以及命中的原文片段，走一个最小只读 agent loop；过程输出保持简短，最终答案是主输出。内置 agent 仅作参考实现——**推荐通过 MCP 让外部 agent 接入记忆系统**。

## 更多文档

- 开发者编译、测试、打包、发布：`docs/cli/developer-workflow.md`
- 安装与打包细节：`docs/cli/install-and-package.md`
- 命令手册：`docs/cli/command-manual.md`
- 命令示例：`docs/cli/command-examples.md`
- 错误目录：`docs/cli/error-catalog.md`
- 规范：`docs/cli/kb-md-spec.md`
