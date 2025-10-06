//! 通知消息系统
//!
//! 提供完整的通知消息功能：
//! - 消息定义和序列化
//! - 消息路由和分发（Broker）
//! - 消息推送网关（Gateway）
//!
//! # 架构
//!
//! ```text
//! Business Module (MatchingEngine/AccountSystem)
//!         ↓
//!    Notification
//!         ↓
//! NotificationBroker (路由、去重、优先级队列)
//!         ↓
//! NotificationGateway (推送、批量、心跳)
//!         ↓
//!    WebSocket Client
//! ```
//!
//! # 示例
//!
//! ```rust,no_run
//! use qaexchange::notification::{
//!     NotificationBroker, NotificationGateway,
//!     Notification, NotificationType, NotificationPayload, AccountUpdateNotify
//! };
//! use std::sync::Arc;
//! use tokio::sync::mpsc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 1. 创建Broker
//!     let broker = Arc::new(NotificationBroker::new());
//!
//!     // 2. 创建Gateway
//!     let (tx, rx) = mpsc::unbounded_channel();
//!     let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));
//!
//!     // 3. 注册Gateway到Broker
//!     broker.register_gateway("gateway_01", tx);
//!
//!     // 4. 订阅用户消息
//!     broker.subscribe("user_01", "gateway_01");
//!
//!     // 5. 注册WebSocket会话
//!     let (session_tx, mut session_rx) = mpsc::unbounded_channel();
//!     gateway.register_session("session_01", "user_01", session_tx);
//!
//!     // 6. 启动推送任务
//!     let _pusher = gateway.clone().start_notification_pusher();
//!     let _processor = broker.clone().start_priority_processor();
//!
//!     // 7. 发布通知
//!     let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
//!         user_id: "user_01".to_string(),
//!         balance: 1000000.0,
//!         available: 980000.0,
//!         frozen: 0.0,
//!         margin: 20000.0,
//!         position_profit: 500.0,
//!         close_profit: 1000.0,
//!         risk_ratio: 0.02,
//!         timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
//!     });
//!
//!     let notification = Notification::new(
//!         NotificationType::AccountUpdate,
//!         Arc::from("user_01"),
//!         payload,
//!         "AccountSystem",
//!     );
//!
//!     broker.publish(notification).unwrap();
//!
//!     // 8. 接收WebSocket消息
//!     if let Some(json) = session_rx.recv().await {
//!         println!("Received: {}", json);
//!     }
//! }
//! ```

pub mod message;
pub mod broker;
pub mod gateway;

// 导出核心类型
pub use message::{
    Notification,
    NotificationType,
    NotificationPayload,
    // 订单相关
    OrderAcceptedNotify,
    OrderRejectedNotify,
    OrderPartiallyFilledNotify,
    OrderFilledNotify,
    OrderCanceledNotify,
    // 成交相关
    TradeExecutedNotify,
    // 账户相关
    AccountUpdateNotify,
    // 持仓相关
    PositionUpdateNotify,
    // 风控相关
    RiskAlertNotify,
    MarginCallNotify,
    // 系统相关
    SystemNoticeNotify,
};

pub use broker::{NotificationBroker, BrokerStatsSnapshot};
pub use gateway::{NotificationGateway, SessionInfo, GatewayStatsSnapshot};
