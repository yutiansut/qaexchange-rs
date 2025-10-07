//! K线数据聚合模块
//!
//! 从 Tick 数据实时聚合成各种周期的 K线数据
//!
//! @yutiansut @quantaxis

use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use serde::{Serialize, Deserialize};

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    /// 时间戳（K线开始时间，毫秒）
    pub timestamp: i64,

    /// 开盘价
    pub open: f64,

    /// 最高价
    pub high: f64,

    /// 最低价
    pub low: f64,

    /// 收盘价
    pub close: f64,

    /// 成交量
    pub volume: i64,

    /// 成交额
    pub amount: f64,

    /// K线是否完成（false=当前K线仍在形成中）
    pub is_finished: bool,
}

impl KLine {
    /// 创建新K线
    pub fn new(timestamp: i64, price: f64) -> Self {
        Self {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0,
            amount: 0.0,
            is_finished: false,
        }
    }

    /// 更新K线数据（用新的tick更新）
    pub fn update(&mut self, price: f64, volume: i64) {
        self.close = price;
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.volume += volume;
        self.amount += price * volume as f64;
    }

    /// 标记K线完成
    pub fn finish(&mut self) {
        self.is_finished = true;
    }
}

/// K线周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KLinePeriod {
    /// 3秒
    Sec3 = 3,

    /// 1分钟
    Min1 = 60,

    /// 5分钟
    Min5 = 300,

    /// 15分钟
    Min15 = 900,

    /// 30分钟
    Min30 = 1800,

    /// 60分钟
    Min60 = 3600,

    /// 日线
    Day = 86400,
}

impl KLinePeriod {
    /// 从整数转换
    pub fn from_int(value: i32) -> Option<Self> {
        match value {
            0 => Some(KLinePeriod::Day),
            3 => Some(KLinePeriod::Sec3),
            4 => Some(KLinePeriod::Min1),
            5 => Some(KLinePeriod::Min5),
            6 => Some(KLinePeriod::Min15),
            7 => Some(KLinePeriod::Min30),
            8 => Some(KLinePeriod::Min60),
            _ => None,
        }
    }

    /// 转换为整数（HQChart格式）
    pub fn to_int(&self) -> i32 {
        match self {
            KLinePeriod::Day => 0,
            KLinePeriod::Sec3 => 3,
            KLinePeriod::Min1 => 4,
            KLinePeriod::Min5 => 5,
            KLinePeriod::Min15 => 6,
            KLinePeriod::Min30 => 7,
            KLinePeriod::Min60 => 8,
        }
    }

    /// 从DIFF协议的duration(纳秒)转换
    pub fn from_duration_ns(duration_ns: i64) -> Option<Self> {
        match duration_ns {
            3_000_000_000 => Some(KLinePeriod::Sec3),     // 3秒
            60_000_000_000 => Some(KLinePeriod::Min1),    // 1分钟
            300_000_000_000 => Some(KLinePeriod::Min5),   // 5分钟
            900_000_000_000 => Some(KLinePeriod::Min15),  // 15分钟
            1_800_000_000_000 => Some(KLinePeriod::Min30), // 30分钟
            3_600_000_000_000 => Some(KLinePeriod::Min60), // 60分钟
            86_400_000_000_000 => Some(KLinePeriod::Day),  // 日线
            _ => None,
        }
    }

    /// 转换为DIFF协议的duration(纳秒)
    pub fn to_duration_ns(&self) -> i64 {
        match self {
            KLinePeriod::Sec3 => 3_000_000_000,
            KLinePeriod::Min1 => 60_000_000_000,
            KLinePeriod::Min5 => 300_000_000_000,
            KLinePeriod::Min15 => 900_000_000_000,
            KLinePeriod::Min30 => 1_800_000_000_000,
            KLinePeriod::Min60 => 3_600_000_000_000,
            KLinePeriod::Day => 86_400_000_000_000,
        }
    }

    /// 获取周期秒数
    pub fn seconds(&self) -> i64 {
        *self as i64
    }

    /// 计算K线周期的起始时间戳
    pub fn align_timestamp(&self, timestamp_ms: i64) -> i64 {
        let ts_sec = timestamp_ms / 1000;
        let period_sec = self.seconds();

        match self {
            KLinePeriod::Day => {
                // 日线：按UTC 0点对齐
                (ts_sec / 86400) * 86400 * 1000
            }
            _ => {
                // 分钟线：按周期对齐
                (ts_sec / period_sec) * period_sec * 1000
            }
        }
    }
}

/// K线聚合器（单个合约）
pub struct KLineAggregator {
    /// 合约代码
    instrument_id: String,

    /// 各周期的当前K线
    current_klines: HashMap<KLinePeriod, KLine>,

    /// 各周期的历史K线（最多保留1000根）
    history_klines: HashMap<KLinePeriod, Vec<KLine>>,

    /// 最大历史K线数量
    max_history: usize,
}

impl KLineAggregator {
    /// 创建新的K线聚合器
    pub fn new(instrument_id: String) -> Self {
        Self {
            instrument_id,
            current_klines: HashMap::new(),
            history_klines: HashMap::new(),
            max_history: 1000,
        }
    }

    /// 处理新的Tick数据
    pub fn on_tick(&mut self, price: f64, volume: i64, timestamp_ms: i64) -> Vec<(KLinePeriod, KLine)> {
        let mut finished_klines = Vec::new();

        // 所有周期（分级采样：3s → 1min → 5min → 15min → 30min → 60min → Day）
        let periods = vec![
            KLinePeriod::Sec3,
            KLinePeriod::Min1,
            KLinePeriod::Min5,
            KLinePeriod::Min15,
            KLinePeriod::Min30,
            KLinePeriod::Min60,
            KLinePeriod::Day,
        ];

        for period in periods {
            let period_start = period.align_timestamp(timestamp_ms);

            // 检查是否需要开始新K线
            let need_new_kline = if let Some(current) = self.current_klines.get(&period) {
                current.timestamp != period_start
            } else {
                true
            };

            if need_new_kline {
                // 完成旧K线
                if let Some(mut old_kline) = self.current_klines.remove(&period) {
                    old_kline.finish();
                    finished_klines.push((period, old_kline.clone()));

                    // 加入历史
                    let history = self.history_klines.entry(period).or_insert_with(Vec::new);
                    history.push(old_kline);

                    // 限制历史数量
                    if history.len() > self.max_history {
                        history.remove(0);
                    }
                }

                // 创建新K线
                self.current_klines.insert(period, KLine::new(period_start, price));
            }

            // 更新当前K线
            if let Some(kline) = self.current_klines.get_mut(&period) {
                kline.update(price, volume);
            }
        }

        finished_klines
    }

    /// 获取当前K线（未完成）
    pub fn get_current_kline(&self, period: KLinePeriod) -> Option<&KLine> {
        self.current_klines.get(&period)
    }

    /// 获取历史K线
    pub fn get_history_klines(&self, period: KLinePeriod, count: usize) -> Vec<KLine> {
        if let Some(history) = self.history_klines.get(&period) {
            let start = if history.len() > count { history.len() - count } else { 0 };
            history[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    /// 获取最近N根K线（包括当前未完成的）
    pub fn get_recent_klines(&self, period: KLinePeriod, count: usize) -> Vec<KLine> {
        let mut klines = self.get_history_klines(period, count);

        // 添加当前K线
        if let Some(current) = self.get_current_kline(period) {
            klines.push(current.clone());
        }

        klines
    }
}

/// K线管理器（所有合约）
pub struct KLineManager {
    /// 各合约的K线聚合器
    aggregators: Arc<RwLock<HashMap<String, KLineAggregator>>>,
}

impl KLineManager {
    /// 创建新的K线管理器
    pub fn new() -> Self {
        Self {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 处理Tick数据
    pub fn on_tick(&self, instrument_id: &str, price: f64, volume: i64, timestamp_ms: i64) -> Vec<(KLinePeriod, KLine)> {
        let mut aggregators = self.aggregators.write();

        let aggregator = aggregators
            .entry(instrument_id.to_string())
            .or_insert_with(|| KLineAggregator::new(instrument_id.to_string()));

        aggregator.on_tick(price, volume, timestamp_ms)
    }

    /// 获取历史K线
    pub fn get_klines(&self, instrument_id: &str, period: KLinePeriod, count: usize) -> Vec<KLine> {
        let aggregators = self.aggregators.read();

        if let Some(aggregator) = aggregators.get(instrument_id) {
            aggregator.get_recent_klines(period, count)
        } else {
            Vec::new()
        }
    }

    /// 获取当前K线
    pub fn get_current_kline(&self, instrument_id: &str, period: KLinePeriod) -> Option<KLine> {
        let aggregators = self.aggregators.read();

        aggregators.get(instrument_id)
            .and_then(|agg| agg.get_current_kline(period))
            .cloned()
    }
}

impl Default for KLineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kline_period_align() {
        let period = KLinePeriod::Min5;

        // 2025-10-07 14:03:25 -> 应该对齐到 14:00:00
        let ts = 1696684405000; // 毫秒
        let aligned = period.align_timestamp(ts);

        let expected = (1696684405000 / 1000 / 300) * 300 * 1000;
        assert_eq!(aligned, expected);
    }

    #[test]
    fn test_kline_aggregator() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        let now = chrono::Utc::now().timestamp_millis();

        // 第一个tick
        let finished = agg.on_tick(3800.0, 10, now);
        assert_eq!(finished.len(), 0); // 第一个tick不会完成任何K线

        // 同一分钟内的tick
        let finished = agg.on_tick(3810.0, 5, now + 10000);
        assert_eq!(finished.len(), 0);

        // 检查当前K线
        let current = agg.get_current_kline(KLinePeriod::Min1).unwrap();
        assert_eq!(current.open, 3800.0);
        assert_eq!(current.close, 3810.0);
        assert_eq!(current.high, 3810.0);
        assert_eq!(current.low, 3800.0);
        assert_eq!(current.volume, 15);
        assert!(!current.is_finished);
    }

    #[test]
    fn test_kline_manager() {
        let manager = KLineManager::new();

        let now = chrono::Utc::now().timestamp_millis();

        manager.on_tick("IF2501", 3800.0, 10, now);
        manager.on_tick("IF2501", 3810.0, 5, now + 10000);

        let klines = manager.get_klines("IF2501", KLinePeriod::Min1, 10);
        assert_eq!(klines.len(), 1); // 只有当前未完成的K线

        let current = manager.get_current_kline("IF2501", KLinePeriod::Min1);
        assert!(current.is_some());
        assert_eq!(current.unwrap().volume, 15);
    }
}
