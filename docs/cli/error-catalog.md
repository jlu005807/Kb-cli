# KB-CLI 错误目录

本文档定义 KB-CLI 的错误模型、退出码分层与已实现的错误码（E-Code）。
本文档随实现记录错误码变化；message、suggestion 与退出码是 CLI 的稳定契约。

> 相关文档：
> - `docs/cli/command-manual.md`
> - `docs/cli/command-examples.md`
> - `docs/cli/state-diff-contract.md`
> - `docs/cli/kbignore-spec.md`
> - `docs/cli/kb-md-spec.md`

## 错误模型

KB-CLI 按 `--output` 模式输出错误：

- `text`：向 `stderr` 输出人类可读错误信息，并以非零退出码退出。
- `json`：向 `stdout` 输出结构化错误对象；日志与诊断信息走 `stderr`。

### JSON 错误包契约

当前稳定版本：

```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E1001",
    "message": "Failed to write configuration",
    "suggestion": "Check file permissions and disk space",
    "details": {}
  }
}
```

约束说明：

- `schema_version` 当前固定为 `"1.0"`。
- `code/message/suggestion/details` 字段结构保持稳定。
- 本文档中的英文 `message` 与 `suggestion` 以当前实现为准。

## 退出码

| 退出码 | 类别 | 说明 |
|---|---|---|
| `0` | 成功 | 命令成功执行 |
| `1` | 系统错误 | 写入失败、持久化失败等内部错误 |
| `2` | 输入错误 | 参数、别名、权限、分类、路径策略等校验失败 |
| `3` | 状态错误 | 工作区初始化状态或 `kb.md` 格式问题 |
| `4` | 权限错误 | 预留分层（当前无 E4xxx 实现） |
| `5` | 资源错误 | 目标路径/别名/上下文不存在 |
| `6` | 外部服务错误 | 预留分层（当前无 E6xxx 实现） |
| `130` | 中断 | 用户中断（SIGINT / Ctrl+C） |

## 错误码分段规则

- `E1xxx`：系统/持久化类错误（通常对应退出码 1）
- `E2xxx`：用户输入与参数类错误（通常对应退出码 2）
- `E3xxx`：工作区与状态类错误（通常对应退出码 3）
- `E4xxx`：权限策略类错误（当前未落地）
- `E5xxx`：资源定位类错误（通常对应退出码 5）
- `E6xxx`：外部服务交互类错误（当前未落地）
- `E9xxx`：中断类错误（当前 `E9001` 对应退出码 130）

## 已实现错误码

说明：

- “message（契约原文）”与“suggestion（契约原文）”来自当前实现，供 JSON 消费方稳定解析。
- “中文释义”仅用于文档阅读，不影响机器契约。

| E-Code | 退出码 | message（契约原文） | 中文释义 | suggestion（契约原文） | 典型命令 |
|---|---|---|---|---|---|
| `E1001` | `1` | `Failed to write configuration` | 配置写入失败 | `Check file permissions and disk space` | `init` 等涉及配置写入 |
| `E1003` | `1` | `Failed to persist index files` | 索引文件持久化失败 | `Check write permissions for .kb directory and retry` | `update` |
| `E1004` | `1` | `Storage unavailable` | 存储不可用或存储配置非法 | `Check storage files under .kb and retry` | `remember`, `correct`, `consolidate`, `ask`、MCP 存储工具 |
| `E1011` | `1` | `Strict mode failed on progressive layer error` | 严格模式下渐进层失败 | `Retry after resolving failed layers or run without --strict` | `update --strict` |
| `E2001` | `2` | `Alias contains invalid characters` | 别名字符不合法 | `Use only letters, digits, -, _, ., / and length 1-128` | `add`, `mv` |
| `E2002` | `2` | `Invalid glob pattern` | glob 过滤模式非法 | `Verify the filter string is a valid glob pattern` | `list` |
| `E2003` | `2` | `Alias conflict` | 别名冲突 | `Choose another alias or cancel` | `add`, `mv` |
| `E2004` | `2` | `Invalid permission string` | 权限串非法 | `Use a valid combination of r, w, x, d, c, or -` | `chmod` |
| `E2006` | `2` | `Path already registered` | 路径已被登记 | `Use a different path or remove existing registration first` | `add`, `path` |
| `E2007` | `2` | `Category not found` | 分类不存在 | `Run 'kb list --tree' to view available categories and try again` | `add --category`, `check --category` |
| `E2008` | `2` | `Configuration file not found` | **已废弃**：旧版 `--config` 参数对应错误；当前 CLI 不再生成此错误码 | `Deprecated; use KB_GLOBAL_CONFIG to select a global config file` | 仅用于识别旧版脚本错误 |
| `E2009` | `2` | `Path is protected and cannot be indexed` | 受保护路径不可索引 | `Use normal content paths or ./.kb/raw/*, avoid kb.md and .kb config files` | `add` |
| `E2010` | `2` | `Path contains characters that cannot be declared in kb.md` | 路径含 `)`、`#` 或控制字符，无法安全写入 kb.md 引用行 | `Rename the target to avoid ')', '#', or control characters in the path` | `add`, `path` |
| `E2011` | `2` | `Declaration is a directory or glob pattern and cannot be rewritten via CLI` | 目录/glob 声明展开为多条记录，单行改写会静默注销全部展开条目 | `Edit the pattern declaration in kb.md manually` | `path` |
| `E3001` | `3` | `Knowledge base is not initialized` | 未初始化知识库 | `Run \`kb init\` in current directory` | 除 `init/env/completion` 外大多数命令 |
| `E3002` | `3` | `Knowledge base already initialized` | 知识库已初始化 | `Use --force to overwrite or run in a different directory` | `init` |
| `E3003` | `3` | `Invalid kb.md format` | `kb.md` 格式非法 | `Fix kb.md syntax and run kb check again` | `check`, `update`, `mv`, `path`, `rm`, `chmod` |
| `E3004` | `3` | `Another kb process is writing this knowledge base` | 另一个 kb 进程正在写入此知识库 | `Wait for the other kb command to finish and retry` | 并发写命令 |
| `E5001` | `5` | `Path does not exist` | 路径不存在 | `Verify the file or directory exists at the provided path` | `add`, `path` |
| `E5002` | `5` | `Alias does not exist` | 别名不存在 | `Run 'kb list' to see available aliases` | `mv`, `rm`, `path`, `chmod` |
| `E5003` | `5` | `No relevant context found` | 没有可用上下文 | `Try a different question or add more files to the knowledge base` | `ask` |
| `E9001` | `130` | `Operation interrupted` | 操作被中断 | `Retry the command when ready` | 任意可中断命令 |

## 维护要求

- 增删错误码时，必须同步更新：
  - `crates/kb-cli/src/errors.rs`
  - `docs/cli/command-manual.md`
  - `docs/cli/command-examples.md`（如涉及示例）
- 不得在未更新文档和测试的情况下调整 `message/suggestion` 契约文本。
