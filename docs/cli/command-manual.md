# KB-CLI 命令手册

本文档为 KB-CLI 命令体系的**权威参考**，用于说明命令语法、参数、选项、语义、错误行为与边界条件。

> **权威声明：** 当 `command-examples.md` 或 `error-catalog.md` 与本手册不一致时，以本手册为准。
>
> **关联文档：**
> - `docs/cli/kb-md-spec.md`
> - `docs/cli/state-diff-contract.md`
> - `docs/cli/kbignore-spec.md`
>
> **开发说明：** CLI 解析建议使用 `clap` derive 宏实现。

> **实现状态补充（2026-04-15）：**
> - `rm --force` 与 `update --yes` 当前为保留参数位，不改变执行逻辑。
> - `check/update` 当前快照来源为 `.kb/refs.json` 投影记录。
> - `list --long` 的 `mtime` 当前输出 Unix 秒字符串。

## 全局选项

| 选项 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `--output <text\|json>` | 枚举 | `text` | 设置输出格式 |
| `--quiet` | 标志 | 关闭 | 抑制所有非必要输出 |
| `--verbose` | 计数 | `0` | 提高日志详细程度，可叠加 `-v` `-vv` |
| `--config <PATH>` | 字符串 | 平台默认 | 覆盖默认配置文件路径 |
| `--no-color` | 标志 | 关闭 | 禁用输出中的 ANSI 颜色 |
| `--help` | 标志 | - | 显示帮助信息并退出 |
| `--version` | 标志 | - | 显示版本信息并退出 |

## 输出规则

| 模式 | 标准输出 (stdout) | 标准错误 (stderr) |
|---|---|---|
| `text` | 供人阅读的命令执行结果、数据列表、提示信息 | 警告、错误信息、执行日志（根据 verbose 级别） |
| `json` | 仅输出结构化的 JSON 结果对象 | 所有日志与人类可读的提示信息、警告、错误文本 |

### JSON 成功响应示例
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {}
}
```

### JSON 错误响应示例
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E1001",
    "message": "配置文件写入失败",
    "suggestion": "检查文件权限和磁盘空间",
    "details": {}
  }
}
```

## 前置条件

除 `kb init` 外，所有命令均要求当前目录存在 `kb.md`。若未初始化，命令将中断并返回 `E3001`（退出码 3）。

## `.kbignore` 规则（MVP）

> 详细语法与交互边界见：`docs/cli/kbignore-spec.md`。

- 文件位置：`./kb/.kbignore`
- 语法子集：
  - 空行与 `#` 注释
  - 简单通配符：`*`、`?`
  - 目录限定：尾随 `/`
- 命中 `.kbignore` 的路径不会进入知识库索引。
- 当前版本不包含复杂规则（如复杂反选链）。

## 命令集

### `kb init`

* **语法：** `kb init [NAME] [--force]`
* **参数：**
  * `NAME`（可选）：知识库名称。默认值为当前目录名。
* **选项：**
  * `--force`：覆盖已存在的初始化结构。
* **语义：**
  * **成功：** 创建 `kb.md` 与 `.kb/`，并在 `./kb/` 下创建空 `.kbignore`。若使用 `--force`，则在 JSON 输出中返回 `overwritten: true`。
  * **失败：** 未使用 `--force` 但当前目录已初始化，返回 `E3002`（退出码 3）。配置文件写入失败，返回 `E1001`（退出码 1）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `project` | string | 项目名称 |
| `path` | string | 初始化路径 |
| `overwritten` | bool | 是否覆盖（仅在提供 `--force` 时存在） |

### `kb add`

* **语法：** `kb add <PATH> [ALIAS] [--mode <link|copy>] [--category <CATEGORY>]`
* **参数：**
  * `PATH`（必填）：目标文件或目录路径，必须存在。
  * `ALIAS`（可选）：自定义别名。
* **选项：**
  * `--mode <link|copy>`：添加模式。默认 `link`。
    * `link`：仅建立路径符号索引，不改变原文件结构。
    * `copy`：复制文件到 `./.kb/raw/`，后续由 `kb update` 自动归档到 `kb.md`。
  * `--category <CATEGORY>`：将条目添加到指定分类。若分类不存在，返回 `E2007` 并提示可选分类。
* **别名解析规则（3 步）：**
  1. 若提供了 `ALIAS` 且不为空，则使用该别名。
  2. 若省略 `ALIAS` 或为空，则使用规范化后的 `PATH` 作为别名。
  3. 规范化规则：绝对路径保持绝对；相对路径保持相对；消除 `./` 并压缩 `//` 为 `/`。
* **语义：**
  * **成功：** 使用解析后的别名注册路径引用。
  * **命中忽略：** 若 `PATH` 命中 `./kb/.kbignore` 规则，则输出提示并忽略本次添加（不写入索引，不作为错误）。
  * **失败：** 路径不存在返回 `E5001`（退出码 5）；别名冲突返回 `E2003`（退出码 2，提示用户重新指定或取消）；路径已使用其他别名注册返回 `E2006`（退出码 2）；别名包含非法字符返回 `E2001`（退出码 2）；指定分类不存在返回 `E2007`（退出码 2）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `alias` | string | 解析后的别名 |
| `path` | string | 目标路径 |

### `kb mv`

* **语法：** `kb mv <ALIAS> <NEW_ALIAS>`
* **参数：**
  * `ALIAS`（必填）：当前别名，必须存在。
  * `NEW_ALIAS`（必填）：目标别名，必须不存在。
* **语义：**
  * **成功：** 重命名别名，物理路径和权限保持不变。
  * **失败：** 源别名不存在返回 `E5002`（退出码 5）；目标别名已存在返回 `E2003`（退出码 2）；目标别名不合法返回 `E2001`（退出码 2）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `old_alias` | string | 原别名 |
| `new_alias` | string | 新别名 |

### `kb rm`

* **语法：** `kb rm <ALIAS> [--force]`
* **参数：**
  * `ALIAS`（必填）：待移除的别名，必须存在。
* **选项：**
  * `--force`：保留参数（当前实现不区分是否传入该参数）。
* **交互行为：** 当前版本无交互确认流程，命令会直接执行“移除引用”。
* **语义：**
  * **成功：** 移除别名引用，**不会**删除实际物理文件。
  * **失败：** 别名不存在返回 `E5002`（退出码 5）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `removed_alias` | string | 被移除的别名 |
| `path` | string | 原关联的物理路径 |

### `kb path`

* **语法：** `kb path <ALIAS> [NEW_PATH]`
* **参数：**
  * `ALIAS`（必填）：目标别名，必须存在。
  * `NEW_PATH`（可选）：新物理路径。
* **语义：**
  * **成功（查询）：** 未提供 `NEW_PATH` 时，返回当前的物理路径。
  * **成功（更新）：** 提供 `NEW_PATH` 时，验证新路径存在后更新引用，输出包含旧路径。
  * **失败：** 别名不存在返回 `E5002`（退出码 5）；新路径不存在返回 `E5001`（退出码 5，仅在更新模式下）。
* **输出字段表（JSON 模式）：**

| 字段（查询） | 类型 | 说明 |
|---|---|---|
| `alias` | string | 别名 |
| `path` | string | 当前路径 |

| 字段（更新） | 类型 | 说明 |
|---|---|---|
| `alias` | string | 别名 |
| `old_path` | string | 旧路径 |
| `new_path` | string | 新路径 |

### `kb chmod`

* **语法：** `kb chmod <ALIAS> <PERMS>`
* **参数：**
  * `ALIAS`（必填）：目标别名。
  * `PERMS`（必填）：权限字符串。
* **权限字符集表：**
  * `r`：读取
  * `w`：写入
  * `x`：执行
  * `d`：删除
  * `c`：配置
  * `-`：无权限（必须单独使用）
* **校验规则：** 仅允许包含 `[r,w,x,d,c,-]`；如果使用 `-` 则不能包含其他字符；字符不能重复；空字符串无效。
* **语义：**
  * **成功：** 更新权限，并返回新的权限字符串。
  * **失败：** 别名不存在返回 `E5002`（退出码 5）；无效权限字符串返回 `E2004`（退出码 2）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `alias` | string | 别名 |
| `permissions` | string | 更新后的权限字符串 |

### `kb list`

* **语法：** `kb list [--long] [--tree] [--filter <PATTERN>]`
* **选项：**
  * `--long`：显示详细信息（修改时间 mtime，大小 size）。
* `--tree`：以树状格式输出；在 JSON 模式下返回 `data.categories` 作为分类发现的权威来源。
  * `--filter <PATTERN>`：使用 glob 模式过滤结果。
* **语义：**
  * **成功：** 返回匹配的条目。列表为空**不**算作错误。
  * **失败：** 无效的 glob 模式返回 `E2002`（退出码 2）。
* **输出字段表（JSON 模式）：**

| 字段（短格式） | 类型 | 说明 |
|---|---|---|
| `items` | array | 条目数组（包含 `alias`, `path`, `perms` 字段） |

| 字段（--long 附加字段） | 类型 | 说明 |
|---|---|---|
| `mtime` | string | Unix 时间戳秒字符串 |
| `size` | number | 文件大小（字节） |

| 字段（--tree 附加字段） | 类型 | 说明 |
|---|---|---|
| `categories` | array | 可用分类名称列表（`--category` 的候选值，如 `raw`, `docs`, `src`） |

### `kb ask`

* **语法：** `kb ask <QUESTION> [--top <N>]`
* **参数：**
  * `QUESTION`（必填）：自然语言查询内容。
* **选项：**
  * `--top <N>`：返回最相关的 N 个上下文，默认值为 1。
* **语义：**
  * **成功：** 返回检索到的上下文与占位结构，`engine_status` 固定为 `"disabled"`；**不会**触发实际的 AI 生成。
  * **失败：** 无可用上下文（知识库为空或无匹配项）返回 `E5003`（退出码 5）。
* **检索与摘要算法（实现约束）：**
  * 检索基于本地 `IndexSnapshot.records`，仅使用 `present=true` 的记录参与打分。
  * 召回排序使用确定性规则：先按 `score` 降序，再按 `alias` 升序，再按 `path` 升序。
  * 摘要优先使用持久化 `excerpt` 字段。
  * 摘要窗口采用 **query-token 命中密度窗口**：在固定窗口长度内选择 query 命中密度最高的片段。
  * 窗口同分时采用显式 tie-break：先最小 `start`，再最小 `end`。
  * 摘要切片按 **grapheme cluster** 边界处理（`unicode-segmentation`，extended clusters），避免组合字符与 ZWJ emoji 被不自然截断。
  * 若无命中窗口，则回退到稳定截断（仍保持 grapheme 边界安全）。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `engine_status` | string | 固定返回 `"disabled"` |
| `provider` | string\|null | 固定返回 `null` |
| `trace_id` | string\|null | 固定返回 `null` |
| `retrieved_context` | array | 检索到的上下文数组（包含 `alias`, `snippet`, `score` 0-1 的数字） |

### `kb check`

> 详细状态机、判定优先级与效率策略见：`docs/cli/state-diff-contract.md`。
>
> `kb.md` 格式与解析约束见：`docs/cli/kb-md-spec.md`。

* **语法：** `kb check [--raw-only] [--category <CATEGORY>] [--top <N>]`
* **选项：**
  * `--raw-only`：仅检查 `path` 以 `./.kb/raw/` 开头的已注册条目。
  * `--category <CATEGORY>`：仅输出指定分类下的差异。若分类不存在，返回 `E2007`（退出码 2）。
  * `--top <N>`：限制每种差异类型返回的条目数，默认 `50`。
* **语义：**
  * 只做检查，不修改 `kb.md`、不写入 `.kb/index*.json`。
  * 当前快照来源为 `.kb/refs.json` 投影记录（而非目录全量扫描）。
  * 计算差异前应用 `./kb/.kbignore`；命中规则的路径不进入差异集合。
  * 输出“待新增 / 待更新 / 待删除（或待标记无效）”列表，供用户确认后执行 `kb update`。
  * 状态判定基线：以“最近一次成功且非 `--dry-run` 的 `kb update` 结果快照”为比较基线；若不存在基线，则按空基线处理。
  * 状态判定键：默认使用规范化路径作为 `doc_id`（路径身份）。在该策略下，重命名/迁移按“旧路径缺失 + 新路径新增”处理，而不是 `path_changed` 更新。
  * 判定优先级（高→低）：`invalid`（不可读/冲突）→ `to_add` 与 `to_remove`（missing_path 观测）→ `to_update` 与 `unchanged`。
  * `to_remove` 语义：表示“缺失路径候选项”；是否真正删除由 `kb update --prune` 决定。
  * 失败：
    * 分类读取失败（`kb.md` 无法读取）-> `E3003`（退出码 3）
    * 指定分类不存在 -> `E2007`（退出码 2）
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `has_changes` | bool | 是否存在任何差异 |
| `to_add` | array | 待新增条目（`alias`, `path`, `source`） |
| `to_update` | array | 待更新条目（`alias`, `path`, `reason`） |
| `to_remove` | array | 待删除或待标记无效条目（`alias`, `path`, `reason`） |
| `reason` | enum | 枚举值：`content_changed`（适用 to_update）、`missing_path`（适用 to_remove）、`invalid`（适用 to_remove）。<br>**注意：当前身份策略（doc_id=path）下已移除 `path_changed` 支持。** |
| `suggested_command` | string | 建议执行命令（如 `kb update` 或 `kb update --prune`） |
| `duration_ms` | number | 耗时（毫秒） |

### `kb update`

> 详细状态机、判定优先级与效率策略见：`docs/cli/state-diff-contract.md`。
>
> `kb.md` 格式与解析约束见：`docs/cli/kb-md-spec.md`。

* **语法：** `kb update [--dry-run] [--prune] [--yes] [--full]`
* **选项：**
  * `--dry-run`：仅显示将发生的变更，不写入文件。
  * `--prune`：删除无效/不存在路径；默认仅标记并提示。
  * `--yes`：保留参数（当前实现不区分是否传入该参数）。
  * `--full`：执行全量重建。
* **语义：**
  * 当前快照来源：`.kb/refs.json` 投影记录（仅保留实际存在的路径）。
  * 差异过滤：应用 `./kb/.kbignore`；命中规则的路径不参与索引生成与变更统计。
  * 输出写入（原子化）：
    * `.kb/index.json`
    * `.kb/index.errors.json`
    * `.kb/index.meta.json`
  * 失败：
    * 基线索引快照读取/解析失败 -> `E3003`（退出码 3）
    * 索引持久化失败 -> `E1003`（退出码 1）
  * 更新/删除判定：
    * `added`：当前快照存在、基线不存在。
    * `updated`：当前快照与基线同一 `doc_id`（路径）下内容签名变化。
    * `marked_invalid`：基线存在但当前缺失（`missing_path`），且未启用 `--prune`。
    * `removed`：基线存在但当前缺失（`missing_path`），且启用 `--prune`。
  * `--dry-run`：仅输出计划，不写 `.kb/index*.json`，也不更新基线快照。
  * 幂等性：在输入文件集与配置不变时，连续执行两次 `kb update`，第二次应返回 `added=0, updated=0, removed=0`。
* **输出字段表（JSON 模式）：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `added` | number | 新增条目数 |
| `updated` | number | 更新条目数 |
| `removed` | number | 移除条目数（仅 `--prune`） |
| `marked_invalid` | number | 标记为无效的条目数 |
| `duration_ms` | number | 耗时（毫秒） |
| `full` | bool | 是否为全量重建 |
| `raw_category_created` | bool | 本次是否创建了 `# raw` 分类 |

## 别名命名规则

| 规则 | 示例 | 结果 |
|---|---|---|
| 允许使用字母、数字、`-`、`_`、`.`、`/` | `my-docs`, `v2/config` | 合法 |
| 禁止使用空格 | `my docs` | 非法 |
| 禁止以 `-` 开头 | `-alias` | 非法 |
| 长度必须在 1-128 之间 | `a` (合法), 空字符串 (非法) | 长度违规 |
| 禁止使用保留字 | `init`, `help`, `version` | 非法 |

违反上述任意规则均返回 `E2001`（退出码 2）。

## 路径规范化规则

| 输入路径 | 规范化结果 | 说明 |
|---|---|---|
| `./docs` | `./docs` | 相对路径统一保留 `./` 前缀 |
| `../docs` | `./../docs` | 相对路径统一加前缀，保留父目录语义 |
| `/home/user/docs` | `/home/user/docs` | 绝对路径保持不变 |
| `docs//src` | `./docs/src` | 压缩 `//` 并补齐 `./` 前缀 |
| `docs/./src` | `./docs/./src` | 当前实现不会清理中间段的 `./` |
