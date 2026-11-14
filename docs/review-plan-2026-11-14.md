# 整体评审后的最小修复实施方案（2026-11-14）

> **For agentic workers:** 使用 `superpowers:executing-plans` 逐阶段执行；使用 `superpowers:requesting-code-review` 进行独立评审。步骤用复选框记录。

**Goal:** 修复本次评审确认的数据保护、配置合并和安装发布问题，优先替换错误兜底和薄弱测试，控制新增代码。

**Architecture:** 沿用当前 CLI → core ports → FS/SQLite stores 的结构，在现有调用边界修正行为。复用现有错误码、原子写、配置层合并器和测试工具，不增加依赖、通用恢复框架或新的存储抽象。

**Tech Stack:** Rust 1.89+、Cargo workspace、serde_json、rusqlite、Node 内置测试、PowerShell、Bash、GitHub Actions。

**状态：v2 的 7 个正式阶段已完成、验证、评审并逐阶段提交。** 基线为 `78484034862da89bf769399948455b3fe4d54b6c`。第 10 节的可选小项不在本轮范围内。评审复现位于 `target/review-2026-11-14/`；本轮门禁日志位于 `target/review-implementation-2026-11-14/`，两者均被忽略。

| 已完成阶段 | 复现与最小实现 | 验证与评审 | 提交 |
|---|---|---|---|
| 1 全局配置保护 | 三个回归先失败；写前严格读/容器校验，注册表和 LLM loader 接受 BOM | Windows 349 / Linux Rust 1.89 348 项测试、两平台 Clippy、Node 回归及 20 组 doc-fixture 通过；独立评审无重要问题 | `0488e0d` |
| 2 copy 防覆盖 | 未登记目标碰撞先失败；源权限/锁定源保护先通过；create_new 复制并保留权限 | Windows 350 / Linux 348 项测试及两平台门禁通过；独立评审无重要问题 | `a4c4470` |
| 3 JSONL 恢复/排序 | 两个既有测试扩展后先失败；字节分行解析，按时间和 id 排序 | 两平台门禁通过，测试数 350/348；评审建议的同时间 id 逆序用例已增强并通过 | `221fcb4` |
| 4 非法存储配置 | resolver、SQLite 分叉及 ask 用例先失败；非法配置停止，ask/MCP 传播 E1004 | Windows 352 / Linux 350 项测试及门禁通过；独立评审无重要发现，补测合法配置下 summary/neighbors 正常缺失 | `25701bd` |
| 5 runtime 合并 | global/环境用例先失败；逐字段 Option 合并，显式 false/0 保留 | Windows 354 / Linux 352 项测试及门禁通过；评审指出的单测环境依赖改为纯配置层测试 | `19ede83` |
| 6 安装/验收隔离 | rc fixture 先失败，旧 smoke 在污染环境下实际请求本地模型；修复转义、原子替换及环境隔离 | 两平台门禁 354/352 通过；当前 release 归档完成 CLI/MCP/卸载及零模型请求验收；Linux 四类 rc、写失败用例通过，评审建议已处理 | `5c7141a` |
| 7 Release 公开时序 | 真实 workflow run 片段的本地 gh stub 先检出提前公开；版本预检、draft 和双平台成功依赖 | 门禁 354/352 通过；Linux Node 4 项，Windows 3 项+1 Linux 专属跳过；两平台当前包验收、YAML 解析、Windows 环境恢复通过，评审的顺序断言已增强 | 本提交 `ci: publish releases after both packages pass acceptance` |

## 1. 评审结论与边界

首轮评审执行的基线：Windows workspace Clippy 无警告、346 项 Rust 测试通过；3 项 Node 回归测试通过；20 组文档与 fixture 匹配。二次复核重新构建当前 CLI、逐项重现问题，并检查候选修法的回归风险，没有把重复跑全量测试当作问题成立的证据。

检查范围包括 CLI/core、FS/SQLite 存储、LLM/检索、安装打包、CI 和测试。下表前八项均有隔离复现，其中 R8 的联网行为在 Linux 实测，Windows 同类路径由源码确认；R9 由真实工作流 DAG 和本地 gh stub 执行其 run 片段确认，未实际调用 GitHub 发布接口。

| 编号 | 优先级 | 已确认问题及影响 | 证据位置，均相对基线 | 实施阶段 |
|---|---|---|---|---|
| R1 | P1 | `init` 将损坏或带 BOM 的全局配置当空对象写回；合法根对象中的不支持字段形状也会被重置，丢失原字段且仍报告注册成功 | `crates/kb-cli/src/env_registry.rs:85`、`:174`、`:190`、`:298` | 1 |
| R2 | P1 | copy 模式只检查声明，不检查真实目标，覆盖尚未登记的 `.kb/raw/` 文件 | `crates/kb-cli/src/commands/runtime/core/handlers/refs.rs:93` | 2 |
| R3 | P1 | episode 日志含截断的 UTF-8 字节时，整文件解码失败，全部有效历史不可读 | `crates/kb-storage-fs/src/episode_files.rs:46` | 3 |
| R4 | P1 | SQLite 项目配置损坏后静默改写 FS；修好配置后，新记忆从 SQLite 查询中消失，数据分散在两处 | `crates/kb-cli/src/storage_backend.rs:45` | 4 |
| R5 | P2 | flags 整组始终存在，环境/项目/全局 runtime 配置不生效；显式 false/0、缺失字段也被错误合并 | `crates/kb-cli/src/runtime_config.rs:42`、`:56`、`:77` | 5 |
| R6 | P2 | FS episode 查询把追加顺序当时间顺序，乱序输入下返回错误的“最近 N 条” | `crates/kb-storage-fs/src/episode_files.rs:136`；契约在 `crates/kb-core/src/ports.rs:46` | 3 |
| R7 | P2 | Linux 卸载替换 rc 符号链接并改权限；自定义安装目录中的 `$` 等字符被 shell 展开成另一条路径 | `packaging/linux/uninstall.sh:34`；`packaging/linux/install.sh:33` | 6 |
| R8 | P2 | 发布验收继承 `KB_LLM_*`，会受开发者配置影响而调用模型；Linux 验收预先添加 PATH，绕过真实安装 PATH 流程 | `tests/scripts/test-linux-installer.sh:31`；`tests/scripts/test-windows-installer.ps1:163` | 6 |
| R9 | P2 | 构建和验收开始前已公开 Release，后续失败可能留下空或只有一平台产物的发布 | `.github/workflows/release.yml:32` | 7 |

P1 优先完成，再处理 P2。阶段 1–5 修改业务行为，阶段 6–7 修正安装和发布验收。每阶段单独 commit，不要求合成一次大改。

### 二次复核对方案本身的修正

| 原方案的不足 | 复核依据 | v2 处理 |
|---|---|---|
| 全局配置只检查根对象，仍可能重置内部 `llm` / `knowledge_bases` | 合法 JSON 中的数组 llm、字符串登记在 init 后被丢弃，已复现 | 只在注册写入口校验将被改写的容器字段；不建立完整配置 schema |
| 把 `env --list` 改成失败退出作为 R1 必需步骤 | list 不写文件，改变读侧策略不是避免覆盖所必需 | 保留现有读侧容错和输出契约，注册写入严格处理错误 |
| `create_new + io::copy` 示例遗漏源打开顺序和权限 | Windows 源被锁时先建目标会残留空文件；流式复制不会自动保留 fs::copy 的只读权限，已实测 | 源先打开，保留源权限，失败关闭句柄后仅清理本次创建的目标 |
| 阶段 4 把所有错误描述成已有 E1004，并要求 builder 改 Result | ask/check/update 现有映射不同；builder 三条入口此前均已 open/read_index | 写明每条命令的错误契约，删除无必要的 builder 返回类型改造 |
| 为保留 rc inode 改为原地 cat 写回 | 写入失败实验将原 6400 字节 rc 截到 1024 字节 | 删除保留 inode 的要求，解析 symlink 目标后在其同目录原子替换，保留链接和权限 |
| 默认安装的测试前置条件不完整，阶段 7 顺带要求预发布标记 | 继承的安装变量或预置 PATH 仍会跳过默认流程；预发布标记与提前公开无关 | 明确清理安装变量和起始 PATH；阶段 7 只修正公开时序 |

v2 中的 rc 片段已直接从本文提取，在 Linux 上验证普通文件、symlink、无匹配、过滤后为空、写入失败五个场景；这只验证候选实现，不表示安装脚本已经修复。copy 的源打开/权限差异也用标准库小程序实测。其余代码变更仍需按各阶段的失败测试、最小实现、门禁和评审执行。

### 不做的扩张

- 不因文件长或存在多个 `unwrap_or` 就拆模块、批量删除分支。
- 保留 MCP panic 隔离、陈旧 chunk 检查、writer lock、原子写失败清理和坏 JSONL 行跳过，这些都有明确故障路径。
- 不新增通用配置系统、自动修复损坏文件、后台重试或数据迁移框架。
- 不把 `target/` 下的本次一次性复现脚本全部转成永久测试。优先扩展对应的既有用例。
- 当前方案不发布、不推送、不打 tag，也不改版本号。正式发布前另行确定版本，现有 `v0.1.4` 已存在。

## 2. 每阶段统一流程

- [x] 新增或修改有行为意义的测试；只断言输出、错误和持久化副作用。
- [x] 在当前实现上运行目标测试，确认失败原因正是待修问题，并记录结果。
- [x] 做该阶段的最小实现；通过目标测试。
- [x] 执行下面的 workspace 与文档门禁；阶段 6–7 另执行安装/工作流检查。
- [x] 独立代码评审，确认错误映射、测试有效性以及新增代码的必要性；处理发现项。
- [x] 只暂存本阶段文件，`git diff --cached --check` 后单独 commit。

每阶段至少有一条问题用例在修复前失败；用于保护现有正确行为的用例应先通过，例如 copy 权限保持。不能为了取得红灯而改错这类用例的预期。

```text
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node --test tests/tools/packaging.test.js tests/tools/validate-doc-fixture-diff.test.js tests/tools/release-workflow.test.js
node tests/tools/validate-doc-fixture-diff.js
git diff --check
```

涉及 Rust 的阶段在 Windows 和 Linux 上执行门禁；Linux 脚本必须在 Linux 上验收。可沿用已有 WSL Rust 1.89 工具链与缓存，但脚本和文档不得依赖本机绝对路径。

集成测试沿用 `crates/kb-cli/tests/common/mod.rs` 隔离配置和 keyring；阶段 5 生效前，需在该 helper 的隔离变量中补齐 `KB_QUIET`、`KB_VERBOSE`、`KB_NO_COLOR`，避免真实环境污染。运行模型相关故障测试只用本地 HTTP stub，不使用真实 key。

## 3. 阶段 1：全局配置读失败时禁止覆盖（R1）

**修改文件：** `crates/kb-cli/src/env_registry.rs`、`crates/kb-cli/tests/integration_core_commands.rs`、`crates/kb-llm/src/config.rs`、`CHANGELOG.md`。不改变 env handler 的返回契约。

### 先改测试

- [x] 用现有 `run_json_with_global_config` 增加损坏 JSON 用例：`init` 的本地初始化成功，但 `global_config_registered=false` 且有 `global_config_error`，原配置字节不变。
- [x] 同一用例以表驱动加入“有效 JSON 但根节点为数组/null”，确认也不重置文件。
- [x] 加入合法根对象中 `llm` 为数组/null、`knowledge_bases` 为字符串或含非字符串成员的场景，确认注册失败且原字节不变。字段缺失继续允许补默认值。
- [x] 有效 BOM 配置保留既有 KB、模型、测试 key 和未知字段；注册新 KB 后仍可读取全部字段。不存在的配置继续正常新建。

核心断言示例，放入现有集成测试文件即可：

```rust
let dir = unique_temp_dir("init-invalid-global");
let config = dir.join("global-config.json");
let before = br#"{"llm":{"key":"test-only"},broken"#;
fs::write(&config, before).unwrap();
let (ok, body) = run_json_with_global_config(&["init"], &dir, &config);
assert!(ok);
assert_eq!(body["data"]["global_config_registered"], false);
assert!(body["data"]["global_config_error"].is_string());
assert_eq!(fs::read(&config).unwrap(), before);
```

运行 `cargo test -p kb-cli --test integration_core_commands init_invalid_global`，修复前应在注册状态或字节不变断言失败；测试函数按该前缀命名。

### 最小实现

- [x] 将 `read_global_config(&Path)` 改成 `Result<Value, String>`：只有 `ErrorKind::NotFound` 返回空对象；读失败、JSON 错误和非对象返回错误。
- [x] JSON 读取前只剥除开头的 UTF-8 BOM；不做 lossy 解码，不尝试修复残缺内容。
- [x] `register_kb_root` 用 `?` 接收结果。保留 `init.rs` 已有的非致命注册失败字段，不新增错误类型。
- [x] 在注册写入口、调用 `ensure_global_config_shape` 之前检查两个实际要改写的容器：若 `llm` 已存在则必须为对象；若 `knowledge_bases` 已存在则必须为字符串数组。否则返回注册错误，不重置字段。未知字段不校验、不删除；不自动迁移不支持的形状。
- [x] `list_existing_kb_roots` 保持返回 `EnvRegistryList`；读取处显式使用 `read_global_config(...).unwrap_or_else(|_| json!({}))` 延续其只读容错。严格读取结果只在注册写入边界阻止写回；不扩展 E1004 到全局列表。
- [x] `kb-llm` 现有 JSON loader 同样接受开头 BOM，补对应 loader 测试；不要复制一套配置恢复逻辑。阶段 5 再对齐 runtime loader。

读取函数的关键结构：

```rust
let raw = match fs::read_to_string(path) {
    Ok(raw) => raw,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
    Err(err) => return Err(format!("read global config '{}': {err}", path.display())),
};
let value: Value = serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(&raw))
    .map_err(|err| format!("parse global config '{}': {err}", path.display()))?;
if !value.is_object() {
    return Err(format!("global config must be an object: '{}'", path.display()));
}
Ok(value)
```

上面的严格读取只校验 JSON 根对象；两个容器的写前校验放在 `register_kb_root` 中，不能遗漏。字符串形式的旧登记由 Windows 安装器已有规则归一化；本阶段 Rust 注册器遇到它只报告失败并保留原文件，不另建迁移逻辑。

- [x] 运行统一门禁、评审，提交 `fix: preserve unreadable global configuration`。

## 4. 阶段 2：copy 模式保护未登记的真实目标（R2）

**修改文件：** `crates/kb-cli/src/commands/runtime/core/handlers/refs.rs`、`crates/kb-cli/tests/integration_core_commands.rs`、`CHANGELOG.md`。

### 先改测试

- [x] 扩展 `add_copy_conflicting_raw_target_returns_e2006`，将“已登记”和“手工放入 raw 但未登记”作为两个 fixture 场景。
- [x] 两种情况都断言退出码 2 / `E2006`，原 raw 字节、`kb.md` 和 `.kb/refs.json` 均不变。
- [x] 保留已有正常 copy 成功路径；不要再写一套只检查 help 或内部布尔值的测试。
- [x] 在现有 copy 成功用例中补源/目标权限相等的断言。Windows 另用标准库 `OpenOptionsExt::share_mode(0)` 持有源文件，断言源打开失败时没有新目标或声明；该行为在当前 fs::copy 实现下应先通过。

```rust
mk_file(&dir, "./.kb/raw/note.md", "original");
mk_file(&dir, "./source/note.md", "replacement");
let kb_before = fs::read(dir.join("kb.md")).unwrap();
let refs_before = fs::read(dir.join(".kb/refs.json")).unwrap();
let (ok, body) = run_json(
    &["add", "source/note.md", "copied", "--mode", "copy"], &dir,
);
assert!(!ok);
assert_eq!(body["error"]["code"], "E2006");
assert_eq!(fs::read(dir.join(".kb/raw/note.md")).unwrap(), b"original");
assert_eq!(fs::read(dir.join("kb.md")).unwrap(), kb_before);
assert_eq!(fs::read(dir.join(".kb/refs.json")).unwrap(), refs_before);
```

运行 `cargo test -p kb-cli --test integration_core_commands add_copy_conflicting_raw_target`，修复前未登记场景应失败。

### 最小实现

- [x] 保留声明冲突检查，用 `OpenOptions::create_new(true)` 创建复制目标；`AlreadyExists` 映射既有 `PathAlreadyRegistered/E2006`。
- [x] 先打开源文件并从已打开句柄读取权限，成功后才创建目标。创建成功后用 `std::io::copy`，再设置源权限，保留原 fs::copy 语义。
- [x] 复制或权限设置失败时先关闭目标句柄，再只清理本次刚创建的目标并返回已有写入错误；不删除 AlreadyExists 对应的旧文件，不引入自动改名、重试或 `--force`。
- [x] 目标创建与复制必须发生在写 `kb.md` 和 refs 之前。不要用单独的 `exists()` 检查替代不覆盖创建。

```rust
let mut input = fs::File::open(&src_path).map_err(|_| write_config_error())?;
let permissions = input.metadata().map_err(|_| write_config_error())?.permissions();
let mut output = fs::OpenOptions::new().write(true).create_new(true)
    .open(&target_abs)
    .map_err(|err| {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            map_core_error(CoreErrorKind::PathAlreadyRegistered, json!({"path": target_rel}))
        } else {
            write_config_error()
        }
    })?;
let copied = std::io::copy(&mut input, &mut output)
    .and_then(|_| output.set_permissions(permissions));
drop(output);
if copied.is_err() {
    let _ = fs::remove_file(&target_abs);
    return Err(write_config_error());
}
```

这段逻辑替换现有 `if src_path.is_file()` 内的文件复制，不顺带重定义目录/glob 的 copy 语义，也不把 append 声明失败扩展成跨文件事务。

- [x] 运行统一门禁、评审，提交 `fix: prevent copy mode from overwriting raw files`。

## 5. 阶段 3：JSONL 行级恢复及真实时间排序（R3、R6）

**修改文件：** `crates/kb-storage-fs/src/episode_files.rs`、`CHANGELOG.md`。既有 `EpisodeStore` 接口不变。

### 先改测试

- [x] 扩展 `append_separates_new_episode_from_truncated_final_line`，测试尾部既含 ASCII 半行也含不完整汉字字节；先写有效旧记录，再制造半行，再 append 有效新记录。两条有效记录均可 query/count。
- [x] 修改 `query_limit_returns_most_recent_in_order` 的输入为时间乱序 `[5, 1, 4, 2, 3]`，断言不限量升序，limit=2 为 `[4, 5]`、limit=0 为空，count 忽略 limit。
- [x] 同时间戳按 id 做稳定的次级排序，与 SQLite 行为一致；无需随机并发测试或读取真实时钟。

```rust
for ts in [5, 1, 4, 2, 3] {
    store.append(&ep(&format!("e{ts}"), "a", "s", EpisodeKind::Query, ts)).unwrap();
}
let filter = EpisodeFilter { limit: Some(2), ..Default::default() };
let out = store.query(&filter).unwrap();
assert_eq!(out.iter().map(|item| item.timestamp.as_nanos()).collect::<Vec<_>>(), vec![4, 5]);
assert_eq!(store.count(&filter).unwrap(), 5);
```

运行 `cargo test -p kb-storage-fs episode_files::tests`，修复前分别出现 UTF-8 读取错误和排序断言失败。

### 最小实现

- [x] `read_all` 使用 `fs::read`，按字节 LF 分行并独立反序列化；保留 I/O 错误，跳过损坏 JSON/UTF-8 行。JSON parser 可直接处理空白，不再先 trim 或做第二遍字符串解码。
- [x] query 过滤后排序再 limit；count 不排序，不重写或清理旧日志。
- [x] 删掉“append-only 因而天然按时间升序”的错误注释。追加位置原子性与管道的 `PIPE_BUF` 不是同一保证，说明中不再把两者等同。

```rust
for line in raw.split(|byte| *byte == b'\n') {
    if let Ok(episode) = serde_json::from_slice::<Episode>(line) {
        out.push(episode);
    }
}
```

```rust
matched.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
```

- [x] 运行统一门禁、评审，提交 `fix: recover and order filesystem episodes correctly`。

## 6. 阶段 4：显式存储配置错误时停止，禁止静默切后端（R4）

**修改文件：** `crates/kb-cli/src/storage_backend.rs`、调用 resolver 的 `handlers/ask/agent.rs` 与 `handlers/mcp/tools.rs`、`crates/kb-cli/tests/integration_memory_mcp.rs`、`crates/kb-cli/tests/integration_core_commands.rs`、`docs/cli/command-manual.md`、`docs/cli/error-catalog.md`、`CHANGELOG.md`。handlers 路径相对 `crates/kb-cli/src/commands/runtime/core/`；`guards.rs` 只修正已过时的 FS 回退注释。

### 先改测试

- [x] 替换 `unknown_backend_falls_back_to_fs` 和 `malformed_json_falls_back_to_fs`：显式非法配置应返回 `DomainError::Config`；未配置 storage 仍默认 FS。
- [x] 集成测试按以下顺序执行：SQLite 写旧记忆 → 损坏 `.kb/config.json` → remember 新记忆 → 恢复 SQLite 配置 → 查询旧记忆。
- [x] 损坏阶段必须返回 `E1004`，不创建 `.kb/episodes.jsonl`，不改动旧 SQLite 数据；恢复后旧记忆仍可见。未知 backend 用同一表驱动覆盖。
- [x] 数据不变性按 episode 内容和条数检查，不比较 SQLite 数据库文件的全部字节。
- [x] 为 ask 的存储打开/索引读取错误补 `E1004` 断言；用现有 MCP 请求 helper 表测 summary/neighbors 的非法配置返回工具错误、文本含 E1004。正常缺失数据继续走既有 `found=false` / 空 neighbors 行为。
- [x] 测试只修改隔离目录里的配置，不删除或转换真实用户数据。

运行 `cargo test -p kb-cli storage_backend::tests` 及新增集成用例（命名 `storage_config_error_does_not_switch_backend`），确认当前实现错误地产生 FS 日志。

### 最小实现

- [x] resolver 签名改为 `Result<StorageBackend, DomainError>`；只有配置不存在、合法对象中未设置 storage/backend 才默认 FS。
- [x] 显式设置的 storage/backend 类型错误、未知值、损坏 JSON、其他 I/O 错误均向上传递。BOM 按阶段 1 的规则处理。不要根据数据库文件是否存在来猜测后端。
- [x] `StorageHandles::open` 用 `?`；修正文件顶部关于“trace 留痕”和“自动回退”的不实注释。
- [x] ask/MCP 内 resolver 的另外两处调用同步改成有错误传播的调用，不使用 `.unwrap_or(Fs)` 把问题绕回去。ask 打开 storage / 读取 index 的错误有意从 E3003 改为现有 `StorageUnavailable/E1004`，复用 `storage_unavailable_error` 并保留 detail；同步错误目录和测试。
- [x] `mcp_sqlite_toolbox_warning` 改为 `Result<Option<String>, String>`，summary/neighbors 两个调用点用 `?`；错误格式沿用已有 MCP 的 E1004 文本。
- [x] 保持 `build_ask_stack` 现有返回类型和派生层降级政策：它的三条调用入口此前都已成功打开 storage 并读取 index，修复静默 FS 写分叉不依赖第二次打开的接口改造。除注释外不改 builder，也不新增其专用错误包装。

```rust
pub fn resolve_storage_backend(cwd: &Path) -> Result<StorageBackend, kb_core::DomainError>
```

```rust
match backend.trim().to_lowercase().as_str() {
    "fs" => Ok(StorageBackend::Fs),
    "sqlite" => Ok(StorageBackend::Sqlite),
    other => Err(kb_core::DomainError::config(format!("unknown storage backend: {other}"))),
}
```

错误映射明确按当前调用方处理，不声称所有命令原本都是 E1004：

| 入口 | 当前映射 | 本阶段结果 |
|---|---|---|
| remember/correct/consolidate、MCP episode 操作 | E1004 | 延续既有映射 |
| ask 打开 store / 读取 index | E3003 | 有意改为 E1004，测试与错误目录同步 |
| MCP read_summary/read_neighbors 的 backend resolver | 没有错误返回路径 | 非法配置返回含 E1004 的工具错误 |
| check 打开 store | E3003 | 保留，不在本阶段统一 |
| update 打开 store | E1003 | 保留，不在本阶段统一 |

非法配置此前可能静默写 FS，现在失败。合法默认 FS/SQLite 的路径和格式不变；上表 ask 的存储故障错误码调整是明确的对外变化，不隐藏在“透传错误”里。

- [x] 运行统一门禁、评审，提交 `fix: reject invalid storage configuration`。

## 7. 阶段 5：逐字段合并 runtime 配置（R5）

**修改文件：** `crates/kb-cli/src/runtime_config.rs`、`crates/kb-cli/src/main.rs`、`crates/kb-cli/tests/common/mod.rs`、`crates/kb-cli/tests/integration_core_commands.rs`、`docs/cli/command-manual.md`、`CHANGELOG.md`。复用已有 Cli 传递已解析的 verbose，避免扩大 dispatch/handler 签名。

### 先改测试

- [x] 替换 `runtime_config_reads_the_selected_global_config_path` 中“global ignored”断言，检查真实 `quiet=true` 及字段 trace 的 global 来源。
- [x] 在现有 runtime 模块中增加一个纯函数表驱动测试：无 flags 时 global 生效；env 的 false/0 覆盖更低层 true/非零；project 缺失字段不盖掉 global；flags 只覆盖其实际设置的字段。
- [x] 复用 CLI helper 验证 `KB_QUIET=1` 时 text stdout 为空、`KB_VERBOSE=1` 有 stderr trace；`KB_QUIET=1` 与 `-v` 并存时仍 quiet。JSON 成功输出保持既有契约。
- [x] helper 清理三个 runtime 环境变量，具体用例通过 `.env(...)` 显式覆盖；不在并行单测里修改进程环境变量。

```rust
kb_command_with_env(&dir, &[("KB_QUIET", "1")])
    .arg("env").assert().success().stdout("");
```

运行 `cargo test -p kb-cli runtime_config` 和对应集成测试，修复前 global、环境 quiet/verbose 均有失败。

### 最小实现

- [x] 复用现有 `FileConfig` 的 `Option<bool>/Option<u8>` 作为各层部分配置，保留 `Some(false)`、`Some(0)` 和 `None` 的区别；loader 不提前填默认值，并接受开头 BOM。
- [x] 当前 CLI 三个 flags 只有开启/计数语义，可用 `cli.quiet.then_some(true)`、`(cli.verbose > 0).then_some(cli.verbose)`、`cli.no_color.then_some(true)` 生成部分配置；不新增反向 flag。
- [x] 复用 `kb_core::ConfigLayers<T>` 按字段合并，不为本项目再造一套通用配置引擎。
- [x] trace 按 quiet/verbose/no_color 标明来源；删除 `RuntimeConfig.source` 这个无法描述混合来源的单值字段，入口日志打印已有的逐字段 trace 即可。
- [x] main 使用 `let mut cli = Cli::parse()`；先完成配置解析，再在 dispatch 前设置 `cli.verbose = resolved.dto.verbose`，让命令内部也使用同一详细级别，不修改用户原始参数参与合并的时机。

单字段合并沿用现有工具：

```rust
let quiet = ConfigLayers::from_layers(
    flags.quiet, env.quiet, project.quiet, global.quiet, false,
).merge();
```

其他两个字段相同规则；默认值只在这最后一步填入。`no_color` 只修正其配置值和 trace，不借机增加配色系统。

- [x] 运行统一门禁、评审，提交 `fix: merge runtime configuration per field`。

## 8. 阶段 6：Linux rc 保护与验收环境隔离（R7、R8）

**修改文件：** `packaging/linux/install.sh`、`packaging/linux/uninstall.sh`、`tests/scripts/test-linux-installer.sh`、`tests/scripts/test-windows-installer.ps1`、`docs/cli/install-and-package.md`、`CHANGELOG.md`。

### 先改测试

- [x] 用现有 Linux 发布验收脚本替换“预先把自定义目录加入 PATH”：先在隔离 home 执行默认安装，source 生成的 profile 后再通过 PATH 查找并运行 kb。
- [x] 默认场景先取消继承的 `KB_INSTALL_DIR`、`KB_BIN_NAME`，起始 PATH 不含该隔离 home 的 `.local/bin`；不要保留当前脚本给这两个变量赋值和预置 PATH 的做法。自定义目录场景单独设置安装变量。
- [x] 同脚本增加精简 fixture 场景：普通 rc、指向真实 rc 的 symlink、没有安装器 PATH 行的 rc。重复安装不重复添加；卸载后 symlink、目标权限、其他行和无匹配文件不变。
- [x] 自定义路径使用字面 `$token`、空格和反斜杠，source 后 PATH 必须指向真实目录；不使用真实用户 profile。
- [x] 用 Linux `RLIMIT_FSIZE` 在临时文件写入阶段制造失败，断言原 rc 字节、权限和 symlink 均不变。单个确定性故障用例即可，不增加备份恢复模块。
- [x] 给真实包验收注入 `KB_LLM_ENABLE_SEMANTIC=1`、假 key 和本地 HTTP stub，期望验收成功且请求计数为 0。当前代码会收到 `/v1/chat/completions` 请求，必须先记录这个失败。
- [x] Windows 脚本也覆盖继承环境清理和 finally 恢复；继续使用其已有临时注册表隔离。

运行：

```text
bash tests/scripts/test-linux-installer.sh <本次打包的-linux-x86_64.tar.gz>
powershell -NoProfile -ExecutionPolicy Bypass -File tests/scripts/test-windows-installer.ps1 -PackageDir <本次打包的-windows-x86_64.zip> -RunCliSmoke
```

两条命令的归档使用脚本当前版本现场构建，不硬编码 `0.1.4`。

### 最小实现

- [x] 自定义 PATH 行在现有 install/uninstall 两处对 `\`、`"`、`$`、反引号四种双引号内特殊字符做一致字面转义。保留默认安装路径原有 PATH 行，使历史默认安装仍能卸载。
- [x] 不选 Bash `%q` 作为持久化 rc 的通用编码：安装器会写 `.zshrc` 和 POSIX `.profile`，持久化内容应在这些 shell 中也可解析。
- [x] 卸载先检查是否有待删除的安装器 PATH 行；没有时直接跳过，不能替换 symlink 或改文件权限。
- [x] 解析现有 rc 的 symlink 实际目标，在目标同目录创建临时文件；过滤成功后 `chmod --reference` 保留目标权限，再 rename 到实际目标。原 symlink 本身保持不动；不要求保留目标 inode。
- [x] `grep` 返回 1 表示过滤后为空是正常结果，返回大于 1 必须报错并保留原文件，删除 `|| true` 吞错。写临时文件、设置权限或替换失败时仅清理本次临时文件，不能退回直接截断原 rc。
- [x] 验收脚本清除继承的 `KB_LLM_*`、`KB_QUIET/KB_VERBOSE/KB_NO_COLOR`、`KB_ASK_OFFLINE_STUB`、`KB_UPDATE_SIMULATE_LAYER_FAILURES`，再设置本次的禁 keyring、agent/session 和隔离配置。Windows 先保存并在 finally 恢复其修改的环境；Bash 验收为独立进程，不影响外层 shell。
- [x] 不新增共享安装器框架，不将一次性本地模型 stub 变成完整的 HTTP 测试服务。

在现有 `remove_path_line` 函数的文件存在检查之后，更新的关键结构如下。只保留 symlink 与 mode，不引入新的 rc 写入抽象：

```bash
local actual_file tmp_file status
actual_file="$(readlink -f -- "$profile_file")" || return 1
if grep -Fqx -- "$PATH_LINE" "$actual_file"; then
  :
else
  status=$?
  [[ "$status" -eq 1 ]] && return 0
  return "$status"
fi
tmp_file="$(mktemp "$(dirname -- "$actual_file")/.kb-path.XXXXXX")" || return 1
if grep -Fvx -- "$PATH_LINE" "$actual_file" > "$tmp_file"; then
  :
else
  status=$?
  if [[ "$status" -ne 1 ]]; then
    rm -f -- "$tmp_file"
    return "$status"
  fi
fi
if chmod --reference="$actual_file" "$tmp_file" && mv -f -- "$tmp_file" "$actual_file"; then
  return 0
fi
rm -f -- "$tmp_file"
return 1
```

临时文件必须与真实目标同目录，避免跨文件系统 mv 退化成复制。这里保留当前临时文件替换的基本能力，同时修正它覆盖 symlink 和改变权限的问题，不额外承诺 fsync 级耐久性。历史自定义路径如果含未转义 `$` 等字符，旧行与新行可能不同；只移除能精确识别的安装行，不能模糊删除用户 PATH 设置，文档说明旧行可手动清理。

- [x] 门禁、两平台真实包验收、评审，提交 `fix: preserve shell profiles during package installation`。

## 9. 阶段 7：两平台验收成功后才公开 Release（R9）

**修改文件：** `.github/workflows/release.yml`、`docs/cli/install-and-package.md`、`CHANGELOG.md`。

### 先验证现有流程的问题

- [x] 用 YAML 解析器核对当前 DAG：`prepare-release` 的公开创建没有依赖 build/验收成功；归档版本检查在创建之后才发生。
- [x] 在隔离临时目录用本地 `gh` stub 记录命令与 draft 状态，禁止使用真实 GitHub token。执行的内容必须取自 `release.yml` 的真实 `run`，不能在测试中手写另一套命令顺序。断言“模拟 Windows 验收失败时，Release 不公开”，当前命令序列应违反此断言。
- [x] 结构检查断言最终发布 job 的 `needs` 同时包含两个 build job，且没有 `always()` 绕过失败；本地 stub 不模拟整个 GitHub Actions 调度器。
- [x] 再覆盖 tag/Cargo 版本不一致时，任何 `gh release create/upload/edit` 均未执行。

### 最小实现

- [x] 准备 job checkout 对应 tag，从 workspace Cargo 读取版本；按当前格式验证 tag，并在创建 Release 前确认 `RELEASE_TAG == v{workspace_version}`。
- [x] 创建缺失 Release 时加 `--draft`；如同名 Release 已公开，明确报错，避免 `--clobber` 覆盖已有正式产物。
- [x] 两个 build job 保留现有构建、归档验收和上传；上传目标是 draft。
- [x] 增加一个只依赖两平台成功的最终 job，正常成功条件下执行 `gh release edit --draft=false`。不要使用 `always()` 绕过失败依赖。
- [x] 只修改公开时序，维持现有 create 事件和 `contents: write` 权限；不在此阶段增加预发布标记策略、完整 SemVer 校验或改用另一套发布框架。

```yaml
publish-release:
  needs: [build-linux, build-windows]
  runs-on: ubuntu-latest
  steps:
    - name: Publish accepted packages
      env:
        GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      run: gh release edit "$RELEASE_TAG" --repo "$GH_REPO" --draft=false
```

不会实际运行上面的发布命令；本阶段只在本地 stub 验证和审阅工作流。必须明确区分“YAML/DAG 与本地命令顺序通过”和“GitHub 托管端实际发布验证”。同名 draft 可重试，同名已公开 Release 报错的政策须写入安装发布文档；这不是自动撤回或重新发布既有版本的流程。

- [x] 运行统一门禁、两平台归档验收、YAML 解析和成功/失败的 stub 场景；评审后提交 `ci: publish releases after both packages pass acceptance`。

## 10. 可独立安排的小项，不扩张前七阶段

| 问题 | 证据与边界 | 最小处理 |
|---|---|---|
| SQLite 未拒绝未来 schema | 将正常库版本改为 999，当前 `remember` 仍成功写入；已复现，尚未证明造成实际格式损坏 | `schema.rs` 迁移前增加 `starting > SCHEMA_VERSION` 拒绝分支，用一个测试确认版本和数据不变；不支持降级迁移 |
| `--profile dev` 打包失败 | `scripts/package.sh:190` 与 `scripts/package.ps1:143` 使用 `dev/`，Cargo 实际输出 `debug/`；Linux 已复现 | 两脚本仅增加 `dev → debug` 映射；在 `tests/tools/packaging.test.js` 现有表测中补一个维度 |
| SQLite meta 查询吞真实错误 | `index_store.rs:42`、`search_store.rs:36`、`graph_store.rs:38` 等使用 `.ok()`；本轮为静态发现，未另造损坏 DB 扩展范围 | 后续先证明 SQL/解码错误被伪装成空快照，再考虑 `OptionalExtension::optional().map_err(...)`；正常缺行仍返回 None |

这些不作为“全部必须马上做”的附加清单。如果安排实施，也遵循先失败测试、最小实现、门禁、评审和独立提交，不能混进其他阶段的顺手重构。

## 11. 测试精简与结束条件

应替换/增强的现有测试：runtime 的“global ignored”、storage 的非法配置回退 FS、FS episode 的纯顺序/ASCII 半行数据、Linux 预置 PATH 的验收流程。这些问题来自测试输入或断言薄弱，优先修它们，而不是增加另一套平行测试。

不为删除而删除领域、端口和 CLI 集成测试。领域规则与 CLI 的错误码/磁盘不变性是不同契约；CI 回归与发布归档验收的运行时机也不同，保留有实际价值的覆盖。

阶段结束后应满足：

- [x] 失败时原全局配置和原 raw 文件不被覆盖。
- [x] 损坏日志行之外的有效 episode 可查询；最近 N 条按时间选取。
- [x] 非法显式配置不能静默改变后端，runtime 配置按字段生效。
- [x] 安装/卸载保留 rc 链接、权限和无关内容；包验收使用隔离配置且不受真实 LLM 环境影响。
- [x] Release 只在两平台包验收成功后公开；本地计划执行不触发真实发布。
- [x] 各阶段门禁、独立评审与 commit 记录写入本文；本轮提交后检查 `git status --short`。

## 12. 本轮验收结果

Windows workspace Clippy 与 354 项 Rust 测试通过；WSL Ubuntu Rust 1.89 的 Clippy 与 352 项测试通过（数量差异来自 Windows 路径和文件锁专属用例）。Linux 的 4 项 Node 测试通过；Windows 运行其中 3 项，Release Bash 工作流测试由 Linux 执行。20 组文档 fixture 匹配。

当前代码构建的 Windows x86_64 MSVC ZIP 和 Linux x86_64 GNU TAR.GZ 均完成解压、安装、`init → add → update → ask`、MCP initialize/tools/list/ping、重复安装与卸载验收。污染 `KB_LLM_*`、运行时及模拟变量后，两平台均无模型 HTTP 请求；Windows 同进程恢复原环境值的额外检查通过。Linux rc 普通文件、symlink、无匹配、四类特殊字符和写入失败检查通过。

各阶段独立评审后，另对相对基线的整体差异做只读交互复核，未发现新的阻断或重要问题。评审中指出的单测环境依赖、同时间 id 排序输入、特殊字符数据、Windows finally 范围、Release 验收步骤缺失假阳性均已处理并验证。

本地产物分别在 `target/review-implementation-2026-11-14/dist/` 与 `target/release-acceptance-2026-11-13/dist/`。它们仍是 workspace 版本 0.1.4 的验收构建。Release 工作流的成功/失败路径使用本地 gh stub，GitHub 托管端发布未执行；本轮未推送、打 tag 或创建真实 Release。

## 13. 本次复现证据

复现脚本均使用隔离 KB/用户目录，模型相关复现仅访问 `127.0.0.1` 的本地 HTTP stub。它们是本地证据，不依赖其永久存在来实施本方案；永久回归应放回上文对应的既有测试。

```text
target/review-2026-11-14/repro-cli-core.py
target/review-2026-11-14/cli-core-kj_ijeu9/results.json
target/review-2026-11-14/storage_review_repro.py
target/review-2026-11-14/storage-4a155e6c79794c6f9f4a729683e2c339/results.json
target/review-2026-11-14/repro-backend-fallback.py
target/review-2026-11-14/backend-fallback-_9ymubje/results.json
target/review-2026-11-14/review-release.sh
target/review-2026-11-14/repro-smoke-env.py
target/review-2026-11-14/repro-profile.py
target/review-2026-11-14/verify-plan-evidence.py
target/review-2026-11-14/config-shape-m1_9vikk/results.json
target/review-2026-11-14/copy_plan_semantics.rs
target/review-2026-11-14/repro-profile-write-failure.py
target/review-2026-11-14/verify-workflow-order.py
target/review-2026-11-14/check-plan-rc-snippet.py
```
