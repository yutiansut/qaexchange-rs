# 订阅过滤机制 (Subscription Filtering)

## 📖 概述

QAExchange-RS 的通知系统提供灵活的订阅过滤机制，允许客户端选择性接收感兴趣的消息类型。通过订阅特定**频道**（channel），客户端可以减少不必要的网络传输和CPU开销，提升系统整体性能。

## 🎯 设计目标

- **按需订阅**: 客户端只接收订阅频道的消息
- **动态管理**: 支持运行时动态添加/删除订阅
- **零配置默认**: 未设置订阅时接收所有消息
- **高效过滤**: O(1) 哈希表查找，无性能开销
- **频道隔离**: 不同频道互不干扰

## 🏗️ 频道分类

### 频道定义

QAExchange 定义了 **5 个核心频道**：

| 频道 | 说明 | 消息类型 | 典型用例 |
|------|------|---------|---------|
| **trade** | 交易相关 | OrderAccepted, OrderFilled, TradeExecuted, OrderCanceled | 交易终端、策略监控 |
| **account** | 账户相关 | AccountOpen, AccountUpdate | 资金管理、财务监控 |
| **position** | 持仓相关 | PositionUpdate, PositionProfit | 持仓监控、风险分析 |
| **risk** | 风控相关 | RiskAlert, MarginCall, PositionLimit | 风控系统、预警监控 |
| **system** | 系统相关 | SystemNotice, TradingSessionStart, MarketHalt | 系统状态监控 |

### 频道映射规则

```rust
// src/notification/message.rs
impl NotificationType {
    pub fn channel(&self) -> &'static str {
        match self {
            // 交易频道
            Self::OrderAccepted
            | Self::OrderRejected
            | Self::OrderPartiallyFilled
            | Self::OrderFilled
            | Self::OrderCanceled
            | Self::OrderExpired
            | Self::TradeExecuted
            | Self::TradeCanceled => "trade",

            // 账户频道
            Self::AccountOpen | Self::AccountUpdate => "account",

            // 持仓频道
            Self::PositionUpdate | Self::PositionProfit => "position",

            // 风控频道
            Self::RiskAlert | Self::MarginCall | Self::PositionLimit => "risk",

            // 系统频道
            Self::SystemNotice
            | Self::TradingSessionStart
            | Self::TradingSessionEnd
            | Self::MarketHalt => "system",
        }
    }
}
```

---

## 📋 1. 订阅数据结构

### 1.1 SessionInfo 结构

```rust
// src/notification/gateway.rs
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 会话ID
    pub session_id: Arc<str>,

    /// 用户ID
    pub user_id: Arc<str>,

    /// 消息发送通道
    pub sender: mpsc::UnboundedSender<String>,

    /// 订阅的频道（trade, account, position, risk, system）
    pub subscriptions: Arc<RwLock<HashSet<String>>>,

    /// 连接时间
    pub connected_at: i64,

    /// 最后活跃时间
    pub last_active: Arc<AtomicI64>,
}
```

**关键设计**:
- `subscriptions: Arc<RwLock<HashSet<String>>>` - 订阅频道集合
- **默认为空**: 未订阅时 `HashSet` 为空，表示接收所有消息
- **读写锁**: 使用 `parking_lot::RwLock` 高性能读写锁
- **Arc 共享**: 允许多线程访问

### 1.2 订阅状态

```
┌─────────────────────────────────────────────────────────┐
│  订阅状态                                                │
│                                                           │
│  ┌─────────────┐         ┌──────────────────┐           │
│  │ 未订阅      │         │ 已订阅特定频道     │           │
│  │             │         │                  │           │
│  │ HashSet::new()       │ {"trade", "risk"} │           │
│  │ (len = 0)   │         │ (len = 2)        │           │
│  └─────────────┘         └──────────────────┘           │
│        │                         │                       │
│        ▼                         ▼                       │
│  接收所有消息               只接收订阅频道消息            │
│                                                           │
└─────────────────────────────────────────────────────────┘
```

---

## 📡 2. 订阅管理 API

### 2.1 订阅单个频道

```rust
// src/notification/gateway.rs
impl NotificationGateway {
    /// 订阅频道
    pub fn subscribe_channel(&self, session_id: &str, channel: impl Into<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().insert(channel.into());
            log::debug!("Session {} subscribed to channel", session_id);
        }
    }
}
```

**使用示例**:
```rust
gateway.subscribe_channel("session_01", "trade");
```

### 2.2 批量订阅频道

```rust
impl NotificationGateway {
    /// 批量订阅频道
    pub fn subscribe_channels(&self, session_id: &str, channels: Vec<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            let mut subs = session.subscriptions.write();
            for channel in channels {
                subs.insert(channel);
            }
            log::debug!("Session {} subscribed to {} channels", session_id, subs.len());
        }
    }
}
```

**使用示例**:
```rust
gateway.subscribe_channels(
    "session_01",
    vec!["trade".to_string(), "account".to_string(), "risk".to_string()]
);
```

### 2.3 取消订阅单个频道

```rust
impl NotificationGateway {
    /// 取消订阅频道
    pub fn unsubscribe_channel(&self, session_id: &str, channel: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().remove(channel);
            log::debug!("Session {} unsubscribed from channel {}", session_id, channel);
        }
    }
}
```

**使用示例**:
```rust
gateway.unsubscribe_channel("session_01", "account");
```

### 2.4 取消所有订阅

```rust
impl NotificationGateway {
    /// 取消所有订阅
    pub fn unsubscribe_all(&self, session_id: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().clear();
            log::debug!("Session {} unsubscribed from all channels", session_id);
        }
    }
}
```

**使用示例**:
```rust
gateway.unsubscribe_all("session_01");
```

### 2.5 查询订阅状态

```rust
impl NotificationGateway {
    /// 获取会话的订阅列表
    pub fn get_subscriptions(&self, session_id: &str) -> Vec<String> {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.read().iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// 检查会话是否订阅了特定频道
    pub fn is_subscribed(&self, session_id: &str, channel: &str) -> bool {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.read().contains(channel)
        } else {
            false
        }
    }
}
```

**使用示例**:
```rust
// 查询订阅列表
let subs = gateway.get_subscriptions("session_01");
println!("Subscriptions: {:?}", subs); // ["trade", "risk"]

// 检查是否订阅
if gateway.is_subscribed("session_01", "trade") {
    println!("Subscribed to trade channel");
}
```

---

## 🔍 3. 过滤机制实现

### 3.1 推送时过滤

```rust
// src/notification/gateway.rs
impl NotificationGateway {
    async fn push_notification(&self, notification: &Notification) {
        // 查找该用户的所有会话
        if let Some(session_ids) = self.user_sessions.get(&notification.user_id) {
            for session_id in session_ids.iter() {
                if let Some(session) = self.sessions.get(session_id.as_ref()) {
                    // 检查订阅过滤
                    let subscriptions = session.subscriptions.read();
                    let notification_channel = notification.message_type.channel();

                    // 过滤规则：
                    // 1. 如果subscriptions为空（未订阅），则推送所有通知
                    // 2. 如果subscriptions非空，则只推送订阅的频道
                    if !subscriptions.is_empty() && !subscriptions.contains(notification_channel) {
                        log::trace!(
                            "Skipping notification {} for session {} (channel {} not subscribed)",
                            notification.message_id,
                            session_id,
                            notification_channel
                        );
                        continue; // 跳过未订阅的通知
                    }

                    drop(subscriptions); // 尽早释放读锁

                    // 发送到WebSocket
                    let json = notification.to_json();
                    if let Err(e) = session.sender.send(json) {
                        log::error!("Failed to send notification to session {}: {}", session_id, e);
                        self.stats.messages_failed.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.stats.messages_pushed.fetch_add(1, Ordering::Relaxed);
                        session.last_active.store(
                            chrono::Utc::now().timestamp(),
                            Ordering::Relaxed
                        );
                    }
                }
            }
        }
    }
}
```

### 3.2 过滤逻辑流程

```
┌────────────────────────────────────────────────────────────┐
│  过滤流程                                                    │
│                                                              │
│  Notification (message_type → channel)                      │
│         │                                                    │
│         ▼                                                    │
│  查找 User 的所有 Session                                     │
│         │                                                    │
│         ▼                                                    │
│  遍历每个 Session                                             │
│         │                                                    │
│         ▼                                                    │
│  ┌───────────────────────────────┐                          │
│  │ 检查订阅过滤                    │                          │
│  │                               │                          │
│  │ subscriptions.is_empty()?    │                          │
│  │    │                         │                          │
│  │    ├─ true  → 推送所有消息     │                          │
│  │    └─ false → 检查频道        │                          │
│  │                   │           │                          │
│  │                   ▼           │                          │
│  │         subscriptions.contains(channel)?                 │
│  │                   │                                      │
│  │                   ├─ true  → 推送消息                     │
│  │                   └─ false → 跳过消息                     │
│  └───────────────────────────────┘                          │
│         │                                                    │
│         ▼                                                    │
│  发送 JSON 到 WebSocket                                      │
│                                                              │
└────────────────────────────────────────────────────────────┘
```

### 3.3 性能分析

**时间复杂度**:
- **订阅检查**: O(1) - HashSet::contains
- **频道映射**: O(1) - 静态字符串映射
- **总体复杂度**: O(1)

**内存开销**:
- **每个频道**: ~8 bytes (String pointer)
- **最大订阅**: 5 channels * 8 bytes = 40 bytes
- **HashSet overhead**: ~24 bytes
- **总计**: ~64 bytes/session

---

## 💡 4. 使用场景

### 4.1 交易终端（只订阅交易）

```rust
// 交易终端只关心订单和成交，不需要账户更新
async fn setup_trading_terminal(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
) {
    gateway.subscribe_channel(session_id, "trade");

    log::info!("Trading terminal subscribed to trade channel");
}
```

**接收的消息**:
- ✅ OrderAccepted
- ✅ OrderFilled
- ✅ TradeExecuted
- ❌ AccountUpdate (不接收)
- ❌ PositionUpdate (不接收)

### 4.2 风控监控（只订阅风控）

```rust
// 风控监控只关心风险预警
async fn setup_risk_monitor(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
) {
    gateway.subscribe_channel(session_id, "risk");

    log::info!("Risk monitor subscribed to risk channel");
}
```

**接收的消息**:
- ✅ RiskAlert
- ✅ MarginCall
- ✅ PositionLimit
- ❌ OrderAccepted (不接收)
- ❌ AccountUpdate (不接收)

### 4.3 全量监控（订阅所有频道）

```rust
// 监控系统需要接收所有消息
async fn setup_full_monitor(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
) {
    // 方式1：订阅所有频道
    gateway.subscribe_channels(
        session_id,
        vec!["trade".to_string(), "account".to_string(), "position".to_string(), "risk".to_string(), "system".to_string()]
    );

    // 方式2：不订阅任何频道（默认接收所有）
    // gateway.unsubscribe_all(session_id);

    log::info!("Full monitor subscribed to all channels");
}
```

### 4.4 动态切换订阅

```rust
// 根据用户操作动态切换订阅
async fn switch_subscription_mode(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
    mode: &str,
) {
    // 先取消所有订阅
    gateway.unsubscribe_all(session_id);

    // 根据模式订阅
    match mode {
        "trading" => {
            gateway.subscribe_channel(session_id, "trade");
        },
        "monitoring" => {
            gateway.subscribe_channels(
                session_id,
                vec!["trade".to_string(), "risk".to_string()]
            );
        },
        "full" => {
            // 不订阅（接收所有）
        },
        _ => {}
    }

    log::info!("Switched to {} mode", mode);
}
```

---

## 🔧 5. WebSocket 协议

### 5.1 订阅请求

客户端通过 WebSocket 发送订阅请求：

```json
{
  "action": "subscribe",
  "channels": ["trade", "risk"]
}
```

### 5.2 取消订阅请求

```json
{
  "action": "unsubscribe",
  "channels": ["account"]
}
```

### 5.3 查询订阅状态

```json
{
  "action": "get_subscriptions"
}
```

**响应**:
```json
{
  "action": "subscriptions_response",
  "channels": ["trade", "risk"]
}
```

### 5.4 服务端实现

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SubscriptionRequest {
    Subscribe { channels: Vec<String> },
    Unsubscribe { channels: Vec<String> },
    GetSubscriptions,
}

#[derive(Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SubscriptionResponse {
    SubscriptionsResponse { channels: Vec<String> },
}

async fn handle_subscription_request(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
    request: SubscriptionRequest,
) -> Option<String> {
    match request {
        SubscriptionRequest::Subscribe { channels } => {
            gateway.subscribe_channels(session_id, channels);
            None
        },
        SubscriptionRequest::Unsubscribe { channels } => {
            for channel in channels {
                gateway.unsubscribe_channel(session_id, &channel);
            }
            None
        },
        SubscriptionRequest::GetSubscriptions => {
            let channels = gateway.get_subscriptions(session_id);
            let response = SubscriptionResponse::SubscriptionsResponse { channels };
            Some(serde_json::to_string(&response).unwrap())
        },
    }
}
```

---

## 📊 6. 性能优化

### 6.1 读写锁优化

```rust
// ❌ 不推荐：长时间持有读锁
let subscriptions = session.subscriptions.read();
let channel = notification.message_type.channel();
if !subscriptions.is_empty() && !subscriptions.contains(channel) {
    // 持有读锁期间执行其他操作
    do_something();
}
drop(subscriptions);

// ✅ 推荐：尽早释放读锁
let should_skip = {
    let subscriptions = session.subscriptions.read();
    let channel = notification.message_type.channel();
    !subscriptions.is_empty() && !subscriptions.contains(channel)
};

if should_skip {
    continue;
}
```

### 6.2 避免频繁锁竞争

```rust
// ❌ 不推荐：在循环中反复获取锁
for notification in notifications {
    let subscriptions = session.subscriptions.read();
    if subscriptions.contains(notification.channel()) {
        // 推送
    }
    drop(subscriptions);
}

// ✅ 推荐：一次获取锁，缓存结果
let subscriptions = session.subscriptions.read();
let subscribed_channels: HashSet<&str> = subscriptions.iter()
    .map(|s| s.as_str())
    .collect();
drop(subscriptions);

for notification in notifications {
    if subscribed_channels.contains(notification.channel()) {
        // 推送
    }
}
```

### 6.3 批量操作优化

```rust
// ✅ 批量订阅（推荐）
gateway.subscribe_channels(
    session_id,
    vec!["trade".to_string(), "account".to_string(), "risk".to_string()]
);

// ❌ 逐个订阅（不推荐）
gateway.subscribe_channel(session_id, "trade");
gateway.subscribe_channel(session_id, "account");
gateway.subscribe_channel(session_id, "risk");
```

---

## 🧪 7. 测试用例

### 7.1 基本订阅测试

```rust
#[tokio::test]
async fn test_channel_subscription() {
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = NotificationGateway::new("gateway_01", rx);

    let (session_tx, _session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 订阅 trade 频道
    gateway.subscribe_channel("session_01", "trade");

    // 验证订阅
    assert!(gateway.is_subscribed("session_01", "trade"));
    assert!(!gateway.is_subscribed("session_01", "account"));

    // 获取订阅列表
    let subs = gateway.get_subscriptions("session_01");
    assert_eq!(subs.len(), 1);
    assert!(subs.contains(&"trade".to_string()));
}
```

### 7.2 过滤测试

```rust
#[tokio::test]
async fn test_notification_filtering() {
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 只订阅 trade 频道
    gateway.subscribe_channel("session_01", "trade");

    // 启动推送任务
    let _handle = gateway.clone().start_notification_pusher();

    // 发送 trade 消息（应该收到）
    let trade_payload = NotificationPayload::OrderAccepted(/* ... */);
    let trade_notif = Notification::new(
        NotificationType::OrderAccepted,
        Arc::from("user_01"),
        trade_payload,
        "MatchingEngine",
    );
    tx.send(trade_notif).unwrap();

    // 发送 account 消息（不应该收到）
    let account_payload = NotificationPayload::AccountUpdate(/* ... */);
    let account_notif = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        account_payload,
        "AccountSystem",
    );
    tx.send(account_notif).unwrap();

    // 等待推送
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 验证：应该只收到1条消息（trade）
    let mut received_count = 0;
    while let Ok(Some(_json)) = tokio::time::timeout(
        Duration::from_millis(50),
        session_rx.recv()
    ).await {
        received_count += 1;
    }

    assert_eq!(received_count, 1);
}
```

### 7.3 默认行为测试

```rust
#[tokio::test]
async fn test_default_receives_all() {
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 不订阅任何频道（默认接收所有）
    // gateway.subscribe_channel(...) NOT CALLED

    // 启动推送任务
    let _handle = gateway.clone().start_notification_pusher();

    // 发送多种类型消息
    let notifications = vec![
        create_trade_notification("user_01"),
        create_account_notification("user_01"),
        create_risk_notification("user_01"),
    ];

    for notif in notifications {
        tx.send(notif).unwrap();
    }

    // 等待推送
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 验证：应该收到所有3条消息
    let mut received_count = 0;
    while let Ok(Some(_json)) = tokio::time::timeout(
        Duration::from_millis(50),
        session_rx.recv()
    ).await {
        received_count += 1;
    }

    assert_eq!(received_count, 3);
}
```

---

## 📚 8. 最佳实践

### 8.1 选择合适的订阅策略

| 场景 | 推荐订阅 | 原因 |
|------|---------|------|
| 交易终端 | `trade` | 只需要订单和成交信息 |
| 账户监控 | `account`, `position` | 关注资金和持仓变化 |
| 风控系统 | `risk` | 只处理风险预警 |
| 完整监控 | 不订阅（默认） | 接收所有消息 |
| 策略执行 | `trade`, `risk` | 交易执行 + 风险监控 |

### 8.2 动态调整订阅

```rust
// 根据用户行为动态调整订阅
async fn adjust_subscriptions_based_on_activity(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
    has_open_orders: bool,
    has_open_positions: bool,
) {
    let mut channels = Vec::new();

    // 有挂单时订阅 trade 频道
    if has_open_orders {
        channels.push("trade".to_string());
    }

    // 有持仓时订阅 position 和 risk 频道
    if has_open_positions {
        channels.push("position".to_string());
        channels.push("risk".to_string());
    }

    // 始终订阅 account 频道
    channels.push("account".to_string());

    gateway.unsubscribe_all(session_id);
    gateway.subscribe_channels(session_id, channels);
}
```

### 8.3 避免过度过滤

```rust
// ❌ 不推荐：过度细粒度订阅（单个消息类型）
// 这需要修改订阅机制，增加复杂度

// ✅ 推荐：使用频道级别订阅（5个频道）
gateway.subscribe_channels(session_id, vec!["trade".to_string(), "risk".to_string()]);
```

---

## 🔍 9. 故障排查

### 9.1 未收到消息

**症状**: 客户端未收到预期消息

**排查步骤**:
1. 检查订阅状态
   ```rust
   let subs = gateway.get_subscriptions(session_id);
   println!("Current subscriptions: {:?}", subs);
   ```

2. 检查消息频道
   ```rust
   let channel = notification.message_type.channel();
   println!("Notification channel: {}", channel);
   ```

3. 检查过滤日志
   ```rust
   log::trace!("Filtering notification {} for session {}", message_id, session_id);
   ```

### 9.2 收到不应该收到的消息

**症状**: 客户端收到未订阅频道的消息

**排查步骤**:
1. 确认订阅状态
2. 检查频道映射是否正确
3. 验证过滤逻辑

### 9.3 性能问题

**症状**: 订阅频道后性能下降

**排查步骤**:
1. 检查读写锁竞争
2. 使用批量订阅而非逐个订阅
3. 避免在推送路径上执行耗时操作

---

## 📚 10. 相关文档

- [通知系统架构](architecture.md) - 完整架构设计
- [WebSocket API](../../04_api/websocket/) - WebSocket 接口说明
- [消息类型定义](../../07_reference/notification_types.md) - 所有消息类型

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
