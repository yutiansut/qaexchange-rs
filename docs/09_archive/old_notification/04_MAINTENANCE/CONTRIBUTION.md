# 文档贡献指南

> 如何维护和贡献通知系统文档

**版本**: v1.1.0
**最后更新**: 2025-10-03

---

## 📚 文档结构

### 目录组织

```
docs/notification/
├── README.md                    # 📚 文档中心索引
├── CHANGELOG.md                 # 📝 版本变更日志
├── ITERATIONS.md                # 🔄 迭代开发历史
│
├── 01_DESIGN/                   # 设计文档
│   ├── SYSTEM_DESIGN.md         # 系统设计
│   ├── IMPLEMENTATION_PLAN.md   # 实施计划
│   └── RKYV_EVALUATION.md       # rkyv 评估
│
├── 02_IMPLEMENTATION/           # 实现文档
│   ├── FINAL_SUMMARY.md         # 实现总结
│   ├── API_REFERENCE.md         # API 参考
│   └── INTEGRATION_GUIDE.md     # 集成指南
│
├── 03_TESTING/                  # 测试文档
│   ├── TESTING.md               # 测试流程
│   └── BENCHMARK.md             # 性能基准
│
└── 04_MAINTENANCE/              # 维护文档
    ├── TROUBLESHOOTING.md       # 故障排查
    └── CONTRIBUTION.md          # 本文档
```

---

## ✏️ 文档更新流程

### 1. 修改文档

```bash
# 1. 创建分支
git checkout -b docs/update-notification

# 2. 修改文档
vim docs/notification/02_IMPLEMENTATION/API_REFERENCE.md

# 3. 提交变更
git add docs/notification/
git commit -m "docs: update notification API reference"
```

### 2. 更新 README 导航

如果添加了新文档，更新 `README.md`：

```markdown
## 📖 文档结构

...新增部分...
├── 02_IMPLEMENTATION/
│   ├── API_REFERENCE.md
│   ├── INTEGRATION_GUIDE.md
│   └── NEW_DOCUMENT.md          # ✅ 添加这里
```

### 3. 记录 CHANGELOG

在 `CHANGELOG.md` 中添加条目：

```markdown
## [Unreleased]

### Added
- 📝 新增 XXX 文档

### Changed
- 📝 更新 API 参考文档，添加 rkyv 序列化方法
```

### 4. 更新迭代历史

如果是重要变更，在 `ITERATIONS.md` 中记录：

```markdown
## Iteration N: 标题

**时间**: 2025-XX-XX
**目标**: 描述

### 📝 完成的工作
- 更新文档 XXX
```

### 5. 提交 Pull Request

```bash
git push origin docs/update-notification

# 在 GitHub 创建 PR，标题示例：
# docs: update notification API reference for rkyv support
```

---

## 📝 文档编写规范

### Markdown 格式

#### 标题层级

```markdown
# H1 - 文档标题（每个文件只有一个）

## H2 - 主要章节

### H3 - 子章节

#### H4 - 细节说明
```

#### 代码块

**指定语言**：
````markdown
```rust
// Rust 代码
```

```bash
# Shell 命令
```

```json
// JSON 示例
```
````

**代码高亮**：
```markdown
- `variable` - 行内代码
- **重点**：使用粗体
- *斜体*：强调
```

#### 表格

```markdown
| 列1 | 列2 | 列3 |
|-----|-----|-----|
| 数据1 | 数据2 | 数据3 |
```

#### 链接

```markdown
# 相对链接（推荐）
[API 参考](../02_IMPLEMENTATION/API_REFERENCE.md)

# 绝对链接（外部资源）
[Rust 官方文档](https://doc.rust-lang.org/)

# 锚点链接
[跳转到章节](#文档编写规范)
```

### 内容规范

#### 1. API 文档

**结构**：
```markdown
### 方法名称

简短描述

```rust
pub fn method_name(...) -> Result<T, E>
```

**参数**:
- `param1` - 参数说明
- `param2` - 参数说明

**返回值**:
- `Ok(T)` - 成功情况
- `Err(E)` - 失败情况

**示例**:
```rust
let result = obj.method_name(arg1, arg2)?;
```

**注意事项**:
- ⚠️ 重要提示
- ✅ 最佳实践
```

#### 2. 集成指南

**结构**：
```markdown
## 模块名称集成

### 1. 修改结构体

```rust
// 代码示例
```

### 2. 实现方法

**场景**: 描述使用场景

```rust
// 实现代码
```

### 3. 注意事项

- ✅ 推荐做法
- ❌ 避免的错误
```

#### 3. 故障排查

**结构**：
```markdown
### 问题标题

#### 症状
描述现象

#### 诊断步骤
1. 检查 A
2. 检查 B

#### 解决方案
```rust
// 解决代码
```
```

### 文档头部

**每个文档顶部添加**：

```markdown
# 文档标题

> 一句话描述

**版本**: v1.x.x
**最后更新**: YYYY-MM-DD

---
```

### 文档尾部

**每个文档底部添加**：

```markdown
---

## 相关链接

- [文档 A](link)
- [文档 B](link)

---

*最后更新: YYYY-MM-DD*
*维护者: @yutiansut*
```

---

## 🔄 版本管理

### 版本号规则

遵循 [语义化版本](https://semver.org/lang/zh-CN/)：

- **MAJOR.MINOR.PATCH** (如 v1.1.0)
- **MAJOR**: 不兼容的 API 变更
- **MINOR**: 向后兼容的功能新增
- **PATCH**: 向后兼容的问题修复

### CHANGELOG 格式

```markdown
## [版本号] - YYYY-MM-DD

### Added
- 新增功能

### Changed
- 变更内容

### Deprecated
- 即将废弃的功能

### Removed
- 已移除的功能

### Fixed
- 问题修复

### Security
- 安全修复

### Performance
- 性能改进
```

### 迭代历史记录

每个迭代包含：

```markdown
## Iteration N: 标题

**时间**: YYYY-MM-DD
**目标**: 简短描述

### 📝 完成的工作
- 列表项

### ⚠️ 遇到的问题
- 问题描述
- 解决方案

### 🎯 成果
- 总结
```

---

## 🧪 文档测试

### 代码示例验证

**确保所有代码示例可编译**：

```bash
# 提取文档中的代码示例
cargo test --doc

# 手动测试
rustc --test docs_example.rs
./docs_example
```

### 链接检查

**检查所有链接有效**：

```bash
# 使用 markdown-link-check
npm install -g markdown-link-check
find docs/notification -name "*.md" -exec markdown-link-check {} \;
```

### 拼写检查

```bash
# 使用 aspell
aspell check docs/notification/README.md
```

---

## 📊 文档质量标准

### 完整性检查

- [ ] 所有公共 API 都有文档
- [ ] 所有文档都有示例代码
- [ ] 所有示例代码都可编译
- [ ] 所有链接都有效
- [ ] 无拼写错误

### 可读性检查

- [ ] 章节结构清晰
- [ ] 代码块有语法高亮
- [ ] 使用适当的图标和 emoji
- [ ] 表格对齐整齐
- [ ] 无过长的段落（建议 < 5 行）

### 准确性检查

- [ ] 代码与实际实现一致
- [ ] 版本号正确
- [ ] 更新日期准确
- [ ] 性能数据最新

---

## 🎨 样式指南

### 使用 Emoji

适度使用 emoji 增强可读性：

```markdown
- ✅ 正确做法
- ❌ 错误做法
- ⚠️ 警告
- 📝 文档
- 🔧 工具
- 🚀 性能
- 🎯 目标
- 📊 数据
```

### 代码注释

```rust
// ✅ 好的注释
pub fn good_example() {
    // 解释为什么这么做
}

// ❌ 差的注释
pub fn bad_example() {
    // 这个函数做了某事（废话）
}
```

### 终端输出

使用代码块展示终端输出：

````markdown
```bash
$ cargo test

running 14 tests
test notification::broker::tests::test_broker_creation ... ok

test result: ok. 14 passed; 0 failed
```
````

---

## 🔍 文档审查清单

### PR 提交前

- [ ] 运行拼写检查
- [ ] 验证所有代码示例
- [ ] 检查所有链接
- [ ] 更新 README 导航
- [ ] 记录 CHANGELOG
- [ ] 更新 ITERATIONS（如适用）
- [ ] 更新版本号和日期

### 代码审查

- [ ] 文档与代码同步
- [ ] API 变更已记录
- [ ] 破坏性变更已标注
- [ ] 迁移指南已提供（如适用）

---

## 📚 参考资源

### Markdown 工具

- [Typora](https://typora.io/) - Markdown 编辑器
- [MarkText](https://github.com/marktext/marktext) - 开源 Markdown 编辑器
- [mdBook](https://rust-lang.github.io/mdBook/) - Rust 文档书籍工具

### 文档风格

- [Rust API 指南](https://rust-lang.github.io/api-guidelines/documentation.html)
- [Microsoft 写作风格指南](https://docs.microsoft.com/zh-cn/style-guide/welcome/)
- [Google 开发者文档风格指南](https://developers.google.com/style)

### 工具

- [markdown-link-check](https://github.com/tcort/markdown-link-check) - 链接检查
- [aspell](http://aspell.net/) - 拼写检查
- [prettier](https://prettier.io/) - Markdown 格式化

---

## 🤝 贡献流程

### 小改动（拼写、链接等）

1. 直接修改并提交 PR
2. 标题格式：`docs: fix typo in API reference`

### 大改动（新章节、重构）

1. 先提 Issue 讨论
2. 达成共识后再开始编写
3. 分步骤提交 PR

### 文档翻译

1. 在 `docs/notification/i18n/` 创建语言目录
2. 翻译文档并保持结构一致
3. 在 README 中添加语言切换链接

---

## 💬 获取帮助

- 📖 查看现有文档示例
- 💬 在 Issue 中提问
- 📧 联系文档维护者

---

## 🏆 贡献者

感谢所有为通知系统文档做出贡献的人！

<!-- 这里可以添加贡献者列表 -->

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
