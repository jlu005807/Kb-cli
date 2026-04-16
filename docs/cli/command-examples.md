# KB-CLI 命令示例

本文档展示 `kb` 命令在 JSON 模式下的标准输出示例。请将这些示例视为快照测试的权威目标。当本文件与 `command-manual.md` 有冲突时，以 `command-manual.md` 为准。

> 更新说明（2026-04-15）：本次仅同步文档状态说明，不修改现有 JSON 快照块，以保持 fixtures 合约稳定。

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
        "reason": "missing_path"
      }
    ],
    "suggested_command": "kb update",
    "duration_ms": 96
  }
}
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

## 8. `kb update [--dry-run] [--prune] [--yes] [--full]`

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
    "marked_invalid": 2,
    "duration_ms": 187,
    "full": false,
    "raw_category_created": false
  }
}
```

### 失败示例
**命令：**
```bash
kb update --output=json
```
**退出码：** 1

**错误输出（JSON）：**
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E1003",
    "message": "Failed to persist index files",
    "suggestion": "Check write permissions for .kb directory and retry",
    "details": {
      "targets": [
        ".kb/index.json",
        ".kb/index.errors.json",
        ".kb/index.meta.json"
      ]
    }
  }
}
```

## 9. `kb list [--long] [--tree] [--filter <PATTERN>]`

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
        "score": 1.0
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
