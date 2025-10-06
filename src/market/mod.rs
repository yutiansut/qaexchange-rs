//! 市场数据服务模块
//!
//! 提供市场数据的业务逻辑，包括订单簿查询、行情数据、成交数据等
//! 遵循解耦原则：业务逻辑与网络层分离

pub mod broadcaster;
pub mod snapshot_broadcaster;
pub mod cache;
pub mod recovery;

use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use serde::{Serialize, Deserialize};

use crate::matching::engine::ExchangeMatchingEngine;
use crate::utils::config::InstrumentConfig;
use crate::ExchangeError;

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// 订单簿快照（买卖五档）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub instrument_id: String,
    pub timestamp: i64,
    pub bids: Vec<PriceLevel>,  // 买盘（降序）
    pub asks: Vec<PriceLevel>,  // 卖盘（升序）
    pub last_price: Option<f64>,
}

/// 价格档位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub volume: i64,
}

/// 最新成交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTrade {
    pub trade_id: String,
    pub instrument_id: String,
    pub price: f64,
    pub volume: i64,
    pub timestamp: i64,
    pub direction: String,  // "BUY" or "SELL"
}

/// 合约信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentInfo {
    pub instrument_id: String,
    pub name: String,
    pub multiplier: f64,
    pub tick_size: f64,
    pub last_price: Option<f64>,
    pub status: String,
}

/// Tick 行情数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickData {
    pub instrument_id: String,
    pub timestamp: i64,
    pub last_price: f64,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub volume: i64,
}

/// 市场数据服务（业务逻辑层）
#[derive(Clone)]
pub struct MarketDataService {
    matching_engine: Arc<ExchangeMatchingEngine>,
    cache: Arc<cache::MarketDataCache>,
    instrument_configs: HashMap<String, InstrumentConfig>,
    /// WAL 存储（用于恢复历史行情数据）
    storage: Option<Arc<crate::storage::hybrid::OltpHybridStorage>>,
    /// iceoryx2 管理器（零拷贝 IPC）
    iceoryx_manager: Option<Arc<RwLock<crate::ipc::IceoryxManager>>>,
}

impl MarketDataService {
    /// 创建市场数据服务
    pub fn new(matching_engine: Arc<ExchangeMatchingEngine>) -> Self {
        Self {
            matching_engine,
            cache: Arc::new(cache::MarketDataCache::new(100)), // 100ms TTL
            instrument_configs: HashMap::new(),
            storage: None,
            iceoryx_manager: None,
        }
    }

    /// 设置存储（用于从 WAL 恢复数据）
    pub fn with_storage(mut self, storage: Arc<crate::storage::hybrid::OltpHybridStorage>) -> Self {
        self.storage = Some(storage.clone());

        // 从WAL恢复最近10分钟的市场数据到缓存
        if let Err(e) = self.recover_recent_market_data(10) {
            log::warn!("Failed to recover market data from WAL: {}", e);
        }

        self
    }

    /// 设置 iceoryx2 管理器（零拷贝 IPC）
    pub fn with_iceoryx(mut self, manager: Arc<RwLock<crate::ipc::IceoryxManager>>) -> Self {
        self.iceoryx_manager = Some(manager);
        log::info!("✅ Market data service: iceoryx2 enabled");
        self
    }

    /// 从WAL恢复最近N分钟的市场数据
    pub fn recover_recent_market_data(&self, minutes: i64) -> Result<()> {
        if let Some(ref storage) = self.storage {
            let recovery = recovery::MarketDataRecovery::new(storage.clone(), self.cache.clone());
            match recovery.recover_recent_minutes(minutes) {
                Ok(stats) if stats.tick_records > 0 || stats.orderbook_records > 0 => {
                    log::info!("✅ [Market Data Recovery] Recovered {} ticks, {} orderbooks in {}ms",
                        stats.tick_records, stats.orderbook_records, stats.recovery_time_ms);
                }
                Ok(_) => {
                    log::debug!("[Market Data Recovery] No recent market data found in WAL");
                }
                Err(e) => {
                    log::warn!("[Market Data Recovery] Failed: {}", e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// 设置合约配置
    pub fn set_instrument_configs(&mut self, configs: Vec<InstrumentConfig>) {
        for config in configs {
            self.instrument_configs.insert(config.instrument_id.clone(), config);
        }
    }

    /// 创建带自定义缓存 TTL 的服务
    pub fn new_with_cache_ttl(matching_engine: Arc<ExchangeMatchingEngine>, cache_ttl_ms: u64) -> Self {
        Self {
            matching_engine,
            cache: Arc::new(cache::MarketDataCache::new(cache_ttl_ms)),
            instrument_configs: HashMap::new(),
            storage: None,
            iceoryx_manager: None,
        }
    }

    /// 创建带配置的市场数据服务
    pub fn new_with_configs(
        matching_engine: Arc<ExchangeMatchingEngine>,
        configs: Vec<InstrumentConfig>,
    ) -> Self {
        let mut service = Self::new(matching_engine);
        service.set_instrument_configs(configs);
        service
    }

    /// 获取合约配置
    pub fn get_instrument_config(&self, instrument_id: &str) -> Option<&InstrumentConfig> {
        self.instrument_configs.get(instrument_id)
    }

    /// 获取缓存统计
    pub fn get_cache_stats(&self) -> cache::CacheStatsSnapshot {
        self.cache.get_stats()
    }

    /// 获取订单簿快照（买卖五档）
    pub fn get_orderbook_snapshot(&self, instrument_id: &str, depth: usize) -> Result<OrderBookSnapshot> {
        log::debug!("📊 [MarketData] get_orderbook_snapshot for {} (depth={})", instrument_id, depth);

        // L1 缓存查询
        if let Some(snapshot) = self.cache.get_orderbook(instrument_id) {
            log::debug!("✅ [L1 Cache] Hit for orderbook {}", instrument_id);
            return Ok(snapshot);
        }
        log::debug!("❌ [L1 Cache] Miss for orderbook {}", instrument_id);

        // L2 缓存查询：从 WAL 恢复最近的快照
        if let Some(ref storage) = self.storage {
            log::debug!("🔍 [L2 Storage] Querying WAL for orderbook {}", instrument_id);
            match self.load_orderbook_from_storage(instrument_id) {
                Ok(snapshot) => {
                    log::info!("✅ [L2 Storage] Found orderbook {} in WAL: {} bids, {} asks",
                        instrument_id, snapshot.bids.len(), snapshot.asks.len());
                    // 更新缓存
                    self.cache.update_orderbook(instrument_id.to_string(), snapshot.clone());
                    return Ok(snapshot);
                }
                Err(e) => {
                    log::debug!("❌ [L2 Storage] Not found in WAL: {}", e);
                }
            }
        } else {
            log::debug!("⚠️  [L2 Storage] Storage not configured");
        }

        // L3 缓存未命中，从 Orderbook 实时计算
        log::debug!("🔍 [L3 Realtime] Computing orderbook from matching engine for {}", instrument_id);
        let engine = &self.matching_engine;

        // 获取指定合约的订单簿
        let orderbook = engine.get_orderbook(instrument_id)
            .ok_or_else(|| ExchangeError::MatchingError(format!("Instrument not found: {}", instrument_id)))?;

        let ob = orderbook.read();

        // 获取买盘（降序排列）
        let bids = if let Some(bid_orders) = ob.bid_queue.get_sorted_orders() {
            use std::collections::HashMap;
            let mut price_map: HashMap<String, f64> = HashMap::new();
            for order in bid_orders.iter().take(depth * 10) {  // 获取足够的订单以聚合
                *price_map.entry(order.price.to_string()).or_insert(0.0) += order.volume;
            }

            let mut levels: Vec<PriceLevel> = price_map.into_iter()
                .filter_map(|(price_str, volume)| {
                    price_str.parse::<f64>().ok().map(|price| PriceLevel {
                        price,
                        volume: volume as i64,
                    })
                })
                .collect();
            levels.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
            levels.truncate(depth);
            levels
        } else {
            Vec::new()
        };

        // 获取卖盘（升序排列）
        let asks = if let Some(ask_orders) = ob.ask_queue.get_sorted_orders() {
            use std::collections::HashMap;
            let mut price_map: HashMap<String, f64> = HashMap::new();
            for order in ask_orders.iter().take(depth * 10) {
                *price_map.entry(order.price.to_string()).or_insert(0.0) += order.volume;
            }

            let mut levels: Vec<PriceLevel> = price_map.into_iter()
                .filter_map(|(price_str, volume)| {
                    price_str.parse::<f64>().ok().map(|price| PriceLevel {
                        price,
                        volume: volume as i64,
                    })
                })
                .collect();
            levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
            levels.truncate(depth);
            levels
        } else {
            Vec::new()
        };

        // 获取最新成交价
        let last_price = Some(ob.lastprice);

        let snapshot = OrderBookSnapshot {
            instrument_id: instrument_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            bids,
            asks,
            last_price,
        };

        // 更新 L1 缓存
        self.cache.update_orderbook(instrument_id.to_string(), snapshot.clone());

        Ok(snapshot)
    }

    /// 获取合约列表
    pub fn get_instruments(&self) -> Result<Vec<InstrumentInfo>> {
        let engine = &self.matching_engine;
        let instruments = engine.get_instruments();

        let mut result = Vec::new();
        for instrument_id in instruments {
            let last_price = engine.get_last_price(&instrument_id);

            // 从配置读取合约信息，如果配置不存在则使用默认值
            let (name, multiplier, tick_size) = if let Some(config) = self.instrument_configs.get(&instrument_id) {
                (config.name.clone(), config.multiplier, config.tick_size)
            } else {
                (format!("{} 期货", instrument_id), 300.0, 0.2)
            };

            result.push(InstrumentInfo {
                instrument_id: instrument_id.clone(),
                name,
                multiplier,
                tick_size,
                last_price,
                status: "Trading".to_string(),
            });
        }

        Ok(result)
    }

    /// 获取指定合约的 Tick 数据
    pub fn get_tick_data(&self, instrument_id: &str) -> Result<TickData> {
        log::debug!("📊 [MarketData] get_tick_data for {}", instrument_id);

        // L1 缓存查询
        if let Some(tick) = self.cache.get_tick(instrument_id) {
            log::debug!("✅ [L1 Cache] Hit for tick {}", instrument_id);
            return Ok(tick);
        }
        log::debug!("❌ [L1 Cache] Miss for tick {}", instrument_id);

        // L2 从 WAL 恢复最近的 Tick
        if let Some(ref storage) = self.storage {
            log::debug!("🔍 [L2 Storage] Querying WAL for tick {}", instrument_id);
            match self.load_tick_from_storage(instrument_id) {
                Ok(tick) => {
                    log::info!("✅ [L2 Storage] Found tick {} in WAL: price={}", instrument_id, tick.last_price);
                    // 更新缓存
                    self.cache.update_tick(instrument_id.to_string(), tick.clone());
                    return Ok(tick);
                }
                Err(e) => {
                    log::debug!("❌ [L2 Storage] Not found in WAL: {}", e);
                }
            }
        } else {
            log::debug!("⚠️  [L2 Storage] Storage not configured");
        }

        // L3 缓存未命中，从 Orderbook 实时计算
        log::debug!("🔍 [L3 Realtime] Computing tick from orderbook for {}", instrument_id);
        let engine = &self.matching_engine;

        // 检查合约是否存在
        let orderbook = engine.get_orderbook(instrument_id)
            .ok_or_else(|| ExchangeError::MatchingError(format!("Instrument not found: {}", instrument_id)))?;

        let ob = orderbook.read();

        // 获取最新成交价
        let last_price = ob.lastprice;

        // 获取最优买卖价（从排序订单中获取第一个）
        let bid_price = ob.bid_queue.get_sorted_orders()
            .and_then(|orders| orders.first().map(|o| o.price));

        let ask_price = ob.ask_queue.get_sorted_orders()
            .and_then(|orders| orders.first().map(|o| o.price));

        // TODO: 从成交记录获取成交量
        let volume = 0;

        let tick = TickData {
            instrument_id: instrument_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            last_price,
            bid_price,
            ask_price,
            volume,
        };

        // 更新 L1 缓存
        self.cache.update_tick(instrument_id.to_string(), tick.clone());

        Ok(tick)
    }

    /// 获取最近成交记录
    pub fn get_recent_trades(&self, instrument_id: &str, limit: usize) -> Result<Vec<RecentTrade>> {
        let trade_recorder = self.matching_engine.get_trade_recorder();
        let trade_records = trade_recorder.get_trades_by_instrument(instrument_id);

        // 按时间降序排序，取最新的 limit 条
        let mut recent_trades: Vec<RecentTrade> = trade_records
            .into_iter()
            .map(|record| RecentTrade {
                trade_id: record.trade_id,
                instrument_id: record.instrument_id,
                price: record.price,
                volume: record.volume as i64,
                timestamp: record.timestamp,
                // 根据买卖方向确定成交方向（这里简化处理，可以根据实际需求调整）
                direction: "TRADE".to_string(),
            })
            .collect();

        recent_trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        recent_trades.truncate(limit);

        Ok(recent_trades)
    }

    /// 从storage加载最近的TickData（私有方法）
    fn load_tick_from_storage(&self, instrument_id: &str) -> Result<TickData> {
        if let Some(ref storage) = self.storage {
            // 查询最近1小时的数据
            let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let start_ts = end_ts - (3600 * 1_000_000_000); // 1小时

            log::debug!("📂 [Storage] range_query for tick: {} - {} (1 hour)", start_ts, end_ts);

            let records = storage.range_query(start_ts, end_ts)
                .map_err(|e| ExchangeError::InternalError(format!("Failed to query WAL: {}", e)))?;

            log::debug!("📂 [Storage] Found {} total records in range", records.len());

            // 从后往前找最新的TickData
            let mut tick_count = 0;
            for (_ts, _seq, record) in records.iter().rev() {
                if let crate::storage::wal::record::WalRecord::TickData {
                    instrument_id: inst_id,
                    last_price,
                    bid_price,
                    ask_price,
                    volume,
                    timestamp,
                } = record {
                    tick_count += 1;
                    let inst_str = crate::storage::wal::record::WalRecord::from_fixed_array(inst_id);
                    if inst_str == instrument_id {
                        log::debug!("✅ [Storage] Found TickData #{} for {}: price={}", tick_count, inst_str, last_price);
                        return Ok(TickData {
                            instrument_id: inst_str,
                            timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                            last_price: *last_price,
                            bid_price: if *bid_price > 0.0 { Some(*bid_price) } else { None },
                            ask_price: if *ask_price > 0.0 { Some(*ask_price) } else { None },
                            volume: *volume,
                        });
                    }
                }
            }

            log::debug!("❌ [Storage] No TickData found for {} (scanned {} tick records)", instrument_id, tick_count);
        }

        Err(ExchangeError::StorageError(format!("No tick data found for {}", instrument_id)))
    }

    /// 从storage加载最近的OrderBookSnapshot（私有方法）
    fn load_orderbook_from_storage(&self, instrument_id: &str) -> Result<OrderBookSnapshot> {
        if let Some(ref storage) = self.storage {
            // 查询最近1小时的数据
            let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let start_ts = end_ts - (3600 * 1_000_000_000); // 1小时

            let records = storage.range_query(start_ts, end_ts)
                .map_err(|e| ExchangeError::InternalError(format!("Failed to query WAL: {}", e)))?;

            // 从后往前找最新的OrderBookSnapshot
            for (_ts, _seq, record) in records.iter().rev() {
                if let crate::storage::wal::record::WalRecord::OrderBookSnapshot {
                    instrument_id: inst_id,
                    bids,
                    asks,
                    last_price,
                    timestamp,
                } = record {
                    let inst_str = crate::storage::wal::record::WalRecord::from_fixed_array(inst_id);
                    if inst_str == instrument_id {
                        // 转换固定数组为Vec
                        let bids_vec: Vec<PriceLevel> = bids.iter()
                            .filter(|(price, _)| *price > 0.0)
                            .map(|(price, volume)| PriceLevel {
                                price: *price,
                                volume: *volume
                            })
                            .collect();

                        let asks_vec: Vec<PriceLevel> = asks.iter()
                            .filter(|(price, _)| *price > 0.0)
                            .map(|(price, volume)| PriceLevel {
                                price: *price,
                                volume: *volume
                            })
                            .collect();

                        return Ok(OrderBookSnapshot {
                            instrument_id: inst_str,
                            timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                            bids: bids_vec,
                            asks: asks_vec,
                            last_price: if *last_price > 0.0 { Some(*last_price) } else { None },
                        });
                    }
                }
            }
        }

        Err(ExchangeError::StorageError(format!("No orderbook snapshot found for {}", instrument_id)))
    }

    /// 获取所有市场的订单统计（管理员功能）
    pub fn get_market_order_stats(&self) -> Result<serde_json::Value> {
        let engine = &self.matching_engine;
        let instruments = engine.get_instruments();

        let mut total_orders = 0;
        let mut total_bids = 0;
        let mut total_asks = 0;

        for instrument_id in instruments {
            if let Some(orderbook) = engine.get_orderbook(&instrument_id) {
                let ob = orderbook.read();

                let bid_count = ob.bid_queue.get_sorted_orders().map(|v| v.len()).unwrap_or(0);
                let ask_count = ob.ask_queue.get_sorted_orders().map(|v| v.len()).unwrap_or(0);

                total_bids += bid_count;
                total_asks += ask_count;
                total_orders += bid_count + ask_count;
            }
        }

        Ok(serde_json::json!({
            "total_orders": total_orders,
            "total_bids": total_bids,
            "total_asks": total_asks,
        }))
    }
}

// 重新导出
pub use broadcaster::{MarketDataBroadcaster, MarketDataEvent};
pub use snapshot_broadcaster::SnapshotBroadcastService;
pub use cache::{MarketDataCache, CacheStatsSnapshot};
pub use recovery::{MarketDataRecovery, RecoveredMarketData, RecoveryStats};
