# 管理端 API 参考文档

**Base URL**: `http://localhost:8080`
**版本**: v1.0
**协议**: HTTP/1.1
**Content-Type**: `application/json`
**权限要求**: 管理员权限

---

## 📋 目录

- [账户管理 API](#账户管理-api)
- [资金管理 API](#资金管理-api)
- [合约管理 API](#合约管理-api)
- [结算管理 API](#结算管理-api)
- [风控管理 API](#风控管理-api)
- [系统监控 API](#系统监控-api)
- [市场数据 API](#市场数据-api)

---

## 账户管理 API

### 1. 获取所有账户列表

**GET** `/api/management/accounts`

获取系统中所有交易账户的列表（管理端专用）。

**查询参数**:
- `page` (number, optional): 页码，默认1
- `page_size` (number, optional): 每页条数，默认20
- `status` (string, optional): 账户状态筛选

**响应**:
```json
{
  "success": true,
  "data": {
    "total": 500,
    "page": 1,
    "page_size": 20,
    "accounts": [
      {
        "user_id": "ACC_xxx",
        "user_name": "主账户",
        "account_type": "Individual",
        "balance": 1000000.0,
        "available": 800000.0,
        "margin_used": 200000.0,
        "risk_ratio": 0.2,
        "created_at": 1704067200000
      }
    ]
  },
  "error": null
}
```

**字段说明**:
- `user_id`: 账户ID（注意：这里实际是account_id）
- `user_name`: 账户名称
- `account_type`: 账户类型（Individual/Institutional/MarketMaker）
- `risk_ratio`: 风险度（0-1，1表示100%）

**示例**:
```bash
# 查询第1页，每页50条
curl 'http://localhost:8080/api/management/accounts?page=1&page_size=50'
```

```javascript
// JavaScript
async function getAllAccounts(page = 1, pageSize = 20) {
  const response = await fetch(
    `http://localhost:8080/api/management/accounts?page=${page}&page_size=${pageSize}`
  );
  return await response.json();
}

// 使用
const result = await getAllAccounts(1, 50);
console.log(`共有 ${result.data.total} 个账户`);
```

---

### 2. 获取账户详情

**GET** `/api/management/account/{user_id}/detail`

获取指定账户的详细信息，包括账户信息、持仓和订单。

**路径参数**:
- `user_id` (string, required): 账户ID

**响应**:
```json
{
  "success": true,
  "data": {
    "account_info": {
      "balance": 1000000.0,
      "available": 800000.0,
      "margin": 200000.0,
      "static_balance": 995000.0,
      "float_profit": 5000.0,
      "position_profit": 3000.0,
      "close_profit": 2000.0,
      "commission": 150.0,
      "risk_ratio": 0.2
    },
    "positions": [
      {
        "instrument_id": "IF2501",
        "volume_long": 10,
        "volume_short": 0,
        "open_price_long": 3800.0,
        "float_profit_long": 5000.0,
        "margin_long": 45600.0
      }
    ],
    "orders": [
      {
        "order_id": "ORD_xxx",
        "instrument_id": "IF2501",
        "direction": "BUY",
        "offset": "OPEN",
        "volume": 10,
        "price": 3800.0,
        "status": "Filled",
        "submit_time": 1704067200000
      }
    ]
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function getAccountDetail(accountId) {
  const response = await fetch(
    `http://localhost:8080/api/management/account/${accountId}/detail`
  );
  return await response.json();
}

// 使用
const detail = await getAccountDetail('ACC_xxx');
console.log('账户余额:', detail.data.account_info.balance);
console.log('持仓数量:', detail.data.positions.length);
console.log('订单数量:', detail.data.orders.length);
```

---

## 资金管理 API

### 3. 入金（管理端）

**POST** `/api/management/deposit`

为账户办理入金业务。

**请求体**:
```json
{
  "user_id": "ACC_xxx",
  "amount": 100000.0,
  "method": "银行转账",
  "remark": "客户入金"
}
```

**字段说明**:
- `user_id` (string, required): 账户ID
- `amount` (number, required): 入金金额
- `method` (string, optional): 入金方式（如：银行转账、第三方支付等）
- `remark` (string, optional): 备注说明

**响应**:
```json
{
  "success": true,
  "data": {
    "transaction_id": "TXN_xxx",
    "user_id": "ACC_xxx",
    "type": "DEPOSIT",
    "amount": 100000.0,
    "method": "银行转账",
    "balance_before": 900000.0,
    "balance_after": 1000000.0,
    "timestamp": 1704067200000,
    "remark": "客户入金"
  },
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function deposit(accountId, amount, method, remark) {
  const response = await fetch('http://localhost:8080/api/management/deposit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user_id: accountId,
      amount,
      method,
      remark
    })
  });
  return await response.json();
}

// 使用
const result = await deposit('ACC_xxx', 100000, '银行转账', '客户初始入金');
console.log(`入金成功，新余额: ${result.data.balance_after}`);
```

---

### 4. 出金（管理端）

**POST** `/api/management/withdraw`

为账户办理出金业务。

**请求体**:
```json
{
  "user_id": "ACC_xxx",
  "amount": 50000.0,
  "method": "银行转账",
  "bank_account": "6222021234567890"
}
```

**字段说明**:
- `user_id` (string, required): 账户ID
- `amount` (number, required): 出金金额
- `method` (string, optional): 出金方式
- `bank_account` (string, optional): 银行账号

**响应**:
```json
{
  "success": true,
  "data": {
    "transaction_id": "TXN_yyy",
    "user_id": "ACC_xxx",
    "type": "WITHDRAW",
    "amount": 50000.0,
    "method": "银行转账",
    "balance_before": 1000000.0,
    "balance_after": 950000.0,
    "timestamp": 1704153600000,
    "remark": "提现至银行账户: 6222021234567890"
  },
  "error": null
}
```

**错误响应（可用资金不足）**:
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

**示例**:
```javascript
// JavaScript
async function withdraw(accountId, amount, bankAccount) {
  const response = await fetch('http://localhost:8080/api/management/withdraw', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user_id: accountId,
      amount,
      method: '银行转账',
      bank_account: bankAccount
    })
  });
  return await response.json();
}

// 使用
try {
  const result = await withdraw('ACC_xxx', 50000, '6222021234567890');
  console.log(`出金成功，新余额: ${result.data.balance_after}`);
} catch (error) {
  console.error('出金失败:', error.message);
}
```

---

### 5. 查询资金流水

**GET** `/api/management/transactions/{user_id}`

查询账户的资金流水记录（入金、出金历史）。

**路径参数**:
- `user_id` (string, required): 账户ID

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
- `balance_before`: 交易前余额
- `balance_after`: 交易后余额

**示例**:
```javascript
// JavaScript
async function getTransactions(accountId, options = {}) {
  const params = new URLSearchParams();
  if (options.startDate) params.append('start_date', options.startDate);
  if (options.endDate) params.append('end_date', options.endDate);
  if (options.limit) params.append('limit', options.limit);

  const url = `http://localhost:8080/api/management/transactions/${accountId}?${params}`;
  const response = await fetch(url);
  return await response.json();
}

// 使用 - 查询最近100条
const result = await getTransactions('ACC_xxx', { limit: 100 });

// 使用 - 按日期范围查询
const result = await getTransactions('ACC_xxx', {
  startDate: '2024-01-01',
  endDate: '2024-12-31'
});

// 计算总入金和总出金
const deposits = result.data.filter(t => t.type === 'DEPOSIT');
const withdraws = result.data.filter(t => t.type === 'WITHDRAW');
const totalDeposit = deposits.reduce((sum, t) => sum + t.amount, 0);
const totalWithdraw = withdraws.reduce((sum, t) => sum + t.amount, 0);

console.log(`总入金: ${totalDeposit}, 总出金: ${totalWithdraw}`);
```

---

## 合约管理 API

### 1. 获取所有合约

**GET** `/admin/instruments`

获取系统中所有合约列表。

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IF2501",
      "instrument_name": "沪深300股指期货2501",
      "instrument_type": "Future",
      "exchange": "CFFEX",
      "contract_multiplier": 300,
      "price_tick": 0.2,
      "margin_rate": 0.12,
      "commission_rate": 0.000023,
      "limit_up_rate": 0.10,
      "limit_down_rate": 0.10,
      "list_date": "2024-01-01",
      "expire_date": "2025-01-15",
      "status": "Trading"
    }
  ],
  "error": null
}
```

**字段说明**:
- `instrument_type`: 合约类型（"Future", "Option", "Stock"）
- `contract_multiplier`: 合约乘数
- `price_tick`: 最小变动价位
- `margin_rate`: 保证金率（0.12 = 12%）
- `commission_rate`: 手续费率
- `status`: 合约状态（"Trading", "Suspended", "Delisted"）

**示例**:
```bash
curl http://localhost:8080/admin/instruments
```

```javascript
// JavaScript
const response = await fetch('http://localhost:8080/admin/instruments');
const instruments = await response.json();
console.log(`共有 ${instruments.data.length} 个合约`);
```

---

### 2. 创建合约

**POST** `/admin/instrument/create`

创建/上市新合约。

**请求体**:
```json
{
  "instrument_id": "IF2502",
  "instrument_name": "沪深300股指期货2502",
  "instrument_type": "Future",
  "exchange": "CFFEX",
  "contract_multiplier": 300,
  "price_tick": 0.2,
  "margin_rate": 0.12,
  "commission_rate": 0.000023,
  "limit_up_rate": 0.10,
  "limit_down_rate": 0.10,
  "list_date": "2025-02-01",
  "expire_date": "2025-02-15"
}
```

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**错误响应**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "message": "Instrument IF2502 already exists"
  }
}
```

**示例**:
```javascript
// JavaScript
async function createInstrument(data) {
  const response = await fetch('http://localhost:8080/admin/instrument/create', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data)
  });
  return await response.json();
}

const result = await createInstrument({
  instrument_id: 'IF2502',
  instrument_name: '沪深300股指期货2502',
  instrument_type: 'Future',
  exchange: 'CFFEX',
  contract_multiplier: 300,
  price_tick: 0.2,
  margin_rate: 0.12,
  commission_rate: 0.000023,
  limit_up_rate: 0.10,
  limit_down_rate: 0.10
});
```

---

### 3. 更新合约

**PUT** `/admin/instrument/{instrument_id}/update`

更新合约参数（不能修改instrument_id）。

**路径参数**:
- `instrument_id` (string, required): 合约代码

**请求体**:
```json
{
  "instrument_name": "沪深300股指期货2501（更新）",
  "contract_multiplier": 300,
  "price_tick": 0.2,
  "margin_rate": 0.15,
  "commission_rate": 0.00003,
  "limit_up_rate": 0.10,
  "limit_down_rate": 0.10
}
```

**注意**: 所有字段均为可选，仅更新提供的字段。

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**示例**:
```javascript
// JavaScript - 仅更新保证金率
async function updateInstrumentMargin(instrumentId, newMarginRate) {
  const response = await fetch(
    `http://localhost:8080/admin/instrument/${instrumentId}/update`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ margin_rate: newMarginRate })
    }
  );
  return await response.json();
}

await updateInstrumentMargin('IF2501', 0.15);
```

---

### 4. 暂停合约交易

**PUT** `/admin/instrument/{instrument_id}/suspend`

暂停合约交易（临时措施）。

**路径参数**:
- `instrument_id` (string, required): 合约代码

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**示例**:
```bash
curl -X PUT http://localhost:8080/admin/instrument/IF2501/suspend
```

---

### 5. 恢复合约交易

**PUT** `/admin/instrument/{instrument_id}/resume`

恢复被暂停的合约交易。

**路径参数**:
- `instrument_id` (string, required): 合约代码

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function resumeInstrument(instrumentId) {
  const response = await fetch(
    `http://localhost:8080/admin/instrument/${instrumentId}/resume`,
    { method: 'PUT' }
  );
  return await response.json();
}

await resumeInstrument('IF2501');
```

---

### 6. 下市合约

**DELETE** `/admin/instrument/{instrument_id}/delist`

永久下市合约（不可逆操作）。

**路径参数**:
- `instrument_id` (string, required): 合约代码

**前置条件**: 所有账户必须没有该合约的未平仓持仓。

**成功响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**错误响应（有持仓）**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "message": "Cannot delist IF2501: 3 account(s) have open positions. Accounts: user001, user002, user003"
  }
}
```

**示例**:
```javascript
// JavaScript
async function delistInstrument(instrumentId) {
  const response = await fetch(
    `http://localhost:8080/admin/instrument/${instrumentId}/delist`,
    { method: 'DELETE' }
  );
  return await response.json();
}

try {
  await delistInstrument('IF2412');
  console.log('合约下市成功');
} catch (error) {
  console.error('下市失败:', error.message);
}
```

---

## 结算管理 API

### 7. 设置结算价

**POST** `/admin/settlement/set-price`

为单个合约设置结算价。

**请求体**:
```json
{
  "instrument_id": "IF2501",
  "settlement_price": 3850.0
}
```

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function setSettlementPrice(instrumentId, price) {
  const response = await fetch('http://localhost:8080/admin/settlement/set-price', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      instrument_id: instrumentId,
      settlement_price: price
    })
  });
  return await response.json();
}

await setSettlementPrice('IF2501', 3850.0);
```

---

### 8. 批量设置结算价

**POST** `/admin/settlement/batch-set-prices`

一次性设置多个合约的结算价。

**请求体**:
```json
{
  "prices": [
    {
      "instrument_id": "IF2501",
      "settlement_price": 3850.0
    },
    {
      "instrument_id": "IH2501",
      "settlement_price": 2650.0
    },
    {
      "instrument_id": "IC2501",
      "settlement_price": 5250.0
    }
  ]
}
```

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

**示例**:
```javascript
// JavaScript
async function batchSetPrices(prices) {
  const response = await fetch('http://localhost:8080/admin/settlement/batch-set-prices', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ prices })
  });
  return await response.json();
}

await batchSetPrices([
  { instrument_id: 'IF2501', settlement_price: 3850.0 },
  { instrument_id: 'IH2501', settlement_price: 2650.0 },
  { instrument_id: 'IC2501', settlement_price: 5250.0 }
]);
```

---

### 9. 执行日终结算

**POST** `/admin/settlement/execute`

执行日终结算，计算所有账户的盈亏和风险。

**前置条件**: 必须先设置所有持仓合约的结算价。

**响应**:
```json
{
  "success": true,
  "data": {
    "settlement_date": "2025-10-05",
    "total_accounts": 1250,
    "settled_accounts": 1247,
    "failed_accounts": 3,
    "force_closed_accounts": ["user123", "user456"],
    "total_commission": 152340.50,
    "total_profit": -234560.75
  },
  "error": null
}
```

**字段说明**:
- `total_accounts`: 总账户数
- `settled_accounts`: 成功结算账户数
- `failed_accounts`: 结算失败账户数
- `force_closed_accounts`: 被强平的账户列表（风险度 >= 100%）
- `total_commission`: 总手续费
- `total_profit`: 总盈亏（正为盈利，负为亏损）

**示例**:
```javascript
// JavaScript - 完整的结算流程
async function dailySettlement(settlementPrices) {
  // Step 1: 批量设置结算价
  await batchSetPrices(settlementPrices);

  // Step 2: 执行结算
  const response = await fetch('http://localhost:8080/admin/settlement/execute', {
    method: 'POST'
  });
  const result = await response.json();

  if (result.success) {
    console.log(`结算完成: ${result.data.settled_accounts}个账户成功`);
    if (result.data.force_closed_accounts.length > 0) {
      console.warn('强平账户:', result.data.force_closed_accounts);
    }
  }

  return result;
}

// 使用
const result = await dailySettlement([
  { instrument_id: 'IF2501', settlement_price: 3850.0 },
  { instrument_id: 'IH2501', settlement_price: 2650.0 }
]);
```

---

### 10. 获取结算历史

**GET** `/admin/settlement/history`

查询历史结算记录。

**查询参数**:
- `start_date` (string, optional): 开始日期（YYYY-MM-DD）
- `end_date` (string, optional): 结束日期（YYYY-MM-DD）

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "settlement_date": "2025-10-05",
      "total_accounts": 1250,
      "settled_accounts": 1247,
      "failed_accounts": 3,
      "force_closed_accounts": ["user123"],
      "total_commission": 152340.50,
      "total_profit": -234560.75
    },
    {
      "settlement_date": "2025-10-04",
      "total_accounts": 1248,
      "settled_accounts": 1248,
      "failed_accounts": 0,
      "force_closed_accounts": [],
      "total_commission": 145230.20,
      "total_profit": 123450.00
    }
  ],
  "error": null
}
```

**示例**:
```javascript
// JavaScript - 查询最近一周的结算记录
async function getRecentSettlements(days = 7) {
  const endDate = new Date();
  const startDate = new Date();
  startDate.setDate(startDate.getDate() - days);

  const params = new URLSearchParams({
    start_date: startDate.toISOString().split('T')[0],
    end_date: endDate.toISOString().split('T')[0]
  });

  const response = await fetch(
    `http://localhost:8080/admin/settlement/history?${params}`
  );
  return await response.json();
}

const history = await getRecentSettlements(7);
```

---

### 11. 获取结算详情

**GET** `/admin/settlement/detail/{date}`

查询指定日期的结算详情。

**路径参数**:
- `date` (string, required): 结算日期（YYYY-MM-DD）

**响应**:
```json
{
  "success": true,
  "data": {
    "settlement_date": "2025-10-05",
    "total_accounts": 1250,
    "settled_accounts": 1247,
    "failed_accounts": 3,
    "force_closed_accounts": ["user123", "user456"],
    "total_commission": 152340.50,
    "total_profit": -234560.75
  },
  "error": null
}
```

**示例**:
```bash
curl http://localhost:8080/admin/settlement/detail/2025-10-05
```

---

## 风控管理 API

### 12. 获取风险账户列表

**GET** `/api/management/risk/accounts`

获取风险账户列表（风险度较高的账户）。

**查询参数**:
- `risk_level` (string, optional): 风险等级筛选（low/medium/high/critical）

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "account_id": "ACC_xxx",
      "user_name": "高风险账户",
      "risk_ratio": 0.85,
      "risk_level": "High",
      "margin": 850000.0,
      "available": 150000.0,
      "balance": 1000000.0,
      "warning_threshold": 0.8
    }
  ],
  "error": null
}
```

**字段说明**:
- `risk_level`: 风险等级
  - `Low`: 低风险（risk_ratio < 0.6）
  - `Medium`: 中风险（0.6 ≤ risk_ratio < 0.8）
  - `High`: 高风险（0.8 ≤ risk_ratio < 1.0）
  - `Critical`: 极高风险（risk_ratio ≥ 1.0）
- `warning_threshold`: 预警阈值

**示例**:
```bash
# 查询所有高风险账户
curl 'http://localhost:8080/api/management/risk/accounts?risk_level=high'
```

```javascript
// JavaScript
async function getRiskAccounts(riskLevel = null) {
  let url = 'http://localhost:8080/api/management/risk/accounts';
  if (riskLevel) {
    url += `?risk_level=${riskLevel}`;
  }
  const response = await fetch(url);
  return await response.json();
}

// 使用 - 查询高风险账户
const highRiskAccounts = await getRiskAccounts('high');
console.log(`高风险账户数量: ${highRiskAccounts.data.length}`);

// 使用 - 查询所有风险账户
const allRiskAccounts = await getRiskAccounts();
```

---

### 13. 获取保证金汇总

**GET** `/api/management/risk/margin-summary`

获取全系统保证金监控汇总数据。

**响应**:
```json
{
  "success": true,
  "data": {
    "total_accounts": 1250,
    "total_margin": 45000000.0,
    "total_balance": 125000000.0,
    "average_risk_ratio": 0.36,
    "high_risk_count": 15,
    "critical_risk_count": 3,
    "low_risk_count": 1150,
    "medium_risk_count": 82
  },
  "error": null
}
```

**字段说明**:
- `total_margin`: 全系统占用保证金总额
- `total_balance`: 全系统账户权益总额
- `average_risk_ratio`: 平均风险度
- `*_risk_count`: 各风险等级账户数量

**示例**:
```javascript
// JavaScript
async function getMarginSummary() {
  const response = await fetch('http://localhost:8080/api/management/risk/margin-summary');
  return await response.json();
}

// 使用
const summary = await getMarginSummary();
console.log(`系统平均风险度: ${(summary.data.average_risk_ratio * 100).toFixed(2)}%`);
console.log(`高风险账户: ${summary.data.high_risk_count}个`);
console.log(`极高风险账户: ${summary.data.critical_risk_count}个`);
```

---

### 14. 获取强平记录

**GET** `/api/management/risk/liquidations`

获取强平记录历史。

**查询参数**:
- `start_date` (string, optional): 开始日期（格式：2024-01-01）
- `end_date` (string, optional): 结束日期（格式：2024-12-31）

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "record_id": "LIQ_xxx",
      "account_id": "ACC_xxx",
      "user_name": "被强平账户",
      "liquidation_time": 1704067200000,
      "pre_balance": 100000.0,
      "post_balance": 5000.0,
      "loss": 95000.0,
      "risk_ratio_before": 1.05,
      "reason": "风险率超过100%",
      "closed_positions": [
        {
          "instrument_id": "IF2501",
          "volume": 10,
          "close_price": 3750.0,
          "loss": 50000.0
        }
      ]
    }
  ],
  "error": null
}
```

**字段说明**:
- `loss`: 强平导致的亏损金额
- `risk_ratio_before`: 强平前的风险度
- `closed_positions`: 被强平的持仓列表

**示例**:
```javascript
// JavaScript
async function getLiquidationRecords(startDate, endDate) {
  const params = new URLSearchParams();
  if (startDate) params.append('start_date', startDate);
  if (endDate) params.append('end_date', endDate);

  const url = `http://localhost:8080/api/management/risk/liquidations?${params}`;
  const response = await fetch(url);
  return await response.json();
}

// 使用 - 查询最近30天的强平记录
const endDate = new Date().toISOString().split('T')[0];
const startDate = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000)
  .toISOString()
  .split('T')[0];

const records = await getLiquidationRecords(startDate, endDate);
console.log(`最近30天强平次数: ${records.data.length}`);

// 计算总损失
const totalLoss = records.data.reduce((sum, r) => sum + r.loss, 0);
console.log(`总损失: ${totalLoss.toLocaleString()}`);
```

---

### 15. 强制平仓（管理端）

**POST** `/api/management/risk/force-liquidate`

管理员手动触发强制平仓。

**请求体**:
```json
{
  "account_id": "ACC_xxx",
  "reason": "风险率超过100%，触发强平"
}
```

**字段说明**:
- `account_id` (string, required): 要强平的账户ID
- `reason` (string, optional): 强平原因说明

**响应**:
```json
{
  "success": true,
  "data": {
    "account_id": "ACC_xxx",
    "liquidation_result": "success",
    "closed_positions": [
      {
        "instrument_id": "IF2501",
        "direction": "LONG",
        "volume": 10,
        "close_price": 3750.0,
        "loss": 50000.0
      }
    ],
    "total_loss": 50000.0,
    "remaining_balance": 5000.0
  },
  "error": null
}
```

**错误响应**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 400,
    "message": "Force liquidation failed: No positions to close"
  }
}
```

**示例**:
```javascript
// JavaScript
async function forceLiquidate(accountId, reason) {
  const response = await fetch('http://localhost:8080/api/management/risk/force-liquidate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      account_id: accountId,
      reason
    })
  });
  return await response.json();
}

// 使用
try {
  const result = await forceLiquidate('ACC_xxx', '风险率达到105%，执行强平');
  console.log('强平成功');
  console.log(`平仓数量: ${result.data.closed_positions.length}`);
  console.log(`总损失: ${result.data.total_loss}`);
  console.log(`剩余资金: ${result.data.remaining_balance}`);
} catch (error) {
  console.error('强平失败:', error.message);
}
```

---

## 系统监控 API

### 15. 系统状态监控

**GET** `/monitoring/system`

获取系统运行状态（CPU、内存、磁盘等）。

**响应**:
```json
{
  "success": true,
  "data": {
    "cpu_usage": 45.2,
    "memory_usage": 62.5,
    "disk_usage": 35.8,
    "uptime": 86400,
    "process_count": 125
  },
  "error": null
}
```

---

### 16. 存储监控

**GET** `/monitoring/storage`

获取存储系统状态（WAL、MemTable、SSTable）。

**响应**:
```json
{
  "success": true,
  "data": {
    "wal_size": 524288000,
    "wal_files": 5,
    "memtable_size": 104857600,
    "memtable_entries": 125000,
    "sstable_count": 23,
    "sstable_size": 2147483648
  },
  "error": null
}
```

---

### 17. 账户监控

**GET** `/monitoring/accounts`

获取账户统计数据。

**响应**:
```json
{
  "success": true,
  "data": {
    "total_accounts": 1250,
    "active_accounts": 856,
    "total_balance": 125000000.0,
    "total_margin": 45000000.0
  },
  "error": null
}
```

---

### 18. 订单监控

**GET** `/monitoring/orders`

获取订单统计数据。

**响应**:
```json
{
  "success": true,
  "data": {
    "total_orders": 52340,
    "pending_orders": 1250,
    "filled_orders": 45230,
    "cancelled_orders": 5860
  },
  "error": null
}
```

---

### 19. 成交监控

**GET** `/monitoring/trades`

获取成交统计数据。

**响应**:
```json
{
  "success": true,
  "data": {
    "total_trades": 45230,
    "total_volume": 452300.0,
    "total_turnover": 12345678900.0
  },
  "error": null
}
```

---

### 20. 生成监控报告

**POST** `/monitoring/report`

生成系统监控报告。

**响应**:
```json
{
  "success": true,
  "data": {
    "report_id": "RPT20251005123456",
    "generated_at": 1696500000000
  },
  "error": null
}
```

---

## 市场数据 API

### 21. 获取行情Tick

**GET** `/api/market/tick/{instrument_id}`

获取合约的最新行情。

**响应**:
```json
{
  "success": true,
  "data": {
    "instrument_id": "IF2501",
    "last_price": 3850.0,
    "bid_price": 3849.8,
    "ask_price": 3850.2,
    "volume": 125000,
    "timestamp": 1696500000000
  },
  "error": null
}
```

---

### 22. 获取订单簿

**GET** `/api/market/orderbook/{instrument_id}`

获取合约的订单簿（盘口数据）。

**响应**:
```json
{
  "success": true,
  "data": {
    "instrument_id": "IF2501",
    "bids": [
      { "price": 3849.8, "volume": 50 },
      { "price": 3849.6, "volume": 120 }
    ],
    "asks": [
      { "price": 3850.2, "volume": 80 },
      { "price": 3850.4, "volume": 150 }
    ],
    "timestamp": 1696500000000
  },
  "error": null
}
```

---

### 23. 获取最近成交

**GET** `/api/market/recent-trades/{instrument_id}`

获取合约的最近成交记录。

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "price": 3850.0,
      "volume": 10,
      "direction": "BUY",
      "timestamp": 1696500000000
    }
  ],
  "error": null
}
```

---

### 24. 获取市场订单统计

**GET** `/api/market/order-stats`

获取市场订单统计数据。

**响应**:
```json
{
  "success": true,
  "data": {
    "total_orders": 52340,
    "buy_orders": 26170,
    "sell_orders": 26170
  },
  "error": null
}
```

---

### 25. 获取交易记录

**GET** `/api/market/transactions`

获取全市场交易记录。

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IF2501",
      "price": 3850.0,
      "volume": 10,
      "buyer": "user001",
      "seller": "user002",
      "timestamp": 1696500000000
    }
  ],
  "error": null
}
```

---

## API 速查表

### 账户管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 获取所有账户列表 | GET | `/api/management/accounts` |
| 获取账户详情 | GET | `/api/management/account/{id}/detail` |

### 资金管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 入金（管理端） | POST | `/api/management/deposit` |
| 出金（管理端） | POST | `/api/management/withdraw` |
| 查询资金流水 | GET | `/api/management/transactions/{id}` |

### 合约管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 获取所有合约 | GET | `/admin/instruments` |
| 创建合约 | POST | `/admin/instrument/create` |
| 更新合约 | PUT | `/admin/instrument/{id}/update` |
| 暂停交易 | PUT | `/admin/instrument/{id}/suspend` |
| 恢复交易 | PUT | `/admin/instrument/{id}/resume` |
| 下市合约 | DELETE | `/admin/instrument/{id}/delist` |

### 结算管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 设置结算价 | POST | `/admin/settlement/set-price` |
| 批量设置结算价 | POST | `/admin/settlement/batch-set-prices` |
| 执行日终结算 | POST | `/admin/settlement/execute` |
| 结算历史 | GET | `/admin/settlement/history` |
| 结算详情 | GET | `/admin/settlement/detail/{date}` |

### 风控管理
| 功能 | Method | Endpoint |
|------|--------|----------|
| 风险账户列表 | GET | `/api/management/risk/accounts` |
| 保证金汇总 | GET | `/api/management/risk/margin-summary` |
| 强平记录 | GET | `/api/management/risk/liquidations` |
| 强制平仓 | POST | `/api/management/risk/force-liquidate` |

### 系统监控
| 功能 | Method | Endpoint |
|------|--------|----------|
| 系统状态 | GET | `/monitoring/system` |
| 存储监控 | GET | `/monitoring/storage` |
| 账户监控 | GET | `/monitoring/accounts` |
| 订单监控 | GET | `/monitoring/orders` |
| 成交监控 | GET | `/monitoring/trades` |
| 生成报告 | POST | `/monitoring/report` |

---

**文档版本**: 1.1
**最后更新**: 2025-11-25
**状态**: ✅ 核心功能已实现完成
