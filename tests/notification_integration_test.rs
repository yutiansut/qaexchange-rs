//! 通知系统集成测试
//!
//! 测试通知系统的端到端功能

use qaexchange::notification::{
    AccountUpdateNotify, Notification, NotificationBroker, NotificationGateway,
    NotificationPayload, NotificationType, OrderAcceptedNotify, TradeExecutedNotify,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// 测试端到端通知流程
#[tokio::test]
async fn test_end_to_end_notification_flow() {
    // 1. 创建系统组件
    let broker = Arc::new(NotificationBroker::new());
    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", gateway_rx));

    // 2. 连接组件
    broker.register_gateway("gateway_01", gateway_tx);
    broker.subscribe("user_01", "gateway_01");

    // 3. 注册WebSocket会话
    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    // 4. 启动推送任务
    let _pusher = gateway.clone().start_notification_pusher();
    let _processor = broker.clone().start_priority_processor();

    // 等待任务启动
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5. 发送账户更新通知
    let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        payload,
        "AccountSystem",
    );

    broker.publish(notification).unwrap();

    // 6. 验证接收
    let received = tokio::time::timeout(Duration::from_secs(1), session_rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("No message received");

    assert!(received.contains("account_update"));
    assert!(received.contains("user_01"));
    assert!(received.contains("1000000"));
}

/// 测试多用户消息隔离
#[tokio::test]
async fn test_multi_user_notification_isolation() {
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    broker.register_gateway("gateway_01", tx);
    broker.subscribe("user_01", "gateway_01");
    broker.subscribe("user_02", "gateway_01");

    let (s1_tx, mut s1_rx) = mpsc::unbounded_channel();
    let (s2_tx, mut s2_rx) = mpsc::unbounded_channel();

    gateway.register_session("session_01", "user_01", s1_tx);
    gateway.register_session("session_02", "user_02", s2_tx);

    let _pusher = gateway.clone().start_notification_pusher();
    let _processor = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发送给 user_01 的消息
    let payload1 = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    broker
        .publish(Notification::new(
            NotificationType::AccountUpdate,
            Arc::from("user_01"),
            payload1,
            "Test",
        ))
        .unwrap();

    // user_01 应该收到
    let msg1 = tokio::time::timeout(Duration::from_millis(200), s1_rx.recv())
        .await
        .expect("Timeout")
        .expect("No message");

    assert!(msg1.contains("user_01"));

    // user_02 不应该收到
    let msg2 = tokio::time::timeout(Duration::from_millis(200), s2_rx.recv()).await;
    assert!(
        msg2.is_err(),
        "user_02 should not receive user_01's message"
    );
}

/// 测试不同优先级消息
#[tokio::test]
async fn test_message_priority() {
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    broker.register_gateway("gateway_01", tx);
    broker.subscribe("user_01", "gateway_01");

    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    let _pusher = gateway.clone().start_notification_pusher();
    let _processor = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发送 P0 消息（风控警告，最高优先级）
    let p0_payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let p0_notif = Notification::with_priority(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        p0_payload,
        0, // P0 最高优先级
        "Test",
    );

    broker.publish(p0_notif).unwrap();

    // 验证 P0 消息立即被推送
    let received = tokio::time::timeout(Duration::from_millis(100), session_rx.recv())
        .await
        .expect("P0 message should be received quickly")
        .expect("No message");

    assert!(received.contains("user_01"));
}

/// 测试批量消息推送
#[tokio::test]
async fn test_batch_notification() {
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    broker.register_gateway("gateway_01", tx);
    broker.subscribe("user_01", "gateway_01");

    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    let _pusher = gateway.clone().start_notification_pusher();
    let _processor = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发送 10 条消息
    for i in 0..10 {
        let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 1000000.0 + i as f64,
            available: 980000.0,
            frozen: 0.0,
            margin: 20000.0,
            position_profit: i as f64 * 100.0,
            close_profit: 1000.0,
            risk_ratio: 0.02,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        });

        broker
            .publish(Notification::new(
                NotificationType::AccountUpdate,
                Arc::from("user_01"),
                payload,
                "Test",
            ))
            .unwrap();
    }

    // 等待批量推送
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 接收所有消息
    let mut count = 0;
    while let Ok(Some(_json)) =
        tokio::time::timeout(Duration::from_millis(50), session_rx.recv()).await
    {
        count += 1;
    }

    assert_eq!(count, 10, "Should receive all 10 messages");
}

/// 测试消息去重
#[tokio::test]
async fn test_message_deduplication() {
    let broker = Arc::new(NotificationBroker::new());

    let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        payload,
        "Test",
    );

    // 第一次发布
    broker.publish(notification.clone()).unwrap();

    // 第二次发布（相同 message_id）
    broker.publish(notification.clone()).unwrap();

    let stats = broker.get_stats();

    // 只发送一次，去重一次
    assert_eq!(stats.messages_sent, 1);
    assert_eq!(stats.messages_deduplicated, 1);
}

/// 测试 Gateway 统计信息
#[tokio::test]
async fn test_gateway_stats() {
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

    broker.register_gateway("gateway_01", tx);
    broker.subscribe("user_01", "gateway_01");

    let (session_tx, _session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    let stats = gateway.get_stats();

    assert_eq!(stats.gateway_id.as_ref(), "gateway_01");
    assert_eq!(stats.active_sessions, 1);
}

/// 测试会话注销
#[tokio::test]
async fn test_session_unregister() {
    let (_tx, rx) = mpsc::unbounded_channel();
    let gateway = NotificationGateway::new("gateway_01", rx);

    let (session_tx, _session_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session_tx);

    assert_eq!(gateway.get_stats().active_sessions, 1);

    gateway.unregister_session("session_01");

    assert_eq!(gateway.get_stats().active_sessions, 0);
}
