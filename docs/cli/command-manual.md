# KB-CLI 命令手册（权威）

本文档是 KB-CLI 命令行为与边界的权威说明。
若与 `command-examples.md` 或其他文档冲突，以本手册为准。

> 实现同步（2026-11-13）
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
- 声明插入到目标分类的第一个同名标题区段内、下一个任意级标题之前；分类重名时使用第一个标题。
- 冲突检测以 `kb.md` 声明为准（手工维护的引用同样参与）：别名冲突返回 `E2003`，路径已登记返回 `E2006`。

主要输出字段：
- `alias`
- `path`

### 4.3 `kb mv <ALIAS> <NEW_ALIAS>`

行为：
- 重命名别名标识：改写 `kb.md` 中对应引用行的 alias（保留显式权限后缀与行内注释），并按解析出的引用身份同步 `refs.json` 工作集。
- 目标可按 alias、path 派生 id 或路径定位（与 `rm` 一致）；给无别名声明（如 update 自动回填的 `[](./.kb/raw/x.md)`）命名的官方途径即 `kb mv ./.kb/raw/x.md <alias>`。
- 不改变物理路径。
- 不直接写 `regs.json`，变更通过 `kb update` 收敛进索引与派生层。
- 以 `kb.md` 声明为真值：目标不在 `kb.md` 中时报 `E5002`（即使 `refs.json` 工作集残留同名条目）。
- 新别名与 `kb.md` 显式别名、无别名声明的 path 派生 id 或工作集别名冲突时报 `E2003`。

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
- 不传 `NEW_PATH`：查询路径（`kb.md` 声明优先，`refs.json` 工作集兜底）。
- 传 `NEW_PATH`：更新路径（新路径必须存在，报 `E5001`）；改写 `kb.md` 对应引用行并同步 `refs.json`。
- 不直接写 `regs.json`，变更通过 `kb update` 收敛进索引与派生层。
- 新路径含 `)`、`#` 或控制字符时报 `E2010`（这些字符无法安全写入 kb.md 引用行）。
- 目标声明是目录或 glob 模式（如 `[docs](./docs/**)`）时报 `E2011`，需手工编辑 `kb.md`。
- 新路径已被其他声明持有时报 `E2006`。

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

### 4.9 `kb update [--dry-run] [--prune] [--full] [--strict] [--llm]`

行为：
- 把 `check` 差异应用到 `regs.json`。
- 刷新后端索引快照。
- 同步 `raw` 与 `kb.md`：
  - 将新增 raw 文件追加到 `# raw`
  - 已确认为 `not_found` 的路径会从登记与索引中移除；`kb.md` 声明默认保留
  - 只有传入 `--prune` 才删除对应的 `kb.md` 声明行
  - 按规范化路径对 raw 声明去重

选项：
- `--dry-run`：仅预览，不写入
- `--prune`：删除 `kb.md` 中已确认为 `not_found` 的声明行；与 `--dry-run` 同用时只预览、不改写文件
- `--full`：全量重建索引
- `--strict`：L2-L5 任一失败时整体失败（非 0 退出）
- `--llm`：本次更新强制尝试启用 L2/L4/L5 的 LLM 路径（若缺少 `KB_LLM_KEY` 会回退到规则路径并给 warning）

LLM 配置优先级：
- 环境变量
- 项目配置 `./.kb/config.json`
- 全局配置（选择顺序见 `kb env --doctor`）

分层成功策略：
- 基础层（L0->L1）失败：默认模式和严格模式都失败。
- 默认模式：L2-L5 失败记入 `warnings`，命令仍返回成功。
- 严格模式（`--strict`）：L2-L5 任一失败记入 `errors`，命令整体失败。

阶段键：

1. `l0_declaration`
2. `l1_registry`
3. `l1_index`
4. `l2_semantic`
5. `l3_projection`
6. `l4_graph`
7. `l5_wiki_view`

主要输出字段：
- `added`
- `updated`
- `removed`
- `marked_invalid`
- `duration_ms`
- `full`
- `raw_category_created`
- `strict`
- `missing_declarations`：未找到的声明路径（`alias`、`path`）；不计入 warnings，也不导致 `--strict` 失败
- `layer_status`
- `l2_compilation`
- `semantic_extractor`
- `graph_builder`
- `wiki_builder`
- `warnings`
- `errors`

`semantic_extractor` / `graph_builder` / `wiki_builder` 关键字段：
- `mode`：`llm` 或 `rule`
- `provider`：LLM 模型与 base_url（仅 `mode=llm` 时有值）
- `agent_tools.enabled`：该层 agent 原子工具上下文是否启用

常见 LLM warning 码：
- `W-L2-LLM-CONFIG-MISSING`
- `W-L4-LLM-CONFIG-MISSING`
- `W-L5-LLM-CONFIG-MISSING`
- `W-L4-LLM-ENRICH-FAILED`
- `W-L5-LLM-ENRICH-FAILED`

错误与退出：
- 索引快照读取或解析失败：`E3003`（退出码 3）
- 索引持久化失败：`E1003`（退出码 1）
- 严格模式下渐进层失败：`E1011`（退出码 1）

### 4.10 `kb ask <QUESTION> [--top <N>] [--answer]`

行为：
- 按分层顺序检索：`L3 -> L2 -> L4 -> L1 fallback`。
- 当 `L2-L4` 产物缺失或不可读时，自动回退到 `L1`，保持 `ask` 可用。
- 默认返回检索包：`mode=retrieve`，包含 `retrieved_context`、`retrieval_trace`、`reasoning_context`。
- 显式传入 `--answer` 时进入最小只读 ask-agent：先检索，再按需调用知识库工具，最后输出答案。
- 未配置 LLM 凭据时：
  - 默认 `ask` 仍可用
  - `ask --answer` 失败并返回 `E5003`
- 配置 LLM 凭据后 `engine_status=configured`。

主要输出字段：
- `mode`
- `engine_status`
- `provider`
- `trace_id`
- `retrieved_context[]`
- `retrieval_trace[]`
- `reasoning_context`
- `retrieved_context[].source_layer`（`l3_projection|l2_semantic|l4_graph|l1_index`）
- `retrieved_context[].record_id`
- `retrieved_context[].chunk_id`（`l1_index` 回退命中时可为 `null`）
- `tool_trace[]`（仅 `--answer`）
- `answer.text`（仅 `--answer`）
- `answer.citations[]`（仅 `--answer`）

`--answer` 模式下的只读原子工具：
- `search_context`
- `read_file_summary`
- `read_graph_neighbors`
- `read_graph_edge`
- `read_wiki_page`
- `read_raw_chunk`

错误与退出：
- 无可用上下文：`E5003`（退出码 5）

### 4.11 `kb remember <CONTENT> [--ref <CHUNK_ID>]... [--confidence <VALUE>]`

行为：
- 记录 agent 观察到的事实，写入 L0 episode 时间线（`.kb/episodes.jsonl` 或 sqlite `episodes` 表）。
- Episode kind 为 `Observation`，默认 confidence 为 0.8。
- `--ref` 可重复传入。`--confidence` 必须为有限数值，NaN 和无穷值导致参数解析失败（退出码 2）；存储时将有限值限制在 [0, 1]。
- 自动记录 `agent_id`（从环境变量 `KB_AGENT_ID` 读取，默认 `kb-cli`）和 `session_id`（从 `KB_SESSION_ID` 读取，可选）。

主要输出字段：
- `episode_id`
- `agent_id`
- `kind`
- `confidence`
- `refs`

完整 content、session_id、timestamp 保存在 episode 时间线中，可通过 MCP `list_episodes` 读取。

错误与退出：
- Episode store 打开或追加失败：`E1004`（退出码 1）

### 4.12 `kb correct <CONTENT> [--ref <CHUNK_ID>]...`

行为：
- 记录用户纠错，写入 L0 episode 时间线。
- Episode kind 为 `UserCorrection`，confidence 固定为 1.0（最高优先级记忆信号）。
- 用于反幻觉和记忆冲突解决。

主要输出字段：
- `episode_id`
- `agent_id`
- `kind`
- `refs`

confidence 固定为 1.0，保存在 episode 时间线中。

错误与退出：
- Episode store 打开或追加失败：`E1004`（退出码 1）

### 4.13 `kb consolidate [--min <N>] [--dry-run]`

行为：
- 将积累的 L0 episodes 固化为语义记忆文件（`.kb/raw/agent-memory/*.md`）。
- 按引用中的 chunk id 推导 record topic 后聚类；没有 refs 的 episode 归入 `general`，分组不依赖 `agent_id`。
- 新文件名为 `{slug}-{topic 的 FNV-1a 64 位哈希前 8 位}.md`；头部 metadata 的 topic 匹配时复用已有文件；短哈希冲突扩展为完整 16 位哈希，仍冲突时不覆盖并报错。
- 支持 `--dry-run` 预览固化计划而不实际写入。
- `--min` 指定每个 topic 的最少 episode 数，默认 2，下限为 1。
- 固化后的文件会在下次 `kb update` 时编译进 L1-L5 层。

主要输出字段：
- `dry_run`、`considered`、`skipped_non_assertive`、`skipped_below_threshold`
- `min_episodes_per_topic`、`memories_planned`
- `memories[]`（`topic`、实际 `file_stem`、`confidence`、`has_user_correction`、`source_episode_ids`、`contributor_agents`、`refs`）
- `written[]`、`added[]`、`already_declared`、`warnings[]`

错误与退出：
- Episode store、记忆目录或记忆文件打开、读取、写入失败：`E1004`（退出码 1）
- 完整哈希仍冲突时不覆盖文件，返回 `E1004`（退出码 1）

### 4.14 `kb serve --mcp`

行为：
- 启动 MCP (Model Context Protocol) server，通过 stdio 接受 JSON-RPC 2.0 请求。
- 暴露 8 个 tools：`recall`、`remember`、`correct`、`verify_citation`、`list_episodes`、`consolidate`、`read_summary`、`read_neighbors`。
- 用于让外部 agent（Claude Desktop / Cursor / Cline）接入记忆系统。
- Server 运行在前台，通过 stdio 通信，按 Ctrl+C 或发送 EOF 停止。
- Ctrl+C：进程以退出码 130 退出；JSON 模式的 `E9001` 写入 stderr，MCP stdout 不追加错误包。

MCP Tools：
- `recall`：检索记忆（分层检索，返回 ranked contexts）
- `remember`：写入观察（L0 episode，kind=Observation）
- `correct`：写入纠错（L0 episode，kind=UserCorrection）
- `verify_citation`：验证引用（词法校验 quote 是否真实存在于 chunk_id；源文件已变化时返回 `stale: true` 和更新提示）
- `list_episodes`：列出 episode 时间线（可按 agent_id 过滤）
- `consolidate`：触发记忆固化（支持 dry_run 预览）
- `read_summary`：读取文件语义摘要（L2）
- `read_neighbors`：读取知识图谱邻接（L4）

环境变量：
- `KB_AGENT_ID`：标识调用方 agent（默认 `kb-cli`）
- `KB_SESSION_ID`：会话 ID（可选）

错误与退出：
- 知识库未初始化：`E3001`（退出码 3）
- JSON-RPC 协议错误：写入错误响应到 stdout，继续运行
- 内部错误：写入错误响应到 stdout，继续运行
- memory episode/index 存储打开、读取或写入失败：工具返回 `E1004`（退出码不适用于常驻 server）

### 4.15 `kb env [--list] [--doctor]`

行为：
- 默认 / `--list`：读取全局注册表并返回当前仍存在的 KB 根目录。未传 `--list` 时，模式为 `list(default)`。
- `--doctor`：诊断模式，打印 LLM 配置解析链路，用于排查“key 究竟从哪里读到 / 为什么 LLM 没启用”：
  - `key` / `model` / `base_url` 各自最终生效的来源（`process_env` / `keyring` / `project_config` / `global_config` / `defaults`）
  - `semantic/graph/wiki` 开关的生效值
  - keyring 状态：当前 `service` / `user` 名（含环境变量覆盖影响）、是否被 `KB_LLM_DISABLE_KEYRING=1` 关闭、对应记录是否存在（不打印 key 本身）
  - 配置解析 trace（逐层解析摘要）
- `--doctor` 与 `--list` 同时传入时，`--doctor` 优先（此时列表信息无意义）。

全局配置文件的选择顺序：
1. 非空 `KB_GLOBAL_CONFIG`。
2. 已存在的可执行文件安装目录 `config.json`（可执行文件位于 `bin/` 时检查其父目录）。
3. 用户目录下的 `.kb-cli/config.json`（Windows 使用用户目录，其他平台使用 home 目录）。

主要输出字段（默认 / `--list` 模式）：
- `mode`
- `items[].path`
- `count`
- `recorded_total`
- `config_path`

主要输出字段（`--doctor` 模式）：
- `mode`（固定 `doctor`）
- `cwd`
- `global_config.path`：实际选中的全局配置文件
- `global_config.rule` / `global_config.source`：命中的选择规则编号与来源
- `llm.model` / `llm.base_url` / `llm.key_present` / `llm.semantic_enabled` / `llm.graph_enabled` / `llm.wiki_enabled`
- `key_source`
- `source_map.key` / `source_map.model` / `source_map.base_url`
- `keyring.service` / `keyring.user` / `keyring.disabled` / `keyring.key_present`
- `trace`

### 4.16 `kb completion <SHELL>`

行为：
- 生成 shell 补全脚本。

主要输出字段：
- `shell`
- `script`

### 4.17 并发写入保护

对同一个知识库，同时只允许一个 `init`、`add`、`mv`、`rm`、`path`、`chmod`、非 dry-run `update` 或非 dry-run `consolidate` 执行。MCP 的 `consolidate` 只在该次工具调用内持锁；只读命令、`remember`、`correct` 和 `serve` 进程本身不持有 writer lock。发生冲突时立即返回 `E3004`（退出码 3），详情包含锁文件记录的持锁进程 PID。系统不支持文件锁的平台会在 `-v` 下提示并继续执行。

`Ctrl+C` 以 `E9001` / 退出码 130 结束进程。JSON 模式的中断错误写入 stderr；MCP stdout 只保留 JSON-RPC 响应流。中断期间原子写可能留下 `.<name>.<nonce>.tmp` 临时文件，可以在确认进程已退出后手动删除。

## 5. 操作边界总结

- `kb add`：更新 `kb.md`
- `kb rm`：更新 `kb.md`
- `kb chmod`：更新 `kb.md`（条目/分类权限）
- `kb check`：比较声明源与 `regs.json`
- `kb update`：唯一负责落盘到 `regs.json` 与索引快照
- `kb list`：读取 `regs.json`
- `kb remember`：写入 L0 episode（Observation）
- `kb correct`：写入 L0 episode（UserCorrection）
- `kb consolidate`：L0 episodes → `.kb/raw/agent-memory/*.md`
- `kb serve --mcp`：启动 MCP server（stdio JSON-RPC）

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
