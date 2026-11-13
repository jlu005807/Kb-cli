# KB-CLI 命令示例

本文档展示 `kb` 命令的标准输出示例，其中 JSON 模式示例是快照测试的权威目标；部分命令附带 text 模式可读示例，用于展示终端交互体验。当本文件与 `command-manual.md` 有冲突时，以 `command-manual.md` 为准。

> 更新说明（2026-04-17）：补充 `kb completion` 与 `kb -h` 示例，并同步 `env` 默认模式（`list(default)`）输出；既有命令的 fixtures 对齐 JSON 快照块保持不变。
> 实现更新（2026-04-18）：`check/update/list` 以 `kb.md + ./.kb/raw/` 为声明源，并基于 `./.kb/regs.json` 进行登记对比与展示。
> 为保持快照一致性，既有命令的 fixture JSON 区块保持不变。
> 内部重构（2026-04-19）：命令实现已拆分为按命令模块 + runtime core 子模块，命令输出保持不变。


> 相关文档：
> - `docs/cli/command-manual.md`
> - `docs/cli/error-catalog.md`
> - `docs/cli/state-diff-contract.md`
> - `docs/cli/kbignore-spec.md`
> - `docs/cli/kb-md-spec.md`

## 1. `kb init [NAME]`

### 成功示例
**命令：**
```bash
kb init demo --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "project": "demo",
    "path": "./kb.md"
  }
}
```

### 失败示例
**命令：**
```bash
kb init demo --output=json
```
**退出码：** 3

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E3002",
    "message": "Knowledge base already initialized",
    "suggestion": "Use --force to overwrite or run in a different directory",
    "details": {}
  }
}
```

## 2. `kb add <PATH> [ALIAS] [--mode <link|copy>] [--category <CATEGORY>]`

### 成功示例
**命令：**
```bash
kb add ./docs my-docs --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "alias": "my-docs",
    "path": "./docs"
  }
}
```

### 失败示例
**命令：**
```bash
kb add ./missing-file --output=json
```
**退出码：** 5

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E5001",
    "message": "Path does not exist",
    "suggestion": "Verify the file or directory exists at the provided path",
    "details": {
      "path": "./missing-file"
    }
  }
}
```

### 失败示例 (分类不存在)
**命令：**
```bash
kb add ./docs my-docs --category unknown-cat --output=json
```
**退出码：** 2

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E2007",
    "message": "Category not found",
    "suggestion": "Run 'kb list --tree' to view available categories and try again",
    "details": {
      "category": "unknown-cat"
    }
  }
}
```

## 3. `kb mv <ALIAS> <NEW_ALIAS>`

### 成功示例
**命令：**
```bash
kb mv my-docs latest-docs --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "old_alias": "my-docs",
    "new_alias": "latest-docs"
  }
}
```

### 失败示例
**命令：**
```bash
kb mv missing-alias new-alias --output=json
```
**退出码：** 5

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E5002",
    "message": "Source alias not found",
    "suggestion": "Run 'kb list' to see available aliases",
    "details": {
      "alias": "missing-alias"
    }
  }
}
```

## 4. `kb rm <ALIAS> [--force]`

> 实现说明（2026-04-19）：`kb rm` 会从 `kb.md` 删除声明条目（按 alias/id/path 匹配），不会直接写入 `./.kb/regs.json`。
> 当目标是 raw 条目（`./.kb/raw/*`）时，`kb rm` 需要确认是否删除源文件；确认后才会删除声明与源文件。
> 未确认时会忽略本次操作，并返回 `ignored=true` 与 `reason=raw_delete_not_confirmed`。
> 建议先执行 `kb check` 预览 `to_remove`，再执行 `kb update` 将删除同步到 `regs.json` 与后端索引产物。

### 成功示例
**命令：**
```bash
kb rm my-docs --force --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "removed_alias": "my-docs",
    "path": "./docs"
  }
}
```

### 忽略示例（raw 未确认删除源文件）
**命令：**
```bash
kb rm raw-a --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "ignored": true,
    "reason": "raw_delete_not_confirmed",
    "alias": "raw-a",
    "path": "./.kb/raw/a.md"
  }
}
```

### 失败示例
**命令：**
```bash
kb rm missing-alias --output=json
```
**退出码：** 5

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E5002",
    "message": "Alias does not exist",
    "suggestion": "Run 'kb list' to see available aliases",
    "details": {
      "alias": "missing-alias"
    }
  }
}
```

## 5. `kb path <ALIAS> [NEW_PATH]`

### 成功示例
**命令：**
```bash
kb path my-docs ./new-docs --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "alias": "my-docs",
    "old_path": "./docs",
    "new_path": "./new-docs"
  }
}
```

### 失败示例
**命令：**
```bash
kb path missing-alias --output=json
```
**退出码：** 5

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E5002",
    "message": "Alias does not exist",
    "suggestion": "Run 'kb list' to see available aliases",
    "details": {
      "alias": "missing-alias"
    }
  }
}
```

## 6. `kb chmod <ALIAS> <PERMS>`

> 实现说明（2026-04-19）：`chmod` 仅修改 `kb.md` 中的权限声明（条目级或分类级），不会直接写入 `./.kb/regs.json`。
> 建议执行 `kb check` 预览权限差异，再通过 `kb update` 持久化到 `regs.json`。


### 成功示例
**命令：**
```bash
kb chmod my-docs rx --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "alias": "my-docs",
    "permissions": "rx"
  }
}
```

### 失败示例
**命令：**
```bash
kb chmod my-docs invalid --output=json
```
**退出码：** 2

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E2004",
    "message": "Invalid permission string",
    "suggestion": "Use a valid combination of r, w, x, d, c, or -",
    "details": {
      "provided": "invalid"
    }
  }
}
```

## 7. `kb check [--raw-only] [--category <CATEGORY>] [--top <N>]`

> 实现说明（2026-04-18）：差异来源为 `kb.md + ./.kb/raw/` 声明集，对比对象为 `./.kb/regs.json` 登记集。
> `reason` 取值：`content_changed` / `metadata_changed` / `deleted` / `not_found`。


### 成功示例
**命令：**
```bash
kb check --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "has_changes": true,
    "to_add": [
      {
        "alias": "raw/spec-a.md",
        "path": "./.kb/raw/spec-a.md",
        "source": "raw"
      }
    ],
    "to_update": [
      {
        "alias": "docs/guide",
        "path": "./docs/guide.md",
        "reason": "content_changed"
      }
    ],
    "to_remove": [
      {
        "alias": "legacy/notes",
        "path": "./docs/legacy-notes.md",
        "reason": "deleted"
      }
    ],
    "suggested_command": "kb update",
    "duration_ms": 96
  }
}
```

**命令（text 模式）：**
```bash
kb check
```
**输出（text）：**
```text
Index diff
has_changes: true
to_add: 1
to_update: 1
to_remove: 1
duration_ms: 96
suggested_command: kb update

To add:
  - raw/spec-a.md | ./.kb/raw/spec-a.md [raw]

To update:
  - docs/guide | ./docs/guide.md (content_changed)

To remove:
  - legacy/notes | ./docs/legacy-notes.md (deleted)
```

### 失败示例
**命令：**
```bash
kb check --output=json
```
**退出码：** 3

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E3003",
    "message": "Invalid kb.md format",
    "suggestion": "Fix kb.md syntax and run kb check again",
    "details": {
      "line": 42
    }
  }
}
```

## 8. `kb update [--dry-run] [--prune] [--full] [--strict] [--llm]`

> 实现说明（2026-04-18）：`update` 会把 `check` 差异写入 `./.kb/regs.json`，移除过期/已删除/不存在条目，并刷新后端索引文件。
> 当 `check` 确认为 `to_remove` 时，`update` 也会清理匹配的陈旧索引记录，即使这些路径已不在 `regs.json` 中。
> 对于确认为 `not_found` 的声明，`update` 会同步删除 `kb.md` 中对应引用行。
> `update` 是 `kb rm`、`kb chmod` 等“声明优先变更”的持久化步骤。
> 为保证 raw 与 `kb.md` 对齐，`update` 还会按路径去重 `kb.md` 中重复的 raw 引用行。
> 分层策略：默认模式下 `L2-L5` 失败只记 `warnings` 并返回成功；`--strict` 下 `L2-L5` 任一失败即整体失败。
> `--llm` 会在本次更新中强制尝试启用 L2/L4/L5 的 LLM 路径；若 `KB_LLM_KEY` 缺失则自动回退规则路径并给 warning。
> 阶段键：`l0_declaration`、`l1_registry`、`l1_index`、`l2_semantic`、`l3_projection`、`l4_graph`、`l5_wiki_view`。


### 成功示例
**命令：**
```bash
kb update --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "added": 3,
    "updated": 1,
    "removed": 0,
    "marked_invalid": 0,
    "duration_ms": 187,
    "full": false,
    "raw_category_created": false,
    "strict": false,
    "missing_declarations": [],
    "layer_status": {
      "l0_declaration": "ok",
      "l1_registry": "ok",
      "l1_index": "ok",
      "l2_semantic": "warning",
      "l3_projection": "ok",
      "l4_graph": "ok",
      "l5_wiki_view": "ok"
    },
    "l2_compilation": {
      "processed": 3,
      "reused": 0,
      "missing": 0,
      "failed": 1
    },
    "warnings": [
      {
        "layer": "l2_semantic",
        "code": "W-L2-EXTRACT-FAILED",
        "message": "Semantic extraction stage failed",
        "retryable": true
      }
    ],
    "errors": []
  }
}
```

### LLM 构建示例（`--llm`）
**命令：**
```bash
kb update --llm --output=json
```
**输出（JSON，节选）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "layer_status": {
      "l2_semantic": "ok",
      "l4_graph": "ok",
      "l5_wiki_view": "ok"
    },
    "semantic_extractor": {
      "forced_by_cli": true,
      "mode": "llm",
      "provider": "openai-compatible:gpt-4o-mini@https://api.openai.com/v1"
    },
    "graph_builder": {
      "mode": "llm",
      "agent_tools": {
        "enabled": true,
        "tools": ["read_file_summary", "read_knowledge_graph", "read_graph_neighbors"]
      }
    },
    "wiki_builder": {
      "mode": "llm",
      "agent_tools": {
        "enabled": true,
        "tools": ["read_file_summary", "read_knowledge_graph", "read_wiki_page"]
      }
    }
  }
}
```

## 8.1 `kb ask <QUESTION> [--top <N>] [--answer]`

### 检索包示例（默认）
**命令：**
```bash
kb ask "这个项目如何更新索引？" --output=json
```

**输出（JSON，节选）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "mode": "retrieve",
    "engine_status": "configured",
    "retrieved_context": [
      {
        "alias": "docs/config",
        "source_layer": "l3_projection",
        "record_id": "docs/config"
      }
    ],
    "retrieval_trace": [
      {
        "message": "1 hit docs/config via l3_projection"
      }
    ],
    "reasoning_context": {
      "question": "这个项目如何更新索引？",
      "context_count": 1
    }
  }
}
```

### 最终回答示例（`--answer`）
**命令：**
```bash
kb ask "这个项目如何更新索引？" --answer --output=json
```

**输出（JSON，节选）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "mode": "answer",
    "engine_status": "configured",
    "tool_trace": [
      {
        "tool": "search_context",
        "summary": "search_context seeded 1 retrieved items"
      },
      {
        "tool": "read_file_summary",
        "record_id": "docs/config"
      }
    ],
    "answer": {
      "text": "Based on the retrieved knowledge-base summaries...",
      "citations": [
        {
          "record_id": "docs/config"
        }
      ]
    }
  }
}
```

### LLM 配置缺失回退示例（`--llm` + 无 key）
**命令：**
```bash
kb update --llm --output=json
```
**输出（JSON，节选）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "semantic_extractor": { "mode": "rule" },
    "graph_builder": { "mode": "rule" },
    "wiki_builder": { "mode": "rule" },
    "warnings": [
      { "code": "W-L2-LLM-CONFIG-MISSING" },
      { "code": "W-L4-LLM-CONFIG-MISSING" },
      { "code": "W-L5-LLM-CONFIG-MISSING" }
    ]
  }
}
```

**命令（text 模式）：**
```bash
kb update
```
**输出（text）：**
```text
Index update complete
added: 3
updated: 1
removed: 0
marked_invalid: 0
duration_ms: 187
dry_run: false
prune: false
full: false
strict: false
layer_status:
  l0_declaration: ok
  l1_registry: ok
  l1_index: ok
  l2_semantic: warning
  l3_projection: ok
  l4_graph: ok
  l5_wiki_view: ok
l2_compilation:
  processed: 3
  reused: 0
  missing: 0
  failed: 1
warnings:
  - [l2_semantic] W-L2-EXTRACT-FAILED: Semantic extraction stage failed (retryable)
```

### 失败示例
**命令：**
```bash
kb update --strict --output=json
```
**退出码：** 1

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E1011",
    "message": "Strict mode failed on progressive layer error",
    "suggestion": "Retry after resolving failed layers or run without --strict",
    "details": {
      "strict": true,
      "layer_status": {
        "l0_declaration": "ok",
        "l1_registry": "ok",
        "l1_index": "ok",
        "l2_semantic": "error",
        "l3_projection": "ok",
        "l4_graph": "ok",
        "l5_wiki_view": "ok"
      },
      "l2_compilation": {
        "processed": 3,
        "reused": 0,
        "missing": 0,
        "failed": 1
      },
      "warnings": [],
      "errors": [
        {
          "layer": "l2_semantic",
          "code": "W-L2-EXTRACT-FAILED",
          "message": "Semantic extraction stage failed",
          "retryable": true
        }
      ],
      "failed_layers": [
        "l2_semantic"
      ]
    }
  }
}
```

## 9. `kb list [--long] [--tree] [--filter <PATTERN>]`

> 实现说明（2026-04-18）：`list` 展示的是 `./.kb/regs.json` 中的登记结果（不是直接解析 `kb.md` 的结果）。


### 成功示例
**命令：**
```bash
kb list --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "items": [
      {
        "alias": "docs",
        "path": "./docs",
        "perms": "rwx"
      },
      {
        "alias": "src",
        "path": "./src",
        "perms": "r"
      }
    ]
  }
}
```

**命令（text 模式）：**
```bash
kb list
```
**输出（text）：**
```text
Tracked references: 2
ALIAS  PERMS  PATH
docs   rwx    ./docs
src    r      ./src
```

### 失败示例
**命令：**
```bash
kb list --filter "[invalid" --output=json
```
**退出码：** 2

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E2002",
    "message": "Invalid glob pattern",
    "suggestion": "Verify the filter string is a valid glob pattern",
    "details": {
      "pattern": "[invalid"
    }
  }
}
```

## 10. `kb ask <QUESTION> [--top <N>]`

> 实现说明（2026-05-02）：`kb ask` 先查 `L3`，再查 `L2`、`L4`，最后回退 `L1`；每条命中会返回 `source_layer/record_id/chunk_id` 便于追溯。  

### 成功示例
**命令：**
```bash
kb ask "how to configure?" --top 1 --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "engine_status": "disabled",
    "provider": null,
    "trace_id": null,
    "retrieved_context": [
      {
        "alias": "docs/config",
        "snippet": "Use kb config to change settings.",
        "score": 1.0,
        "source_layer": "l3_projection",
        "record_id": "docs/config",
        "chunk_id": "docs/config:0001"
      }
    ]
  }
}
```

### 失败示例
**命令：**
```bash
kb ask "unrelated question" --output=json
```
**退出码：** 5

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E5003",
    "message": "No relevant context found",
    "suggestion": "Try a different question or add more files to the knowledge base",
    "details": {
      "query": "unrelated question"
    }
  }
}
```

## 11. `kb env [--list]`

### 成功示例
**命令：**
```bash
kb env --list --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "mode": "list",
    "items": [
      {
        "path": "D:/workspace/project-a"
      },
      {
        "path": "D:/workspace/project-b"
      }
    ],
    "count": 2,
    "recorded_total": 3,
    "config_path": "C:/Users/demo/.kb-cli/config.json"
  }
}
```

**命令（text 模式）：**
```bash
kb env --list
```
**输出（text）：**
```text
Knowledge base registry
config_path: C:/Users/demo/.kb-cli/config.json
recorded_total: 3
count: 2
items:
  - D:/workspace/project-a
  - D:/workspace/project-b
```

> 说明：`items` 仅包含“已登记且仍存在”的知识库目录；`recorded_total` 为全量登记数。

### 默认模式示例（未传 `--list`）
**命令：**
```bash
kb env --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "config_path": "D:\\workspace\\kb-demo\\global-config.json",
    "count": 0,
    "items": [],
    "mode": "list(default)",
    "recorded_total": 0
  }
}
```

## 12. `kb remember <CONTENT>`

### 成功示例
**命令：**
```bash
kb remember "项目使用 Rust workspace + 六边形架构" --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "episode_id": "ep_1748707200000000000_abc123",
    "agent_id": "kb-cli",
    "session_id": null,
    "kind": "Observation",
    "content": "项目使用 Rust workspace + 六边形架构",
    "confidence": 0.8,
    "timestamp": 1748707200000000000
  }
}
```

**命令（text 模式）：**
```bash
kb remember "项目使用 Rust workspace + 六边形架构"
```
**输出（text）：**
```text
Episode recorded
episode_id: ep-1748707200000000000-ab12cd34ef56ab12
kind: Observation
confidence: 0.8
```

## 13. `kb correct <CONTENT>`

### 成功示例
**命令：**
```bash
kb correct "存储后端默认是 fs 不是 sqlite" --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "episode_id": "ep_1748707260000000000_def456",
    "agent_id": "kb-cli",
    "session_id": null,
    "kind": "UserCorrection",
    "content": "存储后端默认是 fs 不是 sqlite",
    "confidence": 1.0,
    "timestamp": 1748707260000000000
  }
}
```

**命令（text 模式）：**
```bash
kb correct "存储后端默认是 fs 不是 sqlite"
```
**输出（text）：**
```text
User correction recorded
episode_id: ep-1748707260000000000-de45fa67bc89de45
kind: UserCorrection
confidence: 1.0 (highest priority)
```

## 14. `kb consolidate`

### 成功示例（dry-run）
**命令：**
```bash
kb consolidate --dry-run --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "mode": "dry_run",
    "episodes_processed": 15,
    "files_created": 0,
    "files_updated": 0,
    "consolidation_plan": [
      {
        "agent_id": "kb-cli",
        "episode_count": 12,
        "target_file": ".kb/raw/agent-memory/kb-cli-session-001.md",
        "action": "create"
      },
      {
        "agent_id": "claude-desktop",
        "episode_count": 3,
        "target_file": ".kb/raw/agent-memory/claude-desktop-session-002.md",
        "action": "update"
      }
    ]
  }
}
```

### 成功示例（execute）
**命令：**
```bash
kb consolidate --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "mode": "execute",
    "episodes_processed": 15,
    "files_created": 1,
    "files_updated": 1
  }
}
```

**命令（text 模式）：**
```bash
kb consolidate
```
**输出（text）：**
```text
Memory consolidation complete
episodes_processed: 15
files_created: 1
files_updated: 1

Next step: Run 'kb update' to compile consolidated memories into L1-L5 layers
```

## 15. `kb serve --mcp`

### 运行示例
**命令：**
```bash
kb serve --mcp
```
**输出（text，启动日志）：**
```text
KB-CLI MCP Server starting...
Protocol: JSON-RPC 2.0 over stdio
Tools available: 8
  - recall
  - remember
  - correct
  - verify_citation
  - list_episodes
  - consolidate
  - read_summary
  - read_neighbors

Waiting for requests... (Press Ctrl+C to stop)
```

**MCP 协议交互示例（JSON-RPC）：**

客户端请求（initialize）：
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "claude-desktop",
      "version": "1.0.0"
    }
  }
}
```

服务端响应：
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {}
    },
    "serverInfo": {
      "name": "kb-cli-mcp",
      "version": "0.1.0"
    }
  }
}
```

客户端请求（tools/list）：
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

服务端响应（部分）：
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "recall",
        "description": "Retrieve memory from the knowledge base using layered search",
        "inputSchema": {
          "type": "object",
          "properties": {
            "query": {
              "type": "string",
              "description": "Search query"
            },
            "top_k": {
              "type": "integer",
              "description": "Number of results to return",
              "default": 5
            }
          },
          "required": ["query"]
        }
      }
    ]
  }
}
```

## 16. `kb completion <SHELL>`

### 成功示例
**命令：**
```bash
kb completion bash --output=json
```
**输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {
    "shell": "bash",
    "script": "_kb() {\n    local i cur prev opts cmd\n    COMPREPLY=()\n    ..."
  }
}
```

**命令（text 模式）：**
```bash
kb completion bash
```
**输出（text，片段）：**
```text
_kb() {
    local i cur prev opts cmd
    COMPREPLY=()
    ...
}
```

## 附录. `kb -h`

**命令：**
```bash
kb -h
```
**输出（text，片段）：**
```text
Usage: kb [OPTIONS] <COMMAND>

These are common kb commands used in various situations:

start a working area
  init        Create kb.md, .kb state and default ignore rules

manage references
  add         Add file contents to the knowledge base
  mv          Move or rename an alias
  rm          Remove an alias from the working set
```

> 维护说明：`kb --help` 的完整输出由快照测试保护，基线文件位于  
> `crates/kb-cli/tests/snapshots/cli_surface__kb_root_help_full.snap`。
