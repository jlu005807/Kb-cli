# Changelog

本文件记录 KB-CLI 的用户可见变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [Unreleased]

### 修复

- Linux 安装/卸载正确转义自定义目录，保留 rc 符号链接与权限，写失败不截断原文件；发布包验收覆盖默认 PATH 并隔离继承的模型/运行时配置。
- runtime 的 quiet/verbose/no_color 改为逐字段合并，环境与文件配置真实生效，保留显式 false/0；命令处理器使用解析后的 verbose。
- 显式存储配置非法时报告错误，不再静默写入 FS；ask 的存储打开/索引读取错误改为 E1004，MCP summary/neighbors 也报告非法后端配置。
- FS episode 读取按字节分行恢复，损坏 UTF-8 行不再阻断有效历史；查询按时间和 id 排序后选取最近 N 条。
- `kb add --mode copy` 同样保护尚未登记的 raw 文件，不覆盖已存在的目标；保留源权限，源打开失败不残留新文件。
- `kb init` 不再覆盖不可读、损坏或字段结构不受支持的全局配置；报告注册失败并保留原文件。全局注册表及 LLM 配置读取兼容 UTF-8 BOM。
- 精简重复测试覆盖：文档 fixture parity 统一由 CI 已运行的 Node 校验器负责，删除重复 Rust 校验目标及重复 helper 测试；移除依赖日历日期的 `Timestamp::now` 断言，并清理未使用的测试依赖。
- 发布包验收修复：Windows 安装器使用无 BOM UTF-8 并保留未知用户配置，损坏配置不再静默覆盖；Windows/Linux 打包脚本遵守 `CARGO_TARGET_DIR`，Linux 版本号解析兼容 CRLF。CI 增加打包与安装回归检查。
- 文档 fixture 校验现在要求每个受检命令都包含成功与失败 JSON 示例；补充了缺失示例的回归测试。
- 移除 KB 总览路径中恒真的 `Some` 包装及 ask agent 不可达错误分支；memory 文件名的 `.md` 后缀改为显式不变量检查。
- 移除未使用的存储后端标签 API 和冗余 SQLite connection clone；FS/SQLite 选择测试改为断言实际数据库文件副作用。
- 陈旧或损坏的 chunk byte range 不再 panic：raw chunk 读取校验 UTF-8 边界及 FNV-1a 内容签名，验证失败时回退到索引 preview 并返回 `stale`；MCP `verify_citation` 会提示运行 `kb update`，单次工具 panic 也不会终止后续 JSON-RPC 请求。
- 修正 Windows 盘符 chunk id 解析，`D:/notes/a.md:0002` 不再被截成 `D`。
- `kb remember --confidence` 拒绝 NaN 和无穷值，参数解析失败且不追加 episode；领域层也忽略非有限 confidence，保留默认值。
- 修复 `kb.md` UTF-8 BOM 导致的 front matter、首行标题识别错误；程序化改写保留 BOM、LF/CRLF 和尾部换行（混合行尾按第一处风格统一）。
- 修复 `kb add --category` 遇到子标题时把声明写入错误分类的问题；现在插入目标分类首个标题区段的末尾。历史错放条目不会自动迁移，请结合 `kb check` 和 `kb list --tree` 检查并手工调整 `kb.md`。
- `kb update` 默认保留 `kb.md` 中缺失路径的声明，仅从登记/索引移除；只有 `--prune` 删除声明。`missing_declarations` 独立报告，不影响 `--strict`。
- 防止 consolidate 产物 slug 碰撞覆盖不同 topic；恢复以 episode 时间线为准的独立记忆文件。文件系统 episode log 尾部无换行时，追加前会先分隔新记录。
- CLI 记忆命令及 MCP 存储工具的存储故障使用 `E1004`，不再误报为 `kb.md` 格式错误 `E3003`。
- CLI 写命令获得单写者文件锁；Ctrl+C 立即返回 130，JSON 错误写 stderr，MCP stdout 不被错误包污染。
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

- **移除全局 `--config` 参数**：改用 `KB_GLOBAL_CONFIG` 选择全局配置；旧 `E2008` 编号保留并标记废弃。
- `kb update` 输出新增 `missing_declarations`；缺失声明默认保留，只有 `--prune` 才删除 `kb.md` 行。
- 新建 consolidate 记忆文件改用 `{slug}-{FNV-1a(topic) 前 8 位}.md`，冲突时扩展为完整 16 位哈希；已有文件头 metadata topic 匹配时复用原文件路径。
- 全局配置统一按非空 `KB_GLOBAL_CONFIG`、已存在的可执行文件安装目录 `config.json`、用户目录 `.kb-cli/config.json` 的顺序选择；新安装且没有安装目录配置时使用用户目录，已有安装目录配置继续生效。
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

- 新错误码 `E3004` 用于并发 writer lock 冲突；锁冲突立即失败并尽可能报告持锁 PID。
- 新错误码 `E1004` 用于记忆与 MCP 存储打开、读取、写入失败。
- `kb env --doctor` 新增所选全局配置路径和规则来源；memory consolidate 输出新增 `already_declared` 与 `warnings`。
- workspace Rust 版本元数据更新为 1.89；Windows x86_64 MSVC 和 WSL Ubuntu x86_64 GNU 上使用 Rust 1.89.0 对当前 `Cargo.lock` 的 Clippy 和 workspace 测试已通过。
- 新错误码：`E2010`（路径含 `)`、`#` 或控制字符，无法安全写入 kb.md
  声明）、`E2011`（目录/glob 模式声明不可经 CLI 单行改写）。
- `LICENSE`（MIT，README 徽章此前指向不存在的文件）。
- 测试 CI（`.github/workflows/ci.yml`）：push/PR 触发，ubuntu + windows
  双平台跑 `cargo test`、`clippy -D warnings` 与文档 fixture 校验。
- 记忆系统与 MCP server 的 Rust 集成测试（此前仅有冒烟脚本且从不失败）。
- kb-llm 内部：三处重复的 LLM 协议辅助代码收敛到共享 `protocol.rs`。
