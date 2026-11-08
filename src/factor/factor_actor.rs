//! 因子计算 Actor
//!
//! 独立的 Actix Actor，订阅 K线完成事件并实时计算因子
//!
//! ## 方案B: 独立 FactorActor（推荐长期架构）
//!
//! 相比方案A（在KLineActor中集成），方案B具有以下优势:
//! - 职责分离：K线聚合和因子计算解耦
//! - 可扩展性：可以独立扩展因子计算能力
//! - 可维护性：因子相关代码集中管理
//!
//! ## 数据流
//!
//! ```text
//! MarketDataBroadcaster (KLineFinished)
//!        ↓
//!   FactorActor (订阅)
//!        ↓
//! StreamFactorEngine (计算 MA, EMA, RSI, MACD 等)
//!        ↓
//! MarketDataBroadcaster (FactorUpdate)
//!        ↓
//!   WebSocket (rtn_data with factors)
//! ```
//!
//! @yutiansut @quantaxis

use actix::{Actor, ActorContext, AsyncContext, Context, Handler, Message};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::factor::{FactorRegistry, StreamFactorEngine};
use crate::market::{MarketDataBroadcaster, MarketDataEvent};

/// 默认启用的因子列表
const DEFAULT_FACTORS: &[&str] = &["ma5", "ma10", "ma20", "ema12", "ema26", "rsi14", "macd"];

/// 因子计算 Actor 配置
#[derive(Debug, Clone)]
pub struct FactorActorConfig {
    /// 启用的因子列表
    pub factors: Vec<String>,
    /// 计算的K线周期（默认只计算1分钟K线的因子）
    /// 0 = 日线, 4 = 1分钟, 5 = 5分钟, 6 = 15分钟, 7 = 30分钟, 8 = 60分钟
    pub periods: Vec<i32>,
}

impl Default for FactorActorConfig {
    fn default() -> Self {
        Self {
            factors: DEFAULT_FACTORS.iter().map(|s| s.to_string()).collect(),
            periods: vec![4, 5, 6], // 默认计算 1min, 5min, 15min 周期的因子
        }
    }
}

/// 因子计算 Actor - 独立处理因子计算，订阅 K线完成事件
///
/// ## 使用示例
///
/// ```ignore
/// let broadcaster = Arc::new(MarketDataBroadcaster::new());
/// let actor = FactorActor::new(broadcaster.clone())
///     .with_config(FactorActorConfig {
///         factors: vec!["ma5", "rsi14", "macd"].into_iter().map(String::from).collect(),
///         periods: vec![4, 5], // 1min, 5min
///     });
/// let addr = actor.start();
/// ```
pub struct FactorActor {
    /// 市场数据广播器（用于订阅K线完成事件和推送因子更新）
    broadcaster: Arc<MarketDataBroadcaster>,

    /// 配置
    config: FactorActorConfig,

    /// 各合约的流式因子引擎
    /// Key: instrument_id, Value: StreamFactorEngine
    engines: Arc<RwLock<HashMap<String, StreamFactorEngine>>>,

    /// 订阅的合约列表（为空表示订阅所有合约）
    subscribed_instruments: Vec<String>,
}

impl FactorActor {
    /// 创建新的因子计算 Actor
    pub fn new(broadcaster: Arc<MarketDataBroadcaster>) -> Self {
        Self {
            broadcaster,
            config: FactorActorConfig::default(),
            engines: Arc::new(RwLock::new(HashMap::new())),
            subscribed_instruments: Vec::new(),
        }
    }

    /// 设置配置
    pub fn with_config(mut self, config: FactorActorConfig) -> Self {
        self.config = config;
        self
    }

    /// 订阅指定合约列表
    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.subscribed_instruments = instruments;
        self
    }

    /// 获取或创建合约的因子引擎
    fn get_or_create_engine(&self, instrument_id: &str) -> bool {
        let mut engines = self.engines.write();
        if engines.contains_key(instrument_id) {
            return true;
        }

        // 创建新的因子引擎
        let registry = FactorRegistry::with_standard_factors();
        let mut engine = StreamFactorEngine::new(registry);

        // 初始化所有启用的因子
        let mut success_count = 0;
        for factor_id in &self.config.factors {
            match engine.init_factor(factor_id) {
                Ok(_) => success_count += 1,
                Err(e) => {
                    log::warn!(
                        "📈 [FactorActor] Failed to init factor {} for {}: {}",
                        factor_id, instrument_id, e
                    );
                }
            }
        }

        engines.insert(instrument_id.to_string(), engine);
        log::info!(
            "📈 [FactorActor] Created factor engine for {} with {}/{} factors",
            instrument_id, success_count, self.config.factors.len()
        );

        true
    }

    /// 处理 K线完成事件
    fn on_kline_finished(
        &self,
        instrument_id: &str,
        period: i32,
        close_price: f64,
        timestamp: i64,
    ) {
        // 检查是否需要计算该周期的因子
        if !self.config.periods.contains(&period) {
            return;
        }

        // 确保因子引擎已创建
        self.get_or_create_engine(instrument_id);

        // 更新因子
        let mut engines = self.engines.write();
        if let Some(engine) = engines.get_mut(instrument_id) {
            let factor_ids: Vec<&str> = self.config.factors.iter().map(|s| s.as_str()).collect();
            let factor_values = engine.update_all(close_price, &factor_ids);

            if !factor_values.is_empty() {
                log::debug!(
                    "📈 [FactorActor] Factor update for {} period={}: {:?}",
                    instrument_id, period, factor_values
                );

                // 广播因子更新事件
                self.broadcaster.broadcast(MarketDataEvent::FactorUpdate {
                    instrument_id: instrument_id.to_string(),
                    factors: factor_values,
                    period,
                    timestamp,
                });
            }
        }
    }
}

impl Actor for FactorActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!(
            "📈 [FactorActor] Starting factor computation actor with {} factors, periods: {:?}",
            self.config.factors.len(),
            self.config.periods
        );

        // 订阅 KLineFinished 事件
        let subscriber_id = format!("factor_actor_{}", uuid::Uuid::new_v4());
        let receiver = self.broadcaster.subscribe(
            subscriber_id.clone(),
            self.subscribed_instruments.clone(),
            vec!["kline_finished".to_string()],
        );

        // 克隆需要移动到异步任务的数据
        let engines = self.engines.clone();
        let broadcaster = self.broadcaster.clone();
        let config = self.config.clone();

        // 启动异步任务处理 K线完成事件
        let fut = async move {
            log::info!(
                "📈 [FactorActor] Subscribed to KLineFinished events (subscriber_id={})",
                subscriber_id
            );

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
                        // 处理 KLineFinished 事件
                        if let MarketDataEvent::KLineFinished {
                            instrument_id,
                            period,
                            kline,
                            timestamp,
                        } = event
                        {
                            // 检查是否需要计算该周期的因子
                            if !config.periods.contains(&period) {
                                continue;
                            }

                            log::debug!(
                                "📈 [FactorActor] Received KLineFinished: {} period={} close={:.2}",
                                instrument_id, period, kline.close
                            );

                            // 确保因子引擎已创建
                            {
                                let mut eng = engines.write();
                                if !eng.contains_key(&instrument_id) {
                                    let registry = FactorRegistry::with_standard_factors();
                                    let mut engine = StreamFactorEngine::new(registry);
                                    for factor_id in &config.factors {
                                        let _ = engine.init_factor(factor_id);
                                    }
                                    eng.insert(instrument_id.clone(), engine);
                                    log::info!(
                                        "📈 [FactorActor] Created factor engine for {}",
                                        instrument_id
                                    );
                                }
                            }

                            // 更新因子
                            let mut eng = engines.write();
                            if let Some(engine) = eng.get_mut(&instrument_id) {
                                let factor_ids: Vec<&str> =
                                    config.factors.iter().map(|s| s.as_str()).collect();
                                let factor_values = engine.update_all(kline.close, &factor_ids);

                                if !factor_values.is_empty() {
                                    log::debug!(
                                        "📈 [FactorActor] Factor update for {}: {:?}",
                                        instrument_id, factor_values
                                    );

                                    // 广播因子更新事件
                                    broadcaster.broadcast(MarketDataEvent::FactorUpdate {
                                        instrument_id: instrument_id.clone(),
                                        factors: factor_values,
                                        period,
                                        timestamp,
                                    });
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
                        log::warn!("📈 [FactorActor] Market data channel disconnected");
                        break;
                    }
                    Err(e) => {
                        log::error!("📈 [FactorActor] spawn_blocking error: {}", e);
                        break;
                    }
                }
            }

            log::info!("📈 [FactorActor] Factor computation task ended");
        };

        ctx.spawn(actix::fut::wrap_future(fut));

        log::info!("📈 [FactorActor] Started successfully");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("📈 [FactorActor] Stopped");
    }
}

/// 因子值响应类型
pub type FactorValues = HashMap<String, f64>;

/// 所有合约因子值响应类型
pub type AllFactorValues = HashMap<String, HashMap<String, f64>>;

/// 查询因子值消息
#[derive(Message)]
#[rtype(result = "FactorValues")]
pub struct GetFactors {
    pub instrument_id: String,
}

impl Handler<GetFactors> for FactorActor {
    type Result = actix::MessageResult<GetFactors>;

    fn handle(&mut self, msg: GetFactors, _ctx: &mut Context<Self>) -> Self::Result {
        let engines = self.engines.read();
        let result = if let Some(engine) = engines.get(&msg.instrument_id) {
            engine.get_all().clone()
        } else {
            HashMap::new()
        };
        actix::MessageResult(result)
    }
}

/// 获取所有合约的因子值
#[derive(Message)]
#[rtype(result = "AllFactorValues")]
pub struct GetAllFactors;

impl Handler<GetAllFactors> for FactorActor {
    type Result = actix::MessageResult<GetAllFactors>;

    fn handle(&mut self, _msg: GetAllFactors, _ctx: &mut Context<Self>) -> Self::Result {
        let engines = self.engines.read();
        let result: AllFactorValues = engines
            .iter()
            .map(|(k, v)| (k.clone(), v.get_all().clone()))
            .collect();
        actix::MessageResult(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_factor_actor_creation() {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let actor = FactorActor::new(broadcaster);
        assert!(actor.engines.read().is_empty());
    }

    #[test]
    fn test_factor_actor_config() {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let config = FactorActorConfig {
            factors: vec!["ma5".to_string(), "rsi14".to_string()],
            periods: vec![4], // 只计算1分钟
        };
        let actor = FactorActor::new(broadcaster).with_config(config.clone());

        assert_eq!(actor.config.factors.len(), 2);
        assert_eq!(actor.config.periods, vec![4]);
    }

    #[test]
    fn test_get_or_create_engine() {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let actor = FactorActor::new(broadcaster);

        // 第一次创建
        assert!(actor.get_or_create_engine("IF2501"));
        assert!(actor.engines.read().contains_key("IF2501"));

        // 第二次应该直接返回
        assert!(actor.get_or_create_engine("IF2501"));
    }
}
