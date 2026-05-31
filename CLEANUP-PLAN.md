# 项目清理计划

## 🗑️ 可删除的冗余文件/目录

### 1. 冗余 README 文件
- `README-OLD.md` (20K) - 旧版备份，已不需要
- `README-SIMPLE.md` (6.0K) - 新创建的简化版，待替换

**建议：** 用 README-SIMPLE.md 替换 README.md，删除 README-OLD.md

### 2. Git Worktrees（1.2G）
- `.worktrees/memory-model-impl/` - 旧的 git worktree，已合并到主分支

**建议：** 删除整个 .worktrees 目录（节省 1.2G 空间）

### 3. Sisyphus 目录（103K）
- `.sisyphus/` - 开发过程中的草稿和笔记

**建议：** 
- 如果是开发工具生成的，可以删除
- 或者添加到 .gitignore

### 4. 优化报告文件
- `DOCUMENTATION-OPTIMIZATION-SUMMARY.md` (9.4K) - 可以移到 docs/ 目录

**建议：** 移动到 docs/ 目录统一管理

### 5. 分发包目录
- `dist/` (7.7M) - 构建产物

**建议：** 
- 添加到 .gitignore
- 不应该提交到 git

## 📊 清理效果预估

| 项目 | 大小 | 操作 |
|------|------|------|
| .worktrees | 1.2G | 删除 |
| README-OLD.md | 20K | 删除 |
| dist/ | 7.7M | 添加到 .gitignore |
| .sisyphus | 103K | 检查后决定 |

**总节省空间：** ~1.2G

## ✅ 清理步骤

1. 备份当前 README.md
2. 用 README-SIMPLE.md 替换 README.md
3. 删除 README-OLD.md 和 README-SIMPLE.md
4. 删除 .worktrees 目录
5. 移动 DOCUMENTATION-OPTIMIZATION-SUMMARY.md 到 docs/
6. 更新 .gitignore（添加 dist/）
7. 检查 .sisyphus 是否需要保留
8. 提交清理

## 🔍 需要确认的项目

- `.sisyphus/` - 是否是开发工具生成的？
- `dist/` - 是否应该在 git 中？
- `.worktrees/` - 是否还在使用？
