# KB-CLI 状态差异契约（当前实现）

本文档描述 `kb check` / `kb update` 的**当前实现行为**。

> 适用范围：本地索引链路（`.kb/regs.json` + `./.kb/.kbignore` + `.kb/index*.json`）。
> 说明：登记快照现为 `.kb/regs.json`；`.kb/refs.json` 仅作为历史格式的迁移读取来源。

---

## 1. 数据来源

### 1.1 当前快照（Current Snapshot）

当前快照由 `.kb/regs.json` 投影得到（`regs.json` 不存在时回退读取历史 `refs.json`）：

1. 读取登记条目（alias/path/category/perms）
2. 应用过滤条件：
   - `--category`
   - `--raw-only`（仅保留 `path` 以 `./.kb/raw/` 开头）
   - `.kbignore`
  - 受保护路径过滤（排除 `kb.md` 与 `./.kb/` 下非 `./.kb/raw/` 文件）
3. 对过滤后的条目，只保留本地路径真实存在的记录
4. 计算记录签名（signature）与摘要（excerpt）

### 1.2 基线快照（Baseline Snapshot）

基线来自最近一次成功写入的索引文件：

- `.kb/index.json`
- `.kb/index.errors.json`
- `.kb/index.meta.json`

若索引不存在，则按“空基线”处理。

---

## 2. 差异判定规则

默认身份键：`path`（路径身份）。

在路径身份策略下：

- 新路径出现：`to_add`
- 同路径签名变化：`to_update`
- 基线路径消失：`to_remove`（reason = `missing_path`）

---

## 3. `kb check` 契约

`kb check` 只做差异预览，不落盘。

输出字段：

- `has_changes`
- `to_add`
- `to_update`
- `to_remove`
- `suggested_command`
- `duration_ms`

---

## 4. `kb update` 契约

`kb update` 在当前快照与基线比较后执行：

- 默认：缺失路径计入 `marked_invalid`
- `--prune`：缺失路径计入 `removed`，并删除 `kb.md` 中确认为 `not_found` 的声明行
- `--dry-run`：仅返回统计，不写索引文件

缺失声明默认留在 `kb.md`，并通过 `missing_declarations` 单独报告；该字段不计入 warnings，也不会导致 `--strict` 失败。

输出字段：

- `added`
- `updated`
- `removed`
- `marked_invalid`
- `duration_ms`
- `full`
- `raw_category_created`（`update` 在自动回填 raw 声明时若新建了 `# raw` 分类则为 `true`）

---

## 5. 与选项的关系

### 5.1 `--raw-only`（仅 check）

- 仅保留 `path` 以 `./.kb/raw/` 开头的登记记录参与比较

### 5.2 `--category`（仅 check）

- 仅保留指定分类条目
- 分类集合来自 `kb.md` 标题行（`# <category>`）

### 5.3 `--yes`（已移除）

- 该标志从未产生任何行为，已从 CLI 移除；现在传入 `--yes` 会得到参数解析错误。

---

## 6. 幂等性

在输入状态不变时：

- 连续两次 `kb update`，第二次应返回：
  - `added=0`
  - `updated=0`
  - `removed=0`

---

## 7. 输出确定性

当前实现保持以下确定性约束：

1. 输出结构稳定（JSON 字段层级固定）
2. 列表顺序稳定（底层按路径/别名排序）
3. 同一输入重复执行输出一致

---

## 8. 关联文档

- 命令手册：`docs/cli/command-manual.md`
- kb.md 规范：`docs/cli/kb-md-spec.md`
- 忽略规则：`docs/cli/kbignore-spec.md`
- 错误目录：`docs/cli/error-catalog.md`
