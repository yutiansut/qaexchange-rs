# 数据模型文档

**版本**: v1.0
**更新时间**: 2025-10-05
**文档类型**: 数据结构定义

---

## 📋 目录

1. [账户相关模型](#账户相关模型)
2. [订单相关模型](#订单相关模型)
3. [持仓相关模型](#持仓相关模型)
4. [合约相关模型](#合约相关模型)
5. [结算相关模型](#结算相关模型)
6. [风控相关模型](#风控相关模型)
7. [WebSocket消息模型](#websocket消息模型)
8. [监控相关模型](#监控相关模型)

---

## 账户相关模型

### Account (QIFI格式)

用户端账户信息的标准格式（QUANTAXIS Interface for Finance）。

**Rust定义** (`qars::qaprotocol::qifi::data::Account`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub user_id: String,
    pub currency: String,              // "CNY", "USD"
    pub pre_balance: f64,              // 上日权益
    pub deposit: f64,                  // 入金
    pub withdraw: f64,                 // 出金
    pub WithdrawQuota: f64,            // 可取资金
    pub close_profit: f64,             // 平仓盈亏
    pub commission: f64,               // 手续费
    pub premium: f64,                  // 权利金
    pub static_balance: f64,           // 静态权益
    pub position_profit: f64,          // 持仓盈亏
    pub float_profit: f64,             // 浮动盈亏
    pub balance: f64,                  // 动态权益（总资产）
    pub margin: f64,                   // 占用保证金
    pub frozen_margin: f64,            // 冻结保证金
    pub frozen_commission: f64,        // 冻结手续费
    pub frozen_premium: f64,           // 冻结权利金
    pub available: f64,                // 可用资金
    pub risk_ratio: f64,               // 风险度（0-1）
}
```

**TypeScript定义** (前端):
```typescript
interface Account {
  user_id: string;
  currency: string;
  pre_balance: number;
  deposit: number;
  withdraw: number;
  WithdrawQuota: number;
  close_profit: number;
  commission: number;
  premium: number;
  static_balance: number;
  position_profit: number;
  float_profit: number;
  balance: number;
  margin: number;
  frozen_margin: number;
  frozen_commission: number;
  frozen_premium: number;
  available: number;
  risk_ratio: number;
}
```

**字段说明**:
- `balance`: 动态权益 = 静态权益 + 持仓盈亏
- `available`: 可用资金 = 动态权益 - 占用保证金 - 冻结资金
- `risk_ratio`: 风险度 = 占用保证金 / 动态权益

---

### QA_Account (内部格式)

系统内部使用的完整账户结构（继承自qars）。

**Rust定义** (`qars::qaaccount::account::QA_Account`):
```rust
pub struct QA_Account {
    pub account_cookie: String,        // 账户ID
    pub portfolio_cookie: String,      // 组合ID
    pub user_cookie: String,           // 用户ID
    pub broker: String,                // 券商
    pub market_type: String,           // 市场类型
    pub running_environment: String,   // 运行环境 ("real", "sim")

    // 账户信息
    pub accounts: Account,             // QIFI账户信息
    pub money: f64,                    // 现金
    pub updatetime: String,            // 更新时间
    pub trading_day: String,           // 交易日

    // 持仓和订单
    pub hold: HashMap<String, QA_Position>,      // 持仓表
    pub orders: HashMap<String, QAOrder>,        // 当日订单
    pub dailyorders: HashMap<String, QAOrder>,   // 历史订单
    pub trades: HashMap<String, Trade>,          // 成交记录

    // 银期转账
    pub banks: HashMap<String, QA_QIFITRANSFER>,
    pub transfers: HashMap<String, QA_QIFITRANSFER>,

    // 事件
    pub event: HashMap<String, String>,
    pub settlement: HashMap<String, f64>,
    pub frozen: HashMap<String, f64>,
}
```

**核心方法**:
```rust
impl QA_Account {
    // 账户查询
    pub fn get_accountmessage(&mut self) -> Account;
    pub fn get_qifi_slice(&mut self) -> QIFI;
    pub fn get_mom_slice(&mut self) -> QAMOMSlice;

    // 资金计算
    pub fn get_balance(&mut self) -> f64;           // 实时权益
    pub fn get_available(&mut self) -> f64;         // 可用资金
    pub fn get_margin(&mut self) -> f64;            // 占用保证金
    pub fn get_riskratio(&mut self) -> f64;         // 风险度
    pub fn get_positionprofit(&mut self) -> f64;    // 持仓盈亏

    // 持仓查询
    pub fn get_position(&mut self, code: &str) -> Option<&mut QA_Position>;
    pub fn get_position_unmut(&self, code: &str) -> Option<&QA_Position>;

    // 订单管理
    pub fn receive_order(&mut self, order: QAOrder) -> bool;
    pub fn receive_deal(&mut self, trade: Trade);
}
```

---

### OpenAccountRequest (开户请求)

**Rust定义** (`src/core/account_ext.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAccountRequest {
    pub user_id: String,
    pub user_name: String,
    pub init_cash: f64,
    pub account_type: AccountType,
    pub password: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AccountType {
    Individual = 0,       // 个人账户
    Institutional = 1,    // 机构账户
}
```

---

## 订单相关模型

### QAOrder (订单)

**Rust定义** (`qars::qaprotocol::qifi::data::QAOrder`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAOrder {
    pub seqno: i64,                    // 序号
    pub user_id: String,               // 用户ID
    pub order_id: String,              // 订单ID
    pub exchange_id: String,           // 交易所ID
    pub instrument_id: String,         // 合约代码
    pub direction: Direction,          // 买卖方向
    pub offset: Offset,                // 开平标志
    pub volume_orign: f64,             // 原始数量
    pub price_type: PriceType,         // 价格类型
    pub limit_price: f64,              // 限价
    pub time_condition: TimeCondition, // 时间条件
    pub volume_condition: VolumeCondition,  // 数量条件
    pub insert_date_time: i64,         // 下单时间（纳秒）
    pub exchange_order_id: String,     // 交易所订单ID
    pub status: OrderStatus,           // 订单状态
    pub volume_left: f64,              // 剩余数量
    pub last_msg: String,              // 最后消息
}
```

**枚举定义**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Direction {
    BUY,      // 买入
    SELL,     // 卖出
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Offset {
    OPEN,            // 开仓
    CLOSE,           // 平仓
    CLOSETODAY,      // 平今
    CLOSEYESTERDAY,  // 平昨
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceType {
    LIMIT,     // 限价
    MARKET,    // 市价
    ANY,       // 任意价
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderStatus {
    PendingRisk,       // 等待风控
    PendingRoute,      // 等待路由
    Submitted,         // 已提交
    PartiallyFilled,   // 部分成交
    Filled,            // 全部成交
    Cancelled,         // 已撤单
    Rejected,          // 已拒绝
}
```

**TypeScript定义**:
```typescript
interface Order {
  order_id: string;
  user_id: string;
  instrument_id: string;
  direction: 'BUY' | 'SELL';
  offset: 'OPEN' | 'CLOSE' | 'CLOSETODAY' | 'CLOSEYESTERDAY';
  volume: number;
  price: number;
  order_type: 'LIMIT' | 'MARKET';
  status: 'PendingRisk' | 'Submitted' | 'PartiallyFilled' | 'Filled' | 'Cancelled' | 'Rejected';
  filled_volume: number;
  submit_time: number;
  update_time: number;
}
```

---

### SubmitOrderRequest (下单请求)

**Rust定义** (`src/service/http/models.rs`):
```rust
#[derive(Debug, Deserialize)]
pub struct SubmitOrderRequest {
    pub user_id: String,
    pub instrument_id: String,
    pub direction: String,      // "BUY" | "SELL"
    pub offset: String,         // "OPEN" | "CLOSE"
    pub volume: f64,
    pub price: f64,
    pub order_type: String,     // "LIMIT" | "MARKET"
}
```

---

## 持仓相关模型

### QA_Position (持仓)

**Rust定义** (`qars::qaaccount::account::QA_Position`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QA_Position {
    pub user_id: String,
    pub exchange_id: String,
    pub instrument_id: String,

    // 多头持仓
    pub volume_long_today: f64,        // 多头今仓
    pub volume_long_his: f64,          // 多头昨仓
    pub volume_long: f64,              // 多头总仓
    pub volume_long_frozen_today: f64, // 多头今仓冻结
    pub volume_long_frozen_his: f64,   // 多头昨仓冻结
    pub volume_long_frozen: f64,       // 多头冻结总数
    pub volume_long_yd: f64,           // 多头昨仓（可用）

    // 空头持仓
    pub volume_short_today: f64,
    pub volume_short_his: f64,
    pub volume_short: f64,
    pub volume_short_frozen_today: f64,
    pub volume_short_frozen_his: f64,
    pub volume_short_frozen: f64,
    pub volume_short_yd: f64,

    // 持仓细分
    pub pos_long_his: f64,
    pub pos_long_today: f64,
    pub pos_short_his: f64,
    pub pos_short_today: f64,

    // 成本和价格
    pub open_price_long: f64,          // 多头开仓均价
    pub open_price_short: f64,         // 空头开仓均价
    pub open_cost_long: f64,           // 多头开仓成本
    pub open_cost_short: f64,          // 空头开仓成本
    pub position_price_long: f64,      // 多头持仓均价
    pub position_price_short: f64,     // 空头持仓均价
    pub position_cost_long: f64,       // 多头持仓成本
    pub position_cost_short: f64,      // 空头持仓成本

    // 盈亏和保证金
    pub last_price: f64,               // 最新价
    pub float_profit_long: f64,        // 多头浮动盈亏
    pub float_profit_short: f64,       // 空头浮动盈亏
    pub float_profit: f64,             // 总浮动盈亏
    pub position_profit_long: f64,     // 多头持仓盈亏
    pub position_profit_short: f64,    // 空头持仓盈亏
    pub position_profit: f64,          // 总持仓盈亏
    pub margin_long: f64,              // 多头保证金
    pub margin_short: f64,             // 空头保证金
    pub margin: f64,                   // 总保证金
}
```

**核心方法**:
```rust
impl QA_Position {
    pub fn volume_long_unmut(&self) -> f64;     // 多头总量（不可变）
    pub fn volume_short_unmut(&self) -> f64;    // 空头总量（不可变）
}
```

**TypeScript定义**:
```typescript
interface Position {
  instrument_id: string;
  volume_long: number;
  volume_short: number;
  cost_long: number;
  cost_short: number;
  profit_long: number;
  profit_short: number;
  margin: number;
}
```

---

## 合约相关模型

### InstrumentInfo (合约信息)

**Rust定义** (`src/exchange/instrument_registry.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentInfo {
    pub instrument_id: String,         // 合约代码
    pub instrument_name: String,       // 合约名称
    pub instrument_type: InstrumentType,
    pub exchange: String,              // 交易所
    pub contract_multiplier: i32,      // 合约乘数
    pub price_tick: f64,               // 最小变动价位
    pub margin_rate: f64,              // 保证金率
    pub commission_rate: f64,          // 手续费率
    pub limit_up_rate: f64,            // 涨停幅度
    pub limit_down_rate: f64,          // 跌停幅度
    pub list_date: Option<String>,     // 上市日期
    pub expire_date: Option<String>,   // 到期日期
    pub status: InstrumentStatus,      // 合约状态
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstrumentType {
    Future,   // 期货
    Option,   // 期权
    Stock,    // 股票
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstrumentStatus {
    Trading,    // 交易中
    Suspended,  // 已暂停
    Delisted,   // 已下市
}
```

**TypeScript定义**:
```typescript
interface Instrument {
  instrument_id: string;
  instrument_name: string;
  instrument_type: 'Future' | 'Option' | 'Stock';
  exchange: string;
  contract_multiplier: number;
  price_tick: number;
  margin_rate: number;
  commission_rate: number;
  limit_up_rate: number;
  limit_down_rate: number;
  list_date?: string;
  expire_date?: string;
  status: 'Trading' | 'Suspended' | 'Delisted';
}
```

---

## 结算相关模型

### SettlementResult (结算结果)

**Rust定义** (`src/exchange/settlement.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    pub settlement_date: String,             // 结算日期
    pub total_accounts: usize,               // 总账户数
    pub settled_accounts: usize,             // 成功结算数
    pub failed_accounts: usize,              // 失败结算数
    pub force_closed_accounts: Vec<String>,  // 强平账户列表
    pub total_commission: f64,               // 总手续费
    pub total_profit: f64,                   // 总盈亏
}
```

**TypeScript定义**:
```typescript
interface SettlementResult {
  settlement_date: string;
  total_accounts: number;
  settled_accounts: number;
  failed_accounts: number;
  force_closed_accounts: string[];
  total_commission: number;
  total_profit: number;
}
```

---

### AccountSettlement (账户结算信息)

**Rust定义** (`src/exchange/settlement.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettlement {
    pub user_id: String,
    pub date: String,
    pub close_profit: f64,       // 平仓盈亏
    pub position_profit: f64,    // 持仓盈亏
    pub commission: f64,         // 手续费
    pub pre_balance: f64,        // 结算前权益
    pub balance: f64,            // 结算后权益
    pub risk_ratio: f64,         // 风险度
    pub force_close: bool,       // 是否强平
}
```

---

## 风控相关模型

### RiskAccount (风险账户)

**规划定义** (`src/exchange/risk_monitor.rs` - 待实现):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAccount {
    pub user_id: String,
    pub user_name: String,
    pub balance: f64,
    pub margin: f64,
    pub available: f64,
    pub risk_ratio: f64,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Normal,    // 正常 (< 50%)
    Warning,   // 警告 (50%-80%)
    High,      // 高风险 (80%-100%)
    Critical,  // 强平 (>= 100%)
}
```

---

## WebSocket消息模型

### ClientMessage (客户端消息)

**Rust定义** (`src/service/websocket/messages.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    // 认证
    Auth {
        user_id: String,
        token: String,
    },

    // 订阅
    Subscribe {
        channels: Vec<String>,       // ["trade", "orderbook", "account"]
        instruments: Vec<String>,    // ["IF2501", "IH2501"]
    },

    // 交易
    SubmitOrder {
        instrument_id: String,
        direction: String,
        offset: String,
        volume: f64,
        price: f64,
        order_type: String,
    },

    CancelOrder {
        order_id: String,
    },

    // 查询
    QueryAccount,
    QueryOrders,
    QueryPositions,

    // 心跳
    Ping,
}
```

---

### ServerMessage (服务端消息)

**Rust定义** (`src/service/websocket/messages.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    // 认证响应
    AuthResponse {
        success: bool,
        user_id: String,
        message: String,
    },

    // 实时推送
    Trade {
        trade_id: String,
        instrument_id: String,
        price: f64,
        volume: f64,
        direction: String,
        timestamp: i64,
    },

    OrderStatus {
        order_id: String,
        status: String,
        filled_volume: f64,
        timestamp: i64,
    },

    AccountUpdate {
        balance: f64,
        available: f64,
        margin_used: f64,
        risk_ratio: f64,
    },

    OrderBook {
        instrument_id: String,
        bids: Vec<(f64, f64)>,  // [(price, volume), ...]
        asks: Vec<(f64, f64)>,
        timestamp: i64,
    },

    Tick {
        instrument_id: String,
        last_price: f64,
        bid_price: f64,
        ask_price: f64,
        volume: f64,
        timestamp: i64,
    },

    // 心跳
    Pong,

    // 错误
    Error {
        code: i32,
        message: String,
    },
}
```

---

## 监控相关模型

### SystemStatus (系统状态)

**Rust定义** (`src/service/http/monitoring.rs`):
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
    pub uptime: u64,
    pub process_count: u32,
}
```

---

### StorageStatus (存储状态)

**Rust定义** (`src/service/http/monitoring.rs`):
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct StorageStatus {
    pub wal_size: u64,
    pub wal_files: usize,
    pub memtable_size: u64,
    pub memtable_entries: usize,
    pub sstable_count: usize,
    pub sstable_size: u64,
}
```

---

### AccountStats (账户统计)

**Rust定义** (`src/service/http/monitoring.rs`):
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountStats {
    pub total_accounts: usize,
    pub active_accounts: usize,
    pub total_balance: f64,
    pub total_margin: f64,
}
```

---

## 类型映射表

### Rust ↔ TypeScript

| 概念 | Rust | TypeScript |
|------|------|-----------|
| 字符串 | `String` | `string` |
| 整数 | `i32`, `i64`, `usize` | `number` |
| 浮点数 | `f32`, `f64` | `number` |
| 布尔值 | `bool` | `boolean` |
| 可选值 | `Option<T>` | `T \| null \| undefined` |
| 数组 | `Vec<T>` | `T[]` | `Array<T>` |
| 哈希表 | `HashMap<K, V>` | `Record<K, V>` | `Map<K, V>` |
| 枚举 | `enum Foo { A, B }` | `type Foo = 'A' \| 'B'` |
| 结构体 | `struct Foo { x: i32 }` | `interface Foo { x: number }` |

---

### 日期时间

| 格式 | Rust | TypeScript | 示例 |
|------|------|-----------|------|
| 日期字符串 | `String` | `string` | `"2025-10-05"` |
| 日期时间 | `String` | `string` | `"2025-10-05 12:30:45"` |
| Unix时间戳（秒） | `i64` | `number` | `1696500000` |
| Unix时间戳（毫秒） | `i64` | `number` | `1696500000000` |
| Unix时间戳（纳秒） | `i64` | `number` | `1696500000000000000` |

---

## 数据流转换

### 账户查询流程

```
1. HTTP请求
   GET /api/account/user001
   ↓
2. 获取QA_Account
   account_mgr.get_account("user001")
   → Arc<RwLock<QA_Account>>
   ↓
3. 转换为QIFI格式
   account.write().get_accountmessage()
   → Account
   ↓
4. 序列化为JSON
   serde_json::to_string(&account)
   → String
   ↓
5. HTTP响应
   {
     "success": true,
     "data": { ... },
     "error": null
   }
```

### 订单提交流程

```
1. HTTP请求 (JSON)
   {
     "user_id": "user001",
     "instrument_id": "IF2501",
     "direction": "BUY",
     "offset": "OPEN",
     "volume": 10,
     "price": 3850.0,
     "order_type": "LIMIT"
   }
   ↓
2. 反序列化
   serde_json::from_str::<SubmitOrderRequest>(body)
   → SubmitOrderRequest
   ↓
3. 转换为QAOrder
   QAOrder::from_request(req)
   → QAOrder
   ↓
4. 提交到撮合引擎
   order_router.submit_order(order)
   → Result<String, ExchangeError>
   ↓
5. 返回订单ID
   {
     "success": true,
     "data": { "order_id": "..." },
     "error": null
   }
```

---

## 数据验证规则

### 账户相关

| 字段 | 规则 |
|------|------|
| user_id | 非空，长度3-32，字母数字 |
| init_cash | >= 0 |
| balance | >= 0 |
| available | >= 0 |
| risk_ratio | 0 <= ratio <= 10 |

### 订单相关

| 字段 | 规则 |
|------|------|
| instrument_id | 非空，存在于合约列表 |
| direction | "BUY" \| "SELL" |
| offset | "OPEN" \| "CLOSE" \| "CLOSETODAY" |
| volume | > 0, 整数倍 |
| price | > 0, 符合价格tick |
| order_type | "LIMIT" \| "MARKET" |

### 合约相关

| 字段 | 规则 |
|------|------|
| contract_multiplier | > 0 |
| price_tick | > 0 |
| margin_rate | 0 < rate <= 1 |
| commission_rate | >= 0 |
| limit_up_rate | > 0 |
| limit_down_rate | > 0 |

---

**文档版本**: 1.0
**最后更新**: 2025-10-05
**维护者**: QAExchange Team
**下一步**: 补充示例代码和字段详细说明
