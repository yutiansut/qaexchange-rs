//! 成交记录器
//!
//! 记录所有撮合成交记录，供查询和统计使用

use crate::core::Trade;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 成交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: String,
    pub instrument_id: String,
    pub buy_user_id: String,
    pub sell_user_id: String,
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub trading_day: String,
}

/// 成交记录器
pub struct TradeRecorder {
    /// 所有成交记录 (trade_id -> TradeRecord)
    trades: DashMap<String, TradeRecord>,

    /// 按合约索引 (instrument_id -> Vec<trade_id>)
    by_instrument: DashMap<String, Arc<RwLock<Vec<String>>>>,

    /// 按账户索引 (user_id -> Vec<trade_id>)
    by_user: DashMap<String, Arc<RwLock<Vec<String>>>>,

    /// 成交序号生成器
    sequence: Arc<RwLock<u64>>,
}

impl TradeRecorder {
    pub fn new() -> Self {
        Self {
            trades: DashMap::new(),
            by_instrument: DashMap::new(),
            by_user: DashMap::new(),
            sequence: Arc::new(RwLock::new(1)),
        }
    }

    /// 记录成交
    pub fn record_trade(
        &self,
        instrument_id: String,
        buy_user_id: String,
        sell_user_id: String,
        buy_order_id: String,
        sell_order_id: String,
        price: f64,
        volume: f64,
        trading_day: String,
    ) -> String {
        let trade_id = self.generate_trade_id();
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let record = TradeRecord {
            trade_id: trade_id.clone(),
            instrument_id: instrument_id.clone(),
            buy_user_id: buy_user_id.clone(),
            sell_user_id: sell_user_id.clone(),
            buy_order_id,
            sell_order_id,
            price,
            volume,
            timestamp,
            trading_day,
        };

        // 存储成交记录
        self.trades.insert(trade_id.clone(), record);

        // 更新合约索引
        self.by_instrument
            .entry(instrument_id)
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(trade_id.clone());

        // 更新买方账户索引
        self.by_user
            .entry(buy_user_id)
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(trade_id.clone());

        // 更新卖方账户索引
        self.by_user
            .entry(sell_user_id)
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(trade_id.clone());

        trade_id
    }

    /// 查询成交记录
    pub fn get_trade(&self, trade_id: &str) -> Option<TradeRecord> {
        self.trades.get(trade_id).map(|r| r.value().clone())
    }

    /// 查询合约的所有成交
    pub fn get_trades_by_instrument(&self, instrument_id: &str) -> Vec<TradeRecord> {
        if let Some(trade_ids) = self.by_instrument.get(instrument_id) {
            trade_ids.read()
                .iter()
                .filter_map(|id| self.get_trade(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 查询账户的所有成交
    pub fn get_trades_by_user(&self, user_id: &str) -> Vec<TradeRecord> {
        if let Some(trade_ids) = self.by_user.get(user_id) {
            trade_ids.read()
                .iter()
                .filter_map(|id| self.get_trade(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取成交统计
    pub fn get_trade_stats(&self, instrument_id: &str) -> TradeStats {
        let trades = self.get_trades_by_instrument(instrument_id);

        if trades.is_empty() {
            return TradeStats::default();
        }

        let total_volume: f64 = trades.iter().map(|t| t.volume).sum();
        let total_amount: f64 = trades.iter().map(|t| t.price * t.volume).sum();
        let avg_price = if total_volume > 0.0 { total_amount / total_volume } else { 0.0 };

        let highest_price = trades.iter().map(|t| t.price).fold(f64::NEG_INFINITY, f64::max);
        let lowest_price = trades.iter().map(|t| t.price).fold(f64::INFINITY, f64::min);

        TradeStats {
            trade_count: trades.len(),
            total_volume,
            total_amount,
            avg_price,
            highest_price,
            lowest_price,
        }
    }

    /// 生成成交ID
    fn generate_trade_id(&self) -> String {
        let mut seq = self.sequence.write();
        let id = format!("T{:016}", *seq);
        *seq += 1;
        id
    }

    /// 清空所有记录
    pub fn clear(&self) {
        self.trades.clear();
        self.by_instrument.clear();
        self.by_user.clear();
        *self.sequence.write() = 1;
    }
}

impl Default for TradeRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// 成交统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeStats {
    pub trade_count: usize,
    pub total_volume: f64,
    pub total_amount: f64,
    pub avg_price: f64,
    pub highest_price: f64,
    pub lowest_price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_recorder() {
        let recorder = TradeRecorder::new();

        let trade_id = recorder.record_trade(
            "TEST2301".to_string(),
            "user1".to_string(),
            "user2".to_string(),
            "order1".to_string(),
            "order2".to_string(),
            100.0,
            10.0,
            "2025-10-03".to_string(),
        );

        assert!(!trade_id.is_empty());

        let trade = recorder.get_trade(&trade_id);
        assert!(trade.is_some());

        let trade = trade.unwrap();
        assert_eq!(trade.price, 100.0);
        assert_eq!(trade.volume, 10.0);
    }

    #[test]
    fn test_trade_query_by_instrument() {
        let recorder = TradeRecorder::new();

        recorder.record_trade(
            "TEST2301".to_string(),
            "user1".to_string(),
            "user2".to_string(),
            "order1".to_string(),
            "order2".to_string(),
            100.0,
            10.0,
            "2025-10-03".to_string(),
        );

        recorder.record_trade(
            "TEST2301".to_string(),
            "user1".to_string(),
            "user3".to_string(),
            "order3".to_string(),
            "order4".to_string(),
            101.0,
            20.0,
            "2025-10-03".to_string(),
        );

        let trades = recorder.get_trades_by_instrument("TEST2301");
        assert_eq!(trades.len(), 2);
    }

    #[test]
    fn test_trade_stats() {
        let recorder = TradeRecorder::new();

        recorder.record_trade(
            "TEST2301".to_string(),
            "user1".to_string(),
            "user2".to_string(),
            "order1".to_string(),
            "order2".to_string(),
            100.0,
            10.0,
            "2025-10-03".to_string(),
        );

        recorder.record_trade(
            "TEST2301".to_string(),
            "user1".to_string(),
            "user3".to_string(),
            "order3".to_string(),
            "order4".to_string(),
            110.0,
            20.0,
            "2025-10-03".to_string(),
        );

        let stats = recorder.get_trade_stats("TEST2301");
        assert_eq!(stats.trade_count, 2);
        assert_eq!(stats.total_volume, 30.0);
        assert_eq!(stats.highest_price, 110.0);
        assert_eq!(stats.lowest_price, 100.0);
    }
}
