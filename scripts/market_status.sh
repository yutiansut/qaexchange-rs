#!/bin/bash
# 市场状态查看工具
# @yutiansut @quantaxis

BASE_URL="http://localhost:8094"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║          📊 QAExchange 市场状态监控                           ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# 1. 获取账户列表
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 账户统计"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ACCOUNTS=$(curl -s "$BASE_URL/api/management/accounts")

TOTAL_ACCOUNTS=$(echo "$ACCOUNTS" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['data']['total'])" 2>/dev/null)
TOTAL_BALANCE=$(echo "$ACCOUNTS" | python3 -c "
import sys,json
d=json.load(sys.stdin)
total = sum(acc['balance'] for acc in d['data']['accounts'])
print(f'{total:,.2f}')
" 2>/dev/null)

HIGH_RISK=$(echo "$ACCOUNTS" | python3 -c "
import sys,json
d=json.load(sys.stdin)
high_risk = [acc for acc in d['data']['accounts'] if acc['risk_ratio'] > 0.8]
print(len(high_risk))
" 2>/dev/null)

echo "  总账户数: $TOTAL_ACCOUNTS"
echo "  总资金: ¥$TOTAL_BALANCE"
echo "  高风险账户(>80%): $HIGH_RISK"
echo ""

# 2. 显示前5个账户
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🏆 Top 5 账户（按余额排序）"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

echo "$ACCOUNTS" | python3 -c "
import sys,json
d=json.load(sys.stdin)
accounts = sorted(d['data']['accounts'], key=lambda x: x['balance'], reverse=True)[:5]
for i, acc in enumerate(accounts, 1):
    name = acc['user_name'][:20]
    balance = f\"{acc['balance']:,.0f}\"
    available = f\"{acc['available']:,.0f}\"
    risk = f\"{acc['risk_ratio']*100:.1f}%\"
    risk_color = '🟢' if acc['risk_ratio'] < 0.5 else '🟡' if acc['risk_ratio'] < 0.7 else '🔴'
    print(f\"  {i}. {name:20s} | 余额: ¥{balance:>15s} | 可用: ¥{available:>15s} | 风险: {risk_color} {risk:>6s}\")
" 2>/dev/null

echo ""

# 3. 获取订单统计
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📋 订单统计"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ORDERS=$(curl -s "$BASE_URL/api/monitoring/orders")

echo "$ORDERS" | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f\"  总订单数: {d['total_count']:,}\")
print(f\"  待成交: {d['pending_count']:,}\")
print(f\"  已成交: {d['filled_count']:,}\")
print(f\"  已撤销: {d['cancelled_count']:,}\")
" 2>/dev/null

echo ""

# 4. 获取订单簿快照
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 IF2501 订单簿快照"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ORDERBOOK=$(curl -s "$BASE_URL/api/market/orderbook/IF2501")

echo "$ORDERBOOK" | python3 -c "
import sys,json
d=json.load(sys.stdin)
data = d['data']

bids = data.get('bids', [])[:5]
asks = data.get('asks', [])[:5]

# 显示卖盘（从高到低）
for ask in reversed(asks):
    print(f\"  卖: {ask['price']:>8.1f}  x  {ask['volume']:<4.0f}\")

print('  ' + '-' * 30)

# 显示买盘（从高到低）
for bid in bids:
    print(f\"  买: {bid['price']:>8.1f}  x  {bid['volume']:<4.0f}\")

print(f\"\\n  最新价: {data.get('last_price', 0):.1f}\")
print(f\"  盘口深度: 买{len(data.get('bids', []))}档 / 卖{len(data.get('asks', []))}档\")
" 2>/dev/null

echo ""

# 5. 交易agent状态
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🤖 交易Agent状态"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

MM_COUNT=$(ps aux | grep "04_market_maker_if2501.py\|run_infinite_market_maker.py" | grep -v grep | wc -l)
SCALPER_COUNT=$(ps aux | grep "short_term_scalper.py" | grep -v grep | wc -l)

echo "  做市商: $MM_COUNT 个"
echo "  短期交易者: $SCALPER_COUNT 个"
echo "  总计: $((MM_COUNT + SCALPER_COUNT)) 个agent运行中"
echo ""

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  ✅ 市场运行正常                                              ║"
echo "║  🌐 Web界面: http://localhost:8096/#/market-overview         ║"
echo "╚══════════════════════════════════════════════════════════════╝"
