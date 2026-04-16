# KB-CLI 安装与打包指南（Windows / Linux）

本文档说明如何把 `kb-cli` 打包为可分发安装包，并在用户机器上一键安装，同时自动配置环境变量（PATH）。

## 1. 产物与目录约定

- Windows 打包脚本：`scripts/package.ps1`
- Linux 打包脚本：`scripts/package.sh`
- Windows 安装模板：`packaging/windows/*`
- Linux 安装模板：`packaging/linux/*`
- 默认输出目录：`dist/`

## 2. Windows 打包与安装

### 2.1 本地打包

在仓库根目录执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

可选参数：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1 `
  -Target x86_64-pc-windows-msvc `
  -Profile release `
  -DistDir dist
```

### 2.2 打包产物

示例：

```text
dist/
└─ kb-cli-0.1.0-windows-x86_64.zip
   └─ kb-cli-0.1.0-windows-x86_64/
      ├─ kb.exe
      ├─ install.cmd
      ├─ install.ps1
      ├─ uninstall.cmd
      └─ uninstall.ps1
```

### 2.3 一键安装

- 双击 `install.cmd`（推荐，已内置 `ExecutionPolicy Bypass`）
- 或命令行执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

安装行为：

- 将可执行文件安装到用户目录：`%LOCALAPPDATA%\kb-cli\bin\kb.exe`
- 自动写入用户级 `PATH`
- 当前会话内尝试即时注入 PATH（新终端一定生效）

### 2.4 卸载

- 双击 `uninstall.cmd`
- 或执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
```

卸载行为：

- 删除已安装二进制
- 清理用户级 PATH 中的安装目录
- 安装目录为空时自动删除

## 3. Linux 打包与安装

### 3.1 本地打包

在仓库根目录执行：

```bash
bash ./scripts/package.sh
```

可选参数：

```bash
bash ./scripts/package.sh \
  --target x86_64-unknown-linux-gnu \
  --profile release \
  --dist-dir dist
```

### 3.1.1 在 Git Bash（Windows）中打 Linux 包

可以，但建议显式指定 Linux target，并先安装对应 Rust target：

```bash
rustup target add x86_64-unknown-linux-gnu
bash ./scripts/package.sh --target x86_64-unknown-linux-gnu
```

说明：

- `scripts/package.sh` 在 Git Bash/MSYS/Cygwin 环境下，若未传 `--target` 会直接报错并给出提示。
- 该脚本只用于 Linux 产物打包；Windows 产物请使用 `scripts/package.ps1`。

### 3.2 打包产物

示例：

```text
dist/
└─ kb-cli-0.1.0-linux-x86_64.tar.gz
   └─ kb-cli-0.1.0-linux-x86_64/
      ├─ kb
      ├─ install.sh
      └─ uninstall.sh
```

### 3.3 一键安装

```bash
bash ./install.sh
```

安装行为：

- 将二进制安装到 `~/.local/bin/kb`（默认）
- 若当前 `PATH` 未包含安装目录，自动追加到：
  - `~/.bashrc`
  - `~/.zshrc`
  - `~/.profile`
- 避免重复写入同一行 PATH 配置

### 3.4 卸载

```bash
bash ./uninstall.sh
```

卸载行为：

- 删除安装的 `kb` 二进制
- 从 shell 配置文件中清理安装时追加的 PATH 行
- 安装目录为空时自动删除

## 4. 自定义安装目录（高级）

### 4.1 Linux

通过环境变量覆盖安装目录/命令名：

```bash
KB_INSTALL_DIR="$HOME/tools/bin" KB_BIN_NAME="kb" bash ./install.sh
```

### 4.2 Windows

通过参数覆盖安装目录/命令名：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 `
  -InstallDir "$env:LOCALAPPDATA\kb-cli\bin" `
  -BinaryName "kb.exe"
```

## 5. 验证安装

```bash
kb --help
kb --version
```

若命令未生效，请重开终端后重试。

## 6. GitHub 自动打包并发布 Release

仓库已提供 workflow：`.github/workflows/release.yml`。

触发方式：

- 推送 tag（`v*`）时自动触发，例如 `v0.1.0`
- 也可在 GitHub Actions 页面手动触发（`workflow_dispatch`）

流程内容：

- Ubuntu runner 构建并打包 Linux 产物（`.tar.gz`）
- Windows runner 构建并打包 Windows 产物（`.zip`）
- 自动创建/更新对应 GitHub Release，并上传上述产物附件

### 6.1 推荐发布步骤

```bash
git tag v0.1.0
git push origin v0.1.0
```

推送完成后，等待 Actions 执行成功，即可在 Releases 页面看到对应版本与附件。
