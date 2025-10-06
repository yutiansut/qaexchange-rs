# 期货交易机制详解

## 目录

1. [期货交易基础](#期货交易基础)
2. [Towards值系统](#towards值系统)
3. [订单生命周期](#订单生命周期)
4. [保证金与资金管理](#保证金与资金管理)
5. [盈亏计算](#盈亏计算)
6. [Sim模式 vs Real模式](#sim模式-vs-real模式)
7. [完整交易案例](#完整交易案例)

---

## 期货交易基础

### 期货合约特点

期货交易与股票交易的核心区别：

| 特性 | 股票 | 期货 |
|------|------|------|
| 交易方向 | 只能买入（做多） | 可以买入/卖出（双向） |
| 持仓性质 | 长期持有 | 有到期日，需平仓 |
| 杠杆 | 无杠杆 | 有保证金杠杆（5%-20%） |
| 交易目的 | 买入后卖出获利 | 开仓→平仓获利（多空双向） |

### 四种基本操作

```
┌─────────────┬──────────────────────────────────────┐
│  操作类型   │              说明                    │
├─────────────┼──────────────────────────────────────┤
│ 买入开仓    │ 建立多头持仓（看涨）                  │
│ (BUY OPEN)  │ 预期价格上涨，低价买入，高价平仓      │
├─────────────┼──────────────────────────────────────┤
│ 卖出开仓    │ 建立空头持仓（看跌）                  │
│ (SELL OPEN) │ 预期价格下跌，高价卖出，低价平仓      │
├─────────────┼──────────────────────────────────────┤
│ 卖出平仓    │ 平掉多头持仓（结束多头头寸）          │
│ (SELL CLOSE)│ 必须先有多头持仓才能平仓              │
├─────────────┼──────────────────────────────────────┤
│ 买入平仓    │ 平掉空头持仓（结束空头头寸）          │
│ (BUY CLOSE) │ 必须先有空头持仓才能平仓              │
└─────────────┴──────────────────────────────────────┘
```

### 完整交易周期

#### 多头交易（看涨）

```
┌──────────────┐          ┌──────────────┐
│  买入开仓    │   持有   │  卖出平仓    │
│  BUY OPEN    │  ─────→  │  SELL CLOSE  │
│  100.0 元    │          │  100.5 元    │
└──────────────┘          └──────────────┘
     ↓                          ↓
   冻结保证金              释放保证金 + 盈利
   (100.0 * 10 * 10%)      盈利 = (100.5 - 100.0) * 10
   = 1000 元               = 5.0 元
```

#### 空头交易（看跌）

```
┌──────────────┐          ┌──────────────┐
│  卖出开仓    │   持有   │  买入平仓    │
│  SELL OPEN   │  ─────→  │  BUY CLOSE   │
│  100.0 元    │          │  99.5 元     │
└──────────────┘          └──────────────┘
     ↓                          ↓
   冻结保证金              释放保证金 + 盈利
   (100.0 * 10 * 10%)      盈利 = (100.0 - 99.5) * 10
   = 1000 元               = 5.0 元
```

---

## Towards值系统

### 设计背景

QARS框架使用 `towards` 参数统一表示**方向(direction) + 开平标志(offset)**，这是中国期货交易的标准做法（参考CTP、飞马等柜台系统）。

### Towards值映射表

```rust
// qars/src/qaaccount/account.rs
match towards {
    1 | 2 => {
        // BUY + OPEN (买入开仓，建立多头)
        // 1: 标准开多
        // 2: 平今不可用时的开多（兼容期货当日平仓限制）
    }
    3 => {
        // BUY + CLOSE (买入平仓，平掉空头)
        // 必须先有空头持仓
    }
    4 => {
        // BUY + CLOSETODAY (买入平今，平掉今日空头)
        // 特定交易所规则（上期所）
    }
    -1 => {
        // SELL + CLOSE (yesterday) (卖出平昨，平掉昨日多头)
        // 只平历史持仓，不平今日开仓
    }
    -2 => {
        // SELL + OPEN (卖出开仓，建立空头)
        // 标准开空操作
    }
    -3 => {
        // SELL + CLOSE (卖出平仓，平掉多头)
        // 优先平今，再平昨（符合交易所规则）
    }
    -4 => {
        // SELL + CLOSETODAY (卖出平今，平掉今日多头)
        // 特定交易所规则（上期所）
    }
}
```

### Direction + Offset → Towards 转换

```rust
// 标准转换逻辑（qaexchange-rs使用）
let towards = match (direction, offset) {
    (OrderDirection::BUY, OrderOffset::OPEN) => 1,      // 买入开仓
    (OrderDirection::SELL, OrderOffset::OPEN) => -2,    // 卖出开仓
    (OrderDirection::BUY, OrderOffset::CLOSE) => 3,     // 买入平仓（平空头）
    (OrderDirection::SELL, OrderOffset::CLOSE) => -3,   // 卖出平仓（平多头）
};

// 数值表示（IPC消息）
pub enum OrderDirection {
    BUY = 0,
    SELL = 1,
}

pub enum OrderOffset {
    OPEN = 0,
    CLOSE = 1,
}
```

### 为什么用1和-2，而不是1和-1？

| Towards | 含义 | 原因 |
|---------|------|------|
| **1** | BUY OPEN | 最常用的开多操作，使用最小正整数 |
| **-2** | SELL OPEN | **-1被SELL CLOSE(yesterday)占用**，表示只平昨日多头持仓 |
| **3** | BUY CLOSE | 买入平仓，平掉空头 |
| **-3** | SELL CLOSE | 卖出平仓，平掉多头（优先平今） |

**核心原因**：中国期货市场有"平今/平昨"的区别，部分交易所（如上期所）对平今仓收取更低的手续费，因此需要区分：
- `-1`: 只平昨日多头（SELL CLOSE yesterday）
- `-3`: 优先平今日多头，再平昨日多头（SELL CLOSE，标准平仓）

---

## 订单生命周期

### Sim模式完整流程（8步）

```
┌─────────────────────────────────────────────────────────────────┐
│                         订单生命周期                             │
└─────────────────────────────────────────────────────────────────┘

1️⃣ Client 发送订单
   ↓
   OrderRequest {
       user_id: "user_01",
       instrument_id: "IX2401",
       direction: BUY (0),
       offset: OPEN (0),
       price: 100.0,
       volume: 10.0,
   }

2️⃣ Gateway 接收订单 → 路由到 AccountSystem
   ↓
   account.send_order(
       code: "IX2401",
       volume: 10.0,
       datetime: "2025-10-03 10:30:00",
       towards: 1,          // BUY OPEN
       price: 100.0,
   )

3️⃣ AccountSystem 风控校验
   ✓ 检查可用资金 (available >= margin_required)
   ✓ 冻结保证金 (frozen += margin_required)
   ✓ 生成 order_id (UUID): "a1b2c3d4-e5f6-..."
   ✓ 记录到 dailyorders

   Order {
       order_id: "a1b2c3d4-e5f6-...",
       exchange_order_id: "",           // 暂时为空
       status: "PENDING",               // 待确认
   }

4️⃣ Gateway 转发到 MatchingEngine
   ↓
   OrderRequest {
       order_id: "a1b2c3d4-e5f6-...",  // 携带账户order_id
       ...
   }

5️⃣ MatchingEngine 撮合
   ✓ 生成 exchange_order_id: "EX_1728123456789_IX2401_B"
   ✓ 订单进入订单簿（Success::Accepted）
   ✓ 发送 OrderAccepted 消息

   OrderAccepted {
       order_id: "a1b2c3d4-e5f6-...",
       exchange_order_id: "EX_1728123456789_IX2401_B",
       timestamp: 1728123456789,
   }

6️⃣ AccountSystem 接收确认 → on_order_confirm()
   ✓ 根据 order_id 查找 dailyorders
   ✓ 更新 exchange_order_id
   ✓ 更新 status: "ALIVE"

   Order {
       order_id: "a1b2c3d4-e5f6-...",
       exchange_order_id: "EX_1728123456789_IX2401_B",
       status: "ALIVE",                 // 已确认
   }

7️⃣ MatchingEngine 撮合成交
   ✓ 匹配到对手盘
   ✓ 发送 TradeReport

   TradeReport {
       trade_id: "TRADE_123456",
       order_id: "a1b2c3d4-e5f6-...",           // 用于匹配账户订单
       exchange_order_id: "EX_1728123456789_IX2401_B",  // 用于行情推送
       user_id: "user_01",
       instrument_id: "IX2401",
       direction: BUY (0),
       offset: OPEN (0),
       price: 100.0,
       volume: 10.0,
       commission: 0.5,
   }

8️⃣ AccountSystem 接收成交 → receive_deal_sim()
   ✓ 根据 order_id 匹配 dailyorders
   ✓ 更新持仓（volume_long += 10）
   ✓ 释放冻结保证金
   ✓ 重新计算实际占用保证金
   ✓ 更新 status: "FILLED"

   Position {
       code: "IX2401",
       volume_long: 10.0,               // 多头持仓
       volume_short: 0.0,
       cost_long: 100.0,                // 开仓均价
   }
```

### 两层ID的作用

| ID类型 | 生成者 | 格式 | 作用 |
|--------|--------|------|------|
| **order_id** | AccountSystem | UUID (36字符) | 匹配账户内部的 dailyorders |
| **exchange_order_id** | MatchingEngine | `EX_{timestamp}_{code}_{dir}` | 全局唯一，用于行情推送和审计 |

**为什么需要两层ID？**

1. **账户匹配**：账户系统需要用 `order_id` 匹配自己生成的订单
2. **全局唯一**：交易所需要 `exchange_order_id` 保证全局不重复（单日内）
3. **行情推送**：使用 `exchange_order_id` 推送逐笔成交，避免暴露用户UUID

---

## 保证金与资金管理

### 保证金计算规则

```rust
// 期货保证金计算（qaexchange使用10%保证金率）
margin_required = price * volume * contract_multiplier * margin_rate

// 示例：IX2401 (中证1000期货)
// price: 100.0
// volume: 10手
// contract_multiplier: 200 (每手200元)
// margin_rate: 10%
margin_required = 100.0 * 10 * 200 * 0.10 = 20,000 元
```

### 资金流转（Sim模式）

#### 1. 开仓阶段（BUY OPEN / SELL OPEN）

```rust
// send_order() 执行：
available -= margin_required;  // 减少可用资金
frozen += margin_required;     // 增加冻结资金

// 账户状态：
balance: 1,000,000.0          // 总资金不变
available: 980,000.0          // 可用资金减少
frozen: 20,000.0              // 冻结保证金
margin: 0.0                   // 实际占用为0（未成交）
```

#### 2. 成交确认阶段（receive_deal_sim）

```rust
// receive_deal_sim() 执行：
frozen -= margin_required;    // 释放冻结
margin += actual_margin;      // 实际占用保证金
position.volume_long += 10;   // 更新持仓

// 账户状态：
balance: 1,000,000.0          // 总资金不变
available: 980,000.0          // 可用资金保持
frozen: 0.0                   // 冻结释放
margin: 20,000.0              // 实际占用保证金
```

#### 3. 平仓阶段（SELL CLOSE / BUY CLOSE）

```rust
// 平仓成交后：
margin -= closed_margin;      // 释放占用的保证金
available += closed_margin + profit;  // 释放保证金 + 盈亏

// 示例：盈利平仓
// 开仓价: 100.0, 平仓价: 100.5, 手数: 10
profit = (100.5 - 100.0) * 10 * 200 = 1,000 元

// 账户状态：
balance: 1,001,000.0          // 总资金增加（含盈利）
available: 1,001,000.0        // 可用资金恢复 + 盈利
frozen: 0.0
margin: 0.0                   // 持仓为0，保证金释放
```

### Real模式差异

| 操作 | Sim模式 | Real模式 |
|------|---------|----------|
| 开仓 | `send_order()` 立即冻结保证金 | `send_order()` 冻结保证金 |
| 成交 | `receive_deal_sim()` 直接更新持仓 | `receive_simpledeal_transaction()` 扣除手续费 |
| 平仓 | 直接计算盈亏 | 需要匹配历史成交（FIFO） |
| 手续费 | 成交时计算 | 实时扣除 |

---

## 盈亏计算

### 多头盈亏（Long Position）

```
盈亏 = (平仓价 - 开仓价) * 手数 * 合约乘数 - 手续费

示例：
开仓：BUY OPEN @ 100.0, 10手
平仓：SELL CLOSE @ 100.5, 10手
合约乘数：200
手续费：开仓0.5 + 平仓0.5 = 1.0

盈亏 = (100.5 - 100.0) * 10 * 200 - 1.0
     = 0.5 * 10 * 200 - 1.0
     = 1000 - 1.0
     = 999.0 元
```

### 空头盈亏（Short Position）

```
盈亏 = (开仓价 - 平仓价) * 手数 * 合约乘数 - 手续费

示例：
开仓：SELL OPEN @ 100.0, 10手
平仓：BUY CLOSE @ 99.5, 10手
合约乘数：200
手续费：开仓0.5 + 平仓0.5 = 1.0

盈亏 = (100.0 - 99.5) * 10 * 200 - 1.0
     = 0.5 * 10 * 200 - 1.0
     = 1000 - 1.0
     = 999.0 元
```

### 持仓盈亏计算（浮动盈亏）

```rust
// 计算当前持仓的浮动盈亏
pub fn calculate_float_profit(&self, code: &str, current_price: f64) -> f64 {
    if let Some(position) = self.positions.get(code) {
        let long_profit = (current_price - position.cost_long)
                          * position.volume_long
                          * contract_multiplier;

        let short_profit = (position.cost_short - current_price)
                           * position.volume_short
                           * contract_multiplier;

        long_profit + short_profit
    } else {
        0.0
    }
}
```

---

## Sim模式 vs Real模式

### 模式对比

| 特性 | Sim模式 | Real模式 |
|------|---------|----------|
| **用途** | 模拟回测、策略测试 | 实盘交易 |
| **资金校验** | 简化校验（仅检查可用资金） | 严格校验（保证金、风险度） |
| **成交处理** | `receive_deal_sim()` | `receive_simpledeal_transaction()` |
| **手续费** | 成交时计算 | 实时扣除 |
| **持仓匹配** | 简化FIFO | 严格FIFO（匹配历史成交） |
| **订单确认** | 需要 `on_order_confirm()` | 需要 `on_order_confirm()` |
| **数据持久化** | 可选 | 必须（数据库） |

### Sim模式特有流程

```rust
// 1. 开仓
account.send_order(code, volume, datetime, towards, price, "", "LIMIT")?;
   ↓
生成 order_id, 冻结保证金, status="PENDING"

// 2. 订单确认
account.on_order_confirm(order_id, exchange_order_id)?;
   ↓
更新 exchange_order_id, status="ALIVE"

// 3. 成交
account.receive_deal_sim(
    order_id,
    exchange_order_id,
    code,
    towards,
    price,
    volume,
    datetime,
)?;
   ↓
更新持仓, 释放冻结, 占用保证金, status="FILLED"
```

### Real模式特有流程

```rust
// 1. 开仓（同Sim）
account.send_order(...)?;

// 2. 订单确认（同Sim）
account.on_order_confirm(order_id, exchange_order_id)?;

// 3. 成交（不同！）
account.receive_simpledeal_transaction(
    order_id,
    code,
    towards,
    price,
    volume,
    datetime,
)?;
   ↓
// Real模式会：
// - 匹配历史成交（FIFO）
// - 实时扣除手续费
// - 更新平仓盈亏
// - 写入数据库
```

---

## 完整交易案例

### 案例1：多头交易（盈利）

```rust
// 账户初始状态
balance: 1,000,000.0
available: 1,000,000.0
frozen: 0.0
margin: 0.0

// ============================================================
// 阶段1：买入开仓（BUY OPEN）
// ============================================================
let order1 = account.send_order(
    "IX2401",           // 中证1000期货
    10.0,               // 10手
    "2025-10-03 09:30:00",
    1,                  // BUY OPEN
    100.0,              // 100.0元/点
    "",
    "LIMIT",
)?;
// order_id: "a1b2c3d4-..."
// status: "PENDING"

// 账户状态：
balance: 1,000,000.0    // 不变
available: 980,000.0    // 减少20,000（冻结保证金）
frozen: 20,000.0        // 冻结
margin: 0.0

// MatchingEngine 确认
account.on_order_confirm("a1b2c3d4-...", "EX_1728123456789_IX2401_B")?;
// status: "ALIVE"

// MatchingEngine 撮合成交
account.receive_deal_sim(
    "a1b2c3d4-...",
    "EX_1728123456789_IX2401_B",
    "IX2401",
    1,              // BUY OPEN
    100.0,
    10.0,
    "2025-10-03 09:30:05",
)?;

// 账户状态：
balance: 999,999.5      // 扣除手续费0.5
available: 979,999.5    // 可用资金
frozen: 0.0             // 冻结释放
margin: 20,000.0        // 实际占用保证金

// 持仓状态：
position.code: "IX2401"
position.volume_long: 10.0      // 多头持仓10手
position.cost_long: 100.0       // 开仓均价100.0

// ============================================================
// 阶段2：卖出平仓（SELL CLOSE），价格上涨到100.5
// ============================================================
let order2 = account.send_order(
    "IX2401",
    10.0,
    "2025-10-03 14:00:00",
    -3,                 // SELL CLOSE
    100.5,
    "",
    "LIMIT",
)?;
// order_id: "b2c3d4e5-..."

// 账户状态（平仓订单冻结资金为0，因为是平仓）：
balance: 999,999.5
available: 979,999.5
frozen: 0.0             // 平仓不需要冻结保证金
margin: 20,000.0

// MatchingEngine 确认
account.on_order_confirm("b2c3d4e5-...", "EX_1728123467890_IX2401_S")?;

// MatchingEngine 撮合成交
account.receive_deal_sim(
    "b2c3d4e5-...",
    "EX_1728123467890_IX2401_S",
    "IX2401",
    -3,             // SELL CLOSE
    100.5,
    10.0,
    "2025-10-03 14:00:05",
)?;

// 盈亏计算：
profit = (100.5 - 100.0) * 10 * 200 = 1,000 元
commission = 0.5

// 账户最终状态：
balance: 1,000,999.0    // 1,000,000 - 0.5(开仓) - 0.5(平仓) + 1,000(盈利)
available: 1,000,999.0  // 全部可用
frozen: 0.0
margin: 0.0             // 持仓为0，保证金释放

// 持仓状态：
position.volume_long: 0.0       // 平仓后持仓为0
```

### 案例2：空头交易（盈利）

```rust
// 账户初始状态
balance: 1,000,000.0
available: 1,000,000.0

// ============================================================
// 阶段1：卖出开仓（SELL OPEN）
// ============================================================
let order1 = account.send_order(
    "IX2401",
    10.0,
    "2025-10-03 09:30:00",
    -2,                 // SELL OPEN（注意：是-2，不是-1！）
    100.0,
    "",
    "LIMIT",
)?;

// 账户状态：
balance: 1,000,000.0
available: 980,000.0    // 冻结保证金
frozen: 20,000.0
margin: 0.0

// 成交后：
account.on_order_confirm("c3d4e5f6-...", "EX_1728123456789_IX2401_S")?;
account.receive_deal_sim(
    "c3d4e5f6-...",
    "EX_1728123456789_IX2401_S",
    "IX2401",
    -2,             // SELL OPEN
    100.0,
    10.0,
    "2025-10-03 09:30:05",
)?;

// 账户状态：
balance: 999,999.5      // 扣除手续费0.5
available: 979,999.5
frozen: 0.0
margin: 20,000.0

// 持仓状态：
position.volume_short: 10.0     // 空头持仓10手
position.cost_short: 100.0      // 开仓均价100.0

// ============================================================
// 阶段2：买入平仓（BUY CLOSE），价格下跌到99.5
// ============================================================
let order2 = account.send_order(
    "IX2401",
    10.0,
    "2025-10-03 14:00:00",
    3,                  // BUY CLOSE
    99.5,
    "",
    "LIMIT",
)?;

// 成交后：
account.on_order_confirm("d4e5f6g7-...", "EX_1728123467890_IX2401_B")?;
account.receive_deal_sim(
    "d4e5f6g7-...",
    "EX_1728123467890_IX2401_B",
    "IX2401",
    3,              // BUY CLOSE
    99.5,
    10.0,
    "2025-10-03 14:00:05",
)?;

// 盈亏计算：
profit = (100.0 - 99.5) * 10 * 200 = 1,000 元
commission = 0.5

// 账户最终状态：
balance: 1,000,999.0    // 盈利1,000，手续费1.0
available: 1,000,999.0
frozen: 0.0
margin: 0.0

// 持仓状态：
position.volume_short: 0.0      // 平仓后持仓为0
```

### 案例3：常见错误 - 使用错误的towards值

```rust
// ❌ 错误：使用 -1 进行卖出开仓
let order_wrong = account.send_order(
    "IX2401",
    10.0,
    "2025-10-03 09:30:00",
    -1,                 // ❌ 错误！-1 是 SELL CLOSE(yesterday)
    100.0,
    "",
    "LIMIT",
)?;

// 结果：
// Error: "SELL CLOSE 仓位不足"
// 原因：-1 表示平掉昨日多头持仓，但账户没有多头持仓

// ✅ 正确：使用 -2 进行卖出开仓
let order_correct = account.send_order(
    "IX2401",
    10.0,
    "2025-10-03 09:30:00",
    -2,                 // ✅ 正确！-2 是 SELL OPEN
    100.0,
    "",
    "LIMIT",
)?;
// 成功建立空头持仓
```

### 案例4：多空对冲持仓

```rust
// 期货允许同时持有多头和空头（锁仓策略）

// 1. 开多头
account.send_order("IX2401", 10.0, datetime, 1, 100.0, "", "LIMIT")?;
// position.volume_long: 10.0

// 2. 开空头（不影响多头持仓）
account.send_order("IX2401", 5.0, datetime, -2, 100.0, "", "LIMIT")?;
// position.volume_long: 10.0
// position.volume_short: 5.0

// 持仓状态：
position.volume_long: 10.0      // 多头10手
position.volume_short: 5.0      // 空头5手
position.volume_long_today: 10.0
position.volume_short_today: 5.0

// 占用保证金：
margin = (10 + 5) * 100.0 * 200 * 0.10 = 30,000 元
// 两个方向都占用保证金（除非交易所支持对锁优惠）
```

---

## 总结

### 关键要点

1. **Towards值选择**：
   - 买入开仓：`1` 或 `2`（推荐 `1`）
   - 卖出开仓：**`-2`**（不是 `-1`！）
   - 买入平仓：`3`
   - 卖出平仓：`-3`

2. **订单流程（Sim模式）**：
   - `send_order()` → 生成order_id，冻结资金
   - `on_order_confirm()` → 更新exchange_order_id
   - `receive_deal_sim()` → 更新持仓，计算盈亏

3. **两层ID设计**：
   - `order_id`: 账户生成，UUID格式，用于匹配dailyorders
   - `exchange_order_id`: 交易所生成，全局唯一，用于行情推送

4. **资金流转**：
   - 开仓：冻结保证金 → 成交后转为占用保证金
   - 平仓：释放保证金 + 盈亏结算

5. **盈亏计算**：
   - 多头：`(平仓价 - 开仓价) * 手数 * 合约乘数`
   - 空头：`(开仓价 - 平仓价) * 手数 * 合约乘数`

### 参考代码位置

- Towards值定义：`qars2/src/qaaccount/account.rs:1166-1220`
- 订单生命周期：`examples/high_performance_demo.rs:112-281`
- 两层ID实现：`src/protocol/ipc_messages.rs:50-80`
- 保证金计算：`qars2/src/qaaccount/account.rs:send_order()`
- 盈亏计算：`qars2/src/qaaccount/account.rs:receive_deal_sim()`
