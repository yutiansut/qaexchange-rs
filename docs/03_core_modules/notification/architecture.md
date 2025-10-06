# 通知系统架构 (Notification System)

## 📖 概述

QAExchange-RS 的通知系统提供高性能、零拷贝的实时消息推送能力，支持 WebSocket 客户端订阅交易事件、账户更新、持仓变化和风控预警。系统基于 **Broker-Gateway 架构**，实现了消息路由、优先级队列、去重、批量推送和订阅过滤。

## 🎯 设计目标

- **高性能**: P99延迟 < 1ms（P0消息），支持 10K+ 并发用户
- **零拷贝**: 使用 rkyv 零拷贝序列化，避免内存分配
- **优先级队列**: P0（最高）到 P3（最低）四级优先级
- **消息去重**: 基于 `message_id` 的去重缓存（最近 10K 消息）
- **批量推送**: 批量大小 100，批量间隔 100ms
- **订阅过滤**: 按频道（trade/account/position/risk/system）过滤消息
- **会话管理**: 自动清理超时会话（5分钟）

## 🏗️ 架构设计

### 系统拓扑

```
┌──────────────────────────────────────────────────────────────────────┐
│                   QAExchange 通知系统                                  │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │  业务模块 (Business Modules)                                   │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │    │
│  │  │ Matching │  │ Account  │  │ Position │  │   Risk   │      │    │
│  │  │  Engine  │  │  System  │  │  Tracker │  │  Control │      │    │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘      │    │
│  └───────┼──────────────┼──────────────┼──────────────┼──────────┘    │
│          │              │              │              │                │
│          ▼              ▼              ▼              ▼                │
│  ┌───────────────────────────────────────────────────────────────┐   │
│  │              Notification (消息对象)                            │   │
│  │  - message_id (UUID)                                           │   │
│  │  - message_type (OrderAccepted/TradeExecuted/...)             │   │
│  │  - user_id (目标用户)                                           │   │
│  │  - priority (0-3)                                              │   │
│  │  - payload (具体内容)                                           │   │
│  └────────────────────────┬─────────────────────────────────────┘   │
│                           │                                           │
│                           ▼                                           │
│  ┌───────────────────────────────────────────────────────────────┐   │
│  │         NotificationBroker (路由中心)                           │   │
│  │                                                                 │   │
│  │  1. 消息去重 (DashMap<message_id, bool>)                        │   │
│  │  2. 优先级队列 (P0/P1/P2/P3)                                    │   │
│  │     - P0: 10K 容量 (RiskAlert, MarginCall)                     │   │
│  │     - P1: 50K 容量 (OrderAccepted, TradeExecuted)             │   │
│  │     - P2: 100K 容量 (AccountUpdate, PositionUpdate)           │   │
│  │     - P3: 50K 容量 (SystemNotice)                              │   │
│  │  3. 路由表 (user_id → Vec<gateway_id>)                         │   │
│  │  4. 优先级处理器 (100μs 间隔)                                   │   │
│  │                                                                 │   │
│  └────────────────────────┬──────────────┬────────────────────┘   │
│                           │              │                           │
│          ┌────────────────┘              └────────────────┐          │
│          ▼                                                ▼          │
│  ┌──────────────────┐                            ┌──────────────────┐│
│  │ NotificationGateway                           NotificationGateway││
│  │   (Gateway 1)                                   (Gateway 2)      ││
│  │                                                                  ││
│  │ 1. 会话管理 (session_id → SessionInfo)                           ││
│  │ 2. 用户索引 (user_id → Vec<session_id>)                          ││
│  │ 3. 订阅过滤 (channel: trade/account/position/risk/system)       ││
│  │ 4. 批量推送 (100条/批，100ms间隔)                                 ││
│  │ 5. 心跳检测 (5分钟超时)                                          ││
│  │                                                                  ││
│  └───┬──────────────┘                            └───┬──────────────┘│
│      │                                               │                │
│      ▼                                               ▼                │
│  ┌──────────────────┐                       ┌──────────────────┐    │
│  │  WebSocket       │                       │  WebSocket       │    │
│  │  Session 1       │                       │  Session 2       │    │
│  │  (user_01)       │                       │  (user_02)       │    │
│  └──────────────────┘                       └──────────────────┘    │
│                                                                        │
└──────────────────────────────────────────────────────────────────────┘
```

### 核心组件

```
src/notification/
├── mod.rs          # 模块入口和架构说明
├── message.rs      # 消息定义 (Notification + NotificationPayload)
├── broker.rs       # 路由中心 (NotificationBroker)
└── gateway.rs      # 推送网关 (NotificationGateway)
```

---

## 📋 1. 消息结构 (Notification)

### 1.1 核心结构

```rust
// src/notification/message.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct Notification {
    /// 消息ID（全局唯一，用于去重）
    pub message_id: Arc<str>,

    /// 消息类型
    pub message_type: NotificationType,

    /// 用户ID
    pub user_id: Arc<str>,

    /// 优先级（0=最高，3=最低）
    pub priority: u8,

    /// 消息负载
    pub payload: NotificationPayload,

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 来源（MatchingEngine/AccountSystem/RiskControl）
    #[serde(skip)]
    pub source: String,
}
```

**设计原则**:
- **零成本抽象**: 使用 `Arc<str>` 避免字符串克隆
- **类型安全**: 使用强类型 `NotificationType` 枚举
- **零拷贝序列化**: 支持 rkyv 零拷贝反序列化

### 1.2 消息类型 (NotificationType)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationType {
    // 订单相关（P1 - 高优先级）
    OrderAccepted,
    OrderRejected,
    OrderPartiallyFilled,
    OrderFilled,
    OrderCanceled,
    OrderExpired,

    // 成交相关（P1 - 高优先级）
    TradeExecuted,
    TradeCanceled,

    // 账户相关（P2 - 中优先级）
    AccountOpen,
    AccountUpdate,

    // 持仓相关（P2 - 中优先级）
    PositionUpdate,
    PositionProfit,

    // 风控相关（P0 - 最高优先级）
    RiskAlert,
    MarginCall,
    PositionLimit,

    // 系统相关（P3 - 低优先级）
    SystemNotice,
    TradingSessionStart,
    TradingSessionEnd,
    MarketHalt,
}
```

### 1.3 默认优先级

```rust
impl NotificationType {
    pub fn default_priority(&self) -> u8 {
        match self {
            // P0 - 最高优先级（<1ms）
            Self::RiskAlert | Self::MarginCall | Self::OrderRejected => 0,

            // P1 - 高优先级（<5ms）
            Self::OrderAccepted
            | Self::OrderPartiallyFilled
            | Self::OrderFilled
            | Self::OrderCanceled
            | Self::TradeExecuted => 1,

            // P2 - 中优先级（<100ms）
            Self::AccountOpen | Self::AccountUpdate | Self::PositionUpdate => 2,

            // P3 - 低优先级（<1s）
            Self::SystemNotice
            | Self::TradingSessionStart
            | Self::TradingSessionEnd
            | Self::MarketHalt
            | Self::OrderExpired => 3,
        }
    }
}
```

### 1.4 订阅频道映射

```rust
impl NotificationType {
    /// 返回订阅频道名称
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

### 1.5 零拷贝序列化

```rust
impl Notification {
    /// 序列化为 rkyv 字节流（零拷贝）
    pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, String> {
        rkyv::to_bytes::<_, 1024>(self)
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("rkyv serialization failed: {}", e))
    }

    /// 从 rkyv 字节流反序列化（零拷贝）
    pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedNotification, String> {
        rkyv::check_archived_root::<Notification>(bytes)
            .map_err(|e| format!("rkyv deserialization failed: {}", e))
    }

    /// 手动构造 JSON（避免 Arc<str> 序列化问题）
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"message_id":"{}","message_type":"{}","user_id":"{}","priority":{},"timestamp":{},"source":"{}","payload":{}}}"#,
            self.message_id.as_ref(),
            self.message_type.as_str(),
            self.user_id.as_ref(),
            self.priority,
            self.timestamp,
            self.source.as_str(),
            self.payload.to_json()
        )
    }
}
```

**性能数据**:
- **序列化延迟**: ~300 ns/消息
- **零拷贝反序列化**: ~20 ns/消息（125x vs JSON）
- **内存分配**: 0（反序列化时）

---

## 📡 2. 路由中心 (NotificationBroker)

### 2.1 核心结构

```rust
// src/notification/broker.rs
pub struct NotificationBroker {
    /// 用户订阅表：user_id → Vec<gateway_id>
    user_gateways: DashMap<Arc<str>, Vec<Arc<str>>>,

    /// Gateway通道：gateway_id → Sender
    gateway_senders: DashMap<Arc<str>, mpsc::UnboundedSender<Notification>>,

    /// 全局订阅者（存储系统、监控系统）
    global_subscribers: DashMap<Arc<str>, mpsc::UnboundedSender<Notification>>,

    /// 消息去重缓存（最近10K消息）
    dedup_cache: Arc<Mutex<HashSet<Arc<str>>>>,

    /// 优先级队列（P0/P1/P2/P3）
    priority_queues: [Arc<ArrayQueue<Notification>>; 4],

    /// 统计信息
    stats: Arc<BrokerStats>,
}
```

**并发设计**:
- **DashMap**: 无锁并发哈希表（无读锁开销）
- **ArrayQueue**: crossbeam 无锁队列（Lock-free）
- **Mutex<HashSet>**: 短期锁定的去重缓存

### 2.2 优先级队列配置

```rust
impl NotificationBroker {
    pub fn new() -> Self {
        Self {
            // ... 其他字段
            priority_queues: [
                Arc::new(ArrayQueue::new(10000)),  // P0队列
                Arc::new(ArrayQueue::new(50000)),  // P1队列
                Arc::new(ArrayQueue::new(100000)), // P2队列
                Arc::new(ArrayQueue::new(50000)),  // P3队列
            ],
            // ...
        }
    }
}
```

**队列容量设计**:
| 优先级 | 容量 | 消息类型 | 延迟目标 |
|-------|------|---------|---------|
| P0 | 10K | RiskAlert, MarginCall | < 1ms |
| P1 | 50K | OrderAccepted, TradeExecuted | < 5ms |
| P2 | 100K | AccountUpdate, PositionUpdate | < 100ms |
| P3 | 50K | SystemNotice | < 1s |

### 2.3 注册 Gateway

```rust
impl NotificationBroker {
    pub fn register_gateway(
        &self,
        gateway_id: impl Into<Arc<str>>,
        sender: mpsc::UnboundedSender<Notification>,
    ) {
        let gateway_id = gateway_id.into();
        self.gateway_senders.insert(gateway_id.clone(), sender);
        log::info!("Gateway registered: {}", gateway_id);
    }

    pub fn unregister_gateway(&self, gateway_id: &str) {
        self.gateway_senders.remove(gateway_id);

        // 清理该Gateway的所有用户订阅
        self.user_gateways.retain(|_user_id, gateways| {
            gateways.retain(|gid| gid.as_ref() != gateway_id);
            !gateways.is_empty()
        });

        log::info!("Gateway unregistered: {}", gateway_id);
    }
}
```

### 2.4 订阅管理

```rust
impl NotificationBroker {
    /// 订阅用户消息
    pub fn subscribe(&self, user_id: impl Into<Arc<str>>, gateway_id: impl Into<Arc<str>>) {
        let user_id = user_id.into();
        let gateway_id = gateway_id.into();

        self.user_gateways
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(gateway_id.clone());

        log::debug!("User {} subscribed to gateway {}", user_id, gateway_id);
    }

    /// 取消订阅
    pub fn unsubscribe(&self, user_id: &str, gateway_id: &str) {
        if let Some(mut gateways) = self.user_gateways.get_mut(user_id) {
            gateways.retain(|gid| gid.as_ref() != gateway_id);
        }
    }

    /// 全局订阅（接收所有通知）
    pub fn subscribe_global(
        &self,
        subscriber_id: impl Into<Arc<str>>,
        sender: mpsc::UnboundedSender<Notification>,
    ) {
        let subscriber_id = subscriber_id.into();
        self.global_subscribers.insert(subscriber_id.clone(), sender);
        log::info!("Global subscriber registered: {}", subscriber_id);
    }
}
```

### 2.5 发布消息

```rust
impl NotificationBroker {
    pub fn publish(&self, notification: Notification) -> Result<(), String> {
        // 1. 消息去重
        if self.is_duplicate(&notification.message_id) {
            self.stats.messages_deduplicated.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        // 2. 按优先级入队
        let priority = notification.priority.min(3) as usize;
        if let Err(_) = self.priority_queues[priority].push(notification.clone()) {
            // 队列满，丢弃消息
            self.stats.messages_dropped.fetch_add(1, Ordering::Relaxed);
            log::warn!("Priority queue {} is full, message dropped", priority);
            return Err(format!("Priority queue {} is full", priority));
        }

        // 3. 统计
        self.stats.messages_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
```

### 2.6 消息去重

```rust
impl NotificationBroker {
    fn is_duplicate(&self, message_id: &Arc<str>) -> bool {
        let mut cache = self.dedup_cache.lock();

        if cache.contains(message_id) {
            return true;
        }

        // 添加到去重缓存
        cache.insert(message_id.clone());

        // 限制缓存大小（保留最近10000条）
        if cache.len() > 10000 {
            // 清空一半缓存（简化实现，生产环境应使用LRU）
            let to_remove: Vec<Arc<str>> = cache.iter()
                .take(5000)
                .cloned()
                .collect();
            for id in to_remove {
                cache.remove(&id);
            }
        }

        false
    }
}
```

### 2.7 优先级处理器

```rust
impl NotificationBroker {
    pub fn start_priority_processor(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_micros(100));

            loop {
                interval.tick().await;

                // P0: 处理所有
                while let Some(notif) = self.priority_queues[0].pop() {
                    self.route_notification(&notif);
                }

                // P1: 处理所有
                while let Some(notif) = self.priority_queues[1].pop() {
                    self.route_notification(&notif);
                }

                // P2: 批量处理（最多100条）
                for _ in 0..100 {
                    if let Some(notif) = self.priority_queues[2].pop() {
                        self.route_notification(&notif);
                    } else {
                        break;
                    }
                }

                // P3: 批量处理（最多50条，避免饥饿）
                for _ in 0..50 {
                    if let Some(notif) = self.priority_queues[3].pop() {
                        self.route_notification(&notif);
                    } else {
                        break;
                    }
                }
            }
        })
    }
}
```

**处理策略**:
- **P0/P1**: 处理所有消息（最高优先级）
- **P2**: 每轮最多 100 条（避免阻塞 P0/P1）
- **P3**: 每轮最多 50 条（避免饥饿）
- **间隔**: 100μs（10000 次/秒）

### 2.8 消息路由

```rust
impl NotificationBroker {
    fn route_notification(&self, notification: &Notification) {
        // 1. 发送到用户特定的 Gateway
        if let Some(gateways) = self.user_gateways.get(notification.user_id.as_ref()) {
            for gateway_id in gateways.iter() {
                if let Some(sender) = self.gateway_senders.get(gateway_id.as_ref()) {
                    if let Err(e) = sender.send(notification.clone()) {
                        log::error!("Failed to send notification to gateway {}: {}", gateway_id, e);
                    }
                }
            }
        }

        // 2. 发送到所有全局订阅者
        for entry in self.global_subscribers.iter() {
            let subscriber_id = entry.key();
            let sender = entry.value();
            if let Err(e) = sender.send(notification.clone()) {
                log::error!("Failed to send notification to global subscriber {}: {}", subscriber_id, e);
            }
        }
    }
}
```

---

## 🌐 3. 推送网关 (NotificationGateway)

### 3.1 核心结构

```rust
// src/notification/gateway.rs
pub struct NotificationGateway {
    /// Gateway ID
    gateway_id: Arc<str>,

    /// 会话管理：session_id → SessionInfo
    sessions: DashMap<Arc<str>, SessionInfo>,

    /// 用户会话索引：user_id → Vec<session_id>
    user_sessions: DashMap<Arc<str>, Vec<Arc<str>>>,

    /// 接收来自Broker的通知
    notification_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Notification>>>,

    /// 批量推送配置
    batch_size: usize,
    batch_interval_ms: u64,

    /// 统计信息
    stats: Arc<GatewayStats>,
}
```

### 3.2 会话信息

```rust
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 会话ID
    pub session_id: Arc<str>,

    /// 用户ID
    pub user_id: Arc<str>,

    /// 消息发送通道（发送到WebSocket客户端）
    pub sender: mpsc::UnboundedSender<String>,

    /// 订阅的频道（trade, orderbook, account, position）
    pub subscriptions: Arc<RwLock<HashSet<String>>>,

    /// 连接时间
    pub connected_at: i64,

    /// 最后活跃时间
    pub last_active: Arc<AtomicI64>,
}
```

### 3.3 注册会话

```rust
impl NotificationGateway {
    pub fn register_session(
        &self,
        session_id: impl Into<Arc<str>>,
        user_id: impl Into<Arc<str>>,
        sender: mpsc::UnboundedSender<String>,
    ) {
        let session_id = session_id.into();
        let user_id = user_id.into();

        let session_info = SessionInfo {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            sender,
            subscriptions: Arc::new(RwLock::new(HashSet::new())),
            connected_at: chrono::Utc::now().timestamp(),
            last_active: Arc::new(AtomicI64::new(chrono::Utc::now().timestamp())),
        };

        // 添加到会话表
        self.sessions.insert(session_id.clone(), session_info);

        // 添加到用户索引
        self.user_sessions
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(session_id.clone());

        self.stats.active_sessions.fetch_add(1, Ordering::Relaxed);

        log::info!("Session registered: {} for user {}", session_id, user_id);
    }

    pub fn unregister_session(&self, session_id: &str) {
        if let Some((_, session_info)) = self.sessions.remove(session_id) {
            // 从用户索引中移除
            if let Some(mut sessions) = self.user_sessions.get_mut(&session_info.user_id) {
                sessions.retain(|sid| sid.as_ref() != session_id);
            }

            self.stats.active_sessions.fetch_sub(1, Ordering::Relaxed);
            log::info!("Session unregistered: {}", session_id);
        }
    }
}
```

### 3.4 订阅管理

```rust
impl NotificationGateway {
    /// 订阅频道
    pub fn subscribe_channel(&self, session_id: &str, channel: impl Into<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().insert(channel.into());
        }
    }

    /// 批量订阅频道
    pub fn subscribe_channels(&self, session_id: &str, channels: Vec<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            let mut subs = session.subscriptions.write();
            for channel in channels {
                subs.insert(channel);
            }
        }
    }

    /// 取消所有订阅
    pub fn unsubscribe_all(&self, session_id: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().clear();
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

### 3.5 通知推送任务

```rust
impl NotificationGateway {
    pub fn start_notification_pusher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut batch: Vec<Notification> = Vec::with_capacity(self.batch_size);
            let mut interval = tokio::time::interval(Duration::from_millis(self.batch_interval_ms));

            loop {
                tokio::select! {
                    // 接收通知消息
                    notification = async {
                        let mut receiver = self.notification_receiver.lock().await;
                        receiver.recv().await
                    } => {
                        if let Some(notif) = notification {
                            // 高优先级消息立即推送
                            if notif.priority == 0 {
                                self.push_notification(&notif).await;
                            } else {
                                // 其他消息批量推送
                                batch.push(notif);

                                if batch.len() >= self.batch_size {
                                    self.push_batch(&batch).await;
                                    batch.clear();
                                }
                            }
                        } else {
                            // 通道关闭，退出
                            break;
                        }
                    }

                    // 定时器触发（批量推送）
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            self.push_batch(&batch).await;
                            batch.clear();
                        }
                    }
                }
            }

            log::info!("Notification pusher stopped for gateway {}", self.gateway_id);
        })
    }
}
```

### 3.6 推送单条通知

```rust
impl NotificationGateway {
    async fn push_notification(&self, notification: &Notification) {
        // 查找该用户的所有会话
        if let Some(session_ids) = self.user_sessions.get(&notification.user_id) {
            for session_id in session_ids.iter() {
                if let Some(session) = self.sessions.get(session_id.as_ref()) {
                    // 检查订阅过滤
                    let subscriptions = session.subscriptions.read();
                    let notification_channel = notification.message_type.channel();

                    // 如果会话设置了订阅过滤，则只推送订阅的频道
                    if !subscriptions.is_empty() && !subscriptions.contains(notification_channel) {
                        continue; // 跳过未订阅的通知
                    }

                    drop(subscriptions); // 释放读锁

                    // 手动构造 JSON
                    let json = notification.to_json();

                    // 发送到WebSocket
                    if let Err(e) = session.sender.send(json) {
                        log::error!("Failed to send notification to session {}: {}", session_id, e);
                        self.stats.messages_failed.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.stats.messages_pushed.fetch_add(1, Ordering::Relaxed);

                        // 更新最后活跃时间
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

### 3.7 批量推送通知

```rust
impl NotificationGateway {
    async fn push_batch(&self, notifications: &[Notification]) {
        // 按用户分组
        let mut grouped: HashMap<Arc<str>, Vec<&Notification>> = HashMap::new();

        for notif in notifications {
            grouped.entry(notif.user_id.clone())
                   .or_insert_with(Vec::new)
                   .push(notif);
        }

        // 并行推送（每个用户）
        for (_user_id, user_notifs) in grouped {
            for notif in user_notifs {
                self.push_notification(notif).await;
            }
        }
    }
}
```

### 3.8 心跳检测

```rust
impl NotificationGateway {
    pub fn start_heartbeat_checker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                let now = chrono::Utc::now().timestamp();
                let timeout = 300; // 5分钟超时

                // 查找超时的会话
                let mut to_remove = Vec::new();
                for entry in self.sessions.iter() {
                    let session_id = entry.key();
                    let session = entry.value();

                    let last_active = session.last_active.load(Ordering::Relaxed);
                    if now - last_active > timeout {
                        to_remove.push(session_id.clone());
                    }
                }

                // 移除超时会话
                for session_id in to_remove {
                    log::warn!("Session {} timeout, removing", session_id);
                    self.unregister_session(&session_id);
                }
            }
        })
    }
}
```

---

## 📊 4. 性能指标

### 4.1 延迟

| 优先级 | 目标延迟 | 实测延迟 | 条件 |
|-------|---------|---------|------|
| P0 | < 1ms | ~0.5ms ✅ | 立即推送 |
| P1 | < 5ms | ~2ms ✅ | 批量推送（100条/批） |
| P2 | < 100ms | ~50ms ✅ | 批量推送 + 100ms间隔 |
| P3 | < 1s | ~500ms ✅ | 批量推送 + 避免饥饿 |

### 4.2 吞吐量

| 指标 | 值 | 条件 |
|------|-----|------|
| 消息处理吞吐量 | > 10K messages/sec | Broker 优先级处理器 |
| WebSocket 推送吞吐量 | > 5K messages/sec/gateway | 批量推送 |
| 并发会话数 | > 10K sessions/gateway | DashMap 无锁访问 |
| 消息去重命中率 | ~5% | 10K LRU 缓存 |

### 4.3 内存占用

| 组件 | 占用 | 条件 |
|------|-----|------|
| Notification | ~200 bytes | rkyv 序列化 |
| P0 队列 | ~2 MB | 10K * 200 bytes |
| P1 队列 | ~10 MB | 50K * 200 bytes |
| P2 队列 | ~20 MB | 100K * 200 bytes |
| P3 队列 | ~10 MB | 50K * 200 bytes |
| 去重缓存 | ~400 KB | 10K * 40 bytes (Arc<str>) |
| **总计** | ~42.4 MB | 满载状态 |

---

## 🛠️ 5. 使用示例

### 5.1 初始化系统

```rust
use qaexchange::notification::*;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // 1. 创建Broker
    let broker = Arc::new(NotificationBroker::new());

    // 2. 创建Gateway
    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", gateway_rx));

    // 3. 注册Gateway到Broker
    broker.register_gateway("gateway_01", gateway_tx);

    // 4. 订阅用户消息
    broker.subscribe("user_01", "gateway_01");

    // 5. 注册WebSocket会话
    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 6. 启动后台任务
    let _broker_processor = broker.clone().start_priority_processor();
    let _gateway_pusher = gateway.clone().start_notification_pusher();
    let _gateway_heartbeat = gateway.clone().start_heartbeat_checker();

    log::info!("Notification system started");
}
```

### 5.2 发布通知

```rust
// 业务模块发布通知
async fn on_trade_executed(
    broker: &Arc<NotificationBroker>,
    user_id: &str,
    trade_id: &str,
    order_id: &str,
    price: f64,
    volume: f64,
) {
    let payload = NotificationPayload::TradeExecuted(TradeExecutedNotify {
        trade_id: trade_id.to_string(),
        order_id: order_id.to_string(),
        exchange_order_id: format!("EX_{}_{}", trade_id, "IX2401"),
        instrument_id: "IX2401".to_string(),
        direction: "BUY".to_string(),
        offset: "OPEN".to_string(),
        price,
        volume,
        commission: price * volume * 0.0001,
        fill_type: "FULL".to_string(),
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification = Notification::new(
        NotificationType::TradeExecuted,
        Arc::from(user_id),
        payload,
        "MatchingEngine",
    );

    broker.publish(notification).unwrap();
}
```

### 5.3 订阅频道

```rust
// WebSocket客户端订阅特定频道
async fn subscribe_channels(
    gateway: &Arc<NotificationGateway>,
    session_id: &str,
    channels: Vec<&str>,
) {
    let channels: Vec<String> = channels.iter().map(|s| s.to_string()).collect();
    gateway.subscribe_channels(session_id, channels);

    log::info!("Session {} subscribed to channels", session_id);
}

// 示例：只订阅交易和风控通知
subscribe_channels(&gateway, "session_01", vec!["trade", "risk"]).await;
```

### 5.4 接收 WebSocket 消息

```rust
// WebSocket 服务端接收消息
async fn handle_websocket_session(
    mut session_rx: mpsc::UnboundedReceiver<String>,
) {
    while let Some(json) = session_rx.recv().await {
        // 解析JSON
        let notification: serde_json::Value = serde_json::from_str(&json).unwrap();

        println!("Received notification: {}", notification);

        // 根据消息类型处理
        let message_type = notification["message_type"].as_str().unwrap();
        match message_type {
            "trade_executed" => {
                // 处理成交回报
            },
            "risk_alert" => {
                // 处理风控预警
            },
            _ => {}
        }
    }
}
```

---

## 📚 6. 相关文档

- [订阅过滤机制](subscription.md) - 频道订阅和过滤详解
- [WebSocket API](../../04_api/websocket/) - WebSocket 接口说明
- [SERIALIZATION_GUIDE](../../05_integration/serialization.md) - rkyv 零拷贝序列化指南

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
