//! 行情快照发布服务(实时)
//!
//! **事件驱动 + 短窗口合并**:盘口变化 / 成交一到就把该合约标脏,`COALESCE` 窗口(10ms)
//! 内的事件合并成**一张**完整快照(10 档盘口 + 当日统计,见 `QuoteStats`)广播出去;
//! 另有心跳:任何合约超过 `heartbeat` 没发过快照也发一张(空闲盘口也能让客户端确认链路活着)。
//!
//! ⚠️ 设计约束(用户 2026-11-08 纠正)@yutiansut @quantaxis
//! qaexchange 的行情是**实时推送**,端到端 < 100ms。500ms 网格采样是**客户端**的事
//! (例如 hft_maker 适配器按 datetime 落网格),服务端不得把推送降成定时快照。
//! 本模块原先是「每 500ms 采一次」的定时器;一度被改成 quotes 只来自它,那是错的。
//!
//! 运行在**独立 OS 线程**:原先 `tokio::spawn` 在 `#[actix_web::main]` 的单线程主运行时上,
//! 被同线程的同步落盘(60s QIFI 快照、K 线 WAL fsync)饿死 —— 实测 8~10 秒只发出 0~1 张。

use crate::market::{MarketDataBroadcaster, MarketDataEvent, MarketDataService};
use crate::matching::engine::ExchangeMatchingEngine;
use crossbeam::channel::{Receiver, RecvTimeoutError};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 合并窗口:首个事件到达后再收这么久,然后统一发布
const COALESCE: Duration = Duration::from_millis(10);
/// 无事件时多久醒一次检查心跳
const IDLE_WAKE: Duration = Duration::from_millis(50);
/// 本服务在广播器上的订阅者 ID(订全部合约的 tick + orderbook 频道)
const SUBSCRIBER_ID: &str = "__quote_publisher__";
/// 每张快照的盘口深度
const DEPTH: usize = 10;

/// 行情快照发布服务
pub struct SnapshotBroadcastService {
    market_data_service: Arc<MarketDataService>,
    market_broadcaster: Arc<MarketDataBroadcaster>,
    matching_engine: Arc<ExchangeMatchingEngine>,
}

impl SnapshotBroadcastService {
    /// 创建快照发布服务
    ///
    /// ⚠️ 必须传入**进程里那一个** `MarketDataService`(成交经 `on_trade` 进的就是它)。
    /// 原实现在这里 `MarketDataService::new(matching_engine)` 另起一个实例 ——
    /// 完整快照要读当日 K 线算累计量/额、昨收,另起的实例里 K 线永远是空的。
    pub fn new(
        matching_engine: Arc<ExchangeMatchingEngine>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
        market_data_service: Arc<MarketDataService>,
    ) -> Self {
        Self {
            market_data_service,
            market_broadcaster,
            matching_engine,
        }
    }

    fn subscribe(&self) -> Receiver<MarketDataEvent> {
        self.market_broadcaster.subscribe(
            SUBSCRIBER_ID.to_string(),
            vec![], // 全部合约
            vec!["tick".to_string(), "orderbook".to_string()],
        )
    }

    /// 只有「盘口变了 / 有成交」才标脏。自己发出的 `OrderBookSnapshot` 也走 orderbook 频道,
    /// 必须忽略,否则会自激成死循环。
    fn mark(event: &MarketDataEvent, dirty: &mut HashSet<String>) {
        match event {
            MarketDataEvent::Tick { instrument_id, .. }
            | MarketDataEvent::OrderBookUpdate { instrument_id, .. } => {
                dirty.insert(instrument_id.clone());
            }
            _ => {}
        }
    }

    /// 发布一个合约的完整快照
    fn publish(&self, instrument_id: &str) {
        match self.market_data_service.quote_snapshot(instrument_id, DEPTH) {
            Ok((snapshot, stats)) => self.market_broadcaster.broadcast_quote_snapshot(snapshot, stats),
            Err(e) => log::warn!("Failed to build quote snapshot for {}: {}", instrument_id, e),
        }
    }

    /// 发布循环(阻塞,跑在独立线程上)
    fn run(&self, heartbeat: Duration) {
        log::info!(
            "Quote publisher started (realtime: coalesce {:?}, heartbeat {:?})",
            COALESCE,
            heartbeat
        );
        let mut rx = self.subscribe();
        let mut dirty: HashSet<String> = HashSet::new();
        let mut last_pub: HashMap<String, Instant> = HashMap::new();

        loop {
            match rx.recv_timeout(IDLE_WAKE) {
                Ok(ev) => {
                    Self::mark(&ev, &mut dirty);
                    // 合并窗口:把窗口内陆续到达的事件并成一张快照
                    let deadline = Instant::now() + COALESCE;
                    loop {
                        let now = Instant::now();
                        if now >= deadline {
                            break;
                        }
                        match rx.recv_timeout(deadline - now) {
                            Ok(ev) => Self::mark(&ev, &mut dirty),
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => {
                                rx = self.subscribe();
                                break;
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                // 广播器会清理「慢订阅者」(连续投递失败)—— 被清掉就重新订阅,不能静默停推
                Err(RecvTimeoutError::Disconnected) => {
                    log::warn!("Quote publisher subscription dropped by broadcaster, resubscribing");
                    rx = self.subscribe();
                }
            }

            // 心跳:超过 heartbeat 没发过的合约也补一张
            for inst in self.matching_engine.get_instruments() {
                if last_pub.get(&inst).map_or(true, |t| t.elapsed() >= heartbeat) {
                    dirty.insert(inst);
                }
            }

            for inst in dirty.drain() {
                self.publish(&inst);
                last_pub.insert(inst, Instant::now());
            }
        }
    }

    /// 启动发布线程。`interval_ms` 为心跳间隔(空闲合约多久至少发一张)。
    pub fn spawn(
        matching_engine: Arc<ExchangeMatchingEngine>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
        market_data_service: Arc<MarketDataService>,
        interval_ms: u64,
    ) -> std::thread::JoinHandle<()> {
        // 持仓量要逐账户取读锁、会被落盘写锁拖住 —— 放到独立线程每秒刷一次缓存,
        // 发布线程只读缓存(见 MarketDataService::open_interest_cached)@yutiansut @quantaxis
        let oi_mds = market_data_service.clone();
        std::thread::Builder::new()
            .name("oi-refresher".to_string())
            .spawn(move || loop {
                oi_mds.refresh_open_interest();
                std::thread::sleep(Duration::from_secs(1));
            })
            .expect("failed to spawn open-interest refresher thread");

        let service = Self::new(matching_engine, market_broadcaster, market_data_service);
        std::thread::Builder::new()
            .name("quote-publisher".to_string())
            .spawn(move || service.run(Duration::from_millis(interval_ms)))
            .expect("failed to spawn quote publisher thread")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::config::InstrumentConfig;

    /// ✨ 实时性:一笔成交之后,订阅者应在 ~COALESCE 量级内收到带统计的完整快照,
    /// 而不是等到下一个心跳。心跳故意设成 60 秒,收到即证明是事件驱动的。@yutiansut @quantaxis
    #[test]
    fn trade_triggers_snapshot_within_coalesce_window() {
        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("IH2501".to_string(), 2800.0).unwrap();
        let bc = Arc::new(MarketDataBroadcaster::new());
        let mut mds = MarketDataService::new(engine.clone()).with_broadcaster(bc.clone());
        mds.set_instrument_configs(vec![InstrumentConfig {
            instrument_id: "IH2501".into(), name: "IH".into(), exchange_id: "CFFEX".into(),
            product_type: "index_future".into(), init_price: 0.0, is_trading: true,
            multiplier: 300.0, tick_size: 0.2,
            commission_per_lot: None, commission_rate: None, margin_rate: None,
        }]);
        let mds = Arc::new(mds);
        let rx = bc.subscribe("client".into(), vec!["IH2501".into()], vec!["orderbook".into()]);
        let _h = SnapshotBroadcastService::spawn(engine, bc, mds.clone(), 60_000);

        // 首次心跳(启动即到期)发一张,先把它吃掉
        let _ = rx.recv_timeout(Duration::from_secs(2)).expect("startup heartbeat");
        std::thread::sleep(Duration::from_millis(50));
        while rx.try_recv().is_ok() {}

        let t0 = Instant::now();
        mds.on_trade("IH2501", 1810.0, 3, "buy");
        let ev = rx.recv_timeout(Duration::from_secs(2)).expect("event-driven snapshot");
        let lat = t0.elapsed();
        match ev {
            MarketDataEvent::OrderBookSnapshot { stats: Some(st), .. } => {
                assert_eq!(st.volume, 3);
                assert_eq!(st.last_price, 1810.0);
            }
            other => panic!("unexpected event: {:?}", other),
        }
        assert!(lat < Duration::from_millis(500), "事件驱动应远快于心跳,实测 {:?}", lat);
    }
}
