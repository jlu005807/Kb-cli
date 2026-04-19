# `kb.md` 规范（当前实现）

本文档描述 **当前代码实现** 中 `kb.md` 的实际语义与约束（截至 2026-04-19）。

> 运行时实现说明（2026-04-19）：CLI 命令分发已解耦为 `commands.rs` + `commands/cmd_*.rs`，`kb.md` 相关落盘逻辑位于 `commands/runtime/core/kbmd_ops.rs`。

---

## 1. 角色定位

`kb.md` 在当前版本中承担三类职责：

1. 声明知识条目（文件/目录/通配符）。
2. 声明分类与分类默认权限（供 `add/check` 的分类校验与继承）。
3. 与 `kb update` 协同：`update` 会在发现 `./.kb/raw/` 新文件时，自动回填到 `# raw` 分类。

当前状态源分工：

- **声明源**：`kb.md` + `./.kb/raw/` 目录。
- **登记源**：`./.kb/regs.json`（`list/check/update` 的主登记快照）。
- **索引源**：`./.kb/index*.json`（检索索引快照）。

---

## 2. 文件结构

`kb.md` 支持可选 Front Matter 与 Markdown 主体：

```text
---
kb: my-project
agent: default
default_perm: r
---

# 分类:权限
[alias](path):权限
```

---

## 3. Front Matter

支持字段：

- `kb`（可选）
- `agent`（可选）
- `default_perm`（可选，默认 `r`）

实现约束：

- Front Matter 必须以 `---` 开始并以 `---` 结束。
- `default_perm` 必须是合法权限串（`r/w/x/c/d/-` 组合规则同命令层）。

---

## 4. 分类标题与权限继承

支持 `#` / `##` / `###` 等层级标题。

- 标题可写权限后缀：`# docs:rw`
- 若无权限后缀，则继承上级标题权限；若无上级，则继承 `default_perm`（默认 `r`）。
- `add --category` 与 `check --category` 的合法分类来自解析后的标题集合。

示例：

```markdown
---
default_perm: r
---

# docs:rw
## api
### internal:r
```

---

## 5. 引用条目语法

### 5.1 当前主语法（推荐）

```markdown
[alias](path):perm  # comment
```

- `alias` 可为空：`[](path)`
- 若 `alias` 为空，系统使用规范化 `path` 作为登记标识
- `:perm` 可省略，省略时继承当前分类权限

### 5.2 兼容语法（历史）

仍兼容旧格式：

```markdown
- alias | path
```

兼容格式不携带显式权限，按当前分类权限继承。

---

## 6. 路径类型与通配

当前实现支持以下声明方式：

- 文件：`[a](docs/a.md)`
- 目录递归：`[src](src/)`、`[all](src/**)`
- 目录单层：`[api](docs/api/*)`
- 通配：`*`、`**`、`?`、`[a-z]`、`{md,txt}`

> 解析结果会被归一化为项目内相对路径（如 `./docs/a.md`）。

---

## 7. 与 `regs/index` 的关系

### 7.1 `./.kb/regs.json`

- `list`：读取 `regs.json` 列表（不是直接扫描 `kb.md`）。
- `check`：比较“声明集（`kb.md` + raw）”与 `regs.json`。
- `update`：应用差异到 `regs.json`（新增写入、删除/不存在移除）。

兼容性：若 `regs.json` 不存在，当前实现会尝试从历史 `refs.json` 迁移读取。

### 7.2 `kb update` 对 `kb.md` 的回填

`kb init` 会创建 `# raw` 分类。

`kb update` 发现 `./.kb/raw/` 新文件后，会按规范追加到 `kb.md` 的 `# raw` 分类，格式为：

```markdown
[](./.kb/raw/<file>)
```

并具备去重保障（不会重复追加同一条目）。

---

## 8. 命令约束

- `kb.md` 无法读取或语法非法：`E3003`
- 分类不存在：`E2007`
- 受保护路径（`kb.md` 与 `./.kb/` 非 raw 内部文件）不可加入索引：`E2009`

---

## 9. 关联文档

- 命令手册：`docs/cli/command-manual.md`
- 命令示例：`docs/cli/command-examples.md`
- 状态差异：`docs/cli/state-diff-contract.md`
- 忽略规则：`docs/cli/kbignore-spec.md`
- 错误目录：`docs/cli/error-catalog.md`

---

## 10. 实现说明（2026-04-19）

- 真值关系：声明来自 `kb.md + ./.kb/raw/`；`./.kb/regs.json` 是同步后的登记快照。
- `kb add`、`kb rm`、`kb chmod` 优先更新声明源（`kb.md`），不直接写 `regs.json`。
- `kb check` 对比声明集与 `regs.json`，并可识别权限元数据变化（`metadata_changed`）。
- 当 `kb check` 报告 `to_remove` 时，`kb update` 会清理索引快照中的对应陈旧记录。
- 即使某些路径已不在 `./.kb/regs.json` 中，但仍残留在基线 `index.json`，上述清理仍会执行。
- `kb chmod <target> <perms>` 仅修改 `kb.md` 内权限声明（引用或分类），不直接写 `regs.json`。
- `kb rm <target>` 从 `kb.md` 删除匹配声明（按 alias/id/path）；真正同步到 `regs.json` 仅发生在 `kb update`。
- `kb update` 会把声明差异同步到 `./.kb/regs.json`，更新后端索引产物，并在 `check` 确认 `not_found` 时删除 `kb.md` 中缺失引用行，同时按路径去重重复 raw 引用。
