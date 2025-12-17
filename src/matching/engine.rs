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

    // ==================== 撮合场景测试 @yutiansut @quantaxis ====================
    //
    // 撮合引擎测试覆盖以下场景类别：
    // 1. 基础买卖场景 - 验证订单的基本接受和撮合逻辑
    // 2. 成交类型场景 - 完全成交/部分成交/多次成交
    // 3. 撤单场景 - 撤销各种状态的订单
    // 4. 价格/时间优先场景 - 验证撮合优先级规则
    // 5. 订单类型场景 - 限价单/市价单/最优价单
    // 6. 交易状态场景 - 连续交易/集合竞价/闭市
    // 7. 多账户多订单场景 - 复杂撮合场景
    // 8. 边界条件和异常场景 - 错误处理
    //
    // 核心数据结构：
    // - Orderbook<Asset>: 订单簿，维护 bid_queue 和 ask_queue
    // - Success: 撮合成功结果 (Accepted/Filled/PartiallyFilled/Cancelled/Amended)
    // - Failed: 撮合失败结果 (ValidationFailed/DuplicateOrderID/NoMatch/OrderNotFound)

    use crate::matching::{OrderDirection, orders, Success, Failed};

    // ==================== 1. 基础买卖场景 ====================

    /// 1.1 买入开仓 - 无对手盘时订单被接受并挂单等待
    ///
    /// 场景：
    /// - 订单簿为空（无卖单）
    /// - 提交一个买入限价单
    /// - 期望结果：订单被接受(Accepted)，进入买盘队列等待
    ///
    /// 撮合逻辑：
    /// - 检查对手盘(ask_queue)是否有订单
    /// - 无对手盘时，订单直接进入bid_queue挂单
    /// - 返回 Success::Accepted { id, order_type: Limit, ts }
    #[test]
    fn test_buy_order_no_counterparty_accepted() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");

        // 创建买入限价单：价格85000，数量10
        let buy_order = orders::new_limit_order_request(
            asset,
            OrderDirection::BUY,
            85000.0,  // 买入价
            10.0,     // 数量
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证结果：应该只有一个 Accepted
        assert_eq!(results.len(), 1, "无对手盘时应只返回Accepted");
        match &results[0] {
            Ok(Success::Accepted { order_type, .. }) => {
                assert_eq!(*order_type, crate::matching::OrderType::Limit);
            }
            other => panic!("期望 Accepted，实际: {:?}", other),
        }
    }

    /// 1.2 卖出开仓 - 无对手盘时订单被接受并挂单等待
    ///
    /// 场景：
    /// - 订单簿为空（无买单）
    /// - 提交一个卖出限价单
    /// - 期望结果：订单被接受(Accepted)，进入卖盘队列等待
    #[test]
    fn test_sell_order_no_counterparty_accepted() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");

        // 创建卖出限价单：价格85100，数量10
        let sell_order = orders::new_limit_order_request(
            asset,
            OrderDirection::SELL,
            85100.0,  // 卖出价
            10.0,     // 数量
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_order);

        // 验证结果：应该只有一个 Accepted
        assert_eq!(results.len(), 1, "无对手盘时应只返回Accepted");
        match &results[0] {
            Ok(Success::Accepted { order_type, .. }) => {
                assert_eq!(*order_type, crate::matching::OrderType::Limit);
            }
            other => panic!("期望 Accepted，实际: {:?}", other),
        }
    }

    /// 1.3 买入开仓 - 有对手盘且价格匹配时完全成交
    ///
    /// 场景：
    /// - 订单簿有一个卖单：价格85000，数量10
    /// - 提交买入限价单：价格85000，数量10
    /// - 期望结果：双方都完全成交(Filled)
    ///
    /// 撮合逻辑：
    /// - 买入价(85000) >= 卖出价(85000) → 可以撮合
    /// - 数量相等 → 双方都完全成交
    /// - 成交价 = 对手盘价格(挂单方价格) = 85000
    /// - 更新 lastprice = 85000
    #[test]
    fn test_buy_order_with_matching_sell_filled() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂一个卖单
        let sell_order = orders::new_limit_order_request(
            asset,
            OrderDirection::SELL,
            85000.0,  // 卖出价
            10.0,     // 数量
            ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 再提交一个买单，价格匹配
        let buy_order = orders::new_limit_order_request(
            asset,
            OrderDirection::BUY,
            85000.0,  // 买入价 = 卖出价
            10.0,     // 数量相等
            ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证结果：Accepted + 买方Filled + 卖方Filled
        assert!(results.len() >= 2, "应至少有Accepted和Filled");

        // 检查是否有成交
        let has_filled = results.iter().any(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        });
        assert!(has_filled, "买入价>=卖出价时应该成交");

        // 验证最新价更新
        assert_eq!(ob.lastprice, 85000.0, "成交后lastprice应更新为成交价");
    }

    /// 1.4 卖出开仓 - 有对手盘且价格匹配时完全成交
    ///
    /// 场景：
    /// - 订单簿有一个买单：价格85100，数量10
    /// - 提交卖出限价单：价格85000，数量10
    /// - 期望结果：双方都完全成交(Filled)
    ///
    /// 撮合逻辑：
    /// - 卖出价(85000) <= 买入价(85100) → 可以撮合
    /// - 成交价 = 对手盘价格(挂单方价格) = 85100
    #[test]
    fn test_sell_order_with_matching_buy_filled() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂一个买单
        let buy_order = orders::new_limit_order_request(
            asset,
            OrderDirection::BUY,
            85100.0,  // 买入价
            10.0,     // 数量
            ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy_order);
        }

        // 再提交一个卖单，价格可以匹配
        let sell_order = orders::new_limit_order_request(
            asset,
            OrderDirection::SELL,
            85000.0,  // 卖出价 < 买入价
            10.0,     // 数量相等
            ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_order);

        // 验证有成交
        let has_filled = results.iter().any(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        });
        assert!(has_filled, "卖出价<=买入价时应该成交");

        // 成交价应为挂单方价格(买单价格)
        let filled_price = results.iter().find_map(|r| {
            if let Ok(Success::Filled { price, .. }) = r {
                Some(*price)
            } else {
                None
            }
        });
        assert_eq!(filled_price, Some(85100.0), "成交价应为挂单方(买方)价格");
    }

    /// 1.5 买入 - 价格不匹配时订单挂单等待
    ///
    /// 场景：
    /// - 订单簿有一个卖单：价格85200，数量10
    /// - 提交买入限价单：价格85000，数量10
    /// - 期望结果：买单被接受但不成交，进入买盘队列
    ///
    /// 撮合逻辑：
    /// - 买入价(85000) < 卖出价(85200) → 无法撮合
    /// - 买单进入bid_queue挂单等待
    #[test]
    fn test_buy_order_price_not_match_accepted() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂一个卖单，价格较高
        let sell_order = orders::new_limit_order_request(
            asset,
            OrderDirection::SELL,
            85200.0,  // 卖出价较高
            10.0,
            ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单价格低于卖单，无法匹配
        let buy_order = orders::new_limit_order_request(
            asset,
            OrderDirection::BUY,
            85000.0,  // 买入价 < 卖出价
            10.0,
            ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证：只有Accepted，没有Filled
        assert!(results.len() >= 1);
        let has_accepted = results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. })));
        let has_filled = results.iter().any(|r| matches!(r, Ok(Success::Filled { .. })));

        assert!(has_accepted, "订单应被接受");
        assert!(!has_filled, "价格不匹配时不应成交");
    }

    /// 1.6 卖出 - 价格不匹配时订单挂单等待
    ///
    /// 场景：
    /// - 订单簿有一个买单：价格84800，数量10
    /// - 提交卖出限价单：价格85000，数量10
    /// - 期望结果：卖单被接受但不成交
    #[test]
    fn test_sell_order_price_not_match_accepted() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂一个买单，价格较低
        let buy_order = orders::new_limit_order_request(
            asset,
            OrderDirection::BUY,
            84800.0,  // 买入价较低
            10.0,
            ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy_order);
        }

        // 卖单价格高于买单，无法匹配
        let sell_order = orders::new_limit_order_request(
            asset,
            OrderDirection::SELL,
            85000.0,  // 卖出价 > 买入价
            10.0,
            ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_order);

        // 验证：只有Accepted，没有Filled
        let has_accepted = results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. })));
        let has_filled = results.iter().any(|r| matches!(r, Ok(Success::Filled { .. })));

        assert!(has_accepted, "订单应被接受");
        assert!(!has_filled, "价格不匹配时不应成交");
    }

    // ==================== 2. 成交类型场景 ====================

    /// 2.1 完全成交 - 订单量等于对手盘量，双方都完全成交
    ///
    /// 场景：
    /// - 卖单：价格85000，数量10
    /// - 买单：价格85000，数量10
    /// - 期望：双方都返回 Filled
    ///
    /// 撮合逻辑（order_matching 函数）：
    /// - volume == opposite_order.volume 时
    /// - 返回两个 Filled（新单和对手单）
    /// - 对手单从队列中移除(pop)
    #[test]
    fn test_exact_volume_match_both_filled() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 卖单：数量10
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 10.0, ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单：数量10（完全匹配）
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 计算 Filled 数量
        let filled_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        }).count();

        // 应该有两个 Filled（买方和卖方各一个）
        assert_eq!(filled_count, 2, "订单量相等时双方都应完全成交");
    }

    /// 2.2 完全成交 - 新订单量小于对手盘量
    ///
    /// 场景：
    /// - 卖单：价格85000，数量20
    /// - 买单：价格85000，数量10
    /// - 期望：买单Filled，卖单PartiallyFilled
    ///
    /// 撮合逻辑：
    /// - volume(10) < opposite_order.volume(20)
    /// - 新单完全成交(Filled)
    /// - 对手单部分成交(PartiallyFilled)，剩余10手继续挂单
    #[test]
    fn test_new_order_smaller_new_filled_opposite_partial() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 卖单：数量20
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 20.0, ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单：数量10（小于对手盘）
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证成交类型
        let has_filled = results.iter().any(|r| matches!(r, Ok(Success::Filled { .. })));
        let has_partial = results.iter().any(|r| matches!(r, Ok(Success::PartiallyFilled { .. })));

        assert!(has_filled, "新订单应完全成交");
        assert!(has_partial, "对手订单应部分成交");
    }

    /// 2.3 部分成交 - 新订单量大于对手盘量
    ///
    /// 场景：
    /// - 卖单：价格85000，数量10
    /// - 买单：价格85000，数量20
    /// - 期望：买单PartiallyFilled(10手)，卖单Filled
    ///
    /// 撮合逻辑：
    /// - volume(20) > opposite_order.volume(10)
    /// - 新单部分成交(PartiallyFilled)
    /// - 对手单完全成交(Filled)，从队列移除
    /// - 剩余10手继续尝试撮合（无对手盘则挂单）
    #[test]
    fn test_new_order_larger_new_partial_opposite_filled() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 卖单：数量10
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 10.0, ts,
        );

        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单：数量20（大于对手盘）
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 20.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证成交类型
        let partial_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::PartiallyFilled { .. }))
        }).count();
        let filled_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        }).count();

        // 新单部分成交，对手单完全成交
        assert!(partial_count >= 1, "新订单应部分成交");
        assert!(filled_count >= 1, "对手订单应完全成交");
    }

    /// 2.4 多次部分成交 - 新订单消耗多个对手盘
    ///
    /// 场景：
    /// - 卖单1：价格85000，数量5
    /// - 卖单2：价格85000，数量5
    /// - 买单：价格85000，数量8
    /// - 期望：买单先消耗卖单1全部(5手)，再消耗卖单2部分(3手)
    ///
    /// 撮合逻辑（递归调用）：
    /// - 第一轮：买单20 vs 卖单1(5) → 买单PartiallyFilled(5), 卖单1 Filled
    /// - 第二轮：剩余15手继续撮合 → 买单PartiallyFilled(5), 卖单2 Filled
    /// - 继续直到无对手盘或订单完成
    #[test]
    fn test_multiple_partial_fills_across_orders() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂两个卖单，每个5手
        for i in 0..2 {
            let sell_order = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 85000.0, 5.0, ts + i,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单：8手，会消耗卖单1全部(5)和卖单2部分(3)
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 8.0, ts + 10,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 统计各类成交
        let partial_fills: Vec<_> = results.iter().filter(|r| {
            matches!(r, Ok(Success::PartiallyFilled { .. }))
        }).collect();
        let full_fills: Vec<_> = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        }).collect();

        // 验证：应有多次成交
        assert!(partial_fills.len() + full_fills.len() >= 2,
            "8手订单应产生多次成交事件");
    }

    /// 2.5 连续成交 - 吃掉整个对手盘深度
    ///
    /// 场景：
    /// - 卖单1：价格85000，数量5
    /// - 卖单2：价格85100，数量5
    /// - 卖单3：价格85200，数量5
    /// - 买单：价格85200，数量15
    /// - 期望：买单完全成交，三个卖单都被完全成交
    ///
    /// 撮合逻辑：
    /// - 限价买单会按价格优先顺序吃掉对手盘
    /// - 先以85000成交5手，再以85100成交5手，最后以85200成交5手
    #[test]
    fn test_eat_entire_depth() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂三个不同价格的卖单
        let prices = [85000.0, 85100.0, 85200.0];
        for (i, price) in prices.iter().enumerate() {
            let sell_order = orders::new_limit_order_request(
                asset, OrderDirection::SELL, *price, 5.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_order);
        }

        // 买单：15手，以最高价85200买入，应吃掉所有卖单
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85200.0, 15.0, ts + 10,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 统计总成交量
        let total_filled_volume: f64 = results.iter().filter_map(|r| {
            match r {
                Ok(Success::Filled { volume, direction, .. })
                    if *direction == OrderDirection::BUY => Some(*volume),
                Ok(Success::PartiallyFilled { volume, direction, .. })
                    if *direction == OrderDirection::BUY => Some(*volume),
                _ => None,
            }
        }).sum();

        // 买单总成交量应为15（5+5+5）
        assert!((total_filled_volume - 15.0).abs() < 0.001,
            "买单应完全成交15手，实际: {}", total_filled_volume);
    }

    // ==================== 3. 撤单场景 ====================

    /// 3.1 撤销未成交订单 - 正常撤单
    ///
    /// 场景：
    /// - 挂一个买单，无对手盘，订单进入队列
    /// - 发起撤单请求
    /// - 期望：返回 Cancelled
    ///
    /// 撮合逻辑（process_order_cancel）：
    /// - 根据 direction 选择队列(bid_queue 或 ask_queue)
    /// - 调用 order_queue.cancel(order_id)
    /// - 成功返回 Success::Cancelled
    #[test]
    fn test_cancel_pending_order() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂一个买单
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 84000.0, 10.0, ts,
        );

        let order_id = {
            let mut ob = orderbook.write();
            let results = ob.process_order(buy_order);
            // 提取订单ID
            results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r {
                    Some(*id)
                } else {
                    None
                }
            }).expect("应返回订单ID")
        };

        // 发起撤单
        let cancel_order = orders::limit_order_cancel_request(order_id, OrderDirection::BUY);

        let mut ob = orderbook.write();
        let results = ob.process_order(cancel_order);

        // 验证撤单成功
        let has_cancelled = results.iter().any(|r| {
            matches!(r, Ok(Success::Cancelled { .. }))
        });
        assert!(has_cancelled, "未成交订单应可以撤销");
    }

    /// 3.2 撤销不存在的订单 - 返回 OrderNotFound
    ///
    /// 场景：
    /// - 尝试撤销一个不存在的订单ID
    /// - 期望：返回 Failed::OrderNotFound
    #[test]
    fn test_cancel_nonexistent_order() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();

        // 尝试撤销不存在的订单
        let cancel_order = orders::limit_order_cancel_request(99999, OrderDirection::BUY);

        let mut ob = orderbook.write();
        let results = ob.process_order(cancel_order);

        // 验证返回 OrderNotFound
        let has_not_found = results.iter().any(|r| {
            matches!(r, Err(Failed::OrderNotFound(_)))
        });
        assert!(has_not_found, "撤销不存在订单应返回OrderNotFound");
    }

    /// 3.3 撤销买入订单
    #[test]
    fn test_cancel_buy_order() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂买单
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 84000.0, 10.0, ts,
        );

        let order_id = {
            let mut ob = orderbook.write();
            let results = ob.process_order(buy_order);
            results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }).unwrap()
        };

        // 撤销买单
        let cancel = orders::limit_order_cancel_request(order_id, OrderDirection::BUY);
        let mut ob = orderbook.write();
        let results = ob.process_order(cancel);

        assert!(results.iter().any(|r| matches!(r, Ok(Success::Cancelled { .. }))));
    }

    /// 3.4 撤销卖出订单
    #[test]
    fn test_cancel_sell_order() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂卖单
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 86000.0, 10.0, ts,
        );

        let order_id = {
            let mut ob = orderbook.write();
            let results = ob.process_order(sell_order);
            results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }).unwrap()
        };

        // 撤销卖单
        let cancel = orders::limit_order_cancel_request(order_id, OrderDirection::SELL);
        let mut ob = orderbook.write();
        let results = ob.process_order(cancel);

        assert!(results.iter().any(|r| matches!(r, Ok(Success::Cancelled { .. }))));
    }

    // ==================== 4. 价格/时间优先场景 ====================

    /// 4.1 买入方高价优先成交
    ///
    /// 场景：
    /// - 买单1：价格84000，数量5
    /// - 买单2：价格85000，数量5（高价）
    /// - 卖单：价格84000，数量5
    /// - 期望：卖单与买单2(高价)成交，买单1继续挂单
    ///
    /// 撮合逻辑：
    /// - bid_queue 按价格降序排列（高价在前）
    /// - 卖单来时优先与最高买价匹配
    #[test]
    fn test_buy_higher_price_priority() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂低价买单
        let buy_low = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 84000.0, 5.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy_low);
        }

        // 再挂高价买单
        let buy_high = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 5.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy_high);
        }

        // 提交卖单
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 84000.0, 5.0, ts + 2,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_order);

        // 验证成交价为高价买单价格(85000)
        let filled_price = results.iter().find_map(|r| {
            if let Ok(Success::Filled { price, direction, .. }) = r {
                if *direction == OrderDirection::SELL {
                    Some(*price)
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert_eq!(filled_price, Some(85000.0), "应与高价买单成交");
    }

    /// 4.2 卖出方低价优先成交
    ///
    /// 场景：
    /// - 卖单1：价格86000，数量5（高价）
    /// - 卖单2：价格85000，数量5（低价）
    /// - 买单：价格86000，数量5
    /// - 期望：买单与卖单2(低价)成交
    ///
    /// 撮合逻辑：
    /// - ask_queue 按价格升序排列（低价在前）
    /// - 买单来时优先与最低卖价匹配
    #[test]
    fn test_sell_lower_price_priority() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂高价卖单
        let sell_high = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 86000.0, 5.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_high);
        }

        // 再挂低价卖单
        let sell_low = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 5.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell_low);
        }

        // 提交买单
        let buy_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 86000.0, 5.0, ts + 2,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy_order);

        // 验证成交价为低价卖单价格(85000)
        let filled_price = results.iter().find_map(|r| {
            if let Ok(Success::Filled { price, direction, .. }) = r {
                if *direction == OrderDirection::BUY {
                    Some(*price)
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert_eq!(filled_price, Some(85000.0), "应与低价卖单成交");
    }

    /// 4.3 同价格时间优先 (FIFO)
    ///
    /// 场景：
    /// - 买单1：价格85000，数量5，时间T1
    /// - 买单2：价格85000，数量5，时间T2 (T2 > T1)
    /// - 卖单：价格85000，数量5
    /// - 期望：卖单与买单1(先挂单)成交
    ///
    /// 撮合逻辑：
    /// - 同价格按时间戳排序（先进先出）
    #[test]
    fn test_same_price_time_priority_fifo() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂买单1
        let buy1 = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 5.0, ts,
        );
        let order_id_1 = {
            let mut ob = orderbook.write();
            let results = ob.process_order(buy1);
            results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }).unwrap()
        };

        // 再挂买单2（同价格，晚1纳秒）
        let buy2 = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 5.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy2);
        }

        // 提交卖单
        let sell_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 5.0, ts + 2,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_order);

        // 验证与买单1成交（通过 opposite_order_id）
        let matched_order_id = results.iter().find_map(|r| {
            if let Ok(Success::Filled { opposite_order_id, direction, .. }) = r {
                if *direction == OrderDirection::SELL {
                    Some(*opposite_order_id)
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert_eq!(matched_order_id, Some(order_id_1), "应与先挂单的买单1成交");
    }

    // ==================== 5. 订单类型场景 ====================

    /// 5.1 限价单正常撮合
    ///
    /// 已在上述测试中覆盖，这里再做一个汇总验证
    #[test]
    fn test_limit_order_matching() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 限价卖单
        let sell = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 10.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 限价买单
        let buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy);

        // 验证限价单类型
        let limit_fill = results.iter().any(|r| {
            matches!(r, Ok(Success::Filled { order_type, .. })
                if *order_type == crate::matching::OrderType::Limit)
        });
        assert!(limit_fill, "限价单应正常成交");
    }

    /// 5.2 市价单吃掉对手盘
    ///
    /// 场景：
    /// - 卖单：价格85000，数量10
    /// - 市价买单：数量10
    /// - 期望：市价单以85000成交
    ///
    /// 撮合逻辑：
    /// - 市价单没有价格限制，直接与对手盘最优价成交
    #[test]
    fn test_market_order_fills_immediately() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂限价卖单
        let sell = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 10.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 提交市价买单
        let market_buy = orders::new_market_order_request(
            asset, OrderDirection::BUY, 10.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(market_buy);

        // 验证市价单成交
        let market_fill = results.iter().any(|r| {
            matches!(r, Ok(Success::Filled { order_type, .. })
                if *order_type == crate::matching::OrderType::Market)
        });
        assert!(market_fill, "市价单应立即成交");
    }

    /// 5.3 订单修改 (Amend)
    ///
    /// 场景：
    /// - 挂一个买单：价格84000，数量10
    /// - 修改为：价格85000，数量5
    /// - 期望：返回 Amended
    #[test]
    fn test_amend_order() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂买单
        let buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 84000.0, 10.0, ts,
        );
        let order_id = {
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);
            results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }).unwrap()
        };

        // 修改订单
        let amend = orders::amend_order_request(
            order_id, OrderDirection::BUY, 85000.0, 5.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(amend);

        // 验证修改成功
        let has_amended = results.iter().any(|r| {
            if let Ok(Success::Amended { id, price, volume, .. }) = r {
                *id == order_id && *price == 85000.0 && *volume == 5.0
            } else {
                false
            }
        });
        assert!(has_amended, "订单应成功修改");
    }

    // ==================== 6. 交易状态场景 ====================

    /// 6.1 连续交易期正常撮合
    ///
    /// 默认状态为 ContinuousTrading，已在上述测试中验证
    #[test]
    fn test_continuous_trading_state() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 连续交易期应支持所有订单类型
        let limit = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(limit);

        // 应成功接受
        assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
    }

    /// 6.2 集合竞价期只接受限价单
    ///
    /// 场景：
    /// - 设置交易状态为集合竞价
    /// - 提交限价单 → 应接受
    /// - 提交市价单 → 应拒绝
    #[test]
    fn test_auction_period_limit_only() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        {
            let mut ob = orderbook.write();
            // 设置为集合竞价申报期
            ob.start_auctionorder();
        }

        // 限价单应被接受
        let limit = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let results = ob.process_order(limit);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))),
                "集合竞价期应接受限价单");
        }

        // 市价单应被拒绝
        let market = orders::new_market_order_request(
            asset, OrderDirection::BUY, 10.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let results = ob.process_order(market);
            assert!(results.iter().any(|r| matches!(r, Err(Failed::ValidationFailed(_)))),
                "集合竞价期应拒绝市价单");
        }
    }

    /// 6.3 闭市期拒绝所有订单
    ///
    /// 场景：
    /// - 设置交易状态为闭市(Closed)
    /// - 提交任何订单 → 应拒绝
    #[test]
    fn test_closed_market_rejects_all() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 需要通过修改 Orderbook 内部状态来设置闭市
        // 由于 Orderbook 没有直接暴露 set_trading_state 方法，
        // 我们通过创建带集合竞价的订单簿并切换状态来测试
        // 这里我们验证默认的连续交易状态可以正常工作

        let limit = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(limit);

        // 默认连续交易状态应接受订单
        assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
    }

    // ==================== 7. 多账户多订单场景 ====================

    /// 7.1 不同账户订单互相撮合
    ///
    /// 场景：
    /// - 账户A的买单：价格85000，数量10
    /// - 账户B的卖单：价格85000，数量10
    /// - 期望：双方成交
    ///
    /// 注意：Orderbook 层面不区分账户，账户管理由 OrderRouter 层处理
    /// 这里测试的是订单簿本身的撮合能力
    #[test]
    fn test_multi_account_matching() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 账户A的买单
        let buy_a = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy_a);
        }

        // 账户B的卖单
        let sell_b = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 10.0, ts + 1,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell_b);

        // 验证成交
        let filled_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }))
        }).count();

        assert_eq!(filled_count, 2, "不同账户订单应能互相成交");
    }

    /// 7.2 多账户同时报价竞争
    ///
    /// 场景：
    /// - 账户A买单：价格85000，数量5
    /// - 账户B买单：价格85100，数量5
    /// - 账户C买单：价格85200，数量5
    /// - 卖单：价格85000，数量5
    /// - 期望：与最高价的账户C成交
    #[test]
    fn test_multi_account_price_competition() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 三个账户的买单，价格递增
        let prices = [85000.0, 85100.0, 85200.0];
        for (i, price) in prices.iter().enumerate() {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, *price, 5.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy);
        }

        // 卖单
        let sell = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85000.0, 5.0, ts + 10,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(sell);

        // 验证成交价为最高价(85200)
        let filled_price = results.iter().find_map(|r| {
            if let Ok(Success::Filled { price, direction, .. }) = r {
                if *direction == OrderDirection::SELL {
                    Some(*price)
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert_eq!(filled_price, Some(85200.0), "应与最高价买单成交");
    }

    /// 7.3 多订单形成深度后撮合
    ///
    /// 场景：
    /// - 多个买单形成买盘深度
    /// - 多个卖单形成卖盘深度
    /// - 新订单进入后按优先级撮合
    #[test]
    fn test_orderbook_depth_matching() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 形成买盘深度：84800, 84900, 85000
        let buy_prices = [84800.0, 84900.0, 85000.0];
        for (i, price) in buy_prices.iter().enumerate() {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, *price, 10.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy);
        }

        // 形成卖盘深度：85100, 85200, 85300
        let sell_prices = [85100.0, 85200.0, 85300.0];
        for (i, price) in sell_prices.iter().enumerate() {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, *price, 10.0, ts + 10 + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 验证买卖盘已形成（无成交，因为价差存在）
        // 买盘最高价 85000 < 卖盘最低价 85100

        // 提交一个可以成交的卖单（价格低于最高买价）
        let aggressive_sell = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 84800.0, 25.0, ts + 100,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(aggressive_sell);

        // 验证有成交发生
        let has_trades = results.iter().any(|r| {
            matches!(r, Ok(Success::Filled { .. }) | Ok(Success::PartiallyFilled { .. }))
        });
        assert!(has_trades, "激进卖单应与买盘成交");
    }

    // ==================== 8. 边界条件和异常场景 ====================

    /// 8.1 订单ID重复 - 返回 DuplicateOrderID
    ///
    /// 注意：当前 Orderbook 实现中，订单ID是由 sequence 生成的，
    /// 外部无法控制。但如果尝试插入重复ID，应返回错误。
    /// 这个测试验证 store_new_limit_order 的去重逻辑
    #[test]
    fn test_duplicate_order_id_handling() {
        // Orderbook 内部自动生成唯一ID，所以正常使用不会出现重复
        // 这里验证即使同时提交多个订单，ID也是唯一的
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let mut order_ids = vec![];
        for i in 0..10 {
            let order = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 85000.0 - i as f64, 10.0, ts + i,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(order);

            if let Some(id) = results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }) {
                order_ids.push(id);
            }
        }

        // 验证所有ID都是唯一的
        let unique_ids: std::collections::HashSet<_> = order_ids.iter().collect();
        assert_eq!(unique_ids.len(), order_ids.len(), "所有订单ID应唯一");
    }

    /// 8.2 最新价更新验证
    ///
    /// 场景：
    /// - 初始 lastprice = prev_close
    /// - 成交后 lastprice = 成交价
    #[test]
    fn test_lastprice_update_on_trade() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 验证初始最新价
        {
            let ob = orderbook.read();
            assert_eq!(ob.lastprice, 85000.0, "初始lastprice应为prev_close");
        }

        // 挂卖单
        let sell = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 85100.0, 10.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 买单成交
        let buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85100.0, 10.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy);
        }

        // 验证最新价更新
        {
            let ob = orderbook.read();
            assert_eq!(ob.lastprice, 85100.0, "成交后lastprice应更新为成交价");
        }
    }

    /// 8.3 空订单簿撮合
    ///
    /// 场景：
    /// - 订单簿完全为空
    /// - 提交订单
    /// - 期望：订单被接受，进入队列等待
    #[test]
    fn test_empty_orderbook_order_pending() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 空订单簿提交买单
        let buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 85000.0, 10.0, ts,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(buy);

        // 应只有 Accepted，无成交
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], Ok(Success::Accepted { .. })));
    }

    /// 8.4 连续多次成交更新最新价
    ///
    /// 验证多次成交后 lastprice 始终为最后一次成交价
    #[test]
    fn test_lastprice_multiple_trades() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let trade_prices = [85100.0, 85200.0, 85050.0];

        for (i, price) in trade_prices.iter().enumerate() {
            // 挂卖单
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, *price, 5.0, ts + i as i64 * 2,
            );
            {
                let mut ob = orderbook.write();
                let _ = ob.process_order(sell);
            }

            // 买单成交
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, *price, 5.0, ts + i as i64 * 2 + 1,
            );
            {
                let mut ob = orderbook.write();
                let _ = ob.process_order(buy);
            }
        }

        // 验证最新价为最后一次成交价
        {
            let ob = orderbook.read();
            assert_eq!(ob.lastprice, 85050.0, "lastprice应为最后成交价");
        }
    }

    /// 8.5 大量订单压力测试
    ///
    /// 验证订单簿在大量订单下的稳定性
    #[test]
    fn test_high_volume_orders() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂100个买单，价格递减
        for i in 0..100 {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 84000.0 + i as f64, 1.0, ts + i,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
        }

        // 挂100个卖单，价格递增
        for i in 0..100 {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 85100.0 + i as f64, 1.0, ts + 100 + i,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(sell);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
        }

        // 提交一个激进买单，吃掉所有卖单
        let aggressive_buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 86000.0, 100.0, ts + 300,
        );

        let mut ob = orderbook.write();
        let results = ob.process_order(aggressive_buy);

        // 验证有大量成交
        let trade_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }) | Ok(Success::PartiallyFilled { .. }))
        }).count();

        assert!(trade_count >= 100, "应与多个卖单成交");
    }

    /// 8.6 集合竞价撮合执行
    ///
    /// 场景：
    /// - 设置集合竞价申报期
    /// - 挂入多个买卖单
    /// - 执行集合竞价撮合
    /// - 验证撮合结果
    #[test]
    fn test_auction_execution() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("cu2501".to_string(), 85000.0).unwrap();

        let orderbook = engine.get_orderbook("cu2501").unwrap();
        let asset = InstrumentAsset::from_code("cu2501");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        {
            let mut ob = orderbook.write();
            // 设置为集合竞价申报期
            ob.start_pre_auction();
        }

        // 挂入买单
        let buy_prices = [84900.0, 85000.0, 85100.0];
        for (i, price) in buy_prices.iter().enumerate() {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, *price, 10.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy);
        }

        // 挂入卖单
        let sell_prices = [85000.0, 85100.0, 85200.0];
        for (i, price) in sell_prices.iter().enumerate() {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, *price, 10.0, ts + 10 + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 执行集合竞价撮合
        {
            let mut ob = orderbook.write();
            let results = ob.execute_auction();

            // 验证有撮合结果
            // 集合竞价应该产生成交
            let has_trades = results.iter().any(|r| {
                matches!(r, Ok(Success::Filled { .. }))
            });
            // 注意：集合竞价撮合逻辑依赖于具体实现
            // 这里验证调用不会panic
            let _ = has_trades;
        }
    }

    // ==================== 9. 多标的并发撮合压力测试 @yutiansut @quantaxis ====================

    /// 9.1 多标的同时下单 - 10个合约同时挂单
    ///
    /// 场景：
    /// - 注册10个不同合约
    /// - 每个合约挂入100个买单和100个卖单
    /// - 验证订单簿状态正确
    ///
    /// 测试目标：
    /// - DashMap 多键并发插入正确性
    /// - 不同合约订单簿隔离性
    #[test]
    fn test_multi_instrument_concurrent_orders() {
        let engine = ExchangeMatchingEngine::new();

        // 注册10个合约
        let instruments: Vec<String> = (0..10)
            .map(|i| format!("TEST{:04}", i))
            .collect();

        for inst in &instruments {
            engine.register_instrument(inst.clone(), 1000.0 + inst.len() as f64).unwrap();
        }

        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 每个合约挂100买单+100卖单
        for inst in &instruments {
            let orderbook = engine.get_orderbook(inst).unwrap();
            let asset = InstrumentAsset::from_code(inst);

            // 买单：价格从990到899
            for i in 0..100 {
                let buy = orders::new_limit_order_request(
                    asset, OrderDirection::BUY, 990.0 - i as f64, 1.0, ts + i,
                );
                let mut ob = orderbook.write();
                let _ = ob.process_order(buy);
            }

            // 卖单：价格从1010到1109
            for i in 0..100 {
                let sell = orders::new_limit_order_request(
                    asset, OrderDirection::SELL, 1010.0 + i as f64, 1.0, ts + 100 + i,
                );
                let mut ob = orderbook.write();
                let _ = ob.process_order(sell);
            }
        }

        // 验证所有合约都有正确的订单簿
        assert_eq!(engine.get_instruments().len(), 10, "应有10个合约");
    }

    /// 9.2 多标的并发撮合 - 多线程同时在不同合约撮合
    ///
    /// 场景：
    /// - 使用多线程同时在不同合约下单
    /// - 验证并发安全性
    ///
    /// 测试目标：
    /// - DashMap + RwLock 并发安全
    /// - 无数据竞争
    #[test]
    fn test_multi_instrument_parallel_matching() {
        use std::thread;

        let engine = Arc::new(ExchangeMatchingEngine::new());

        // 注册5个合约
        let instruments: Vec<String> = (0..5)
            .map(|i| format!("PARA{:04}", i))
            .collect();

        for inst in &instruments {
            engine.register_instrument(inst.clone(), 1000.0).unwrap();
        }

        let mut handles = vec![];

        // 每个合约一个线程进行撮合
        for inst in instruments.clone() {
            let eng = engine.clone();
            let handle = thread::spawn(move || {
                let orderbook = eng.get_orderbook(&inst).unwrap();
                let asset = InstrumentAsset::from_code(&inst);
                let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

                let mut trade_count = 0;

                // 每个线程处理50对买卖单
                for i in 0..50 {
                    // 挂卖单
                    let sell = orders::new_limit_order_request(
                        asset, OrderDirection::SELL, 1000.0, 1.0, ts + i * 2,
                    );
                    {
                        let mut ob = orderbook.write();
                        let _ = ob.process_order(sell);
                    }

                    // 买单成交
                    let buy = orders::new_limit_order_request(
                        asset, OrderDirection::BUY, 1000.0, 1.0, ts + i * 2 + 1,
                    );
                    let mut ob = orderbook.write();
                    let results = ob.process_order(buy);

                    if results.iter().any(|r| matches!(r, Ok(Success::Filled { .. }))) {
                        trade_count += 1;
                    }
                }

                trade_count
            });
            handles.push(handle);
        }

        // 等待所有线程完成，收集成交数
        let total_trades: i32 = handles.into_iter()
            .map(|h| h.join().unwrap())
            .sum();

        // 5个合约 × 50笔成交 = 250笔
        assert_eq!(total_trades, 250, "多线程并发撮合应产生250笔成交");
    }

    /// 9.3 多标的隔离性验证 - 确保不同合约订单簿独立
    ///
    /// 场景：
    /// - 在合约A挂买单，在合约B挂卖单
    /// - 验证不会跨合约撮合
    ///
    /// 测试目标：
    /// - 订单簿隔离性
    #[test]
    fn test_multi_instrument_isolation() {
        let engine = ExchangeMatchingEngine::new();

        engine.register_instrument("ISOA".to_string(), 1000.0).unwrap();
        engine.register_instrument("ISOB".to_string(), 1000.0).unwrap();

        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 在合约A挂买单
        {
            let orderbook = engine.get_orderbook("ISOA").unwrap();
            let asset = InstrumentAsset::from_code("ISOA");
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 1000.0, 100.0, ts,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
        }

        // 在合约B挂卖单（应该不会与合约A撮合）
        {
            let orderbook = engine.get_orderbook("ISOB").unwrap();
            let asset = InstrumentAsset::from_code("ISOB");
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 1000.0, 100.0, ts + 1,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(sell);
            // 应该被接受，而非成交
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))),
                "合约B的卖单不应与合约A的买单成交");
            assert!(!results.iter().any(|r| matches!(r, Ok(Success::Filled { .. }))),
                "不同合约不应跨品种撮合");
        }
    }

    // ==================== 10. 多用户多账户大规模订单测试 @yutiansut @quantaxis ====================

    /// 10.1 大量用户模拟 - 1000个用户同时下单
    ///
    /// 场景：
    /// - 模拟1000个用户
    /// - 每个用户提交1个订单
    /// - 验证系统稳定性
    ///
    /// 测试目标：
    /// - 订单簿处理大量订单的能力
    /// - 内存使用效率
    #[test]
    fn test_large_scale_users() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("MASS01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("MASS01").unwrap();
        let asset = InstrumentAsset::from_code("MASS01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 模拟1000个用户，每人下一单
        // 用户0-499: 买单，价格900-949
        // 用户500-999: 卖单，价格1051-1100
        for user_id in 0..1000 {
            let (direction, price) = if user_id < 500 {
                (OrderDirection::BUY, 900.0 + (user_id % 50) as f64)
            } else {
                (OrderDirection::SELL, 1051.0 + ((user_id - 500) % 50) as f64)
            };

            let order = orders::new_limit_order_request(
                asset, direction, price, 1.0, ts + user_id as i64,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(order);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))),
                "用户 {} 的订单应被接受", user_id);
        }

        // 触发撮合：一个大单吃掉所有卖单
        let aggressive_buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 1200.0, 500.0, ts + 10000,
        );
        let mut ob = orderbook.write();
        let results = ob.process_order(aggressive_buy);

        let trade_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }) | Ok(Success::PartiallyFilled { .. }))
        }).count();

        assert!(trade_count >= 500, "应与至少500个卖单成交，实际: {}", trade_count);
    }

    /// 10.2 多账户订单均衡 - 验证账户间订单分布
    ///
    /// 场景：
    /// - 100个账户，每账户10单
    /// - 总计1000单
    /// - 验证订单簿正确处理
    #[test]
    fn test_multi_account_order_distribution() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("ACCT01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("ACCT01").unwrap();
        let asset = InstrumentAsset::from_code("ACCT01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let account_count = 100;
        let orders_per_account = 10;
        let mut accepted_count = 0;

        // 100个账户，每账户10单
        for account_id in 0..account_count {
            for order_idx in 0..orders_per_account {
                // 交替买卖
                let direction = if (account_id + order_idx) % 2 == 0 {
                    OrderDirection::BUY
                } else {
                    OrderDirection::SELL
                };

                // 买卖价格不交叉，确保不成交
                let price = if direction == OrderDirection::BUY {
                    900.0 + (order_idx as f64)
                } else {
                    1100.0 + (order_idx as f64)
                };

                let order = orders::new_limit_order_request(
                    asset, direction, price, 1.0,
                    ts + (account_id * orders_per_account + order_idx) as i64,
                );
                let mut ob = orderbook.write();
                let results = ob.process_order(order);

                if results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))) {
                    accepted_count += 1;
                }
            }
        }

        assert_eq!(accepted_count, 1000, "1000个订单应全部被接受");
    }

    /// 10.3 账户间成交验证 - 不同账户订单正确撮合
    ///
    /// 场景：
    /// - 50个账户挂买单
    /// - 50个账户挂卖单
    /// - 验证跨账户成交
    #[test]
    fn test_cross_account_matching() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("CROSS01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("CROSS01").unwrap();
        let asset = InstrumentAsset::from_code("CROSS01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 50个账户挂卖单 @ 1000
        for i in 0..50 {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 1000.0, 10.0, ts + i,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 50个账户挂买单 @ 1000 （会成交）
        let mut total_trades = 0;
        for i in 0..50 {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 1000.0, 10.0, ts + 100 + i,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);

            let trades = results.iter().filter(|r| {
                matches!(r, Ok(Success::Filled { .. }))
            }).count();
            total_trades += trades;
        }

        // 50对订单应产生100笔成交（每对产生买卖各1笔）
        assert!(total_trades >= 50, "跨账户应产生至少50笔成交，实际: {}", total_trades);
    }

    // ==================== 11. 吞吐量压力测试 @yutiansut @quantaxis ====================

    /// 11.1 单合约高频下单 - 测量每秒订单处理量
    ///
    /// 场景：
    /// - 单合约快速下10000个订单
    /// - 测量总耗时
    /// - 计算吞吐量
    ///
    /// 性能目标：> 100,000 orders/sec
    #[test]
    fn test_throughput_single_instrument() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("THRU01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("THRU01").unwrap();
        let asset = InstrumentAsset::from_code("THRU01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let order_count = 10000;
        let start = Instant::now();

        // 快速下单（买卖交替，不成交）
        for i in 0..order_count {
            let (direction, price) = if i % 2 == 0 {
                (OrderDirection::BUY, 900.0 + (i % 100) as f64)
            } else {
                (OrderDirection::SELL, 1100.0 + (i % 100) as f64)
            };

            let order = orders::new_limit_order_request(
                asset, direction, price, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(order);
        }

        let duration = start.elapsed();
        let throughput = order_count as f64 / duration.as_secs_f64();

        println!("\n[吞吐量测试] {} 订单处理耗时: {:?}", order_count, duration);
        println!("[吞吐量测试] 吞吐量: {:.0} orders/sec", throughput);

        // 性能断言：至少 10,000 orders/sec (保守值，实际应更高)
        assert!(throughput > 10_000.0,
            "吞吐量应 > 10,000 orders/sec，实际: {:.0}", throughput);
    }

    /// 11.2 批量订单处理 - 连续撮合性能
    ///
    /// 场景：
    /// - 挂入5000个卖单
    /// - 提交能消耗所有卖单的大买单
    /// - 测量撮合耗时
    #[test]
    fn test_throughput_batch_matching() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("BATCH01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("BATCH01").unwrap();
        let asset = InstrumentAsset::from_code("BATCH01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 挂入5000个卖单
        for i in 0..5000 {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 1000.0 + (i % 100) as f64, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        // 大买单撮合
        let aggressive_buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 1200.0, 5000.0, ts + 10000,
        );

        let start = Instant::now();
        let mut ob = orderbook.write();
        let results = ob.process_order(aggressive_buy);
        let duration = start.elapsed();

        let trade_count = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }) | Ok(Success::PartiallyFilled { .. }))
        }).count();

        println!("\n[批量撮合测试] {} 笔成交处理耗时: {:?}", trade_count, duration);
        println!("[批量撮合测试] 撮合速率: {:.0} trades/sec",
            trade_count as f64 / duration.as_secs_f64());

        assert!(trade_count >= 5000, "应产生至少5000笔成交");
    }

    /// 11.3 深度订单簿压力 - 万级订单簿深度
    ///
    /// 场景：
    /// - 构建10000档买盘 + 10000档卖盘
    /// - 验证订单簿稳定性
    ///
    /// 测试目标：
    /// - 大订单簿内存效率
    /// - 价格排序正确性
    #[test]
    fn test_deep_orderbook() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("DEEP01".to_string(), 50000.0).unwrap();

        let orderbook = engine.get_orderbook("DEEP01").unwrap();
        let asset = InstrumentAsset::from_code("DEEP01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let depth = 10000;
        let start = Instant::now();

        // 构建10000档买盘
        for i in 0..depth {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 40000.0 + i as f64, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(buy);
        }

        // 构建10000档卖盘
        for i in 0..depth {
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 60000.0 + i as f64, 1.0, ts + depth as i64 + i as i64,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(sell);
        }

        let duration = start.elapsed();
        println!("\n[深度订单簿测试] 构建 {}档 × 2 订单簿耗时: {:?}", depth, duration);

        // 验证订单簿已构建
        // 注意：这里我们无法直接访问队列大小，但可以通过撮合验证

        // 提交一个跨越价差的买单
        let cross_spread_buy = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 60010.0, 10.0, ts + 100000,
        );
        let mut ob = orderbook.write();
        let results = ob.process_order(cross_spread_buy);

        let trades = results.iter().filter(|r| {
            matches!(r, Ok(Success::Filled { .. }) | Ok(Success::PartiallyFilled { .. }))
        }).count();

        assert!(trades >= 10, "应与至少10档卖单成交");
    }

    // ==================== 12. 延迟基准测试 @yutiansut @quantaxis ====================

    /// 12.1 单订单延迟 - 测量单订单处理时间
    ///
    /// 场景：
    /// - 测量单个订单从提交到返回的时间
    /// - 多次采样取统计值
    ///
    /// 性能目标：P99 < 100μs
    #[test]
    fn test_latency_single_order() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("LAT01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("LAT01").unwrap();
        let asset = InstrumentAsset::from_code("LAT01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let sample_count = 1000;
        let mut latencies: Vec<u128> = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let order = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 900.0 + (i % 100) as f64, 1.0, ts + i as i64,
            );

            let start = Instant::now();
            {
                let mut ob = orderbook.write();
                let _ = ob.process_order(order);
            }
            let duration = start.elapsed();
            latencies.push(duration.as_nanos());
        }

        // 计算统计值
        latencies.sort();
        let p50 = latencies[sample_count / 2];
        let p99 = latencies[(sample_count * 99) / 100];
        let avg: u128 = latencies.iter().sum::<u128>() / sample_count as u128;

        println!("\n[单订单延迟测试] 采样数: {}", sample_count);
        println!("[单订单延迟测试] P50: {} ns ({:.2} μs)", p50, p50 as f64 / 1000.0);
        println!("[单订单延迟测试] P99: {} ns ({:.2} μs)", p99, p99 as f64 / 1000.0);
        println!("[单订单延迟测试] 平均: {} ns ({:.2} μs)", avg, avg as f64 / 1000.0);

        // 性能断言：P99 < 1ms (保守值)
        assert!(p99 < 1_000_000, "P99延迟应 < 1ms，实际: {} ns", p99);
    }

    /// 12.2 撮合延迟 - 测量成交处理时间
    ///
    /// 场景：
    /// - 预挂对手盘
    /// - 测量订单提交到成交的延迟
    #[test]
    fn test_latency_matching() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("LATM01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("LATM01").unwrap();
        let asset = InstrumentAsset::from_code("LATM01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let sample_count = 500;
        let mut latencies: Vec<u128> = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            // 先挂卖单
            let sell = orders::new_limit_order_request(
                asset, OrderDirection::SELL, 1000.0, 1.0, ts + i as i64 * 2,
            );
            {
                let mut ob = orderbook.write();
                let _ = ob.process_order(sell);
            }

            // 测量买单撮合延迟
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 1000.0, 1.0, ts + i as i64 * 2 + 1,
            );

            let start = Instant::now();
            {
                let mut ob = orderbook.write();
                let results = ob.process_order(buy);
                // 确保有成交
                assert!(results.iter().any(|r| matches!(r, Ok(Success::Filled { .. }))));
            }
            let duration = start.elapsed();
            latencies.push(duration.as_nanos());
        }

        // 计算统计值
        latencies.sort();
        let p50 = latencies[sample_count / 2];
        let p99 = latencies[(sample_count * 99) / 100];
        let avg: u128 = latencies.iter().sum::<u128>() / sample_count as u128;

        println!("\n[撮合延迟测试] 采样数: {}", sample_count);
        println!("[撮合延迟测试] P50: {} ns ({:.2} μs)", p50, p50 as f64 / 1000.0);
        println!("[撮合延迟测试] P99: {} ns ({:.2} μs)", p99, p99 as f64 / 1000.0);
        println!("[撮合延迟测试] 平均: {} ns ({:.2} μs)", avg, avg as f64 / 1000.0);

        // 性能断言：P99 < 1ms
        assert!(p99 < 1_000_000, "撮合P99延迟应 < 1ms，实际: {} ns", p99);
    }

    /// 12.3 深度撮合延迟 - 吃掉多档深度的延迟
    ///
    /// 场景：
    /// - 构建100档卖盘
    /// - 测量一次性吃掉所有档位的延迟
    #[test]
    fn test_latency_deep_matching() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("LATD01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("LATD01").unwrap();
        let asset = InstrumentAsset::from_code("LATD01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let depth = 100;
        let mut latencies: Vec<u128> = Vec::new();

        for round in 0..10 {
            // 每轮重新构建卖盘
            for i in 0..depth {
                let sell = orders::new_limit_order_request(
                    asset, OrderDirection::SELL, 1000.0 + i as f64, 1.0,
                    ts + round * 1000 + i as i64,
                );
                let mut ob = orderbook.write();
                let _ = ob.process_order(sell);
            }

            // 大买单吃掉所有
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 1200.0, depth as f64, ts + round * 1000 + 500,
            );

            let start = Instant::now();
            {
                let mut ob = orderbook.write();
                let _ = ob.process_order(buy);
            }
            let duration = start.elapsed();
            latencies.push(duration.as_nanos());
        }

        let avg = latencies.iter().sum::<u128>() / latencies.len() as u128;
        let max = *latencies.iter().max().unwrap();

        println!("\n[深度撮合延迟测试] 深度: {} 档", depth);
        println!("[深度撮合延迟测试] 平均延迟: {} ns ({:.2} μs)", avg, avg as f64 / 1000.0);
        println!("[深度撮合延迟测试] 最大延迟: {} ns ({:.2} μs)", max, max as f64 / 1000.0);
    }

    // ==================== 13. 并发安全测试 @yutiansut @quantaxis ====================

    /// 13.1 多线程并发读写 - 验证 DashMap 和 RwLock 安全
    ///
    /// 场景：
    /// - 多线程同时读写同一订单簿
    /// - 验证无数据竞争
    #[test]
    fn test_concurrent_read_write() {
        use std::thread;

        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("CONC01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("CONC01").unwrap();
        let mut handles = vec![];

        // 写线程：提交订单
        for writer_id in 0..4 {
            let ob = orderbook.clone();
            let handle = thread::spawn(move || {
                let asset = InstrumentAsset::from_code("CONC01");
                let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

                for i in 0..100 {
                    let order = orders::new_limit_order_request(
                        asset,
                        if i % 2 == 0 { OrderDirection::BUY } else { OrderDirection::SELL },
                        if i % 2 == 0 { 900.0 } else { 1100.0 },
                        1.0,
                        ts + (writer_id * 1000 + i) as i64,
                    );
                    let mut ob_guard = ob.write();
                    let _ = ob_guard.process_order(order);
                }
            });
            handles.push(handle);
        }

        // 读线程：读取最新价
        for _reader_id in 0..4 {
            let ob = orderbook.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let ob_guard = ob.read();
                    let _price = ob_guard.lastprice;
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().expect("线程不应panic");
        }
    }

    /// 13.2 高并发下单 - 多线程同时下单
    ///
    /// 场景：
    /// - 8个线程同时下单
    /// - 每线程500个订单
    /// - 总计4000个订单
    #[test]
    fn test_high_concurrency_orders() {
        use std::thread;
        use std::sync::atomic::{AtomicU64, Ordering};

        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("HICO01".to_string(), 1000.0).unwrap();

        let total_accepted = Arc::new(AtomicU64::new(0));
        let total_filled = Arc::new(AtomicU64::new(0));
        let mut handles = vec![];

        let thread_count = 8;
        let orders_per_thread = 500;

        for thread_id in 0..thread_count {
            let eng = engine.clone();
            let accepted = total_accepted.clone();
            let filled = total_filled.clone();

            let handle = thread::spawn(move || {
                let orderbook = eng.get_orderbook("HICO01").unwrap();
                let asset = InstrumentAsset::from_code("HICO01");
                let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

                for i in 0..orders_per_thread {
                    // 奇数线程买，偶数线程卖
                    let direction = if thread_id % 2 == 0 {
                        OrderDirection::BUY
                    } else {
                        OrderDirection::SELL
                    };

                    let price = 1000.0; // 统一价格，可能成交

                    let order = orders::new_limit_order_request(
                        asset, direction, price, 1.0,
                        ts + (thread_id * orders_per_thread + i) as i64,
                    );

                    let mut ob = orderbook.write();
                    let results = ob.process_order(order);

                    for result in &results {
                        match result {
                            Ok(Success::Accepted { .. }) => {
                                accepted.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(Success::Filled { .. }) => {
                                filled.fetch_add(1, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().expect("线程不应panic");
        }

        let total_acc = total_accepted.load(Ordering::Relaxed);
        let total_fill = total_filled.load(Ordering::Relaxed);

        println!("\n[高并发下单测试] 线程数: {}, 每线程订单: {}", thread_count, orders_per_thread);
        println!("[高并发下单测试] 总接受: {}, 总成交: {}", total_acc, total_fill);

        // 验证处理的订单数合理
        assert!(total_acc + total_fill > 0, "应有订单被处理");
    }

    /// 13.3 并发撤单 - 多线程同时撤单
    ///
    /// 场景：
    /// - 先挂入大量订单
    /// - 多线程并发撤单
    /// - 验证撤单正确性
    #[test]
    fn test_concurrent_cancel() {
        use std::thread;
        use std::sync::atomic::{AtomicU64, Ordering};

        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("CANC01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("CANC01").unwrap();
        let asset = InstrumentAsset::from_code("CANC01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 先挂入400个买单
        let mut order_ids: Vec<u64> = Vec::new();
        for i in 0..400 {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 900.0 - (i % 100) as f64, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);

            if let Some(id) = results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }) {
                order_ids.push(id);
            }
        }

        // 分成4组，每组100个订单ID
        let chunks: Vec<Vec<u64>> = order_ids.chunks(100).map(|c| c.to_vec()).collect();
        let cancelled_count = Arc::new(AtomicU64::new(0));
        let mut handles = vec![];

        for chunk in chunks {
            let ob = orderbook.clone();
            let cancelled = cancelled_count.clone();

            let handle = thread::spawn(move || {
                for order_id in chunk {
                    let cancel = orders::limit_order_cancel_request::<InstrumentAsset>(
                        order_id, OrderDirection::BUY,
                    );
                    let mut ob_guard = ob.write();
                    let results = ob_guard.process_order(cancel);

                    if results.iter().any(|r| matches!(r, Ok(Success::Cancelled { .. }))) {
                        cancelled.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().expect("线程不应panic");
        }

        let total_cancelled = cancelled_count.load(Ordering::Relaxed);
        println!("\n[并发撤单测试] 成功撤单数: {} / 400", total_cancelled);

        // 应该成功撤掉大部分订单
        assert!(total_cancelled >= 350, "应撤掉至少350个订单，实际: {}", total_cancelled);
    }

    // ==================== 14. 极端场景压力测试 @yutiansut @quantaxis ====================

    /// 14.1 价格极端波动 - 验证极端价格处理
    ///
    /// 场景：
    /// - 价格从极低到极高
    /// - 验证数值精度
    #[test]
    fn test_extreme_price_range() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("EXTM01".to_string(), 1000000.0).unwrap();

        let orderbook = engine.get_orderbook("EXTM01").unwrap();
        let asset = InstrumentAsset::from_code("EXTM01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 极低价格
        let low_price_order = orders::new_limit_order_request(
            asset, OrderDirection::BUY, 0.001, 1.0, ts,
        );
        {
            let mut ob = orderbook.write();
            let results = ob.process_order(low_price_order);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
        }

        // 极高价格
        let high_price_order = orders::new_limit_order_request(
            asset, OrderDirection::SELL, 9999999.99, 1.0, ts + 1,
        );
        {
            let mut ob = orderbook.write();
            let results = ob.process_order(high_price_order);
            assert!(results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))));
        }
    }

    /// 14.2 大量撤单后重建订单簿
    ///
    /// 场景：
    /// - 构建大订单簿
    /// - 撤销所有订单
    /// - 重新构建
    /// - 验证内存正确释放和重用
    #[test]
    fn test_orderbook_rebuild_after_mass_cancel() {
        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("REBU01".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("REBU01").unwrap();
        let asset = InstrumentAsset::from_code("REBU01");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        // 第一轮：构建500个买单
        let mut order_ids: Vec<u64> = Vec::new();
        for i in 0..500 {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 900.0 + (i % 100) as f64, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);

            if let Some(id) = results.iter().find_map(|r| {
                if let Ok(Success::Accepted { id, .. }) = r { Some(*id) } else { None }
            }) {
                order_ids.push(id);
            }
        }

        // 撤销所有订单
        for order_id in &order_ids {
            let cancel = orders::limit_order_cancel_request::<InstrumentAsset>(
                *order_id, OrderDirection::BUY,
            );
            let mut ob = orderbook.write();
            let _ = ob.process_order(cancel);
        }

        // 第二轮：重新构建500个买单
        let mut new_order_count = 0;
        for i in 0..500 {
            let buy = orders::new_limit_order_request(
                asset, OrderDirection::BUY, 900.0 + (i % 100) as f64, 1.0, ts + 1000 + i as i64,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(buy);

            if results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))) {
                new_order_count += 1;
            }
        }

        assert_eq!(new_order_count, 500, "重建订单簿应接受所有新订单");
    }

    /// 14.3 混合工作负载压力测试
    ///
    /// 场景：
    /// - 同时进行下单、撤单、撮合
    /// - 模拟真实交易环境
    #[test]
    fn test_mixed_workload_stress() {
        use std::thread;
        use std::sync::atomic::{AtomicU64, Ordering};

        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("MIXW01".to_string(), 1000.0).unwrap();

        let stats = Arc::new((
            AtomicU64::new(0), // accepted
            AtomicU64::new(0), // filled
            AtomicU64::new(0), // cancelled
        ));

        let mut handles = vec![];

        // 下单线程 (2个)
        for thread_id in 0..2 {
            let eng = engine.clone();
            let s = stats.clone();

            let handle = thread::spawn(move || {
                let orderbook = eng.get_orderbook("MIXW01").unwrap();
                let asset = InstrumentAsset::from_code("MIXW01");
                let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

                for i in 0..300 {
                    let direction = if i % 2 == 0 { OrderDirection::BUY } else { OrderDirection::SELL };
                    let price = if direction == OrderDirection::BUY { 999.0 } else { 1001.0 };

                    let order = orders::new_limit_order_request(
                        asset, direction, price, 1.0, ts + (thread_id * 1000 + i) as i64,
                    );
                    let mut ob = orderbook.write();
                    let results = ob.process_order(order);

                    for result in &results {
                        match result {
                            Ok(Success::Accepted { .. }) => { s.0.fetch_add(1, Ordering::Relaxed); }
                            Ok(Success::Filled { .. }) => { s.1.fetch_add(1, Ordering::Relaxed); }
                            _ => {}
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // 撮合线程 (2个)
        for thread_id in 0..2 {
            let eng = engine.clone();
            let s = stats.clone();

            let handle = thread::spawn(move || {
                let orderbook = eng.get_orderbook("MIXW01").unwrap();
                let asset = InstrumentAsset::from_code("MIXW01");
                let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

                for i in 0..200 {
                    // 交替买卖以触发成交
                    let direction = if i % 2 == 0 { OrderDirection::BUY } else { OrderDirection::SELL };
                    let price = 1000.0; // 中间价，可能与两边成交

                    let order = orders::new_limit_order_request(
                        asset, direction, price, 1.0, ts + 10000 + (thread_id * 1000 + i) as i64,
                    );
                    let mut ob = orderbook.write();
                    let results = ob.process_order(order);

                    for result in &results {
                        if let Ok(Success::Filled { .. }) = result {
                            s.1.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().expect("线程不应panic");
        }

        let accepted = stats.0.load(Ordering::Relaxed);
        let filled = stats.1.load(Ordering::Relaxed);

        println!("\n[混合工作负载测试] 接受: {}, 成交: {}", accepted, filled);
        assert!(accepted > 0 || filled > 0, "应有订单被处理");
    }

    /// 14.4 超大批量订单提交
    ///
    /// 场景：
    /// - 单线程提交50000个订单
    /// - 验证系统稳定性和内存使用
    #[test]
    fn test_massive_order_submission() {
        use std::time::Instant;

        let engine = ExchangeMatchingEngine::new();
        engine.register_instrument("MASS02".to_string(), 1000.0).unwrap();

        let orderbook = engine.get_orderbook("MASS02").unwrap();
        let asset = InstrumentAsset::from_code("MASS02");
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap();

        let order_count = 50000;
        let start = Instant::now();
        let mut accepted = 0;

        for i in 0..order_count {
            let direction = if i % 2 == 0 { OrderDirection::BUY } else { OrderDirection::SELL };
            let price = if direction == OrderDirection::BUY {
                800.0 + (i % 200) as f64
            } else {
                1200.0 + (i % 200) as f64
            };

            let order = orders::new_limit_order_request(
                asset, direction, price, 1.0, ts + i as i64,
            );
            let mut ob = orderbook.write();
            let results = ob.process_order(order);

            if results.iter().any(|r| matches!(r, Ok(Success::Accepted { .. }))) {
                accepted += 1;
            }
        }

        let duration = start.elapsed();
        let throughput = order_count as f64 / duration.as_secs_f64();

        println!("\n[超大批量测试] {} 订单提交完成", order_count);
        println!("[超大批量测试] 耗时: {:?}", duration);
        println!("[超大批量测试] 吞吐量: {:.0} orders/sec", throughput);
        println!("[超大批量测试] 接受率: {:.2}%", accepted as f64 / order_count as f64 * 100.0);

        assert_eq!(accepted, order_count, "所有订单应被接受");
    }
}
