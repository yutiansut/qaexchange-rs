# 高性能交易所架构设计

## 设计目标

- **订单吞吐**: > 1,000,000 orders/sec
- **撮合延迟**: P99 < 100μs
- **行情延迟**: P99 < 10μs
- **并发账户**: > 100,000
- **零拷贝通信**: iceoryx2 共享内存

## 架构原则（参考上交所/CTP）

### 1. 职责分离

| 系统 | 职责 | 独立性 |
|------|------|--------|
| **撮合引擎** | 订单匹配（价格优先、时间优先） | 独立进程 |
| **账户系统** | 资金/持仓管理 | 独立进程 |
| **风控系统** | 盘前/盘中风控 | 独立服务 |
| **行情系统** | Level1/Level2/逐笔推送 | 独立进程 |
| **交易网关** | WebSocket/HTTP 接入 | 多实例 |

### 2. 通信机制

```
iceoryx2 共享内存（零拷贝）
    ↓
订单请求 → 撮合引擎 → 成交回报 → 账户系统
                      ↓
                   行情系统
```

### 3. 数据流向

```
用户订单流:
Client → Gateway → RiskCheck → OrderRouter
                                    ↓ (iceoryx2)
                              MatchingEngine
                                    ↓ (iceoryx2)
                         ┌──────────┴──────────┐
                         ↓                     ↓
                    AccountSystem         MarketData
                         ↓                     ↓
                    TradeNotify          Subscribers
```

## 核心组件设计

### 组件 1: 撮合引擎核心 (MatchingEngineCore)

**独立进程**，每个品种一个 Orderbook

```rust
// src/matching/core/mod.rs
use qars::qadatastruct::orderbook::{Orderbook, Success, Failure};

pub struct MatchingEngineCore {
    // 订单簿池（每个品种独立）
    orderbooks: DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>,

    // iceoryx2 订单接收器
    order_receiver: Receiver<OrderRequest>,

    // iceoryx2 成交发送器
    trade_sender: Sender<TradeReport>,

    // iceoryx2 行情发送器
    market_sender: Sender<OrderbookSnapshot>,

    // iceoryx2 订单确认发送器（Sim模式）
    accepted_sender: Sender<OrderAccepted>,
}

impl MatchingEngineCore {
    pub fn run(&self) {
        while let Ok(order_req) = self.order_receiver.recv() {
            let instrument_id = std::str::from_utf8(&order_req.instrument_id)
                .unwrap_or("")
                .trim_end_matches('\0');

            // 1. 获取对应的订单簿
            if let Some(orderbook) = self.orderbooks.get(instrument_id) {
                let mut ob = orderbook.write();

                // 2. 撮合（纯内存操作）
                let result = ob.insert_order(
                    order_req.price,
                    order_req.volume,
                    order_req.direction == 0, // true=BUY, false=SELL
                );

                // 3. 处理撮合结果
                match result {
                    Ok(success) => self.handle_success(success, &order_req),
                    Err(failure) => self.handle_failure(failure, &order_req),
                }

                // 4. 发送行情更新（零拷贝）
                let snapshot = self.create_snapshot(&ob, instrument_id);
                let _ = self.market_sender.send(snapshot);
            }
        }
    }

    fn handle_success(&self, success: Success, req: &OrderRequest) {
        match success {
            Success::Accepted { id, ts, .. } => {
                // 订单进入订单簿 → 发送确认消息
                let accepted = self.create_order_accepted(req, ts);
                let _ = self.accepted_sender.send(accepted);

                log::info!("Order accepted: {:?}", id);
            }
            Success::Filled { id, ts, price, volume, trades } => {
                // 完全成交 → 发送成交回报
                for trade in trades {
                    let trade_report = self.create_trade_report(req, &trade, ts);
                    let _ = self.trade_sender.send(trade_report);
                }

                log::info!("Order filled: {:?} @ {} x {}", id, price, volume);
            }
            Success::PartiallyFilled { id, ts, filled_volume, trades, .. } => {
                // 部分成交 → 发送成交回报
                for trade in trades {
                    let trade_report = self.create_trade_report(req, &trade, ts);
                    let _ = self.trade_sender.send(trade_report);
                }

                log::info!("Order partially filled: {:?}, filled {}", id, filled_volume);
            }
        }
    }

    fn create_order_accepted(&self, req: &OrderRequest, timestamp: i64) -> OrderAccepted {
        let mut accepted = OrderAccepted {
            order_id: req.order_id,
            exchange_order_id: [0; 32],
            user_id: req.user_id,
            instrument_id: req.instrument_id,
            timestamp,
            gateway_id: req.gateway_id,
            session_id: req.session_id,
        };

        // 生成全局唯一的 exchange_order_id
        let instrument_id = std::str::from_utf8(&req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let direction_str = if req.direction == 0 { "B" } else { "S" };
        let exchange_order_id = format!("EX_{}_{}{}", timestamp, instrument_id, direction_str);

        let ex_bytes = exchange_order_id.as_bytes();
        let ex_len = ex_bytes.len().min(32);
        accepted.exchange_order_id[..ex_len].copy_from_slice(&ex_bytes[..ex_len]);

        accepted
    }

    fn create_trade_report(&self, req: &OrderRequest, trade: &Trade, timestamp: i64) -> TradeReport {
        let mut report = TradeReport {
            trade_id: [0; 32],
            order_id: req.order_id,
            exchange_order_id: [0; 32],
            user_id: req.user_id,
            instrument_id: req.instrument_id,
            direction: req.direction,
            offset: req.offset,
            price: trade.price,
            volume: trade.volume,
            timestamp,
            commission: trade.volume * 0.05, // 示例手续费
        };

        // 生成 trade_id
        let trade_id = format!("TRADE_{}", timestamp);
        let trade_bytes = trade_id.as_bytes();
        let trade_len = trade_bytes.len().min(32);
        report.trade_id[..trade_len].copy_from_slice(&trade_bytes[..trade_len]);

        // 生成 exchange_order_id（与 OrderAccepted 相同格式）
        let instrument_id = std::str::from_utf8(&req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let direction_str = if req.direction == 0 { "B" } else { "S" };
        let exchange_order_id = format!("EX_{}_{}{}", timestamp, instrument_id, direction_str);

        let ex_bytes = exchange_order_id.as_bytes();
        let ex_len = ex_bytes.len().min(32);
        report.exchange_order_id[..ex_len].copy_from_slice(&ex_bytes[..ex_len]);

        report
    }
}
```

**关键点**：
- **双消息机制**：Success::Accepted → OrderAccepted；Success::Filled → TradeReport
- **exchange_order_id 生成**：格式 `EX_{timestamp}_{instrument}_{direction}`，全局唯一
- **order_id 传递**：从 Gateway 接收并在所有消息中传递，用于账户匹配
- **纯内存操作**：无需锁定账户，专注于撮合逻辑
- **零拷贝通信**：减少序列化开销

### 组件 2: 账户系统 (AccountSystemCore)

**独立进程**，异步更新账户

```rust
// src/account/core/mod.rs
use crossbeam::channel::{Receiver, Sender, select};

pub struct AccountSystemCore {
    // 账户池
    accounts: DashMap<String, Arc<RwLock<QA_Account>>>,

    // iceoryx2 成交订阅器
    trade_receiver: Receiver<TradeReport>,

    // iceoryx2 订单确认订阅器（Sim模式必需）
    accepted_receiver: Receiver<OrderAccepted>,

    // 账户更新通知发送器（可选）
    update_sender: Option<Sender<AccountUpdateNotify>>,

    // 更新队列（批量处理）
    batch_size: usize,
}

impl AccountSystemCore {
    pub fn run(&self) {
        use crossbeam::channel::select;
        let mut update_queue = Vec::with_capacity(self.batch_size);

        loop {
            // 使用 select! 监听多个通道
            select! {
                // 1. 接收订单确认（Sim模式）
                recv(self.accepted_receiver) -> msg => {
                    if let Ok(accepted) = msg {
                        self.handle_order_accepted(accepted);
                    }
                }

                // 2. 接收成交回报
                recv(self.trade_receiver) -> msg => {
                    if let Ok(trade) = msg {
                        update_queue.push(trade);

                        // 达到批量大小，立即处理
                        if update_queue.len() >= self.batch_size {
                            self.batch_update_accounts(&update_queue);
                            update_queue.clear();
                        }
                    }
                }

                // 3. 超时处理（确保队列不会无限等待）
                default(Duration::from_millis(10)) => {
                    if !update_queue.is_empty() {
                        self.batch_update_accounts(&update_queue);
                        update_queue.clear();
                    }
                }
            }
        }
    }

    fn handle_order_accepted(&self, accepted: OrderAccepted) {
        let order_id = std::str::from_utf8(&accepted.order_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let exchange_order_id = std::str::from_utf8(&accepted.exchange_order_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let user_id = std::str::from_utf8(&accepted.user_id)
            .unwrap_or("")
            .trim_end_matches('\0');

        if let Some(account) = self.accounts.get(user_id) {
            let mut acc = account.write();
            if let Err(e) = acc.on_order_confirm(order_id, exchange_order_id) {
                log::error!("Failed to confirm order {}: {}", order_id, e);
            }
        }
    }

    fn batch_update_accounts(&self, update_queue: &[TradeReport]) {
        // 按账户分组
        let mut grouped: HashMap<String, Vec<TradeReport>> = HashMap::new();
        for trade in update_queue {
            let user_id = std::str::from_utf8(&trade.user_id)
                .unwrap_or("")
                .trim_end_matches('\0')
                .to_string();
            grouped.entry(user_id)
                   .or_insert(Vec::new())
                   .push(*trade);
        }

        // 并行更新（每个账户独立锁）
        grouped.par_iter().for_each(|(user_id, trades)| {
            if let Some(account) = self.accounts.get(user_id) {
                let mut acc = account.write();
                for trade in trades {
                    self.apply_trade(&mut acc, trade);
                }
            }
        });
    }

    fn apply_trade(&self, acc: &mut QA_Account, trade: &TradeReport) {
        let order_id = std::str::from_utf8(&trade.order_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let exchange_order_id = std::str::from_utf8(&trade.exchange_order_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let instrument_id = std::str::from_utf8(&trade.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let datetime = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 计算 towards（qars标准）
        let towards = match (trade.direction, trade.offset) {
            (0, 0) => 1,      // BUY OPEN
            (1, 0) => -2,     // SELL OPEN
            (0, 1) => 3,      // BUY CLOSE
            (1, 1) => -3,     // SELL CLOSE
            _ => 1,
        };

        // 更新订单的 exchange_order_id（重要！）
        if let Some(order) = acc.dailyorders.get_mut(order_id) {
            order.exchange_order_id = exchange_order_id.to_string();
        }

        // 调用 sim 模式的成交处理
        if let Err(e) = acc.receive_deal_sim(
            order_id,
            exchange_order_id,
            instrument_id,
            towards,
            trade.price,
            trade.volume,
            &datetime,
        ) {
            log::error!("Failed to apply trade for {}: {}", order_id, e);
        }
    }
}
```

**关键点**：
- **双通道监听**：使用 `crossbeam::select!` 同时监听 OrderAccepted 和 TradeReport
- **Sim模式流程**：先 `on_order_confirm()` 更新 exchange_order_id，再 `receive_deal_sim()` 更新持仓
- **批量处理**：成交回报按批次处理，减少锁开销
- **并行更新**：不同账户并行更新，提高吞吐量
- **towards转换**：从 direction+offset 转换为 qars 的 towards 值

### 组件 3: 行情系统 (MarketDataCore)

**独立进程**，零拷贝广播

```rust
// src/market/core/mod.rs
pub struct MarketDataCore {
    // iceoryx2 订单簿订阅器
    orderbook_subscriber: iceoryx2::Subscriber<OrderbookSnapshot>,

    // qadataswap 广播器
    broadcaster: DataBroadcaster,

    // 订阅管理
    subscriptions: DashMap<String, Vec<String>>, // user_id -> instruments
}

impl MarketDataCore {
    pub fn run(&mut self) {
        loop {
            // 1. 接收订单簿快照（零拷贝）
            if let Some(snapshot) = self.orderbook_subscriber.take() {
                // 2. 广播给所有订阅者（零拷贝）
                self.broadcaster.publish(
                    &snapshot.instrument_id,
                    MarketDataType::Level2,
                    &snapshot.data
                );
            }
        }
    }
}
```

### 组件 4: 交易网关 (Gateway)

**独立线程**，订单路由与风控

```rust
// examples/high_performance_demo.rs - Gateway 线程
let gateway_handle = {
    let account_sys = account_system.clone();
    let order_sender = order_tx.clone();

    thread::Builder::new()
        .name("Gateway".to_string())
        .spawn(move || {
            while let Ok(mut order_req) = client_rx.recv() {
                // 1. 提取用户信息
                let user_id = std::str::from_utf8(&order_req.user_id)
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .to_string();

                let instrument_id = std::str::from_utf8(&order_req.instrument_id)
                    .unwrap_or("")
                    .trim_end_matches('\0');

                // 2. 先通过账户系统 send_order()
                //    这是关键！必须先经过账户系统：
                //    - 生成 order_id (UUID)
                //    - 校验资金/保证金
                //    - 冻结资金
                //    - 记录到 dailyorders
                if let Some(account) = account_sys.get_account(&user_id) {
                    let mut acc = account.write();

                    // 计算 towards（direction + offset → towards）
                    let towards = if order_req.direction == 0 {
                        if order_req.offset == 0 { 1 } else { 3 }  // BUY OPEN=1, BUY CLOSE=3
                    } else {
                        if order_req.offset == 0 { -2 } else { -3 }  // SELL OPEN=-2, SELL CLOSE=-3
                    };

                    let datetime = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

                    // 关键：调用 send_order() 进行风控校验和资金冻结
                    match acc.send_order(
                        instrument_id,
                        order_req.volume,
                        &datetime,
                        towards,
                        order_req.price,
                        "",
                        "LIMIT",
                    ) {
                        Ok(qars_order) => {
                            // 3. 获取账户生成的 order_id
                            let account_order_id = qars_order.order_id.clone();

                            // 4. 将 order_id 写入 OrderRequest（用于撮合引擎和回报匹配）
                            let order_id_bytes = account_order_id.as_bytes();
                            let len = order_id_bytes.len().min(40);
                            order_req.order_id[..len].copy_from_slice(&order_id_bytes[..len]);

                            log::info!("[Gateway] {} 订单已创建: {} (冻结资金完成)", user_id, account_order_id);

                            // 5. 发送到撮合引擎
                            let _ = order_sender.send(order_req);
                        }
                        Err(e) => {
                            log::error!("[Gateway] {} 订单被拒绝: {:?}", user_id, e);
                        }
                    }
                }
            }
        })
        .unwrap()
};
```

**关键点**：
- **订单必须先经过 AccountSystem.send_order()**：这是架构的核心原则！
- **生成 order_id**：由账户系统生成，不是由撮合引擎生成
- **风控前置**：资金校验、保证金检查、仓位检查都在 send_order() 中完成
- **资金冻结**：开仓时立即冻结保证金，防止超额下单
- **Towards 转换**：将 direction+offset 转换为 qars 的 towards 值
- **拒绝处理**：资金不足、仓位不足等风控失败时，订单不会进入撮合引擎

## 两层订单ID设计（真实交易所架构）

### 为什么需要两层ID？

**问题**：如果只用一个ID，会导致：
- 账户系统生成ID：全局可能重复（多账户可能生成相同UUID）
- 交易所生成ID：账户系统无法匹配回原始订单

**解决方案**：两层ID设计
1. **`order_id`**：账户系统生成（UUID格式，40字节），用于账户内部匹配 `dailyorders`
2. **`exchange_order_id`**：交易所生成，全局唯一（单日不重复），用于行情推送

### 完整流程（Sim模式，8步）

```
┌─────────┐
│ Client  │
└────┬────┘
     │ 1. 发送订单请求（OrderRequest）
     │    direction: BUY(0)/SELL(1)
     │    offset: OPEN(0)/CLOSE(1)
     ↓
┌──────────────────────────────────────────────────────────┐
│ Gateway (订单路由线程)                                    │
│                                                           │
│  2. 调用 AccountSystem.send_order()                      │
│     ✓ 生成 order_id (UUID): "a1b2c3d4-e5f6-..."         │
│     ✓ 计算 towards 值 (direction + offset → towards)    │
│     ✓ 校验资金/保证金                                    │
│     ✓ 冻结资金 (frozen += margin_required)              │
│     ✓ 记录到 dailyorders (status="PENDING")             │
│                                                           │
│  3. 将 order_id 写入 OrderRequest.order_id[40]          │
│     转发到 MatchingEngine                                │
└──────────────────┬───────────────────────────────────────┘
                   │
                   ↓
┌──────────────────────────────────────────────────────────┐
│ MatchingEngine (撮合引擎线程)                             │
│                                                           │
│  4. 订单进入订单簿 (Success::Accepted)                   │
│     ✓ 生成 exchange_order_id (全局唯一)                 │
│       格式: "EX_{timestamp}_{code}_{direction}"          │
│       示例: "EX_1728123456789_IX2401_B"                  │
│                                                           │
│  5. 发送 OrderAccepted 消息                              │
│     order_id: "a1b2c3d4-e5f6-..."                       │
│     exchange_order_id: "EX_1728123456789_IX2401_B"      │
└──────────────────┬───────────────────────────────────────┘
                   │
                   ↓
┌──────────────────────────────────────────────────────────┐
│ AccountSystem (账户系统线程)                              │
│                                                           │
│  6. 接收 OrderAccepted → on_order_confirm()             │
│     ✓ 根据 order_id 查找 dailyorders                    │
│     ✓ 更新 order.exchange_order_id                      │
│     ✓ 更新 order.status = "ALIVE"                       │
└───────────────────────────────────────────────────────────┘
                   │
                   │ (撮合成交)
                   ↓
┌──────────────────────────────────────────────────────────┐
│ MatchingEngine (撮合引擎线程)                             │
│                                                           │
│  7. 撮合成功 → 发送 TradeReport                          │
│     trade_id: "TRADE_123456"                             │
│     order_id: "a1b2c3d4-e5f6-..."      (用于匹配账户)   │
│     exchange_order_id: "EX_..."         (用于行情推送)   │
└──────────────────┬───────────────────────────────────────┘
                   │
                   ↓
┌──────────────────────────────────────────────────────────┐
│ AccountSystem (账户系统线程)                              │
│                                                           │
│  8. 接收 TradeReport → receive_deal_sim()               │
│     ✓ 根据 order_id 匹配 dailyorders                    │
│     ✓ 更新持仓 (volume_long/volume_short)               │
│     ✓ 释放冻结保证金 (frozen -= margin)                 │
│     ✓ 占用实际保证金 (margin += actual_margin)          │
│     ✓ 更新 order.status = "FILLED"                      │
└───────────────────────────────────────────────────────────┘
                   │
                   ↓
┌──────────────────┐
│ MarketData       │  使用 exchange_order_id 推送逐笔成交
│ (行情推送)        │  (保护用户隐私，不暴露UUID)
└──────────────────┘
```

### Towards值系统（期货交易）

qaexchange-rs 使用 QARS 的 `towards` 参数统一表示方向+开平：

| Direction | Offset | Towards | 含义 |
|-----------|--------|---------|------|
| BUY (0) | OPEN (0) | **1** | 买入开仓（开多） |
| SELL (1) | OPEN (0) | **-2** | 卖出开仓（开空） |
| BUY (0) | CLOSE (1) | **3** | 买入平仓（平空头） |
| SELL (1) | CLOSE (1) | **-3** | 卖出平仓（平多头） |

**注意**：SELL OPEN 使用 `-2` 而非 `-1`，因为 `-1` 在QARS中表示 "SELL CLOSE yesterday"（只平昨日多头）。

转换代码示例：
```rust
let towards = if order_req.direction == 0 {
    if order_req.offset == 0 { 1 } else { 3 }  // BUY OPEN=1, BUY CLOSE=3
} else {
    if order_req.offset == 0 { -2 } else { -3 }  // SELL OPEN=-2, SELL CLOSE=-3
};
```

详细说明请参考：[期货交易机制详解](./TRADING_MECHANISM.md)

## 数据结构定义（零拷贝）

```rust
// src/protocol/ipc_messages.rs
use serde_big_array::BigArray;

#[repr(C)]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct OrderRequest {
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],        // 账户订单ID（UUID 36字符+填充）
    pub user_id: [u8; 32],
    pub instrument_id: [u8; 16],
    pub direction: u8,      // 0=BUY, 1=SELL
    pub offset: u8,         // 0=OPEN, 1=CLOSE
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub gateway_id: u32,
    pub session_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct OrderAccepted {
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],           // 账户订单ID（用于匹配 dailyorders）
    pub exchange_order_id: [u8; 32],  // 交易所订单ID（全局唯一）
    pub user_id: [u8; 32],
    pub instrument_id: [u8; 16],
    pub timestamp: i64,
    pub gateway_id: u32,
    pub session_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct TradeReport {
    pub trade_id: [u8; 32],           // 成交ID（交易所生成，全局唯一）
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],           // 账户订单ID（用于匹配 dailyorders）
    pub exchange_order_id: [u8; 32],  // 交易所订单ID（全局唯一，用于行情推送）
    pub user_id: [u8; 32],
    pub instrument_id: [u8; 16],
    pub direction: u8,
    pub offset: u8,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub commission: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    pub instrument_id: [u8; 16],
    pub timestamp: i64,
    pub bids: [PriceLevel; 10],
    pub asks: [PriceLevel; 10],
}

#[repr(C)]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub volume: f64,
    pub order_count: u32,
}
```

**关键点**：
- `#[repr(C)]` 保证内存布局稳定
- 固定大小，无需动态分配
- 可直接放入共享内存（iceoryx2）
- **order_id 使用40字节**：UUID是36字符，需要额外空间存储字符串结束符
- **serde-big-array**：Serde默认只支持32字节以下数组，超过需要 `#[serde(with = "BigArray")]`

### UUID截断问题的解决

**问题**：标准UUID是36字符（如 `a1b2c3d4-e5f6-7890-abcd-1234567890ab`），如果使用32字节数组会被截断。

**解决方案**：
```rust
// Cargo.toml 添加依赖
serde-big-array = "0.5"

// 扩展数组大小
pub order_id: [u8; 40]  // 36 + 终止符 + 对齐

// 写入时确保长度正确
let order_id_bytes = account_order_id.as_bytes();
let len = order_id_bytes.len().min(40);
order_req.order_id[..len].copy_from_slice(&order_id_bytes[..len]);

// 读取时正确处理
let order_id = std::str::from_utf8(&trade.order_id)
    .unwrap_or("")
    .trim_end_matches('\0');  // 移除填充的空字符
```

## 部署架构

```
┌─────────────────────────────────────────────────────────┐
│                    Process Topology                      │
└─────────────────────────────────────────────────────────┘

┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  Gateway-1   │  │  Gateway-2   │  │  Gateway-N   │
│  (Port 8080) │  │  (Port 8081) │  │  (Port 808N) │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │
       └────────────┬────┴─────────────────┘
                    │ iceoryx2
                    ↓
       ┌────────────────────────────┐
       │   MatchingEngineCore       │
       │   (Single Process)          │
       │   ┌────────┐  ┌────────┐  │
       │   │ IX2401 │  │ IF2401 │  │
       │   └────────┘  └────────┘  │
       └────────┬───────────────────┘
                │ iceoryx2
       ┌────────┴───────────┐
       ↓                    ↓
┌──────────────┐    ┌──────────────┐
│ AccountCore  │    │ MarketCore   │
│ (Sharded)    │    │              │
└──────────────┘    └──────────────┘
```

## 性能优化策略

### 1. 撮合引擎优化

```rust
// 使用 SPSC 队列（单生产者单消费者）
use crossbeam::queue::ArrayQueue;

pub struct OptimizedOrderbook {
    // 预分配价格档位
    price_levels: Vec<PriceLevel>,

    // 无锁订单队列
    pending_orders: ArrayQueue<OrderRequest>,

    // CPU 亲和性绑定
    cpu_affinity: usize,
}
```

### 2. 账户系统优化

```rust
// 账户分片（减少锁竞争）
pub struct ShardedAccountSystem {
    shards: Vec<AccountSystemCore>,
    shard_count: usize,
}

impl ShardedAccountSystem {
    fn get_shard(&self, user_id: &str) -> usize {
        // 哈希分片
        let hash = hash(user_id);
        hash % self.shard_count
    }
}
```

### 3. 行情推送优化

```rust
// 使用 qadataswap 的零拷贝广播
let broadcaster = DataBroadcaster::new(BroadcastConfig {
    topic: "market_data",
    buffer_size: 1024 * 1024, // 1MB
    subscriber_capacity: 10000,
});
```

## 监控指标

```rust
pub struct ExchangeMetrics {
    // 撮合延迟分布
    matching_latency_p50: Histogram,
    matching_latency_p99: Histogram,

    // 订单吞吐
    order_throughput: Counter,

    // 成交吞吐
    trade_throughput: Counter,

    // 账户更新延迟
    account_update_latency: Histogram,

    // 行情推送延迟
    market_publish_latency: Histogram,
}
```

## 容错设计

1. **撮合引擎**：单点故障 → 主备切换（Raft）
2. **账户系统**：定期快照 + WAL 日志
3. **行情系统**：无状态，可随时重启
4. **网关**：无状态，可水平扩展

## 核心架构原则总结

### 1. 订单必须先经过账户系统（关键！）

这是整个架构的核心原则，参考真实交易所（上交所、中金所、CTP等）的设计：

```
❌ 错误流程（会导致崩溃）：
Client → Gateway → MatchingEngine → AccountSystem
                       ↓
                   订单直接进入撮合
                       ↓
                   TradeReport 返回
                       ↓
                   AccountSystem.receive_deal_sim()
                       ↓
                   💥 NOT IN DAY ORDER 错误！
                   （因为 dailyorders 中没有这个订单）

✅ 正确流程：
Client → Gateway → AccountSystem.send_order()
                       ↓
                   生成 order_id, 冻结资金, 记录 dailyorders
                       ↓
                   MatchingEngine（携带 order_id）
                       ↓
                   OrderAccepted → AccountSystem.on_order_confirm()
                       ↓
                   TradeReport → AccountSystem.receive_deal_sim()
                       ↓
                   ✅ 成功更新持仓（order_id 匹配成功）
```

### 2. 两层ID设计原因

| ID类型 | 生成时机 | 格式 | 用途 |
|--------|---------|------|------|
| **order_id** | Gateway调用send_order()时 | UUID（36字符） | 账户内部匹配dailyorders |
| **exchange_order_id** | MatchingEngine接受订单时 | EX_{ts}_{code}_{dir} | 全局唯一，行情推送 |

**为什么不能只用一个ID？**
- 只用 order_id：撮合引擎无法保证全局唯一性（多账户可能生成相同UUID）
- 只用 exchange_order_id：账户系统无法匹配回 dailyorders（因为send_order时还没有这个ID）

### 3. Sim模式三阶段流程

```
阶段1: send_order()
  ✓ 生成 order_id (UUID)
  ✓ 校验资金/保证金
  ✓ 冻结资金 (frozen += margin)
  ✓ 记录到 dailyorders (status="PENDING")

阶段2: on_order_confirm()
  ✓ 更新 order.exchange_order_id
  ✓ 更新 order.status = "ALIVE"

阶段3: receive_deal_sim()
  ✓ 更新持仓 (volume_long/short)
  ✓ 释放冻结 (frozen -= margin)
  ✓ 占用保证金 (margin += actual_margin)
  ✓ 更新 order.status = "FILLED"
```

**关键**：Real模式也需要 on_order_confirm()，只是成交处理用 receive_simpledeal_transaction()

### 4. Towards值系统（期货特有）

```rust
// 标准映射（qaexchange-rs）
BUY OPEN    → 1    // 开多
SELL OPEN   → -2   // 开空（注意：不是-1！）
BUY CLOSE   → 3    // 平空
SELL CLOSE  → -3   // 平多

// 为什么 SELL OPEN 是 -2 而不是 -1？
// -1 在 QARS 中表示 "SELL CLOSE yesterday"（只平昨日多头）
// -2 才是标准的卖出开仓（建立空头持仓）
```

### 5. UUID截断问题的解决

**问题**：UUID是36字符，但32字节数组会截断

```
原始UUID: a1b2c3d4-e5f6-7890-abcd-1234567890ab  (36字符)
32字节截断: a1b2c3d4-e5f6-7890-abcd-5c4b797a  (丢失12字符)
```

**解决方案**：
1. 扩展数组到40字节：`pub order_id: [u8; 40]`
2. 添加依赖：`serde-big-array = "0.5"`
3. 添加属性：`#[serde(with = "BigArray")]`

### 6. 通道复用（crossbeam::select）

AccountSystem 需要同时监听两个通道：

```rust
select! {
    recv(accepted_receiver) -> msg => {
        // 订单确认 → on_order_confirm()
    }
    recv(trade_receiver) -> msg => {
        // 成交回报 → receive_deal_sim()
    }
    default(Duration::from_millis(10)) => {
        // 超时处理批量队列
    }
}
```

### 7. 批量处理策略

```rust
// 成交回报按账户分组 → 并行更新
grouped.par_iter().for_each(|(user_id, trades)| {
    // 每个账户独立锁，减少锁竞争
    if let Some(account) = self.accounts.get(user_id) {
        let mut acc = account.write();
        for trade in trades {
            acc.receive_deal_sim(/* ... */);
        }
    }
});
```

## 已完成的P4阶段改进

### ✅ P4.1: 两层ID设计
- [x] 扩展 order_id 到40字节（支持完整UUID）
- [x] 添加 exchange_order_id 字段
- [x] MatchingEngine 生成全局唯一ID
- [x] serde-big-array 支持

### ✅ P4.2: Sim模式完整流程
- [x] 新增 OrderAccepted 消息类型
- [x] 新增 accepted_sender/receiver 通道
- [x] 实现 on_order_confirm() 调用
- [x] AccountSystemCore 使用 select! 监听多通道

### ✅ P4.3: Gateway订单路由
- [x] Gateway 先调用 AccountSystem.send_order()
- [x] 风控前置（资金校验、保证金冻结）
- [x] Towards 值转换（direction+offset → towards）
- [x] order_id 传递到撮合引擎

### ✅ P4.4: Towards值修正
- [x] BUY OPEN = 1（不是2）
- [x] SELL OPEN = -2（不是-1）
- [x] 所有示例代码更新

### ✅ P4.5: 文档完善
- [x] 创建 TRADING_MECHANISM.md（期货交易机制详解）
- [x] 更新 HIGH_PERFORMANCE_ARCHITECTURE.md（完整流程）
- [x] 更新 P1_P2_IMPLEMENTATION_SUMMARY.md（P4章节）

## 下一步实现计划（P5阶段）

### Phase 5.1: iceoryx2 零拷贝通信
- [ ] 替换 crossbeam::channel 为 iceoryx2
- [ ] 定义共享内存服务配置
- [ ] 实现 Publisher/Subscriber 模式

### Phase 5.2: 性能优化
- [ ] CPU 亲和性绑定（撮合引擎固定到核心0）
- [ ] 预分配内存池（订单、成交回报）
- [ ] 无锁数据结构（SPSC队列）

### Phase 5.3: 账户分片
- [ ] 按 user_id 哈希分片
- [ ] 减少单个 AccountSystem 的锁竞争
- [ ] 支持水平扩展

### Phase 5.4: 监控与容错
- [ ] Prometheus 指标导出
- [ ] 撮合引擎主备切换（Raft）
- [ ] WAL 日志（账户状态持久化）

### Phase 5.5: 性能基准测试
- [ ] 执行 benchmark_million_orders.rs
- [ ] 测量撮合延迟（P50/P99/P999）
- [ ] 测量订单吞吐量
- [ ] 对比集中式 vs 分布式架构
