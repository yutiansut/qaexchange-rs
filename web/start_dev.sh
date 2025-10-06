#!/bin/bash

# QAExchange Web 前端开发启动脚本

echo "========================================="
echo "  QAExchange Web 前端开发环境启动"
echo "========================================="
echo ""

# 检查依赖
if [ ! -d "node_modules" ]; then
  echo "未检测到 node_modules，开始安装依赖..."
  npm install
fi

# 检查后端服务
echo "检查后端服务状态..."
if curl -s http://127.0.0.1:8094/health > /dev/null; then
  echo "✓ 后端服务已启动 (http://127.0.0.1:8094)"
else
  echo "✗ 后端服务未启动！"
  echo ""
  echo "请在另一个终端窗口启动后端："
  echo "  cd /home/quantaxis/qaexchange-rs"
  echo "  cargo run --bin qaexchange-server"
  echo ""
  read -p "后端启动后按 Enter 继续..."
fi

echo ""
echo "启动前端开发服务器..."
echo "前端地址: http://localhost:8096"
echo "后端地址: http://127.0.0.1:8094"
echo ""
echo "按 Ctrl+C 停止服务器"
echo ""

# 启动开发服务器
export NODE_OPTIONS=--openssl-legacy-provider
npm run serve
