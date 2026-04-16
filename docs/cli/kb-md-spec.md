# `kb.md` 规范（当前实现）

本文档描述 **当前代码实现** 中 `kb.md` 的实际语义与约束（截至 2026-04-15）。

---

## 1. 定位

`kb.md` 在当前版本的作用是：

1. 作为可读清单文件（面向人工查看）
2. 提供分类标题来源（`add --category` / `check --category` 校验）
3. 接收 `kb add` 的追加写入

> 说明：当前状态差异与索引更新的主数据来源是 `.kb/refs.json`，不是 `kb.md` 条目解析结果。

---

## 2. 当前可识别结构

### 2.1 分类标题

当前实现会识别形如 `# <category>` 的标题行作为分类集合来源。

示例：

```markdown
# raw
# docs
# src
```

### 2.2 条目写入格式

`kb add` 追加写入时使用以下格式：

```markdown
- <alias> | <path>
```

示例：

```markdown
# raw
- docs/config | ./docs/config.md
```

---

## 3. 当前未实现能力

以下能力在当前版本**不作为解析契约**：

1. Front Matter / YAML 头部解析
2. 多行对象条目（`alias/path/perms` 分行结构）解析
3. 通过 `kb.md` 条目直接驱动 `check/update` 的全量状态构建

---

## 4. 与 `.kb/refs.json` 的关系

当前实现中：

- `kb add/mv/rm/path/chmod` 会更新 `.kb/refs.json`
- `kb check/update/list` 主要基于 `.kb/refs.json` 投影后的记录运行
- `kb.md` 主要用于“分类定义 + 文本展示”

---

## 5. 命令交互约束

### 5.1 `kb init`

- 初始化时会写入 `kb.md`，默认内容为：

```markdown
# raw
```

### 5.2 `kb add --category`

- 指定分类必须已出现在 `kb.md` 的标题行中
- 不存在则返回 `E2007`

### 5.3 `kb check --category`

- 分类合法性同样基于 `kb.md` 标题行
- `kb.md` 不可读时返回 `E3003`

### 5.4 `kb update`

- 当前不会解析 `kb.md` 条目来构建索引输入
- 以 `.kb/refs.json` 投影记录作为输入快照

---

## 6. 兼容性建议

为降低歧义，建议：

1. 将 `kb.md` 视为“展示清单 + 分类注册表”
2. 将 `.kb/refs.json` 视为“命令执行与索引更新的状态源”
3. 修改分类前先确保 `kb.md` 存在对应 `# <category>` 标题

---

## 7. 关联文档

- 命令语义：`docs/cli/command-manual.md`
- 状态差异：`docs/cli/state-diff-contract.md`
- 忽略规则：`docs/cli/kbignore-spec.md`
- 错误目录：`docs/cli/error-catalog.md`
