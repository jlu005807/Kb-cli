# 贡献指南

感谢你对 KB-CLI 的关注！我们欢迎各种形式的贡献。

---

## 📋 目录

- [行为准则](#行为准则)
- [如何贡献](#如何贡献)
- [开发环境设置](#开发环境设置)
- [代码规范](#代码规范)
- [提交规范](#提交规范)
- [测试指南](#测试指南)
- [文档贡献](#文档贡献)

---

## 行为准则

我们致力于为所有人提供友好、安全和包容的环境。参与本项目即表示你同意遵守以下准则：

- 尊重不同的观点和经验
- 优雅地接受建设性批评
- 关注对社区最有利的事情
- 对其他社区成员表现出同理心

---

## 如何贡献

### 报告 Bug

发现 Bug？请通过 [GitHub Issues](https://github.com/your-repo/kb-cli/issues/new) 报告，并包含：

- **清晰的标题**：简洁描述问题
- **复现步骤**：详细的操作步骤
- **期望行为**：你期望发生什么
- **实际行为**：实际发生了什么
- **环境信息**：操作系统、Rust 版本、KB-CLI 版本
- **日志输出**：相关的错误日志（使用 `-vv` 获取详细日志）

**示例：**

```markdown
### Bug 描述
运行 `kb update --llm` 时出现超时错误

### 复现步骤
1. 初始化知识库：`kb init test`
2. 添加文件：`kb add README.md`
3. 运行更新：`kb update --llm`

### 期望行为
成功更新索引并启用 LLM 增强

### 实际行为
超时错误：`E1008: LLM request timeout`

### 环境信息
- OS: Windows 11
- Rust: 1.75.0
- KB-CLI: 0.1.4

### 日志
```
[详细日志内容]
```
```

### 提出功能请求

有新想法？请通过 [GitHub Issues](https://github.com/your-repo/kb-cli/issues/new) 提出，并包含：

- **功能描述**：清晰描述你想要的功能
- **使用场景**：为什么需要这个功能
- **替代方案**：是否考虑过其他方案
- **额外信息**：任何有助于理解的信息

### 提交代码

1. **Fork 仓库**
2. **创建分支**：`git checkout -b feature/your-feature`
3. **编写代码**：遵循代码规范
4. **编写测试**：确保测试覆盖
5. **提交更改**：遵循提交规范
6. **推送分支**：`git push origin feature/your-feature`
7. **创建 Pull Request**

---

## 开发环境设置

### 前置要求

- **Rust**: 1.70 或更高版本
- **Git**: 用于版本控制
- **Cargo**: Rust 包管理器（随 Rust 安装）

### 克隆仓库

```bash
git clone https://github.com/your-repo/kb-cli.git
cd kb-cli
```

### 构建项目

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 运行集成测试
bash test-mcp-basic.sh
```

### 运行 Linter

```bash
# 检查代码格式
cargo fmt --check

# 格式化代码
cargo fmt

# 运行 Clippy
cargo clippy -- -D warnings
```

### 本地测试

```bash
# 使用开发构建测试
./target/debug/kb-cli --version

# 初始化测试知识库
./target/debug/kb-cli init test-kb
cd test-kb
../target/debug/kb-cli add README.md
../target/debug/kb-cli update
```

---

## 代码规范

### Rust 代码风格

遵循 [Rust 官方风格指南](https://doc.rust-lang.org/1.0.0/style/)：

- 使用 4 空格缩进
- 使用 `snake_case` 命名变量和函数
- 使用 `PascalCase` 命名类型和 trait
- 使用 `SCREAMING_SNAKE_CASE` 命名常量
- 每行最多 100 字符

### 项目约定

1. **六边形架构**
   - `kb-core`: 领域逻辑，不依赖外部
   - `kb-storage-*`: 存储适配器
   - `kb-cli`: CLI 表层

2. **错误处理**
   - 使用 `Result<T, E>` 返回可能失败的操作
   - 自定义错误类型继承 `std::error::Error`
   - 提供有意义的错误消息

3. **测试**
   - 单元测试放在模块内部
   - 集成测试放在 `tests/` 目录
   - 使用 `#[cfg(test)]` 标记测试模块

### 代码示例

```rust
// 好的示例
pub fn add_knowledge_entry(
    store: &mut dyn KnowledgeStore,
    entry: KnowledgeEntry,
) -> Result<(), KbError> {
    store.insert(entry)?;
    Ok(())
}

// 避免
pub fn AddKnowledgeEntry(Store: &mut dyn KnowledgeStore, Entry: KnowledgeEntry) {
    Store.insert(Entry).unwrap(); // 不要使用 unwrap
}
```

---

## 提交规范

使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

### 格式

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Type

- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式（不影响功能）
- `refactor`: 重构（不是新功能也不是 Bug 修复）
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建过程或辅助工具的变动

### Scope

- `core`: kb-core 模块
- `cli`: kb-cli 模块
- `storage`: 存储相关
- `llm`: LLM 集成
- `mcp`: MCP server
- `docs`: 文档

### 示例

```bash
# 新功能
git commit -m "feat(mcp): add verify_citation tool"

# Bug 修复
git commit -m "fix(storage): resolve sqlite transaction deadlock"

# 文档更新
git commit -m "docs(readme): add FAQ section"

# 重构
git commit -m "refactor(core): simplify episode consolidation logic"
```

### 完整示例

```
feat(mcp): add verify_citation tool

Add lexical verification for citation quotes to prevent hallucination.
The tool checks if a quote exists in the specified chunk using substring
match and token overlap.

Closes #123
```

---

## 测试指南

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_entry() {
        let mut store = MockKnowledgeStore::new();
        let entry = KnowledgeEntry::new("test");
        
        let result = add_knowledge_entry(&mut store, entry);
        
        assert!(result.is_ok());
    }
}
```

### 集成测试

```rust
// tests/integration_test.rs
use kb_cli::commands::run_init;

#[test]
fn test_init_command() {
    let temp_dir = tempdir().unwrap();
    let result = run_init(temp_dir.path(), "test-kb");
    
    assert!(result.is_ok());
    assert!(temp_dir.path().join("kb.md").exists());
}
```

### 测试覆盖率

```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 运行覆盖率测试
cargo tarpaulin --out Html
```

---

## 文档贡献

### 文档类型

1. **用户文档**
   - README.md
   - docs/cli/*.md
   - docs/mcp-integration-guide.md

2. **架构文档**
   - docs/architecture/*.md

3. **API 文档**
   - 代码注释（`///`）

### 文档规范

- 使用清晰、简洁的语言
- 提供实际的代码示例
- 包含命令输出示例
- 使用 Markdown 格式
- 检查拼写和语法

### 文档示例

```markdown
## kb add

添加文件或目录到知识库。

### 语法

```bash
kb add <path> [alias]
```

### 参数

- `path`: 文件或目录路径
- `alias`: 可选的别名

### 示例

```bash
# 添加单个文件
kb add docs/readme.md

# 添加目录
kb add docs/

# 指定别名
kb add docs/readme.md my-readme
```

### 输出

```json
{
  "status": "success",
  "data": {
    "alias": "docs/readme",
    "path": "docs/readme.md"
  }
}
```
```

---

## Pull Request 流程

### 创建 PR

1. 确保代码通过所有测试
2. 更新相关文档
3. 填写 PR 模板
4. 链接相关 Issue

### PR 模板

```markdown
## 描述

简要描述这个 PR 的目的和改动。

## 改动类型

- [ ] Bug 修复
- [ ] 新功能
- [ ] 重构
- [ ] 文档更新
- [ ] 性能优化

## 测试

描述你如何测试这些改动。

## Checklist

- [ ] 代码遵循项目规范
- [ ] 添加了必要的测试
- [ ] 更新了相关文档
- [ ] 所有测试通过
- [ ] Commit 消息遵循规范

## 相关 Issue

Closes #123
```

### Code Review

- 所有 PR 需要至少一个 reviewer 批准
- 回应 review 意见
- 保持讨论专业和建设性

---

## 发布流程

（仅限维护者）

1. 更新版本号（`Cargo.toml`）
2. 更新 CHANGELOG.md
3. 创建 git tag
4. 构建发布包
5. 发布到 GitHub Releases

---

## 获取帮助

- **GitHub Discussions**: [提问和讨论](https://github.com/your-repo/kb-cli/discussions)
- **GitHub Issues**: [报告问题](https://github.com/your-repo/kb-cli/issues)

---

## 许可证

贡献代码即表示你同意将代码以 MIT 许可证发布。

---

感谢你的贡献！🎉
