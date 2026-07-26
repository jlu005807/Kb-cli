# Changelog

本文件记录 KB-CLI 的用户可见变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [Unreleased]

### 修复

- **`kb mv` / `kb path` 此前是空操作**：两个命令只写入管线从不读取的
  `.kb/refs.json`，改名/改路径对 `list`/`check`/`update`/`ask` 完全不可见。
  现在它们改写 `kb.md` 声明行（保留显式权限后缀与行内注释），经
  `kb update` 收敛进索引与派生层。
- `kb add --mode copy` 落点与既有声明同名时不再静默覆盖 `./.kb/raw/`
  内容，改报 `E2006`。
- `kb update` 输出中的 `marked_invalid` 与 `raw_category_created` 此前
  硬编码为 `0`/`false`，现在报告真实值。
- kb.md 的程序化改写改用原子写协议（临时文件 + fsync + rename），进程
  中断不再可能留下半写入的 kb.md。
- 段落切分兼容 CRLF：Windows 换行（`\r\n\r\n`）的文件此前整个文件被当作
  一个 chunk，现按空行正确分段，并携带真实 `byte_range` 与内容哈希签名。
- fs 后端 schema 版本比较从字符串字典序改为数值比较（`"10.0"` 不再被误判）。
- SQLite 后端此前丢失 `KnowledgeEntry` 的记忆字段（confidence / source /
  created_at / last_verified_at / provenance），schema v4+v5 迁移后完整
  round-trip，与 fs 后端语义一致。
- 文档大量对齐实现：重写乱码的安装指南、修正 episode id 格式
  （`ep-<nanos>-<hash16>`）、`.kb/search.json` 路径、MCP 工具参数名等。

### 变更

- **移除 `kb update --yes`**：该标志从未产生任何行为；现在传入会得到
  参数解析错误。
- `kb mv`/`kb path` 以 `kb.md` 声明为真值：目标不在 `kb.md` 中时报
  `E5002`，不再对残留工作集条目做"幻影改名"。
- `kb ask --answer` 的工具循环轮数独立为 `ask_loop_max_rounds`
  （`KB_LLM_ASK_LOOP_MAX_ROUNDS`，默认 8，clamp `[4, 12]`）；
  `agent_loop_max_rounds` 现仅管 L2/L4/L5 编译的反思轮（clamp `[1, 4]`）。
  此前同一配置项被两处以互相矛盾的 clamp 消费。
- SQLite `storage.backend=sqlite` 下，依赖 fs JSON 布局的读侧工具
  （`ask --answer` 的 toolbox、MCP `read_summary`/`read_neighbors`）在
  数据缺失时附带显式 `warning`，不再静默退化。
- 打包脚本在解析 workspace 版本失败时直接报错，不再回退到过期的硬编码
  版本号。

### 新增

- 新错误码：`E2010`（路径含 `)`、`#` 或控制字符，无法安全写入 kb.md
  声明）、`E2011`（目录/glob 模式声明不可经 CLI 单行改写）。
- `LICENSE`（MIT，README 徽章此前指向不存在的文件）。
- 测试 CI（`.github/workflows/ci.yml`）：push/PR 触发，ubuntu + windows
  双平台跑 `cargo test`、`clippy -D warnings` 与文档 fixture 校验。
- 记忆系统与 MCP server 的 Rust 集成测试（此前仅有冒烟脚本且从不失败）。
- kb-llm 内部：三处重复的 LLM 协议辅助代码收敛到共享 `protocol.rs`。
