//! 市场数据广播系统
//!
//! 负责将订单簿变化、成交数据广播给所有订阅者
//!
//! ## 高性能特性
//!
//! - **有界通道**: 防止内存溢出，提供背压控制
//! - **慢订阅者检测**: 自动降级或断开慢速消费者
//! - **指标统计**: 实时监控广播质量
//! - **零阻塞广播**: 使用 `try_send` 避免阻塞生产者
//!
//! @author @yutiansut @quantaxis

use super::PriceLevel;
use crate::ExchangeError;
use crossbeam::channel::{bounded, Receiver, Sender, TrySendError};
use dashmap::DashMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 市场数据事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketDataEvent {
    /// Level2 订单簿快照
    OrderBookSnapshot {
        instrument_id: String,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        timestamp: i64,
    },

    /// 订单簿增量更新
    OrderBookUpdate {
        instrument_id: String,
        side: String, // "bid" or "ask"
        price: f64,
        volume: f64, // 0 表示删除该价格档位
        timestamp: i64,
    },

    /// Tick 成交数据
    Tick {
        instrument_id: String,
        price: f64,
        volume: f64,
        direction: String, // "buy" or "sell"
        timestamp: i64,
    },

    /// 最新价更新
    LastPrice {
        instrument_id: String,
        price: f64,
        timestamp: i64,
    },

    /// K线完成事件（用于实时推送）
    KLineFinished {
        instrument_id: String,
        period: i32, // HQChart周期格式 (0=日线, 4=1分钟, 5=5分钟等)
        kline: super::kline::KLine,
        timestamp: i64,
    },
}

/// 广播器配置
#[derive(Debug, Clone)]
pub struct BroadcasterConfig {
    /// 每个订阅者的通道容量
    pub channel_capacity: usize,
    /// 警告阈值（队列使用率）
    pub warning_threshold: f64,
    /// 自动断开慢订阅者的阈值
    pub disconnect_threshold: usize,
    /// 是否启用批量广播
    pub enable_batch: bool,
    /// 批量大小
    pub batch_size: usize,
}

impl Default for BroadcasterConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 10000,      // 10K 事件缓冲
            warning_threshold: 0.8,        // 80% 使用率警告
            disconnect_threshold: 5,       // 连续5次满则断开
            enable_batch: true,
            batch_size: 100,               // 批量发送100个事件
        }
    }
}

/// 订阅者统计信息
#[derive(Debug, Default)]
pub struct SubscriberStats {
    /// 发送成功次数
    pub sent_count: AtomicU64,
    /// 发送失败（队列满）次数
    pub dropped_count: AtomicU64,
    /// 连续失败次数
    pub consecutive_failures: AtomicU64,
    /// 最后活跃时间 (unix timestamp ms)
    pub last_active_ms: AtomicU64,
}

impl SubscriberStats {
    pub fn new() -> Self {
        Self {
            sent_count: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
            consecutive_failures: AtomicU64::new(0),
            last_active_ms: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            ),
        }
    }

    pub fn record_success(&self) {
        self.sent_count.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.last_active_ms.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn record_failure(&self) {
        self.dropped_count.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_slow(&self, threshold: usize) -> bool {
        self.consecutive_failures.load(Ordering::Relaxed) >= threshold as u64
    }
}

/// 订阅信息
#[derive(Debug, Clone)]
struct Subscription {
    /// 订阅的合约列表
    instruments: Vec<String>,
    /// 订阅的频道（orderbook, tick, etc.）
    channels: Vec<String>,
}

/// 订阅者完整信息
struct SubscriberInfo {
    sender: Sender<MarketDataEvent>,
    subscription: Subscription,
    stats: Arc<SubscriberStats>,
}

/// 广播统计信息
#[derive(Debug, Default)]
pub struct BroadcastStats {
    /// 总广播次数
    pub total_broadcasts: AtomicU64,
    /// 总发送成功次数
    pub total_sent: AtomicU64,
    /// 总丢弃次数
    pub total_dropped: AtomicU64,
    /// 广播总耗时 (微秒)
    pub total_time_us: AtomicU64,
    /// 慢订阅者断开次数
    pub slow_disconnects: AtomicU64,
}

impl BroadcastStats {
    pub fn get_summary(&self) -> BroadcastStatsSummary {
        BroadcastStatsSummary {
            total_broadcasts: self.total_broadcasts.load(Ordering::Relaxed),
            total_sent: self.total_sent.load(Ordering::Relaxed),
            total_dropped: self.total_dropped.load(Ordering::Relaxed),
            total_time_us: self.total_time_us.load(Ordering::Relaxed),
            slow_disconnects: self.slow_disconnects.load(Ordering::Relaxed),
        }
    }
}

/// 广播统计摘要 (可序列化)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastStatsSummary {
    pub total_broadcasts: u64,
    pub total_sent: u64,
    pub total_dropped: u64,
    pub total_time_us: u64,
    pub slow_disconnects: u64,
}

impl BroadcastStatsSummary {
    /// 计算丢弃率
    pub fn drop_rate(&self) -> f64 {
        let total = self.total_sent + self.total_dropped;
        if total == 0 {
            0.0
        } else {
            self.total_dropped as f64 / total as f64
        }
    }

    /// 计算平均广播延迟 (微秒)
    pub fn avg_broadcast_latency_us(&self) -> f64 {
        if self.total_broadcasts == 0 {
            0.0
        } else {
            self.total_time_us as f64 / self.total_broadcasts as f64
        }
    }
}

/// 市场数据广播器
///
/// 高性能有界通道广播器，支持：
/// - 背压控制：防止慢消费者拖慢系统
/// - 慢订阅者检测：自动断开持续落后的订阅者
/// - 实时统计：监控广播性能和丢弃率
pub struct MarketDataBroadcaster {
    /// 配置
    config: BroadcasterConfig,
    /// 订阅者映射 (subscriber_id -> SubscriberInfo)
    subscribers: Arc<DashMap<String, SubscriberInfo>>,
    /// 全局统计
    stats: Arc<BroadcastStats>,
    /// 待清理的慢订阅者列表
    pending_cleanup: Arc<DashMap<String, ()>>,
}

impl MarketDataBroadcaster {
    /// 创建新的广播器（使用默认配置）
    pub fn new() -> Self {
        Self::with_config(BroadcasterConfig::default())
    }

    /// 使用指定配置创建广播器
    pub fn with_config(config: BroadcasterConfig) -> Self {
        Self {
            config,
            subscribers: Arc::new(DashMap::new()),
            stats: Arc::new(BroadcastStats::default()),
            pending_cleanup: Arc::new(DashMap::new()),
        }
    }

    /// 订阅市场数据
    ///
    /// # 参数
    /// - `subscriber_id`: 订阅者ID（通常是session_id）
    /// - `instruments`: 订阅的合约列表
    /// - `channels`: 订阅的频道（orderbook, tick, etc.）
    ///
    /// # 返回
    /// 返回 (接收器, 统计信息句柄)
    pub fn subscribe(
        &self,
        subscriber_id: String,
        instruments: Vec<String>,
        channels: Vec<String>,
    ) -> Receiver<MarketDataEvent> {
        // 使用有界通道
        let (sender, receiver) = bounded(self.config.channel_capacity);

        let subscription = Subscription {
            instruments: instruments.clone(),
            channels: channels.clone(),
        };

        let stats = Arc::new(SubscriberStats::new());

        let info = SubscriberInfo {
            sender,
            subscription,
            stats: stats.clone(),
        };

        self.subscribers.insert(subscriber_id.clone(), info);

        log::info!(
            "Market data subscriber {} subscribed to instruments: {:?}, channels: {:?} (capacity: {})",
            subscriber_id,
            instruments,
            channels,
            self.config.channel_capacity
        );

        receiver
    }

    /// 订阅市场数据（带统计信息返回）
    pub fn subscribe_with_stats(
        &self,
        subscriber_id: String,
        instruments: Vec<String>,
        channels: Vec<String>,
    ) -> (Receiver<MarketDataEvent>, Arc<SubscriberStats>) {
        let (sender, receiver) = bounded(self.config.channel_capacity);

        let subscription = Subscription {
            instruments: instruments.clone(),
            channels: channels.clone(),
        };

        let stats = Arc::new(SubscriberStats::new());

        let info = SubscriberInfo {
            sender,
            subscription,
            stats: stats.clone(),
        };

        self.subscribers.insert(subscriber_id.clone(), info);

        log::info!(
            "Market data subscriber {} subscribed with stats tracking",
            subscriber_id
        );

        (receiver, stats)
    }

    /// 取消订阅
    pub fn unsubscribe(&self, subscriber_id: &str) {
        if let Some((_, info)) = self.subscribers.remove(subscriber_id) {
            let stats = info.stats;
            log::info!(
                "Market data subscriber {} unsubscribed (sent: {}, dropped: {})",
                subscriber_id,
                stats.sent_count.load(Ordering::Relaxed),
                stats.dropped_count.load(Ordering::Relaxed)
            );
        }
        self.pending_cleanup.remove(subscriber_id);
    }

    /// 更新订阅
    pub fn update_subscription(
        &self,
        subscriber_id: &str,
        instruments: Vec<String>,
        channels: Vec<String>,
    ) -> Result<(), ExchangeError> {
        if let Some(mut entry) = self.subscribers.get_mut(subscriber_id) {
            entry.subscription.instruments = instruments;
            entry.subscription.channels = channels;
            // 重置连续失败计数
            entry.stats.consecutive_failures.store(0, Ordering::Relaxed);
            Ok(())
        } else {
            Err(ExchangeError::InternalError(format!(
                "Subscriber not found: {}",
                subscriber_id
            )))
        }
    }

    /// 获取订阅者统计信息
    pub fn get_subscriber_stats(&self, subscriber_id: &str) -> Option<(u64, u64, u64)> {
        self.subscribers.get(subscriber_id).map(|entry| {
            let stats = &entry.stats;
            (
                stats.sent_count.load(Ordering::Relaxed),
                stats.dropped_count.load(Ordering::Relaxed),
                stats.consecutive_failures.load(Ordering::Relaxed),
            )
        })
    }

    /// 获取全局广播统计
    pub fn get_stats(&self) -> BroadcastStatsSummary {
        self.stats.get_summary()
    }

    /// 清理慢订阅者
    pub fn cleanup_slow_subscribers(&self) -> Vec<String> {
        let mut disconnected = Vec::new();

        // 收集要断开的订阅者
        for entry in self.pending_cleanup.iter() {
            disconnected.push(entry.key().clone());
        }

        // 断开订阅者
        for id in &disconnected {
            self.subscribers.remove(id);
            self.pending_cleanup.remove(id);
            self.stats.slow_disconnects.fetch_add(1, Ordering::Relaxed);
            log::warn!("Disconnected slow subscriber: {}", id);
        }

        disconnected
    }

    /// 广播市场数据事件
    ///
    /// 高性能广播实现：
    /// - 使用 try_send 避免阻塞
    /// - 记录发送/丢弃统计
    /// - 自动标记慢订阅者
    pub fn broadcast(&self, event: MarketDataEvent) {
        let start = Instant::now();

        let instrument_id = match &event {
            MarketDataEvent::OrderBookSnapshot { instrument_id, .. } => instrument_id,
            MarketDataEvent::OrderBookUpdate { instrument_id, .. } => instrument_id,
            MarketDataEvent::Tick { instrument_id, .. } => instrument_id,
            MarketDataEvent::LastPrice { instrument_id, .. } => instrument_id,
            MarketDataEvent::KLineFinished { instrument_id, .. } => instrument_id,
        };

        let channel = match &event {
            MarketDataEvent::OrderBookSnapshot { .. } | MarketDataEvent::OrderBookUpdate { .. } => {
                "orderbook"
            }
            MarketDataEvent::Tick { .. } => "tick",
            MarketDataEvent::LastPrice { .. } => "last_price",
            MarketDataEvent::KLineFinished { .. } => "kline",
        };

        let mut sent_count = 0u64;
        let mut dropped_count = 0u64;

        // 找到所有订阅了该合约和频道的订阅者
        for entry in self.subscribers.iter() {
            let subscriber_id = entry.key();
            let info = entry.value();

            // 检查是否订阅了该合约
            let subscribed_instrument = info.subscription.instruments.is_empty()
                || info
                    .subscription
                    .instruments
                    .iter()
                    .any(|id| id == instrument_id);

            // 检查是否订阅了该频道
            let subscribed_channel = info.subscription.channels.is_empty()
                || info.subscription.channels.iter().any(|ch| ch == channel);

            if subscribed_instrument && subscribed_channel {
                match info.sender.try_send(event.clone()) {
                    Ok(()) => {
                        info.stats.record_success();
                        sent_count += 1;
                    }
                    Err(TrySendError::Full(_)) => {
                        info.stats.record_failure();
                        dropped_count += 1;

                        // 检查是否为慢订阅者
                        if info.stats.is_slow(self.config.disconnect_threshold) {
                            self.pending_cleanup.insert(subscriber_id.clone(), ());
                            log::warn!(
                                "Subscriber {} marked for cleanup (consecutive failures: {})",
                                subscriber_id,
                                info.stats.consecutive_failures.load(Ordering::Relaxed)
                            );
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        // 接收端已断开，标记清理
                        self.pending_cleanup.insert(subscriber_id.clone(), ());
                    }
                }
            }
        }

        // 更新全局统计
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.stats.total_broadcasts.fetch_add(1, Ordering::Relaxed);
        self.stats.total_sent.fetch_add(sent_count, Ordering::Relaxed);
        self.stats.total_dropped.fetch_add(dropped_count, Ordering::Relaxed);
        self.stats.total_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// 批量广播市场数据事件（高性能模式）
    ///
    /// 使用 Rayon 并行发送到多个订阅者
    pub fn broadcast_batch(&self, events: Vec<MarketDataEvent>) {
        let start = Instant::now();
        let disconnect_threshold = self.config.disconnect_threshold;

        // 按合约分组事件
        let mut events_by_instrument: std::collections::HashMap<String, Vec<MarketDataEvent>> =
            std::collections::HashMap::new();

        for event in events {
            let instrument_id = match &event {
                MarketDataEvent::OrderBookSnapshot { instrument_id, .. } => instrument_id.clone(),
                MarketDataEvent::OrderBookUpdate { instrument_id, .. } => instrument_id.clone(),
                MarketDataEvent::Tick { instrument_id, .. } => instrument_id.clone(),
                MarketDataEvent::LastPrice { instrument_id, .. } => instrument_id.clone(),
                MarketDataEvent::KLineFinished { instrument_id, .. } => instrument_id.clone(),
            };
            events_by_instrument
                .entry(instrument_id)
                .or_default()
                .push(event);
        }

        // 收集所有订阅者信息快照（避免长时间持锁）
        let subscribers: Vec<_> = self
            .subscribers
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().sender.clone(),
                    entry.value().subscription.instruments.clone(),
                    entry.value().subscription.channels.clone(),
                    entry.value().stats.clone(),
                )
            })
            .collect();

        // 并行发送到每个订阅者
        let results: Vec<(u64, u64, Vec<String>)> = subscribers
            .par_iter()
            .map(|(subscriber_id, sender, instruments, channels, stats)| {
                let mut sent = 0u64;
                let mut dropped = 0u64;
                let mut to_cleanup = Vec::new();

                for (instrument_id, events_for_instrument) in &events_by_instrument {
                    // 检查订阅
                    let subscribed = instruments.is_empty()
                        || instruments.iter().any(|id| id == instrument_id);

                    if !subscribed {
                        continue;
                    }

                    for event in events_for_instrument {
                        let channel = match &event {
                            MarketDataEvent::OrderBookSnapshot { .. }
                            | MarketDataEvent::OrderBookUpdate { .. } => "orderbook",
                            MarketDataEvent::Tick { .. } => "tick",
                            MarketDataEvent::LastPrice { .. } => "last_price",
                            MarketDataEvent::KLineFinished { .. } => "kline",
                        };

                        let channel_match =
                            channels.is_empty() || channels.iter().any(|ch| ch == channel);

                        if !channel_match {
                            continue;
                        }

                        match sender.try_send(event.clone()) {
                            Ok(()) => {
                                stats.record_success();
                                sent += 1;
                            }
                            Err(TrySendError::Full(_)) => {
                                stats.record_failure();
                                dropped += 1;
                                if stats.is_slow(disconnect_threshold) {
                                    to_cleanup.push(subscriber_id.clone());
                                }
                            }
                            Err(TrySendError::Disconnected(_)) => {
                                to_cleanup.push(subscriber_id.clone());
                            }
                        }
                    }
                }

                (sent, dropped, to_cleanup)
            })
            .collect();

        // 汇总结果
        let mut total_sent = 0u64;
        let mut total_dropped = 0u64;
        for (sent, dropped, to_cleanup) in results {
            total_sent += sent;
            total_dropped += dropped;
            for id in to_cleanup {
                self.pending_cleanup.insert(id, ());
            }
        }

        // 更新统计
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.stats.total_broadcasts.fetch_add(1, Ordering::Relaxed);
        self.stats.total_sent.fetch_add(total_sent, Ordering::Relaxed);
        self.stats.total_dropped.fetch_add(total_dropped, Ordering::Relaxed);
        self.stats.total_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// 广播订单簿快照
    pub fn broadcast_orderbook_snapshot(
        &self,
        instrument_id: String,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
    ) {
        let event = MarketDataEvent::OrderBookSnapshot {
            instrument_id,
            bids,
            asks,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.broadcast(event);
    }

    /// 广播订单簿增量更新
    pub fn broadcast_orderbook_update(
        &self,
        instrument_id: String,
        side: String,
        price: f64,
        volume: f64,
    ) {
        let event = MarketDataEvent::OrderBookUpdate {
            instrument_id,
            side,
            price,
            volume,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.broadcast(event);
    }

    /// 广播成交Tick
    pub fn broadcast_tick(
        &self,
        instrument_id: String,
        price: f64,
        volume: f64,
        direction: String,
    ) {
        let event = MarketDataEvent::Tick {
            instrument_id,
            price,
            volume,
            direction,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.broadcast(event);
    }

    /// 广播最新价
    pub fn broadcast_last_price(&self, instrument_id: String, price: f64) {
        let event = MarketDataEvent::LastPrice {
            instrument_id,
            price,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.broadcast(event);
    }

    /// 获取订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// 获取订阅特定合约的订阅者数量
    pub fn instrument_subscriber_count(&self, instrument_id: &str) -> usize {
        self.subscribers
            .iter()
            .filter(|entry| {
                let subscription = &entry.value().subscription;
                subscription.instruments.is_empty()
                    || subscription
                        .instruments
                        .iter()
                        .any(|id| id == instrument_id)
            })
            .count()
    }

    /// 获取配置
    pub fn config(&self) -> &BroadcasterConfig {
        &self.config
    }

    /// 获取所有订阅者 ID
    pub fn get_subscriber_ids(&self) -> Vec<String> {
        self.subscribers.iter().map(|e| e.key().clone()).collect()
    }

    /// 获取待清理订阅者数量
    pub fn pending_cleanup_count(&self) -> usize {
        self.pending_cleanup.len()
    }
}

impl Default for MarketDataBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_and_broadcast() {
        let broadcaster = MarketDataBroadcaster::new();

        // 订阅
        let receiver = broadcaster.subscribe(
            "session1".to_string(),
            vec!["IX2301".to_string()],
            vec!["orderbook".to_string()],
        );

        // 广播订单簿快照
        broadcaster.broadcast_orderbook_snapshot(
            "IX2301".to_string(),
            vec![PriceLevel {
                price: 100.0,
                volume: 10,
            }],
            vec![PriceLevel {
                price: 101.0,
                volume: 5,
            }],
        );

        // 接收事件
        let event = receiver.try_recv().unwrap();
        match event {
            MarketDataEvent::OrderBookSnapshot { instrument_id, .. } => {
                assert_eq!(instrument_id, "IX2301");
            }
            _ => panic!("Expected OrderBookSnapshot"),
        }
    }

    #[test]
    fn test_unsubscribe() {
        let broadcaster = MarketDataBroadcaster::new();

        let _receiver = broadcaster.subscribe(
            "session1".to_string(),
            vec!["IX2301".to_string()],
            vec!["orderbook".to_string()],
        );

        assert_eq!(broadcaster.subscriber_count(), 1);

        broadcaster.unsubscribe("session1");

        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[test]
    fn test_broadcast_tick() {
        let broadcaster = MarketDataBroadcaster::new();

        let receiver = broadcaster.subscribe(
            "session1".to_string(),
            vec!["IX2301".to_string()],
            vec!["tick".to_string()],
        );

        broadcaster.broadcast_tick("IX2301".to_string(), 100.5, 10.0, "buy".to_string());

        let event = receiver.try_recv().unwrap();
        match event {
            MarketDataEvent::Tick { price, volume, .. } => {
                assert_eq!(price, 100.5);
                assert_eq!(volume, 10.0);
            }
            _ => panic!("Expected Tick"),
        }
    }

    #[test]
    fn test_bounded_channel_backpressure() {
        // 创建小容量通道测试背压
        let config = BroadcasterConfig {
            channel_capacity: 5,
            disconnect_threshold: 3,
            ..Default::default()
        };
        let broadcaster = MarketDataBroadcaster::with_config(config);

        let _receiver = broadcaster.subscribe(
            "slow_subscriber".to_string(),
            vec!["IX2301".to_string()],
            vec!["tick".to_string()],
        );

        // 发送超过容量的消息
        for i in 0..10 {
            broadcaster.broadcast_tick(
                "IX2301".to_string(),
                100.0 + i as f64,
                1.0,
                "buy".to_string(),
            );
        }

        // 检查统计
        let stats = broadcaster.get_stats();
        assert_eq!(stats.total_broadcasts, 10);
        assert!(stats.total_dropped > 0, "Should have dropped messages");
    }

    #[test]
    fn test_slow_subscriber_detection() {
        let config = BroadcasterConfig {
            channel_capacity: 2,
            disconnect_threshold: 3,
            ..Default::default()
        };
        let broadcaster = MarketDataBroadcaster::with_config(config);

        let _receiver = broadcaster.subscribe(
            "slow_subscriber".to_string(),
            vec!["IX2301".to_string()],
            vec!["tick".to_string()],
        );

        // 触发连续失败
        for _ in 0..10 {
            broadcaster.broadcast_tick("IX2301".to_string(), 100.0, 1.0, "buy".to_string());
        }

        // 检查是否被标记为待清理
        assert!(
            broadcaster.pending_cleanup_count() > 0,
            "Slow subscriber should be marked for cleanup"
        );

        // 清理慢订阅者
        let cleaned = broadcaster.cleanup_slow_subscribers();
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0], "slow_subscriber");
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[test]
    fn test_subscriber_stats_tracking() {
        let broadcaster = MarketDataBroadcaster::new();

        let (_receiver, stats) = broadcaster.subscribe_with_stats(
            "session1".to_string(),
            vec!["IX2301".to_string()],
            vec!["tick".to_string()],
        );

        // 发送消息
        for _ in 0..5 {
            broadcaster.broadcast_tick("IX2301".to_string(), 100.0, 1.0, "buy".to_string());
        }

        // 检查统计
        assert_eq!(stats.sent_count.load(Ordering::Relaxed), 5);
        assert_eq!(stats.dropped_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_batch_broadcast() {
        let broadcaster = MarketDataBroadcaster::new();

        let receiver = broadcaster.subscribe(
            "session1".to_string(),
            vec!["IX2301".to_string()],
            vec!["tick".to_string()],
        );

        // 批量广播
        let events: Vec<MarketDataEvent> = (0..10)
            .map(|i| MarketDataEvent::Tick {
                instrument_id: "IX2301".to_string(),
                price: 100.0 + i as f64,
                volume: 1.0,
                direction: "buy".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
            })
            .collect();

        broadcaster.broadcast_batch(events);

        // 接收所有事件
        let mut received = 0;
        while receiver.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, 10);
    }

    #[test]
    fn test_stats_summary() {
        let broadcaster = MarketDataBroadcaster::new();

        let _receiver = broadcaster.subscribe(
            "session1".to_string(),
            vec![],
            vec![],
        );

        // 发送消息
        for _ in 0..100 {
            broadcaster.broadcast_tick("IX2301".to_string(), 100.0, 1.0, "buy".to_string());
        }

        let stats = broadcaster.get_stats();
        assert_eq!(stats.total_broadcasts, 100);
        assert_eq!(stats.total_sent, 100);
        assert_eq!(stats.drop_rate(), 0.0);
        assert!(stats.avg_broadcast_latency_us() >= 0.0);
    }
}
