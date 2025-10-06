# qifi-js Submodule 快速指南

## 🚀 一键集成（推荐）

```bash
cd /home/quantaxis/qaexchange-rs

# 运行自动化脚本
./scripts/setup_qifi_submodule.sh
```

脚本会自动:
1. ✅ 备份现有 websocket 目录
2. ✅ 从 git 删除现有目录
3. ✅ 添加 qifi-js 为 submodule
4. ✅ 初始化并更新 submodule
5. ✅ 验证文件完整性
6. ✅ 提示下一步操作

---

## 📋 手动操作（备选）

### 步骤 1: 删除现有 websocket 目录

```bash
cd /home/quantaxis/qaexchange-rs

# 备份（可选）
cp -r web/src/websocket /tmp/websocket-backup

# 从 git 删除
git rm -r web/src/websocket

# 提交删除
git commit -m "Remove web/src/websocket, prepare for submodule"
```

### 步骤 2: 添加 qifi-js submodule

```bash
# 添加 submodule
git submodule add https://github.com/yutiansut/qifi-js.git web/src/websocket

# 初始化
git submodule init
git submodule update
```

### 步骤 3: 提交配置

```bash
# 提交 submodule 配置
git add .gitmodules web/src/websocket
git commit -m "Add qifi-js as submodule"

# 推送
git push origin master
```

---

## ✅ 验证集成

```bash
# 1. 检查 submodule 状态
git submodule status
# 应输出: <commit> web/src/websocket (heads/master)

# 2. 检查文件
ls -la web/src/websocket/
# 应看到: index.js, WebSocketManager.js, SnapshotManager.js, etc.

# 3. 测试前端
cd web
npm run serve
# 访问: http://localhost:8080/#/websocket-test
```

---

## 🔄 日常操作

### 更新 qifi-js

```bash
# 进入 submodule
cd web/src/websocket

# 拉取最新代码
git pull origin master

# 回到主项目
cd /home/quantaxis/qaexchange-rs

# 提交更新
git add web/src/websocket
git commit -m "Update qifi-js to latest"
git push
```

### 修改 qifi-js 代码

```bash
# 进入 submodule
cd web/src/websocket

# 修改代码...

# 提交到 qifi-js 仓库
git add .
git commit -m "Fix: some bug"
git push origin master

# 回到主项目
cd /home/quantaxis/qaexchange-rs

# 更新 submodule 引用
git add web/src/websocket
git commit -m "Update qifi-js submodule"
git push
```

---

## 👥 团队协作

### 其他开发者克隆项目

```bash
# 方法 1: 克隆时包含 submodule
git clone --recurse-submodules https://github.com/your-org/qaexchange-rs.git

# 方法 2: 克隆后初始化 submodule
git clone https://github.com/your-org/qaexchange-rs.git
cd qaexchange-rs
git submodule init
git submodule update
```

### 更新 submodule

```bash
# 其他开发者拉取包含 submodule 更新的代码
git pull

# 更新 submodule
git submodule update
```

---

## 🐛 常见问题

### Q: submodule 目录是空的

**解决**:
```bash
git submodule init
git submodule update
```

### Q: 如何切换到 submodule 的特定版本

**解决**:
```bash
cd web/src/websocket
git checkout v1.0.0  # 或 commit hash
cd /home/quantaxis/qaexchange-rs
git add web/src/websocket
git commit -m "Pin qifi-js to v1.0.0"
```

### Q: 如何移除 submodule

**解决**:
```bash
# 1. 移除 submodule
git submodule deinit -f web/src/websocket
git rm -f web/src/websocket

# 2. 删除缓存
rm -rf .git/modules/web/src/websocket

# 3. 提交
git commit -m "Remove qifi-js submodule"
```

### Q: CI/CD 如何处理 submodule

**GitHub Actions**:
```yaml
- uses: actions/checkout@v3
  with:
    submodules: recursive
```

**GitLab CI**:
```yaml
variables:
  GIT_SUBMODULE_STRATEGY: recursive
```

---

## 📖 更多信息

- 详细集成指南: [web/QIFI_JS_INTEGRATION.md](web/QIFI_JS_INTEGRATION.md)
- qifi-js 仓库: https://github.com/yutiansut/qifi-js
- WebSocket 测试指南: [web/QUICK_TEST.md](web/QUICK_TEST.md)

---

## 🎯 推荐工作流

1. **开发阶段**: 使用 Git Submodule（本指南）
2. **生产部署**: 考虑发布为 npm 包（见 QIFI_JS_INTEGRATION.md）
3. **版本管理**: 在 qifi-js 使用 git tags 管理版本

这样既保证开发灵活性，又保证生产稳定性。
