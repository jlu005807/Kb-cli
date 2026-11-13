# KB-CLI 安装与打包指南（Windows / Linux）

本文说明如何获取并安装 `kb`，以及如何把 `kb-cli` 打包成可分发的安装包。安装脚本会自动完成二进制落盘与用户级 `PATH` 配置。

相关文件（本文的事实来源）：

- 打包脚本：`scripts/package.ps1`（Windows）、`scripts/package.sh`（Linux）
- 安装脚本模板：`packaging/windows/`（`install.ps1`、`install.cmd`、`uninstall.ps1`、`uninstall.cmd`）、`packaging/linux/`（`install.sh`、`uninstall.sh`）
- Release 流水线：`.github/workflows/release.yml`
- 默认输出目录：`dist/`

## 1. 产物命名约定

安装包统一命名为：

```text
kb-cli-{version}-{os}-{arch}
```

- `version`：取自工作区 `Cargo.toml` 的 `[workspace.package].version`（如 `0.1.4`）
- `os`：`windows` 或 `linux`
- `arch`：`x86_64` / `aarch64` / `x86`（由 target triple 或宿主机架构推断）

示例：

- Windows：`kb-cli-0.1.4-windows-x86_64.zip`
- Linux：`kb-cli-0.1.4-linux-x86_64.tar.gz`

## 2. 从 Release 安装（推荐）

从 GitHub Release 下载对应平台的安装包并解压。

### 2.1 Windows

压缩包内容：

```text
kb-cli-<version>-windows-x86_64/
├─ kb.exe
├─ install.cmd / install.ps1
├─ uninstall.cmd / uninstall.ps1
└─ INSTALL.md
```

安装方式（二选一）：

- 双击 `install.cmd`（内部以 `ExecutionPolicy Bypass` 调用 `install.ps1`）
- 命令行执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

安装行为（`install.ps1`）：

1. 把 `kb.exe` 复制到 `%USERPROFILE%\.kb-cli\bin\kb.exe`（`USERPROFILE` 缺失时回退到 `%LOCALAPPDATA%`；可用 `-InstallDir` 覆盖安装目录）。
2. 确保全局配置 `%USERPROFILE%\.kb-cli\config.json` 存在：若已存在则保留 `knowledge_bases`、`llm` 及其他未知字段，只补缺少的默认 LLM 键。配置使用不带 BOM 的 UTF-8 保存。已有 JSON 无法解析时安装会报错，不会静默覆盖配置。
3. 把安装目录写入**用户级** `PATH`（已存在则跳过），并尝试在当前会话立即生效。

验证：

```powershell
kb --help
# 若提示找不到 kb，重新打开终端让 PATH 生效
```

卸载：双击 `uninstall.cmd` 或执行 `uninstall.ps1`。会删除 `kb.exe`、从用户级 `PATH` 移除安装目录，并在目录为空时删除该目录（`config.json` 所在目录非空时会保留）。

### 2.2 Linux

压缩包内容：

```text
kb-cli-<version>-linux-x86_64/
├─ kb
├─ install.sh
├─ uninstall.sh
└─ INSTALL.md
```

安装：

```bash
bash ./install.sh
```

安装行为（`install.sh`）：

1. 把 `kb` 安装到 `~/.local/bin/kb`（可用环境变量 `KB_INSTALL_DIR` / `KB_BIN_NAME` 覆盖），权限 `0755`。
2. 若当前 `PATH` 未包含安装目录，则向已存在的 `~/.bashrc`、`~/.zshrc`、`~/.profile` 追加一行 `export PATH="$HOME/.local/bin:$PATH"`（都不存在时写入 `~/.profile`；重复执行不会重复追加）。

验证：

```bash
kb --help
# 若未识别，执行 source ~/.profile 或重开终端
```

卸载：`bash ./uninstall.sh`。会删除二进制、清理 rc 文件中由安装脚本追加的 PATH 行，并在安装目录为空时删除该目录。

## 3. 从源码构建

```bash
git clone <repo-url>
cd Kb-cli

# 构建 release 二进制
cargo build -p kb-cli --release

# 产物位置
# Windows: target/release/kb-cli.exe
# Linux:   target/release/kb-cli
```

可以直接把产物改名为 `kb`（或 `kb.exe`）放入自己的 PATH，也可以继续走第 4 节的打包脚本生成标准安装包。

提示：在仓库内直接运行 `packaging/windows/install.ps1` 也能安装——它会自动从 `target/release/` 或最新的 `dist/kb-cli-*-windows-*/` 目录发现二进制。

## 4. 本地打包

### 4.1 Windows：`scripts/package.ps1`

```powershell
# 使用宿主默认 target
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1

# 显式指定 target / profile / 输出目录
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1 `
  -Target x86_64-pc-windows-msvc `
  -Profile release `
  -DistDir dist

# 跳过构建，直接打包已有二进制
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1 -SkipBuild
```

行为要点：

- **只接受 Windows target**（triple 含 `-windows-`；传入其他 target 直接报错并提示改用 `package.sh`）。
- 传入 `-Target` 时会校验该 target 已通过 `rustup target add` 安装。
- 执行 `cargo build -p kb-cli --profile <profile>`，把 `kb-cli.exe` 重命名为 `kb.exe`，连同 `packaging/windows/` 四个安装/卸载脚本与生成的 `INSTALL.md` 一起放入 `dist/kb-cli-<version>-windows-<arch>/`，并压缩为同名 `.zip`。

### 4.2 Linux：`scripts/package.sh`

```bash
# 常规用法（推荐显式指定 target）
bash ./scripts/package.sh --target x86_64-unknown-linux-gnu

# 其他选项
bash ./scripts/package.sh --profile release --dist-dir dist --skip-build
```

行为要点：

- **只接受 Linux target**（triple 含 `-linux-`；否则报错并提示改用 `package.ps1`）。
- 在 Git Bash / MSYS / Cygwin 环境下运行时必须显式传 `--target <linux triple>`。
- 执行 `cargo build -p kb-cli --profile <profile>`，把二进制重命名为 `kb`，连同 `packaging/linux/install.sh`、`uninstall.sh` 与生成的 `INSTALL.md` 打入 `dist/kb-cli-<version>-linux-<arch>/`，并归档为同名 `.tar.gz`。

## 5. Release 自动发布（GitHub Actions）

工作流：`.github/workflows/release.yml`，在推送 `v*` 格式（`vMAJOR.MINOR.PATCH`）的 tag 时触发：

1. `prepare-release`：校验 tag 格式，创建（或复用）对应 GitHub Release。
2. `build-linux`：在 `ubuntu-latest` 上执行 `bash ./scripts/package.sh --target x86_64-unknown-linux-gnu`，上传 `dist/*.tar.gz`。
3. `build-windows`：在 `windows-latest` 上执行 `./scripts/package.ps1 -Target x86_64-pc-windows-msvc`，上传 `dist/*.zip`。

发布步骤：

```bash
git push origin main
git tag v0.1.4
git push origin v0.1.4
```

## 6. 关联文档

- 开发者工作流：`docs/cli/developer-workflow.md`
- 命令手册：`docs/cli/command-manual.md`
- 项目主页安装说明：`README.md`
