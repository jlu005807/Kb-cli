# `.kbignore` 规范（当前实现）

本文档定义 `./.kb/.kbignore` 在当前版本中的解析与生效边界（截至 2026-04-16）。

---

## 1. 文件位置

- 固定路径：`./.kb/.kbignore`
- `kb init` 会自动创建该文件，并写入默认规则（包含 `kb.md` 与 `.kb` 内部配置文件）

默认规则示例（当前实现）：

```text
./kb.md
./.kb/.kbignore
./.kb/refs.json
./.kb/index.json
./.kb/index.errors.json
./.kb/index.meta.json
./.kb/schema.version
./.kb/*.lock
```

---

## 2. 语法子集

当前实现支持：

1. 空行（忽略）
2. 注释行（以 `#` 开头）
3. 通配符：
   - `*`：任意长度字符
   - `?`：单个字符
4. 目录规则：
   - 以 `/` 结尾的规则按前缀匹配

---

## 3. 路径归一化

规则在匹配前会先进行路径归一化：

1. `\` 统一为 `/`
2. 压缩重复 `/`
3. 相对路径补齐 `./` 前缀

---

## 4. 生效命令

### 4.1 `kb add`

- 如果输入路径命中规则：
  - 返回成功包（`ignored=true`）
  - 不写入 refs 状态
  - 不作为错误
- 额外保护：
   - `kb.md` 与 `./.kb/` 下除 `./.kb/raw/` 外的内部文件属于受保护路径
   - 即使未显式写入 `.kbignore`，也会被拦截（`E2009`）

### 4.2 `kb check` / `kb update`

- 在差异计算前对 `.kb/refs.json` 投影记录进行过滤
- 命中规则的路径不进入变更统计
- 对受保护路径执行强制过滤（`kb.md` 与 `./.kb/` 非 raw 文件）

---

## 5. 当前边界

当前版本不支持：

1. 复杂反选链（如多层 `!` 组合）
2. 级联多层 `.kbignore`
3. 完整 gitignore 语义等价实现

---

## 6. 关联文档

- 命令手册：`docs/cli/command-manual.md`
- 状态差异：`docs/cli/state-diff-contract.md`
- kb.md 规范：`docs/cli/kb-md-spec.md`
