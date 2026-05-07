# KB-CLI

KB-CLI 是一个本地知识库命令行工具，用于把分散文件声明、索引、语义记忆和检索结果组织成可持续维护的知识库。

## 项目作用

- 统一管理知识源声明（`kb.md` + `./.kb/raw/`）
- 持续增量更新索引与语义层（L1-L5）
- 提供可脚本化的命令行工作流（检查、更新、问答、环境查看）

## 核心功能

- 声明管理：`add` / `rm` / `mv` / `path` / `chmod`
- 差异检查：`check` 对比声明与登记状态
- 分层编译：`update` 生成并更新 `.kb/index*`、`.kb/knowledge.json`、`.kb/search.json`、`.kb/graph.json`、`.kb/wiki/`
- 本地问答：`ask` 基于分层检索返回上下文结果
- 环境管理：`env --list` 查看系统已登记知识库

## 命令速览

- `kb init [name]`：初始化知识库目录与配置
- `kb add <path> [alias]`：添加声明
- `kb rm <alias>`：删除声明
- `kb mv <alias> <new_alias>`：重命名声明
- `kb path <alias> [new_path]`：查看或修改路径
- `kb chmod <alias|category> <perms>`：修改权限
- `kb list`：列出当前登记项
- `kb check`：检查声明与登记差异
- `kb update [--llm]`：执行分层更新（可用 `--llm` 强制启用本次 LLM 语义/图谱/wiki 构建）
- `kb ask <question>`：执行检索问答
- `kb env --list`：列出已登记知识库
- `kb completion <shell>`：生成补全脚本

## 安装快速开始

1. 打开仓库 **Releases** 页面，下载与你系统匹配的安装包：
- Windows：`kb-cli-<version>-windows-x86_64.zip`
- Linux：`kb-cli-<version>-linux-x86_64.tar.gz`

2. 解压后安装：
- Windows：双击 `install.cmd`（或执行 `install.ps1`）
- Linux：执行 `bash ./install.sh`

Windows 默认安装位置：

- 可执行文件：`%USERPROFILE%\.kb-cli\bin\kb.exe`
- 全局配置：`%USERPROFILE%\.kb-cli\config.json`

3. 验证：

```bash
kb --version
kb --help
```

## 使用快速开始

```bash
# 1) 初始化
kb init demo

# 2) 添加文件声明
kb add ./docs/a.md docs/a

# 3) 检查差异
kb check

# 4) 生成/更新索引与记忆层
kb update

# 5) 发起问答
kb ask "这个项目如何更新索引？"
```

## LLM 配置说明（简版）

`kb init` 会自动生成：

- 项目级配置：`./.kb/config.json`
- 环境模板：`./.kb/.env.template`
- 全局配置：默认 `kb-cli` 可执行文件同目录 `config.json`（Windows 安装脚本默认对应 `%USERPROFILE%\.kb-cli\config.json`，可被 `KB_GLOBAL_CONFIG` 覆盖）

LLM 生效优先级：

`环境变量 > 项目 .kb/config.json > 全局 config.json`

LLM 参与构建（`kb update`）：

- 满足 `KB_LLM_ENABLE_SEMANTIC=1` 且配置 `KB_LLM_KEY` 后，L2 语义编译将进入 `mode=llm`
- 可额外启用：
- `KB_LLM_ENABLE_GRAPH=1`：L4 图谱在规则构建后进入 LLM 增强
- `KB_LLM_ENABLE_WIKI=1`：L5 wiki 在规则构建后进入 LLM 增强
- `--llm` 会在本次 `update` 中同时尝试启用 semantic/graph/wiki 的 LLM 路径
- 默认使用 OpenAI-compatible Chat Completions（标准 `messages` 请求格式）
- 内置轻量 agent 风格原子工具上下文：
- `read_raw_file`：读取当前源文件原文片段
- `read_knowledge_graph`：读取 `.kb/graph.json` 关联关系
- `read_file_summary`：读取 `.kb/knowledge.json` 中历史摘要
- `read_graph_neighbors`：读取指定文件节点邻接关系（L4）
- `read_wiki_page`：读取 wiki 页面上下文（L5）

快速验证：

```bash
kb update --output json
```

检查返回字段：

- `data.semantic_extractor.mode = "llm"`
- `data.semantic_extractor.agent_tools.enabled = true`
- `data.semantic_extractor.agent_tools.tools` 包含上述 3 个工具名
- `data.graph_builder.mode`（`llm`/`rule`）
- `data.wiki_builder.mode`（`llm`/`rule`）

## 更多文档

- 开发者编译、测试、打包、发布：`docs/cli/developer-workflow.md`
- 安装与打包细节：`docs/cli/install-and-package.md`
- 命令手册：`docs/cli/command-manual.md`
- 命令示例：`docs/cli/command-examples.md`
- 错误目录：`docs/cli/error-catalog.md`
- 规范：`docs/cli/kb-md-spec.md`
