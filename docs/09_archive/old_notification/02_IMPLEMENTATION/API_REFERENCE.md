# Notification System API Reference

> 完整的通知系统 API 参考文档

**版本**: v1.1.0 (with rkyv support)
**最后更新**: 2025-10-03

---

## 📚 目录

- [NotificationBroker](#notificationbroker) - 消息路由中心
- [NotificationGateway](#notificationgateway) - 推送网关
- [Notification](#notification) - 核心消息结构
- [NotificationType](#notificationtype) - 消息类型枚举
- [NotificationPayload](#notificationpayload) - 消息负载
- [统计结构](#统计结构)

---

## NotificationBroker

**用途**: 消息路由中心，负责消息的发布、路由、去重和优先级处理

### 创建

```rust
pub fn new() -> Self
```

**示例**:
```rust
let broker = Arc::new(NotificationBroker::new());
```

### 方法

#### `register_gateway`

注册 Gateway，建立路由连接

```rust
pub fn register_gateway(
    &self,
    gateway_id: impl Into<Arc<str>>,
    sender: mpsc::UnboundedSender<Notification>
)
```

**参数**:
- `gateway_id` - Gateway 唯一标识
- `sender` - 发送通知的通道

**示例**:
```rust
let (tx, rx) = mpsc::unbounded_channel();
broker.register_gateway("gateway_01", tx);
```

#### `unregister_gateway`

注销 Gateway

```rust
pub fn unregister_gateway(&self, gateway_id: &str)
```

**示例**:
```rust
broker.unregister_gateway("gateway_01");
```

#### `subscribe`

用户订阅 Gateway

```rust
pub fn subscribe(
    &self,
    user_id: impl Into<Arc<str>>,
    gateway_id: impl Into<Arc<str>>
)
```

**参数**:
- `user_id` - 用户 ID
- `gateway_id` - Gateway ID

**示例**:
```rust
broker.subscribe("user_01", "gateway_01");
```

#### `unsubscribe`

用户取消订阅

```rust
pub fn unsubscribe(&self, user_id: &str, gateway_id: &str)
```

**示例**:
```rust
broker.unsubscribe("user_01", "gateway_01");
```

#### `publish`

发布通知消息

```rust
pub fn publish(&self, notification: Notification) -> Result<(), String>
```

**参数**:
- `notification` - 通知消息

**返回值**:
- `Ok(())` - 发布成功
- `Err(String)` - 发布失败（队列满）

**特性**:
- 自动去重（基于 `message_id`）
- 按优先级入队
- 统计发送和去重次数

**示例**:
```rust
let notification = Notification::new(
    NotificationType::AccountUpdate,
    Arc::from("user_01"),
    payload,
    "AccountSystem",
);
broker.publish(notification)?;
```

#### `start_priority_processor`

启动优先级处理器（异步任务）

```rust
pub fn start_priority_processor(self: Arc<Self>) -> tokio::task::JoinHandle<()>
```

**返回值**: 异步任务句柄

**处理策略**:
| 优先级 | 策略 | 延迟目标 |
|-------|------|---------|
| P0 | 处理所有 | < 1ms |
| P1 | 处理所有 | < 5ms |
| P2 | 批量处理 100 条 | < 100ms |
| P3 | 批量处理 50 条 | < 1s |

**示例**:
```rust
let _processor = broker.clone().start_priority_processor();
```

#### `get_stats`

获取统计信息

```rust
pub fn get_stats(&self) -> BrokerStatsSnapshot
```

**返回值**:
```rust
pub struct BrokerStatsSnapshot {
    pub messages_sent: u64,
    pub messages_deduplicated: u64,
    pub messages_dropped: u64,
    pub active_users: usize,
    pub active_gateways: usize,
    pub queue_sizes: [usize; 4],
}
```

**示例**:
```rust
let stats = broker.get_stats();
println!("Sent: {}, Dedup: {}", stats.messages_sent, stats.messages_deduplicated);
```

---

## NotificationGateway

**用途**: WebSocket 推送网关，负责会话管理和消息推送

### 创建

```rust
pub fn new(
    gateway_id: impl Into<Arc<str>>,
    notification_receiver: mpsc::UnboundedReceiver<Notification>
) -> Self
```

**参数**:
- `gateway_id` - Gateway 唯一标识
- `notification_receiver` - 接收 Broker 消息的通道

**示例**:
```rust
let (tx, rx) = mpsc::unbounded_channel();
let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));
```

### 方法

#### `register_session`

注册 WebSocket 会话

```rust
pub fn register_session(
    &self,
    session_id: impl Into<Arc<str>>,
    user_id: impl Into<Arc<str>>,
    sender: mpsc::UnboundedSender<String>
)
```

**参数**:
- `session_id` - 会话唯一标识
- `user_id` - 用户 ID
- `sender` - 发送 JSON 到 WebSocket 的通道

**示例**:
```rust
let (session_tx, mut session_rx) = mpsc::unbounded_channel();
gateway.register_session("session_01", "user_01", session_tx);

// 接收 WebSocket 消息
while let Some(json) = session_rx.recv().await {
    println!("Received: {}", json);
}
```

#### `unregister_session`

注销会话

```rust
pub fn unregister_session(&self, session_id: &str)
```

**示例**:
```rust
gateway.unregister_session("session_01");
```

#### `subscribe_channel`

订阅频道（可选，用于消息过滤）

```rust
pub fn subscribe_channel(&self, session_id: &str, channel: impl Into<String>)
```

**参数**:
- `session_id` - 会话 ID
- `channel` - 频道名称（如 "trade", "orderbook"）

**示例**:
```rust
gateway.subscribe_channel("session_01", "trade");
gateway.subscribe_channel("session_01", "account");
```

#### `unsubscribe_channel`

取消订阅频道

```rust
pub fn unsubscribe_channel(&self, session_id: &str, channel: &str)
```

#### `start_notification_pusher`

启动通知推送任务（异步）

```rust
pub fn start_notification_pusher(self: Arc<Self>) -> tokio::task::JoinHandle<()>
```

**特性**:
- **批量推送**: 100ms 或 100 条消息触发
- **P0 消息**: 立即推送
- **JSON 序列化**: 使用手动 JSON 构造（避免 Arc<str> 问题）

**示例**:
```rust
let _pusher = gateway.clone().start_notification_pusher();
```

#### `start_heartbeat_checker`

启动心跳检测任务（异步）

```rust
pub fn start_heartbeat_checker(self: Arc<Self>) -> tokio::task::JoinHandle<()>
```

**特性**:
- 每 30 秒检查一次
- 超时 5 分钟的会话自动清理

**示例**:
```rust
let _heartbeat = gateway.clone().start_heartbeat_checker();
```

#### `get_stats`

获取统计信息

```rust
pub fn get_stats(&self) -> GatewayStatsSnapshot
```

**返回值**:
```rust
pub struct GatewayStatsSnapshot {
    pub gateway_id: Arc<str>,
    pub messages_pushed: u64,
    pub messages_failed: u64,
    pub active_sessions: usize,
}
```

---

## Notification

**核心消息结构**

### 结构定义

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct Notification {
    pub message_id: Arc<str>,
    pub message_type: NotificationType,
    pub user_id: Arc<str>,
    pub priority: u8,
    pub payload: NotificationPayload,
    pub timestamp: i64,
    pub source: String,
}
```

**字段说明**:
- `message_id` - 全局唯一 ID（UUID）
- `message_type` - 消息类型枚举
- `user_id` - 目标用户 ID
- `priority` - 优先级 0-3（0=最高）
- `payload` - 消息负载（具体内容）
- `timestamp` - 时间戳（纳秒）
- `source` - 来源模块（如 "AccountSystem"）

### 创建方法

#### `new`

创建通知（自动分配优先级）

```rust
pub fn new(
    message_type: NotificationType,
    user_id: impl Into<Arc<str>>,
    payload: NotificationPayload,
    source: impl Into<String>,
) -> Self
```

**示例**:
```rust
let notification = Notification::new(
    NotificationType::AccountUpdate,
    Arc::from("user_01"),
    NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        // ...
    }),
    "AccountSystem",
);
```

#### `with_priority`

创建通知（手动指定优先级）

```rust
pub fn with_priority(
    message_type: NotificationType,
    user_id: impl Into<Arc<str>>,
    payload: NotificationPayload,
    priority: u8,
    source: impl Into<String>,
) -> Self
```

**示例**:
```rust
let notification = Notification::with_priority(
    NotificationType::RiskAlert,
    Arc::from("user_01"),
    payload,
    0,  // P0 最高优先级
    "RiskControl",
);
```

### JSON 序列化

#### `to_json`

手动构造 JSON（避免 Arc<str> 序列化问题）

```rust
pub fn to_json(&self) -> String
```

**示例**:
```rust
let json = notification.to_json();
println!("{}", json);
// {"message_id":"...","message_type":"account_update",...}
```

### rkyv 序列化（v1.1.0+）

#### `to_rkyv_bytes`

序列化为 rkyv 字节流

```rust
pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, String>
```

**用途**:
- 跨进程通信（共享内存）
- 持久化存储
- 高性能消息传递

**性能**:
- 序列化: ~3 ms/10K messages
- 反序列化: ~0.02 ms/10K messages（零拷贝）

**示例**:
```rust
let bytes = notification.to_rkyv_bytes()?;
// 发送到其他进程或存储
```

#### `from_rkyv_bytes`

从 rkyv 字节流反序列化（零拷贝，带验证）

```rust
pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedNotification, String>
```

**特性**:
- 零拷贝（直接内存映射）
- 数据完整性验证
- 适用于不可信来源

**示例**:
```rust
let archived = Notification::from_rkyv_bytes(&bytes)?;
println!("User: {}", archived.user_id);  // 直接访问，无需分配
```

#### `from_rkyv_bytes_unchecked`

从 rkyv 字节流反序列化（零拷贝，不验证）

```rust
pub unsafe fn from_rkyv_bytes_unchecked(bytes: &[u8]) -> &ArchivedNotification
```

**⚠️ 安全性**: 仅用于可信的内部消息

**性能提升**:
- 反序列化延迟: 0.02 ms → 0.005 ms（4倍提升）

**使用场景**:
- Broker → Gateway 内部传递
- 同进程内模块间通信

**示例**:
```rust
let archived = unsafe { Notification::from_rkyv_bytes_unchecked(&bytes) };
```

#### `from_archived`

将 ArchivedNotification 转换为 Notification

```rust
pub fn from_archived(archived: &ArchivedNotification) -> Result<Self, String>
```

**注意**: 这个操作会分配内存，对于只读访问，直接使用 `ArchivedNotification` 更高效

**示例**:
```rust
let archived = Notification::from_rkyv_bytes(&bytes)?;
let notification = Notification::from_archived(archived)?;
```

---

## NotificationType

**15 种通知消息类型**

### 枚举定义

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    // 订单相关（P1）
    OrderAccepted,
    OrderRejected,
    OrderPartiallyFilled,
    OrderFilled,
    OrderCanceled,
    OrderExpired,

    // 成交相关（P1）
    TradeExecuted,
    TradeCanceled,

    // 账户相关（P2）
    AccountUpdate,

    // 持仓相关（P2）
    PositionUpdate,
    PositionProfit,

    // 风控相关（P0）
    RiskAlert,
    MarginCall,
    PositionLimit,

    // 系统相关（P3）
    SystemNotice,
    TradingSessionStart,
    TradingSessionEnd,
    MarketHalt,
}
```

### 方法

#### `default_priority`

返回默认优先级

```rust
pub fn default_priority(&self) -> u8
```

**优先级映射**:
| 优先级 | 消息类型 |
|-------|---------|
| **P0** | RiskAlert, MarginCall, OrderRejected |
| **P1** | OrderAccepted, OrderFilled, TradeExecuted |
| **P2** | AccountUpdate, PositionUpdate |
| **P3** | SystemNotice, MarketHalt |

**示例**:
```rust
assert_eq!(NotificationType::RiskAlert.default_priority(), 0);  // P0
assert_eq!(NotificationType::OrderAccepted.default_priority(), 1);  // P1
```

#### `as_str`

返回字符串表示

```rust
pub fn as_str(&self) -> &'static str
```

**示例**:
```rust
assert_eq!(NotificationType::OrderAccepted.as_str(), "order_accepted");
```

---

## NotificationPayload

**消息负载枚举**

### 变体

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationPayload {
    OrderAccepted(OrderAcceptedNotify),
    OrderRejected(OrderRejectedNotify),
    OrderPartiallyFilled(OrderPartiallyFilledNotify),
    OrderFilled(OrderFilledNotify),
    OrderCanceled(OrderCanceledNotify),
    TradeExecuted(TradeExecutedNotify),
    AccountUpdate(AccountUpdateNotify),
    PositionUpdate(PositionUpdateNotify),
    RiskAlert(RiskAlertNotify),
    MarginCall(MarginCallNotify),
    SystemNotice(SystemNoticeNotify),
}
```

### 负载结构

#### OrderAcceptedNotify

```rust
pub struct OrderAcceptedNotify {
    pub order_id: String,
    pub exchange_order_id: String,
    pub instrument_id: String,
    pub direction: String,  // "BUY" / "SELL"
    pub offset: String,     // "OPEN" / "CLOSE"
    pub price: f64,
    pub volume: f64,
    pub order_type: String,  // "LIMIT" / "MARKET"
    pub frozen_margin: f64,
    pub timestamp: i64,
}
```

#### AccountUpdateNotify

```rust
pub struct AccountUpdateNotify {
    pub user_id: String,
    pub balance: f64,
    pub available: f64,
    pub frozen: f64,
    pub margin: f64,
    pub position_profit: f64,
    pub close_profit: f64,
    pub risk_ratio: f64,
    pub timestamp: i64,
}
```

#### TradeExecutedNotify

```rust
pub struct TradeExecutedNotify {
    pub trade_id: String,
    pub order_id: String,
    pub exchange_order_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub commission: f64,
    pub fill_type: String,  // "FULL" / "PARTIAL"
    pub timestamp: i64,
}
```

#### RiskAlertNotify

```rust
pub struct RiskAlertNotify {
    pub user_id: String,
    pub alert_type: String,   // "MARGIN_INSUFFICIENT" / "POSITION_LIMIT"
    pub severity: String,     // "WARNING" / "CRITICAL" / "EMERGENCY"
    pub message: String,
    pub risk_ratio: f64,
    pub suggestion: String,
    pub timestamp: i64,
}
```

*其他负载结构请参考 [message.rs](../../../src/notification/message.rs)*

---

## 统计结构

### BrokerStatsSnapshot

```rust
pub struct BrokerStatsSnapshot {
    pub messages_sent: u64,
    pub messages_deduplicated: u64,
    pub messages_dropped: u64,
    pub active_users: usize,
    pub active_gateways: usize,
    pub queue_sizes: [usize; 4],  // [P0, P1, P2, P3]
}
```

### GatewayStatsSnapshot

```rust
pub struct GatewayStatsSnapshot {
    pub gateway_id: Arc<str>,
    pub messages_pushed: u64,
    pub messages_failed: u64,
    pub active_sessions: usize,
}
```

---

## 使用示例

### 完整示例

```rust
use qaexchange::notification::*;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建 Broker 和 Gateway
    let broker = Arc::new(NotificationBroker::new());
    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", gateway_rx));

    // 2. 连接组件
    broker.register_gateway("gateway_01", gateway_tx);
    broker.subscribe("user_01", "gateway_01");

    // 3. 注册 WebSocket 会话
    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 4. 启动任务
    let _processor = broker.clone().start_priority_processor();
    let _pusher = gateway.clone().start_notification_pusher();
    let _heartbeat = gateway.clone().start_heartbeat_checker();

    // 5. 发送通知
    let notification = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        NotificationPayload::AccountUpdate(AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 1000000.0,
            available: 980000.0,
            frozen: 0.0,
            margin: 20000.0,
            position_profit: 500.0,
            close_profit: 1000.0,
            risk_ratio: 0.02,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        }),
        "AccountSystem",
    );

    broker.publish(notification)?;

    // 6. 接收 WebSocket 消息
    if let Some(json) = session_rx.recv().await {
        println!("Received: {}", json);
    }

    // 7. 查看统计
    let stats = broker.get_stats();
    println!("Broker stats: {:?}", stats);

    let gw_stats = gateway.get_stats();
    println!("Gateway stats: {:?}", gw_stats);

    Ok(())
}
```

### rkyv 序列化示例

```rust
// 序列化
let notification = Notification::new(...);
let bytes = notification.to_rkyv_bytes()?;

// 跨进程发送
send_to_other_process(&bytes);

// 接收端：零拷贝反序列化
let archived = Notification::from_rkyv_bytes(&bytes)?;
println!("User: {}", archived.user_id);  // 直接访问，无需分配内存

// 需要完整所有权时
let notification = Notification::from_archived(archived)?;
```

---

## 错误处理

### Broker 错误

```rust
match broker.publish(notification) {
    Ok(()) => println!("Published"),
    Err(e) => eprintln!("Failed to publish: {}", e),
}
```

### Gateway 错误

Gateway 内部错误通过日志记录，不返回错误。使用统计信息监控失败：

```rust
let stats = gateway.get_stats();
if stats.messages_failed > 0 {
    eprintln!("Gateway has {} failed messages", stats.messages_failed);
}
```

---

## 性能建议

### 1. 使用 Arc 避免克隆

```rust
// ✅ 好
let user_id = Arc::from("user_01");
let notification = Notification::new(NotificationType::..., user_id, ...);

// ❌ 差
let notification = Notification::new(NotificationType::..., "user_01".to_string(), ...);
```

### 2. 批量发布

```rust
// 批量发布时，重用 payload
let payload = NotificationPayload::AccountUpdate(...);
for user_id in user_ids {
    let notification = Notification::new(
        NotificationType::AccountUpdate,
        user_id.clone(),
        payload.clone(),  // Clone payload
        "AccountSystem",
    );
    broker.publish(notification)?;
}
```

### 3. 使用 rkyv 进行跨进程通信

```rust
// ✅ 零拷贝反序列化
let archived = Notification::from_rkyv_bytes(&bytes)?;
process_archived(archived);

// ❌ 避免不必要的完整反序列化
let notification = Notification::from_archived(archived)?;  // 分配内存
```

---

## 相关链接

- [文档中心](../README.md)
- [系统设计](../01_DESIGN/SYSTEM_DESIGN.md)
- [集成指南](INTEGRATION_GUIDE.md)
- [源代码](../../../src/notification/)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
