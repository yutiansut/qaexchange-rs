# 交易所通知消息系统设计

## 目录

1. [系统概述](#系统概述)
2. [业界标准调研](#业界标准调研)
3. [通知消息分类](#通知消息分类)
4. [架构设计](#架构设计)
5. [消息类型定义](#消息类型定义)
6. [通知路由机制](#通知路由机制)
7. [性能优化](#性能优化)
8. [实现方案](#实现方案)

---

## 系统概述

交易所通知消息系统负责将交易系统内部的状态变化（订单、成交、账户、持仓等）实时推送给客户端。

### 设计目标

- **实时性**：消息延迟 < 10ms（P99）
- **可靠性**：保证消息不丢失，支持断线重连后补发
- **顺序性**：保证同一账户的消息有序
- **可扩展性**：支持水平扩展，单网关支持 10,000+ WebSocket 连接

### 核心原则

1. **推送为主，查询为辅**：所有状态变化主动推送，减少客户端查询
2. **分级推送**：重要消息立即推送，次要消息批量推送
3. **用户隔离**：每个用户只接收自己的消息，保护隐私
4. **消息去重**：防止网络抖动导致的重复推送

---

## 业界标准调研

### 1. CTP（综合交易平台）

CTP是中国期货市场最广泛使用的交易API，其回调机制是行业标准。

#### 核心回调函数

```cpp
// 订单委托回调（订单状态变化）
virtual void OnRtnOrder(CThostFtdcOrderField *pOrder) = 0;

// 成交回调（成交发生）
virtual void OnRtnTrade(CThostFtdcTradeField *pTrade) = 0;

// 错误报单回调（报单被拒绝）
virtual void OnErrRtnOrderInsert(CThostFtdcInputOrderField *pInputOrder,
                                   CThostFtdcRspInfoField *pRspInfo) = 0;

// 错误撤单回调（撤单失败）
virtual void OnErrRtnOrderAction(CThostFtdcOrderActionField *pOrderAction,
                                   CThostFtdcRspInfoField *pRspInfo) = 0;

// 资金账户变动回调
virtual void OnRtnTradingAccount(CThostFtdcTradingAccountField *pTradingAccount) = 0;

// 持仓变动回调
virtual void OnRtnInvestorPosition(CThostFtdcInvestorPositionField *pInvestorPosition) = 0;
```

#### CTP 消息特点

| 特性 | 说明 |
|------|------|
| **OnRsp vs OnRtn** | OnRsp响应只能由发起请求的连接接收；OnRtn推送可被同一账户的所有连接接收 |
| **主动推送** | 订单/成交/账户/持仓变化主动推送，无需查询 |
| **私有流 vs 公有流** | 私有流推送账户相关消息，公有流推送行情消息 |
| **流量控制** | 单账户连接数限制（通常1-2个），防止滥用 |

### 2. 上交所 STEP 协议

STEP（Securities Trading Exchange Protocol）是中国证券行业数据通信标准。

#### 消息类型

```
订单回报（Order Report）
  ├─ 订单已接受（Accepted）
  ├─ 订单已拒绝（Rejected）
  ├─ 订单部分成交（PartiallyFilled）
  ├─ 订单全部成交（Filled）
  └─ 订单已撤销（Canceled）

成交回报（Trade Report）
  ├─ 成交ID
  ├─ 订单ID
  ├─ 成交价格/数量
  └─ 成交时间

资金变动通知（Cash Movement）
  ├─ 可用资金变化
  ├─ 冻结资金变化
  └─ 保证金占用变化
```

#### STEP 协议特点

| 特性 | 说明 |
|------|------|
| **双向通信** | 支持请求-响应和主动推送两种模式 |
| **消息编号** | 每条消息有唯一序列号，支持消息补发 |
| **心跳机制** | 定期发送心跳，检测连接状态 |
| **压缩传输** | 支持消息压缩，减少带宽占用 |

### 3. 飞马（Femas）交易系统

飞马是上交所开发的新一代交易系统。

#### 消息推送机制

```
实时推送（Real-time Push）
  ├─ 订单状态变化（立即推送）
  ├─ 成交回报（立即推送）
  └─ 风控警告（立即推送）

批量推送（Batch Push）
  ├─ 账户汇总信息（每秒推送一次）
  ├─ 持仓汇总信息（每秒推送一次）
  └─ 盈亏统计（每秒推送一次）

查询响应（Query Response）
  ├─ 历史订单查询
  ├─ 历史成交查询
  └─ 当日持仓查询
```

---

## 通知消息分类

### 按紧急程度分类

| 级别 | 消息类型 | 推送策略 | 延迟要求 |
|------|---------|----------|----------|
| **P0（最高）** | 订单被拒绝、风控警告 | 立即推送 | < 1ms |
| **P1（高）** | 订单确认、成交回报 | 立即推送 | < 5ms |
| **P2（中）** | 账户更新、持仓更新 | 批量推送（100ms） | < 100ms |
| **P3（低）** | 日终结算、统计信息 | 定时推送（1s） | < 1s |

### 按业务类型分类

```
订单相关（Order）
  ├─ OrderAccepted         订单已接受
  ├─ OrderRejected         订单已拒绝
  ├─ OrderPartiallyFilled  订单部分成交
  ├─ OrderFilled           订单全部成交
  ├─ OrderCanceled         订单已撤销
  └─ OrderExpired          订单已过期

成交相关（Trade）
  ├─ TradeExecuted         成交发生
  └─ TradeCanceled         成交撤销（错单）

账户相关（Account）
  ├─ AccountUpdate         账户余额/保证金变化
  ├─ RiskAlert             风控预警（保证金不足）
  └─ MarginCall            追加保证金通知

持仓相关（Position）
  ├─ PositionUpdate        持仓变化
  ├─ PositionProfit        浮动盈亏更新
  └─ PositionClosed        持仓平仓

系统相关（System）
  ├─ TradingSessionStart   交易时段开始
  ├─ TradingSessionEnd     交易时段结束
  ├─ MarketHalt            熔断/停牌
  └─ SystemMaintenance     系统维护通知
```

---

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                     通知消息系统架构                         │
└─────────────────────────────────────────────────────────────┘

                        Client (WebSocket)
                             ↑
                             │ (5) 推送消息
                             │
┌────────────────────────────┴────────────────────────────────┐
│                    NotificationGateway                       │
│  ┌────────────────────────────────────────────────────────┐│
│  │  SessionManager: 管理所有 WebSocket 连接               ││
│  │  - user_sessions: HashMap<UserId, Vec<SessionId>>     ││
│  │  - session_subscriptions: HashMap<SessionId, Channels>││
│  └────────────────────────────────────────────────────────┘│
└────────────────────────────┬────────────────────────────────┘
                             │ (4) 接收通知
                             │
┌────────────────────────────┴────────────────────────────────┐
│                   NotificationBroker                         │
│  ┌────────────────────────────────────────────────────────┐│
│  │  消息路由和分发                                         ││
│  │  - 按用户ID路由消息                                    ││
│  │  - 消息去重（基于消息ID）                              ││
│  │  - 消息优先级队列（P0/P1/P2/P3）                      ││
│  │  - 消息持久化（Redis/MongoDB）                        ││
│  └────────────────────────────────────────────────────────┘│
└────────────────────────────┬────────────────────────────────┘
                             │ (3) 发送通知
                             │
┌────────────────────────────┴────────────────────────────────┐
│                  NotificationProducer（各业务模块）          │
│                                                              │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────┐ │
│  │ MatchingEngine │  │ AccountSystem  │  │ TradeGateway │ │
│  │ (订单确认/成交) │  │ (账户/持仓)    │  │ (风控警告)   │ │
│  └────────┬───────┘  └────────┬───────┘  └──────┬───────┘ │
│           │                   │                   │          │
│           └───────────────────┴───────────────────┘          │
│                  (2) 生成通知消息                            │
└──────────────────────────────────────────────────────────────┘
                             ↑
                             │ (1) 业务事件
                             │
┌─────────────────────────────────────────────────────────────┐
│                     核心业务系统                             │
│  MatchingEngineCore | AccountSystemCore | RiskControlCore  │
└─────────────────────────────────────────────────────────────┘
```

### 数据流向

```
1. 业务事件发生
   MatchingEngine: 订单进入订单簿 → OrderAccepted
   MatchingEngine: 订单撮合成功 → TradeExecuted
   AccountSystem: 账户余额变化 → AccountUpdate
   RiskControl: 保证金不足 → RiskAlert

2. NotificationProducer 生成通知消息
   创建 Notification 对象
   设置消息ID、时间戳、用户ID、优先级

3. NotificationBroker 路由和分发
   按优先级排队
   按用户ID路由到对应的 Gateway
   持久化到 Redis（支持断线重连）

4. NotificationGateway 推送到客户端
   查找该用户的所有在线会话
   检查订阅频道（是否订阅该类型消息）
   序列化为 JSON/Protobuf
   通过 WebSocket 推送

5. 客户端接收消息
   解析消息
   更新UI（订单列表、账户余额、持仓等）
```

---

## 消息类型定义

### 通知消息基础结构

```rust
/// 通知消息（内部传递）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// 消息ID（全局唯一，用于去重）
    pub message_id: String,

    /// 消息类型
    pub message_type: NotificationType,

    /// 用户ID
    pub user_id: String,

    /// 优先级（0=最高，3=最低）
    pub priority: u8,

    /// 消息内容（JSON）
    pub payload: serde_json::Value,

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 来源（MatchingEngine/AccountSystem/RiskControl）
    pub source: String,
}

/// 通知消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NotificationType {
    // 订单相关
    OrderAccepted,
    OrderRejected,
    OrderPartiallyFilled,
    OrderFilled,
    OrderCanceled,

    // 成交相关
    TradeExecuted,

    // 账户相关
    AccountUpdate,
    RiskAlert,
    MarginCall,

    // 持仓相关
    PositionUpdate,
    PositionProfit,

    // 系统相关
    SystemNotice,
}
```

### 具体消息结构

#### 1. 订单确认通知

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAcceptedNotify {
    /// 订单ID（账户生成的UUID）
    pub order_id: String,

    /// 交易所订单ID（全局唯一）
    pub exchange_order_id: String,

    /// 合约代码
    pub instrument_id: String,

    /// 方向：BUY/SELL
    pub direction: String,

    /// 开平：OPEN/CLOSE
    pub offset: String,

    /// 价格
    pub price: f64,

    /// 数量
    pub volume: f64,

    /// 订单类型：LIMIT/MARKET
    pub order_type: String,

    /// 时间戳
    pub timestamp: i64,

    /// 预计冻结保证金
    pub frozen_margin: f64,
}
```

#### 2. 成交回报通知

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecutedNotify {
    /// 成交ID（交易所生成）
    pub trade_id: String,

    /// 订单ID
    pub order_id: String,

    /// 交易所订单ID
    pub exchange_order_id: String,

    /// 合约代码
    pub instrument_id: String,

    /// 方向：BUY/SELL
    pub direction: String,

    /// 开平：OPEN/CLOSE
    pub offset: String,

    /// 成交价格
    pub price: f64,

    /// 成交数量
    pub volume: f64,

    /// 手续费
    pub commission: f64,

    /// 成交类型：FULL（完全成交）/ PARTIAL（部分成交）
    pub fill_type: String,

    /// 时间戳
    pub timestamp: i64,
}
```

#### 3. 账户更新通知

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdateNotify {
    /// 用户ID
    pub user_id: String,

    /// 账户余额
    pub balance: f64,

    /// 可用资金
    pub available: f64,

    /// 冻结资金
    pub frozen: f64,

    /// 占用保证金
    pub margin: f64,

    /// 持仓盈亏（浮动盈亏）
    pub position_profit: f64,

    /// 平仓盈亏（已实现盈亏）
    pub close_profit: f64,

    /// 风险度（保证金占用率）
    pub risk_ratio: f64,

    /// 时间戳
    pub timestamp: i64,
}
```

#### 4. 持仓更新通知

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionUpdateNotify {
    /// 用户ID
    pub user_id: String,

    /// 合约代码
    pub instrument_id: String,

    /// 多头持仓
    pub volume_long: f64,

    /// 空头持仓
    pub volume_short: f64,

    /// 多头开仓均价
    pub cost_long: f64,

    /// 空头开仓均价
    pub cost_short: f64,

    /// 多头浮动盈亏
    pub profit_long: f64,

    /// 空头浮动盈亏
    pub profit_short: f64,

    /// 时间戳
    pub timestamp: i64,
}
```

#### 5. 风控预警通知

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlertNotify {
    /// 用户ID
    pub user_id: String,

    /// 警告类型：MARGIN_INSUFFICIENT（保证金不足）/ POSITION_LIMIT（持仓超限）
    pub alert_type: String,

    /// 警告级别：WARNING（警告）/ CRITICAL（严重）/ EMERGENCY（紧急）
    pub severity: String,

    /// 警告消息
    pub message: String,

    /// 当前风险度
    pub risk_ratio: f64,

    /// 建议操作
    pub suggestion: String,

    /// 时间戳
    pub timestamp: i64,
}
```

---

## 通知路由机制

### 路由策略

```rust
/// NotificationBroker 负责消息路由
pub struct NotificationBroker {
    /// 用户订阅表：user_id -> Vec<gateway_id>
    user_subscriptions: DashMap<String, Vec<String>>,

    /// 消息持久化（Redis）
    redis: Arc<redis::Client>,

    /// 发送到各个 Gateway 的通道
    gateway_senders: DashMap<String, Sender<Notification>>,

    /// 消息去重缓存（最近1小时的消息ID）
    dedup_cache: Arc<Mutex<HashSet<String>>>,

    /// 优先级队列
    priority_queues: [ArrayQueue<Notification>; 4], // P0/P1/P2/P3
}

impl NotificationBroker {
    /// 发布通知
    pub fn publish(&self, notification: Notification) {
        // 1. 消息去重
        if !self.is_duplicate(&notification.message_id) {
            // 2. 持久化到 Redis（支持断线重连）
            self.persist_to_redis(&notification);

            // 3. 按优先级入队
            let queue = &self.priority_queues[notification.priority as usize];
            let _ = queue.push(notification.clone());

            // 4. 路由到对应的 Gateway
            if let Some(gateways) = self.user_subscriptions.get(&notification.user_id) {
                for gateway_id in gateways.iter() {
                    if let Some(sender) = self.gateway_senders.get(gateway_id) {
                        let _ = sender.send(notification.clone());
                    }
                }
            }
        }
    }

    /// 消息去重
    fn is_duplicate(&self, message_id: &str) -> bool {
        let mut cache = self.dedup_cache.lock().unwrap();
        if cache.contains(message_id) {
            return true;
        }
        cache.insert(message_id.to_string());
        false
    }

    /// 持久化到 Redis
    fn persist_to_redis(&self, notification: &Notification) {
        // 使用 Redis List 存储用户的消息队列
        let key = format!("notifications:{}", notification.user_id);
        let value = serde_json::to_string(notification).unwrap();

        // LPUSH + LTRIM 保留最近1000条消息
        let mut conn = self.redis.get_connection().unwrap();
        let _: () = redis::pipe()
            .lpush(&key, value)
            .ltrim(&key, 0, 999)
            .query(&mut conn)
            .unwrap();
    }
}
```

### 断线重连补发

```rust
impl NotificationGateway {
    /// 客户端重连后，补发未接收的消息
    pub async fn resend_missed_messages(&self, user_id: &str, last_message_id: &str) {
        // 1. 从 Redis 获取该用户的消息队列
        let key = format!("notifications:{}", user_id);
        let mut conn = self.redis.get_connection().unwrap();
        let messages: Vec<String> = conn.lrange(&key, 0, -1).unwrap();

        // 2. 找到 last_message_id 的位置
        let mut start_index = 0;
        for (i, msg_str) in messages.iter().enumerate() {
            let msg: Notification = serde_json::from_str(msg_str).unwrap();
            if msg.message_id == last_message_id {
                start_index = i + 1;
                break;
            }
        }

        // 3. 补发未接收的消息
        for msg_str in messages[start_index..].iter().rev() {
            let notification: Notification = serde_json::from_str(msg_str).unwrap();
            self.send_to_client(user_id, notification).await;
        }
    }
}
```

---

## 性能优化

### 1. 批量推送（减少网络往返）

```rust
/// 批量推送器（每100ms推送一次，或累积到100条消息）
pub struct BatchPusher {
    batch_queue: Arc<Mutex<Vec<Notification>>>,
    batch_size: usize,
    batch_interval: Duration,
}

impl BatchPusher {
    pub fn start(&self) {
        let queue = self.batch_queue.clone();
        let size = self.batch_size;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));

            loop {
                interval.tick().await;

                let mut batch = queue.lock().unwrap();
                if !batch.is_empty() {
                    // 发送批量消息
                    self.send_batch(&batch).await;
                    batch.clear();
                }
            }
        });
    }

    async fn send_batch(&self, notifications: &[Notification]) {
        // 按用户分组
        let mut grouped: HashMap<String, Vec<Notification>> = HashMap::new();
        for notif in notifications {
            grouped.entry(notif.user_id.clone())
                   .or_insert(Vec::new())
                   .push(notif.clone());
        }

        // 并行推送
        for (user_id, user_notifs) in grouped {
            self.send_to_user(&user_id, user_notifs).await;
        }
    }
}
```

### 2. 消息压缩（减少带宽）

```rust
/// WebSocket 消息压缩
pub fn compress_message(message: &str) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(message.as_bytes()).unwrap();
    encoder.finish().unwrap()
}
```

### 3. 消息优先级队列

```rust
/// 优先级消息处理器（高优先级先处理）
pub struct PriorityProcessor {
    queues: [ArrayQueue<Notification>; 4], // P0/P1/P2/P3
}

impl PriorityProcessor {
    pub async fn process(&self) {
        loop {
            // 优先处理 P0 消息
            if let Some(notif) = self.queues[0].pop() {
                self.handle_notification(notif).await;
                continue;
            }

            // 再处理 P1 消息
            if let Some(notif) = self.queues[1].pop() {
                self.handle_notification(notif).await;
                continue;
            }

            // P2 和 P3 按比例处理（避免饥饿）
            for _ in 0..5 {
                if let Some(notif) = self.queues[2].pop() {
                    self.handle_notification(notif).await;
                }
            }

            if let Some(notif) = self.queues[3].pop() {
                self.handle_notification(notif).await;
            }

            tokio::time::sleep(Duration::from_micros(100)).await;
        }
    }
}
```

---

## 实现方案

### 阶段1：核心通知模块

1. **创建通知消息结构**
   - 定义 `Notification` 基础结构
   - 实现各类通知消息（订单、成交、账户、持仓）
   - 添加消息序列化/反序列化

2. **实现 NotificationBroker**
   - 消息路由和分发
   - 消息去重机制
   - Redis 持久化

3. **集成到现有系统**
   - MatchingEngineCore 发送 OrderAccepted/TradeExecuted
   - AccountSystemCore 发送 AccountUpdate/PositionUpdate
   - RiskControl 发送 RiskAlert

### 阶段2：WebSocket 推送

1. **扩展 WebSocket 消息协议**
   - 添加订阅/取消订阅消息类型
   - 添加消息确认机制（ACK）
   - 添加断线重连补发

2. **实现 NotificationGateway**
   - SessionManager（管理所有 WebSocket 连接）
   - 消息路由到对应的会话
   - 心跳检测和自动重连

### 阶段3：性能优化

1. **批量推送**
   - 实现 BatchPusher
   - 配置批量大小和间隔

2. **消息压缩**
   - WebSocket 消息压缩（Gzip）
   - Protobuf 二进制序列化（替代 JSON）

3. **优先级队列**
   - 实现 PriorityProcessor
   - 按优先级处理消息

### 阶段4：监控和运维

1. **监控指标**
   - 消息推送延迟（P50/P99）
   - 消息推送成功率
   - WebSocket 连接数
   - 消息队列长度

2. **告警规则**
   - 消息延迟 > 100ms
   - 消息队列积压 > 10000
   - WebSocket 连接失败率 > 5%

---

## 代码示例

### 1. AccountSystemCore 发送通知

```rust
// src/account/core/mod.rs

impl AccountSystemCore {
    fn apply_trade(&self, acc: &mut QA_Account, trade: &TradeReport) {
        // ... 更新账户逻辑 ...

        // 发送账户更新通知
        if let Some(ref sender) = self.update_sender {
            let notify = AccountUpdateNotify {
                user_id: user_id.to_string(),
                balance: acc.balance,
                available: acc.available,
                frozen: acc.frozen,
                margin: acc.margin,
                position_profit: acc.get_position_profit(),
                close_profit: acc.close_profit,
                risk_ratio: acc.margin / acc.balance,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            };

            // 发送到 NotificationBroker
            let _ = sender.send(Notification {
                message_id: uuid::Uuid::new_v4().to_string(),
                message_type: NotificationType::AccountUpdate,
                user_id: user_id.to_string(),
                priority: 2, // P2（中优先级）
                payload: serde_json::to_value(&notify).unwrap(),
                timestamp: notify.timestamp,
                source: "AccountSystem".to_string(),
            });
        }
    }
}
```

### 2. NotificationBroker 路由消息

```rust
// src/notification/broker.rs

pub struct NotificationBroker {
    user_gateways: DashMap<String, Vec<String>>,
    gateway_senders: DashMap<String, Sender<Notification>>,
    redis: Arc<redis::Client>,
}

impl NotificationBroker {
    pub fn publish(&self, notification: Notification) {
        // 1. 持久化到 Redis
        self.persist(&notification);

        // 2. 路由到所有订阅的 Gateway
        if let Some(gateways) = self.user_gateways.get(&notification.user_id) {
            for gateway_id in gateways.iter() {
                if let Some(sender) = self.gateway_senders.get(gateway_id) {
                    let _ = sender.send(notification.clone());
                }
            }
        }
    }

    fn persist(&self, notification: &Notification) {
        let key = format!("notifications:{}", notification.user_id);
        let value = serde_json::to_string(notification).unwrap();

        let mut conn = self.redis.get_connection().unwrap();
        let _: () = redis::pipe()
            .lpush(&key, value)
            .ltrim(&key, 0, 999)
            .expire(&key, 86400) // 保留24小时
            .query(&mut conn)
            .unwrap();
    }
}
```

### 3. WebSocket 推送通知

```rust
// src/service/websocket/session.rs

pub struct WebSocketSession {
    user_id: String,
    session_id: String,
    addr: Addr<WebSocketSession>,
    notification_receiver: Receiver<Notification>,
}

impl WebSocketSession {
    pub fn start_notification_listener(&mut self, ctx: &mut Context<Self>) {
        let receiver = self.notification_receiver.clone();

        ctx.add_stream(
            receiver.into_stream().map(|notification| {
                // 转换为 ServerMessage
                match notification.message_type {
                    NotificationType::AccountUpdate => {
                        let payload: AccountUpdateNotify =
                            serde_json::from_value(notification.payload).unwrap();

                        ServerMessage::AccountUpdate {
                            balance: payload.balance,
                            available: payload.available,
                            frozen: payload.frozen,
                            margin: payload.margin,
                            profit: payload.position_profit + payload.close_profit,
                            risk_ratio: payload.risk_ratio,
                            timestamp: payload.timestamp,
                        }
                    },
                    NotificationType::TradeExecuted => {
                        let payload: TradeExecutedNotify =
                            serde_json::from_value(notification.payload).unwrap();

                        ServerMessage::Trade {
                            trade_id: payload.trade_id,
                            order_id: payload.order_id,
                            instrument_id: payload.instrument_id,
                            direction: payload.direction,
                            offset: payload.offset,
                            price: payload.price,
                            volume: payload.volume,
                            timestamp: payload.timestamp,
                        }
                    },
                    // ... 其他消息类型 ...
                }
            })
        );
    }
}
```

---

## 总结

本文档设计了一个完整的交易所通知消息系统，包括：

1. ✅ **业界标准调研**：参考 CTP、上交所 STEP、飞马等系统
2. ✅ **消息分类**：按紧急程度和业务类型分类
3. ✅ **架构设计**：NotificationBroker + NotificationGateway
4. ✅ **消息定义**：订单、成交、账户、持仓、风控等通知
5. ✅ **路由机制**：用户隔离、消息去重、断线重连
6. ✅ **性能优化**：批量推送、消息压缩、优先级队列
7. ✅ **实现方案**：分4个阶段实施

### 下一步行动

- [ ] 创建 `src/notification` 模块
- [ ] 实现 NotificationBroker 核心逻辑
- [ ] 集成到 AccountSystemCore 和 MatchingEngineCore
- [ ] 扩展 WebSocket 消息协议
- [ ] 添加 Redis 持久化
- [ ] 性能测试和优化
