#!/bin/bash
# 市场数据 API 调试脚本

HOST="127.0.0.1:8094"
INSTRUMENT="IF2501"

echo "======================================"
echo "🔍 市场数据 API 调试测试"
echo "======================================"

echo ""
echo "步骤1: 注册用户"
curl -s -X POST http://${HOST}/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "test_market_user",
    "user_name": "Market Test User",
    "password": "test123"
  }' | jq '.'

echo ""
echo "步骤2: 开户"
curl -s -X POST http://${HOST}/api/account/open \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "test_market_user",
    "user_name": "Market Test User",
    "init_cash": 1000000,
    "account_type": "individual",
    "password": "test123"
  }' | jq '.'

echo ""
echo "步骤3: 获取账户信息获取 account_id"
ACCOUNT_RESPONSE=$(curl -s http://${HOST}/api/user/test_market_user/accounts)
echo $ACCOUNT_RESPONSE | jq '.'
ACCOUNT_ID=$(echo $ACCOUNT_RESPONSE | jq -r '.data[0].account_id')
echo "✅ 获取到 account_id: $ACCOUNT_ID"

echo ""
echo "步骤4: 提交买单（触发市场数据写入）"
curl -s -X POST http://${HOST}/api/order/submit \
  -H 'Content-Type: application/json' \
  -d "{
    \"user_id\": \"test_market_user\",
    \"account_id\": \"$ACCOUNT_ID\",
    \"instrument_id\": \"${INSTRUMENT}\",
    \"direction\": \"BUY\",
    \"offset\": \"OPEN\",
    \"volume\": 1,
    \"price\": 3800,
    \"order_type\": \"LIMIT\"
  }" | jq '.'

echo ""
echo "步骤5: 提交卖单（触发成交和市场数据写入）"
curl -s -X POST http://${HOST}/api/order/submit \
  -H 'Content-Type: application/json' \
  -d "{
    \"user_id\": \"test_market_user\",
    \"account_id\": \"$ACCOUNT_ID\",
    \"instrument_id\": \"${INSTRUMENT}\",
    \"direction\": \"SELL\",
    \"offset\": \"OPEN\",
    \"volume\": 1,
    \"price\": 3800,
    \"order_type\": \"LIMIT\"
  }" | jq '.'

echo ""
echo "等待3秒..."
sleep 3

echo ""
echo "======================================"
echo "📊 测试市场数据 API"
echo "======================================"

echo ""
echo "1️⃣ 测试 Tick API"
echo "GET /api/market/tick/${INSTRUMENT}"
curl -s http://${HOST}/api/market/tick/${INSTRUMENT} | jq '.'

echo ""
echo "2️⃣ 测试 OrderBook API"
echo "GET /api/market/orderbook/${INSTRUMENT}?depth=5"
curl -s http://${HOST}/api/market/orderbook/${INSTRUMENT}?depth=5 | jq '.'

echo ""
echo "3️⃣ 测试 Instruments API"
echo "GET /api/market/instruments"
curl -s http://${HOST}/api/market/instruments | jq '.'

echo ""
echo "======================================"
echo "📂 检查存储文件"
echo "======================================"
ls -lh output/qaexchange/storage/market_data/ 2>/dev/null || echo "市场数据目录不存在"

echo ""
echo "======================================"
echo "✅ 测试完成"
echo "======================================"
echo ""
echo "🔍 请检查服务器日志，查找以下关键字："
echo "   - [HTTP API]  - HTTP 请求日志"
echo "   - [MarketData] - 市场数据服务日志"
echo "   - [L1 Cache] / [L2 Storage] / [L3 Realtime] - 三层缓存查询日志"
echo "   - [Storage] - WAL 存储查询日志"
echo ""
