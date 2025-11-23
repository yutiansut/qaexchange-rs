//! 滚动窗口算子 - Rolling Mean, Std, Corr, EMA, RSI
//!
//! @yutiansut @quantaxis
//!
//! 提供金融量化常用的滚动窗口统计算子：
//! - RollingMean: 移动平均
//! - RollingStd: 移动标准差
//! - RollingCorr: 滚动相关系数
//! - EMA: 指数移动平均
//! - RSI: 相对强弱指数

use super::ring_buffer::{NumericRingBuffer, PairedRingBuffer, RingBuffer};
use super::welford::{WelfordCovarianceState, WelfordState, WindowedWelfordState};

// ═══════════════════════════════════════════════════════════════════════════
// RollingMean - 移动平均
// ═══════════════════════════════════════════════════════════════════════════

/// 滚动均值算子
#[derive(Debug, Clone)]
pub struct RollingMean {
    buffer: NumericRingBuffer,
}

impl RollingMean {
    pub fn new(window_size: usize) -> Self {
        Self {
            buffer: NumericRingBuffer::new(window_size),
        }
    }

    pub fn update(&mut self, value: f64) {
        self.buffer.push(value);
    }

    pub fn value(&self) -> f64 {
        self.buffer.mean()
    }

    pub fn sum(&self) -> f64 {
        self.buffer.sum()
    }

    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for RollingMean {
    fn default() -> Self {
        Self::new(20)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RollingStd - 移动标准差 (基于 Welford)
// ═══════════════════════════════════════════════════════════════════════════

/// 滚动标准差算子 (使用 Welford 算法保证数值稳定性)
#[derive(Debug, Clone)]
pub struct RollingStd {
    welford: WindowedWelfordState,
}

impl RollingStd {
    pub fn new(window_size: usize) -> Self {
        Self {
            welford: WindowedWelfordState::new(window_size),
        }
    }

    pub fn update(&mut self, value: f64) {
        self.welford.update(value);
    }

    /// 总体标准差
    pub fn value(&self) -> f64 {
        self.welford.std()
    }

    /// 总体方差
    pub fn variance(&self) -> f64 {
        self.welford.variance()
    }

    /// 均值
    pub fn mean(&self) -> f64 {
        self.welford.mean()
    }

    /// 偏度
    pub fn skewness(&self) -> f64 {
        self.welford.skewness()
    }

    /// 峰度
    pub fn kurtosis(&self) -> f64 {
        self.welford.kurtosis()
    }

    pub fn count(&self) -> usize {
        self.welford.count()
    }

    pub fn is_full(&self) -> bool {
        self.welford.is_full()
    }

    pub fn reset(&mut self) {
        self.welford.reset();
    }
}

impl Default for RollingStd {
    fn default() -> Self {
        Self::new(20)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RollingCorr - 滚动相关系数
// ═══════════════════════════════════════════════════════════════════════════

/// 滚动相关系数算子
#[derive(Debug, Clone)]
pub struct RollingCorr {
    buffer: PairedRingBuffer,
}

impl RollingCorr {
    pub fn new(window_size: usize) -> Self {
        Self {
            buffer: PairedRingBuffer::new(window_size),
        }
    }

    pub fn update(&mut self, x: f64, y: f64) {
        self.buffer.push(x, y);
    }

    /// 相关系数
    pub fn value(&self) -> f64 {
        self.buffer.correlation()
    }

    /// 协方差
    pub fn covariance(&self) -> f64 {
        self.buffer.covariance()
    }

    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for RollingCorr {
    fn default() -> Self {
        Self::new(20)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EMA - 指数移动平均
// ═══════════════════════════════════════════════════════════════════════════

/// 指数移动平均算子
///
/// EMA_t = α * price_t + (1 - α) * EMA_{t-1}
/// 其中 α = 2 / (period + 1)
#[derive(Debug, Clone)]
pub struct EMA {
    alpha: f64,
    ema: Option<f64>,
    count: u64,
}

impl EMA {
    pub fn new(period: usize) -> Self {
        let alpha = 2.0 / (period as f64 + 1.0);
        Self {
            alpha,
            ema: None,
            count: 0,
        }
    }

    /// 自定义平滑因子
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            ema: None,
            count: 0,
        }
    }

    pub fn update(&mut self, value: f64) {
        self.count += 1;
        self.ema = Some(match self.ema {
            None => value,
            Some(prev) => self.alpha * value + (1.0 - self.alpha) * prev,
        });
    }

    pub fn value(&self) -> Option<f64> {
        self.ema
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn reset(&mut self) {
        self.ema = None;
        self.count = 0;
    }
}

impl Default for EMA {
    fn default() -> Self {
        Self::new(20)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DEMA - 双指数移动平均
// ═══════════════════════════════════════════════════════════════════════════

/// 双指数移动平均
/// DEMA = 2 * EMA - EMA(EMA)
#[derive(Debug, Clone)]
pub struct DEMA {
    ema1: EMA,
    ema2: EMA,
}

impl DEMA {
    pub fn new(period: usize) -> Self {
        Self {
            ema1: EMA::new(period),
            ema2: EMA::new(period),
        }
    }

    pub fn update(&mut self, value: f64) {
        self.ema1.update(value);
        if let Some(ema1_val) = self.ema1.value() {
            self.ema2.update(ema1_val);
        }
    }

    pub fn value(&self) -> Option<f64> {
        match (self.ema1.value(), self.ema2.value()) {
            (Some(e1), Some(e2)) => Some(2.0 * e1 - e2),
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RSI - 相对强弱指数
// ═══════════════════════════════════════════════════════════════════════════

/// RSI 相对强弱指数
///
/// RSI = 100 - 100 / (1 + RS)
/// RS = AvgGain / AvgLoss
#[derive(Debug, Clone)]
pub struct RSI {
    period: usize,
    prev_price: Option<f64>,
    avg_gain: f64,
    avg_loss: f64,
    count: u64,
}

impl RSI {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prev_price: None,
            avg_gain: 0.0,
            avg_loss: 0.0,
            count: 0,
        }
    }

    pub fn update(&mut self, price: f64) {
        if let Some(prev) = self.prev_price {
            let change = price - prev;
            let gain = if change > 0.0 { change } else { 0.0 };
            let loss = if change < 0.0 { -change } else { 0.0 };

            self.count += 1;
            let n = self.period as f64;

            if self.count <= self.period as u64 {
                // 初始化阶段：简单平均
                self.avg_gain = (self.avg_gain * (self.count - 1) as f64 + gain) / self.count as f64;
                self.avg_loss = (self.avg_loss * (self.count - 1) as f64 + loss) / self.count as f64;
            } else {
                // 使用 Wilder 平滑
                self.avg_gain = (self.avg_gain * (n - 1.0) + gain) / n;
                self.avg_loss = (self.avg_loss * (n - 1.0) + loss) / n;
            }
        }
        self.prev_price = Some(price);
    }

    pub fn value(&self) -> Option<f64> {
        if self.count < self.period as u64 {
            return None;
        }

        if self.avg_loss == 0.0 {
            return Some(100.0); // 全涨
        }

        let rs = self.avg_gain / self.avg_loss;
        Some(100.0 - 100.0 / (1.0 + rs))
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn reset(&mut self) {
        self.prev_price = None;
        self.avg_gain = 0.0;
        self.avg_loss = 0.0;
        self.count = 0;
    }
}

impl Default for RSI {
    fn default() -> Self {
        Self::new(14)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MACD - 指数平滑异同移动平均线
// ═══════════════════════════════════════════════════════════════════════════

/// MACD 输出值
#[derive(Debug, Clone, Default)]
pub struct MACDValue {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
}

/// MACD 指标
#[derive(Debug, Clone)]
pub struct MACD {
    fast_ema: EMA,
    slow_ema: EMA,
    signal_ema: EMA,
}

impl MACD {
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_ema: EMA::new(fast_period),
            slow_ema: EMA::new(slow_period),
            signal_ema: EMA::new(signal_period),
        }
    }

    pub fn update(&mut self, price: f64) {
        self.fast_ema.update(price);
        self.slow_ema.update(price);

        if let (Some(fast), Some(slow)) = (self.fast_ema.value(), self.slow_ema.value()) {
            let macd = fast - slow;
            self.signal_ema.update(macd);
        }
    }

    pub fn value(&self) -> Option<MACDValue> {
        let fast = self.fast_ema.value()?;
        let slow = self.slow_ema.value()?;
        let signal = self.signal_ema.value()?;

        let macd = fast - slow;
        let histogram = macd - signal;

        Some(MACDValue {
            macd,
            signal,
            histogram,
        })
    }

    pub fn reset(&mut self) {
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.signal_ema.reset();
    }
}

impl Default for MACD {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bollinger Bands - 布林带
// ═══════════════════════════════════════════════════════════════════════════

/// 布林带输出值
#[derive(Debug, Clone, Default)]
pub struct BollingerBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub bandwidth: f64,
    pub percent_b: f64,
}

/// 布林带指标
#[derive(Debug, Clone)]
pub struct BollingerBands {
    rolling_std: RollingStd,
    num_std: f64,
}

impl BollingerBands {
    pub fn new(period: usize, num_std: f64) -> Self {
        Self {
            rolling_std: RollingStd::new(period),
            num_std,
        }
    }

    pub fn update(&mut self, price: f64) {
        self.rolling_std.update(price);
    }

    pub fn value(&self, current_price: f64) -> Option<BollingerBandsValue> {
        if !self.rolling_std.is_full() {
            return None;
        }

        let middle = self.rolling_std.mean();
        let std = self.rolling_std.value();
        let upper = middle + self.num_std * std;
        let lower = middle - self.num_std * std;

        let bandwidth = if middle != 0.0 {
            (upper - lower) / middle * 100.0
        } else {
            0.0
        };

        let percent_b = if upper != lower {
            (current_price - lower) / (upper - lower)
        } else {
            0.5
        };

        Some(BollingerBandsValue {
            upper,
            middle,
            lower,
            bandwidth,
            percent_b,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.rolling_std.is_full()
    }

    pub fn reset(&mut self) {
        self.rolling_std.reset();
    }
}

impl Default for BollingerBands {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ATR - 平均真实范围
// ═══════════════════════════════════════════════════════════════════════════

/// ATR 平均真实范围
#[derive(Debug, Clone)]
pub struct ATR {
    period: usize,
    prev_close: Option<f64>,
    atr: Option<f64>,
    count: u64,
}

impl ATR {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prev_close: None,
            atr: None,
            count: 0,
        }
    }

    /// 更新 ATR (需要 high, low, close)
    pub fn update(&mut self, high: f64, low: f64, close: f64) {
        let tr = match self.prev_close {
            None => high - low,
            Some(prev) => {
                let hl = high - low;
                let hc = (high - prev).abs();
                let lc = (low - prev).abs();
                hl.max(hc).max(lc)
            }
        };

        self.count += 1;
        let n = self.period as f64;

        self.atr = Some(match self.atr {
            None => tr,
            Some(prev_atr) => {
                if self.count <= self.period as u64 {
                    // 初始化：简单平均
                    (prev_atr * (self.count - 1) as f64 + tr) / self.count as f64
                } else {
                    // Wilder 平滑
                    (prev_atr * (n - 1.0) + tr) / n
                }
            }
        });

        self.prev_close = Some(close);
    }

    pub fn value(&self) -> Option<f64> {
        if self.count >= self.period as u64 {
            self.atr
        } else {
            None
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn reset(&mut self) {
        self.prev_close = None;
        self.atr = None;
        self.count = 0;
    }
}

impl Default for ATR {
    fn default() -> Self {
        Self::new(14)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_mean() {
        let mut rm = RollingMean::new(3);

        rm.update(1.0);
        rm.update(2.0);
        rm.update(3.0);
        assert!((rm.value() - 2.0).abs() < 1e-10);

        rm.update(4.0);
        assert!((rm.value() - 3.0).abs() < 1e-10); // (2+3+4)/3
    }

    #[test]
    fn test_rolling_std() {
        let mut rs = RollingStd::new(5);

        let data = [2.0, 4.0, 4.0, 4.0, 5.0];
        for &x in &data {
            rs.update(x);
        }

        // 均值 = 3.8, 方差 = 1.04, 标准差 ≈ 1.0198
        assert!((rs.mean() - 3.8).abs() < 1e-10);
        assert!((rs.variance() - 1.04).abs() < 0.01);
    }

    #[test]
    fn test_rolling_corr() {
        let mut rc = RollingCorr::new(5);

        // 完美正相关
        for i in 1..=5 {
            rc.update(i as f64, i as f64 * 2.0);
        }

        assert!((rc.value() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ema() {
        let mut ema = EMA::new(10);

        for i in 1..=20 {
            ema.update(i as f64);
        }

        // EMA 应该接近但略低于最新值
        let val = ema.value().unwrap();
        assert!(val > 15.0 && val < 20.0);
    }

    #[test]
    fn test_rsi() {
        let mut rsi = RSI::new(14);

        // 模拟上涨趋势
        let prices = [
            44.0, 44.5, 44.2, 44.8, 45.2, 45.0, 45.5, 45.8, 46.0, 45.5, 45.8, 46.2, 46.5, 46.8,
            47.0,
        ];

        for &p in &prices {
            rsi.update(p);
        }

        let val = rsi.value().unwrap();
        // 上涨趋势 RSI 应该 > 50
        assert!(val > 50.0, "RSI should be > 50 in uptrend, got {}", val);
    }

    #[test]
    fn test_macd() {
        let mut macd = MACD::new(12, 26, 9);

        // 模拟价格序列
        for i in 1..=50 {
            macd.update(100.0 + (i as f64).sin() * 5.0);
        }

        let val = macd.value();
        assert!(val.is_some());
    }

    #[test]
    fn test_bollinger_bands() {
        let mut bb = BollingerBands::new(20, 2.0);

        // 填充数据
        for i in 1..=25 {
            bb.update(100.0 + (i as f64 % 5.0));
        }

        let current_price = 103.0;
        let val = bb.value(current_price);
        assert!(val.is_some());

        let bands = val.unwrap();
        assert!(bands.upper > bands.middle);
        assert!(bands.middle > bands.lower);
    }

    #[test]
    fn test_atr() {
        let mut atr = ATR::new(14);

        // 模拟 OHLC 数据
        let data = [
            (101.0, 99.0, 100.0),
            (102.0, 100.0, 101.0),
            (103.0, 100.0, 102.0),
            (104.0, 101.0, 103.0),
            (105.0, 102.0, 104.0),
            (106.0, 103.0, 105.0),
            (107.0, 104.0, 106.0),
            (108.0, 105.0, 107.0),
            (109.0, 106.0, 108.0),
            (110.0, 107.0, 109.0),
            (111.0, 108.0, 110.0),
            (112.0, 109.0, 111.0),
            (113.0, 110.0, 112.0),
            (114.0, 111.0, 113.0),
            (115.0, 112.0, 114.0),
        ];

        for &(h, l, c) in &data {
            atr.update(h, l, c);
        }

        let val = atr.value();
        assert!(val.is_some());
        // ATR 应该约等于 true range 的平均值
        assert!(val.unwrap() > 0.0);
    }
}
