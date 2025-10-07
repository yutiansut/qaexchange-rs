# Actix Actor 架构

> **架构作者**: @yutiansut @quantaxis
> **最后更新**: 2025-10-07

## 概述

QAExchange 采用 **Actix Actor 模型** 实现高并发、低延迟的异步消息处理架构。Actor 模型通过消息传递隔离状态，避免共享内存锁竞争，实现系统的高性能和可扩展性。

## Actor 架构总览

### 系统中的 Actor 实例

QAExchange 系统包含以下 3 类核心 Actor：

| Actor 类型 | 实例数量 | 职责 | 生命周期 |
|-----------|---------|------|---------|
| **KLineActor** | 1 | K线实时聚合、历史查询、WAL持久化 | 系统启动时创建，运行至系统关闭 |
| **WsSession** | N (每个WebSocket连接1个) | WebSocket会话管理、消息路由 | 连接建立时创建，连接断开时销毁 |
| **DiffWebsocketSession** | N (每个DIFF协议连接1个) | DIFF协议处理、业务截面同步 | 连接建立时创建，连接断开时销毁 |

### 架构分层

```
┌────────────────────────────────────────────────────────────────┐
│                        应用层 (Client)                          │
│                  WebSocket / HTTP 客户端                        │
└────────────────────────────────────────────────────────────────┘
                                ▲
                                │ WebSocket / JSON
                                ▼
┌────────────────────────────────────────────────────────────────┐
│                     Actor 层 (Actix Actors)                    │
│                                                                 │
│  ┌─────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │  KLineActor │  │   WsSession      │  │ DiffWebsocket    │  │
│  │             │  │   (N instances)  │  │ Session          │  │
│  │  - 订阅tick │  │   - 消息路由     │  │ (N instances)    │  │
│  │  - 聚合K线  │  │   - 心跳管理     │  │ - peek_message   │  │
│  │  - WAL持久化│  │   - 订阅通知     │  │ - rtn_data       │  │
│  │  - 历史查询 │  │                  │  │ - 业务截面管理   │  │
│  └─────────────┘  └──────────────────┘  └──────────────────┘  │
│         ▲                ▲                      ▲               │
│         │                │                      │               │
└─────────┼────────────────┼──────────────────────┼───────────────┘
          │                │                      │
          │  crossbeam     │  crossbeam           │
          │  channel       │  channel             │
          ▼                ▼                      ▼
┌────────────────────────────────────────────────────────────────┐
│                   消息总线层 (Message Bus)                      │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │          MarketDataBroadcaster (Pub/Sub)                │  │
│  │                                                          │  │
│  │  - tick 事件        (Tick价格、成交量)                  │  │
│  │  - kline_finished   (完成的K线)                         │  │
│  │  - orderbook_update (订单簿快照/增量)                   │  │
│  │  - trade_executed   (成交通知)                          │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │          TradeGateway (Point-to-Point)                  │  │
│  │                                                          │  │
│  │  - 订单回报 (OrderAccepted/Rejected)                    │  │
│  │  - 成交通知 (TradeNotification)                         │  │
│  │  - 账户更新 (AccountUpdate)                             │  │
│  └─────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌────────────────────────────────────────────────────────────────┐
│                      业务逻辑层 (Business)                      │
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐    │
│  │ OrderRouter │  │ Account     │  │ MatchingEngine      │    │
│  │             │  │ Manager     │  │                     │    │
│  │             │  │             │  │ - 撮合              │    │
│  │             │  │             │  │ - 发布tick事件      │    │
│  └─────────────┘  └─────────────┘  └─────────────────────┘    │
└────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌────────────────────────────────────────────────────────────────┐
│                      持久化层 (Persistence)                     │
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐    │
│  │ WAL         │  │ MemTable    │  │ SSTable             │    │
│  │ (K线持久化) │  │ (OLAP列存)  │  │ (rkyv/Parquet)      │    │
│  └─────────────┘  └─────────────┘  └─────────────────────┘    │
└────────────────────────────────────────────────────────────────┘
```

## Actor 详细设计

### 1. KLineActor - K线聚合 Actor

**文件位置**: `src/market/kline_actor.rs`

#### 职责

- 订阅 MarketDataBroadcaster 的 tick 事件
- 实现分级采样：3s/1min/5min/15min/30min/60min/Day
- 完成的 K 线广播到 MarketDataBroadcaster（`KLineFinished` 事件）
- WAL 持久化和恢复
- 提供历史 K 线查询服务（HTTP/WebSocket API）

#### 消息处理

**订阅的消息**:
- `MarketDataEvent::Tick` - 来自撮合引擎的 tick 数据

**发送的消息**:
- `MarketDataEvent::KLineFinished` - 完成的 K 线事件

**处理的 Actor 消息**:
- `GetKLines` - 查询历史 K 线（HTTP API）
- `GetCurrentKLine` - 查询当前未完成的 K 线

#### 数据流

```
MarketDataBroadcaster (tick)
         │
         ▼
    KLineActor
    ┌──────────────────────┐
    │ 1. on_tick()         │──┐
    │ 2. 聚合各周期K线     │  │
    │ 3. 完成的K线:        │  │
    │    - 广播事件        │◄─┘
    │    - WAL持久化       │
    │    - 加入历史        │
    └──────────────────────┘
         │           │
         ▼           ▼
  MarketDataEvent  WalManager
   ::KLineFinished   (append)
```

#### 启动流程

```rust
// main.rs
let kline_wal_manager = Arc::new(WalManager::new("./data/wal/klines"));
let kline_actor = KLineActor::new(
    market_broadcaster.clone(),
    kline_wal_manager.clone()
).start();  // 返回 Addr<KLineActor>
```

#### WAL 恢复

启动时自动从 WAL 恢复历史 K 线：

```rust
fn started(&mut self, ctx: &mut Self::Context) {
    // 1. 从WAL恢复历史数据
    self.recover_from_wal();

    // 2. 订阅tick事件
    let receiver = self.broadcaster.subscribe(
        subscriber_id,
        vec![],  // 空列表表示订阅所有合约
        vec!["tick".to_string()]
    );

    // 3. 启动异步任务处理tick
    ctx.spawn(actix::fut::wrap_future(fut));
}
```

### 2. WsSession - WebSocket 会话 Actor

**文件位置**: `src/service/websocket/session.rs`

#### 职责

- WebSocket 连接生命周期管理
- 客户端认证和会话状态维护
- 消息路由（Client → Business Logic）
- 心跳检测（5s间隔，10s超时）
- 订阅管理（订阅合约、频道）

#### 会话状态

```rust
pub enum SessionState {
    Unauthenticated,              // 未认证
    Authenticated { user_id: String },  // 已认证
}
```

#### 消息处理

**接收的消息**:
- `ClientMessage::Auth` - 认证请求
- `ClientMessage::Subscribe` - 订阅行情
- `ClientMessage::SubmitOrder` - 下单
- `ClientMessage::CancelOrder` - 撤单
- `ClientMessage::QueryAccount` - 查询账户
- `ClientMessage::Ping` - 心跳

**发送的消息**:
- `ServerMessage::AuthResponse` - 认证响应
- `ServerMessage::Trade` - 成交通知
- `ServerMessage::OrderStatus` - 订单状态
- `ServerMessage::AccountUpdate` - 账户更新
- `ServerMessage::OrderBook` - 订单簿
- `ServerMessage::Pong` - 心跳响应

#### 数据流

```
Client
  │
  ▼ WebSocket
WsSession
  ┌─────────────────────────┐
  │ 1. 接收Client消息       │
  │ 2. 路由到业务逻辑       │
  │ 3. 订阅市场数据         │
  │ 4. 订阅成交通知         │
  │ 5. 推送Server消息       │
  └─────────────────────────┘
    ▲              │
    │              ▼
MarketData    TradeGateway
Broadcaster   (notification_receiver)
```

#### 心跳机制

```rust
fn heartbeat(&self, ctx: &mut Self::Context) {
    ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
        if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
            log::warn!("WebSocket Client heartbeat failed, disconnecting!");
            ctx.stop();
            return;
        }
        ctx.ping(b"");
    });
}
```

### 3. DiffWebsocketSession - DIFF 协议 WebSocket Actor

**文件位置**: `src/service/websocket/diff_handler.rs`

#### 职责

- 实现 DIFF 协议的 `peek_message` / `rtn_data` 机制
- 维护业务截面（账户、持仓、订单、行情、K线）
- JSON Merge Patch 增量更新
- 订阅管理（合约、图表）
- 指令处理（下单、撤单、银期转账、set_chart）

#### DIFF 协议核心机制

**peek_message 机制**:

```
Client                          Server
  │                                │
  │──── peek_message ────────────▶│
  │                                │ (等待数据更新)
  │                                │
  │                                │ (有更新发生)
  │◀──── rtn_data (JSON Patch) ───│
  │                                │
  │──── peek_message ────────────▶│
  │                                │
  └────────────────────────────────┘
```

**业务截面结构**:

```json
{
  "account_id": "user1",
  "balance": 10000000.0,
  "quotes": {
    "SHFE.cu1612": { "last_price": 36580.0, ... }
  },
  "klines": {
    "SHFE.cu1612": {
      "60000000000": {  // 1分钟K线（纳秒）
        "last_id": 12345,
        "data": {
          "12340": { "open": 36500, "close": 36580, ... }
        }
      }
    }
  }
}
```

#### 消息处理

**接收的指令**:
- `peek_message` - 请求业务截面更新
- `subscribe_quote` - 订阅行情
- `set_chart` - 订阅 K 线图表
- `insert_order` - 下单
- `cancel_order` - 撤单
- `req_login` - 登录
- `req_transfer` - 银期转账

**发送的数据包**:
- `rtn_data` - JSON Merge Patch 数组
- `notify` - 通知消息（INFO/WARNING/ERROR）

#### 数据流

```
MarketDataBroadcaster           DiffWebsocketSession
         │                             │
         │  ┌──────────────────────────┤
         │  │ 1. subscribe_quote       │
         │  │    订阅合约列表          │
         │  └─────────────────────────▶│
         │                             │
         │  ┌──────────────────────────┤
         │  │ 2. set_chart             │
         │  │    订阅K线图表           │
         │  └─────────────────────────▶│
         │                             │
    tick事件                           │
         │─────────────────────────────▶│
         │                             │ 更新 quotes 截面
         │                             │
  KLineFinished事件                    │
         │─────────────────────────────▶│
         │                             │ 更新 klines 截面
         │                             │
         │                peek_message │
         │◀─────────────────────────────│
         │                             │
         │  rtn_data (JSON Patch)      │
         │─────────────────────────────▶│
         │                             │
```

#### set_chart K线订阅

```rust
// 客户端请求
{
  "aid": "set_chart",
  "chart_id": "chart1",
  "ins_list": "SHFE.cu1701",
  "duration": 60000000000,  // 1分钟（纳秒）
  "view_width": 500         // 最新500根K线
}

// 服务端响应（rtn_data）
{
  "aid": "rtn_data",
  "data": [{
    "klines": {
      "SHFE.cu1701": {
        "60000000000": {
          "last_id": 12345,
          "data": {
            "12340": {
              "datetime": 1696684800000000000,  // UnixNano
              "open": 36500.0,
              "high": 36600.0,
              "low": 36480.0,
              "close": 36580.0,
              "volume": 1234,
              "open_oi": 23000,
              "close_oi": 23100
            }
          }
        }
      }
    }
  }]
}
```

## 消息总线设计

### MarketDataBroadcaster - Pub/Sub 模式

**文件位置**: `src/market/broadcaster.rs`

#### 架构

```rust
pub struct MarketDataBroadcaster {
    channels: Arc<RwLock<HashMap<String, Vec<Sender<MarketDataEvent>>>>>,
    //                    频道名        订阅者列表
}
```

#### 订阅机制

```rust
// 订阅示例
let receiver = broadcaster.subscribe(
    "subscriber_id_123",           // 订阅者ID
    vec!["SHFE.cu1612".to_string()], // 订阅的合约（空=所有）
    vec!["tick".to_string()]         // 订阅的事件类型
);

// 接收事件
loop {
    match receiver.recv() {
        Ok(MarketDataEvent::Tick { instrument_id, price, volume, .. }) => {
            // 处理tick
        }
        Ok(MarketDataEvent::KLineFinished { instrument_id, period, kline, .. }) => {
            // 处理完成的K线
        }
        _ => {}
    }
}
```

#### 事件类型

```rust
pub enum MarketDataEvent {
    Tick {
        instrument_id: String,
        price: f64,
        volume: i64,
        direction: String,
        timestamp: i64,
    },
    OrderBookSnapshot { ... },
    OrderBookDelta { ... },
    KLineFinished {
        instrument_id: String,
        period: i32,      // HQChart格式（4=1min, 5=5min等）
        kline: KLine,     // 完成的K线数据
        timestamp: i64,
    },
    TradeExecuted { ... },
}
```

### TradeGateway - Point-to-Point 模式

**文件位置**: `src/exchange/trade_gateway.rs`

#### 架构

```rust
pub struct TradeGateway {
    subscribers: Arc<DashMap<String, Sender<Notification>>>,
    //                      user_id    通知发送器
}
```

#### 订阅流程

```rust
// WebSocket会话订阅用户通知
let notification_receiver = trade_gateway.subscribe_user(user_id.clone());

// 接收通知
loop {
    match notification_receiver.try_recv() {
        Ok(notification) => {
            let json = notification.to_json();
            websocket.send(json)?;
        }
        Err(_) => break,
    }
}
```

#### 通知类型

```rust
pub enum NotificationType {
    OrderAccepted,        // 订单接受
    OrderRejected,        // 订单拒绝
    Trade,                // 成交
    OrderCancelled,       // 撤单成功
    CancelRejected,       // 撤单拒绝
    AccountUpdate,        // 账户更新
    PositionUpdate,       // 持仓更新
    MarginCall,           // 追加保证金
    ForceLiquidation,     // 强制平仓
}
```

## Actor 通信模式

### 1. Actor 消息传递（Actix Message）

用于 Actor 内部的同步/异步消息处理：

```rust
// 定义消息
#[derive(Message)]
#[rtype(result = "Vec<KLine>")]
pub struct GetKLines {
    pub instrument_id: String,
    pub period: KLinePeriod,
    pub count: usize,
}

// 消息处理器
impl Handler<GetKLines> for KLineActor {
    type Result = Vec<KLine>;

    fn handle(&mut self, msg: GetKLines, _ctx: &mut Context<Self>) -> Self::Result {
        // 从aggregators查询K线
        // ...
    }
}

// 发送消息
let klines = kline_actor.send(GetKLines {
    instrument_id: "IF2501".to_string(),
    period: KLinePeriod::Min1,
    count: 100,
}).await?;
```

### 2. Channel 消息传递（crossbeam）

用于跨模块、跨线程的异步事件分发：

```rust
// 创建channel
let (tx, rx) = crossbeam::channel::unbounded();

// 生产者
tx.send(MarketDataEvent::Tick { ... })?;

// 消费者（在Actor内部）
loop {
    match rx.recv() {
        Ok(event) => { /* 处理事件 */ }
        Err(_) => break,
    }
}
```

### 3. Arc + RwLock 共享状态

用于多个 Actor 读取共享状态（如账户、订单簿）：

```rust
// 共享账户管理器
let account_mgr = Arc::new(AccountManager::new());

// Actor 1: 读取账户
let account = account_mgr.get_account(user_id)?;

// Actor 2: 更新账户
account_mgr.update_balance(user_id, new_balance)?;
```

## 性能优化

### 1. Zero-Copy 订阅

MarketDataBroadcaster 使用 `Arc<MarketDataEvent>` 避免消息克隆：

```rust
// 内部实现
let event = Arc::new(MarketDataEvent::Tick { ... });
for subscriber in subscribers {
    subscriber.send(event.clone())?;  // 只克隆Arc指针
}
```

### 2. 批量发送

DiffWebsocketSession 批量发送 rtn_data：

```rust
// 累积100个事件或100ms超时后批量发送
if events.len() >= 100 || elapsed > 100ms {
    let patches = events.iter().map(to_json_patch).collect();
    send_rtn_data(patches)?;
    events.clear();
}
```

### 3. 背压控制

WebSocket 会话实现背压控制，防止慢客户端阻塞系统：

```rust
// 队列超过500个事件时跳过新事件
if pending_events.len() > 500 {
    log::warn!("Client {} queue full, dropping event", session_id);
    continue;
}
```

## Actor 生命周期管理

### KLineActor

- **启动**: `main.rs` 中调用 `.start()` 创建 Addr
- **运行**: 持续订阅 tick 事件，聚合 K 线
- **停止**: 系统关闭时自动停止

### WsSession / DiffWebsocketSession

- **启动**: 每个 WebSocket 连接建立时创建
- **运行**: 处理客户端消息，推送服务端消息
- **停止**:
  - 客户端断开连接
  - 心跳超时（10秒）
  - 认证失败

### 清理流程

```rust
impl Actor for WsSession {
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("WebSocket session {} stopped", self.id);

        // 1. 取消订阅
        if let Some(broadcaster) = &self.market_broadcaster {
            broadcaster.unsubscribe(&self.id);
        }

        // 2. 从会话映射中移除
        if let Some(sessions) = &self.sessions {
            sessions.write().remove(&self.id);
        }

        // 3. 释放资源
        drop(self.notification_receiver);
        drop(self.market_data_receiver);
    }
}
```

## 故障处理

### Actor 崩溃恢复

- **KLineActor**: 通过 WAL 恢复历史 K 线，tick 事件不会丢失（由撮合引擎重发）
- **WsSession**: 自动断开连接，客户端重连后重新认证和订阅
- **DiffWebsocketSession**: 重连后通过 `peek_message` 获取最新业务截面

### 消息丢失处理

- **MarketDataBroadcaster**: 使用 `unbounded` channel，不会丢失消息（除非内存耗尽）
- **TradeGateway**: 使用 `unbounded` channel + 持久化到 WAL，保证通知不丢失

### 背压处理

- **慢订阅者**: 队列超过阈值时丢弃事件（WebSocket）或断开连接
- **快生产者**: 无背压，依赖消费者处理能力

## 监控指标

### Actor 指标

| 指标 | 说明 | 告警阈值 |
|------|------|---------|
| `actor.kline.pending_events` | KLineActor 待处理 tick 数量 | > 1000 |
| `actor.ws_session.count` | 活跃 WebSocket 会话数 | > 5000 |
| `actor.ws_session.heartbeat_timeout` | 心跳超时次数 | > 100/min |

### 消息总线指标

| 指标 | 说明 | 告警阈值 |
|------|------|---------|
| `broadcaster.tick.throughput` | Tick 事件吞吐量 | < 10K/s |
| `broadcaster.subscribers` | MarketDataBroadcaster 订阅者数量 | > 1000 |
| `trade_gateway.notification_latency` | 成交通知延迟 | P99 > 10ms |

## 最佳实践

### 1. Actor 消息设计

✅ **推荐**:
```rust
// 使用Arc避免大对象克隆
#[derive(Message)]
#[rtype(result = "()")]
pub struct ProcessMarketData {
    pub data: Arc<Vec<MarketDataEvent>>,
}
```

❌ **不推荐**:
```rust
// 直接传递大对象，导致克隆开销
pub struct ProcessMarketData {
    pub data: Vec<MarketDataEvent>,  // 可能包含10000+事件
}
```

### 2. 订阅管理

✅ **推荐**:
```rust
// 精确订阅需要的合约和事件
let receiver = broadcaster.subscribe(
    subscriber_id,
    vec!["SHFE.cu1612".to_string()],  // 只订阅cu1612
    vec!["tick".to_string()]          // 只订阅tick
);
```

❌ **不推荐**:
```rust
// 订阅所有合约和事件（高流量）
let receiver = broadcaster.subscribe(
    subscriber_id,
    vec![],  // 所有合约
    vec![]   // 所有事件
);
```

### 3. 错误处理

✅ **推荐**:
```rust
// Actor内部处理错误，记录日志，继续运行
match self.process_tick(&event) {
    Ok(_) => {}
    Err(e) => {
        log::error!("Failed to process tick: {}", e);
        // 继续处理下一个事件
    }
}
```

❌ **不推荐**:
```rust
// 错误传播导致Actor崩溃
self.process_tick(&event)?;  // 可能导致整个Actor停止
```

## 总结

QAExchange 的 Actix Actor 架构实现了：

1. **隔离性**: 每个 Actor 独立运行，状态隔离，避免锁竞争
2. **可扩展性**: 支持 N 个并发 WebSocket 会话，单个 KLineActor 处理所有 K 线聚合
3. **高性能**:
   - Zero-copy 消息传递（Arc）
   - 批量发送（100 events/batch）
   - 背压控制（队列阈值 500）
4. **容错性**:
   - WAL 持久化 + 恢复
   - 心跳检测 + 自动断开
   - 错误隔离 + 日志记录

通过 Actor 模型 + Pub/Sub 消息总线的组合，系统实现了 P99 < 1ms 的 WebSocket 推送延迟和 > 10K/s 的 tick 处理吞吐量。

---

**相关文档**:
- [K线功能文档](../03_core_modules/market/kline.md)
- [市场数据模块](../03_core_modules/market/README.md)
- [DIFF 协议](../04_api/websocket/diff_protocol.md)
- [高性能架构](high_performance.md)
