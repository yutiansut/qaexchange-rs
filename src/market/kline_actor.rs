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
    /// ⚠️ 这个方法与 `update_factors` 是**死的重复实现** @yutiansut @quantaxis
    ///
    /// 真正在跑的因子计算是**内联**在两处事件循环里的
    /// (定时器分支 :~318-328、K线收盘分支 :~468-482),各自建引擎、调
    /// `engine.update_all(..)`、再 broadcast。这一对方法从头到尾没有调用者
    /// (`update_factors` 零调用,`get_or_create_factor_engine` 只被它调)——
    /// 编译器的 dead_code 警告能报出来,但被 `lib.rs:40` 的
    /// `#![allow(dead_code)]` 全局关掉了(见 tasks/lessons.md L51)。
    ///
    /// 保留它们只是为了不改动既有 API 表面;**改因子逻辑要改内联那两处**,
    /// 改这里不会有任何效果。
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

                    // ✨ 用恢复出来的收盘价**播种** `last_price`。@yutiansut @quantaxis
                    //
                    // `KLineAggregator::on_timer`(kline.rs:307-310)在 `last_price` 为
                    // `None` 时**直接返回空**,而 `last_price` 只在收到 tick 时赋值
                    // (kline.rs:236)。恢复流程原先只填 `history_klines`,不碰它 ——
                    // 于是重启后**任何没有立刻成交的合约,K 线永久停在恢复出来的那一根**,
                    // 哪怕我们明知它的最后收盘价。
                    //
                    // 线上实测:IH2501 的最新 bar 距今 33 小时,而进程只跑了 26 小时
                    // —— 它的最后一根 K 线早于本次启动,启动后一根都没再生成;
                    // 同时 IF2501 每 60 秒都有一根 `V=0` 的平推 bar(它启动后收到过 tick)。
                    // 用户看到的是「价格显示当前值、图表停在两天前」。
                    //
                    // replay 按 WAL 顺序(即时间顺序)回放,所以最后写入的那根最新,
                    // 直接覆盖即可;真实 tick 到达后会立刻覆盖掉这个种子值。
                    let seed_close = kline.close;
                    history.push(kline);

                    // 限制历史数量
                    if history.len() > aggregator.max_history {
                        history.remove(0);
                    }

                    if seed_close.is_finite() && seed_close > 0.0 {
                        aggregator.last_price = Some(seed_close);
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

        // ✨ 定时器驱动的K线生成（每秒检查一次）
        // @yutiansut @quantaxis
        // 即使没有交易，也要按时间周期生成K线
        let timer_aggregators = self.aggregators.clone();
        let timer_broadcaster = self.broadcaster.clone();
        let timer_wal_manager = self.wal_manager.clone();
        let timer_factor_engines = self.factor_engines.clone();
        let timer_enabled_factors = self.enabled_factors.clone();
        let timer_enable_factor_compute = self.enable_factor_compute;

        ctx.run_interval(std::time::Duration::from_secs(1), move |_act, _ctx| {
            let current_timestamp_ms = chrono::Utc::now().timestamp_millis();
            let mut all_finished_klines = Vec::new();

            // 遍历所有聚合器，检查是否有K线需要完成
            {
                let mut agg_map = timer_aggregators.write();
                for (instrument_id, aggregator) in agg_map.iter_mut() {
                    let finished_klines = aggregator.on_timer(current_timestamp_ms);
                    for (period, kline) in finished_klines {
                        all_finished_klines.push((instrument_id.clone(), period, kline));
                    }
                }
            }

            // 广播完成的K线
            for (instrument_id, period, kline) in all_finished_klines {
                log::debug!(
                    "📊 [KLineActor Timer] Finished {} {:?} K-line: O={:.2} H={:.2} L={:.2} C={:.2} V={}",
                    instrument_id, period, kline.open, kline.high, kline.low, kline.close, kline.volume
                );

                // 广播K线完成事件
                timer_broadcaster.broadcast(MarketDataEvent::KLineFinished {
                    instrument_id: instrument_id.clone(),
                    period: period.to_int(),
                    kline: kline.clone(),
                    timestamp: current_timestamp_ms,
                });

                // ✨ 方案A: K线完成时计算因子并广播
                if timer_enable_factor_compute {
                    let mut engines = timer_factor_engines.write();
                    if !engines.contains_key(&instrument_id) {
                        let registry = FactorRegistry::with_standard_factors();
                        let mut engine = StreamFactorEngine::new(registry);
                        for factor_id in &timer_enabled_factors {
                            if let Err(e) = engine.init_factor(factor_id) {
                                log::warn!(
                                    "📈 [KLineActor Timer] Failed to init factor {}: {}",
                                    factor_id, e
                                );
                            }
                        }
                        engines.insert(instrument_id.clone(), engine);
                    }

                    if let Some(engine) = engines.get_mut(&instrument_id) {
                        let factor_ids: Vec<&str> = timer_enabled_factors.iter().map(|s| s.as_str()).collect();
                        let factor_values = engine.update_all(kline.close, &factor_ids);

                        if !factor_values.is_empty() {
                            timer_broadcaster.broadcast(MarketDataEvent::FactorUpdate {
                                instrument_id: instrument_id.clone(),
                                factors: factor_values,
                                period: period.to_int(),
                                timestamp: current_timestamp_ms,
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

                if let Err(e) = timer_wal_manager.append(wal_record) {
                    log::error!(
                        "📊 [KLineActor Timer] Failed to persist K-line to WAL: {}",
                        e
                    );
                }
            }
        });

        log::info!("📊 [KLineActor] Timer-based K-line generation enabled (1s interval)");

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
                // ⚠️ 这里用 spawn_blocking(recv()) 是**可以的**,但仅限于单例消费者。
                //    阻塞中的线程不算 idle,tokio 阻塞池永不回收 —— 所以这个写法
                //    等价于「常驻一个 OS 线程」。本 actor 是单例(全局只启动一次)、
                //    且消费全量行情,一个专用线程换低延迟是划算的。
                //    ❌ **不要把这个形状复制到按连接/按订阅者的路径上** ——
                //    `diff_handler.rs` 曾这么做,实测 40 个订阅 → 线程 24→120 且永不回收。
                //    那种场景用 `session.rs:140` 的 run_interval + try_recv 批量取。
                //    @yutiansut @quantaxis
                let receiver_clone = receiver.clone();
                // ⚠️ 必须用 `recv_timeout` 而不是 `recv()` @yutiansut @quantaxis
                //
                // `recv()` 会一直阻塞到有事件为止 —— 而 tokio 运行时 **drop 时会
                // 无限期等待所有 spawn_blocking 任务结束**。无行情时这个任务永不返回,
                // 于是:
                //   SIGTERM → 优雅停机日志全部打完 → main 返回 → runtime drop **卡死**
                //   → 进程永不退出(实测隔离实例停机 6 分钟后仍存活,端口已释放、
                //      HTTP 无响应,18 个线程全在 futex_wait)
                //   → 部署脚本 `kill; wait; start` 永久挂起
                //   → 于是只能 `kill -9` —— **而 kill -9 正是 2026-09-05 毁掉
                //      172,035 条订单那次事故的直接动作**。
                //
                // 100ms 超时不改变有事件时的延迟(有数据立即返回),
                // 只保证无事件时最多 100ms 就让出,runtime 得以完成 drop。
                // 原注释接受「常驻一个 OS 线程」的代价是对的,但没考虑到
                // 「这个线程让进程永远退不出去」。
                match tokio::task::spawn_blocking(move || receiver_clone.recv_timeout(std::time::Duration::from_millis(100))).await {
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
                    // ⚠️ 必须区分「超时」与「通道关闭」@yutiansut @quantaxis
                    // 改成 recv_timeout 后,超时返回的也是 `Ok(Err(..))`。
                    // 原来这一支无条件 `break`,会让 actor 在 100ms 无行情后
                    // **直接退出** —— 那比原来的「进程退不出去」更糟。
                    Ok(Err(crossbeam::channel::RecvTimeoutError::Timeout)) => {
                        // 无事件,继续等下一轮(这正是让 runtime 能 drop 的关键)
                        continue;
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
