//! K线数据聚合模块
//!
//! 从 Tick 数据实时聚合成各种周期的 K线数据
//!
//! @yutiansut @quantaxis

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

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

    /// 起始持仓量（DIFF协议要求）
    pub open_oi: i64,

    /// 结束持仓量（DIFF协议要求）
    pub close_oi: i64,

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
            open_oi: 0, // 持仓量初始化为0（需要从行情数据获取）
            close_oi: 0,
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

    /// 更新持仓量
    pub fn update_open_interest(&mut self, open_interest: i64) {
        if self.open_oi == 0 {
            self.open_oi = open_interest; // 第一次tick设置起始持仓
        }
        self.close_oi = open_interest; // 每次更新结束持仓
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
            3_000_000_000 => Some(KLinePeriod::Sec3),      // 3秒
            60_000_000_000 => Some(KLinePeriod::Min1),     // 1分钟
            300_000_000_000 => Some(KLinePeriod::Min5),    // 5分钟
            900_000_000_000 => Some(KLinePeriod::Min15),   // 15分钟
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
/// @yutiansut @quantaxis
pub struct KLineAggregator {
    /// 合约代码
    instrument_id: String,

    /// 各周期的当前K线
    current_klines: HashMap<KLinePeriod, KLine>,

    /// 各周期的历史K线（最多保留1000根）
    pub(crate) history_klines: HashMap<KLinePeriod, Vec<KLine>>,

    /// 最大历史K线数量
    pub(crate) max_history: usize,

    /// 最新价格（用于定时器驱动的K线生成，无成交时使用）
    /// @yutiansut @quantaxis
    pub(crate) last_price: Option<f64>,

    /// 各周期最后一次处理的时间戳（用于检测跨周期）
    last_period_timestamps: HashMap<KLinePeriod, i64>,
}

impl KLineAggregator {
    /// 创建新的K线聚合器
    pub fn new(instrument_id: String) -> Self {
        Self {
            instrument_id,
            current_klines: HashMap::new(),
            history_klines: HashMap::new(),
            max_history: 1000,
            last_price: None,
            last_period_timestamps: HashMap::new(),
        }
    }

    /// 处理新的Tick数据
    /// @yutiansut @quantaxis
    pub fn on_tick(
        &mut self,
        price: f64,
        volume: i64,
        timestamp_ms: i64,
    ) -> Vec<(KLinePeriod, KLine)> {
        let mut finished_klines = Vec::new();

        // 更新最新价格
        self.last_price = Some(price);

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

            // 更新最后处理的周期时间戳
            self.last_period_timestamps.insert(period, period_start);

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
                    let history = self.history_klines.entry(period).or_default();
                    history.push(old_kline);

                    // 限制历史数量
                    if history.len() > self.max_history {
                        history.remove(0);
                    }
                }

                // 创建新K线
                self.current_klines
                    .insert(period, KLine::new(period_start, price));
            }

            // 更新当前K线
            if let Some(kline) = self.current_klines.get_mut(&period) {
                kline.update(price, volume);
            }
        }

        finished_klines
    }

    /// 定时器驱动的K线完成检查
    /// @yutiansut @quantaxis
    ///
    /// 在没有交易发生时，仍然按时间周期生成K线
    /// OHLC = 上一根K线的收盘价（或最新价格）
    ///
    /// # Arguments
    /// * `current_timestamp_ms` - 当前时间戳（毫秒）
    ///
    /// # Returns
    /// 完成的K线列表 (period, kline)
    pub fn on_timer(&mut self, current_timestamp_ms: i64) -> Vec<(KLinePeriod, KLine)> {
        let mut finished_klines = Vec::new();

        // 如果没有最新价格，无法生成K线
        let last_price = match self.last_price {
            Some(price) => price,
            None => return finished_klines,
        };

        // 所有周期
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
            let current_period_start = period.align_timestamp(current_timestamp_ms);
            let period_ms = period.seconds() * 1000;

            // 获取上次处理的时间戳
            let last_period_ts = self.last_period_timestamps.get(&period).copied();

            // 检查当前K线是否已过期（时间戳不是当前周期）
            if let Some(current_kline) = self.current_klines.get(&period) {
                if current_kline.timestamp != current_period_start {
                    let old_ts = current_kline.timestamp;

                    // 当前K线已过期，需要完成它
                    if let Some(mut old_kline) = self.current_klines.remove(&period) {
                        old_kline.finish();
                        finished_klines.push((period, old_kline.clone()));

                        // 加入历史
                        let history = self.history_klines.entry(period).or_default();
                        history.push(old_kline);

                        // 限制历史数量
                        if history.len() > self.max_history {
                            history.remove(0);
                        }
                    }

                    // 填补中间跳过的周期（多个周期无交易的情况）
                    // @yutiansut @quantaxis
                    let mut gap_ts = old_ts + period_ms;
                    let mut gap_count = 0;
                    while gap_ts < current_period_start && gap_count < 100 {
                        // 创建空K线（OHLC = last_price, volume = 0）
                        let mut gap_kline = KLine::new(gap_ts, last_price);
                        gap_kline.finish();
                        finished_klines.push((period, gap_kline.clone()));

                        // 加入历史
                        let history = self.history_klines.entry(period).or_default();
                        history.push(gap_kline);

                        // 限制历史数量
                        if history.len() > self.max_history {
                            history.remove(0);
                        }

                        gap_ts += period_ms;
                        gap_count += 1;
                    }

                    if gap_count > 0 {
                        log::debug!(
                            "📊 [KLineAggregator] Filled {} gap K-lines for {} {:?}",
                            gap_count, self.instrument_id, period
                        );
                    }

                    // 创建新K线（无交易时使用最新价格）
                    self.current_klines
                        .insert(period, KLine::new(current_period_start, last_price));
                }
            } else {
                // 没有当前K线，创建一个新的
                self.current_klines
                    .insert(period, KLine::new(current_period_start, last_price));
            }

            // 更新最后处理的周期时间戳
            self.last_period_timestamps.insert(period, current_period_start);
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
            let start = if history.len() > count {
                history.len() - count
            } else {
                0
            };
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
    pub fn on_tick(
        &self,
        instrument_id: &str,
        price: f64,
        volume: i64,
        timestamp_ms: i64,
    ) -> Vec<(KLinePeriod, KLine)> {
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

        aggregators
            .get(instrument_id)
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

        // 同一分钟内的tick（10秒后，会完成3秒K线但不会完成分钟线）
        let finished = agg.on_tick(3810.0, 5, now + 10000);
        // 10秒内会完成3个3秒K线（0-3s, 3-6s, 6-9s）
        assert!(finished.len() >= 1, "应该至少完成1个3秒K线");
        // 检查没有Min1 K线完成
        assert!(
            !finished.iter().any(|(p, _)| *p == KLinePeriod::Min1),
            "不应完成分钟K线"
        );

        // 检查当前Min1 K线
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

    #[test]
    fn test_kline_finish() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        // 对齐到分钟边界
        let base_time = (chrono::Utc::now().timestamp_millis() / 60000) * 60000;

        // 第一分钟的tick
        agg.on_tick(3800.0, 10, base_time + 1000);
        agg.on_tick(3810.0, 5, base_time + 30000);

        // 跨到下一分钟 - 应该完成第一根K线
        let finished = agg.on_tick(3820.0, 8, base_time + 61000);

        // 至少会完成3s周期的K线
        assert!(finished.len() > 0, "Should finish at least one K-line");

        // 检查是否有1分钟K线完成
        let min1_finished = finished
            .iter()
            .find(|(period, _)| *period == KLinePeriod::Min1);
        assert!(min1_finished.is_some(), "Should finish 1-minute K-line");

        let (_, kline) = min1_finished.unwrap();
        assert_eq!(kline.open, 3800.0);
        assert_eq!(kline.close, 3810.0);
        assert_eq!(kline.high, 3810.0);
        assert_eq!(kline.low, 3800.0);
        assert_eq!(kline.volume, 15);
        assert!(kline.is_finished);
    }

    #[test]
    fn test_multiple_periods() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        let base_time = (chrono::Utc::now().timestamp_millis() / 300000) * 300000; // 对齐到5分钟

        // 填充5分钟的数据
        for i in 0..5 {
            let tick_time = base_time + i * 60000 + 1000; // 每分钟一个tick
            agg.on_tick(3800.0 + i as f64, 10, tick_time);
        }

        // 跨到下一个5分钟 - 应该完成多个周期的K线
        let finished = agg.on_tick(3900.0, 10, base_time + 301000);

        // 应该完成3s, 1min, 5min周期的K线
        assert!(finished.len() >= 3, "Should finish multiple periods");

        // 验证有5分钟K线
        let min5_finished = finished
            .iter()
            .find(|(period, _)| *period == KLinePeriod::Min5);
        assert!(min5_finished.is_some(), "Should finish 5-minute K-line");
    }

    #[test]
    fn test_open_interest_update() {
        let mut kline = KLine::new(1000000, 3800.0);

        // 第一次更新持仓量
        kline.update_open_interest(1000);
        assert_eq!(kline.open_oi, 1000);
        assert_eq!(kline.close_oi, 1000);

        // 第二次更新持仓量
        kline.update_open_interest(1050);
        assert_eq!(kline.open_oi, 1000); // 起始持仓不变
        assert_eq!(kline.close_oi, 1050); // 结束持仓更新
    }

    #[test]
    fn test_period_conversion() {
        // 测试HQChart格式转换
        assert_eq!(KLinePeriod::Day.to_int(), 0);
        assert_eq!(KLinePeriod::Sec3.to_int(), 3);
        assert_eq!(KLinePeriod::Min1.to_int(), 4);
        assert_eq!(KLinePeriod::Min5.to_int(), 5);

        assert_eq!(KLinePeriod::from_int(0), Some(KLinePeriod::Day));
        assert_eq!(KLinePeriod::from_int(4), Some(KLinePeriod::Min1));

        // 测试DIFF协议纳秒转换
        assert_eq!(KLinePeriod::Sec3.to_duration_ns(), 3_000_000_000);
        assert_eq!(KLinePeriod::Min1.to_duration_ns(), 60_000_000_000);

        assert_eq!(
            KLinePeriod::from_duration_ns(60_000_000_000),
            Some(KLinePeriod::Min1)
        );
    }

    #[test]
    fn test_history_limit() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        let base_time = (chrono::Utc::now().timestamp_millis() / 60000) * 60000;

        // 生成1005根K线（超过max_history=1000）
        for i in 0..1005 {
            let tick_time = base_time + i * 60000;
            agg.on_tick(3800.0, 10, tick_time);
        }

        // 检查历史K线数量
        let history = agg.get_history_klines(KLinePeriod::Min1, 10000);
        assert!(
            history.len() <= 1000,
            "History should be limited to max_history (1000), got {}",
            history.len()
        );
    }

    /// 测试定时器驱动的K线生成
    /// @yutiansut @quantaxis
    /// 即使没有交易，也要按时间周期生成K线
    #[test]
    fn test_timer_driven_kline_generation() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        // 对齐到分钟边界
        let base_time = (chrono::Utc::now().timestamp_millis() / 60000) * 60000;

        // 第一个tick，初始化last_price
        agg.on_tick(3800.0, 10, base_time + 1000);

        // 检查last_price已设置
        assert_eq!(agg.last_price, Some(3800.0));

        // 第二个tick，更新价格
        agg.on_tick(3850.0, 5, base_time + 30000);
        assert_eq!(agg.last_price, Some(3850.0));

        // 模拟时间流逝到下一分钟，但没有新的tick
        // 调用on_timer应该完成当前K线并创建新的
        let next_minute = base_time + 60000 + 1000;
        let finished = agg.on_timer(next_minute);

        // 应该完成至少1分钟K线
        let min1_finished = finished
            .iter()
            .find(|(p, _)| *p == KLinePeriod::Min1);
        assert!(min1_finished.is_some(), "Should finish 1-minute K-line via timer");

        let (_, kline) = min1_finished.unwrap();
        assert_eq!(kline.open, 3800.0);
        assert_eq!(kline.close, 3850.0);
        assert_eq!(kline.high, 3850.0);
        assert_eq!(kline.low, 3800.0);
        assert_eq!(kline.volume, 15);
        assert!(kline.is_finished);

        // 检查新的当前K线已创建（使用last_price）
        let current = agg.get_current_kline(KLinePeriod::Min1);
        assert!(current.is_some());
        let current_kline = current.unwrap();
        // 新K线的OHLC应该是last_price
        assert_eq!(current_kline.open, 3850.0);
        assert_eq!(current_kline.volume, 0); // 无交易
        assert!(!current_kline.is_finished);
    }

    /// 测试定时器填补多个跳过的周期
    /// @yutiansut @quantaxis
    #[test]
    fn test_timer_fills_gap_periods() {
        let mut agg = KLineAggregator::new("IF2501".to_string());

        // 对齐到分钟边界
        let base_time = (chrono::Utc::now().timestamp_millis() / 60000) * 60000;

        // 第一个tick
        agg.on_tick(3800.0, 10, base_time + 1000);

        // 模拟跳过3分钟（没有任何tick）
        let skip_time = base_time + 4 * 60000 + 1000; // 跳过3分钟
        let finished = agg.on_timer(skip_time);

        // 应该填补了多个K线
        let min1_count = finished
            .iter()
            .filter(|(p, _)| *p == KLinePeriod::Min1)
            .count();

        // 应该有多个分钟K线被填补
        assert!(
            min1_count >= 3,
            "Should fill at least 3 gap K-lines, got {}",
            min1_count
        );

        // 验证历史K线
        let history = agg.get_history_klines(KLinePeriod::Min1, 10);
        assert!(
            history.len() >= 3,
            "History should have at least 3 K-lines, got {}",
            history.len()
        );

        // 验证填补的K线OHLC都是last_price
        for kline in &history[1..] {
            // 跳过第一个有交易的K线
            assert_eq!(kline.open, 3800.0);
            assert_eq!(kline.high, 3800.0);
            assert_eq!(kline.low, 3800.0);
            assert_eq!(kline.close, 3800.0);
            assert_eq!(kline.volume, 0);
        }
    }
}
