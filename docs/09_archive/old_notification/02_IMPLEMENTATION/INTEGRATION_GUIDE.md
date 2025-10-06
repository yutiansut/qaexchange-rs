# 通知系统集成指南

> 如何将通知系统集成到业务模块中

**版本**: v1.1.0
**最后更新**: 2025-10-03

---

## 📚 目录

- [集成概述](#集成概述)
- [AccountSystem 集成](#accountsystem-集成)
- [MatchingEngine 集成](#matchingengine-集成)
- [RiskControl 集成](#riskcontrol-集成)
- [WebSocket 服务集成](#websocket-服务集成)
- [完整示例](#完整示例)

---

## 集成概述

### 架构图

```
┌─────────────────┐
│ AccountSystem   │──┐
└─────────────────┘  │
                     │
┌─────────────────┐  │    ┌──────────────────┐
│ MatchingEngine  │──┼───→│ Notification     │
└─────────────────┘  │    │ Broker           │
                     │    └────────┬─────────┘
┌─────────────────┐  │             │
│ RiskControl     │──┘             ↓
└─────────────────┘         ┌──────────────────┐
                            │ Notification     │
                            │ Gateway          │
                            └────────┬─────────┘
                                     ↓
                            ┌──────────────────┐
                            │ WebSocket        │
                            │ Service          │
                            └──────────────────┘
```

### 集成步骤

1. **添加 Broker 依赖**：在业务模块中持有 `Arc<NotificationBroker>`
2. **发送通知**：在关键业务逻辑点调用 `broker.publish()`
3. **选择消息类型**：根据业务事件选择合适的 `NotificationType`
4. **构造负载**：填充相应的 `NotificationPayload`

---

## AccountSystem 集成

### 1. 修改结构体

```rust
// src/account/core/mod.rs

use crate::notification::{
    NotificationBroker, Notification, NotificationType,
    NotificationPayload, AccountUpdateNotify,
};
use std::sync::Arc;

pub struct AccountSystemCore {
    // 现有字段...
    accounts: Arc<DashMap<String, Arc<RwLock<QA_Account>>>>,

    // ✅ 添加 NotificationBroker
    notification_broker: Arc<NotificationBroker>,
}

impl AccountSystemCore {
    pub fn new(notification_broker: Arc<NotificationBroker>) -> Self {
        Self {
            accounts: Arc::new(DashMap::new()),
            notification_broker,
        }
    }
}
```

### 2. 账户更新通知

**场景**: 成交回报导致账户变更

```rust
// src/account/core/mod.rs

impl AccountSystemCore {
    fn apply_trade(&self, acc: &mut QA_Account, trade: &TradeReport) {
        // 1. 应用成交（现有逻辑）
        acc.receive_simpledeal(trade);

        // 2. 发送账户更新通知
        let notification = Notification::new(
            NotificationType::AccountUpdate,
            Arc::from(acc.user_id.clone()),
            NotificationPayload::AccountUpdate(AccountUpdateNotify {
                user_id: acc.user_id.clone(),
                balance: acc.balance,
                available: acc.available,
                frozen: acc.frozen,
                margin: acc.margin,
                position_profit: acc.get_position_profit(),
                close_profit: acc.close_profit,
                risk_ratio: if acc.balance > 0.0 {
                    acc.margin / acc.balance
                } else {
                    0.0
                },
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            }),
            "AccountSystem",
        );

        // 3. 发布通知（不阻塞主流程）
        if let Err(e) = self.notification_broker.publish(notification) {
            log::warn!("Failed to publish account update: {}", e);
        }
    }
}
```

### 3. 持仓更新通知

**场景**: 持仓变化

```rust
impl AccountSystemCore {
    fn update_position(&self, acc: &mut QA_Account, instrument_id: &str) {
        // 现有逻辑...

        // 获取持仓信息
        if let Some(pos) = acc.positions.get(instrument_id) {
            let notification = Notification::new(
                NotificationType::PositionUpdate,
                Arc::from(acc.user_id.clone()),
                NotificationPayload::PositionUpdate(PositionUpdateNotify {
                    user_id: acc.user_id.clone(),
                    instrument_id: instrument_id.to_string(),
                    volume_long: pos.volume_long,
                    volume_short: pos.volume_short,
                    cost_long: pos.cost_long,
                    cost_short: pos.cost_short,
                    profit_long: pos.profit_long,
                    profit_short: pos.profit_short,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }),
                "AccountSystem",
            );

            self.notification_broker.publish(notification).ok();
        }
    }
}
```

---

## MatchingEngine 集成

### 1. 修改结构体

```rust
// src/matching/core/mod.rs

use crate::notification::{
    NotificationBroker, Notification, NotificationType,
    NotificationPayload, OrderAcceptedNotify, TradeExecutedNotify,
    OrderRejectedNotify, OrderFilledNotify,
};

pub struct MatchingEngineCore {
    // 现有字段...
    orderbooks: DashMap<String, Arc<RwLock<Orderbook>>>,

    // ✅ 添加 NotificationBroker
    notification_broker: Arc<NotificationBroker>,
}
```

### 2. 订单接受通知

**场景**: 订单被撮合引擎接受

```rust
impl MatchingEngineCore {
    fn handle_order_accepted(
        &self,
        order_id: &str,
        exchange_order_id: &str,
        req: &OrderRequest
    ) {
        let notification = Notification::new(
            NotificationType::OrderAccepted,
            Arc::from(String::from_utf8_lossy(&req.user_id).to_string()),
            NotificationPayload::OrderAccepted(OrderAcceptedNotify {
                order_id: order_id.to_string(),
                exchange_order_id: exchange_order_id.to_string(),
                instrument_id: String::from_utf8_lossy(&req.instrument_id).to_string(),
                direction: if req.direction == 0 { "BUY".to_string() } else { "SELL".to_string() },
                offset: if req.offset == 0 { "OPEN".to_string() } else { "CLOSE".to_string() },
                price: req.price,
                volume: req.volume,
                order_type: "LIMIT".to_string(),
                frozen_margin: self.calculate_margin(req),
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            }),
            "MatchingEngine",
        );

        self.notification_broker.publish(notification).ok();
    }
}
```

### 3. 成交回报通知

**场景**: 订单成交

```rust
impl MatchingEngineCore {
    fn handle_trade(
        &self,
        trade: &TradeReport
    ) {
        let user_id = String::from_utf8_lossy(&trade.user_id).to_string();
        let notification = Notification::new(
            NotificationType::TradeExecuted,
            Arc::from(user_id.clone()),
            NotificationPayload::TradeExecuted(TradeExecutedNotify {
                trade_id: String::from_utf8_lossy(&trade.trade_id).to_string(),
                order_id: String::from_utf8_lossy(&trade.order_id).to_string(),
                exchange_order_id: String::from_utf8_lossy(&trade.exchange_order_id).to_string(),
                instrument_id: String::from_utf8_lossy(&trade.instrument_id).to_string(),
                direction: if trade.direction == 0 { "BUY".to_string() } else { "SELL".to_string() },
                offset: if trade.offset == 0 { "OPEN".to_string() } else { "CLOSE".to_string() },
                price: trade.price,
                volume: trade.volume,
                commission: trade.commission,
                fill_type: if trade.fill_type == 0 { "FULL".to_string() } else { "PARTIAL".to_string() },
                timestamp: trade.timestamp,
            }),
            "MatchingEngine",
        );

        self.notification_broker.publish(notification).ok();
    }
}
```

### 4. 订单拒绝通知

**场景**: 订单被拒绝

```rust
impl MatchingEngineCore {
    fn handle_order_rejected(
        &self,
        order_id: &str,
        req: &OrderRequest,
        reason: &str,
        error_code: u32
    ) {
        let notification = Notification::with_priority(
            NotificationType::OrderRejected,
            Arc::from(String::from_utf8_lossy(&req.user_id).to_string()),
            NotificationPayload::OrderRejected(OrderRejectedNotify {
                order_id: order_id.to_string(),
                instrument_id: String::from_utf8_lossy(&req.instrument_id).to_string(),
                reason: reason.to_string(),
                error_code,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            }),
            0,  // P0 最高优先级
            "MatchingEngine",
        );

        self.notification_broker.publish(notification).ok();
    }
}
```

---

## RiskControl 集成

### 1. 修改结构体

```rust
// src/risk/control.rs

use crate::notification::{
    NotificationBroker, Notification, NotificationType,
    NotificationPayload, RiskAlertNotify, MarginCallNotify,
};

pub struct RiskControl {
    notification_broker: Arc<NotificationBroker>,
}
```

### 2. 风控预警通知

**场景**: 风险度超过阈值

```rust
impl RiskControl {
    pub fn check_risk(&self, account: &QA_Account) {
        let risk_ratio = if account.balance > 0.0 {
            account.margin / account.balance
        } else {
            0.0
        };

        // 风险度阈值检查
        if risk_ratio > 0.8 {
            let (alert_type, severity, suggestion) = if risk_ratio > 0.95 {
                (
                    "MARGIN_INSUFFICIENT",
                    "EMERGENCY",
                    "立即追加保证金或平仓，否则将被强制平仓"
                )
            } else if risk_ratio > 0.9 {
                (
                    "MARGIN_INSUFFICIENT",
                    "CRITICAL",
                    "建议尽快追加保证金或减少持仓"
                )
            } else {
                (
                    "MARGIN_INSUFFICIENT",
                    "WARNING",
                    "建议关注账户风险，适当控制仓位"
                )
            };

            let notification = Notification::with_priority(
                NotificationType::RiskAlert,
                Arc::from(account.user_id.clone()),
                NotificationPayload::RiskAlert(RiskAlertNotify {
                    user_id: account.user_id.clone(),
                    alert_type: alert_type.to_string(),
                    severity: severity.to_string(),
                    message: format!("账户风险度 {:.2}% 超过预警线", risk_ratio * 100.0),
                    risk_ratio,
                    suggestion: suggestion.to_string(),
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }),
                0,  // P0 最高优先级
                "RiskControl",
            );

            self.notification_broker.publish(notification).ok();
        }
    }
}
```

### 3. 追加保证金通知

**场景**: 需要追加保证金

```rust
impl RiskControl {
    pub fn margin_call(&self, account: &QA_Account, required_margin: f64) {
        let notification = Notification::with_priority(
            NotificationType::MarginCall,
            Arc::from(account.user_id.clone()),
            NotificationPayload::MarginCall(MarginCallNotify {
                user_id: account.user_id.clone(),
                current_margin: account.margin,
                required_margin,
                deadline: chrono::Utc::now().timestamp() + 3600,  // 1小时后
                message: format!(
                    "请在1小时内追加保证金 {:.2} 元，当前保证金 {:.2} 元",
                    required_margin, account.margin
                ),
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            }),
            0,  // P0 最高优先级
            "RiskControl",
        );

        self.notification_broker.publish(notification).ok();
    }
}
```

---

## WebSocket 服务集成

### 1. 创建系统单例

```rust
// src/service/websocket/mod.rs

use crate::notification::{NotificationBroker, NotificationGateway};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct NotificationSystem {
    pub broker: Arc<NotificationBroker>,
    pub gateway: Arc<NotificationGateway>,
}

impl NotificationSystem {
    pub fn new() -> Self {
        let broker = Arc::new(NotificationBroker::new());
        let (tx, rx) = mpsc::unbounded_channel();
        let gateway = Arc::new(NotificationGateway::new("main_gateway", rx));

        // 注册 Gateway
        broker.register_gateway("main_gateway", tx);

        Self { broker, gateway }
    }

    pub fn start(&self) {
        // 启动优先级处理器
        let _processor = self.broker.clone().start_priority_processor();

        // 启动推送任务
        let _pusher = self.gateway.clone().start_notification_pusher();

        // 启动心跳检测
        let _heartbeat = self.gateway.clone().start_heartbeat_checker();

        log::info!("Notification system started");
    }
}
```

### 2. WebSocket 会话处理

```rust
// src/service/websocket/session.rs

use actix::{Actor, StreamHandler, AsyncContext};
use actix_web_actors::ws;

pub struct WebSocketSession {
    session_id: String,
    user_id: Option<String>,
    notification_gateway: Arc<NotificationGateway>,
    broker: Arc<NotificationBroker>,
    notification_rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl Actor for WebSocketSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("WebSocket session started: {}", self.session_id);
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        // 注销会话
        self.notification_gateway.unregister_session(&self.session_id);

        if let Some(user_id) = &self.user_id {
            self.broker.unsubscribe(user_id, "main_gateway");
        }

        log::info!("WebSocket session stopped: {}", self.session_id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                // 处理客户端消息
                if let Some(user_id) = &self.user_id {
                    // 已认证，转发到业务逻辑
                } else {
                    // 未认证，处理认证
                    self.handle_auth(text.to_string(), ctx);
                }
            }
            _ => {}
        }
    }
}

impl WebSocketSession {
    fn handle_auth(&mut self, msg: String, ctx: &mut ws::WebsocketContext<Self>) {
        // 解析认证消息
        if let Ok(auth) = serde_json::from_str::<AuthMessage>(&msg) {
            self.user_id = Some(auth.user_id.clone());

            // 创建通知接收通道
            let (tx, mut rx) = mpsc::unbounded_channel();

            // 注册会话
            self.notification_gateway.register_session(
                self.session_id.clone(),
                auth.user_id.clone(),
                tx,
            );

            // 订阅用户消息
            self.broker.subscribe(&auth.user_id, "main_gateway");

            // 启动通知转发任务
            let addr = ctx.address();
            ctx.spawn(async move {
                while let Some(json) = rx.recv().await {
                    addr.do_send(NotificationMessage(json));
                }
            }.into_actor(self));

            log::info!("User {} authenticated", auth.user_id);
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct NotificationMessage(String);

impl Handler<NotificationMessage> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: NotificationMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}
```

### 3. 服务器启动

```rust
// src/main.rs

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 创建通知系统
    let notification_system = Arc::new(NotificationSystem::new());
    notification_system.start();

    // 创建业务系统（注入 notification_broker）
    let account_system = Arc::new(AccountSystemCore::new(
        notification_system.broker.clone()
    ));

    let matching_engine = Arc::new(MatchingEngineCore::new(
        notification_system.broker.clone()
    ));

    // 启动 HTTP 服务
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(notification_system.clone()))
            .app_data(web::Data::new(account_system.clone()))
            .app_data(web::Data::new(matching_engine.clone()))
            .route("/ws", web::get().to(websocket_handler))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    notification_system: web::Data<Arc<NotificationSystem>>,
) -> Result<HttpResponse, Error> {
    let session = WebSocketSession {
        session_id: uuid::Uuid::new_v4().to_string(),
        user_id: None,
        notification_gateway: notification_system.gateway.clone(),
        broker: notification_system.broker.clone(),
        notification_rx: None,
    };

    ws::start(session, &req, stream)
}
```

---

## 完整示例

### 端到端流程

```rust
// 完整的端到端示例

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建通知系统
    let broker = Arc::new(NotificationBroker::new());
    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", gateway_rx));

    broker.register_gateway("gateway_01", gateway_tx);

    // 2. 创建业务系统
    let account_system = Arc::new(AccountSystemCore::new(broker.clone()));
    let matching_engine = Arc::new(MatchingEngineCore::new(broker.clone()));
    let risk_control = Arc::new(RiskControl::new(broker.clone()));

    // 3. 启动通知任务
    let _processor = broker.clone().start_priority_processor();
    let _pusher = gateway.clone().start_notification_pusher();
    let _heartbeat = gateway.clone().start_heartbeat_checker();

    // 4. 模拟 WebSocket 连接
    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    broker.subscribe("user_01", "gateway_01");
    gateway.register_session("session_01", "user_01", session_tx);

    // 5. 业务流程
    tokio::spawn(async move {
        // 接收 WebSocket 消息
        while let Some(json) = session_rx.recv().await {
            println!("WebSocket received: {}", json);
        }
    });

    // 6. 模拟业务事件

    // 账户更新
    account_system.update_account_balance("user_01", 1000000.0);

    // 订单接受
    matching_engine.accept_order("order_123", "user_01");

    // 成交回报
    matching_engine.execute_trade("trade_456", "order_123", "user_01");

    // 风控预警
    risk_control.check_risk_for_user("user_01");

    // 等待消息推送
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 7. 查看统计
    let stats = broker.get_stats();
    println!("Broker stats: {:?}", stats);

    let gw_stats = gateway.get_stats();
    println!("Gateway stats: {:?}", gw_stats);

    Ok(())
}
```

---

## 注意事项

### 1. 错误处理

**不要因为通知失败而阻塞业务逻辑**：

```rust
// ✅ 好
if let Err(e) = self.notification_broker.publish(notification) {
    log::warn!("Failed to publish notification: {}", e);
    // 继续业务流程
}

// ❌ 差
self.notification_broker.publish(notification)?;  // 可能阻塞业务
```

### 2. 性能优化

**批量发送时重用 payload**：

```rust
// ✅ 好
let payload = NotificationPayload::AccountUpdate(notify);
for user_id in user_ids {
    let notification = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from(user_id.clone()),
        payload.clone(),
        "AccountSystem",
    );
    broker.publish(notification).ok();
}
```

### 3. 线程安全

**所有组件都是线程安全的**：

```rust
// ✅ 可以在多线程中使用
let broker_clone = broker.clone();
std::thread::spawn(move || {
    broker_clone.publish(notification).ok();
});
```

---

## 相关链接

- [API 参考](API_REFERENCE.md)
- [系统设计](../01_DESIGN/SYSTEM_DESIGN.md)
- [测试文档](../03_TESTING/TESTING.md)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
