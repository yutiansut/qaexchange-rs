//! 交易所撮合引擎
//!
//! 基于 qars::Orderbook 的封装，添加成交记录和行情推送功能

use crate::core::Order;
use crate::matching::trade_recorder::TradeRecorder;
use crate::matching::{Failed, OrderProcessingResult, Orderbook, Success, TradingState};
use crate::ExchangeError;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;

/// 合约资产类型（使用字符串的哈希值作为 Copy 类型）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstrumentAsset(u64);

impl InstrumentAsset {
    pub fn from_code(code: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        code.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// 交易所撮合引擎
pub struct ExchangeMatchingEngine {
    /// 合约代码 -> 订单簿映射
    orderbooks: DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>,

    /// 成交记录器
    trade_recorder: Arc<TradeRecorder>,

    /// 合约昨收盘价
    prev_close_map: DashMap<String, f64>,

    /// 当前交易日
    trading_day: Arc<RwLock<String>>,
}

impl ExchangeMatchingEngine {
    pub fn new() -> Self {
        Self {
            orderbooks: DashMap::new(),
            trade_recorder: Arc::new(TradeRecorder::new()),
            prev_close_map: DashMap::new(),
            trading_day: Arc::new(RwLock::new(String::new())),
        }
    }

    /// 注册新合约
    pub fn register_instrument(
        &self,
        instrument_id: String,
        prev_close: f64,
    ) -> Result<(), ExchangeError> {
        let asset = InstrumentAsset::from_code(&instrument_id);
        let orderbook = Orderbook::new(asset, prev_close);
        self.orderbooks
            .insert(instrument_id.clone(), Arc::new(RwLock::new(orderbook)));
        self.prev_close_map
            .insert(instrument_id.clone(), prev_close);
        log::info!(
            "Registered instrument: {} with prev_close: {}",
            instrument_id,
            prev_close
        );
        Ok(())
    }

    /// 获取订单簿
    pub fn get_orderbook(
        &self,
        instrument_id: &str,
    ) -> Option<Arc<RwLock<Orderbook<InstrumentAsset>>>> {
        self.orderbooks
            .get(instrument_id)
            .map(|r| r.value().clone())
    }

    /// 获取所有合约列表
    pub fn get_instruments(&self) -> Vec<String> {
        self.orderbooks.iter().map(|r| r.key().clone()).collect()
    }

    /// 设置交易日
    pub fn set_trading_day(&self, trading_day: String) {
        *self.trading_day.write() = trading_day;
    }

    /// 获取交易日
    pub fn get_trading_day(&self) -> String {
        self.trading_day.read().clone()
    }

    /// 获取成交记录器
    pub fn get_trade_recorder(&self) -> Arc<TradeRecorder> {
        self.trade_recorder.clone()
    }

    /// 获取合约的昨收盘价
    pub fn get_prev_close(&self, instrument_id: &str) -> Option<f64> {
        self.prev_close_map
            .get(instrument_id)
            .map(|entry| *entry.value())
    }

    /// 设置交易状态 (TODO: 需要 qars Orderbook 支持此方法)
    pub fn set_trading_state(
        &self,
        instrument_id: &str,
        _state: TradingState,
    ) -> Result<(), ExchangeError> {
        if self.get_orderbook(instrument_id).is_some() {
            // TODO: 实现 set_trading_state
            // let mut ob = orderbook.write();
            // ob.set_trading_state(state);
            log::info!(
                "Set trading state for {}: {:?} (NOT IMPLEMENTED)",
                instrument_id,
                _state
            );
            Ok(())
        } else {
            Err(ExchangeError::MatchingError(format!(
                "Instrument not found: {}",
                instrument_id
            )))
        }
    }

    /// 获取最新价格
    pub fn get_last_price(&self, instrument_id: &str) -> Option<f64> {
        self.get_orderbook(instrument_id)
            .map(|ob| ob.read().lastprice)
    }
}

impl Default for ExchangeMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_instrument() {
        let engine = ExchangeMatchingEngine::new();
        let result = engine.register_instrument("TEST2301".to_string(), 100.0);
        assert!(result.is_ok());

        let instruments = engine.get_instruments();
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0], "TEST2301");
    }

    #[test]
    fn test_get_orderbook() {
        let engine = ExchangeMatchingEngine::new();
        engine
            .register_instrument("TEST2301".to_string(), 100.0)
            .unwrap();

        let orderbook = engine.get_orderbook("TEST2301");
        assert!(orderbook.is_some());

        let last_price = engine.get_last_price("TEST2301");
        // Note: Orderbook initializes lastprice to 0.0, not prev_close
        assert!(last_price.is_some());
    }

    // ==================== InstrumentAsset 测试 @yutiansut @quantaxis ====================

    /// 测试 InstrumentAsset::from_code 哈希生成
    /// 相同代码应产生相同哈希值
    #[test]
    fn test_instrument_asset_from_code_same() {
        let asset1 = InstrumentAsset::from_code("cu2501");
        let asset2 = InstrumentAsset::from_code("cu2501");

        assert_eq!(asset1, asset2, "相同代码应产生相同的 InstrumentAsset");
    }

    /// 测试不同代码产生不同哈希
    #[test]
    fn test_instrument_asset_from_code_different() {
        let asset1 = InstrumentAsset::from_code("cu2501");
        let asset2 = InstrumentAsset::from_code("au2512");

        assert_ne!(asset1, asset2, "不同代码应产生不同的 InstrumentAsset");
    }

    /// 测试空字符串处理
    #[test]
    fn test_instrument_asset_empty_code() {
        let asset = InstrumentAsset::from_code("");
        // 空字符串也应产生有效的哈希
        assert_ne!(asset.0, 0, "空字符串哈希不应为0");
    }

    /// 测试 InstrumentAsset 的 Copy 和 Clone trait
    #[test]
    fn test_instrument_asset_copy_clone() {
        let asset1 = InstrumentAsset::from_code("IF2512");
        let asset2 = asset1; // Copy
        let asset3 = asset1.clone(); // Clone

        assert_eq!(asset1, asset2);
        assert_eq!(asset1, asset3);
    }

    /// 测试 InstrumentAsset 作为 HashMap 键
    #[test]
    fn test_instrument_asset_as_hash_key() {
        use std::collections::HashMap;

        let mut map: HashMap<InstrumentAsset, String> = HashMap::new();
        let asset = InstrumentAsset::from_code("cu2501");

        map.insert(asset, "Copper".to_string());

        assert_eq!(map.get(&asset), Some(&"Copper".to_string()));
    }

    // ==================== ExchangeMatchingEngine 测试 @yutiansut @quantaxis ====================

    /// 测试引擎创建
    #[test]
    fn test_exchange_matching_engine_new() {
        let engine = ExchangeMatchingEngine::new();

        assert!(engine.orderbooks.is_empty(), "初始订单簿应为空");
        assert!(engine.prev_close_map.is_empty(), "初始昨收盘价映射应为空");
        assert!(engine.trading_day.read().is_empty(), "初始交易日应为空");
    }

    /// 测试 Default trait 实现
    #[test]
    fn test_exchange_matching_engine_default() {
        let engine = ExchangeMatchingEngine::default();

        assert!(engine.orderbooks.is_empty());
        assert!(engine.prev_close_map.is_empty());
    }

    /// 测试注册多个合约
    #[test]
    fn test_register_multiple_instruments() {
        let engine = ExchangeMatchingEngine::new();

        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();
        engine.register_instrument("au2512".to_string(), 935.56).unwrap();
        engine.register_instrument("IF2512".to_string(), 4500.0).unwrap();

        let instruments = engine.get_instruments();
        assert_eq!(instruments.len(), 3);
    }

    /// 测试获取不存在的订单簿
    #[test]
    fn test_get_orderbook_not_found() {
        let engine = ExchangeMatchingEngine::new();

        let orderbook = engine.get_orderbook("NON_EXISTENT");
        assert!(orderbook.is_none(), "不存在的合约应返回 None");
    }

    /// 测试设置和获取交易日
    #[test]
    fn test_set_get_trading_day() {
        let engine = ExchangeMatchingEngine::new();

        assert!(engine.get_trading_day().is_empty(), "初始交易日应为空");

        engine.set_trading_day("20241217".to_string());
        assert_eq!(engine.get_trading_day(), "20241217");

        engine.set_trading_day("20241218".to_string());
        assert_eq!(engine.get_trading_day(), "20241218");
    }

    /// 测试获取成交记录器
    #[test]
    fn test_get_trade_recorder() {
        let engine = ExchangeMatchingEngine::new();

        let recorder1 = engine.get_trade_recorder();
        let recorder2 = engine.get_trade_recorder();

        // 应返回同一个 Arc 引用
        assert!(Arc::ptr_eq(&recorder1, &recorder2), "应返回同一个 TradeRecorder 实例");
    }

    /// 测试获取昨收盘价
    #[test]
    fn test_get_prev_close() {
        let engine = ExchangeMatchingEngine::new();

        // 未注册合约
        assert!(engine.get_prev_close("cu2501").is_none());

        // 注册合约后
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();
        assert_eq!(engine.get_prev_close("cu2501"), Some(85000.0));

        // 不同合约
        engine.register_instrument("au2512".to_string(), 935.56).unwrap();
        assert_eq!(engine.get_prev_close("au2512"), Some(935.56));
    }

    /// 测试获取最新价格 - 未注册合约
    #[test]
    fn test_get_last_price_not_found() {
        let engine = ExchangeMatchingEngine::new();

        let price = engine.get_last_price("NON_EXISTENT");
        assert!(price.is_none());
    }

    /// 测试设置交易状态 - 合约不存在
    #[test]
    fn test_set_trading_state_instrument_not_found() {
        let engine = ExchangeMatchingEngine::new();

        let result = engine.set_trading_state("NON_EXISTENT", TradingState::ContinuousTrading);
        assert!(result.is_err());

        match result {
            Err(ExchangeError::MatchingError(msg)) => {
                assert!(msg.contains("Instrument not found"));
            }
            _ => panic!("应返回 MatchingError"),
        }
    }

    /// 测试设置交易状态 - 合约存在
    #[test]
    fn test_set_trading_state_success() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        // 目前是 TODO 状态，但调用应成功
        let result = engine.set_trading_state("cu2501", TradingState::ContinuousTrading);
        assert!(result.is_ok());
    }

    /// 测试重复注册同一合约
    /// 行为: 覆盖旧订单簿
    #[test]
    fn test_register_instrument_overwrite() {
        let engine = ExchangeMatchingEngine::new();

        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();
        assert_eq!(engine.get_prev_close("cu2501"), Some(85000.0));

        // 重新注册，更新昨收盘价
        engine.register_instrument("cu2501".to_string(), 86000.0).unwrap();
        assert_eq!(engine.get_prev_close("cu2501"), Some(86000.0));

        // 合约数量不变
        assert_eq!(engine.get_instruments().len(), 1);
    }

    /// 测试获取合约列表为空
    #[test]
    fn test_get_instruments_empty() {
        let engine = ExchangeMatchingEngine::new();

        let instruments = engine.get_instruments();
        assert!(instruments.is_empty());
    }

    /// 测试负数昨收盘价
    /// 边界情况: 允许但实际不应出现
    #[test]
    fn test_register_instrument_negative_prev_close() {
        let engine = ExchangeMatchingEngine::new();

        // 负数价格技术上允许注册
        let result = engine.register_instrument("TEST".to_string(), -100.0);
        assert!(result.is_ok());

        assert_eq!(engine.get_prev_close("TEST"), Some(-100.0));
    }

    /// 测试零昨收盘价
    #[test]
    fn test_register_instrument_zero_prev_close() {
        let engine = ExchangeMatchingEngine::new();

        let result = engine.register_instrument("TEST".to_string(), 0.0);
        assert!(result.is_ok());

        assert_eq!(engine.get_prev_close("TEST"), Some(0.0));
    }

    /// 测试特殊字符合约代码
    #[test]
    fn test_register_instrument_special_chars() {
        let engine = ExchangeMatchingEngine::new();

        // 带交易所前缀的完整代码
        engine.register_instrument("SHFE.cu2501".to_string(), 85000.0).unwrap();
        engine.register_instrument("CFFEX.IF2512".to_string(), 4500.0).unwrap();

        assert!(engine.get_orderbook("SHFE.cu2501").is_some());
        assert!(engine.get_orderbook("CFFEX.IF2512").is_some());
    }

    /// 测试并发注册合约
    #[test]
    fn test_register_instrument_concurrent() {
        use std::thread;

        let engine = Arc::new(ExchangeMatchingEngine::new());
        let mut handles = vec![];

        for i in 0..10 {
            let engine_clone = engine.clone();
            handles.push(thread::spawn(move || {
                let code = format!("INST{:02}", i);
                engine_clone.register_instrument(code, 100.0 + i as f64).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(engine.get_instruments().len(), 10);
    }

    /// 测试订单簿锁定和读取
    #[test]
    fn test_orderbook_read_lock() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();

        // 可以同时获取多个读锁
        let _guard1 = orderbook.read();
        // 第二个读锁在第一个释放后可获取
        // (这里不能同时持有两个，因为测试是单线程的)
    }
}
