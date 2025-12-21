//! 物化视图状态管理
//!
//! @yutiansut @quantaxis
//!
//! 提供因子物化视图的管理功能：
//! - 因子快照管理
//! - 增量更新触发
//! - 状态持久化接口
//! - 多合约因子缓存

use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::operators::rolling::*;
use super::operators::welford::*;

// ═══════════════════════════════════════════════════════════════════════════
// 因子值类型
// ═══════════════════════════════════════════════════════════════════════════

/// 因子值枚举
#[derive(Debug, Clone)]
pub enum FactorValue {
    Scalar(f64),
    Optional(Option<f64>),
    Vector(Vec<f64>),
    MACD(MACDValue),
    Bollinger(BollingerBandsValue),
}

impl FactorValue {
    pub fn as_scalar(&self) -> Option<f64> {
        match self {
            FactorValue::Scalar(v) => Some(*v),
            FactorValue::Optional(v) => *v,
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 因子状态容器
// ═══════════════════════════════════════════════════════════════════════════

/// 单合约因子状态
#[derive(Debug, Clone)]
pub struct InstrumentFactorState {
    /// 滚动均值 (多周期)
    pub rolling_means: HashMap<usize, RollingMean>,
    /// 滚动标准差 (多周期)
    pub rolling_stds: HashMap<usize, RollingStd>,
    /// EMA (多周期)
    pub emas: HashMap<usize, EMA>,
    /// RSI (多周期)
    pub rsis: HashMap<usize, RSI>,
    /// MACD
    pub macd: Option<MACD>,
    /// 布林带
    pub bollinger: Option<BollingerBands>,
    /// ATR
    pub atr: Option<ATR>,
    /// 滚动相关系数 (与其他合约)
    pub correlations: HashMap<String, RollingCorr>,
    /// 自定义因子状态
    pub custom: HashMap<String, FactorValue>,
    /// 最后更新时间
    pub last_update: Instant,
    /// 更新计数
    pub update_count: u64,
}

impl InstrumentFactorState {
    pub fn new() -> Self {
        Self {
            rolling_means: HashMap::new(),
            rolling_stds: HashMap::new(),
            emas: HashMap::new(),
            rsis: HashMap::new(),
            macd: None,
            bollinger: None,
            atr: None,
            correlations: HashMap::new(),
            custom: HashMap::new(),
            last_update: Instant::now(),
            update_count: 0,
        }
    }

    /// 初始化默认因子
    pub fn with_defaults() -> Self {
        let mut state = Self::new();

        // 常用周期的滚动均值
        for period in [5, 10, 20, 60, 120] {
            state.rolling_means.insert(period, RollingMean::new(period));
            state.rolling_stds.insert(period, RollingStd::new(period));
            state.emas.insert(period, EMA::new(period));
        }

        // RSI
        state.rsis.insert(14, RSI::new(14));

        // MACD
        state.macd = Some(MACD::default());

        // 布林带
        state.bollinger = Some(BollingerBands::default());

        // ATR
        state.atr = Some(ATR::default());

        state
    }

    /// 更新价格因子
    pub fn update_price(&mut self, price: f64) {
        // 更新所有滚动均值
        for rm in self.rolling_means.values_mut() {
            rm.update(price);
        }

        // 更新所有滚动标准差
        for rs in self.rolling_stds.values_mut() {
            rs.update(price);
        }

        // 更新所有 EMA
        for ema in self.emas.values_mut() {
            ema.update(price);
        }

        // 更新所有 RSI
        for rsi in self.rsis.values_mut() {
            rsi.update(price);
        }

        // 更新 MACD
        if let Some(macd) = &mut self.macd {
            macd.update(price);
        }

        // 更新布林带
        if let Some(bb) = &mut self.bollinger {
            bb.update(price);
        }

        self.last_update = Instant::now();
        self.update_count += 1;
    }

    /// 更新 OHLC 数据 (用于 ATR 等)
    pub fn update_ohlc(&mut self, high: f64, low: f64, close: f64) {
        if let Some(atr) = &mut self.atr {
            atr.update(high, low, close);
        }
    }

    /// 获取因子快照
    pub fn snapshot(&self, current_price: f64) -> FactorSnapshot {
        let mut values = HashMap::new();

        // 滚动均值
        for (&period, rm) in &self.rolling_means {
            values.insert(format!("ma_{}", period), FactorValue::Scalar(rm.value()));
        }

        // 滚动标准差
        for (&period, rs) in &self.rolling_stds {
            values.insert(format!("std_{}", period), FactorValue::Scalar(rs.value()));
        }

        // EMA
        for (&period, ema) in &self.emas {
            values.insert(
                format!("ema_{}", period),
                FactorValue::Optional(ema.value()),
            );
        }

        // RSI
        for (&period, rsi) in &self.rsis {
            values.insert(
                format!("rsi_{}", period),
                FactorValue::Optional(rsi.value()),
            );
        }

        // MACD
        if let Some(macd) = &self.macd {
            if let Some(v) = macd.value() {
                values.insert("macd".to_string(), FactorValue::MACD(v));
            }
        }

        // 布林带
        if let Some(bb) = &self.bollinger {
            if let Some(v) = bb.value(current_price) {
                values.insert("bollinger".to_string(), FactorValue::Bollinger(v));
            }
        }

        // ATR
        if let Some(atr) = &self.atr {
            values.insert("atr".to_string(), FactorValue::Optional(atr.value()));
        }

        // 相关系数
        for (key, corr) in &self.correlations {
            values.insert(format!("corr_{}", key), FactorValue::Scalar(corr.value()));
        }

        // 自定义因子
        values.extend(self.custom.clone());

        FactorSnapshot {
            values,
            timestamp: self.last_update,
            update_count: self.update_count,
        }
    }
}

impl Default for InstrumentFactorState {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 因子快照
// ═══════════════════════════════════════════════════════════════════════════

/// 因子快照
#[derive(Debug, Clone)]
pub struct FactorSnapshot {
    pub values: HashMap<String, FactorValue>,
    pub timestamp: Instant,
    pub update_count: u64,
}

impl FactorSnapshot {
    pub fn get(&self, name: &str) -> Option<&FactorValue> {
        self.values.get(name)
    }

    pub fn get_scalar(&self, name: &str) -> Option<f64> {
        self.values.get(name).and_then(|v| v.as_scalar())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 物化视图管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 物化视图管理器
///
/// 管理所有合约的因子状态
pub struct MaterializedViewManager {
    /// 合约因子状态映射
    states: DashMap<String, InstrumentFactorState>,
    /// 全局相关系数计算器
    global_correlations: DashMap<(String, String), RollingCorr>,
    /// 配置
    config: ViewConfig,
}

/// 视图配置
#[derive(Debug, Clone)]
pub struct ViewConfig {
    /// 默认窗口大小
    pub default_window: usize,
    /// 相关系数窗口大小
    pub correlation_window: usize,
    /// 是否自动初始化因子
    pub auto_init: bool,
    /// 因子过期时间
    pub ttl: Duration,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            default_window: 20,
            correlation_window: 60,
            auto_init: true,
            ttl: Duration::from_secs(3600),
        }
    }
}

impl MaterializedViewManager {
    pub fn new(config: ViewConfig) -> Self {
        Self {
            states: DashMap::new(),
            global_correlations: DashMap::new(),
            config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ViewConfig::default())
    }

    /// 获取或创建合约因子状态
    pub fn get_or_create(&self, instrument_id: &str) -> dashmap::mapref::one::RefMut<'_, String, InstrumentFactorState> {
        self.states
            .entry(instrument_id.to_string())
            .or_insert_with(|| {
                if self.config.auto_init {
                    InstrumentFactorState::with_defaults()
                } else {
                    InstrumentFactorState::new()
                }
            })
    }

    /// 更新合约价格
    pub fn update_price(&self, instrument_id: &str, price: f64) {
        let mut state = self.get_or_create(instrument_id);
        state.update_price(price);
    }

    /// 更新 OHLC 数据
    pub fn update_ohlc(&self, instrument_id: &str, high: f64, low: f64, close: f64) {
        let mut state = self.get_or_create(instrument_id);
        state.update_ohlc(high, low, close);
    }

    /// 更新两个合约的相关系数
    pub fn update_correlation(&self, inst1: &str, inst2: &str, price1: f64, price2: f64) {
        let key = if inst1 < inst2 {
            (inst1.to_string(), inst2.to_string())
        } else {
            (inst2.to_string(), inst1.to_string())
        };

        self.global_correlations
            .entry(key)
            .or_insert_with(|| RollingCorr::new(self.config.correlation_window))
            .update(price1, price2);
    }

    /// 获取因子快照
    pub fn snapshot(&self, instrument_id: &str, current_price: f64) -> Option<FactorSnapshot> {
        self.states
            .get(instrument_id)
            .map(|state| state.snapshot(current_price))
    }

    /// 获取相关系数
    pub fn get_correlation(&self, inst1: &str, inst2: &str) -> Option<f64> {
        let key = if inst1 < inst2 {
            (inst1.to_string(), inst2.to_string())
        } else {
            (inst2.to_string(), inst1.to_string())
        };

        self.global_correlations.get(&key).map(|c| c.value().value())
    }

    /// 获取所有合约 ID
    pub fn instrument_ids(&self) -> Vec<String> {
        self.states.iter().map(|r| r.key().clone()).collect()
    }

    /// 清理过期状态
    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        self.states.retain(|_, state| {
            now.duration_since(state.last_update) < self.config.ttl
        });
    }

    /// 重置指定合约状态
    pub fn reset(&self, instrument_id: &str) {
        self.states.remove(instrument_id);
    }

    /// 重置所有状态
    pub fn reset_all(&self) {
        self.states.clear();
        self.global_correlations.clear();
    }

    /// 合约数量
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 因子订阅器
// ═══════════════════════════════════════════════════════════════════════════

/// 因子更新通知
#[derive(Debug, Clone)]
pub struct FactorUpdate {
    pub instrument_id: String,
    pub factor_name: String,
    pub value: FactorValue,
    pub timestamp: Instant,
}

/// 因子订阅回调类型
pub type FactorCallback = Box<dyn Fn(FactorUpdate) + Send + Sync>;

/// 因子订阅管理器
pub struct FactorSubscriptionManager {
    subscriptions: DashMap<String, Vec<Arc<FactorCallback>>>,
}

impl FactorSubscriptionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
        }
    }

    /// 订阅因子更新
    pub fn subscribe(&self, instrument_id: &str, callback: FactorCallback) {
        self.subscriptions
            .entry(instrument_id.to_string())
            .or_default()
            .push(Arc::new(callback));
    }

    /// 通知因子更新
    pub fn notify(&self, update: FactorUpdate) {
        if let Some(callbacks) = self.subscriptions.get(&update.instrument_id) {
            for callback in callbacks.iter() {
                callback(update.clone());
            }
        }
    }

    /// 取消订阅
    pub fn unsubscribe(&self, instrument_id: &str) {
        self.subscriptions.remove(instrument_id);
    }
}

impl Default for FactorSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_factor_state() {
        let mut state = InstrumentFactorState::with_defaults();

        // 模拟价格序列
        for i in 1..=100 {
            let price = 100.0 + (i as f64).sin() * 5.0;
            state.update_price(price);
        }

        let snapshot = state.snapshot(105.0);

        // 验证因子存在
        assert!(snapshot.get("ma_20").is_some());
        assert!(snapshot.get("std_20").is_some());
        assert!(snapshot.get("ema_20").is_some());
        assert!(snapshot.get("rsi_14").is_some());
        assert!(snapshot.get("macd").is_some());
        assert!(snapshot.get("bollinger").is_some());
    }

    #[test]
    fn test_materialized_view_manager() {
        let manager = MaterializedViewManager::with_defaults();

        // 更新多个合约
        for i in 1..=50 {
            manager.update_price("cu2501", 80000.0 + i as f64 * 10.0);
            manager.update_price("au2501", 900.0 + i as f64);
        }

        // 获取快照
        let cu_snapshot = manager.snapshot("cu2501", 80500.0);
        let au_snapshot = manager.snapshot("au2501", 950.0);

        assert!(cu_snapshot.is_some());
        assert!(au_snapshot.is_some());

        // 验证 MA20
        let cu_ma20 = cu_snapshot.unwrap().get_scalar("ma_20");
        assert!(cu_ma20.is_some());
    }

    #[test]
    fn test_correlation() {
        let manager = MaterializedViewManager::with_defaults();

        // 模拟正相关数据
        for i in 1..=100 {
            let x = i as f64;
            let y = x * 2.0 + 10.0;
            manager.update_correlation("A", "B", x, y);
        }

        let corr = manager.get_correlation("A", "B");
        assert!(corr.is_some());
        assert!((corr.unwrap() - 1.0).abs() < 0.01);
    }
}
