# QAExchange Web 测试指南

## 快速启动

### 方式 1：使用启动脚本（推荐）

```bash
cd /home/quantaxis/qaexchange-rs/web
./start_dev.sh
```

### 方式 2：手动启动

**终端 1 - 启动后端**
```bash
cd /home/quantaxis/qaexchange-rs
cargo run --bin qaexchange-server
```

**终端 2 - 启动前端**
```bash
cd /home/quantaxis/qaexchange-rs/web
npm run dev
```

## 系统访问

- 前端地址: http://localhost:8096
- 后端地址: http://127.0.0.1:8094
- 健康检查: http://127.0.0.1:8094/health

## 功能测试清单

### 1. 监控仪表盘测试 ✓

**访问**: http://localhost:8096/#/dashboard

**测试点**:
- [ ] 页面加载成功
- [ ] 显示 6 个统计卡片（账户数、总权益、保证金、订单数、成交数、存储记录）
- [ ] 显示 3 个图表（账户余额分布、订单状态分布、OLAP 转换任务）
- [ ] 数据每 10 秒自动刷新

**预期数据**:
```
总账户数: 0
总订单数: 0
总成交数: 0
```

### 2. 账户管理测试 ✓

**访问**: http://localhost:8096/#/accounts

**测试流程**:

#### Step 1: 开户
1. 点击 "开户" 按钮
2. 填写表单：
   - 用户ID: `user1`
   - 用户名称: `测试用户1`
   - 初始资金: `1000000`
   - 账户类型: `个人账户`
   - 密码: `123456`
3. 点击 "确定"
4. 预期：提示 "开户成功"，列表刷新显示新账户

#### Step 2: 入金
1. 找到 `user1` 账户，点击 "存款"
2. 输入金额: `100000`
3. 点击 "确定"
4. 预期：提示 "存款成功"，账户余额增加

#### Step 3: 查询账户
1. 在查询框输入: `user1`
2. 点击 "查询"
3. 预期：只显示 `user1` 账户

### 3. 交易面板测试 ⭐ 核心功能

**访问**: http://localhost:8096/#/trade

**测试流程**:

#### Step 1: 查看合约列表
1. 点击合约选择框
2. 预期：显示 4 个合约（IF2501, IF2502, IC2501, IH2501）
3. 选择 `IF2501`

#### Step 2: 查看订单簿
1. 查看左侧订单簿区域
2. 预期：
   - 显示买卖五档
   - 有 volume bar 可视化
   - 显示最新价分隔线
   - 可点击价格快速填充到下单表单

3. 切换深度：点击 "10档" 按钮
4. 预期：订单簿显示 10 档数据

#### Step 3: 查看实时行情
1. 查看顶部行情卡片
2. 预期：
   - 显示最新价
   - 显示涨跌幅
   - 显示买一价、卖一价
   - 每 2 秒自动刷新

#### Step 4: 下单测试（限价单）
1. 选择 "买入开仓" 标签页
2. 选择 "限价单"
3. 设置价格：点击 "当前" 按钮，或手动调整
4. 设置数量：点击 "10" 快捷按钮，或手动输入
5. 查看预估金额和保证金
6. 点击 "买入开仓" 按钮
7. 预期：
   - 提示 "订单提交成功"
   - 下方 "当前委托" 表格显示新订单
   - 订单状态为 "待成交"

#### Step 5: 下单测试（市价单）
1. 选择 "卖出开仓" 标签页
2. 选择 "市价单"
3. 设置数量：5 手
4. 点击 "卖出开仓" 按钮
5. 预期：订单提交成功

#### Step 6: 撤单测试
1. 在 "当前委托" 表格找到待成交订单
2. 点击 "撤单" 按钮
3. 确认撤单
4. 预期：
   - 提示 "撤单成功"
   - 订单状态变为 "已撤销"

### 4. 订单管理测试 ✓

**访问**: http://localhost:8096/#/orders

**测试流程**:
1. 点击 "下单" 按钮提交测试订单
2. 使用用户ID筛选器，选择 `user1`
3. 点击 "查询"
4. 预期：显示 `user1` 的所有订单
5. 点击 "重置" 清除筛选

### 5. 持仓管理测试 ✓

**访问**: http://localhost:8096/#/positions

**测试流程**:
1. 选择用户 `user1`
2. 查看持仓列表（模拟数据）
3. 查看顶部统计卡片：
   - 总持仓市值
   - 浮动盈亏
   - 持仓品种数
   - 盈亏比
4. 点击某个持仓的 "平仓" 按钮
5. 设置平仓量和平仓类型
6. 点击 "确定平仓"
7. 预期：提交平仓委托成功

### 6. 成交记录测试 ✓

**访问**: http://localhost:8096/#/trades

**测试流程**:
1. 查看成交历史列表（模拟数据）
2. 查看顶部统计：今日成交、成交金额、买入笔数、卖出笔数
3. 使用合约筛选器选择 `IF2501`
4. 点击 "查询"
5. 预期：只显示 IF2501 的成交记录

## API 端点测试

### 使用 curl 测试后端 API

#### 1. 健康检查
```bash
curl http://127.0.0.1:8094/health
```
预期输出:
```json
{"status":"ok","service":"qaexchange"}
```

#### 2. 获取合约列表
```bash
curl http://127.0.0.1:8094/api/market/instruments
```

#### 3. 获取订单簿
```bash
curl http://127.0.0.1:8094/api/market/orderbook/IF2501?depth=5
```

#### 4. 获取 Tick 行情
```bash
curl http://127.0.0.1:8094/api/market/tick/IF2501
```

#### 5. 获取市场统计（管理员）
```bash
curl http://127.0.0.1:8094/api/admin/market/order-stats
```

#### 6. 开户
```bash
curl -X POST http://127.0.0.1:8094/api/account/open \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "test_user",
    "user_name": "测试用户",
    "init_cash": 1000000.0,
    "account_type": "individual",
    "password": "123456"
  }'
```

#### 7. 提交订单
```bash
curl -X POST http://127.0.0.1:8094/api/order/submit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "test_user",
    "instrument_id": "IF2501",
    "direction": "BUY",
    "offset": "OPEN",
    "price": 3800.0,
    "volume": 1,
    "order_type": "LIMIT"
  }'
```

## 性能测试

### 1. 前端性能
- 页面加载时间: < 2 秒
- 页面刷新流畅度: 无卡顿
- 表格滚动: 使用虚拟滚动，支持大数据量

### 2. 后端性能
```bash
# 使用 wrk 压力测试（需安装 wrk）
wrk -t4 -c100 -d10s http://127.0.0.1:8094/health
```

### 3. API 响应时间
- 健康检查: < 10ms
- 获取合约列表: < 50ms
- 获取订单簿: < 100ms
- 提交订单: < 200ms

## 常见问题排查

### 1. 前端无法连接后端
**现象**: 页面显示 "请求失败"

**检查**:
```bash
# 检查后端是否启动
curl http://127.0.0.1:8094/health

# 检查端口占用
netstat -an | grep 8094
```

### 2. 订单提交失败
**现象**: 提示 "订单提交失败"

**检查**:
1. 确认已开户
2. 确认账户余额充足
3. 查看后端日志：`RUST_LOG=debug cargo run --bin qaexchange-server`

### 3. 订单簿为空
**现象**: 订单簿显示空白

**原因**: 初始状态没有挂单

**解决**: 提交几个限价单后，订单簿会显示数据

### 4. 页面数据不更新
**现象**: 数据长时间不变化

**检查**:
1. 打开浏览器开发者工具（F12）
2. 查看 Console 是否有错误
3. 查看 Network 面板，确认 API 请求正常

## 自动化测试（TODO）

### 单元测试
```bash
cd /home/quantaxis/qaexchange-rs/web
npm run test:unit
```

### E2E 测试
```bash
npm run test:e2e
```

## 测试数据准备脚本

创建测试账户和订单：

```bash
#!/bin/bash

# 创建 10 个测试账户
for i in {1..10}; do
  curl -X POST http://127.0.0.1:8094/api/account/open \
    -H "Content-Type: application/json" \
    -d "{
      \"user_id\": \"user$i\",
      \"user_name\": \"测试用户$i\",
      \"init_cash\": 1000000.0,
      \"account_type\": \"individual\",
      \"password\": \"123456\"
    }"
  echo ""
  sleep 0.5
done

# 提交测试订单
for i in {1..5}; do
  curl -X POST http://127.0.0.1:8094/api/order/submit \
    -H "Content-Type: application/json" \
    -d "{
      \"user_id\": \"user1\",
      \"instrument_id\": \"IF2501\",
      \"direction\": \"BUY\",
      \"offset\": \"OPEN\",
      \"price\": $((3800 + i)),
      \"volume\": 1,
      \"order_type\": \"LIMIT\"
    }"
  echo ""
  sleep 0.5
done
```

## 反馈和报告

测试中发现问题，请记录：
1. 问题现象
2. 复现步骤
3. 预期行为
4. 实际行为
5. 浏览器版本和操作系统

提交 Issue: https://github.com/yourusername/qaexchange-rs/issues
