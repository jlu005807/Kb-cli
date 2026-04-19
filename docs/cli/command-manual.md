# KB-CLI 命令手册（权威）

本文档是 KB-CLI 命令行为与边界的权威说明。
若与 `command-examples.md` 或其他文档冲突，以本手册为准。

> 实现同步（2026-04-19）
> - 命令路由已解耦：`commands.rs` 仅分发，`commands/cmd_*.rs` 负责命令连接，运行时逻辑位于 `commands/runtime/core/*`。
> - 当前采用“声明优先”模型：声明来自 `kb.md + ./.kb/raw/`；登记快照为 `./.kb/regs.json`。
> - `kb add` / `kb rm` / `kb chmod` 只更新声明源（`kb.md`），不直接写入 `regs.json`。
> - `kb check` 比较声明源与 `regs.json`。
> - `kb update` 是将差异落盘到 `regs.json` 与后端索引快照的唯一入口。

## 1. 全局选项

| 选项 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `--output <text|json>` | string | `text` | 输出模式 |
| `--quiet` | flag | false | 抑制非必要输出 |
| `--verbose` | count | 0 | 日志详细级别（`-v`、`-vv`） |
| `--config <PATH>` | string | auto | 指定配置文件路径 |
| `--no-color` | flag | false | 禁用 ANSI 颜色 |
| `--help` | flag | - | 显示帮助 |
| `--version` | flag | - | 显示版本 |

## 2. 输出契约

成功 JSON 信封：

```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {}
}
```

失败 JSON 错误包：

```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E0000",
    "message": "...",
    "suggestion": "...",
    "details": {}
  }
}
```

说明：
- `text` 模式面向终端人工阅读。
- `json` 模式面向自动化与稳定解析。
- 错误术语与契约定义以 `docs/cli/error-catalog.md` 为准。

## 3. 真值来源模型

- 声明源：`kb.md` 与 `./.kb/raw/`
- 登记快照：`./.kb/regs.json`
- 检索/索引快照：`./.kb/index.json`、`./.kb/index.errors.json`、`./.kb/index.meta.json`

受保护路径：
- `kb.md` 与 `./.kb/`（`./.kb/raw/` 除外）为受保护路径，不能作为普通索引条目。

初始化行为：
- `kb init` 会在 `kb.md` 中创建 `# raw` 分类，并初始化 `./.kb/.kbignore`。

## 4. 命令说明

### 4.1 `kb init [NAME] [--force]`

行为：
- 初始化 `kb.md`、`.kb/`、`./.kb/raw/`、`./.kb/.kbignore`。
- 未使用 `--force` 的重复初始化返回 `E3002`。
- 成功后尝试写入全局知识库注册（注册失败不影响 init 成功）。

主要输出字段：
- `project`
- `path`
- `overwritten`（仅 `--force` 时）

### 4.2 `kb add <PATH> [ALIAS] [--mode <link|copy>] [--category <CATEGORY>]`

行为：
- 向 `kb.md` 添加声明项（不是直接写 `regs.json`）。
- `--mode copy` 会把文件复制到 `./.kb/raw/` 并写入 raw 声明。
- 空别名会回退为规范化路径标识。
- 分类不存在返回 `E2007`。

主要输出字段：
- `alias`
- `path`

### 4.3 `kb mv <ALIAS> <NEW_ALIAS>`

行为：
- 重命名别名标识。
- 不改变物理路径。

主要输出字段：
- `old_alias`
- `new_alias`

### 4.4 `kb rm <TARGET> [--force]`

行为：
- 从 `kb.md` 删除声明项（按 alias/id/path 匹配）。
- 不直接写 `regs.json`。
- 变更需通过 `kb update` 才会真正同步。
- 当目标是 `./.kb/raw/*` 条目时，`rm` 会先确认是否删除 raw 源文件：
  - 已确认（或传入 `--force`）才执行删除，并同时删除源文件。
  - 未确认则忽略本次操作（不删除 `kb.md` 声明，也不删除源文件）。

主要输出字段：
- 正常删除：`removed_alias`、`path`
- 忽略删除：`ignored=true`、`reason=raw_delete_not_confirmed`、`alias`、`path`

### 4.5 `kb path <ALIAS> [NEW_PATH]`

行为：
- 不传 `NEW_PATH`：查询路径。
- 传 `NEW_PATH`：更新路径（新路径必须存在）。

主要输出字段：
- 查询模式：`alias`、`path`
- 更新模式：`alias`、`old_path`、`new_path`

### 4.6 `kb chmod <TARGET> <PERMS>`

行为：
- 修改 `kb.md` 中的权限声明。
- 目标可以是引用条目，也可以是分类标题。
- 不直接写 `regs.json`。
- 权限变更会在 `kb check` 中体现为 `to_update(reason=metadata_changed)`。

权限字符：
- `r`, `w`, `x`, `c`, `d`, `-`

校验规则：
- 仅允许上述字符。
- `-` 只能单独使用。
- 不允许重复字符与空字符串。

主要输出字段：
- `alias`
- `permissions`

### 4.7 `kb list [--long] [--tree] [--filter <PATTERN>]`

行为：
- 列出 `regs.json` 中的登记条目。
- 列表行不直接由 `kb.md` 扫描生成。
- `--long` 增加 `size` 与 `mtime`。
- `--tree` 在 JSON 中额外包含 `categories`。

主要输出字段：
- `items[].alias`
- `items[].path`
- `items[].perms`
- `items[].size`（可选）
- `items[].mtime`（可选）
- `categories`（`--tree`）

### 4.8 `kb check [--raw-only] [--category <CATEGORY>] [--top <N>]`

行为：
- 比较声明源（`kb.md + raw`）与 `regs.json`。
- 只读检查，不落盘。

差异集合：
- `to_add`：已声明但未登记
- `to_update`：元数据或内容发生变化
- `to_remove`：已登记但声明已删除，或声明路径不存在

`reason` 枚举：
- `metadata_changed`
- `content_changed`
- `deleted`
- `not_found`

主要输出字段：
- `has_changes`
- `to_add`
- `to_update`
- `to_remove`
- `suggested_command`
- `duration_ms`

错误与退出：
- `kb.md` 格式非法：`E3003`（退出码 3）
- 指定分类不存在：`E2007`（退出码 2）

### 4.9 `kb update [--dry-run] [--prune] [--yes] [--full]`

行为：
- 把 `check` 差异应用到 `regs.json`。
- 刷新后端索引快照。
- 同步 `raw` 与 `kb.md`：
  - 将新增 raw 文件追加到 `# raw`
  - 删除已确认为 `not_found` 的声明行
  - 按规范化路径对 raw 声明去重

选项：
- `--dry-run`：仅预览，不写入
- `--prune`：显式清理无效/缺失条目
- `--yes`：保留兼容参数
- `--full`：全量重建索引

主要输出字段：
- `added`
- `updated`
- `removed`
- `marked_invalid`
- `duration_ms`
- `full`
- `raw_category_created`

错误与退出：
- 索引快照读取或解析失败：`E3003`（退出码 3）
- 索引持久化失败：`E1003`（退出码 1）

### 4.10 `kb ask <QUESTION> [--top <N>]`

行为：
- 从本地索引快照检索上下文。
- 当前 `engine_status` 固定为 `disabled`（不调用外部 LLM）。

主要输出字段：
- `engine_status`
- `provider`
- `trace_id`
- `retrieved_context[]`

错误与退出：
- 无可用上下文：`E5003`（退出码 5）

### 4.11 `kb env [--list]`

行为：
- 读取全局注册表并返回当前仍存在的 KB 根目录。
- 未传 `--list` 时，模式为 `list(default)`。

主要输出字段：
- `mode`
- `items[].path`
- `count`
- `recorded_total`
- `config_path`

### 4.12 `kb completion <SHELL>`

行为：
- 生成 shell 补全脚本。

主要输出字段：
- `shell`
- `script`

## 5. 操作边界总结

- `kb add`：更新 `kb.md`
- `kb rm`：更新 `kb.md`
- `kb chmod`：更新 `kb.md`（条目/分类权限）
- `kb check`：比较声明源与 `regs.json`
- `kb update`：唯一负责落盘到 `regs.json` 与索引快照
- `kb list`：读取 `regs.json`

## 6. 错误术语约定

- “错误包”指 JSON 输出中的 `error` 对象。
- “退出码”指进程返回码（如 1、2、3、5、130）。
- “E-Code”指业务错误码（如 `E2007`）。
- `message` 与 `suggestion` 的契约原文以代码实现和 `error-catalog.md` 为准。

## 7. 关联文档

- `docs/cli/kb-md-spec.md`
- `docs/cli/command-examples.md`
- `docs/cli/error-catalog.md`
- `docs/cli/state-diff-contract.md`
- `docs/cli/kbignore-spec.md`
