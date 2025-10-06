# REST API 参考文档

**Base URL**: `http://localhost:8080`
**版本**: v1.0
**协议**: HTTP/1.1
**Content-Type**: `application/json`

---

## 📋 目录

- [通用说明](#通用说明)
- [账户管理 API](#账户管理-api)
- [订单管理 API](#订单管理-api)
- [持仓查询 API](#持仓查询-api)
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

## 账户管理 API

### 1. 开户

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
  "instrument_id": "IX2301",
  "direction": "BUY",          // "BUY" | "SELL"
  "offset": "OPEN",             // "OPEN" | "CLOSE" | "CLOSETODAY"
  "volume": 10.0,
  "price": 120.0,
  "order_type": "LIMIT"         // "LIMIT" | "MARKET"
}
```

**字段说明**:
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

// 买入开仓
const buyOrder = await submitOrder({
  user_id: 'user001',
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
  instrument_id: 'IX2301',
  direction: 'SELL',
  offset: 'CLOSE',
  volume: 5,
  price: 125.0,
  order_type: 'LIMIT'
});
```

```python
# Python - 提交订单
def submit_order(user_id, instrument_id, direction, offset, volume, price):
    url = 'http://localhost:8080/api/order/submit'
    data = {
        'user_id': user_id,
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
result = submit_order('user001', 'IX2301', 'BUY', 'OPEN', 10, 120.0)
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
  "order_id": "O17251234567890000001"
}
```

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
// JavaScript
async function cancelOrder(userId, orderId) {
  const response = await fetch('http://localhost:8080/api/order/cancel', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user_id: userId, order_id: orderId })
  });
  return await response.json();
}

// 使用
const result = await cancelOrder('user001', 'O17251234567890000001');
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

### 9. 查询持仓

**GET** `/api/position/{user_id}`

查询用户持仓。

**路径参数**:
- `user_id` (string, required): 用户ID

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IX2301",
      "volume_long": 10.0,
      "volume_short": 0.0,
      "cost_long": 120.0,
      "cost_short": 0.0,
      "profit_long": 500.0,
      "profit_short": 0.0
    },
    {
      "instrument_id": "IF2301",
      "volume_long": 0.0,
      "volume_short": 5.0,
      "cost_long": 0.0,
      "cost_short": 4500.0,
      "profit_long": 0.0,
      "profit_short": -250.0
    }
  ],
  "error": null
}
```

**字段说明**:
- `volume_long`: 多头持仓量
- `volume_short`: 空头持仓量
- `cost_long`: 多头开仓成本
- `cost_short`: 空头开仓成本
- `profit_long`: 多头浮动盈亏
- `profit_short`: 空头浮动盈亏

**示例**:
```javascript
// JavaScript
async function getPositions(userId) {
  const response = await fetch(`http://localhost:8080/api/position/${userId}`);
  const result = await response.json();
  return result.data;
}

// 使用
const positions = await getPositions('user001');

// 计算总持仓盈亏
const totalProfit = positions.reduce((sum, pos) =>
  sum + pos.profit_long + pos.profit_short, 0
);
console.log('总浮动盈亏:', totalProfit);

// 筛选有持仓的合约
const activePositions = positions.filter(pos =>
  pos.volume_long > 0 || pos.volume_short > 0
);
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

| 功能 | Method | Endpoint | 说明 |
|------|--------|----------|------|
| 开户 | POST | `/api/account/open` | 创建新账户 |
| 查询账户 | GET | `/api/account/{user_id}` | 查询账户信息 |
| 入金 | POST | `/api/account/deposit` | 账户充值 |
| 出金 | POST | `/api/account/withdraw` | 账户提现 |
| 提交订单 | POST | `/api/order/submit` | 下单 |
| 撤单 | POST | `/api/order/cancel` | 撤销订单 |
| 查询订单 | GET | `/api/order/{order_id}` | 订单详情 |
| 查询用户订单 | GET | `/api/order/user/{user_id}` | 用户订单列表 |
| 查询持仓 | GET | `/api/position/{user_id}` | 持仓信息 |
| 健康检查 | GET | `/health` | 服务状态 |

---

**文档版本**: v1.0
**最后更新**: 2025-10-03
