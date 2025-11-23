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
}
