# 修复计划 v2（2026-11 审查）

状态：v2 已执行。阶段 0–6 均已修复、验证、评审并分别提交。初稿中“待确认”“不提交 commit”及由用户代跑门禁的流程，已被后续明确的逐阶段实施、验证、评审和提交要求取代。

| 阶段 | 提交 |
|---|---|
| 0 测试隔离与基线 | `22fdb4f` |
| 1 崩溃与解析 | `6dc2aa9` |
| 2 kb.md 写路径 | `741e400` |
| 3 记忆系统 | `544f3d1` |
| 4 并发与中断 | `3c8c53a` |
| 5 配置与 CLI 面 | `156e54a` |
| 6 文档对齐 | `7e5e566` |

验证记录：当前 `Cargo.lock` 在 Rust 1.89.0 上通过两平台门禁：Windows Clippy 无警告、362 项测试通过；WSL Ubuntu Clippy 无警告、361 项测试通过。Windows `CTRL_C_EVENT`、并发 update 冲突和中断后恢复、doc-fixture 与文档相对链接检查均通过。

Cursor 的项目级/全局 MCP 路径已由 [Cursor Staff 回复](https://forum.cursor.com/t/can-cursor-cli-support-add-mcp-server-parameter/167812/4)确认，该回复链接到 [Cursor MCP 文档](https://cursor.com/docs/cli/mcp)。Cline 的扩展配置入口和默认文件路径已按 [MCP 设置说明](https://docs.cline.bot/mcp/mcp-overview) 与[配置目录说明](https://docs.cline.bot/getting-started/config)核验。Claude Code CLI 在隔离配置目录中完成 initialize 和 tools/list，收到全部 8 个工具；未启动模型会话。Linux SIGINT 集成用例已在 WSL Ubuntu、Rust 1.89.0 和 1.94.1 上通过。

## 0. 约束与已定决策

实现约束：
- 不新增依赖。
- 初稿提出不提交 commit；后续用户要求每阶段独立提交，实施时按后续要求提交。

初稿的会话限制已被后续用户授权取代：允许逐阶段实施、运行门禁、独立提交和联网开展最终验收。最终验收使用隔离知识库、当前锁定依赖和本机 mock LLM 服务；客户端测试使用独立配置目录。

已定决策：
- #8 移除全局 `--config` 参数。
- #12 `kb update` 默认不删 kb.md 声明行，只有 `--prune` 才删。

统一门禁（每阶段实施后执行，全部通过才进入下一阶段）：

```bash
export TMP="$PWD/target/tmp" TEMP="$PWD/target/tmp" KB_LLM_DISABLE_KEYRING=1
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node tests/tools/validate-doc-fixture-diff.js
```

## 1. 评审记录：v1 → v2 的修正

| # | v1 方案的问题 | v2 处理 |
|---|---|---|
| 1 | 把下标收敛到字符边界，但文件已变更时切出的是错误内容，且静默返回 | 用 chunk 的 `content_signature` 校验切片；不一致时标记 `stale`，回退到索引时的 preview |
| 1 | 只提了 MCP；`ask --answer` 也调用同一函数（`ask/agent.rs:408`、`ask/tools.rs:441`） | 修根因，两条路径同时覆盖；MCP 另加 panic 兜底 |
| 2 | "插到下一个同级或更高级标题前"，遇到子标题时新条目会落进子分类 | 插到目标标题之后的第一个任意级标题之前 |
| 3 | 文件名一律加哈希，会让所有老用户的产物改名，新旧两份声明重复入索引 | 先按产物头部元数据里的 topic 复用已有文件，只有新 topic 才用带哈希的名字 |
| 6+9 | `exit(130)` 不执行 Drop，锁文件残留；10 分钟超时会抢走仍在运行的长 `update --llm` 的锁 | 改用操作系统文件锁 `File::try_lock`，进程退出（含 Ctrl+C、被杀）时由系统自动释放 |
| 9 | 给 remember/correct 也加锁、consolidate 内部调用 add 会重入、serve 常驻持锁会挡住所有 CLI 写命令 | 只在命令边界加锁，范围见阶段 4 |
| 7 | 漏了测试隔离：多数集成测试执行 `init` 时没设 `KB_GLOBAL_CONFIG`，改动后会写到真实的 `~/.kb-cli/config.json` | 新增阶段 0，作为 #7 的前置 |
| 7 | 测试会继承开发者的环境变量（`KB_LLM_KEY`、keyring） | 阶段 0 的测试 helper 统一清理 |
| 12 | 把"声明缺失"放进 `warnings`，会让 `--strict` 因缺文件而失败 | 用独立的输出字段，不进入 strict 判定 |
| 5 | MCP 走 JSON，本来就传不了 NaN，v1 写了两处入口 | 只在 CLI 入口与领域层处理 |
| 新 | episodes.jsonl 末尾若有半行（崩溃残留、无换行），下一次追加会和它拼成一行，新 episode 也随之丢失 | 新增 #14 |
| 新 | README 的 Cline 配置说明（"VSCode settings.json"）同样不对 | 并入阶段 6 |
| 新 | BOM 还会破坏首行标题的识别与 chmod | #4 与 #11 共用一个 kb.md 读写 helper |
| 文档 | "MSRV 1.74" 无法离线验证；引入 `File::try_lock` 后最低版本实际为 1.89 | 阶段 6 初始按 1.89 声明；最终验收已补测 Windows 与 Linux Rust 1.89.0 |

## 2. 阶段总览

| 阶段 | 内容 | 依赖 |
|---|---|---|
| 0 | 测试隔离与基线 | 无 |
| 1 | 崩溃与解析：#1 #4(解析侧) #10 #5 | 0 |
| 2 | kb.md 写路径：共享读写 helper(#4 写侧、#11)、#2、#12 | 1 |
| 3 | 记忆系统：#3、consolidate 告警、#14 | 2 |
| 4 | 并发与中断：#9、#6 | 2 |
| 5 | 配置与 CLI 面：#7、#8、#13 | 0 |
| 6 | 文档对齐 | 1–5 |

每个阶段都写明：设计、改动点、测试、验收标准、风险。

## 3. 阶段 0：测试隔离与基线

设计：
- 三个集成测试文件（`integration_core_commands.rs`、`integration_memory_mcp.rs`、`cli_surface.rs`）里所有 `Command::cargo_bin("kb-cli")` 统一经过一个 helper：
  - 设置 `KB_GLOBAL_CONFIG=<该测试的临时目录>/global-config.json`
  - 设置 `KB_LLM_DISABLE_KEYRING=1`
  - 清掉 `KB_LLM_KEY`、`KB_LLM_MODEL`、`KB_LLM_BASE_URL`、`KB_LLM_ENABLE_*`、`KB_AGENT_ID`、`KB_SESSION_ID`
  - 个别用例需要的环境变量（如 `demo-key`）通过 helper 参数显式传入。
- 已有的 `run_json_with_global_config` 改为基于该 helper。
- 记录基线：改动前由你执行一次门禁，贴回各 crate 的测试数量与耗时，作为后续阶段的对照。

验收：
- 门禁全部通过，测试数与基线一致。
- `grep -c 'Command::cargo_bin'` 只命中 helper 内部。
- 门禁跑完后 `target/debug/config.json` 的修改时间不变，证明测试不再写可执行文件旁的全局配置。

风险：helper 清掉环境变量后，可能暴露出依赖开发者环境的用例；如有，逐个改为显式传参。

## 4. 阶段 1：崩溃与解析

### #1 陈旧 byte_range 导致 panic

设计：
- 把 kb-core 的 `fnv1a64` 设为 pub，kb-llm 删掉 `protocol.rs:375` 的重复实现，改为复用。
- `excerpt_for_chunk`（`agent_tools.rs:627`）的返回值从 `String` 改为 `(String, bool)`，bool 表示是否陈旧：
  - 区间越界、不在字符边界上，或切片的 `fnv1a64:{:016x}` 与 `chunk.content_signature` 不一致，都视为陈旧，返回索引时保存的 `chunk.preview`。
  - 若 `content_signature` 不是 `fnv1a64:` 前缀（例如 LLM 路径生成的签名），只做边界检查，不做签名校验。
- `AgentRawChunkItem` 增加 `stale: bool` 字段。MCP `verify_citation` 原样透出该字段，`stale=true` 时附带提示"run `kb update`"。
- `mcp_dispatch_tool_call` 外层用 `catch_unwind(AssertUnwindSafe(..))` 兜底，panic 转成 `isError: true` 的工具结果。
  - 前提：workspace 没有配置 `panic = "abort"`（已确认无 profile 配置）。
  - 默认的 panic hook 写 stderr，不会污染 stdout 上的 JSON-RPC 流。

测试：
- 单测（kb-llm）：中文文本配三种区间：错位到字符中间、越界、内容已变。断言不 panic、`stale=true`、返回 preview。另测内容未变时 `stale=false`。
- 单测（kb-cli mcp）：`mcp_guard` 包一个会 panic 的闭包，结果是 `isError: true`。
- 集成（memory_mcp）：复现审查脚本：中文文件 → update → 文件前插一个字符 → 依次发送 `verify_citation` 和 `ping`。断言两条请求都有响应，进程在 EOF 时退出码为 0。
- `ask --answer` 路径需要可用的 LLM，不做集成测试。它与 MCP 共用 `read_raw_chunk`，由上面的 kb-llm 单测覆盖根因。

验收：审查时的复现脚本不再输出 panic，`verify_citation` 返回 `stale: true`，同一进程内 `ping` 仍有响应。

### #4（解析侧）BOM

设计：`parse_kb_document` 入口去掉开头的 `\u{feff}`（不改变行号）。
- 需要逐个确认读 kb.md 的入口都走 `parse_kb_document`：`guards.rs`、`check.rs`、`env_registry.rs`、`refs.rs`、`update.rs`、`kbmd_ops.rs`。
- 写回侧的 BOM 保留放在阶段 2。

测试：
- 带 BOM 的 front matter（`default_perm: rw`），断言该值生效。
- 文件首行直接是 `# docs`（前面有 BOM），断言能识别为标题。

验收：审查复现脚本中 `list` 输出的 `perms` 为 `rw`。

### #10 chunk_id → record_id

设计：kb-core 新增 `pub fn record_id_of_chunk(chunk_id: &str) -> &str`：
- 用 `rsplit_once(':')` 切分；冒号后非空且全为 ASCII 数字时才去掉后缀，否则原样返回。
- `consolidation.rs:88` 和 `mcp/tools.rs:20` 改用它；实现时再全仓 grep `split(':')` 确认没有遗漏。

测试（表驱动）：
- `docs/a:0001` → `docs/a`
- `D:/notes/a.md:0002` → `D:/notes/a.md`
- `docs/a` → `docs/a`
- `a:b` → `a:b`
- `x:` → `x:`

### #5 NaN 置信度

设计：
- CLI 用 clap 的 `value_parser` 拒绝非有限值，报参数错误（退出码 2）。超出 [0, 1] 的有限值保持现有的 clamp 行为。
- 领域层 `Episode::with_confidence` 遇到非有限值时保留默认值，作为最后一道防线（MCP 侧 f64 转 f32 溢出会得到 inf）。

测试：
- CLI 执行 `--confidence NaN` 非 0 退出，episodes.jsonl 行数不变。
- 单测：`with_confidence(f32::NAN)` 和 `with_confidence(f32::INFINITY)` 都保留默认值。

阶段 1 验收：门禁全过；以上测试全部为新增并通过。

## 5. 阶段 2：kb.md 写路径

### 共享读写 helper（#4 写侧、#11）

设计：kbmd_ops 新增 `KbMdText`，所有程序化改写 kb.md 的函数都经过它：

```rust
struct KbMdText {
    bom: bool,              // 原文是否以 \u{feff} 开头
    eol: &'static str,      // "\r\n" 或 "\n"
    lines: Vec<String>,     // 不含 BOM、不含行尾
    trailing_eol: bool,     // 原文是否以行尾结束
}
impl KbMdText {
    fn read(path: &Path) -> io::Result<Self>;      // 文件不存在时返回空文档
    fn write_atomic(&self, path: &Path) -> io::Result<()>;
}
```

- 行尾判定：原文含 `\r\n` 时取第一处行尾的风格，否则用 `\n`。混合行尾统一为第一处的风格（这是唯一会改动非目标行的情况，在文档里说明）。
- 改用它的函数：`append_to_kb_md`、`apply_rm_to_kb_md`、`apply_mv_to_kb_md`、`apply_path_to_kb_md`、`apply_chmod_to_kb_md`、`prune_missing_refs_from_kb_md`、`dedupe_raw_refs_in_kb_md`。
- 行号约定：`KbReference.line` 从 1 开始，与 `lines` 下标一一对应。BOM 不占行，所以去掉 BOM 不影响行号。

测试（单测）：
- LF、CRLF、带 BOM、不以换行结尾的四种文件，读入后不做修改直接写回，字节完全一致。
- CRLF 文件执行一次 chmod，只有目标行变化，其余字节不变。

验收：在 CRLF 的 kb.md 上执行 `chmod`/`mv`/`path`/`rm`/`update`，`git diff` 只显示目标行。

### #2 追加位置

设计：
- `KbDocument` 增加 `headings: Vec<HeadingInfo { line, level, name }>`，在解析时一并收集，避免重复写一套标题识别逻辑。
- `append_to_kb_md(category)` 的插入点：
  1. 找到 `name == category` 的第一个标题（重名时取第一个，并在 kb-md-spec 中写明）。
  2. 从该标题行往下扫，遇到下一个任意级别的标题就停，插入点是停止位置之前最后一个非空行的下一行；区段为空时插在标题的下一行。
  3. 找不到该分类时，在文件末尾新建 `# {category}` 再追加（目前只有 raw 回填会走到这里，`add` 已经提前校验了分类存在）。
- 行为保持：kb.md 解析失败时沿用现在的"直接追加到末尾"，不在写路径里制造新错误。

测试：
- 单测（插入点）：目标分类在最后、在中间、带子标题、区段为空、区段尾部有空行、文件为空，共六种情况，断言插入行号。
- 集成：kb.md 为 `# raw` / `# docs:r` / `# notes:rw`：
  - `add ./a.md mydoc --category docs` 后，`list` 中 mydoc 的 perms 为 `r`，`list --tree` 的 categories 包含 `docs`。
  - 放入 `.kb/raw/new.md` 后执行 `update`，新条目的 category 为 `raw`。
  - `consolidate` 写出的产物同样落在 `raw` 下（与阶段 3 的测试合并）。

验收：审查时的复现脚本 kb.md 输出中，`[mydoc](./a.md)` 位于 `# docs:r` 下，`[](./.kb/raw/new.md)` 位于 `# raw` 下。

风险：已经被错放的历史条目不会被自动搬回原分类。CHANGELOG 写明该问题，并给出用 `kb check` 发现、手工调整的方法。

### #12 声明缺失时默认不删行

设计：
- `run_update` 中 `prune_missing_refs_from_kb_md` 只在 `prune == true` 时调用。
- regs.json 与索引的处理不变：not_found 条目仍从登记中移除，避免检索命中不存在的文件。
- 输出新增字段 `missing_declarations: [{alias, path}]`，text 模式追加一行提示：`N declared path(s) not found; run 'kb update --prune' to remove them from kb.md`。
- 该信息不放进 `warnings`/`progressive_issues`，所以不影响 `--strict` 的判定。

需要同步：
- `tests/fixtures/cli/update_success.json` 与 `command-examples.md` 的示例（doc-fixture 校验会比对两者）。
- `command-manual.md` 第 4.9 节。

测试：
- 声明一个不存在的路径，执行 `update`：kb.md 中该行仍在，`missing_declarations` 有 1 条，`list` 中没有该条目。
- 再执行 `update --prune`：该行被删除，`missing_declarations` 为空。
- `update --dry-run --prune`：kb.md 不变。
- `update --strict`（只有缺失声明，没有其他问题）：退出码为 0。
- 检查已有集成测试里依赖"不带 --prune 也删行"的用例，改为显式传 `--prune`，并在用例注释中说明原因。

验收：门禁全过；doc-fixture 校验通过；command-manual 与实现一致。

阶段 2 验收：门禁全过；审查时 #2、#4、#11 的复现结果都已改变；没有一个已有用例被删除，只允许因行为变更而修改，并在用例中注明。

## 6. 阶段 3：记忆系统

### #3 consolidate 文件名碰撞

现状：`file_stem` 只保留 ASCII 字母数字，`docs/a` 与 `docs-a` 都得到 `docs-a`，中文路径更容易碰撞；后写入的产物覆盖先写入的。

设计：
- 产物文件名的确定顺序：
  1. 复用：扫描 `.kb/raw/agent-memory/*.md`，解析每个文件头部 `<!-- kb-agent-memory ... -->` 里的 `topic`。已有文件对应当前 topic 时，继续写同一个文件。
  2. 新建：`{slug}-{hash8}.md`。`slug` 沿用现在的清洗规则（为空时用 `topic`），`hash8` 是 `fnv1a64(topic)` 的前 8 位十六进制。
  3. 冲突检查：新名字已存在、而文件头里是另一个 topic（哈希碰撞，概率极低）时，改用完整 16 位哈希；仍冲突就报错，不覆盖。
- 纯函数放在 kb-core：`memory_file_name(topic, existing: &BTreeMap<String, String>) -> Result<String, ..>`。文件扫描（IO）放在 kb-cli 的 `memory.rs`。
- 头部无法解析的文件不参与复用，也不会被覆盖：它的文件名会被视为已占用。
- 产物改用 `kb_storage_fs::atomic_write_bytes` 写入（现在是 `fs::write`）。

兼容性：
- 没碰撞过的老用户，文件名和声明都不变。
- 碰撞过的用户：episodes.jsonl 才是真值，consolidate 每次都从全部 episode 重新生成。修复后第一次 consolidate 会把被覆盖掉的 topic 写成新文件，丢失的内容自动恢复。
- 碰撞时被覆盖的那个旧文件：复用以文件头部记录的 topic 为准，它会按该 topic 重新生成，不会残留另一个 topic 的内容。

### consolidate 吞错（`memory.rs:173`）

设计：`run_add` 返回的错误按错误码分类：
- `E2003`（alias 冲突）与 `E2006`（路径已登记）视为"已声明"，计入 `already_declared`。
- `run_add` 返回 `ignored: true`（被 .kbignore 忽略）以及其他所有错误，记入新字段 `warnings: [{file, code, message}]`。
- 实现时先核对 errors.rs 中这两个错误码的实际取值。

测试（集成）：
- 碰撞：对 `docs/a:0001` 和 `docs-a:0001` 各写 2 条 episode，执行 consolidate。断言产生 2 个文件，各自只包含自己的事实。
- 中文：对 `./设计.md:0001` 和 `./需求.md:0001` 各写 2 条，断言产生 2 个文件。
- 幂等：连续执行两次 consolidate，文件集合不变，kb.md 中没有重复声明，第二次的 `already_declared` 等于文件数。
- 复用：预先放一个老格式文件 `docs-a.md`，头部 topic 为 `docs/a`，consolidate 后仍写入这个文件，没有生成 `docs-a-<hash>.md`。
- 告警：.kbignore 加入 `.kb/raw/agent-memory/`，consolidate 输出的 `warnings` 不为空。
- 单测：`memory_file_name` 的复用、新建、哈希碰撞三个分支。

验收：审查时的复现脚本输出 2 个文件，`fact B1` 和 `fact B2` 不再丢失。

### #14 episodes.jsonl 半行拼接（新增）

现状：崩溃可能留下一行没有换行符的半截 JSON，下一次 `append` 直接写在它后面，两条合成一行；读取时整行解析失败被跳过，新 episode 也随之丢失。

设计：`FsEpisodeStore::append` 打开文件后，若文件非空且最后一个字节不是 `\n`，先写一个 `\n` 再写新行。只需读最后 1 个字节，不必读整个文件。

测试（单测，`episode_files.rs`）：手工写入 `{"id":"half` 且不加换行，再 append 一条正常 episode。断言 `query` 能读到这条新 episode。

验收：门禁全过；sqlite 后端不受影响（它不走这段代码）。

阶段 3 验收：门禁全过；`integration_memory_mcp.rs` 的已有用例全部保持通过。

## 7. 阶段 4：并发与中断

### #9 单写者锁

设计：
- 锁实现：`SingleWriterLockManager::acquire` 改为打开（不存在就创建）`.kb/writer.lock`，调用 `File::try_lock()`（Rust 1.89 起稳定，当前工具链 1.94）。
  - 成功后写入 `pid=<pid> started=<unix 秒>`，只用于诊断提示。
  - 释放：`File` 被 drop 时自动释放；进程崩溃、被杀或 Ctrl+C 退出时，操作系统也会释放。
  - 锁文件本身不删除，避免"删除后又被别人创建"的竞争。也因此不存在陈旧锁问题。
  - `try_lock` 返回 `WouldBlock` 表示被占用；返回 `Unsupported`（部分网络文件系统）时不加锁继续执行，并在 `-v` 模式下提示。
- 加锁范围（只在命令入口加一次，内部调用不再加锁）：

| 加锁 | 不加锁 |
|---|---|
| `init`（`.kb` 创建之后）、`add`、`mv`、`rm`、`path`、`chmod` | `list`、`check`、`ask`、`env`、`completion` |
| `update`（`--dry-run` 除外） | `remember`、`correct`（只追加 episode，#14 已保证追加安全） |
| `consolidate`（`--dry-run` 除外） | `serve` 进程本身 |
| MCP 的 `consolidate` 工具，每次调用时加锁，调用结束释放 | MCP 的其他工具 |

- 重入：`run_consolidate` 内部会调用 `run_add`。把实际逻辑拆成不加锁的 `*_unlocked` 函数，加锁只发生在 `dispatch` 和 MCP 派发这两个入口，避免同一进程对同一文件二次加锁。
- 失败时报新错误码 `E3004`（退出码 3）：`Another kb process is writing this knowledge base`，并带上锁文件中的 pid。立即失败，不等待。实现时先确认 `E3004` 未被占用。

测试：
- 单测（`lock_fs.rs`）：同一进程内先取得一把锁，再用另一个 `File` 句柄 `try_lock` 会失败；drop 之后可以重新取得。
- 集成：测试进程自己打开 `.kb/writer.lock` 并持有锁，期间执行 `kb add`，断言退出码 3、错误码 E3004、kb.md 未变；释放后再执行，成功。
- 集成：先手工写一个锁文件内容但不持有锁（模拟崩溃残留），`kb add` 能成功。
- 集成：持锁期间 `kb list`、`kb ask`、`kb remember` 不受影响。

### #6 Ctrl+C

设计：
- `ctrlc` 回调在 `Cli::parse()` 之后安装，从而能知道输出格式。回调里直接输出 E9001 并以 `process::exit(130)` 退出。
  - JSON 模式：错误信息写到 stderr。stdout 只用于成功结果和 `serve` 的 JSON-RPC 流，不能被打断写入。
  - 删除 `main.rs:84` 处事后检查中断标记的代码（改完之后不会再被执行到）。
- 安全性依据：kb.md 与 `.kb/*.json` 都是原子写；锁由操作系统释放；episodes 的半行由 #14 处理。被打断的 `update` 可能让 L2–L5 处在新旧混合的状态，下一次 `update` 会按增量逻辑重新收敛，这一点写进文档。
- 原子写被打断时可能残留 `.<name>.<nonce>.tmp` 临时文件。本次只在文档中说明可以安全删除，不做自动清理。

测试：
- 集成（仅 unix，`#[cfg(unix)]`）：启动 `kb serve --mcp`，向它发 SIGINT，断言 5 秒内以 130 退出；沿用现有测试中 60 秒 watchdog 的写法。CI 的 ubuntu 任务会执行它。
- 测试通过 `kill -INT <pid>` 发送信号（Rust 标准库不能直接发信号，不为此新增依赖）。
- Windows：没有可靠的自动化方式，列入第 11 节的手工验收清单。

阶段 4 验收：门禁全过；手工清单中 Ctrl+C 的两项通过。

## 8. 阶段 5：配置与 CLI 面

### #7 全局配置路径

现状：`executable_dir_config_path` 总是返回 `Some`，`~/.kb-cli/config.json` 永远不会被选中，与文档不符。

设计：
- 选择顺序：
  1. `KB_GLOBAL_CONFIG`（非空时）
  2. 可执行文件旁的 `config.json`（只在文件已存在时）
  3. `~/.kb-cli/config.json`
- 抽成纯函数 `select_global_config_path(env_override, exe_candidate, exe_candidate_exists, home) -> PathBuf`，单测只测它，不在测试里修改进程环境变量（并行测试修改环境变量容易互相干扰）。
- `runtime_config.rs:54` 目前直接读 `KB_GLOBAL_CONFIG`，改为调用同一个解析函数，保证两处选到的是同一个文件。
- `kb env --doctor` 输出实际选中的路径以及它来自第几条规则。

兼容性：
- 已经执行过 `init` 的用户，可执行文件旁已有 config.json，仍走第 2 条，行为不变。
- 全新的 `cargo install` 用户会改用家目录，与文档一致。
- 安装包是否把 config.json 放在可执行文件旁，实施时要先看 `packaging/` 下的安装脚本确认。

测试：
- 纯函数的四种组合：有 env、仅有 exe 旁文件、两者都没有、env 为空字符串。
- 集成：设置 `KB_GLOBAL_CONFIG` 后执行 `kb env --doctor`，输出该路径（依赖阶段 0 的隔离）。

### #8 移除 `--config`

改动：
- `cli.rs` 删除该字段，`main.rs:55` 删除存在性检查。
- `CoreErrorKind::ConfigNotFound` 没有其他调用方，一并删除。`E2008` 在错误目录中标为"已废弃"，编号不复用。
- 同步：help 快照 `cli_surface__kb_root_help_full.snap`、`cli_surface.rs:40` 的断言、`integration_core_commands.rs:1990` 的用例、`command-manual.md:20`、`error-catalog.md:83`。

测试：
- 原来的 1990 行用例改为：`kb --config x init` 被 clap 拒绝，退出码 2。
- help 快照不再包含 `--config`。

兼容性：在脚本里传了 `--config` 的用户会直接报错。CHANGELOG 中列为破坏性变更。

### #13 记忆命令的错误码

设计：新增 `StorageUnavailable`（编号实施时在 E1xxx 段内选一个未占用的，退出码 1）。用于 `remember`、`correct`、`consolidate` 和 MCP 工具中存储打开、读取、写入失败的情况，不再报 `E3003`（kb.md 格式错误）。`E3003` 只保留给真正的 kb.md 解析失败。

测试：把 `.kb/episodes.jsonl` 换成一个同名目录，执行 `kb remember x`，断言返回新错误码、退出码 1。

阶段 5 验收：门禁全过；help 快照已更新并经人工确认；`kb env --doctor` 显示的路径与规则一致。

## 9. 阶段 6：文档对齐

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | `[workspace.package]` 加 `rust-version = "1.89"`，各 crate 继承 |
| `README.md` | Rust 徽章改为 1.89+；删除 production-ready 徽章；`your-repo` 占位链接替换为实际仓库地址（实施时读取项目内的 `.git/config`，取不到就改成说明文字） |
| `README.md` | Cursor：项目级 `.cursor/mcp.json` 或全局 `~/.cursor/mcp.json`；Cline：通过扩展的 MCP Servers 设置编辑 `mcpServers`，默认全局文件为 `~/.cline/data/settings/cline_mcp_settings.json`；附官方文档链接 |
| `docs/README.md` | 删除指向被 gitignore 的 `docs/superpowers/` 的链接 |
| `cli.rs:147` | 注释路径改为 `.kb/raw/agent-memory/` |
| `command-manual.md` | 4.2 节：add 插入位置；4.9 节：`--prune` 行为与 `missing_declarations`；新增"并发写"小节（E3004）；全局选项表删除 `--config`；配置路径改为三条规则 |
| `error-catalog.md` | 新增 E3004、存储错误码；E2008 标为已废弃 |
| `kb-md-spec.md` | BOM 与 CRLF 的处理；分类重名时取第一个 |
| `memory-model.md`、`mcp-integration-guide.md` | 产物命名规则；`verify_citation` 的 `stale` 字段 |
| `CHANGELOG.md` | `[Unreleased]` 分为"修复 / 变更（含破坏性）/ 新增"三部分，覆盖本计划所有用户可见的改动 |

说明：当前 `Cargo.lock` 下，workspace 已在 Windows x86_64 MSVC 与 WSL Ubuntu x86_64 GNU 的 Rust 1.89.0 上通过 Clippy 与测试。

验收：doc-fixture 校验通过；对 `docs/` 做一遍链接检查，没有指向不存在文件的链接；README 中出现的每条命令和参数都能在 `cli.rs` 中找到。

## 10. 验收矩阵

| 问题 | 自动化测试 | 复现脚本结果（修复前 → 修复后） |
|---|---|---|
| #1 | kb-llm 单测、mcp 单测、memory_mcp 集成 | panic、exit 101 → `stale: true`，`ping` 有响应 |
| #2 | 插入点单测、core 集成 | mydoc 在 `notes:rw` 下 → 在 `docs:r` 下，perms 为 `r` |
| #3 | kb-core 单测、memory_mcp 集成 | 1 个文件、丢失 B 组事实 → 2 个文件 |
| #4 | 解析单测、读写 helper 单测 | perms `r` → `rw` |
| #5 | CLI 集成、领域单测 | 写入 `null` 后被静默跳过 → 参数错误，不写入 |
| #6 | unix 集成 + 手工 | 无法中断 → 退出码 130 |
| #7 | 纯函数单测 | 家目录配置永远不会被读取 → 按三条规则选择 |
| #8 | CLI 集成、help 快照 | 参数存在但不生效 → 参数被拒绝 |
| #9 | lock 单测、core 集成 | 无锁 → E3004 |
| #10 | 表驱动单测 | `D` → `D:/notes/a.md` |
| #11 | 读写 helper 单测 | 整个文件变成 LF → 只改目标行 |
| #12 | core 集成 | 默认删行 → 只有 `--prune` 才删 |
| #13 | CLI 集成 | E3003 → 存储错误码 |
| #14 | episode_files 单测 | 新 episode 丢失 → 正常读出 |

## 11. 最终验收记录（2026-11-13）

执行人：Codex。Windows 验收使用 x86_64 MSVC 目标；Linux 验收使用 WSL Ubuntu x86_64。

| 检查 | 结果 | 验证方式与边界 |
|---|---|---|
| MCP server 中断 | 通过 | 独立辅助进程向隐藏测试控制台发送 `CTRL_C_EVENT`，退出码 130、stderr 含 `E9001`，MCP stdout 只有 ping 响应；`CTRL_BREAK_EVENT` 也已通过。 |
| `update --llm` 中断与恢复 | 通过 | 本机 mock HTTP 服务收到 LLM 请求后发送 `CTRL_C_EVENT`；退出码 130、stderr 含 `E9001`，之后规则模式 `kb update` 成功。未调用外部模型。 |
| writer lock 冲突与释放 | 通过 | 第一个 `update --llm` 挂起时，第二个 `update` 返回退出码 3 和 `E3004`；中断第一个进程后再次 update 成功。 |
| BOM 和权限解析 | 通过 | 隔离 kb.md 写入 UTF-8 BOM 与 `default_perm: rw`，`update` 后 `list` 返回 `rw`。 |
| CRLF `chmod` | 通过 | 字节级比较显示仅声明行变化，其他行保持 CRLF。 |
| MCP client | 通过 | Claude Code CLI 2.1.318 使用独立 `CLAUDE_CONFIG_DIR` 显示 Connected；通信记录确认客户端完成 initialize 和 tools/list 并收到全部 8 个工具，无需模型调用。 |
| Unix SIGINT | 通过 | WSL Ubuntu 的 Rust 1.89.0 全量测试包含此用例；Rust 1.94.1 下也已单独通过。 |
| Rust 1.89 工具链 | 通过（Windows / Linux） | Rust 1.89.0 下两平台 workspace Clippy 无警告；Windows 362 项、Linux 361 项测试通过，数量差异来自平台专属用例。 |

最低版本验收命令（使用当前锁定依赖）：

```bash
cargo +1.89.0 clippy --locked --workspace --all-targets -- -D warnings
cargo +1.89.0 test --locked --workspace
node tests/tools/validate-doc-fixture-diff.js
```

收尾中发现 Unix SIGINT 测试的 `writeln!` 触发 Linux Clippy `write_literal`；先复现门禁失败，再改用 `write_all` 写入相同 JSON 请求和 LF，两平台门禁通过。业务代码和依赖未变更。

## 12. 每阶段的交接流程

后续执行要求：

1. 新增或修改测试。
2. 先确认测试可以复现问题。
3. 做最小实现。
4. 运行 workspace clippy、test 和 doc-fixture。
5. 进行代码评审并处理发现项。
6. 每阶段单独 commit。

阶段 0–6 已按此流程完成并分别提交，提交编号见本文顶部的阶段记录。

## 13. 风险与回滚

- 每个阶段只在门禁全部通过后才进入下一阶段。某阶段无法通过时，只回退该阶段涉及的文件（工作区未提交，由你用 git 决定），前面已通过的阶段保留。
- 行为变更（#8、#12，以及 #7 对新用户的影响）在 CHANGELOG 中集中列出，便于发布时说明。
- 阶段 4 依赖 `File::try_lock`；如果在某个目标平台上不可用，退回到"锁文件 + 心跳更新时间"的方案，届时再单独评审。

## 附录 A：复现脚本（由你在 Git Bash 中执行）

在仓库根目录执行，需要先运行 `cargo build`。每次使用一个新的目录名，脚本不做删除。

```bash
BIN="$PWD/target/debug/kb-cli.exe"
export KB_LLM_DISABLE_KEYRING=1
R="$PWD/target/repro-$(date +%s)"; mkdir -p "$R"; export KB_GLOBAL_CONFIG="$R/global.json"

# #2 追加位置：期望 mydoc 在 "# docs:r" 下、perms 为 r；new.md 在 "# raw" 下
mkdir "$R/r2" && cd "$R/r2" && "$BIN" init >/dev/null
printf '\n# docs:r\n\n# notes:rw\n' >> kb.md; echo a > a.md
"$BIN" add ./a.md mydoc --category docs >/dev/null; echo raw > .kb/raw/new.md
"$BIN" update >/dev/null; cat kb.md; "$BIN" --output json list

# #3 文件名碰撞：期望 2 个文件，A、B 两组事实都在
mkdir "$R/r3" && cd "$R/r3" && "$BIN" init >/dev/null
for i in 1 2; do "$BIN" remember "fact A$i" --ref docs/a:0001 >/dev/null
                 "$BIN" remember "fact B$i" --ref docs-a:0001 >/dev/null; done
"$BIN" consolidate >/dev/null; ls .kb/raw/agent-memory/; grep -ho 'fact [AB][12]' .kb/raw/agent-memory/*.md

# #4 BOM：期望 perms 为 rw
mkdir "$R/r4" && cd "$R/r4" && "$BIN" init >/dev/null; echo x > x.md
printf '\xef\xbb\xbf---\ndefault_perm: rw\n---\n# docs\n[xdoc](./x.md)\n' > kb.md
"$BIN" update >/dev/null; "$BIN" --output json list | grep perms

# #1 陈旧区间：期望返回 stale: true、两条响应，退出码 0
mkdir "$R/r1" && cd "$R/r1" && "$BIN" init >/dev/null
printf '第一段中文内容。\n\n第二段中文内容，用来测试。\n' > zh.md
"$BIN" add ./zh.md zh >/dev/null; "$BIN" update >/dev/null
printf 'X第一段中文内容。\n\n第二段中文内容，用来测试。\n' > zh.md
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"verify_citation","arguments":{"chunk_id":"zh:0002","quote":"第二段"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"ping"}' | "$BIN" serve --mcp; echo "exit=$?"
```

## 14. 实施前待核对项及结果

- `errors.rs`：已核对 E2003、E2006、E3004、E1004 的映射和退出码。
- 打包脚本：已核对全局配置安装路径与可执行文件目录回退规则。
- LLM chunk signature：已核对并覆盖带 `fnv1a64:` 与其他签名的读取路径。
- kb.md 解析入口：已核对 BOM 由共享 parser/helper 处理。
- `--prune` 回归：已核对相关集成用例显式传入 prune。
- Cursor 与 Cline MCP 配置：见本文顶部核验来源和阶段 6 文档。
