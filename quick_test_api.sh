#!/bin/bash
# 快速测试 API 是否正常工作

HOST="127.0.0.1:8094"

echo "======================================"
echo "🧪 快速 API 测试"
echo "======================================"

echo ""
echo "1️⃣ Health Check"
curl -s http://${HOST}/health | jq '.'

echo ""
echo "2️⃣ 获取合约列表"
curl -s http://${HOST}/api/market/instruments | jq '.data | length'

echo ""
echo "3️⃣ 测试 Tick API (IF2501)"
echo "Response:"
curl -s http://${HOST}/api/market/tick/IF2501 | jq '.'

echo ""
echo "4️⃣ 测试 OrderBook API (IF2501)"
echo "Response:"
curl -s http://${HOST}/api/market/orderbook/IF2501?depth=5 | jq '.'

echo ""
echo "======================================"
echo "✅ 如果看到 JSON 响应（不是 parse error），说明 API 已修复！"
echo "======================================"
