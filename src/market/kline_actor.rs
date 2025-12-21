//! K线聚合Actor
//!
//! 独立的Actix Actor，负责K线实时聚合和历史查询
//! 订阅MarketDataBroadcaster的tick事件，实现分级采样
//! 支持持久化和恢复
//!
//! ## 方案A: 集成因子引擎 (2025-12-16)
//!
//! K线完成时自动触发因子计算并广播:
//! - StreamFactorEngine 增量计算 MA, EMA, RSI, MACD 等
//! - 广播 FactorUpdate 事件给 WebSocket 订阅者
//!
//! @yutiansut @quantaxis

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::kline::{KLine, KLineAggregator, KLinePeriod};
use super::MarketDataBroadcaster;
use super::MarketDataEvent;
use crate::factor::{FactorRegistry, StreamFactorEngine};
use crate::storage::wal::{WalManager, WalRecord};

/// K线Actor - 独立处理K线聚合，避免阻塞交易流程
///
/// ## 方案A集成因子引擎
///
/// K线完成时自动触发因子计算:
/// ```text
/// Tick → KLineAggregator → KLineFinished → StreamFactorEngine → FactorUpdate
/// ```
pub struct KLineActor {
    /// 各合约的K线聚合器
    aggregators: Arc<RwLock<HashMap<String, KLineAggregator>>>,

    /// 市场数据广播器（用于订阅tick和推送K线完成事件）
    broadcaster: Arc<MarketDataBroadcaster>,

    /// 订阅的合约列表（为空表示订阅所有合约）
    subscribed_instruments: Vec<String>,

    /// WAL管理器（用于K线持久化和恢复）
    wal_manager: Arc<WalManager>,

    /// ✨ 方案A: 各合约的流式因子引擎
    /// @yutiansut @quantaxis
    factor_engines: Arc<RwLock<HashMap<String, StreamFactorEngine>>>,

    /// 启用的因子列表 (默认: ma5, ma10, ma20, ema12, ema26, rsi14, macd)
    enabled_factors: Vec<String>,

    /// 是否启用因子计算
    enable_factor_compute: bool,
}

/// 默认启用的因子列表
/// @yutiansut @quantaxis
const DEFAULT_FACTORS: &[&str] = &["ma5", "ma10", "ma20", "ema12", "ema26", "rsi14", "macd"];

impl KLineActor {
    /// 创建新的K线Actor
    pub fn new(broadcaster: Arc<MarketDataBroadcaster>, wal_manager: Arc<WalManager>) -> Self {
        Self {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            broadcaster,
            subscribed_instruments: Vec::new(), // 默认订阅所有
            wal_manager,
            factor_engines: Arc::new(RwLock::new(HashMap::new())),
            enabled_factors: DEFAULT_FACTORS.iter().map(|s| s.to_string()).collect(),
            enable_factor_compute: false, // 默认关闭，需要显式启用
        }
    }

    /// 订阅指定合约列表
    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.subscribed_instruments = instruments;
        self
    }

    /// ✨ 方案A: 启用因子计算
    /// @yutiansut @quantaxis
    ///
    /// # Example
    /// ```ignore
    /// let actor = KLineActor::new(broadcaster, wal_manager)
    ///     .with_factor_compute(true)
    ///     .with_factors(vec!["ma5", "rsi14", "macd"]);
    /// ```
    pub fn with_factor_compute(mut self, enable: bool) -> Self {
        self.enable_factor_compute = enable;
        if enable {
            log::info!("📈 [KLineActor] Factor computation enabled with factors: {:?}", self.enabled_factors);
        }
        self
    }

    /// 设置启用的因子列表
    pub fn with_factors(mut self, factors: Vec<&str>) -> Self {
        self.enabled_factors = factors.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 获取或创建合约的因子引擎
    /// @yutiansut @quantaxis
    fn get_or_create_factor_engine(&self, instrument_id: &str) -> Option<()> {
        if !self.enable_factor_compute {
            return None;
        }

        let mut engines = self.factor_engines.write();
        if !engines.contains_key(instrument_id) {
            // 创建新的因子引擎
            let registry = FactorRegistry::with_standard_factors();
            let mut engine = StreamFactorEngine::new(registry);

            // 初始化所有启用的因子
            for factor_id in &self.enabled_factors {
                if let Err(e) = engine.init_factor(factor_id) {
                    log::warn!(
                        "📈 [KLineActor] Failed to init factor {} for {}: {}",
                        factor_id, instrument_id, e
                    );
                }
            }

            engines.insert(instrument_id.to_string(), engine);
            log::info!(
                "📈 [KLineActor] Created factor engine for {} with {} factors",
                instrument_id, self.enabled_factors.len()
            );
        }

        Some(())
    }

    /// 更新因子并返回因子值映射
    /// @yutiansut @quantaxis
    fn update_factors(&self, instrument_id: &str, close_price: f64) -> HashMap<String, f64> {
        if !self.enable_factor_compute {
            return HashMap::new();
        }

        // 确保因子引擎已创建
        self.get_or_create_factor_engine(instrument_id);

        let mut engines = self.factor_engines.write();
        if let Some(engine) = engines.get_mut(instrument_id) {
            // 使用收盘价更新所有因子
            let factor_ids: Vec<&str> = self.enabled_factors.iter().map(|s| s.as_str()).collect();
            engine.update_all(close_price, &factor_ids)
        } else {
            HashMap::new()
        }
    }

    /// 从WAL恢复历史K线数据
    fn recover_from_wal(&self) {
        log::info!("📊 [KLineActor] Recovering K-line data from WAL...");

        let mut recovered_count = 0;
        let mut error_count = 0;

        // 使用WAL的replay方法遍历所有记录
        let result = self.wal_manager.replay(|entry| {
            // 只处理KLineFinished记录
            if let WalRecord::KLineFinished {
                instrument_id,
                period,
                kline_timestamp,
                open,
                high,
                low,
                close,
                volume,
                amount,
                open_oi,
                close_oi,
                ..
            } = &entry.record
            {
                // 转换instrument_id
                let instrument_id_str = WalRecord::from_fixed_array(instrument_id);

                // 转换period
                if let Some(kline_period) = super::kline::KLinePeriod::from_int(*period) {
                    // 重建K线数据
                    let kline = super::kline::KLine {
                        timestamp: *kline_timestamp,
                        open: *open,
                        high: *high,
                        low: *low,
                        close: *close,
                        volume: *volume,
                        amount: *amount,
                        open_oi: *open_oi,
                        close_oi: *close_oi,
                        is_finished: true,
                    };

                    // 添加到aggregators的历史K线
                    let mut agg_map = self.aggregators.write();
                    let aggregator =
                        agg_map.entry(instrument_id_str.clone()).or_insert_with(|| {
                            super::kline::KLineAggregator::new(instrument_id_str.clone())
                        });

                    // 添加到历史K线（保持max_history限制）
                    let history = aggregator
                        .history_klines
                        .entry(kline_period)
                        .or_default();

                    history.push(kline);

                    // 限制历史数量
                    if history.len() > aggregator.max_history {
                        history.remove(0);
                    }

                    recovered_count += 1;

                    if recovered_count % 1000 == 0 {
                        log::debug!("📊 [KLineActor] Recovered {} K-lines...", recovered_count);
                    }
                } else {
                    log::warn!("📊 [KLineActor] Unknown K-line period: {}", period);
                    error_count += 1;
                }
            }

            Ok(())
        });

        match result {
            Ok(_) => {
                log::info!(
                    "📊 [KLineActor] WAL recovery completed: {} K-lines recovered, {} errors",
                    recovered_count,
                    error_count
                );
            }
            Err(e) => {
                log::error!("📊 [KLineActor] WAL recovery failed: {}", e);
            }
        }
    }
}

impl Actor for KLineActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("📊 [KLineActor] Starting K-line aggregator...");

        // 从WAL恢复历史数据
        self.recover_from_wal();

        // 订阅市场数据的tick频道
        let subscriber_id = uuid::Uuid::new_v4().to_string();
        let receiver = self.broadcaster.subscribe(
            subscriber_id.clone(),
            self.subscribed_instruments.clone(), // 空列表表示订阅所有合约
            vec!["tick".to_string()],            // 只订阅tick事件
        );

        // 启动异步任务持续接收tick事件
        let aggregators = self.aggregators.clone();
        let broadcaster = self.broadcaster.clone();
        let wal_manager = self.wal_manager.clone();
        // ✨ 方案A: 因子引擎相关数据
        let factor_engines = self.factor_engines.clone();
        let enabled_factors = self.enabled_factors.clone();
        let enable_factor_compute = self.enable_factor_compute;
        let _addr = ctx.address();

        let fut = async move {
            log::info!(
                "📊 [KLineActor] Subscribed to tick events (subscriber_id={})",
                subscriber_id
            );

            if enable_factor_compute {
                log::info!(
                    "📈 [KLineActor] Factor computation enabled with factors: {:?}",
                    enabled_factors
                );
            }

            loop {
                // 使用spawn_blocking避免阻塞Tokio执行器
                let receiver_clone = receiver.clone();
                match tokio::task::spawn_blocking(move || receiver_clone.recv()).await {
                    Ok(Ok(event)) => {
                        // 处理tick事件
                        if let MarketDataEvent::Tick {
                            instrument_id,
                            price,
                            volume,
                            timestamp,
                            ..
                        } = event
                        {
                            log::debug!(
                                "📊 [KLineActor] Received tick event: instrument={}, price={}, volume={}, ts={}",
                                instrument_id, price, volume, timestamp
                            );
                            let mut agg_map = aggregators.write();
                            let aggregator = agg_map
                                .entry(instrument_id.clone())
                                .or_insert_with(|| KLineAggregator::new(instrument_id.clone()));

                            // 聚合K线
                            let finished_klines =
                                aggregator.on_tick(price, volume as i64, timestamp);

                            // 广播完成的K线
                            for (period, kline) in finished_klines {
                                log::debug!(
                                    "📊 [KLineActor] Finished {} {:?} K-line: O={:.2} H={:.2} L={:.2} C={:.2} V={}",
                                    instrument_id, period, kline.open, kline.high, kline.low, kline.close, kline.volume
                                );

                                // 广播K线完成事件
                                broadcaster.broadcast(MarketDataEvent::KLineFinished {
                                    instrument_id: instrument_id.clone(),
                                    period: period.to_int(),
                                    kline: kline.clone(),
                                    timestamp,
                                });

                                // ✨ 方案A: K线完成时计算因子并广播
                                // @yutiansut @quantaxis
                                if enable_factor_compute {
                                    // 确保因子引擎已创建
                                    let mut engines = factor_engines.write();
                                    if !engines.contains_key(&instrument_id) {
                                        let registry = FactorRegistry::with_standard_factors();
                                        let mut engine = StreamFactorEngine::new(registry);
                                        for factor_id in &enabled_factors {
                                            if let Err(e) = engine.init_factor(factor_id) {
                                                log::warn!(
                                                    "📈 [KLineActor] Failed to init factor {}: {}",
                                                    factor_id, e
                                                );
                                            }
                                        }
                                        engines.insert(instrument_id.clone(), engine);
                                        log::info!(
                                            "📈 [KLineActor] Created factor engine for {}",
                                            instrument_id
                                        );
                                    }

                                    // 使用收盘价更新因子
                                    if let Some(engine) = engines.get_mut(&instrument_id) {
                                        let factor_ids: Vec<&str> = enabled_factors.iter().map(|s| s.as_str()).collect();
                                        let factor_values = engine.update_all(kline.close, &factor_ids);

                                        if !factor_values.is_empty() {
                                            log::debug!(
                                                "📈 [KLineActor] Factor update for {}: {:?}",
                                                instrument_id, factor_values
                                            );

                                            // 广播因子更新事件
                                            broadcaster.broadcast(MarketDataEvent::FactorUpdate {
                                                instrument_id: instrument_id.clone(),
                                                factors: factor_values,
                                                period: period.to_int(),
                                                timestamp,
                                            });
                                        }
                                    }
                                }

                                // 持久化K线到WAL
                                let wal_record = WalRecord::KLineFinished {
                                    instrument_id: WalRecord::to_fixed_array_16(&instrument_id),
                                    period: period.to_int(),
                                    kline_timestamp: kline.timestamp,
                                    open: kline.open,
                                    high: kline.high,
                                    low: kline.low,
                                    close: kline.close,
                                    volume: kline.volume,
                                    amount: kline.amount,
                                    open_oi: kline.open_oi,
                                    close_oi: kline.close_oi,
                                    timestamp: chrono::Utc::now()
                                        .timestamp_nanos_opt()
                                        .unwrap_or(0),
                                };

                                if let Err(e) = wal_manager.append(wal_record) {
                                    log::error!(
                                        "📊 [KLineActor] Failed to persist K-line to WAL: {}",
                                        e
                                    );
                                } else {
                                    log::trace!(
                                        "📊 [KLineActor] K-line persisted to WAL: {} {:?}",
                                        instrument_id,
                                        period
                                    );
                                }
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        log::warn!("📊 [KLineActor] Market data channel disconnected");
                        break;
                    }
                    Err(e) => {
                        log::error!("📊 [KLineActor] spawn_blocking error: {}", e);
                        break;
                    }
                }
            }

            log::info!("📊 [KLineActor] Tick processing task ended");
        };

        ctx.spawn(actix::fut::wrap_future(fut));

        log::info!("📊 [KLineActor] Started successfully");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("📊 [KLineActor] Stopped");
    }
}

/// 查询K线消息 - 用于HTTP API和DIFF set_chart
#[derive(Message)]
#[rtype(result = "Vec<KLine>")]
pub struct GetKLines {
    pub instrument_id: String,
    pub period: KLinePeriod,
    pub count: usize,
}

/// 从合约ID中提取基础合约代码（去除交易所前缀）
/// @yutiansut @quantaxis
///
/// 支持两种格式：
/// - "CFFEX.IF2501" -> "IF2501"
/// - "IF2501" -> "IF2501"
fn extract_base_instrument_id(instrument_id: &str) -> &str {
    if let Some(pos) = instrument_id.find('.') {
        &instrument_id[pos + 1..]
    } else {
        instrument_id
    }
}

impl Handler<GetKLines> for KLineActor {
    type Result = Vec<KLine>;

    /// ✨ 增强调试日志 @yutiansut @quantaxis
    fn handle(&mut self, msg: GetKLines, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        log::info!(
            "📊 [KLineActor GetKLines] Query received: instrument={}, period={:?}, count={}, available_instruments={:?}",
            msg.instrument_id, msg.period, msg.count,
            aggregators.keys().collect::<Vec<_>>()
        );

        // 首先尝试直接匹配
        if let Some(aggregator) = aggregators.get(&msg.instrument_id) {
            let klines = aggregator.get_recent_klines(msg.period, msg.count);
            log::info!(
                "📊 [KLineActor GetKLines] Direct match found for {}, returning {} K-lines",
                msg.instrument_id, klines.len()
            );
            return klines;
        }

        // 如果直接匹配失败，尝试用基础合约代码匹配
        // @yutiansut @quantaxis
        let base_id = extract_base_instrument_id(&msg.instrument_id);
        log::info!(
            "📊 [KLineActor GetKLines] Direct match failed, trying base_id: {}",
            base_id
        );

        // 遍历所有aggregator，找到匹配的
        for (key, aggregator) in aggregators.iter() {
            let key_base = extract_base_instrument_id(key);
            if key_base == base_id {
                log::info!(
                    "📊 [KLineActor GetKLines] Found matching aggregator: {} -> {} for query {}",
                    msg.instrument_id,
                    key,
                    base_id
                );
                let klines = aggregator.get_recent_klines(msg.period, msg.count);
                log::info!(
                    "📊 [KLineActor GetKLines] Returning {} K-lines via base_id match",
                    klines.len()
                );
                return klines;
            }
        }

        log::warn!(
            "📊 [KLineActor GetKLines] No aggregator found for instrument: {} (base: {})",
            msg.instrument_id, base_id
        );
        Vec::new()
    }
}

/// 获取当前K线消息（未完成的K线）
#[derive(Message)]
#[rtype(result = "Option<KLine>")]
pub struct GetCurrentKLine {
    pub instrument_id: String,
    pub period: KLinePeriod,
}

impl Handler<GetCurrentKLine> for KLineActor {
    type Result = Option<KLine>;

    fn handle(&mut self, msg: GetCurrentKLine, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        // 首先尝试直接匹配
        if let Some(agg) = aggregators.get(&msg.instrument_id) {
            return agg.get_current_kline(msg.period).cloned();
        }

        // 如果直接匹配失败，尝试用基础合约代码匹配
        // @yutiansut @quantaxis
        let base_id = extract_base_instrument_id(&msg.instrument_id);

        for (key, aggregator) in aggregators.iter() {
            let key_base = extract_base_instrument_id(key);
            if key_base == base_id {
                return aggregator.get_current_kline(msg.period).cloned();
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;
    use std::sync::Arc;
    use tempfile;

    #[test]
    fn test_kline_actor_creation() {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal_manager = Arc::new(crate::storage::wal::WalManager::new(
            tmp_dir.path().to_str().unwrap(),
        ));

        let actor = KLineActor::new(broadcaster, wal_manager);
        assert!(actor.aggregators.read().is_empty());
    }

    #[actix::test]
    async fn test_kline_query() {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal_manager = Arc::new(crate::storage::wal::WalManager::new(
            tmp_dir.path().to_str().unwrap(),
        ));

        let actor = KLineActor::new(broadcaster, wal_manager);
        let addr = actor.start();

        // 手动添加一些测试K线数据
        let now = chrono::Utc::now().timestamp_millis();

        // 查询K线（应该为空）
        let klines = addr
            .send(GetKLines {
                instrument_id: "IF2501".to_string(),
                period: KLinePeriod::Min1,
                count: 10,
            })
            .await
            .unwrap();

        assert_eq!(klines.len(), 0); // 没有数据时应为空
    }

    #[test]
    fn test_wal_recovery() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal_path = tmp_dir.path().to_str().unwrap();

        // 第一步：创建WAL并写入K线数据
        {
            let wal_manager = crate::storage::wal::WalManager::new(wal_path);

            // 写入3根K线
            for i in 0..3 {
                let record = WalRecord::KLineFinished {
                    instrument_id: WalRecord::to_fixed_array_16("IF2501"),
                    period: 4,                            // Min1
                    kline_timestamp: 1000000 + i * 60000, // 每分钟一根
                    open: 3800.0 + i as f64,
                    high: 3850.0 + i as f64,
                    low: 3750.0 + i as f64,
                    close: 3820.0 + i as f64,
                    volume: 100 + i,
                    amount: (3800.0 + i as f64) * (100 + i) as f64,
                    open_oi: 1000,
                    close_oi: 1010 + i,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                };
                wal_manager.append(record).unwrap();
            }
        }

        // 第二步：创建新的Actor并恢复
        {
            let broadcaster = Arc::new(MarketDataBroadcaster::new());
            let wal_manager = Arc::new(crate::storage::wal::WalManager::new(wal_path));
            let actor = KLineActor::new(broadcaster, wal_manager);

            // 触发恢复
            actor.recover_from_wal();

            // 验证恢复的数据
            let agg_map = actor.aggregators.read();
            let aggregator = agg_map
                .get("IF2501")
                .expect("Should have IF2501 aggregator");

            let history = aggregator
                .history_klines
                .get(&KLinePeriod::Min1)
                .expect("Should have Min1 history");
            assert_eq!(history.len(), 3, "Should have recovered 3 K-lines");

            // 验证第一根K线
            assert_eq!(history[0].open, 3800.0);
            assert_eq!(history[0].close, 3820.0);
            assert_eq!(history[0].volume, 100);
        }
    }
}
