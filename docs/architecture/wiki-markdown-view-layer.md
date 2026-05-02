# KB-CLI Wiki Markdown 输出视图层（L5）设计文档

> 状态：Draft（2026-05-02）  
> 目标：在现有 L0-L4 之上新增 **L5: Wiki Markdown View Layer**，产出稳定、可读、可追溯的 wiki 视图。

## 1. 背景

当前记忆模型已经覆盖：

1. L0 声明层（`kb.md` + raw/外部路径）
2. L1 机器快照层（`index*.json`）
3. L2 语义层（`knowledge.json`）
4. L3 搜索投影层
5. L4 图谱层（`graph.json`）

但还缺一个“人可读、可持续累积”的输出层，来承接 LLM Wiki 的核心价值：  
**把机器记忆编译为可浏览、可引用、可审阅的 Markdown Wiki。**

## 2. 层定义

L5 是**视图层/投影层**，不是事实源：

1. 输入来自 L2/L4（必要时回读 L1）
2. 输出为 Markdown 页面集合
3. 可删除重建，不反向覆盖 L0/L1/L2 原始事实

核心原则：

1. 不修改用户源文件
2. 每条关键结论可追溯到 `record_id/chunk_id`
3. 默认 deterministic 生成，LLM 只做可选润色

## 3. 文件布局（建议）

```text
.kb/wiki/
  index.md
  log.md
  overview.md
  pages/
    source/<record_id>.md
    concept/<slug>.md
    topic/<slug>.md
  manifest.json
```

说明：

1. `index.md`：目录入口（分类 + 一行摘要）
2. `log.md`：按运行批次追加更新日志
3. `overview.md`：全局摘要和变化提示
4. `pages/source/*`：单文档语义页
5. `pages/concept/*`：概念聚合页
6. `pages/topic/*`：主题/社区页（可选第一阶段）
7. `manifest.json`：页面签名与来源映射，支持增量构建

## 4. 页面契约

每个页面前置元数据（YAML frontmatter）建议至少包含：

1. `id`
2. `type`（source/concept/topic/overview）
3. `title`
4. `updated_at`
5. `source_records`
6. `source_chunks`
7. `generator`（rule/llm + model/base_url + run_id）
8. `input_signature`

正文建议结构：

1. `Summary`
2. `Key Points`
3. `Evidence`（指向 record/chunk）
4. `See Also`（wikilinks）

## 5. 生成流程

L5 生成建议拆为两阶段：

Stage A（必选，deterministic）：

1. 读取 L2/L4 快照
2. 规划页面增量（新增/更新/删除）
3. 模板渲染 Markdown（规则化结构）
4. 更新 `index.md`、`log.md`、`manifest.json`

Stage B（可选，LLM 润色）：

1. 输入 Stage A 页面草稿 + 元数据
2. LLM 仅优化可读性、段落组织、标题表达
3. 不得删除 evidence 区块，不得改写来源指针
4. 输出不合规时回退 Stage A 文本

## 6. 与 `kb update` 的关系

新增阶段标识建议：

1. `l5_wiki_view`

默认模式：

1. L5 失败记 warning，不影响 L0/L1 成功态

严格模式（`--strict`）：

1. L5 失败计入 errors，命令整体失败

输出沿用现有契约：

1. `layer_status`
2. `warnings/errors`
3. `retryable`

## 7. Ask 消费策略（后续）

`kb ask` 默认仍以 L3/L2/L1 为检索主路径；  
L5 作为“人读视图 + 轻量补充上下文”：

1. 优先从 L3/L2 取证据块
2. 当问题偏总结/对比时，可读 L5 对应页面作为提示层
3. 任何结论仍应可回到 L2/L1 证据

## 8. 风险与边界

风险：

1. L5 页面漂移（内容好看但失真）
2. 页面数量膨胀
3. 跨页链接失效

控制：

1. 强制 evidence 区块 + source id 映射
2. 页面签名与 manifest 增量清理
3. lint 检查 orphan page / broken wikilink

## 9. 完成定义（L5 第一阶段）

1. `.kb/wiki/` 能稳定生成 `index.md + source pages + manifest.json`
2. 每个 source 页面含可追溯来源元数据
3. `kb update` 输出新增 `l5_wiki_view` 状态
4. 默认模式下 L5 失败不影响基础更新成功
5. `--strict` 下 L5 失败可阻断并返回非 0
