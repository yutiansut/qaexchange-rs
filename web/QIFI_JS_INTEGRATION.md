# qifi-js 集成方案

将独立的 qifi-js 仓库集成到 qaexchange-rs 主项目中。

## 🎯 目标

- ✅ `qifi-js` 作为独立仓库维护: https://github.com/yutiansut/qifi-js
- ✅ `qaexchange-rs` 主项目引用 `qifi-js`
- ✅ 保持代码同步和版本管理

---

## 📋 方案对比

| 方案 | 优点 | 缺点 | 适用场景 |
|------|------|------|----------|
| **Git Submodule** | 简单快速，实时同步 | 需要 git 操作，克隆复杂 | 开发阶段 |
| **NPM Package** | 标准化，版本管理 | 需要发布流程 | 生产环境 |

**推荐**: 两者结合使用
- 开发阶段: Git Submodule (实时同步)
- 生产环境: NPM Package (稳定版本)

---

## 方案 1: Git Submodule（推荐开发使用）

### 优点
- ✅ 实时同步 qifi-js 仓库
- ✅ 不需要发布流程
- ✅ 适合快速迭代开发

### 步骤

#### 1.1 在主项目中删除现有 websocket 目录

```bash
cd /home/quantaxis/qaexchange-rs

# 从 git 中删除 web/src/websocket 目录（保留文件）
git rm -r --cached web/src/websocket

# 删除实际文件
rm -rf web/src/websocket

# 提交删除
git commit -m "Remove web/src/websocket, will use git submodule"
```

#### 1.2 添加 qifi-js 作为 submodule

```bash
cd /home/quantaxis/qaexchange-rs

# 添加 submodule
git submodule add https://github.com/yutiansut/qifi-js.git web/src/websocket

# 初始化 submodule
git submodule init

# 更新 submodule
git submodule update

# 提交 submodule 配置
git add .gitmodules web/src/websocket
git commit -m "Add qifi-js as git submodule at web/src/websocket"
```

#### 1.3 验证集成

```bash
# 检查 submodule 状态
git submodule status

# 应输出:
# <commit-hash> web/src/websocket (heads/master)

# 检查文件是否存在
ls -la web/src/websocket/

# 应看到 qifi-js 的文件
```

#### 1.4 其他开发者克隆项目

其他开发者克隆主项目时需要:

```bash
# 方法 1: 克隆时包含 submodule
git clone --recurse-submodules https://github.com/your-org/qaexchange-rs.git

# 方法 2: 克隆后初始化 submodule
git clone https://github.com/your-org/qaexchange-rs.git
cd qaexchange-rs
git submodule init
git submodule update
```

#### 1.5 更新 qifi-js

当 qifi-js 有更新时:

```bash
cd /home/quantaxis/qaexchange-rs/web/src/websocket

# 拉取最新代码
git pull origin master

# 回到主项目
cd /home/quantaxis/qaexchange-rs

# 提交 submodule 更新
git add web/src/websocket
git commit -m "Update qifi-js submodule to latest version"
git push
```

#### 1.6 修改 qifi-js 代码

如果需要在主项目中修改 qifi-js:

```bash
cd /home/quantaxis/qaexchange-rs/web/src/websocket

# 修改代码...

# 提交到 qifi-js 仓库
git add .
git commit -m "Fix: some bug"
git push origin master

# 回到主项目，更新 submodule 引用
cd /home/quantaxis/qaexchange-rs
git add web/src/websocket
git commit -m "Update qifi-js submodule"
git push
```

---

## 方案 2: NPM Package（推荐生产使用）

### 优点
- ✅ 标准的前端包管理
- ✅ 版本控制清晰
- ✅ 可发布到 npm registry
- ✅ 支持语义化版本

### 步骤

#### 2.1 准备 qifi-js 为 npm 包

在 qifi-js 仓库中创建 `package.json`:

```bash
cd /path/to/qifi-js

# 创建 package.json
cat > package.json << 'EOF'
{
  "name": "@yutiansut/qifi-js",
  "version": "1.0.0",
  "description": "QIFI/DIFF protocol WebSocket client for JavaScript",
  "main": "index.js",
  "module": "index.js",
  "files": [
    "index.js",
    "WebSocketManager.js",
    "SnapshotManager.js",
    "DiffProtocol.js",
    "utils/",
    "README.md"
  ],
  "keywords": [
    "qifi",
    "diff",
    "websocket",
    "trading",
    "quantaxis"
  ],
  "author": "yutiansut",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/yutiansut/qifi-js.git"
  },
  "bugs": {
    "url": "https://github.com/yutiansut/qifi-js/issues"
  },
  "homepage": "https://github.com/yutiansut/qifi-js#readme"
}
EOF

# 提交
git add package.json
git commit -m "Add package.json for npm publishing"
git push
```

#### 2.2 发布到 npm（可选）

```bash
# 登录 npm
npm login

# 发布
npm publish --access public

# 或者发布到 GitHub Packages
npm publish --registry=https://npm.pkg.github.com
```

#### 2.3 在主项目中安装

```bash
cd /home/quantaxis/qaexchange-rs/web

# 方法 1: 从 npm 安装
npm install @yutiansut/qifi-js

# 方法 2: 从 GitHub 直接安装（不需要发布到 npm）
npm install git+https://github.com/yutiansut/qifi-js.git

# 方法 3: 从本地安装（开发时）
npm install file:../../qifi-js
```

#### 2.4 更新引用路径

在主项目中修改所有引用:

**修改前**:
```javascript
import WebSocketManager from '@/websocket'
```

**修改后**:
```javascript
import WebSocketManager from '@yutiansut/qifi-js'
```

或者使用 webpack alias:

```javascript
// vue.config.js
module.exports = {
  configureWebpack: {
    resolve: {
      alias: {
        '@/websocket': '@yutiansut/qifi-js'
      }
    }
  }
}
```

#### 2.5 更新版本

当 qifi-js 有更新时:

```bash
cd /home/quantaxis/qaexchange-rs/web

# 更新到最新版本
npm update @yutiansut/qifi-js

# 或指定版本
npm install @yutiansut/qifi-js@1.1.0
```

---

## 方案 3: 混合方案（推荐）

结合 Git Submodule 和 NPM 的优点。

### 工作流程

#### 开发阶段

使用 Git Submodule 进行快速开发:

```bash
# 1. 添加 submodule
git submodule add https://github.com/yutiansut/qifi-js.git web/src/websocket

# 2. 直接在 submodule 中开发和测试
cd web/src/websocket
# 修改代码...
git commit -am "Add new feature"
git push

# 3. 更新主项目引用
cd /home/quantaxis/qaexchange-rs
git add web/src/websocket
git commit -m "Update qifi-js"
```

#### 生产部署

使用 NPM Package 进行稳定部署:

```bash
# 1. 在 qifi-js 发布新版本
cd /path/to/qifi-js
npm version patch  # 或 minor, major
git push --tags
npm publish

# 2. 在主项目中删除 submodule，改用 npm 包
cd /home/quantaxis/qaexchange-rs
git submodule deinit -f web/src/websocket
git rm -f web/src/websocket
rm -rf .git/modules/web/src/websocket

# 3. 安装 npm 包
cd web
npm install @yutiansut/qifi-js@latest

# 4. 配置 webpack alias（保持引用路径不变）
# 在 vue.config.js 中添加 alias 配置
```

---

## 推荐的目录结构

### Git Submodule 方案

```
qaexchange-rs/
├── .gitmodules                  # submodule 配置
├── web/
│   ├── src/
│   │   ├── websocket/          # git submodule -> qifi-js
│   │   ├── store/
│   │   │   └── modules/
│   │   │       └── websocket.js  # 引用 @/websocket
│   │   └── views/
│   │       └── WebSocketTest.vue
│   └── package.json
└── README.md
```

### NPM Package 方案

```
qaexchange-rs/
├── web/
│   ├── node_modules/
│   │   └── @yutiansut/
│   │       └── qifi-js/        # npm 包
│   ├── src/
│   │   ├── store/
│   │   │   └── modules/
│   │   │       └── websocket.js  # 引用 @yutiansut/qifi-js
│   │   └── views/
│   │       └── WebSocketTest.vue
│   ├── package.json             # 依赖 @yutiansut/qifi-js
│   └── vue.config.js            # 配置 alias
└── README.md
```

---

## 快速决策指南

### 选择 Git Submodule 如果:

- ✅ 你是主要开发者，需要频繁修改 qifi-js
- ✅ qifi-js 还在快速迭代，版本变化频繁
- ✅ 团队都熟悉 git submodule 操作
- ✅ 希望 qifi-js 和主项目保持实时同步

### 选择 NPM Package 如果:

- ✅ qifi-js 已经稳定，不经常修改
- ✅ 希望使用语义化版本管理
- ✅ 团队成员不熟悉 git submodule
- ✅ 希望 qifi-js 可以被其他项目复用
- ✅ 需要在 CI/CD 中使用

---

## 具体操作步骤（推荐方案）

### 当前状态

你已经:
1. ✅ 创建了独立仓库: https://github.com/yutiansut/qifi-js
2. ✅ 在主项目中有 `web/src/websocket` 目录
3. ⚠️ 需要决定如何集成

### 推荐操作（Git Submodule）

```bash
# Step 1: 备份当前 websocket 目录
cd /home/quantaxis/qaexchange-rs
cp -r web/src/websocket /tmp/websocket-backup

# Step 2: 确认 qifi-js 仓库包含所有文件
cd /tmp/websocket-backup
ls -la
# 确认所有文件都已推送到 https://github.com/yutiansut/qifi-js

# Step 3: 从主项目删除 websocket 目录
cd /home/quantaxis/qaexchange-rs
git rm -r web/src/websocket
git commit -m "Remove web/src/websocket, prepare for submodule"

# Step 4: 添加 qifi-js 作为 submodule
git submodule add https://github.com/yutiansut/qifi-js.git web/src/websocket

# Step 5: 提交 submodule 配置
git add .gitmodules web/src/websocket
git commit -m "Add qifi-js as submodule at web/src/websocket"

# Step 6: 验证
git submodule status
ls -la web/src/websocket/

# Step 7: 推送到远程
git push origin master
```

### 验证集成成功

```bash
# 1. 检查 .gitmodules 文件
cat .gitmodules

# 应输出:
# [submodule "web/src/websocket"]
#   path = web/src/websocket
#   url = https://github.com/yutiansut/qifi-js.git

# 2. 检查文件结构
ls -la web/src/websocket/
# 应看到所有 qifi-js 文件

# 3. 测试前端应用
cd web
npm run serve
# 访问 http://localhost:8080/#/websocket-test
```

---

## 常见问题

### Q1: git submodule 克隆后是空的？

```bash
# 初始化并更新 submodule
git submodule init
git submodule update
```

### Q2: 如何移除 submodule？

```bash
# 1. 移除 submodule
git submodule deinit -f web/src/websocket
git rm -f web/src/websocket

# 2. 删除 .git/modules 中的缓存
rm -rf .git/modules/web/src/websocket

# 3. 提交
git commit -m "Remove qifi-js submodule"
```

### Q3: submodule 指向错误的 commit？

```bash
# 进入 submodule
cd web/src/websocket

# 切换到正确的分支/commit
git checkout master
git pull origin master

# 回到主项目
cd /home/quantaxis/qaexchange-rs

# 更新 submodule 引用
git add web/src/websocket
git commit -m "Update qifi-js to latest commit"
```

### Q4: 如何在 CI/CD 中使用？

#### GitHub Actions

```yaml
# .github/workflows/deploy.yml
name: Deploy

on:
  push:
    branches: [ master ]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code with submodules
        uses: actions/checkout@v3
        with:
          submodules: recursive  # 递归克隆 submodule

      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '16'

      - name: Install dependencies
        run: |
          cd web
          npm install

      - name: Build
        run: |
          cd web
          npm run build
```

---

## 维护指南

### 更新 qifi-js

```bash
# 方法 1: 在主项目中更新
cd /home/quantaxis/qaexchange-rs/web/src/websocket
git pull origin master
cd /home/quantaxis/qaexchange-rs
git add web/src/websocket
git commit -m "Update qifi-js to <commit-hash>"

# 方法 2: 直接在 qifi-js 仓库开发
cd /path/to/qifi-js
# 修改代码...
git commit -am "Add feature"
git push

# 然后在主项目中拉取更新
cd /home/quantaxis/qaexchange-rs/web/src/websocket
git pull
```

### 版本管理

在 qifi-js 仓库使用 git tags:

```bash
cd /path/to/qifi-js

# 打 tag
git tag -a v1.0.0 -m "Release version 1.0.0"
git push origin v1.0.0

# 在主项目中指向特定版本
cd /home/quantaxis/qaexchange-rs/web/src/websocket
git checkout v1.0.0
cd /home/quantaxis/qaexchange-rs
git add web/src/websocket
git commit -m "Pin qifi-js to v1.0.0"
```

---

## 总结

**立即执行（推荐）**:
1. 使用 **Git Submodule** 方案
2. 执行上面的 "推荐操作" 步骤
3. 测试集成是否成功

**未来规划**:
1. qifi-js 稳定后，发布为 npm 包
2. 在生产环境使用 npm 包
3. 开发环境继续使用 submodule

这样既保证了开发灵活性，又保证了生产稳定性。
