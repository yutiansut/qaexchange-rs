//! 市场数据服务模块
//!
//! 提供市场数据的业务逻辑，包括订单簿查询、行情数据、成交数据等
//! 遵循解耦原则：业务逻辑与网络层分离

pub mod broadcaster;
pub mod cache;
pub mod kline;
pub mod kline_actor;
pub mod recovery;
pub mod snapshot_broadcaster;
pub mod snapshot_generator;

#[cfg(test)]
mod data_production_tests;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::exchange::AccountManager;
use crate::matching::engine::ExchangeMatchingEngine;
use crate::utils::config::InstrumentConfig;
use crate::ExchangeError;

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// 订单簿快照（买卖五档）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub instrument_id: String,
    pub timestamp: i64,
    pub bids: Vec<PriceLevel>, // 买盘（降序）
    pub asks: Vec<PriceLevel>, // 卖盘（升序）
    pub last_price: Option<f64>,
}

/// 价格档位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub volume: i64,
}

/// DIFF `quotes` 完整快照里**盘口以外**的字段(对齐 CTP DepthMarketData / TqSdk quote)。
/// 由 `MarketDataService::quote_snapshot` 生成,随**实时**盘口快照事件一起广播
/// (事件驱动、10ms 合并,见 `snapshot_broadcaster.rs`;500ms 只是空闲心跳)。@yutiansut @quantaxis
///
/// 口径(本交易所**从不结算**,所以「日」与「昨」都是代理口径,见各字段):
/// - `volume` / `amount`:当日累计成交量(手)/ 成交额(**含合约乘数**),
///   取自 `MarketDataService` 的日 K(UTC 零点 = 北京 08:00 切日)。
///   ⚠️ 日 K 只在内存里,**后端重启后从 0 重新累计** —— 客户端应把「累计量变小」当作重置。
/// - `pre_close` = `pre_settlement`:上一根日 K 收盘价;没有则当日开盘价;再没有则注册价。
/// - `upper_limit` / `lower_limit`:`pre_settlement × (1 ± 10%)`,涨停向下、跌停向上取整到 `price_tick`。
///   撮合与风控**不校验**涨跌停,这两个只是信息字段。
/// - `open` / `highest` / `lowest`:当日尚无成交时为 `None`(JSON `null`)。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct QuoteStats {
    pub last_price: f64,
    pub volume: i64,
    pub amount: f64,
    pub open: Option<f64>,
    pub highest: Option<f64>,
    pub lowest: Option<f64>,
    pub pre_close: f64,
    pub pre_settlement: f64,
    pub upper_limit: f64,
    pub lower_limit: f64,
    pub open_interest: i64,
    pub volume_multiple: f64,
    pub price_tick: f64,
}

/// 涨跌停幅度。合约注册表的 `limit_up_rate` / `limit_down_rate` 对所有合约默认都是 0.1,
/// 而 `MarketDataService` 不持有注册表 —— 与 `snapshot_generator` 的 10% 假设保持一致。
const QUOTE_LIMIT_RATE: f64 = 0.10;

/// 按最小变动价位取整:`up=true` 向上取整,否则向下;`tick<=0` 时原样返回。
/// 结果再按 tick 的小数位四舍五入,消掉 `0.2 * n` 这类浮点尾巴。
/// 当日累计成交额(含乘数),按交易所口径。@yutiansut @quantaxis
///
/// - **郑商所(CZCE)**:CTP 的 `Turnover` = 「当日成交均价按最小变动价位四舍五入」× 累计量 × 乘数,
///   **不是**逐笔 Σ价×量。hft_maker 在本机 tick 库实测 FG2701 11-06 共 41,198 张快照:
///   `Turnover / Volume / 20` 的小数部分 100% 为 0;区间 Δ成交额 / Δ量 / 乘数因此总落在当日均价 ±0.5 内,
///   而不是区间真实成交价 —— 其模型的主动成交流特征就是在这个口径上训练的。
/// - 其他交易所:Σ价×量 × 乘数。
///
/// `sum_px_vol` 为当日 Σ价×量(不含乘数,即日 K 的 amount)。
pub(crate) fn exchange_turnover(exchange_id: &str, sum_px_vol: f64, volume: i64, mult: f64, tick: f64) -> f64 {
    if exchange_id == "CZCE" && volume > 0 && tick > 0.0 {
        let avg = sum_px_vol / volume as f64;
        // 与 round_to_tick 同样的浮点容差处理:先按格数取整
        let avg_r = (avg / tick + 1e-9).round() * tick;
        avg_r * volume as f64 * mult
    } else {
        sum_px_vol * mult
    }
}

fn round_to_tick(price: f64, tick: f64, up: bool) -> f64 {
    if !(tick > 0.0) || !price.is_finite() {
        return price;
    }
    let n = price / tick;
    // 1e-9 容差:1835.0 / 0.2 = 9174.999999… 不能被向下取整成 9174
    let n = if up { (n - 1e-9).ceil() } else { (n + 1e-9).floor() };
    let decs = (-(tick.log10().floor())).max(0.0) as i32;
    let scale = 10f64.powi(decs + 1);
    ((n * tick) * scale).round() / scale
}

/// 最新成交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTrade {
    pub trade_id: String,
    pub instrument_id: String,
    pub price: f64,
    pub volume: i64,
    pub timestamp: i64,
    pub direction: String, // "BUY" or "SELL"
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
    /// 快照生成器（每秒级别市场快照）
    snapshot_generator: Option<Arc<snapshot_generator::MarketSnapshotGenerator>>,
    /// K线管理器（实时K线聚合）
    kline_manager: Arc<kline::KLineManager>,
    /// 账户管理器（用于快照统计）
    account_manager: Option<Arc<AccountManager>>,
    /// 市场数据广播器
    market_broadcaster: Option<Arc<MarketDataBroadcaster>>,
    /// 每个合约上一张 DIFF 行情快照的时间戳(毫秒) —— 保证同合约快照 datetime 严格递增
    quote_ts: Arc<dashmap::DashMap<String, i64>>,
    /// 持仓量缓存 `合约 -> (值, 计算时刻 ms)`,由 `refresh_open_interest` 在**后台线程**刷新。
    /// 快照路径只读缓存、绝不现算 —— 见 `open_interest_cached`。
    oi_cache: Arc<dashmap::DashMap<String, (i64, i64)>>,
    /// 每个合约最近一次解析出的最新价。冷启动(还没有日 K)时 `quote_snapshot` 要退到
    /// `get_tick_data`,其 L2 会区间扫描一小时 WAL —— 只扫第一次,之后复用这里的值
    last_px: Arc<dashmap::DashMap<String, f64>>,
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
            snapshot_generator: None,
            kline_manager: Arc::new(kline::KLineManager::new()),
            account_manager: None,
            market_broadcaster: None,
            quote_ts: Arc::new(dashmap::DashMap::new()),
            oi_cache: Arc::new(dashmap::DashMap::new()),
            last_px: Arc::new(dashmap::DashMap::new()),
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

    /// 注入账户管理器（用于快照统计）
    pub fn with_account_manager(mut self, account_manager: Arc<AccountManager>) -> Self {
        self.account_manager = Some(account_manager);
        self
    }

    /// 设置 iceoryx2 管理器（零拷贝 IPC）
    pub fn with_iceoryx(mut self, manager: Arc<RwLock<crate::ipc::IceoryxManager>>) -> Self {
        self.iceoryx_manager = Some(manager);
        log::info!("✅ Market data service: iceoryx2 enabled");
        self
    }

    /// 设置市场数据广播器
    pub fn with_broadcaster(mut self, broadcaster: Arc<MarketDataBroadcaster>) -> Self {
        self.market_broadcaster = Some(broadcaster);
        log::info!("✅ Market data service: broadcaster connected");
        self
    }

    /// 设置快照生成器（每秒级别市场快照）
    pub fn with_snapshot_generator(mut self, instruments: Vec<String>, interval_ms: u64) -> Self {
        let config = snapshot_generator::SnapshotGeneratorConfig {
            interval_ms,
            enable_persistence: false, // WAL持久化后续实现
            instruments,
        };

        let mut generator =
            snapshot_generator::MarketSnapshotGenerator::new(self.matching_engine.clone(), config);

        if let Some(account_manager) = &self.account_manager {
            generator = generator.with_account_manager(account_manager.clone());
        }

        let generator = Arc::new(generator);

        self.snapshot_generator = Some(generator);
        log::info!(
            "✅ Market data service: snapshot generator configured (interval: {}ms)",
            interval_ms
        );
        self
    }

    /// 启动快照生成器（必须在配置后调用）
    pub fn start_snapshot_generator(&self) -> Option<std::thread::JoinHandle<()>> {
        if let Some(generator) = &self.snapshot_generator {
            let handle = generator.clone().start();
            log::info!("✅ Snapshot generator started");
            Some(handle)
        } else {
            log::warn!("⚠️  Snapshot generator not configured");
            None
        }
    }

    /// 订阅市场快照
    pub fn subscribe_snapshots(
        &self,
    ) -> Option<crossbeam::channel::Receiver<snapshot_generator::MarketSnapshot>> {
        self.snapshot_generator.as_ref().map(|g| g.subscribe())
    }

    /// 更新成交统计（由TradeGateway调用）
    pub fn update_trade_stats(&self, instrument_id: &str, volume: i64, turnover: f64) {
        if let Some(generator) = &self.snapshot_generator {
            generator.update_trade_stats(instrument_id, volume, turnover);
        }
    }

    /// 设置昨收盘价（启动时调用）
    pub fn set_pre_close(&self, instrument_id: &str, pre_close: f64) {
        if let Some(generator) = &self.snapshot_generator {
            generator.set_pre_close(instrument_id, pre_close);
        }
    }

    /// 获取快照生成器引用（用于其他模块集成）
    pub fn snapshot_generator(&self) -> Option<&Arc<snapshot_generator::MarketSnapshotGenerator>> {
        self.snapshot_generator.as_ref()
    }

    /// 快照通道未消费条数(诊断用)。@yutiansut @quantaxis
    /// 没有生成器时返回 None,与「有生成器但队列为 0」区分开。
    pub fn snapshot_pending(&self) -> Option<usize> {
        self.snapshot_generator.as_ref().map(|g| g.pending_len())
    }

    /// 从WAL恢复最近N分钟的市场数据
    pub fn recover_recent_market_data(&self, minutes: i64) -> Result<()> {
        if let Some(ref storage) = self.storage {
            let recovery = recovery::MarketDataRecovery::new(storage.clone(), self.cache.clone());
            match recovery.recover_recent_minutes(minutes) {
                Ok(stats) if stats.tick_records > 0 || stats.orderbook_records > 0 => {
                    log::info!(
                        "✅ [Market Data Recovery] Recovered {} ticks, {} orderbooks in {}ms",
                        stats.tick_records,
                        stats.orderbook_records,
                        stats.recovery_time_ms
                    );
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
            self.instrument_configs
                .insert(config.instrument_id.clone(), config);
        }
    }

    /// 创建带自定义缓存 TTL 的服务
    pub fn new_with_cache_ttl(
        matching_engine: Arc<ExchangeMatchingEngine>,
        cache_ttl_ms: u64,
    ) -> Self {
        Self {
            matching_engine,
            cache: Arc::new(cache::MarketDataCache::new(cache_ttl_ms)),
            instrument_configs: HashMap::new(),
            storage: None,
            iceoryx_manager: None,
            snapshot_generator: None,
            kline_manager: Arc::new(kline::KLineManager::new()),
            account_manager: None,
            market_broadcaster: None,
            quote_ts: Arc::new(dashmap::DashMap::new()),
            oi_cache: Arc::new(dashmap::DashMap::new()),
            last_px: Arc::new(dashmap::DashMap::new()),
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

    /// 缓存快照是否**足以**回答一个 `depth` 档的请求。
    ///
    /// 判据是「买卖两边**各自**都够」——只要有一边比请求浅,就可能是被截断的,
    /// 不能拿它顶替。注意:一边天然就浅(比如买盘只有 1 档)时这会导致每次都落 L3,
    /// 那是**有意的**——正确性优先于一次缓存命中,而 L3 只是读锁 + 排序。
    fn snapshot_covers_depth(s: &OrderBookSnapshot, depth: usize) -> bool {
        s.bids.len() >= depth && s.asks.len() >= depth
    }

    /// 把快照截到 `depth` 档。`depth == 0` 视为不截断(与既有 `?depth=0` 的行为一致)。
    fn truncate_snapshot(mut s: OrderBookSnapshot, depth: usize) -> OrderBookSnapshot {
        if depth > 0 {
            s.bids.truncate(depth);
            s.asks.truncate(depth);
        }
        s
    }

    /// 获取订单簿快照，深度由 `depth` 指定。
    ///
    /// ⚠️ 原实现的文档注释写的是「买卖五档」,而签名却收 `depth` —— 两者自相矛盾,
    /// 而且 **L1/L2 两条缓存路径完全不看 `depth`,直接返回缓存里的那份快照**。
    /// 缓存由 `snapshot_broadcaster.rs:60` 用**固定 depth=10** 周期填充,
    /// 于是同一个 HTTP 请求会随缓存命中与否给出**完全不同的档数**,且无任何提示。
    ///
    /// 实测(2026-09-06,从 access log 按 depth 分组统计响应字节数):
    /// ```text
    ///   depth=5    → 296 字节 ×209,  4808 字节 ×3
    ///   depth=200  → 4808 ×8,        296 ×5      ← 同一请求两种结果
    ///   depth=500  → 4808 ×261,      296 ×1
    ///   depth=100  → 2959 ×1,        296 ×1      ← 2959≈100 档,证明 L3 确实用了 depth
    /// ```
    /// 按 4808 字节 / 166 档 ≈ 29 字节/档 换算,296 字节就是 5 档 ——
    /// **客户端请求 200 档却拿到 5 档,拿不到任何区分依据**。
    ///
    /// 更干净的复现(2026-09-06,浏览器 + 接口双轨实测):
    /// **冷调用** `depth=500` 得到真实深度(IF2502=152 / IC2501=126 / IH2501=131 档),
    /// 此后所有调用无论请求 10 还是 500,**恒为 10 档** —— 30/30 配对采样完全相同,
    /// 正是「缓存由 broadcaster 用固定 depth=10 填,而 L1/L2 不看请求值」。
    /// ⚠️ 冷/热**无法从单次响应上区分**,所以「真实深度」只能靠冷热差异推断,
    /// 不能把任何一次读数当真值。
    ///
    /// **影响面(实测校正)**:当前前端**看不见**这个缺陷 ——
    /// `views/trade/index.vue` 只有 5/10 两个档位按钮(全仓唯一调用点),
    /// **能请求的最大深度 10 恰好等于缓存填充深度 10**,截断永远不咬人;
    /// 且 `:399` 客户端还会 `slice(0, depth)` 再截一次。
    /// 64 次界面切换 + 2,240 次接口压测,**零命中**。
    /// 所以它今天只影响**请求 >10 档的消费方**(外部脚本、行情下游、
    /// 或将来给 `/trade` 加「20档/50档」按钮)。修它是为了守住
    /// 「请求多少给多少」这个契约,不是在救一个正在发作的故障。
    /// @yutiansut @quantaxis
    ///
    /// 修法:缓存**比请求浅就落到 L3 重算**(可能是被截断的),**比请求深就截断**。
    /// 代价是浅缓存下多一次 L3 计算(读锁 + 排序),换取「请求多少给多少」这个契约。
    ///
    /// ⚠️ 2026-11-07:查找顺序改为 **L1 → L3(实时撮合簿)→ L2(WAL)兜底** @yutiansut @quantaxis
    ///
    /// 原顺序是 L1 → L2 → L3。L1 只有 100ms TTL,于是绝大多数请求落到 L2 ——
    /// 每次**区间扫描最近 1 小时的行情 WAL**,取最新一条 `OrderBookSnapshot` 返回。
    /// 那条记录:① 由 `order_router::persist_orderbook_snapshot` **限流每秒最多写一次**,
    /// 实测接口返回的盘口比当前时刻**旧 1.2~1.6 秒**;② 写入时取的是前 10 张**订单**
    /// 而非 10 个**价位**,读回来同一价位重复出现(`1896.0×1, 1896.0×1, 1896.0×1`),
    /// 而 10 张单又恰好让 `snapshot_covers_depth(_, 5/10)` 判为「够深」,永远落不到 L3。
    /// 80 个仿真参与者与前端(WS 快照广播同样走本函数)看到的都是这份旧且未聚合的簿。
    /// 撮合引擎里就有实时、权威的订单簿,L3 只是一次读锁 + 排序 —— 比扫一小时 WAL 便宜。
    /// L2 只留给「撮合引擎里没有这个合约」的情形(历史合约 / 尚未注册)。
    pub fn get_orderbook_snapshot(
        &self,
        instrument_id: &str,
        depth: usize,
    ) -> Result<OrderBookSnapshot> {
        // L1 缓存查询（降低日志级别到trace以减少输出）
        if let Some(snapshot) = self.cache.get_orderbook(instrument_id) {
            if Self::snapshot_covers_depth(&snapshot, depth) {
                log::trace!("✅ [L1 Cache] Hit for orderbook {}", instrument_id);
                return Ok(Self::truncate_snapshot(snapshot, depth));
            }
            log::trace!(
                "↩️ [L1 Cache] 命中但深度不足({} bids / {} asks < 请求 {}),落到实时计算",
                snapshot.bids.len(), snapshot.asks.len(), depth
            );
        }
        log::trace!("❌ [L1 Cache] Miss for orderbook {}", instrument_id);

        // L3:撮合引擎实时盘口(按价位聚合,见 ExchangeMatchingEngine::price_levels)
        if let Some((bids, asks, last)) = self.matching_engine.price_levels(instrument_id, depth) {
            let snapshot = OrderBookSnapshot {
                instrument_id: instrument_id.to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                bids: Self::to_price_levels(bids),
                asks: Self::to_price_levels(asks),
                last_price: Some(last),
            };
            // 更新 L1 缓存
            self.cache
                .update_orderbook(instrument_id.to_string(), snapshot.clone());
            return Ok(snapshot);
        }

        // L2 兜底:撮合引擎里没有该合约时,才用 WAL 里最近的快照
        if let Some(ref _storage) = self.storage {
            log::trace!("🔍 [L2 Storage] Querying WAL for orderbook {}", instrument_id);
            match self.load_orderbook_from_storage(instrument_id) {
                Ok(snapshot) => {
                    log::trace!(
                        "✅ [L2 Storage] Found orderbook {} in WAL: {} bids, {} asks",
                        instrument_id,
                        snapshot.bids.len(),
                        snapshot.asks.len()
                    );
                    return Ok(Self::truncate_snapshot(snapshot, depth));
                }
                Err(e) => {
                    log::trace!("❌ [L2 Storage] Not found in WAL: {}", e);
                }
            }
        } else {
            log::trace!("⚠️  [L2 Storage] Storage not configured");
        }

        Err(ExchangeError::NotFound(format!("Instrument not found: {}", instrument_id)))
    }

    /// `price_levels` 的 `(价格, 量)` → `PriceLevel`
    fn to_price_levels(v: Vec<(f64, f64)>) -> Vec<PriceLevel> {
        v.into_iter()
            .map(|(price, volume)| PriceLevel { price, volume: volume as i64 })
            .collect()
    }

    /// 合约持仓量 —— **只读缓存,绝不在调用线程上现算**。@yutiansut @quantaxis
    ///
    /// `get_instrument_open_interest` 要对全部账户逐个取读锁;而 60s QIFI 快照落盘
    /// 对每个账户持**写锁**做 `get_qifi_slice`(大账户 60~90MB,几十~几百 ms)。
    /// 2026-11-08 实测:在快照路径上现算(哪怕 1 秒 TTL)会让每张快照被拖到 ~250ms,
    /// 实时发布线程被持仓量扫描占满,DIFF 只剩 0.4 张/秒。
    /// 缓存由 `refresh_open_interest` 在后台线程每秒刷新;启动后第一秒内为 0。
    fn open_interest_cached(&self, instrument_id: &str, _now_ms: i64) -> i64 {
        self.oi_cache.get(instrument_id).map(|e| e.value().0).unwrap_or(0)
    }

    /// 刷新全部合约的持仓量缓存。**会阻塞**(逐账户取读锁),只应在专用后台线程调用。
    pub fn refresh_open_interest(&self) {
        let Some(mgr) = self.account_manager.as_ref() else { return };
        for inst in self.matching_engine.get_instruments() {
            let v = mgr.get_instrument_open_interest(&inst);
            let at = chrono::Utc::now().timestamp_millis();
            self.oi_cache.insert(inst, (v, at));
        }
    }

    /// DIFF `quotes` 完整快照:盘口(按价位聚合,最多 `depth` 档)+ 当日统计(`QuoteStats`)。
    /// 口径见 `QuoteStats` 文档。@yutiansut @quantaxis
    ///
    /// 返回盘口的 `timestamp`(毫秒)对同一合约**严格递增** —— 同一毫秒内再取会顺延 1ms;
    /// 下游(如 hft_maker)按 `datetime` 去重。
    pub fn quote_snapshot(
        &self,
        instrument_id: &str,
        depth: usize,
    ) -> Result<(OrderBookSnapshot, QuoteStats)> {
        use kline::KLinePeriod;

        // 实时推送直接读撮合簿 —— `get_orderbook_snapshot` 的 L1 缓存有 100ms TTL,
        // 实时路径不能吃最多 100ms 旧的簿。引擎里没有该合约时才走原路径(L2 兜底 / NotFound)。
        let mut book = match self.matching_engine.price_levels(instrument_id, depth) {
            Some((bids, asks, last)) => OrderBookSnapshot {
                instrument_id: instrument_id.to_string(),
                timestamp: 0, // 下面统一赋严格递增的时间戳
                bids: Self::to_price_levels(bids),
                asks: Self::to_price_levels(asks),
                last_price: Some(last),
            },
            None => self.get_orderbook_snapshot(instrument_id, depth)?,
        };

        let cfg = self.get_instrument_config(instrument_id);
        let mult = cfg.map(|c| c.multiplier).filter(|m| *m > 0.0).unwrap_or(1.0);
        let tick = cfg.map(|c| c.tick_size).filter(|t| *t > 0.0).unwrap_or(0.0);

        let now_ms = chrono::Utc::now().timestamp_millis();
        let today_start = KLinePeriod::Day.align_timestamp(now_ms);
        // 当前日 K 的起点不是今天 ⇒ 切日后还没有成交,这根其实是「上一根」
        let (today_bar, stale_prev) =
            match self.kline_manager.get_current_kline(instrument_id, KLinePeriod::Day) {
                Some(k) if k.timestamp == today_start => (Some(k), None),
                Some(k) => (None, Some(k)),
                None => (None, None),
            };

        // 昨收/昨结:上一根日 K 收盘 → 当日开盘 → 注册价
        let pre = stale_prev
            .as_ref()
            .map(|k| k.close)
            .or_else(|| {
                self.kline_manager
                    .get_last_finished_kline(instrument_id, KLinePeriod::Day)
                    .map(|k| k.close)
            })
            .filter(|p| *p > 0.0)
            .or_else(|| today_bar.as_ref().map(|k| k.open).filter(|p| *p > 0.0))
            .or_else(|| cfg.map(|c| c.init_price).filter(|p| *p > 0.0))
            .or_else(|| self.matching_engine.get_prev_close(instrument_id))
            .unwrap_or(0.0);

        // 最新价:今日最后一笔 → 上一根日 K 收盘 → 上次解析值 → tick(仅首次,L2 会扫 WAL)→ 撮合簿
        let last_price = today_bar
            .as_ref()
            .map(|k| k.close)
            .or_else(|| stale_prev.as_ref().map(|k| k.close))
            .filter(|p| *p > 0.0)
            .or_else(|| self.last_px.get(instrument_id).map(|e| *e.value()))
            .or_else(|| self.get_tick_data(instrument_id).ok().map(|t| t.last_price))
            .filter(|p| *p > 0.0)
            .or(book.last_price)
            .unwrap_or(0.0);
        if last_price > 0.0 {
            self.last_px.insert(instrument_id.to_string(), last_price);
        }
        book.last_price = Some(last_price);

        let volume = today_bar.as_ref().map(|k| k.volume).unwrap_or(0);
        let stats = QuoteStats {
            last_price,
            volume,
            // 日 K 的 amount 是 Σ价×量(kline.rs `amount += price * volume`),不含乘数;
            // 郑商所按「当日均价取整 × 量 × 乘数」口径(见 exchange_turnover)
            amount: exchange_turnover(
                cfg.map(|c| c.exchange_id.as_str()).unwrap_or(""),
                today_bar.as_ref().map(|k| k.amount).unwrap_or(0.0),
                volume,
                mult,
                tick,
            ),
            open: today_bar.as_ref().map(|k| k.open),
            highest: today_bar.as_ref().map(|k| k.high),
            lowest: today_bar.as_ref().map(|k| k.low),
            pre_close: pre,
            pre_settlement: pre,
            upper_limit: round_to_tick(pre * (1.0 + QUOTE_LIMIT_RATE), tick, false),
            lower_limit: round_to_tick(pre * (1.0 - QUOTE_LIMIT_RATE), tick, true),
            open_interest: self.open_interest_cached(instrument_id, now_ms),
            volume_multiple: mult,
            price_tick: tick,
        };

        // 同合约时间戳严格递增
        let ts = {
            let mut e = self.quote_ts.entry(instrument_id.to_string()).or_insert(0);
            let ts = now_ms.max(*e + 1);
            *e = ts;
            ts
        };
        book.timestamp = ts;

        Ok((book, stats))
    }

    /// 获取合约列表
    pub fn get_instruments(&self) -> Result<Vec<InstrumentInfo>> {
        let engine = &self.matching_engine;
        let instruments = engine.get_instruments();

        let mut result = Vec::new();
        for instrument_id in instruments {
            let last_price = engine.get_last_price(&instrument_id);

            // 从配置读取合约信息，如果配置不存在则使用默认值
            let (name, multiplier, tick_size) =
                if let Some(config) = self.instrument_configs.get(&instrument_id) {
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
        log::trace!("📊 [MarketData] get_tick_data for {}", instrument_id);

        // L1 缓存查询（降低日志级别）
        if let Some(tick) = self.cache.get_tick(instrument_id) {
            log::trace!("✅ [L1 Cache] Hit for tick {}", instrument_id);
            return Ok(tick);
        }
        log::trace!("❌ [L1 Cache] Miss for tick {}", instrument_id);

        // L2 从 WAL 恢复最近的 Tick（降低日志级别）
        if let Some(ref _storage) = self.storage {
            log::trace!("🔍 [L2 Storage] Querying WAL for tick {}", instrument_id);
            match self.load_tick_from_storage(instrument_id) {
                Ok(tick) => {
                    log::trace!(
                        "✅ [L2 Storage] Found tick {} in WAL: price={}",
                        instrument_id,
                        tick.last_price
                    );
                    // 更新缓存
                    self.cache
                        .update_tick(instrument_id.to_string(), tick.clone());
                    return Ok(tick);
                }
                Err(e) => {
                    log::trace!("❌ [L2 Storage] Not found in WAL: {}", e);
                }
            }
        } else {
            log::trace!("⚠️  [L2 Storage] Storage not configured");
        }

        // L3 缓存未命中，从 Orderbook 实时计算（降低日志级别）
        log::trace!(
            "🔍 [L3 Realtime] Computing tick from orderbook for {}",
            instrument_id
        );
        let engine = &self.matching_engine;

        // 检查合约是否存在
        let orderbook = engine.get_orderbook(instrument_id).ok_or_else(|| {
            ExchangeError::NotFound(format!("Instrument not found: {}", instrument_id))
        })?;

        let ob = orderbook.read();

        // 获取最新成交价
        let last_price = ob.lastprice;

        // 获取最优买卖价（从排序订单中获取第一个）
        let bid_price = ob
            .bid_queue
            .get_sorted_orders()
            .and_then(|orders| orders.first().map(|o| o.price));

        let ask_price = ob
            .ask_queue
            .get_sorted_orders()
            .and_then(|orders| orders.first().map(|o| o.price));

        let trade_recorder = self.matching_engine.get_trade_recorder();
        let volume = trade_recorder
            .get_trades_by_instrument(instrument_id)
            .last()
            .map(|trade| trade.volume as i64)
            .unwrap_or(0);

        let tick = TickData {
            instrument_id: instrument_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            last_price,
            bid_price,
            ask_price,
            volume,
        };

        // 更新 L1 缓存
        self.cache
            .update_tick(instrument_id.to_string(), tick.clone());

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

            log::debug!(
                "📂 [Storage] range_query for tick: {} - {} (1 hour)",
                start_ts,
                end_ts
            );

            let records = storage
                .range_query(start_ts, end_ts)
                .map_err(|e| ExchangeError::InternalError(format!("Failed to query WAL: {}", e)))?;

            log::debug!(
                "📂 [Storage] Found {} total records in range",
                records.len()
            );

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
                } = record
                {
                    tick_count += 1;
                    let inst_str =
                        crate::storage::wal::record::WalRecord::from_fixed_array(inst_id);
                    if inst_str == instrument_id {
                        log::debug!(
                            "✅ [Storage] Found TickData #{} for {}: price={}",
                            tick_count,
                            inst_str,
                            last_price
                        );
                        return Ok(TickData {
                            instrument_id: inst_str,
                            timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                            last_price: *last_price,
                            bid_price: if *bid_price > 0.0 {
                                Some(*bid_price)
                            } else {
                                None
                            },
                            ask_price: if *ask_price > 0.0 {
                                Some(*ask_price)
                            } else {
                                None
                            },
                            volume: *volume,
                        });
                    }
                }
            }

            log::debug!(
                "❌ [Storage] No TickData found for {} (scanned {} tick records)",
                instrument_id,
                tick_count
            );
        }

        Err(ExchangeError::StorageError(format!(
            "No tick data found for {}",
            instrument_id
        )))
    }

    /// 从storage加载最近的OrderBookSnapshot（私有方法）
    fn load_orderbook_from_storage(&self, instrument_id: &str) -> Result<OrderBookSnapshot> {
        if let Some(ref storage) = self.storage {
            // 查询最近1小时的数据
            let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
            let start_ts = end_ts - (3600 * 1_000_000_000); // 1小时

            let records = storage
                .range_query(start_ts, end_ts)
                .map_err(|e| ExchangeError::InternalError(format!("Failed to query WAL: {}", e)))?;

            // 从后往前找最新的OrderBookSnapshot
            for (_ts, _seq, record) in records.iter().rev() {
                if let crate::storage::wal::record::WalRecord::OrderBookSnapshot {
                    instrument_id: inst_id,
                    bids,
                    asks,
                    last_price,
                    timestamp,
                } = record
                {
                    let inst_str =
                        crate::storage::wal::record::WalRecord::from_fixed_array(inst_id);
                    if inst_str == instrument_id {
                        // 转换固定数组为Vec
                        let bids_vec: Vec<PriceLevel> = bids
                            .iter()
                            .filter(|(price, _)| *price > 0.0)
                            .map(|(price, volume)| PriceLevel {
                                price: *price,
                                volume: *volume,
                            })
                            .collect();

                        let asks_vec: Vec<PriceLevel> = asks
                            .iter()
                            .filter(|(price, _)| *price > 0.0)
                            .map(|(price, volume)| PriceLevel {
                                price: *price,
                                volume: *volume,
                            })
                            .collect();

                        return Ok(OrderBookSnapshot {
                            instrument_id: inst_str,
                            timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                            bids: bids_vec,
                            asks: asks_vec,
                            last_price: if *last_price > 0.0 {
                                Some(*last_price)
                            } else {
                                None
                            },
                        });
                    }
                }
            }
        }

        Err(ExchangeError::StorageError(format!(
            "No orderbook snapshot found for {}",
            instrument_id
        )))
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

                let bid_count = ob
                    .bid_queue
                    .get_sorted_orders()
                    .map(|v| v.len())
                    .unwrap_or(0);
                let ask_count = ob
                    .ask_queue
                    .get_sorted_orders()
                    .map(|v| v.len())
                    .unwrap_or(0);

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

    /// 处理Tick数据并更新K线（成交时调用）
    /// @yutiansut @quantaxis
    /// ✨ `direction` 是主动方方向("buy"/"sell") @yutiansut @quantaxis
    ///
    /// 本函数是**行情摄入的唯一入口** —— Tick 广播只从这里发出。
    /// order_router 此前也直接 broadcast_tick(`:1144`/`:1257`),与这里构成
    /// 完全相同的事件(broadcaster.rs:693 是同一个 MarketDataEvent::Tick),
    /// 两条并存导致每笔成交重复计入 K线 volume。
    /// 现改为:order_router 不再广播,由本函数统一发出并带真实 direction
    /// (原来这里填空串)。单测 data_production_tests::test_matching_to_broadcast_flow
    /// 直接调本函数并断言收到 Tick —— 该契约必须保留。
    pub fn on_trade(&self, instrument_id: &str, price: f64, volume: i64, direction: &str) {
        let timestamp_ms = chrono::Utc::now().timestamp_millis();

        if let Some(broadcaster) = &self.market_broadcaster {
            broadcaster.broadcast(MarketDataEvent::Tick {
                instrument_id: instrument_id.to_string(),
                price,
                volume: volume as f64,
                direction: direction.to_string(),
                timestamp: timestamp_ms,
            });
        }

        // 更新K线
        let finished_klines =
            self.kline_manager
                .on_tick(instrument_id, price, volume, timestamp_ms);

        // 记录完成的K线（可以发送到WebSocket订阅者）
        for (period, kline) in finished_klines {
            log::debug!(
                "📊 [KLine] Finished {} {:?} K-line: O={:.2} H={:.2} L={:.2} C={:.2} V={}",
                instrument_id,
                period,
                kline.open,
                kline.high,
                kline.low,
                kline.close,
                kline.volume
            );

            if let Some(broadcaster) = &self.market_broadcaster {
                broadcaster.broadcast(MarketDataEvent::KLineFinished {
                    instrument_id: instrument_id.to_string(),
                    period: period.to_int(),
                    kline: kline.clone(),
                    timestamp: timestamp_ms,
                });
            }
        }
    }

    /// 获取K线数据
    pub fn get_klines(
        &self,
        instrument_id: &str,
        period: kline::KLinePeriod,
        count: usize,
    ) -> Vec<kline::KLine> {
        self.kline_manager.get_klines(instrument_id, period, count)
    }

    /// 获取当前未完成的K线
    pub fn get_current_kline(
        &self,
        instrument_id: &str,
        period: kline::KLinePeriod,
    ) -> Option<kline::KLine> {
        self.kline_manager.get_current_kline(instrument_id, period)
    }
}

// 重新导出
pub use broadcaster::{MarketDataBroadcaster, MarketDataEvent};
pub use cache::{CacheStatsSnapshot, MarketDataCache};
pub use kline_actor::{GetCurrentKLine, GetKLines, KLineActor};
pub use recovery::{MarketDataRecovery, RecoveredMarketData, RecoveryStats};
pub use snapshot_broadcaster::SnapshotBroadcastService;
pub use snapshot_generator::{MarketSnapshot, MarketSnapshotGenerator, SnapshotGeneratorConfig};

#[cfg(test)]
mod orderbook_depth_tests {
    use super::*;

    fn snap(nb: usize, na: usize) -> OrderBookSnapshot {
        OrderBookSnapshot {
            instrument_id: "IF2501".to_string(),
            timestamp: 0,
            bids: (0..nb).map(|i| PriceLevel { price: 100.0 - i as f64, volume: 1 }).collect(),
            asks: (0..na).map(|i| PriceLevel { price: 101.0 + i as f64, volume: 1 }).collect(),
            last_price: Some(100.5),
        }
    }

    /// ✨ 核心:缓存**比请求浅**时不能顶替 —— 那正是「请求 200 档拿到 5 档」的成因。
    #[test]
    fn shallow_cache_does_not_satisfy_deep_request() {
        assert!(!MarketDataService::snapshot_covers_depth(&snap(5, 5), 200));
        assert!(!MarketDataService::snapshot_covers_depth(&snap(10, 10), 20));
    }

    /// 够深就可以用。
    #[test]
    fn deep_enough_cache_satisfies() {
        assert!(MarketDataService::snapshot_covers_depth(&snap(10, 10), 10));
        assert!(MarketDataService::snapshot_covers_depth(&snap(166, 166), 5));
    }

    /// 一边浅也不行 —— 判据是两边**各自**都够。
    #[test]
    fn one_shallow_side_is_not_enough() {
        assert!(!MarketDataService::snapshot_covers_depth(&snap(1, 166), 5));
        assert!(!MarketDataService::snapshot_covers_depth(&snap(166, 1), 5));
    }

    /// 截断到请求档数,且两边独立。
    #[test]
    fn truncate_respects_depth() {
        let t = MarketDataService::truncate_snapshot(snap(166, 166), 5);
        assert_eq!((t.bids.len(), t.asks.len()), (5, 5));
    }

    /// 比请求浅的一边保持原样,不会被补齐(不能编造档位)。
    #[test]
    fn truncate_does_not_pad() {
        let t = MarketDataService::truncate_snapshot(snap(2, 166), 5);
        assert_eq!((t.bids.len(), t.asks.len()), (2, 5), "浅的一边保持 2,不能补成 5");
    }

    /// `depth == 0` 不截断 —— 与既有 `?depth=0` 的行为保持一致。
    #[test]
    fn zero_depth_means_no_truncation() {
        let t = MarketDataService::truncate_snapshot(snap(7, 9), 0);
        assert_eq!((t.bids.len(), t.asks.len()), (7, 9));
    }
}

/// ✨ DIFF 完整行情快照(2026-11-08,hft_maker 接入)@yutiansut @quantaxis
#[cfg(test)]
mod quote_snapshot_tests {
    use super::*;
    use crate::matching::engine::InstrumentAsset;
    use crate::matching::{orders, OrderDirection};

    fn service() -> MarketDataService {
        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("IH2501".to_string(), 2800.0).unwrap();
        let mut s = MarketDataService::new(engine);
        // 与 main.rs 一致:init_price 填 0,注册价只在撮合引擎的 prev_close 里
        s.set_instrument_configs(vec![InstrumentConfig {
            instrument_id: "IH2501".into(),
            name: "IH".into(),
            exchange_id: "CFFEX".into(),
            product_type: "index_future".into(),
            init_price: 0.0,
            is_trading: true,
            multiplier: 300.0,
            tick_size: 0.2,
            commission_per_lot: None,
            commission_rate: None,
            margin_rate: None,
        }]);
        s
    }

    #[test]
    fn round_to_tick_directions_and_float_tails() {
        assert_eq!(round_to_tick(2018.5, 0.2, false), 2018.4);
        assert_eq!(round_to_tick(1643.94, 0.2, true), 1644.0);
        // 1835.0 / 0.2 = 9174.999… —— 无容差会被向下取整成 1834.8
        assert_eq!(round_to_tick(1835.0, 0.2, false), 1835.0);
        assert_eq!(round_to_tick(1835.0, 0.2, true), 1835.0);
        assert_eq!(round_to_tick(123.456, 0.0, true), 123.456);
    }

    /// 冷启动无任何成交:昨收落到注册价,累计为 0,开高低为 None
    #[test]
    fn cold_start_falls_back_to_registration_price() {
        let s = service();
        let (_book, st) = s.quote_snapshot("IH2501", 5).unwrap();
        assert_eq!((st.volume, st.amount, st.open, st.highest, st.lowest), (0, 0.0, None, None, None));
        assert_eq!((st.pre_close, st.pre_settlement), (2800.0, 2800.0));
        assert_eq!((st.upper_limit, st.lower_limit), (3080.0, 2520.0));
        assert_eq!((st.volume_multiple, st.price_tick), (300.0, 0.2));
    }

    /// 今日有成交、无上一根日 K:累计量 / 成交额(含乘数)/ 开高低,昨收 = 当日开盘
    #[test]
    fn today_trades_accumulate_with_multiplier() {
        let s = service();
        s.on_trade("IH2501", 1810.0, 2, "buy");
        s.on_trade("IH2501", 1812.0, 3, "sell");
        s.on_trade("IH2501", 1808.0, 1, "buy");
        let (book, st) = s.quote_snapshot("IH2501", 5).unwrap();
        assert_eq!(st.volume, 6);
        let expect = (1810.0 * 2.0 + 1812.0 * 3.0 + 1808.0 * 1.0) * 300.0;
        assert!((st.amount - expect).abs() < 1e-6, "amount={} expect={}", st.amount, expect);
        assert_eq!((st.open, st.highest, st.lowest), (Some(1810.0), Some(1812.0), Some(1808.0)));
        assert_eq!((st.last_price, book.last_price), (1808.0, Some(1808.0)));
        assert_eq!(st.pre_close, 1810.0, "无上一根日K时昨收 = 当日开盘");
        assert_eq!(st.upper_limit, 1991.0); // 1810×1.1 = 1991.0
        assert_eq!(st.lower_limit, 1629.0); // 1810×0.9 = 1629.0
    }

    /// ✨ 郑商所成交额口径:当日均价按 tick 四舍五入 × 量 × 乘数;其他交易所不变 @yutiansut @quantaxis
    #[test]
    fn czce_turnover_uses_rounded_daily_average() {
        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("FG2701".to_string(), 882.0).unwrap();
        let mut s = MarketDataService::new(engine);
        s.set_instrument_configs(vec![InstrumentConfig {
            instrument_id: "FG2701".into(),
            name: "玻璃2701".into(),
            exchange_id: "CZCE".into(),
            product_type: "futures".into(),
            init_price: 882.0,
            is_trading: true,
            multiplier: 20.0,
            tick_size: 1.0,
            commission_per_lot: None,
            commission_rate: None,
            margin_rate: None,
        }]);
        s.on_trade("FG2701", 881.0, 3, "buy");
        s.on_trade("FG2701", 882.0, 2, "sell");
        let (_b, st) = s.quote_snapshot("FG2701", 5).unwrap();
        // 均价 (2643 + 1764) / 5 = 881.4 → 881;成交额 = 881 × 5 × 20(逐笔口径会是 88,140)
        assert_eq!(st.volume, 5);
        assert!((st.amount - 88_100.0).abs() < 1e-6, "amount={}", st.amount);
        assert_eq!((st.amount / st.volume as f64 / 20.0).fract(), 0.0);
        // 再成交 3 手 @884:均价 (4407 + 2652) / 8 = 882.375 → 882
        s.on_trade("FG2701", 884.0, 3, "buy");
        let (_b, st) = s.quote_snapshot("FG2701", 5).unwrap();
        assert!((st.amount - 882.0 * 8.0 * 20.0).abs() < 1e-6, "amount={}", st.amount);
    }

    #[test]
    fn exchange_turnover_rounding_and_other_exchanges() {
        // 半格四舍五入(远离零):881.5 → 882
        assert_eq!(exchange_turnover("CZCE", 881.5 * 2.0, 2, 20.0, 1.0), 882.0 * 2.0 * 20.0);
        // 其他交易所 = Σ价×量 × 乘数
        assert_eq!(exchange_turnover("CFFEX", 1810.3 * 2.0, 2, 300.0, 0.2), 1810.3 * 2.0 * 300.0);
        // 无成交 / 无 tick 配置:退回逐笔口径
        assert_eq!(exchange_turnover("CZCE", 0.0, 0, 20.0, 1.0), 0.0);
        assert_eq!(exchange_turnover("CZCE", 881.4 * 5.0, 5, 20.0, 0.0), 881.4 * 5.0 * 20.0);
    }

    /// 切日:当前日 K 是昨天的 ⇒ 今日累计归零、昨收 = 昨天收盘;今天首笔成交后昨收不变
    #[test]
    fn day_roll_resets_volume_and_keeps_previous_close() {
        let s = service();
        let yesterday = chrono::Utc::now().timestamp_millis() - 86_400_000;
        s.kline_manager.on_tick("IH2501", 1900.0, 5, yesterday);
        s.kline_manager.on_tick("IH2501", 1890.0, 5, yesterday + 1);
        let (_b, st) = s.quote_snapshot("IH2501", 5).unwrap();
        assert_eq!((st.volume, st.amount, st.open), (0, 0.0, None));
        assert_eq!(st.pre_close, 1890.0);
        assert_eq!(st.last_price, 1890.0, "切日后无成交时最新价沿用上一笔");

        s.on_trade("IH2501", 1880.0, 1, "buy");
        let (_b, st2) = s.quote_snapshot("IH2501", 5).unwrap();
        assert_eq!(st2.volume, 1, "切日后从 0 重新累计");
        assert_eq!(st2.pre_close, 1890.0, "昨天那根已进入历史,昨收不应变成今日开盘");
    }

    /// 同一毫秒内连续取,时间戳也必须严格递增(下游按 datetime 去重)
    #[test]
    fn timestamps_strictly_increase() {
        let s = service();
        let mut last = 0;
        for _ in 0..200 {
            let (b, _) = s.quote_snapshot("IH2501", 1).unwrap();
            assert!(b.timestamp > last, "{} !> {}", b.timestamp, last);
            last = b.timestamp;
        }
    }

    /// 盘口来自实时撮合簿且按价位聚合(复用 ExchangeMatchingEngine::price_levels)
    #[test]
    fn quote_book_is_live_and_aggregated() {
        let s = service();
        let ob = s.matching_engine.get_orderbook("IH2501").unwrap();
        let asset = InstrumentAsset::from_code("IH2501");
        for (i, (side, px)) in [(OrderDirection::BUY, 1809.8), (OrderDirection::BUY, 1809.8),
                                 (OrderDirection::BUY, 1809.6), (OrderDirection::SELL, 1810.2)]
            .iter().enumerate()
        {
            ob.write().process_order(orders::new_limit_order_request(asset, *side, *px, 1.0, 10 + i as i64));
        }
        let (book, _) = s.quote_snapshot("IH2501", 10).unwrap();
        let bids: Vec<(f64, i64)> = book.bids.iter().map(|l| (l.price, l.volume)).collect();
        assert_eq!(bids, vec![(1809.8, 2), (1809.6, 1)]);
        assert_eq!(book.asks.len(), 1);
    }
}
