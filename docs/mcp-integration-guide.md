# MCP Integration Guide

本文档说明如何将 KB-CLI 的记忆系统通过 MCP (Model Context Protocol) 集成到外部 agent。

## 快速开始

### 1. 启动 MCP Server

```bash
kb serve --mcp
```

Server 通过 stdio 接受 JSON-RPC 2.0 请求，暴露 8 个 tools。

### 2. 配置外部 Agent

#### Claude Desktop

**Windows:**

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

**macOS:**

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

重启 Claude Desktop 后，agent 会自动看到 8 个 tools。

#### Cursor

在项目级 `.cursor/mcp.json` 或全局 `~/.cursor/mcp.json` 中配置 MCP server：

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

#### Cline

在 Cline 扩展的 MCP Servers 设置中添加 server（配置文件名为 `cline_mcp_settings.json`），不要写入 VS Code 的 `settings.json`：

```json
{
  "mcpServers": {
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

## 可用工具

MCP server 暴露以下 8 个 tools：

| Tool | 功能 | 主要参数 |
|------|------|---------|
| `recall` | 检索记忆 | `query`, `top_k` |
| `remember` | 写入观察 | `content`, `confidence`, `refs` |
| `correct` | 写入纠错 | `content`, `refs` |
| `verify_citation` | 验证引用 | `quote`, `chunk_id` |
| `list_episodes` | 列出时间线 | `agent_id`, `limit` |
| `consolidate` | 固化记忆 | `dry_run`, `min` |
| `read_summary` | 读取摘要 | `record_id_or_path` |
| `read_neighbors` | 读取图谱邻接 | `record_id` |

### 工具详细说明

#### `recall`

检索记忆，使用分层检索（L3 → L2 → L4 → L1）。

**输入：**
```json
{
  "query": "项目架构是什么",
  "top_k": 5
}
```

**输出：**
```json
{
  "query": "项目架构是什么",
  "top_k": 5,
  "results": [
    {
      "alias": "architecture",
      "snippet": "项目使用 Rust workspace + 六边形架构",
      "source_layer": "l2_semantic",
      "record_id": "test-doc.md",
      "chunk_id": "architecture:0001",
      "score": 0.95
    }
  ]
}
```

#### `remember`

记录 agent 观察到的事实（写入 L0 episode）。

**输入：**
```json
{
  "content": "用户偏好使用 Rust 进行系统编程",
  "confidence": 0.8
}
```

**输出：**
```json
{
  "command": "remember",
  "episode_id": "ep-1780232776512863500-364524e31ae9b138",
  "agent_id": "claude-desktop",
  "kind": "observation",
  "confidence": 0.800000011920929,
  "refs": []
}
```

#### `correct`

记录用户纠错（最高优先级，confidence 固定 1.0）。

**输入：**
```json
{
  "content": "存储后端默认是 fs 不是 sqlite"
}
```

**输出：**
```json
{
  "command": "correct",
  "episode_id": "ep-1780232776419638000-cc840e4e00cd0a72",
  "agent_id": "claude-desktop",
  "kind": "user_correction",
  "refs": []
}
```

#### `verify_citation`

验证引用的真实性（反幻觉）。当 chunk 的源文件切片边界失效或 FNV-1a 内容签名不符时，server 会回退到索引时的 preview 验证，并返回 `stale: true` 和更新提示。

**输入：**
```json
{
  "quote": "项目使用 Rust workspace",
  "chunk_id": "docs/architecture.md:0001"
}
```

**输出：**
```json
{
  "chunk_id": "docs/architecture.md:0001",
  "quote": "项目使用 Rust workspace",
  "matched": true,
  "method": "substring",
  "coverage": 1.0,
  "stale": false
}
```

源文件已变化时响应包含 `stale: true` 和 `warning`，提示运行 `kb update` 刷新索引。

#### `list_episodes`

列出 episode 时间线。

**输入：**
```json
{
  "agent_id": "claude-desktop",
  "limit": 10
}
```

**输出：**
```json
{
  "count": 1,
  "episodes": [
    {
      "id": "ep-...",
      "agent_id": "claude-desktop",
      "session_id": "session-1",
      "kind": "observation",
      "content": "...",
      "refs": [],
      "confidence": 0.8,
      "timestamp": 1780232776512863500
    }
  ]
}
```

#### `consolidate`

触发记忆固化（按引用中的 record topic 将 episodes 聚类，生成 `.kb/raw/agent-memory/*.md`）。新文件名为 `{slug}-{topic 的 FNV-1a 64 位哈希前 8 位}.md`；已有文件头部 metadata 的 topic 匹配时会复用旧路径。短哈希文件名冲突时扩展到完整 16 位哈希，不会覆盖其他 topic。未能登记的文件会列入 `warnings`；已登记项计入 `already_declared`。

**输入：**
```json
{
  "dry_run": true,
  "min": 2
}
```

**输出：**
```json
{
  "command": "consolidate",
  "dry_run": true,
  "min_episodes_per_topic": 2,
  "considered": 2,
  "skipped_non_assertive": 0,
  "skipped_below_threshold": 0,
  "memories_planned": 1,
  "memories": [
    {
      "topic": "general",
      "file_stem": "general-9e275796",
      "confidence": 0.9142857193946838,
      "has_user_correction": true,
      "source_episode_ids": ["ep-1780232776419638000-cc840e4e00cd0a72", "ep-1780232776512863500-364524e31ae9b138"],
      "contributor_agents": ["claude-desktop"],
      "refs": []
    }
  ],
  "written": [],
  "added": [],
  "already_declared": 0,
  "warnings": []
}
```

#### `read_summary`

读取文件的语义摘要（L2 层）。

**输入：**
```json
{
  "record_id_or_path": "test-doc.md"
}
```

**输出：**
```json
{
  "summary": "This document describes...",
  "tags": ["rust", "architecture"],
  "key_concepts": ["hexagonal architecture", "workspace"]
}
```

未命中时返回 `{"found": false, "target": "..."}`。

> **sqlite 后端注意**：`read_summary` / `read_neighbors` / `verify_citation`
> 直接读取 `.kb/knowledge.json` 等 fs 文件布局。当 `storage.backend` 为
> `sqlite` 时（L2/L4/L5 落在 `store.sqlite3`），这些工具可能报告数据缺失，
> 响应中会附带 `warning` 字段说明原因；`recall`/`remember`/`correct`/
> `list_episodes`/`consolidate` 不受影响。

episode 或 recall index 存储打开、读取、写入失败时，MCP 工具错误内容包含 `E1004`；`verify_citation` 找不到可用候选证据时返回 `matched: false` 的正常工具结果。

#### `read_neighbors`

读取知识图谱中某条记录的邻接关系（L4 层）。

**输入：**
```json
{
  "record_id": "docs/architecture.md"
}
```

**输出：**
```json
{
  "record_id": "docs/architecture.md",
  "neighbors": [
    {
      "edge_id": "file:docs/architecture.md->file:docs/design.md",
      "edge_type": "file_related_to_file",
      "related_node_id": "file:docs/design.md",
      "reason": "..."
    }
  ]
}
```

## 环境变量

- `KB_AGENT_ID`：标识调用方 agent（默认 `kb-cli`）
  - 用于 episode 归属和审计
  - 建议为每个 agent 设置唯一 ID（如 `claude-desktop`、`cursor`、`cline`）
  
- `KB_SESSION_ID`：会话 ID（可选）
  - 用于按会话过滤 episodes
  - 可以设置为对话 ID 或任务 ID

## 工作流程

### 典型使用场景

1. **Agent 学习新知识**
   ```
   用户: "这个项目使用 Rust workspace"
   Agent: 调用 remember("项目使用 Rust workspace", confidence=0.8)
   ```

2. **用户纠正 Agent**
   ```
   用户: "不对，存储后端默认是 fs"
   Agent: 调用 correct("存储后端默认是 fs")
   ```

3. **Agent 检索记忆**
   ```
   用户: "项目架构是什么？"
   Agent: 调用 recall("项目架构", top_k=5)
   Agent: 基于检索结果回答
   ```

4. **Agent 验证引用**
   ```
   Agent: 准备引用 "项目使用 Rust workspace"
   Agent: 调用 verify_citation(quote="项目使用 Rust workspace", chunk_id="...")
   Agent: 如果 verified=true，则使用该引用
   ```

5. **定期固化记忆**
   ```
   Agent: 积累了 50+ episodes
   Agent: 调用 consolidate(dry_run=false)
   系统: 将 episodes 聚类提炼成 .kb/raw/agent-memory/*.md
   用户: 运行 kb update 将记忆编译进 L1-L5 层
   ```

## 测试验证

### 基础功能测试

```bash
# 1. 初始化测试知识库
kb init test-mcp
cd test-mcp

# 2. 添加测试内容
echo "# Test" > test.md
kb add test.md
kb update

# 3. 测试记忆命令
export KB_AGENT_ID="test-agent"
kb remember "测试观察"
kb correct "测试纠错"
kb consolidate --dry-run

# 4. 启动 MCP server（另一个终端）
kb serve --mcp
```

### MCP 协议测试

使用 JSON-RPC 2.0 格式发送请求：

**Initialize:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "test-client",
      "version": "1.0.0"
    }
  }
}
```

**List Tools:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

**Call Tool:**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "remember",
    "arguments": {
      "content": "测试内容",
      "confidence": 0.8
    }
  }
}
```

## 故障排查

### MCP Server 无法启动

1. 检查知识库是否已初始化：`kb list`
2. 检查二进制文件路径是否正确
3. 查看错误日志（stderr）

### Agent 看不到 Tools

1. 确认 MCP server 配置正确
2. 重启 agent 应用（Claude Desktop / Cursor / VSCode）
3. 检查环境变量是否设置

### 记忆无法检索

1. 确认已运行 `kb consolidate`
2. 确认已运行 `kb update` 将记忆编译进索引
3. 使用 `kb ask` 测试检索功能

### Episode 写入失败

1. 检查知识库目录权限
2. 检查 `.kb/episodes.jsonl` 是否可写
3. 如果使用 sqlite 后端，检查数据库文件

## 设计原则

1. **反幻觉优先**
   - `verify_citation` 做词法验证
   - `UserCorrection` 最高优先级（confidence 1.0）
   - 冲突可见但不覆写（provenance 链保留历史）

2. **轻量无依赖**
   - 不用向量嵌入（保持"无 LLM 也能跑"）
   - 手写 stdio JSON-RPC（零额外依赖）
   - Append-only episode store（JSONL 或 sqlite）

3. **多 agent 共享**
   - 每个 episode 带 `agent_id` 归属
   - `consolidate` 聚合多 agent 贡献
   - 通过 `KB_AGENT_ID` 隔离不同调用方

4. **借鉴前沿研究**
   - 4 类记忆分层（Working / Episodic / Semantic / Procedural）
   - 双时态标注（事实成立时间 + 观测时间）
   - Confidence decay + provenance chains

## 参考资料

- [Model Context Protocol 规范](https://modelcontextprotocol.io)
- [KB-CLI 命令手册](cli/command-manual.md)
- [记忆模型设计文档](architecture/memory-model.md)
- [arXiv:2404.13501 - Agent Memory Taxonomy](https://arxiv.org/abs/2404.13501)
