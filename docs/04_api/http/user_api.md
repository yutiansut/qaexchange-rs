# REST API 参考文档

**Base URL**: `http://localhost:8080`
**版本**: v1.0
**协议**: HTTP/1.1
**Content-Type**: `application/json`

---

## 📋 目录

- [通用说明](#通用说明)
- [用户认证 API](#用户认证-api)
- [用户账户管理 API](#用户账户管理-api)
- [账户管理 API](#账户管理-api)
- [订单管理 API](#订单管理-api)
- [持仓查询 API](#持仓查询-api)
- [成交记录 API](#成交记录-api)
- [资金流水 API](#资金流水-api)
- [权益曲线 API](#权益曲线-api)
- [系统 API](#系统-api)
- [错误处理](#错误处理)

---

## 通用说明

### 请求头

所有请求建议携带以下 Header：

```http
Content-Type: application/json
Authorization: Bearer {token}  # 需要认证的接口
```

### 响应格式

所有 API 响应统一格式：

**成功响应**:
```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

**失败响应**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 400,
    "message": "错误描述"
  }
}
```

### 错误码

| 错误码 | 说明 |
|--------|------|
| 400 | 请求参数错误 |
| 401 | 未授权/认证失败 |
| 404 | 资源不存在 |
| 500 | 服务器内部错误 |
| 1001 | 资金不足 |
| 1002 | 订单不存在 |
| 1003 | 账户不存在 |
| 1004 | 持仓不足 |

---

## 用户认证 API

### 1. 用户注册

**POST** `/api/auth/register`

注册新用户账号。

**请求体**:
```json
{
  "username": "zhangsan",
  "password": "password123",
  "phone": "13800138000",
  "email": "zhangsan@example.com",
  "real_name": "张三"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "zhangsan",
    "message": "注册成功"
  },
  "error": null
}
```

**示例**:
```bash
curl -X POST http://localhost:8080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "zhangsan",
    "password": "password123",
    "phone": "13800138000",
    "email": "zhangsan@example.com",
    "real_name": "张三"
  }'
```

---

### 2. 用户登录

**POST** `/api/auth/login`

用户登录认证。

**请求体**:
```json
{
  "username": "zhangsan",
  "password": "password123"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "success": true,
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "zhangsan",
    "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "message": "登录成功"
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function login(username, password) {
  const response = await fetch('http://localhost:8080/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password })
  });
  const result = await response.json();

  if (result.data.success) {
    // 保存token
    localStorage.setItem('token', result.data.token);
    localStorage.setItem('user_id', result.data.user_id);
  }

  return result.data;
}

// 使用
const loginResult = await login('zhangsan', 'password123');
```

---

### 3. 获取用户信息

**GET** `/api/auth/user/{user_id}`

获取当前登录用户的详细信息。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "zhangsan",
    "phone": "13800138000",
    "email": "zhangsan@example.com",
    "real_name": "张三",
    "account_ids": ["ACC_xxx", "ACC_yyy"],
    "created_at": 1704067200000,
    "status": "Active"
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getUserInfo(userId) {
  const response = await fetch(`http://localhost:8080/api/auth/user/${userId}`);
  return await response.json();
}
```

---

### 4. 获取所有用户列表（管理员）

**GET** `/api/auth/users`

获取系统中所有用户的列表（仅管理员可用）。

**响应**:
```json
{
  "success": true,
  "data": {
    "users": [
      {
        "user_id": "550e8400-e29b-41d4-a716-446655440000",
        "username": "zhangsan",
        "phone": "13800138000",
        "email": "zhangsan@example.com",
        "real_name": "张三",
        "account_ids": ["ACC_xxx", "ACC_yyy"],
        "created_at": 1704067200000,
        "status": "Active"
      }
    ],
    "total": 100
  },
  "error": null
}
```

**示例**:
```bash
curl http://localhost:8080/api/auth/users
```

---

## 用户账户管理 API

### 5. 为用户创建交易账户

**POST** `/api/user/{user_id}/account/create`

为指定用户创建新的交易账户。

**路径参数**:
- `user_id` (string, required): 用户ID

**请求体**:
```json
{
  "account_name": "主账户",
  "init_cash": 1000000.0,
  "account_type": "individual"
}
```

**字段说明**:
- `account_type`: 账户类型
  - `individual`: 个人账户
  - `institutional`: 机构账户
  - `market_maker`: 做市商账户

**响应**:
```json
{
  "success": true,
  "data": {
    "account_id": "ACC_125d84fdfc2a4a2a906ac9f7fc2bf3b0",
    "account_name": "主账户",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "balance": 1000000.0,
    "created_at": 1704067200000
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function createAccount(userId, accountName, initCash) {
  const response = await fetch(`http://localhost:8080/api/user/${userId}/account/create`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      account_name: accountName,
      init_cash: initCash,
      account_type: 'individual'
    })
  });
  return await response.json();
}

// 使用
const result = await createAccount('user_uuid', '主账户', 1000000);
```

---

### 6. 获取用户的所有账户

**GET** `/api/user/{user_id}/accounts`

获取指定用户的所有交易账户列表。

**路径参数**:
- `user_id` (string, required): 用户ID（支持UUID或账户ID）

**支持两种模式**:
- 传入 `user_id` (UUID格式) → 返回该用户的所有账户（经纪商模式）
- 传入 `account_id` (ACC_xxx格式) → 返回该账户（交易所模式）

**响应**:
```json
{
  "success": true,
  "data": {
    "accounts": [
      {
        "account_id": "ACC_125d84fdfc2a4a2a906ac9f7fc2bf3b0",
        "account_name": "主账户",
        "balance": 1000000.0,
        "available": 800000.0,
        "margin": 200000.0,
        "risk_ratio": 0.2,
        "profit": 5000.0,
        "account_type": "Individual",
        "created_at": 1704067200000
      }
    ],
    "total": 1
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getUserAccounts(userId) {
  const response = await fetch(`http://localhost:8080/api/user/${userId}/accounts`);
  return await response.json();
}

// 使用
const result = await getUserAccounts('user_uuid');
console.log(`用户共有 ${result.data.total} 个账户`);
```

---

## 账户管理 API

### 7. 开户

**POST** `/api/account/open`

创建新的交易账户。

**请求体**:
```json
{
  "user_id": "user001",
  "user_name": "张三",
  "init_cash": 1000000.0,
  "account_type": "individual",  // "individual" | "institutional"
  "password": "password123"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "account_id": "user001"
  },
  "error": null
}
```

**示例**:
```bash
curl -X POST http://localhost:8080/api/account/open \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user001",
    "user_name": "张三",
    "init_cash": 1000000,
    "account_type": "individual",
    "password": "password123"
  }'
```

```javascript
// JavaScript
const response = await fetch('http://localhost:8080/api/account/open', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    user_id: 'user001',
    user_name: '张三',
    init_cash: 1000000,
    account_type: 'individual',
    password: 'password123'
  })
});
const result = await response.json();
```

---

### 2. 查询账户

**GET** `/api/account/{user_id}`

查询账户详细信息。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "user001",
    "user_name": "张三",
    "balance": 1000000.0,
    "available": 950000.0,
    "frozen": 50000.0,
    "margin": 50000.0,
    "profit": 5000.0,
    "risk_ratio": 0.05,
    "account_type": "individual",
    "created_at": 1696320000000
  },
  "error": null
}
```

**字段说明**:
- `balance`: 账户权益（总资产）
- `available`: 可用资金
- `frozen`: 冻结资金
- `margin`: 占用保证金
- `profit`: 累计盈亏
- `risk_ratio`: 风险度（0-1，1表示100%）

**示例**:
```bash
curl http://localhost:8080/api/account/user001
```

```javascript
// JavaScript
const response = await fetch('http://localhost:8080/api/account/user001');
const account = await response.json();
console.log('账户余额:', account.data.balance);
```

```python
# Python
import requests

response = requests.get('http://localhost:8080/api/account/user001')
account = response.json()
print(f"账户余额: {account['data']['balance']}")
```

---

### 3. 入金

**POST** `/api/account/deposit`

向账户充值资金。

**请求体**:
```json
{
  "user_id": "user001",
  "amount": 100000.0
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "balance": 1100000.0,
    "available": 1050000.0
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function deposit(userId, amount) {
  const response = await fetch('http://localhost:8080/api/account/deposit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user_id: userId, amount })
  });
  return await response.json();
}

// 使用
const result = await deposit('user001', 100000);
```

---

### 4. 出金

**POST** `/api/account/withdraw`

从账户提取资金。

**请求体**:
```json
{
  "user_id": "user001",
  "amount": 50000.0
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "balance": 1050000.0,
    "available": 1000000.0
  },
  "error": null
}
```

**错误情况**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 400,
    "message": "Insufficient available balance"
  }
}
```

---

## 订单管理 API

### 5. 提交订单

**POST** `/api/order/submit`

提交交易订单。

**请求体**:
```json
{
  "user_id": "user001",
  "account_id": "ACC_user001_01",  // ✨ Phase 10: 必填，指定交易账户
  "instrument_id": "IX2301",
  "direction": "BUY",          // "BUY" | "SELL"
  "offset": "OPEN",             // "OPEN" | "CLOSE" | "CLOSETODAY"
  "volume": 10.0,
  "price": 120.0,
  "order_type": "LIMIT"         // "LIMIT" | "MARKET"
}
```

**字段说明**:
- `user_id` (string, required): 用户ID，用于身份验证
- `account_id` (string, required): 交易账户ID，指定使用哪个账户交易
  - ⚠️ 系统会验证 `account_id` 是否属于 `user_id`，防止跨账户操作
- `direction`:
  - `BUY`: 买入
  - `SELL`: 卖出
- `offset`:
  - `OPEN`: 开仓
  - `CLOSE`: 平仓（平昨仓）
  - `CLOSETODAY`: 平今仓
- `order_type`:
  - `LIMIT`: 限价单
  - `MARKET`: 市价单

**响应**:
```json
{
  "success": true,
  "data": {
    "order_id": "O17251234567890000001",
    "status": "submitted"
  },
  "error": null
}
```

**风控拒绝响应**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 1001,
    "message": "Insufficient funds: available=50000.00, required=120000.00"
  }
}
```

**示例**:
```javascript
// JavaScript - 提交买单
async function submitOrder(params) {
  const response = await fetch('http://localhost:8080/api/order/submit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params)
  });
  return await response.json();
}

// 买入开仓（✨ Phase 10: 必须包含 account_id）
const buyOrder = await submitOrder({
  user_id: 'user001',
  account_id: 'ACC_user001_01',  // ✨ 指定交易账户
  instrument_id: 'IX2301',
  direction: 'BUY',
  offset: 'OPEN',
  volume: 10,
  price: 120.0,
  order_type: 'LIMIT'
});

// 卖出平仓
const sellOrder = await submitOrder({
  user_id: 'user001',
  account_id: 'ACC_user001_01',  // ✨ 指定交易账户
  instrument_id: 'IX2301',
  direction: 'SELL',
  offset: 'CLOSE',
  volume: 5,
  price: 125.0,
  order_type: 'LIMIT'
});
```

```python
# Python - 提交订单（✨ Phase 10: 添加 account_id 参数）
def submit_order(user_id, account_id, instrument_id, direction, offset, volume, price):
    url = 'http://localhost:8080/api/order/submit'
    data = {
        'user_id': user_id,
        'account_id': account_id,  # ✨ 交易账户ID
        'instrument_id': instrument_id,
        'direction': direction,
        'offset': offset,
        'volume': volume,
        'price': price,
        'order_type': 'LIMIT'
    }
    response = requests.post(url, json=data)
    return response.json()

# 使用
result = submit_order('user001', 'ACC_user001_01', 'IX2301', 'BUY', 'OPEN', 10, 120.0)
print(f"订单ID: {result['data']['order_id']}")
```

---

### 6. 撤单

**POST** `/api/order/cancel`

撤销未成交或部分成交的订单。

**请求体**:
```json
{
  "user_id": "user001",
  "account_id": "ACC_user001_01",  // ✨ Phase 10: 必填，指定交易账户
  "order_id": "O17251234567890000001"
}
```

**字段说明**:
- `user_id` (string, required): 用户ID，用于身份验证
- `account_id` (string, required): 交易账户ID
  - ⚠️ 系统会验证订单是否属于该账户，防止跨账户撤单
- `order_id` (string, required): 订单ID

**响应**:
```json
{
  "success": true,
  "data": {
    "order_id": "O17251234567890000001"
  },
  "error": null
}
```

**错误情况**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 1002,
    "message": "Order cannot be cancelled in status: Filled"
  }
}
```

**示例**:
```javascript
// JavaScript（✨ Phase 10: 添加 account_id 参数）
async function cancelOrder(userId, accountId, orderId) {
  const response = await fetch('http://localhost:8080/api/order/cancel', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user_id: userId,
      account_id: accountId,  // ✨ 指定账户ID
      order_id: orderId
    })
  });
  return await response.json();
}

// 使用
const result = await cancelOrder('user001', 'ACC_user001_01', 'O17251234567890000001');
```

---

### 7. 查询订单

**GET** `/api/order/{order_id}`

查询单个订单详情。

**路径参数**:
- `order_id` (string, required): 订单ID

**响应**:
```json
{
  "success": true,
  "data": {
    "order_id": "O17251234567890000001",
    "user_id": "user001",
    "instrument_id": "IX2301",
    "direction": "BUY",
    "offset": "OPEN",
    "volume": 10.0,
    "price": 120.0,
    "filled_volume": 5.0,
    "status": "PartiallyFilled",
    "submit_time": 1696320000000,
    "update_time": 1696320001000
  },
  "error": null
}
```

**订单状态**:
- `PendingRisk`: 等待风控检查
- `PendingRoute`: 等待路由
- `Submitted`: 已提交到撮合引擎
- `PartiallyFilled`: 部分成交
- `Filled`: 全部成交
- `Cancelled`: 已撤单
- `Rejected`: 被拒绝

**示例**:
```javascript
// JavaScript
const response = await fetch('http://localhost:8080/api/order/O17251234567890000001');
const order = await response.json();
console.log('订单状态:', order.data.status);
console.log('已成交量:', order.data.filled_volume);
```

---

### 8. 查询用户订单列表

**GET** `/api/order/user/{user_id}`

查询用户的所有订单。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "order_id": "O17251234567890000001",
      "user_id": "user001",
      "instrument_id": "IX2301",
      "direction": "BUY",
      "offset": "OPEN",
      "volume": 10.0,
      "price": 120.0,
      "filled_volume": 10.0,
      "status": "Filled",
      "submit_time": 1696320000000,
      "update_time": 1696320001000
    },
    {
      "order_id": "O17251234567890000002",
      "user_id": "user001",
      "instrument_id": "IX2301",
      "direction": "SELL",
      "offset": "CLOSE",
      "volume": 5.0,
      "price": 125.0,
      "filled_volume": 0.0,
      "status": "Submitted",
      "submit_time": 1696320010000,
      "update_time": 1696320010000
    }
  ],
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getUserOrders(userId) {
  const response = await fetch(`http://localhost:8080/api/order/user/${userId}`);
  const result = await response.json();
  return result.data;
}

// 使用
const orders = await getUserOrders('user001');
console.log(`用户共有 ${orders.length} 个订单`);

// 筛选未成交订单
const pendingOrders = orders.filter(o =>
  o.status === 'Submitted' || o.status === 'PartiallyFilled'
);
```

---

## 持仓查询 API

### 9. 查询用户所有持仓

**GET** `/api/position/user/{user_id}`

查询用户所有账户的持仓（聚合查询）。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "account_id": "ACC_xxx",
      "instrument_id": "IF2501",
      "volume_long": 10.0,
      "volume_short": 0.0,
      "cost_long": 3800.0,
      "cost_short": 0.0,
      "profit_long": 5000.0,
      "profit_short": 0.0
    },
    {
      "account_id": "ACC_yyy",
      "instrument_id": "IC2501",
      "volume_long": 0.0,
      "volume_short": 5.0,
      "cost_long": 0.0,
      "cost_short": 6500.0,
      "profit_long": 0.0,
      "profit_short": -250.0
    }
  ],
  "error": null
}
```

**字段说明**:
- `account_id`: 账户ID（用于区分不同账户的持仓）
- `volume_long`: 多头持仓量
- `volume_short`: 空头持仓量
- `cost_long`: 多头开仓成本
- `cost_short`: 空头开仓成本
- `profit_long`: 多头浮动盈亏
- `profit_short`: 空头浮动盈亏

**示例**:
```javascript
// JavaScript
async function getUserPositions(userId) {
  const response = await fetch(`http://localhost:8080/api/position/user/${userId}`);
  const result = await response.json();
  return result.data;
}

// 使用
const positions = await getUserPositions('user001');

// 计算总持仓盈亏
const totalProfit = positions.reduce((sum, pos) =>
  sum + pos.profit_long + pos.profit_short, 0
);
console.log('总浮动盈亏:', totalProfit);

// 按账户分组持仓
const positionsByAccount = positions.reduce((acc, pos) => {
  if (!acc[pos.account_id]) acc[pos.account_id] = [];
  acc[pos.account_id].push(pos);
  return acc;
}, {});
```

---

### 10. 查询账户持仓

**GET** `/api/position/account/{account_id}`

查询指定账户的持仓。

**路径参数**:
- `account_id` (string, required): 账户ID

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IF2501",
      "volume_long": 10.0,
      "volume_short": 0.0,
      "cost_long": 3800.0,
      "cost_short": 0.0,
      "profit_long": 5000.0,
      "profit_short": 0.0
    }
  ],
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getAccountPositions(accountId) {
  const response = await fetch(`http://localhost:8080/api/position/account/${accountId}`);
  const result = await response.json();
  return result.data;
}
```

---

## 成交记录 API

### 11. 查询用户所有成交

**GET** `/api/trades/user/{user_id}`

查询用户所有账户的成交记录（聚合查询）。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": {
    "trades": [
      {
        "trade_id": "TRD_xxx",
        "order_id": "ORD_xxx",
        "account_id": "ACC_xxx",
        "instrument_id": "IF2501",
        "direction": "BUY",
        "offset": "OPEN",
        "volume": 5,
        "price": 3800.0,
        "trade_time": 1704067300000,
        "commission": 10.5
      }
    ],
    "total": 200
  },
  "error": null
}
```

**字段说明**:
- `trade_id`: 成交ID
- `order_id`: 关联的订单ID
- `account_id`: 账户ID
- `direction`: 买卖方向（BUY/SELL）
- `offset`: 开平标志（OPEN/CLOSE）
- `commission`: 手续费

**示例**:
```javascript
// JavaScript
async function getUserTrades(userId) {
  const response = await fetch(`http://localhost:8080/api/trades/user/${userId}`);
  const result = await response.json();
  return result.data;
}

// 使用
const { trades, total } = await getUserTrades('user001');
console.log(`用户共有 ${total} 条成交记录`);

// 计算总手续费
const totalCommission = trades.reduce((sum, trade) =>
  sum + trade.commission, 0
);
console.log('总手续费:', totalCommission);
```

---

### 12. 查询账户成交

**GET** `/api/trades/account/{account_id}`

查询指定账户的成交记录。

**路径参数**:
- `account_id` (string, required): 账户ID

**响应**:
```json
{
  "success": true,
  "data": {
    "trades": [
      {
        "trade_id": "TRD_xxx",
        "order_id": "ORD_xxx",
        "instrument_id": "IF2501",
        "direction": "BUY",
        "offset": "OPEN",
        "volume": 5,
        "price": 3800.0,
        "trade_time": 1704067300000,
        "commission": 10.5
      }
    ],
    "total": 100
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getAccountTrades(accountId) {
  const response = await fetch(`http://localhost:8080/api/trades/account/${accountId}`);
  const result = await response.json();
  return result.data;
}
```

---

## 资金流水 API

### 13. 查询资金流水（管理端）

**GET** `/api/management/transactions/{user_id}`

查询用户的资金流水记录（仅管理员可用）。

**路径参数**:
- `user_id` (string, required): 用户ID或账户ID

**查询参数**:
- `start_date` (string, optional): 开始日期（格式：2024-01-01）
- `end_date` (string, optional): 结束日期（格式：2024-12-31）
- `limit` (number, optional): 最多返回条数

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "transaction_id": "TXN_xxx",
      "user_id": "ACC_xxx",
      "type": "DEPOSIT",
      "amount": 100000.0,
      "method": "银行转账",
      "balance_before": 900000.0,
      "balance_after": 1000000.0,
      "timestamp": 1704067200000,
      "remark": "初始入金"
    },
    {
      "transaction_id": "TXN_yyy",
      "user_id": "ACC_xxx",
      "type": "WITHDRAW",
      "amount": 50000.0,
      "method": "银行转账",
      "balance_before": 1000000.0,
      "balance_after": 950000.0,
      "timestamp": 1704153600000,
      "remark": "客户提现"
    }
  ],
  "error": null
}
```

**字段说明**:
- `type`: 交易类型（DEPOSIT: 入金, WITHDRAW: 出金）
- `method`: 入金/出金方式（如：银行转账、第三方支付等）
- `balance_before`: 交易前余额
- `balance_after`: 交易后余额

**示例**:
```javascript
// JavaScript
async function getTransactions(userId, startDate, endDate, limit) {
  const params = new URLSearchParams();
  if (startDate) params.append('start_date', startDate);
  if (endDate) params.append('end_date', endDate);
  if (limit) params.append('limit', limit);

  const url = `http://localhost:8080/api/management/transactions/${userId}?${params}`;
  const response = await fetch(url);
  return await response.json();
}

// 使用 - 查询最近100条
const result = await getTransactions('user001', null, null, 100);

// 使用 - 按日期范围查询
const result = await getTransactions('user001', '2024-01-01', '2024-12-31');
```

---

## 权益曲线 API

### 14. 获取账户权益曲线

**GET** `/api/account/{user_id}/equity-curve`

获取账户的权益曲线数据（每日结算数据）。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "account_id": "ACC_xxx",
      "account_name": "主账户",
      "balance": 1000000.0,
      "available": 800000.0,
      "margin": 200000.0,
      "settlements": [
        {
          "date": "2024-01-01",
          "equity": 1000000.0,
          "profit": 0.0,
          "return_rate": 0.0
        },
        {
          "date": "2024-01-02",
          "equity": 1005000.0,
          "profit": 5000.0,
          "return_rate": 0.005
        }
      ]
    }
  ],
  "error": null
}
```

**字段说明**:
- `settlements`: 每日结算记录数组
  - `date`: 日期
  - `equity`: 账户权益
  - `profit`: 当日盈亏
  - `return_rate`: 收益率

**示例**:
```javascript
// JavaScript
async function getEquityCurve(userId) {
  const response = await fetch(`http://localhost:8080/api/account/${userId}/equity-curve`);
  const result = await response.json();
  return result.data;
}

// 使用
const accounts = await getEquityCurve('user001');

// 绘制权益曲线
accounts.forEach(account => {
  const dates = account.settlements.map(s => s.date);
  const equities = account.settlements.map(s => s.equity);

  console.log(`账户 ${account.account_name} 权益曲线:`);
  console.log('日期:', dates);
  console.log('权益:', equities);

  // 计算总收益率
  const initialEquity = account.settlements[0]?.equity || 0;
  const currentEquity = account.balance;
  const totalReturn = (currentEquity - initialEquity) / initialEquity;
  console.log(`总收益率: ${(totalReturn * 100).toFixed(2)}%`);
});
```

---

## 系统 API

### 10. 健康检查

**GET** `/health`

检查服务器运行状态。

**响应**:
```json
{
  "status": "ok",
  "service": "qaexchange"
}
```

**示例**:
```javascript
// JavaScript
async function checkHealth() {
  const response = await fetch('http://localhost:8080/health');
  const health = await response.json();
  return health.status === 'ok';
}

// 使用
if (await checkHealth()) {
  console.log('服务器运行正常');
}
```

---

## 错误处理

### 错误响应格式

所有错误响应遵循统一格式：

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 错误码,
    "message": "错误描述"
  }
}
```

### 常见错误处理

```javascript
// JavaScript - 统一错误处理
async function apiCall(url, options = {}) {
  try {
    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options.headers
      }
    });

    const result = await response.json();

    if (!result.success) {
      throw new Error(`API Error: ${result.error.message} (code: ${result.error.code})`);
    }

    return result.data;
  } catch (error) {
    console.error('API调用失败:', error);
    throw error;
  }
}

// 使用
try {
  const account = await apiCall('http://localhost:8080/api/account/user001');
  console.log('账户余额:', account.balance);
} catch (error) {
  // 处理错误
  if (error.message.includes('1003')) {
    console.error('账户不存在');
  }
}
```

---

## 完整示例

### React 示例

```jsx
import React, { useState, useEffect } from 'react';

const API_BASE = 'http://localhost:8080';

function TradingApp() {
  const [account, setAccount] = useState(null);
  const [orders, setOrders] = useState([]);
  const [positions, setPositions] = useState([]);

  useEffect(() => {
    loadAccountData('user001');
  }, []);

  async function loadAccountData(userId) {
    try {
      // 查询账户
      const accountRes = await fetch(`${API_BASE}/api/account/${userId}`);
      const accountData = await accountRes.json();
      setAccount(accountData.data);

      // 查询订单
      const ordersRes = await fetch(`${API_BASE}/api/order/user/${userId}`);
      const ordersData = await ordersRes.json();
      setOrders(ordersData.data);

      // 查询持仓
      const positionsRes = await fetch(`${API_BASE}/api/position/${userId}`);
      const positionsData = await positionsRes.json();
      setPositions(positionsData.data);
    } catch (error) {
      console.error('加载数据失败:', error);
    }
  }

  async function submitOrder(orderParams) {
    const response = await fetch(`${API_BASE}/api/order/submit`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(orderParams)
    });

    const result = await response.json();

    if (result.success) {
      alert(`订单提交成功: ${result.data.order_id}`);
      loadAccountData('user001'); // 刷新数据
    } else {
      alert(`订单提交失败: ${result.error.message}`);
    }
  }

  return (
    <div>
      <h1>交易终端</h1>

      {/* 账户信息 */}
      {account && (
        <div className="account-info">
          <h2>账户信息</h2>
          <p>余额: {account.balance}</p>
          <p>可用: {account.available}</p>
          <p>风险度: {(account.risk_ratio * 100).toFixed(2)}%</p>
        </div>
      )}

      {/* 下单区域 */}
      <div className="order-form">
        <button onClick={() => submitOrder({
          user_id: 'user001',
          account_id: 'ACC_user001_01',  // ✨ Phase 10: 必须指定账户
          instrument_id: 'IX2301',
          direction: 'BUY',
          offset: 'OPEN',
          volume: 10,
          price: 120.0,
          order_type: 'LIMIT'
        })}>
          买入开仓
        </button>
      </div>

      {/* 订单列表 */}
      <div className="orders">
        <h2>我的订单</h2>
        {orders.map(order => (
          <div key={order.order_id}>
            {order.instrument_id} - {order.status}
          </div>
        ))}
      </div>

      {/* 持仓列表 */}
      <div className="positions">
        <h2>我的持仓</h2>
        {positions.map(pos => (
          <div key={pos.instrument_id}>
            {pos.instrument_id} - 多:{pos.volume_long} 空:{pos.volume_short}
          </div>
        ))}
      </div>
    </div>
  );
}

export default TradingApp;
```

---

## API 速查表

### 用户认证
| 功能 | Method | Endpoint |
|------|--------|----------|
| 用户注册 | POST | `/api/auth/register` |
| 用户登录 | POST | `/api/auth/login` |
| 获取用户信息 | GET | `/api/auth/user/{user_id}` |
| 获取用户列表（管理员） | GET | `/api/auth/users` |

### 用户账户管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 创建交易账户 | POST | `/api/user/{user_id}/account/create` |
| 获取用户所有账户 | GET | `/api/user/{user_id}/accounts` |

### 账户管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 开户 | POST | `/api/account/open` |
| 查询账户 | GET | `/api/account/{account_id}` |
| 入金 | POST | `/api/account/deposit` |
| 出金 | POST | `/api/account/withdraw` |
| 权益曲线 | GET | `/api/account/{user_id}/equity-curve` |

### 订单管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 提交订单 | POST | `/api/order/submit` |
| 撤单 | POST | `/api/order/cancel` |
| 查询订单 | GET | `/api/order/{order_id}` |
| 查询用户订单 | GET | `/api/order/user/{user_id}` |

### 持仓查询
| 功能 | Method | Endpoint |
|------|--------|----------|
| 查询用户所有持仓 | GET | `/api/position/user/{user_id}` |
| 查询账户持仓 | GET | `/api/position/account/{account_id}` |

### 成交记录
| 功能 | Method | Endpoint |
|------|--------|----------|
| 查询用户所有成交 | GET | `/api/trades/user/{user_id}` |
| 查询账户成交 | GET | `/api/trades/account/{account_id}` |

### 资金流水（管理端）
| 功能 | Method | Endpoint |
|------|--------|----------|
| 查询资金流水 | GET | `/api/management/transactions/{user_id}` |

### 系统
| 功能 | Method | Endpoint |
|------|--------|----------|
| 健康检查 | GET | `/health` |

---

**文档版本**: v1.1
**最后更新**: 2025-11-25
**Base URL**: `http://localhost:8080` (默认端口可能是8094)
