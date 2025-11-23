//! 通知推送网关（NotificationGateway）
//!
//! 职责：
//! 1. 管理所有WebSocket会话
//! 2. 接收来自Broker的通知消息
//! 3. 推送消息到对应的WebSocket客户端
//! 4. 批量推送优化（减少网络往返）
//! 5. 断线重连处理

use super::message::Notification;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// WebSocket会话信息
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 会话ID
    pub session_id: Arc<str>,

    /// 用户ID
    pub user_id: Arc<str>,

    /// 消息发送通道（发送到WebSocket客户端）
    pub sender: mpsc::UnboundedSender<String>,

    /// 订阅的频道（trade, orderbook, account, position）
    pub subscriptions: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,

    /// 连接时间
    pub connected_at: i64,

    /// 最后活跃时间
    pub last_active: Arc<std::sync::atomic::AtomicI64>,
}

/// 通知推送网关
pub struct NotificationGateway {
    /// Gateway ID
    gateway_id: Arc<str>,

    /// 会话管理：session_id -> SessionInfo
    sessions: DashMap<Arc<str>, SessionInfo>,

    /// 用户会话索引：user_id -> Vec<session_id>
    user_sessions: DashMap<Arc<str>, Vec<Arc<str>>>,

    /// 接收来自Broker的通知（使用tokio::sync::Mutex）
    notification_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Notification>>>,

    /// 批量推送配置
    batch_size: usize,
    batch_interval_ms: u64,

    /// 统计信息
    stats: Arc<GatewayStats>,
}

/// Gateway统计信息
#[derive(Debug, Default)]
pub struct GatewayStats {
    /// 已推送消息数
    pub messages_pushed: std::sync::atomic::AtomicU64,

    /// 推送失败数
    pub messages_failed: std::sync::atomic::AtomicU64,

    /// 当前会话数
    pub active_sessions: std::sync::atomic::AtomicUsize,
}

impl NotificationGateway {
    /// 创建新的Gateway
    pub fn new(
        gateway_id: impl Into<Arc<str>>,
        notification_receiver: mpsc::UnboundedReceiver<Notification>,
    ) -> Self {
        Self {
            gateway_id: gateway_id.into(),
            sessions: DashMap::new(),
            user_sessions: DashMap::new(),
            notification_receiver: Arc::new(tokio::sync::Mutex::new(notification_receiver)),
            batch_size: 100,
            batch_interval_ms: 100,
            stats: Arc::new(GatewayStats::default()),
        }
    }

    /// 注册WebSocket会话
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
            subscriptions: Arc::new(parking_lot::RwLock::new(std::collections::HashSet::new())),
            connected_at: chrono::Utc::now().timestamp(),
            last_active: Arc::new(std::sync::atomic::AtomicI64::new(
                chrono::Utc::now().timestamp(),
            )),
        };

        // 添加到会话表
        self.sessions.insert(session_id.clone(), session_info);

        // 添加到用户索引
        self.user_sessions
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(session_id.clone());

        self.stats
            .active_sessions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        log::info!("Session registered: {} for user {}", session_id, user_id);
    }

    /// 注销WebSocket会话
    pub fn unregister_session(&self, session_id: &str) {
        if let Some((_, session_info)) = self.sessions.remove(session_id) {
            // 从用户索引中移除
            if let Some(mut sessions) = self.user_sessions.get_mut(&session_info.user_id) {
                sessions.retain(|sid| sid.as_ref() != session_id);
            }

            self.stats
                .active_sessions
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

            log::info!("Session unregistered: {}", session_id);
        }
    }

    /// 订阅频道
    pub fn subscribe_channel(&self, session_id: &str, channel: impl Into<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().insert(channel.into());
            log::debug!("Session {} subscribed to channel", session_id);
        }
    }

    /// 取消订阅频道
    pub fn unsubscribe_channel(&self, session_id: &str, channel: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().remove(channel);
            log::debug!(
                "Session {} unsubscribed from channel {}",
                session_id,
                channel
            );
        }
    }

    /// 批量订阅频道
    pub fn subscribe_channels(&self, session_id: &str, channels: Vec<String>) {
        if let Some(session) = self.sessions.get(session_id) {
            let mut subs = session.subscriptions.write();
            for channel in channels {
                subs.insert(channel);
            }
            log::debug!(
                "Session {} subscribed to {} channels",
                session_id,
                subs.len()
            );
        }
    }

    /// 取消所有订阅
    pub fn unsubscribe_all(&self, session_id: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            session.subscriptions.write().clear();
            log::debug!("Session {} unsubscribed from all channels", session_id);
        }
    }

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

    /// 启动通知推送任务
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

            log::info!(
                "Notification pusher stopped for gateway {}",
                self.gateway_id
            );
        })
    }

    /// 推送单条通知
    async fn push_notification(&self, notification: &Notification) {
        // 查找该用户的所有会话
        if let Some(session_ids) = self.user_sessions.get(&notification.user_id) {
            for session_id in session_ids.iter() {
                if let Some(session) = self.sessions.get(session_id.as_ref()) {
                    // 检查订阅过滤
                    let subscriptions = session.subscriptions.read();
                    let notification_channel = notification.message_type.channel();

                    // 如果会话设置了订阅过滤（subscriptions非空），则只推送订阅的频道
                    // 如果subscriptions为空，则推送所有通知（默认行为）
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

                    // 手动构造 JSON（避免 Arc<str> 序列化问题）
                    let json = notification.to_json();

                    // 发送到WebSocket
                    if let Err(e) = session.sender.send(json) {
                        log::error!(
                            "Failed to send notification to session {}: {}",
                            session_id,
                            e
                        );
                        self.stats
                            .messages_failed
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        self.stats
                            .messages_pushed
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        // 更新最后活跃时间
                        session.last_active.store(
                            chrono::Utc::now().timestamp(),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                    }
                }
            }
        }
    }

    /// 批量推送通知
    async fn push_batch(&self, notifications: &[Notification]) {
        // 按用户分组
        let mut grouped: std::collections::HashMap<Arc<str>, Vec<&Notification>> =
            std::collections::HashMap::new();

        for notif in notifications {
            grouped
                .entry(notif.user_id.clone())
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

    /// 启动心跳检测任务（清理死连接）
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

                    let last_active = session
                        .last_active
                        .load(std::sync::atomic::Ordering::Relaxed);
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

    /// 获取统计信息
    pub fn get_stats(&self) -> GatewayStatsSnapshot {
        GatewayStatsSnapshot {
            gateway_id: self.gateway_id.clone(),
            messages_pushed: self
                .stats
                .messages_pushed
                .load(std::sync::atomic::Ordering::Relaxed),
            messages_failed: self
                .stats
                .messages_failed
                .load(std::sync::atomic::Ordering::Relaxed),
            active_sessions: self
                .stats
                .active_sessions
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// 获取Gateway ID
    pub fn gateway_id(&self) -> &Arc<str> {
        &self.gateway_id
    }
}

/// 统计信息快照
#[derive(Debug, Clone)]
pub struct GatewayStatsSnapshot {
    pub gateway_id: Arc<str>,
    pub messages_pushed: u64,
    pub messages_failed: u64,
    pub active_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::message::{
        AccountUpdateNotify, NotificationPayload, NotificationType,
    };

    #[tokio::test]
    async fn test_gateway_creation() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let gateway = NotificationGateway::new("gateway_01", rx);

        let stats = gateway.get_stats();
        assert_eq!(stats.active_sessions, 0);
    }

    #[tokio::test]
    async fn test_session_registration() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let gateway = NotificationGateway::new("gateway_01", rx);

        let (session_tx, _session_rx) = mpsc::unbounded_channel();
        gateway.register_session("session_01", "user_01", session_tx);

        let stats = gateway.get_stats();
        assert_eq!(stats.active_sessions, 1);
    }

    #[tokio::test]
    async fn test_notification_push() {
        let (tx, rx) = mpsc::unbounded_channel();
        let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

        let (session_tx, mut session_rx) = mpsc::unbounded_channel();
        gateway.register_session("session_01", "user_01", session_tx);

        // 启动推送任务
        let _handle = gateway.clone().start_notification_pusher();

        // 发送通知
        let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 1000000.0,
            available: 980000.0,
            frozen: 0.0,
            margin: 20000.0,
            position_profit: 500.0,
            close_profit: 1000.0,
            risk_ratio: 0.02,
            timestamp: 1728123456789,
        });

        let notification = Notification::new(
            NotificationType::AccountUpdate,
            Arc::from("user_01"),
            payload,
            "AccountSystem",
        );

        tx.send(notification).unwrap();

        // 接收WebSocket消息
        tokio::time::timeout(Duration::from_secs(1), async {
            if let Some(json) = session_rx.recv().await {
                assert!(json.contains("account_update"));
                assert!(json.contains("user_01"));
            }
        })
        .await
        .expect("Timeout waiting for message");
    }

    #[tokio::test]
    async fn test_batch_push() {
        let (tx, rx) = mpsc::unbounded_channel();
        let gateway = Arc::new(NotificationGateway::new("gateway_01", rx));

        let (session_tx, mut session_rx) = mpsc::unbounded_channel();
        gateway.register_session("session_01", "user_01", session_tx);

        // 启动推送任务
        let _handle = gateway.clone().start_notification_pusher();

        // 发送多条通知
        for i in 0..5 {
            let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
                user_id: "user_01".to_string(),
                balance: 1000000.0 + i as f64,
                available: 980000.0,
                frozen: 0.0,
                margin: 20000.0,
                position_profit: 500.0,
                close_profit: 1000.0,
                risk_ratio: 0.02,
                timestamp: 1728123456789,
            });

            let notification = Notification::new(
                NotificationType::AccountUpdate,
                Arc::from("user_01"),
                payload,
                "AccountSystem",
            );

            tx.send(notification).unwrap();
        }

        // 等待批量推送
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 接收所有消息
        let mut count = 0;
        while let Ok(Some(_json)) =
            tokio::time::timeout(Duration::from_millis(100), session_rx.recv()).await
        {
            count += 1;
        }

        assert_eq!(count, 5);
    }
}
