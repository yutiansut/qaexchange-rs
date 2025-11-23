//! 通知消息系统完整示例
//!
//! 演示如何使用通知消息系统：
//! 1. 创建 NotificationBroker 和 NotificationGateway
//! 2. 注册 WebSocket 会话
//! 3. 发送各类通知消息
//! 4. 接收和处理通知

use qaexchange::notification::{
    AccountUpdateNotify, Notification, NotificationBroker, NotificationGateway,
    NotificationPayload, NotificationType, OrderAcceptedNotify, RiskAlertNotify,
    TradeExecutedNotify,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // 初始化日志
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("=== 通知消息系统示例 ===\n");

    // ============================================================================
    // 1. 创建 NotificationBroker
    // ============================================================================
    log::info!("1. 创建 NotificationBroker");
    let broker = Arc::new(NotificationBroker::new());

    // ============================================================================
    // 2. 创建 NotificationGateway
    // ============================================================================
    log::info!("2. 创建 NotificationGateway");
    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gateway_01", gateway_rx));

    // ============================================================================
    // 3. 注册 Gateway 到 Broker
    // ============================================================================
    log::info!("3. 注册 Gateway 到 Broker");
    broker.register_gateway("gateway_01", gateway_tx.clone());

    // ============================================================================
    // 4. 订阅用户消息
    // ============================================================================
    log::info!("4. 订阅用户消息");
    broker.subscribe("user_01", "gateway_01");
    broker.subscribe("user_02", "gateway_01");

    // ============================================================================
    // 5. 注册 WebSocket 会话
    // ============================================================================
    log::info!("5. 注册 WebSocket 会话");

    // 用户1的会话
    let (session1_tx, mut session1_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_01", "user_01", session1_tx);

    // 用户2的会话
    let (session2_tx, mut session2_rx) = mpsc::unbounded_channel();
    gateway.register_session("session_02", "user_02", session2_tx);

    // ============================================================================
    // 6. 启动推送任务
    // ============================================================================
    log::info!("6. 启动推送任务");
    let gateway_clone = gateway.clone();
    let _pusher_handle = gateway_clone.start_notification_pusher();

    let broker_clone = broker.clone();
    let _processor_handle = broker_clone.start_priority_processor();

    // 等待任务启动
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ============================================================================
    // 7. 启动消息接收任务
    // ============================================================================
    log::info!("7. 启动消息接收任务\n");

    // 用户1接收任务
    let session1_handle = tokio::spawn(async move {
        let mut count = 0;
        while let Some(json) = session1_rx.recv().await {
            count += 1;
            log::info!(
                "[Session 1] Received message {}:\n{}\n",
                count,
                serde_json::to_string_pretty(
                    &serde_json::from_str::<serde_json::Value>(&json).unwrap()
                )
                .unwrap()
            );

            if count >= 5 {
                break;
            }
        }
        log::info!("[Session 1] Received {} messages total", count);
    });

    // 用户2接收任务
    let session2_handle = tokio::spawn(async move {
        let mut count = 0;
        while let Some(json) = session2_rx.recv().await {
            count += 1;
            log::info!(
                "[Session 2] Received message {}:\n{}\n",
                count,
                serde_json::to_string_pretty(
                    &serde_json::from_str::<serde_json::Value>(&json).unwrap()
                )
                .unwrap()
            );

            if count >= 2 {
                break;
            }
        }
        log::info!("[Session 2] Received {} messages total", count);
    });

    // ============================================================================
    // 8. 发送各类通知消息
    // ============================================================================
    log::info!("8. 发送各类通知消息\n");

    // 等待接收任务启动
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 8.1 订单确认通知（P1 - 高优先级）
    log::info!("8.1 发送订单确认通知（用户1）");
    let order_accepted = NotificationPayload::OrderAccepted(OrderAcceptedNotify {
        order_id: "a1b2c3d4-e5f6-7890-abcd-1234567890ab".to_string(),
        exchange_order_id: "EX_1728123456789_IX2401_B".to_string(),
        instrument_id: "IX2401".to_string(),
        direction: "BUY".to_string(),
        offset: "OPEN".to_string(),
        price: 100.0,
        volume: 10.0,
        order_type: "LIMIT".to_string(),
        frozen_margin: 20000.0,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification1 = Notification::new(
        NotificationType::OrderAccepted,
        Arc::from("user_01"),
        order_accepted,
        "MatchingEngine",
    );

    broker.publish(notification1).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8.2 成交回报通知（P1 - 高优先级）
    log::info!("8.2 发送成交回报通知（用户1）");
    let trade_executed = NotificationPayload::TradeExecuted(TradeExecutedNotify {
        trade_id: "TRADE_1728123456789".to_string(),
        order_id: "a1b2c3d4-e5f6-7890-abcd-1234567890ab".to_string(),
        exchange_order_id: "EX_1728123456789_IX2401_B".to_string(),
        instrument_id: "IX2401".to_string(),
        direction: "BUY".to_string(),
        offset: "OPEN".to_string(),
        price: 100.0,
        volume: 10.0,
        commission: 0.5,
        fill_type: "FULL".to_string(),
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification2 = Notification::new(
        NotificationType::TradeExecuted,
        Arc::from("user_01"),
        trade_executed,
        "MatchingEngine",
    );

    broker.publish(notification2).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8.3 账户更新通知（P2 - 中优先级）
    log::info!("8.3 发送账户更新通知（用户1）");
    let account_update = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 979999.5,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 0.0,
        close_profit: 0.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification3 = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        account_update,
        "AccountSystem",
    );

    broker.publish(notification3).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8.4 风控预警通知（P0 - 最高优先级）
    log::info!("8.4 发送风控预警通知（用户1）");
    let risk_alert = NotificationPayload::RiskAlert(RiskAlertNotify {
        user_id: "user_01".to_string(),
        alert_type: "MARGIN_INSUFFICIENT".to_string(),
        severity: "WARNING".to_string(),
        message: "保证金占用率接近80%，请注意风险".to_string(),
        risk_ratio: 0.78,
        suggestion: "建议追加保证金或减少持仓".to_string(),
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification4 = Notification::new(
        NotificationType::RiskAlert,
        Arc::from("user_01"),
        risk_alert,
        "RiskControl",
    );

    broker.publish(notification4).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8.5 给用户2发送账户更新（测试多用户）
    log::info!("8.5 发送账户更新通知（用户2）");
    let account_update2 = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_02".to_string(),
        balance: 500000.0,
        available: 490000.0,
        frozen: 0.0,
        margin: 10000.0,
        position_profit: 500.0,
        close_profit: 200.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification5 = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_02"),
        account_update2,
        "AccountSystem",
    );

    broker.publish(notification5).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 8.6 测试批量推送（快速发送多条P2消息）
    log::info!("8.6 测试批量推送（用户1，5条账户更新）");
    for i in 0..5 {
        let account_update = NotificationPayload::AccountUpdate(AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 1000000.0 + i as f64 * 100.0,
            available: 980000.0 + i as f64 * 100.0,
            frozen: 0.0,
            margin: 20000.0,
            position_profit: i as f64 * 10.0,
            close_profit: 0.0,
            risk_ratio: 0.02,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        });

        let notification = Notification::new(
            NotificationType::AccountUpdate,
            Arc::from("user_01"),
            account_update,
            "AccountSystem",
        );

        broker.publish(notification).unwrap();
    }

    // 给用户2发送一条
    let account_update2 = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_02".to_string(),
        balance: 500000.0,
        available: 490000.0,
        frozen: 0.0,
        margin: 10000.0,
        position_profit: 1000.0,
        close_profit: 500.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification6 = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_02"),
        account_update2,
        "AccountSystem",
    );

    broker.publish(notification6).unwrap();

    // ============================================================================
    // 9. 等待消息接收完成
    // ============================================================================
    log::info!("\n9. 等待消息接收完成...\n");

    let _ = tokio::join!(session1_handle, session2_handle);

    // ============================================================================
    // 10. 显示统计信息
    // ============================================================================
    log::info!("\n10. 统计信息");
    let broker_stats = broker.get_stats();
    log::info!("Broker统计:");
    log::info!("  - 已发送消息: {}", broker_stats.messages_sent);
    log::info!("  - 已去重消息: {}", broker_stats.messages_deduplicated);
    log::info!("  - 已丢弃消息: {}", broker_stats.messages_dropped);
    log::info!("  - 活跃用户数: {}", broker_stats.active_users);
    log::info!("  - 活跃Gateway数: {}", broker_stats.active_gateways);
    log::info!(
        "  - 队列大小: P0={}, P1={}, P2={}, P3={}",
        broker_stats.queue_sizes[0],
        broker_stats.queue_sizes[1],
        broker_stats.queue_sizes[2],
        broker_stats.queue_sizes[3]
    );

    let gateway_stats = gateway.get_stats();
    log::info!("\nGateway统计:");
    log::info!("  - Gateway ID: {}", gateway_stats.gateway_id);
    log::info!("  - 已推送消息: {}", gateway_stats.messages_pushed);
    log::info!("  - 推送失败数: {}", gateway_stats.messages_failed);
    log::info!("  - 活跃会话数: {}", gateway_stats.active_sessions);

    log::info!("\n=== 示例完成 ===");
}
