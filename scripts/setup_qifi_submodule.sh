#!/bin/bash

# =====================================================
# qifi-js Git Submodule 集成脚本
# =====================================================
#
# 功能: 将 qifi-js 仓库作为 git submodule 添加到主项目
# 用法: ./scripts/setup_qifi_submodule.sh
#
# =====================================================

set -e  # 遇到错误立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEBSOCKET_DIR="$PROJECT_ROOT/web/src/websocket"
QIFI_REPO="https://github.com/yutiansut/qifi-js.git"

echo -e "${GREEN}======================================${NC}"
echo -e "${GREEN}qifi-js Git Submodule 集成脚本${NC}"
echo -e "${GREEN}======================================${NC}"
echo ""

# 检查当前目录
echo -e "${YELLOW}[1/6] 检查项目目录...${NC}"
if [ ! -d "$PROJECT_ROOT/.git" ]; then
    echo -e "${RED}错误: 未在 git 仓库根目录运行此脚本${NC}"
    exit 1
fi
echo -e "${GREEN}✓ 项目目录: $PROJECT_ROOT${NC}"
echo ""

# 检查 websocket 目录是否存在
echo -e "${YELLOW}[2/6] 检查 websocket 目录...${NC}"
if [ -d "$WEBSOCKET_DIR" ]; then
    echo -e "${YELLOW}警告: web/src/websocket 目录已存在${NC}"
    echo -e "${YELLOW}需要先删除该目录才能添加 submodule${NC}"

    read -p "是否继续? 将备份并删除现有目录 (y/n): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${RED}操作取消${NC}"
        exit 1
    fi

    # 备份现有目录
    BACKUP_DIR="/tmp/websocket-backup-$(date +%Y%m%d-%H%M%S)"
    echo -e "${YELLOW}备份到: $BACKUP_DIR${NC}"
    cp -r "$WEBSOCKET_DIR" "$BACKUP_DIR"
    echo -e "${GREEN}✓ 已备份${NC}"

    # 从 git 删除
    echo -e "${YELLOW}从 git 删除 web/src/websocket...${NC}"
    cd "$PROJECT_ROOT"
    git rm -rf web/src/websocket || true

    # 删除实际文件
    rm -rf "$WEBSOCKET_DIR"

    echo -e "${GREEN}✓ 已删除现有目录${NC}"
else
    echo -e "${GREEN}✓ websocket 目录不存在，可以添加 submodule${NC}"
fi
echo ""

# 检查是否已经是 submodule
echo -e "${YELLOW}[3/6] 检查 submodule 状态...${NC}"
if git submodule status | grep -q "web/src/websocket"; then
    echo -e "${YELLOW}警告: web/src/websocket 已经是 submodule${NC}"
    read -p "是否重新初始化? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${YELLOW}移除现有 submodule...${NC}"
        git submodule deinit -f web/src/websocket
        git rm -f web/src/websocket
        rm -rf .git/modules/web/src/websocket
        echo -e "${GREEN}✓ 已移除${NC}"
    else
        echo -e "${GREEN}保持现有 submodule${NC}"
        exit 0
    fi
fi
echo ""

# 添加 submodule
echo -e "${YELLOW}[4/6] 添加 qifi-js 作为 submodule...${NC}"
cd "$PROJECT_ROOT"

if git submodule add "$QIFI_REPO" web/src/websocket; then
    echo -e "${GREEN}✓ Submodule 添加成功${NC}"
else
    echo -e "${RED}错误: 添加 submodule 失败${NC}"
    exit 1
fi
echo ""

# 初始化 submodule
echo -e "${YELLOW}[5/6] 初始化 submodule...${NC}"
git submodule init
git submodule update

if [ -f "$WEBSOCKET_DIR/index.js" ]; then
    echo -e "${GREEN}✓ Submodule 初始化成功${NC}"
else
    echo -e "${RED}错误: Submodule 初始化失败，文件不存在${NC}"
    exit 1
fi
echo ""

# 验证文件
echo -e "${YELLOW}[6/6] 验证文件完整性...${NC}"

REQUIRED_FILES=(
    "index.js"
    "WebSocketManager.js"
    "SnapshotManager.js"
    "DiffProtocol.js"
    "utils/jsonMergePatch.js"
    "utils/logger.js"
    "README.md"
)

ALL_EXIST=true
for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$WEBSOCKET_DIR/$file" ]; then
        echo -e "${GREEN}  ✓ $file${NC}"
    else
        echo -e "${RED}  ✗ $file (缺失)${NC}"
        ALL_EXIST=false
    fi
done

if [ "$ALL_EXIST" = true ]; then
    echo -e "${GREEN}✓ 所有核心文件存在${NC}"
else
    echo -e "${RED}警告: 部分文件缺失${NC}"
fi
echo ""

# 显示 submodule 状态
echo -e "${YELLOW}Submodule 状态:${NC}"
git submodule status
echo ""

# 提示下一步
echo -e "${GREEN}======================================${NC}"
echo -e "${GREEN}集成完成!${NC}"
echo -e "${GREEN}======================================${NC}"
echo ""
echo -e "${YELLOW}下一步操作:${NC}"
echo ""
echo -e "1. 提交 submodule 配置:"
echo -e "   ${GREEN}git add .gitmodules web/src/websocket${NC}"
echo -e "   ${GREEN}git commit -m 'Add qifi-js as submodule at web/src/websocket'${NC}"
echo ""
echo -e "2. 推送到远程:"
echo -e "   ${GREEN}git push origin master${NC}"
echo ""
echo -e "3. 测试前端应用:"
echo -e "   ${GREEN}cd web${NC}"
echo -e "   ${GREEN}npm run serve${NC}"
echo -e "   访问: ${GREEN}http://localhost:8080/#/websocket-test${NC}"
echo ""
echo -e "4. 其他开发者克隆项目时需要:"
echo -e "   ${GREEN}git clone --recurse-submodules <repo-url>${NC}"
echo -e "   或者:"
echo -e "   ${GREEN}git clone <repo-url>${NC}"
echo -e "   ${GREEN}git submodule init${NC}"
echo -e "   ${GREEN}git submodule update${NC}"
echo ""
echo -e "${YELLOW}更多信息请查看: web/QIFI_JS_INTEGRATION.md${NC}"
echo ""

# 询问是否立即提交
read -p "是否立即提交 submodule 配置? (y/n): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}提交 submodule 配置...${NC}"
    git add .gitmodules web/src/websocket
    git commit -m "Add qifi-js as submodule at web/src/websocket

- Repository: https://github.com/yutiansut/qifi-js
- Path: web/src/websocket
- Integration: Git Submodule
"
    echo -e "${GREEN}✓ 已提交${NC}"
    echo ""
    echo -e "${YELLOW}是否推送到远程? (y/n): ${NC}"
    read -p "" -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        git push origin master
        echo -e "${GREEN}✓ 已推送${NC}"
    fi
else
    echo -e "${YELLOW}请手动提交 submodule 配置${NC}"
fi

echo ""
echo -e "${GREEN}完成!${NC}"
