# mdbook 配置说明

本文档说明如何使用 mdbook 构建和部署 QAExchange 文档到 GitHub Pages。

## 📁 文件结构

```
qaexchange-rs/
├── book.toml              # mdbook 配置文件
├── docs/                  # 文档源文件目录（markdown）
│   ├── SUMMARY.md        # 文档目录结构（mdbook 必需）
│   ├── README.md         # 文档首页
│   └── ...               # 其他文档
├── book/                  # 构建输出目录（git ignored）
└── .github/
    └── workflows/
        └── mdbook.yml    # GitHub Actions 自动构建和部署
```

## 🔧 配置文件

### book.toml

mdbook 主配置文件，关键配置：

```toml
[book]
title = "QAExchange Documentation"
language = "zh"
src = "docs"  # 使用 docs/ 作为源目录（而非默认的 src/）

[build]
build-dir = "book"  # 输出到 book/ 目录

[output.html]
git-repository-url = "https://github.com/QUANTAXIS/qaexchange-rs"
site-url = "/qaexchange-rs/"
```

### docs/SUMMARY.md

mdbook 目录结构定义文件，必须存在。格式：

```markdown
# Summary

[介绍](README.md)

# 章节标题
- [文档名称](path/to/file.md)
  - [子文档](path/to/subfile.md)
```

**重要**: 每个文件路径只能出现一次，否则会报错 "Duplicate file in SUMMARY.md"。

## 🚀 本地构建

### 安装 mdbook

```bash
cargo install mdbook --version 0.4.36
```

### 构建文档

```bash
# 构建静态 HTML
mdbook build

# 本地预览（启动 HTTP 服务器）
mdbook serve --open

# 访问: http://localhost:3000
```

### 清理构建产物

```bash
mdbook clean
# 或
rm -rf book/
```

## 🌐 GitHub Pages 部署

### 自动部署流程

1. **触发条件**: 推送到 `master` 分支
2. **构建**: GitHub Actions 运行 `mdbook build`
3. **部署**: 自动部署 `book/` 目录到 GitHub Pages
4. **访问**: https://quantaxis.github.io/qaexchange-rs/

### 手动触发

在 GitHub 仓库页面：
1. 进入 "Actions" 标签
2. 选择 "Deploy mdBook site to Pages"
3. 点击 "Run workflow"

### GitHub Pages 设置

确保仓库设置正确：

1. Settings → Pages
2. Source: GitHub Actions
3. Build and deployment: GitHub Actions

## 📝 添加新文档

### 步骤

1. **创建 markdown 文件**:
   ```bash
   # 例如添加新的 API 文档
   vim docs/04_api/new_api.md
   ```

2. **更新 SUMMARY.md**:
   ```markdown
   # 在 docs/SUMMARY.md 中添加

   ## API 文档
   - [新 API](04_api/new_api.md)  # 添加这一行
   ```

3. **本地验证**:
   ```bash
   mdbook build
   # 检查是否有错误
   ```

4. **提交和推送**:
   ```bash
   git add docs/04_api/new_api.md docs/SUMMARY.md
   git commit -m "docs: add new API documentation"
   git push origin master
   ```

5. **自动部署**: GitHub Actions 会自动构建和部署

## 🐛 常见问题

### Q1: 构建失败 "Couldn't open SUMMARY.md"

**原因**: 缺少 `docs/SUMMARY.md` 文件

**解决**: 确保 `docs/SUMMARY.md` 存在且格式正确

### Q2: "Duplicate file in SUMMARY.md"

**原因**: SUMMARY.md 中同一个文件路径出现多次

**解决**: 检查并移除重复的文件引用

**错误示例**:
```markdown
## 章节
- [文档A](path/a.md)
  - [文档A](path/a.md)  # ❌ 重复
```

**正确示例**:
```markdown
## 章节
- [文档A](path/a.md)  # ✅ 只出现一次
- [文档B](path/b.md)
```

### Q3: 文档更新后 GitHub Pages 未更新

**检查步骤**:
1. 确认 commit 已推送: `git log --oneline -1`
2. 查看 GitHub Actions 状态: Actions 标签
3. 等待构建完成（通常 2-3 分钟）
4. 清除浏览器缓存: Ctrl+F5

### Q4: 链接失效 (404)

**原因**: SUMMARY.md 中的路径与实际文件路径不匹配

**解决**:
```bash
# 检查文件是否存在
ls -l docs/path/to/file.md

# 确保 SUMMARY.md 中的路径与实际路径一致
```

## 📚 文档编写规范

### Markdown 格式

- 使用标准 Markdown 语法
- 代码块指定语言: \`\`\`rust
- 内部链接使用相对路径: `[链接](../other/file.md)`
- 外部链接使用完整 URL

### 中文文档注意事项

- 中英文之间加空格（可选）
- 专业术语使用英文（如 WAL, SSTable）
- 代码和数字与中文之间加空格

### 文档结构

```markdown
# 标题

简短介绍（1-2 段）

## 目录（可选）
- [章节1](#章节1)
- [章节2](#章节2)

---

## 章节1

内容...

### 子章节

内容...

## 章节2

内容...

---

**版本**: v1.0.0
**最后更新**: 2025-10-06

[返回文档中心](../README.md)
```

## 🔗 相关资源

- [mdbook 官方文档](https://rust-lang.github.io/mdBook/)
- [GitHub Pages 文档](https://docs.github.com/en/pages)
- [Markdown 语法指南](https://www.markdownguide.org/)

---

**创建日期**: 2025-10-06
**维护者**: QAExchange Team
