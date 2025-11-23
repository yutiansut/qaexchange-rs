//! 通知消息路由中心（NotificationBroker）
//!
//! 职责：
//! 1. 接收来自各业务模块的通知消息
//! 2. 按用户ID路由消息到对应的Gateway
//! 3. 消息去重（基于message_id）
//! 4. 消息持久化（可选，支持断线重连）
//! 5. 优先级队列管理

use super::message::Notification;
use crossbeam::queue::ArrayQueue;
use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 通知路由中心
pub struct NotificationBroker {
    /// 用户订阅表：user_id -> Vec<gateway_id>
    /// 使用 DashMap 实现无锁并发访问
    user_gateways: DashMap<Arc<str>, Vec<Arc<str>>>,

    /// Gateway通道：gateway_id -> Sender
    /// 使用 tokio::mpsc 异步通道
    gateway_senders: DashMap<Arc<str>, mpsc::UnboundedSender<Notification>>,

    /// 全局订阅者（接收所有通知，如：存储系统、监控系统）
    /// 使用 DashMap 存储，key 为 gateway_id
    global_subscribers: DashMap<Arc<str>, mpsc::UnboundedSender<Notification>>,

    /// 消息去重缓存（最近1小时的消息ID）
    /// 使用 Mutex<HashSet> 保护（短暂锁定）
    dedup_cache: Arc<Mutex<HashSet<Arc<str>>>>,

    /// 优先级队列（P0/P1/P2/P3）
    /// 使用 crossbeam 无锁队列
    priority_queues: [Arc<ArrayQueue<Notification>>; 4],

    /// 统计信息
    stats: Arc<BrokerStats>,
}

/// Broker统计信息
#[derive(Debug, Default)]
pub struct BrokerStats {
    /// 已发送消息数
    pub messages_sent: std::sync::atomic::AtomicU64,

    /// 已去重消息数
    pub messages_deduplicated: std::sync::atomic::AtomicU64,

    /// 已丢弃消息数（队列满）
    pub messages_dropped: std::sync::atomic::AtomicU64,
}

impl NotificationBroker {
    /// 创建新的Broker
    pub fn new() -> Self {
        Self {
            user_gateways: DashMap::new(),
            gateway_senders: DashMap::new(),
            global_subscribers: DashMap::new(),
            dedup_cache: Arc::new(Mutex::new(HashSet::new())),
            priority_queues: [
                Arc::new(ArrayQueue::new(10000)),  // P0队列
                Arc::new(ArrayQueue::new(50000)),  // P1队列
                Arc::new(ArrayQueue::new(100000)), // P2队列
                Arc::new(ArrayQueue::new(50000)),  // P3队列
            ],
            stats: Arc::new(BrokerStats::default()),
        }
    }

    /// 注册Gateway
    ///
    /// # 参数
    /// - `gateway_id`: Gateway唯一标识
    /// - `sender`: 发送通道
    pub fn register_gateway(
        &self,
        gateway_id: impl Into<Arc<str>>,
        sender: mpsc::UnboundedSender<Notification>,
    ) {
        let gateway_id = gateway_id.into();
        self.gateway_senders.insert(gateway_id.clone(), sender);
        log::info!("Gateway registered: {}", gateway_id);
    }

    /// 注销Gateway
    pub fn unregister_gateway(&self, gateway_id: &str) {
        self.gateway_senders.remove(gateway_id);

        // 清理该Gateway的所有用户订阅
        self.user_gateways.retain(|_user_id, gateways| {
            gateways.retain(|gid| gid.as_ref() != gateway_id);
            !gateways.is_empty()
        });

        log::info!("Gateway unregistered: {}", gateway_id);
    }

    /// 订阅用户消息
    ///
    /// # 参数
    /// - `user_id`: 用户ID
    /// - `gateway_id`: Gateway ID
    pub fn subscribe(&self, user_id: impl Into<Arc<str>>, gateway_id: impl Into<Arc<str>>) {
        let user_id = user_id.into();
        let gateway_id = gateway_id.into();

        self.user_gateways
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(gateway_id.clone());

        log::debug!("User {} subscribed to gateway {}", user_id, gateway_id);
    }

    /// 全局订阅（接收所有通知）
    ///
    /// # 参数
    /// - `subscriber_id`: 订阅者ID（如：storage, monitoring）
    /// - `sender`: 发送通道
    pub fn subscribe_global(
        &self,
        subscriber_id: impl Into<Arc<str>>,
        sender: mpsc::UnboundedSender<Notification>,
    ) {
        let subscriber_id = subscriber_id.into();
        self.global_subscribers
            .insert(subscriber_id.clone(), sender);
        log::info!("Global subscriber registered: {}", subscriber_id);
    }

    /// 取消全局订阅
    pub fn unsubscribe_global(&self, subscriber_id: &str) {
        self.global_subscribers.remove(subscriber_id);
        log::info!("Global subscriber unregistered: {}", subscriber_id);
    }

    /// 取消订阅
    pub fn unsubscribe(&self, user_id: &str, gateway_id: &str) {
        if let Some(mut gateways) = self.user_gateways.get_mut(user_id) {
            gateways.retain(|gid| gid.as_ref() != gateway_id);
        }

        log::debug!("User {} unsubscribed from gateway {}", user_id, gateway_id);
    }

    /// 发布通知消息
    ///
    /// # 参数
    /// - `notification`: 通知消息
    ///
    /// # 返回
    /// - `Ok(())`: 发布成功
    /// - `Err(String)`: 发布失败
    pub fn publish(&self, notification: Notification) -> Result<(), String> {
        // 1. 消息去重
        if self.is_duplicate(&notification.message_id) {
            self.stats
                .messages_deduplicated
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(());
        }

        // 2. 按优先级入队
        let priority = notification.priority.min(3) as usize;
        if let Err(_) = self.priority_queues[priority].push(notification.clone()) {
            // 队列满，丢弃消息
            self.stats
                .messages_dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            log::warn!("Priority queue {} is full, message dropped", priority);
            return Err(format!("Priority queue {} is full", priority));
        }

        // 3. 消息已入队，由 priority_processor 统一路由
        // 注意：不要在这里立即路由，否则会导致消息重复发送
        self.stats
            .messages_sent
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// 路由通知到Gateway
    fn route_notification(&self, notification: &Notification) {
        // 1. 发送到用户特定的 Gateway
        if let Some(gateways) = self.user_gateways.get(notification.user_id.as_ref()) {
            for gateway_id in gateways.iter() {
                if let Some(sender) = self.gateway_senders.get(gateway_id.as_ref()) {
                    // 使用 tokio::mpsc 发送（零成本）
                    if let Err(e) = sender.send(notification.clone()) {
                        log::error!(
                            "Failed to send notification to gateway {}: {}",
                            gateway_id,
                            e
                        );
                    }
                } else {
                    log::warn!("Gateway {} not found in senders", gateway_id);
                }
            }
        } else {
            log::debug!("No gateways found for user {}", notification.user_id);
        }

        // 2. 发送到所有全局订阅者
        for entry in self.global_subscribers.iter() {
            let subscriber_id = entry.key();
            let sender = entry.value();
            if let Err(e) = sender.send(notification.clone()) {
                log::error!(
                    "Failed to send notification to global subscriber {}: {}",
                    subscriber_id,
                    e
                );
            }
        }
    }

    /// 检查消息是否重复
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
            let to_remove: Vec<Arc<str>> = cache.iter().take(5000).cloned().collect();
            for id in to_remove {
                cache.remove(&id);
            }
        }

        false
    }

    /// 启动优先级处理器（在单独的tokio任务中运行）
    pub fn start_priority_processor(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_micros(100));

            loop {
                interval.tick().await;

                // 优先处理高优先级消息
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

    /// 获取统计信息
    pub fn get_stats(&self) -> BrokerStatsSnapshot {
        BrokerStatsSnapshot {
            messages_sent: self
                .stats
                .messages_sent
                .load(std::sync::atomic::Ordering::Relaxed),
            messages_deduplicated: self
                .stats
                .messages_deduplicated
                .load(std::sync::atomic::Ordering::Relaxed),
            messages_dropped: self
                .stats
                .messages_dropped
                .load(std::sync::atomic::Ordering::Relaxed),
            active_users: self.user_gateways.len(),
            active_gateways: self.gateway_senders.len(),
            queue_sizes: [
                self.priority_queues[0].len(),
                self.priority_queues[1].len(),
                self.priority_queues[2].len(),
                self.priority_queues[3].len(),
            ],
        }
    }

    /// 清空去重缓存（用于测试）
    #[cfg(test)]
    pub fn clear_dedup_cache(&self) {
        self.dedup_cache.lock().clear();
    }
}

impl Default for NotificationBroker {
    fn default() -> Self {
        Self::new()
    }
}

/// 统计信息快照
#[derive(Debug, Clone)]
pub struct BrokerStatsSnapshot {
    pub messages_sent: u64,
    pub messages_deduplicated: u64,
    pub messages_dropped: u64,
    pub active_users: usize,
    pub active_gateways: usize,
    pub queue_sizes: [usize; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::message::{
        AccountUpdateNotify, NotificationPayload, NotificationType,
    };

    #[tokio::test]
    async fn test_broker_creation() {
        let broker = NotificationBroker::new();
        let stats = broker.get_stats();

        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.active_users, 0);
        assert_eq!(stats.active_gateways, 0);
    }

    #[tokio::test]
    async fn test_gateway_registration() {
        let broker = NotificationBroker::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        broker.register_gateway("gateway_01", tx);

        let stats = broker.get_stats();
        assert_eq!(stats.active_gateways, 1);
    }

    #[tokio::test]
    async fn test_user_subscription() {
        let broker = NotificationBroker::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        broker.register_gateway("gateway_01", tx);
        broker.subscribe("user_01", "gateway_01");

        let stats = broker.get_stats();
        assert_eq!(stats.active_users, 1);
    }

    #[tokio::test]
    async fn test_publish_notification() {
        let broker = Arc::new(NotificationBroker::new());
        let (tx, mut rx) = mpsc::unbounded_channel();

        broker.register_gateway("gateway_01", tx);
        broker.subscribe("user_01", "gateway_01");

        // 启动优先级处理器
        let _processor = broker.clone().start_priority_processor();

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

        broker.publish(notification.clone()).unwrap();

        // 接收消息（等待优先级处理器处理）
        let received = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .expect("Timeout waiting for message")
            .unwrap();

        assert_eq!(received.user_id.as_ref(), "user_01");
        assert_eq!(received.message_type, NotificationType::AccountUpdate);
    }

    #[tokio::test]
    async fn test_message_deduplication() {
        let broker = NotificationBroker::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        broker.register_gateway("gateway_01", tx);
        broker.subscribe("user_01", "gateway_01");

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

        // 第一次发布
        broker.publish(notification.clone()).unwrap();

        // 第二次发布（相同message_id）
        broker.publish(notification.clone()).unwrap();

        let stats = broker.get_stats();
        assert_eq!(stats.messages_sent, 1); // 只发送一次
        assert_eq!(stats.messages_deduplicated, 1); // 去重一次
    }

    #[tokio::test]
    async fn test_priority_queue() {
        let broker = Arc::new(NotificationBroker::new());
        let (tx, _rx) = mpsc::unbounded_channel();

        broker.register_gateway("gateway_01", tx);
        broker.subscribe("user_01", "gateway_01");

        // 发送不同优先级的消息
        for priority in 0..=3 {
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

            let notification = Notification::with_priority(
                NotificationType::AccountUpdate,
                Arc::from("user_01"),
                payload,
                priority,
                "Test",
            );

            broker.publish(notification).unwrap();
        }

        // 启动优先级处理器
        let _handle = broker.clone().start_priority_processor();

        // 接收消息（应该按优先级顺序）
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // 验证队列大小
        let stats = broker.get_stats();
        println!("Queue sizes: {:?}", stats.queue_sizes);
    }
}
