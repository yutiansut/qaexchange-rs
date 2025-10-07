//! 市场数据广播系统
//!
//! 负责将订单簿变化、成交数据广播给所有订阅者

use std::sync::Arc;
use dashmap::DashMap;
use crossbeam::channel::{Sender, Receiver, unbounded};
use serde::{Serialize, Deserialize};
use crate::ExchangeError;
use super::PriceLevel;

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
        side: String,  // "bid" or "ask"
        price: f64,
        volume: f64,  // 0 表示删除该价格档位
        timestamp: i64,
    },

    /// Tick 成交数据
    Tick {
        instrument_id: String,
        price: f64,
        volume: f64,
        direction: String,  // "buy" or "sell"
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
        period: i32,  // HQChart周期格式 (0=日线, 4=1分钟, 5=5分钟等)
        kline: super::kline::KLine,
        timestamp: i64,
    },
}

/// 订阅信息
#[derive(Debug, Clone)]
struct Subscription {
    /// 订阅的合约列表
    instruments: Vec<String>,
    /// 订阅的频道（orderbook, tick, etc.）
    channels: Vec<String>,
}

/// 市场数据广播器
pub struct MarketDataBroadcaster {
    /// 订阅者映射 (subscriber_id -> Sender<MarketDataEvent>)
    subscribers: Arc<DashMap<String, (Sender<MarketDataEvent>, Subscription)>>,
}

impl MarketDataBroadcaster {
    /// 创建新的广播器
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(DashMap::new()),
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
    /// 返回接收器，订阅者通过该接收器接收市场数据
    pub fn subscribe(
        &self,
        subscriber_id: String,
        instruments: Vec<String>,
        channels: Vec<String>,
    ) -> Receiver<MarketDataEvent> {
        let (sender, receiver) = unbounded();

        let subscription = Subscription {
            instruments: instruments.clone(),
            channels: channels.clone(),
        };

        self.subscribers.insert(subscriber_id.clone(), (sender, subscription));

        log::info!(
            "Market data subscriber {} subscribed to instruments: {:?}, channels: {:?}",
            subscriber_id,
            instruments,
            channels
        );

        receiver
    }

    /// 取消订阅
    pub fn unsubscribe(&self, subscriber_id: &str) {
        self.subscribers.remove(subscriber_id);
        log::info!("Market data subscriber {} unsubscribed", subscriber_id);
    }

    /// 更新订阅
    pub fn update_subscription(
        &self,
        subscriber_id: &str,
        instruments: Vec<String>,
        channels: Vec<String>,
    ) -> Result<(), ExchangeError> {
        if let Some(mut entry) = self.subscribers.get_mut(subscriber_id) {
            entry.1.instruments = instruments;
            entry.1.channels = channels;
            Ok(())
        } else {
            Err(ExchangeError::InternalError(format!(
                "Subscriber not found: {}",
                subscriber_id
            )))
        }
    }

    /// 广播市场数据事件
    pub fn broadcast(&self, event: MarketDataEvent) {
        let instrument_id = match &event {
            MarketDataEvent::OrderBookSnapshot { instrument_id, .. } => instrument_id,
            MarketDataEvent::OrderBookUpdate { instrument_id, .. } => instrument_id,
            MarketDataEvent::Tick { instrument_id, .. } => instrument_id,
            MarketDataEvent::LastPrice { instrument_id, .. } => instrument_id,
            MarketDataEvent::KLineFinished { instrument_id, .. } => instrument_id,
        };

        let channel = match &event {
            MarketDataEvent::OrderBookSnapshot { .. } | MarketDataEvent::OrderBookUpdate { .. } => "orderbook",
            MarketDataEvent::Tick { .. } => "tick",
            MarketDataEvent::LastPrice { .. } => "last_price",
            MarketDataEvent::KLineFinished { .. } => "kline",
        };

        // 找到所有订阅了该合约和频道的订阅者
        for entry in self.subscribers.iter() {
            let (subscriber_id, (sender, subscription)) = entry.pair();

            // 检查是否订阅了该合约
            let subscribed_instrument = subscription.instruments.is_empty()
                || subscription.instruments.iter().any(|id| id == instrument_id);

            // 检查是否订阅了该频道
            let subscribed_channel = subscription.channels.is_empty()
                || subscription.channels.iter().any(|ch| ch == channel);

            if subscribed_instrument && subscribed_channel {
                if let Err(e) = sender.try_send(event.clone()) {
                    log::warn!(
                        "Failed to send market data to subscriber {}: {}",
                        subscriber_id,
                        e
                    );
                }
            }
        }
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
                let subscription = &entry.value().1;
                subscription.instruments.is_empty()
                    || subscription.instruments.iter().any(|id| id == instrument_id)
            })
            .count()
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
            vec![PriceLevel { price: 100.0, volume: 10 }],
            vec![PriceLevel { price: 101.0, volume: 5 }],
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

        let receiver = broadcaster.subscribe(
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

        broadcaster.broadcast_tick(
            "IX2301".to_string(),
            100.5,
            10.0,
            "buy".to_string(),
        );

        let event = receiver.try_recv().unwrap();
        match event {
            MarketDataEvent::Tick { price, volume, .. } => {
                assert_eq!(price, 100.5);
                assert_eq!(volume, 10.0);
            }
            _ => panic!("Expected Tick"),
        }
    }
}
