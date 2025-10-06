# QAExchange 完整功能场景分析

## 📋 功能场景清单

### 一、用户与账户管理 (User & Account Management)

#### 1.1 用户注册 (User Registration)
**场景描述**: 新用户注册交易所账号
**前端交互**:
- 用户填写注册信息(用户名、邮箱、密码、手机号)
- 提交注册请求
- 显示注册结果

**后端API**:
```
POST /api/user/register
Request: {
  "username": "user001",
  "email": "user@example.com",
  "password": "******",
  "phone": "13800138000"
}
Response: {
  "success": true,
  "data": {
    "user_id": "user001",
    "created_at": "2025-10-04 12:00:00"
  }
}
```

**业务逻辑**:
- 验证用户名唯一性
- 密码加密存储
- 生成用户ID
- 自动创建关联的交易账户

#### 1.2 账户开通 (Account Opening)
**场景描述**: 为注册用户开通交易账户 (通常在注册时自动完成)
**前端交互**:
- 填写初始入金金额
- 选择账户类型(模拟/实盘)
- 提交开户申请

**后端API**:
```
POST /api/account/open
Request: {
  "user_id": "user001",
  "account_name": "user001_trading",
  "initial_balance": 100000.0
}
Response: {
  "success": true,
  "data": {
    "account_id": "user001",
    "balance": 100000.0,
    "created_at": "2025-10-04 12:00:00"
  }
}
```

**业务逻辑**:
- 创建 QA_Account 对象
- 设置初始资金
- 初始化持仓和订单列表
- 注册到 AccountManager

#### 1.3 账户列表查询 (Admin: List All Accounts)
**场景描述**: 管理员查看所有交易账户
**前端页面**: 管理端 - 账户管理
**后端API**:
```
GET /api/admin/accounts?page=1&page_size=20&status=active
Response: {
  "success": true,
  "data": {
    "total": 100,
    "accounts": [
      {
        "user_id": "user001",
        "account_name": "user001_trading",
        "balance": 100000.0,
        "available": 95000.0,
        "margin_used": 5000.0,
        "risk_ratio": 0.05,
        "status": "active",
        "created_at": "2025-10-04 12:00:00"
      }
    ]
  }
}
```

**业务逻辑**:
- 遍历 AccountManager 中的所有账户
- 计算实时风险指标
- 支持分页和筛选

#### 1.4 账户详情查询 (Account Detail)
**场景描述**: 查看单个账户的完整信息
**后端API**:
```
GET /api/account/{user_id}/detail
Response: {
  "success": true,
  "data": {
    "account_info": {...},
    "positions": [...],
    "orders": [...],
    "trades": [...],
    "balance_history": [...]
  }
}
```

---

### 二、资金管理 (Fund Management)

#### 2.1 入金 (Deposit)
**场景描述**: 向交易账户存入资金
**前端交互**:
- 填写入金金额
- 选择入金方式(银行卡/微信/支付宝)
- 提交入金申请

**后端API**:
```
POST /api/account/deposit
Request: {
  "user_id": "user001",
  "amount": 50000.0,
  "method": "bank_transfer",
  "remark": "初始入金"
}
Response: {
  "success": true,
  "data": {
    "transaction_id": "TXN20251004001",
    "user_id": "user001",
    "amount": 50000.0,
    "balance_before": 100000.0,
    "balance_after": 150000.0,
    "created_at": "2025-10-04 12:00:00"
  }
}
```

**业务逻辑**:
- 验证金额合法性
- 更新账户可用资金
- 记录资金流水
- 生成交易凭证

#### 2.2 出金 (Withdrawal)
**场景描述**: 从交易账户提取资金
**前端交互**:
- 填写出金金额
- 选择提现方式
- 验证交易密码
- 提交出金申请

**后端API**:
```
POST /api/account/withdraw
Request: {
  "user_id": "user001",
  "amount": 20000.0,
  "method": "bank_transfer",
  "bank_account": "6222021234567890"
}
Response: {
  "success": true,
  "data": {
    "transaction_id": "TXN20251004002",
    "user_id": "user001",
    "amount": 20000.0,
    "balance_before": 150000.0,
    "balance_after": 130000.0,
    "status": "pending",
    "created_at": "2025-10-04 12:00:00"
  }
}
```

**业务逻辑**:
- 验证可用资金充足
- 检查是否有持仓限制
- 扣除手续费
- 更新账户余额
- 记录资金流水
- 触发审核流程

#### 2.3 资金流水查询 (Transaction History)
**场景描述**: 查看出入金历史记录
**后端API**:
```
GET /api/account/{user_id}/transactions?start_date=2025-10-01&end_date=2025-10-04
Response: {
  "success": true,
  "data": [
    {
      "transaction_id": "TXN20251004001",
      "type": "deposit",
      "amount": 50000.0,
      "balance_before": 100000.0,
      "balance_after": 150000.0,
      "status": "completed",
      "created_at": "2025-10-04 12:00:00"
    },
    {
      "transaction_id": "TXN20251004002",
      "type": "withdrawal",
      "amount": 20000.0,
      "balance_before": 150000.0,
      "balance_after": 130000.0,
      "status": "pending",
      "created_at": "2025-10-04 13:00:00"
    }
  ]
}
```

---

### 三、交易管理 (Trading Management) - 已部分实现

#### 3.1 下单 (Submit Order) ✅
**已实现**: `POST /api/order/submit`

#### 3.2 撤单 (Cancel Order) ✅
**已实现**: `POST /api/order/cancel`

#### 3.3 订单查询 (Query Orders) ✅
**已实现**: `GET /api/order/user/{user_id}`

#### 3.4 持仓查询 (Query Positions) ✅
**已实现**: `GET /api/position/{user_id}`

#### 3.5 成交查询 (Query Trades)
**场景描述**: 查看历史成交记录
**后端API**:
```
GET /api/trade/user/{user_id}?start_date=2025-10-01
Response: {
  "success": true,
  "data": [
    {
      "trade_id": "TRADE001",
      "order_id": "ORDER001",
      "instrument_id": "IF2501",
      "direction": "buy",
      "volume": 2,
      "price": 3856.8,
      "commission": 2.31,
      "trade_time": "2025-10-04 14:30:00"
    }
  ]
}
```

---

### 四、风控管理 (Risk Management)

#### 4.1 风险账户监控 (Risk Account Monitoring)
**场景描述**: 实时监控风险率过高的账户
**前端页面**: 管理端 - 风控监控
**后端API**:
```
GET /api/admin/risk/accounts?risk_level=high
Response: {
  "success": true,
  "data": [
    {
      "user_id": "user005",
      "balance": 50000.0,
      "margin_used": 48000.0,
      "risk_ratio": 0.96,
      "unrealized_pnl": -3000.0,
      "positions": [...]
    }
  ]
}
```

**业务逻辑**:
- 实时计算每个账户的风险率
- 按风险等级分类:
  - 低风险: risk_ratio < 0.6
  - 中风险: 0.6 <= risk_ratio < 0.8
  - 高风险: 0.8 <= risk_ratio < 0.95
  - 临界风险: risk_ratio >= 0.95
- 自动预警和通知

#### 4.2 强平记录查询 (Forced Liquidation History)
**场景描述**: 查看强制平仓历史
**后端API**:
```
GET /api/admin/risk/liquidations?start_date=2025-10-01
Response: {
  "success": true,
  "data": [
    {
      "user_id": "user009",
      "liquidation_time": "2025-10-04 15:00:00",
      "risk_ratio_before": 0.98,
      "positions_closed": [...],
      "total_loss": 5000.0
    }
  ]
}
```

#### 4.3 保证金监控 (Margin Monitoring)
**场景描述**: 监控账户保证金占用情况
**后端API**:
```
GET /api/admin/risk/margin-summary
Response: {
  "success": true,
  "data": {
    "total_accounts": 100,
    "total_margin_used": 5000000.0,
    "total_available": 8000000.0,
    "average_risk_ratio": 0.38,
    "high_risk_count": 5
  }
}
```

---

### 五、结算管理 (Settlement Management) - 已实现 ✅

#### 5.1 设置结算价 ✅
**已实现**: `POST /api/admin/settlement/set-price`

#### 5.2 批量设置结算价 ✅
**已实现**: `POST /api/admin/settlement/batch-set-prices`

#### 5.3 执行日终结算 ✅
**已实现**: `POST /api/admin/settlement/execute`

#### 5.4 结算历史查询 ✅
**已实现**: `GET /api/admin/settlement/history`

---

### 六、合约管理 (Instrument Management) - 已实现 ✅

#### 6.1 获取所有合约 ✅
**已实现**: `GET /api/admin/instruments`

#### 6.2 创建合约 ✅
**已实现**: `POST /api/admin/instrument/create`

#### 6.3 更新合约 ✅
**已实现**: `PUT /api/admin/instrument/{id}/update`

#### 6.4 暂停/恢复/下市合约 ✅
**已实现**: `PUT /api/admin/instrument/{id}/suspend|resume|delist`

---

## 🎯 实现优先级

### Phase 1: 基础用户与资金管理 (本次实现)
1. ✅ 用户注册 (简化版:自动开户)
2. ✅ 账户列表查询 (管理端)
3. ✅ 入金/出金
4. ✅ 资金流水查询

### Phase 2: 风控增强
1. ✅ 风险账户监控
2. ✅ 强平记录查询
3. ✅ 保证金监控

### Phase 3: 前端页面
1. ✅ 用户注册页面
2. ✅ 出入金页面
3. ✅ 账户管理页面 (管理端)
4. ✅ 资金流水页面

### Phase 4: 完善与优化
1. 成交查询
2. 权限控制
3. 数据持久化

---

## 📊 数据模型设计

### FundTransaction (资金流水)
```rust
pub struct FundTransaction {
    pub transaction_id: String,
    pub user_id: String,
    pub transaction_type: TransactionType,  // Deposit, Withdrawal, Commission, PnL
    pub amount: f64,
    pub balance_before: f64,
    pub balance_after: f64,
    pub status: TransactionStatus,  // Pending, Completed, Failed
    pub method: Option<String>,     // bank_transfer, alipay, wechat
    pub remark: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub enum TransactionType {
    Deposit,
    Withdrawal,
    Commission,
    PnL,
    Settlement,
}

pub enum TransactionStatus {
    Pending,
    Completed,
    Failed,
    Cancelled,
}
```

### RiskMonitor (风险监控)
```rust
pub struct RiskAccount {
    pub user_id: String,
    pub balance: f64,
    pub available: f64,
    pub margin_used: f64,
    pub risk_ratio: f64,
    pub unrealized_pnl: f64,
    pub position_count: usize,
    pub risk_level: RiskLevel,
}

pub enum RiskLevel {
    Low,      // < 60%
    Medium,   // 60-80%
    High,     // 80-95%
    Critical, // >= 95%
}
```

---

## 🔄 业务流程图

### 用户注册与开户流程
```
用户注册 → 验证信息 → 创建用户 → 自动开户 → 初始入金 → 开始交易
```

### 入金流程
```
提交入金 → 验证金额 → 更新余额 → 记录流水 → 发送通知
```

### 出金流程
```
提交出金 → 验证可用资金 → 检查持仓 → 扣除金额 → 审核 → 转账 → 记录流水
```

### 风险监控流程
```
实时计算风险率 → 分级预警 → 触发强平 → 记录日志 → 发送通知
```

---

**文档版本**: v1.0
**创建日期**: 2025-10-04
**状态**: ✅ 规划完成,开始实现
