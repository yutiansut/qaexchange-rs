//! 市场快照生成器
//!
//! 每秒生成一次市场行情快照，包含：
//! - 最新价、涨跌幅
//! - 买卖五档
//! - 成交量、成交额
//! - 持仓量
//! - 最高价、最低价、开盘价

use crate::exchange::AccountManager;
use crate::matching::engine::ExchangeMatchingEngine;
use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// 市场快照（每秒级别）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    /// 合约ID
    pub instrument_id: String,

    /// 快照时间戳（纳秒）
    pub timestamp: i64,

    /// 交易日期
    pub trading_day: String,

    /// 最新价
    pub last_price: f64,

    /// 涨跌幅（百分比）
    pub change_percent: f64,

    /// 涨跌额
    pub change_amount: f64,

    /// 买一价
    pub bid_price1: f64,
    /// 买一量
    pub bid_volume1: i64,
    /// 买二价
    pub bid_price2: f64,
    /// 买二量
    pub bid_volume2: i64,
    /// 买三价
    pub bid_price3: f64,
    /// 买三量
    pub bid_volume3: i64,
    /// 买四价
    pub bid_price4: f64,
    /// 买四量
    pub bid_volume4: i64,
    /// 买五价
    pub bid_price5: f64,
    /// 买五量
    pub bid_volume5: i64,

    /// 卖一价
    pub ask_price1: f64,
    /// 卖一量
    pub ask_volume1: i64,
    /// 卖二价
    pub ask_price2: f64,
    /// 卖二量
    pub ask_volume2: i64,
    /// 卖三价
    pub ask_price3: f64,
    /// 卖三量
    pub ask_volume3: i64,
    /// 卖四价
    pub ask_price4: f64,
    /// 卖四量
    pub ask_volume4: i64,
    /// 卖五价
    pub ask_price5: f64,
    /// 卖五量
    pub ask_volume5: i64,

    /// 今日开盘价
    pub open: f64,
    /// 今日最高价
    pub high: f64,
    /// 今日最低价
    pub low: f64,
    /// 昨收盘价
    pub pre_close: f64,

    /// 成交量（手）
    pub volume: i64,
    /// 成交额
    pub turnover: f64,

    /// 持仓量
    pub open_interest: i64,

    /// 涨停价
    pub upper_limit: f64,
    /// 跌停价
    pub lower_limit: f64,
}

impl Default for MarketSnapshot {
    fn default() -> Self {
        Self {
            instrument_id: String::new(),
            timestamp: 0,
            trading_day: String::new(),
            last_price: 0.0,
            change_percent: 0.0,
            change_amount: 0.0,
            bid_price1: 0.0,
            bid_volume1: 0,
            bid_price2: 0.0,
            bid_volume2: 0,
            bid_price3: 0.0,
            bid_volume3: 0,
            bid_price4: 0.0,
            bid_volume4: 0,
            bid_price5: 0.0,
            bid_volume5: 0,
            ask_price1: 0.0,
            ask_volume1: 0,
            ask_price2: 0.0,
            ask_volume2: 0,
            ask_price3: 0.0,
            ask_volume3: 0,
            ask_price4: 0.0,
            ask_volume4: 0,
            ask_price5: 0.0,
            ask_volume5: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            pre_close: 0.0,
            volume: 0,
            turnover: 0.0,
            open_interest: 0,
            upper_limit: 0.0,
            lower_limit: 0.0,
        }
    }
}

/// 快照生成器配置
/// 快照通道在**无人消费**时的最大堆积条数,超出即丢最旧的。
///
/// 512 条 × ~344 B ≈ 176 KB 上限。取这个量级是因为:
/// ① 足够让一个刚建立的订阅者拿到近期历史(1 秒/条时约 8.5 分钟);
/// ② 又远小于无界堆积的代价(实测理论 4.7 MB/小时 = 113 MB/天)。
/// @yutiansut @quantaxis
const SNAPSHOT_CHANNEL_CAP: usize = 512;

#[derive(Debug, Clone)]
pub struct SnapshotGeneratorConfig {
    /// 快照生成间隔（毫秒）
    pub interval_ms: u64,

    /// 是否启用快照持久化
    pub enable_persistence: bool,

    /// 订阅的合约列表
    pub instruments: Vec<String>,
}

impl Default for SnapshotGeneratorConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1000, // 默认1秒
            enable_persistence: true,
            instruments: Vec::new(),
        }
    }
}

/// 市场快照生成器
pub struct MarketSnapshotGenerator {
    /// 撮合引擎引用
    matching_engine: Arc<ExchangeMatchingEngine>,

    /// 账户管理器（用于统计持仓）
    account_manager: Option<Arc<AccountManager>>,

    /// 配置
    config: SnapshotGeneratorConfig,

    /// 广播通道（发送端）
    snapshot_tx: Sender<MarketSnapshot>,

    /// 广播通道（接收端 - 用于克隆给订阅者）
    snapshot_rx: Arc<RwLock<Receiver<MarketSnapshot>>>,

    /// 统计：已生成快照数
    snapshot_count: Arc<RwLock<u64>>,

    /// 活跃订阅者数量。@yutiansut @quantaxis
    ///
    /// 用来在**无人订阅时整轮跳过快照生成** —— 生成一次要对**每个合约**
    /// 遍历全部账户取**写锁**(`account_mgr.rs:get_instrument_open_interest`),
    /// 生产 4 合约 × 82 账户 = **328 次账户写锁/秒**,还与同样 1 秒一轮的
    /// `RiskMonitor` 和下单路径争锁。而 `MarketDataService::subscribe_snapshots`
    /// 生产**零调用方** ⇒ 这些锁全是白取的。
    subscriber_count: Arc<std::sync::atomic::AtomicUsize>,

    /// 历史数据缓存（合约ID -> 日内统计）
    daily_stats: Arc<RwLock<HashMap<String, DailyStats>>>,
}

/// 日内统计数据
#[derive(Debug, Clone)]
struct DailyStats {
    /// 开盘价
    open: f64,
    /// 最高价
    high: f64,
    /// 最低价
    low: f64,
    /// 昨收盘价
    pre_close: f64,
    /// 成交量累计
    volume: i64,
    /// 成交额累计
    turnover: f64,
}

impl MarketSnapshotGenerator {
    /// 创建快照生成器
    pub fn new(
        matching_engine: Arc<ExchangeMatchingEngine>,
        config: SnapshotGeneratorConfig,
    ) -> Self {
        let (tx, rx) = unbounded();

        Self {
            matching_engine,
            account_manager: None,
            config,
            snapshot_tx: tx,
            snapshot_rx: Arc::new(RwLock::new(rx)),
            snapshot_count: Arc::new(RwLock::new(0)),
            subscriber_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            daily_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注入账户管理器
    pub fn with_account_manager(mut self, account_manager: Arc<AccountManager>) -> Self {
        self.account_manager = Some(account_manager);
        self
    }

    /// 启动快照生成器
    ///
    /// # 返回
    /// 返回后台线程句柄
    pub fn start(self: Arc<Self>) -> std::thread::JoinHandle<()> {
        let interval = Duration::from_millis(self.config.interval_ms);
        // 兜底列表：仅在撮合引擎尚未注册任何合约时使用 @yutiansut @quantaxis
        let seed_instruments = self.config.instruments.clone();

        std::thread::spawn(move || {
            log::info!(
                "Market snapshot generator started (interval: {}ms, seed instruments: {})",
                self.config.interval_ms,
                seed_instruments.len()
            );

            loop {
                std::thread::sleep(interval);

                // 每轮从撮合引擎重读合约表 @yutiansut @quantaxis
                //
                // 原实现在 start() 时 clone 了 config.instruments 进闭包，此后永久冻结：
                // 通过 /api/admin/instrument/create 新增的合约永远拿不到快照，
                // 前端行情、涨跌幅、快照广播全都看不到它。
                // 撮合引擎的 orderbooks 才是「哪些合约存在」的唯一真相源。
                let instruments = {
                    let live = self.matching_engine.get_instruments();
                    if live.is_empty() {
                        seed_instruments.clone()
                    } else {
                        live
                    }
                };

                // 为每个合约生成快照
                // ✨ 无人订阅就整轮跳过 —— 见 `subscriber_count` 字段说明。
                // 生成一次要对每个合约遍历全部账户取写锁,而输出没人要。
                // @yutiansut @quantaxis
                if self.subscriber_count.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                    continue;
                }

                for instrument_id in &instruments {
                    if let Ok(snapshot) = self.generate_snapshot(instrument_id) {
                        // 广播快照
                        if let Err(e) = self.snapshot_tx.send(snapshot.clone()) {
                            log::error!(
                                "Failed to broadcast snapshot for {}: {}",
                                instrument_id,
                                e
                            );
                        } else {
                            *self.snapshot_count.write() += 1;

                            // ✨ 无人消费时给通道封顶,否则无界堆积。@yutiansut @quantaxis
                            //
                            // `snapshot_tx` 发进的是 `unbounded()` 通道(:217),
                            // 唯一读取点在 `subscribe()` 的转发线程里(:534),而
                            // `MarketDataService::subscribe_snapshots`(market/mod.rs:185)
                            // **生产零调用方** ⇒ 转发线程从不启动 ⇒ 没有任何东西排空它。
                            // 单测 `unsubscribed_channel_accumulates_without_bound` 实测:
                            // 无订阅者时 600ms 积 11 条、1200ms 积 23 条(线性增长),
                            // 而**有订阅者时恒为 0** —— 成因确实是没有消费者。
                            //
                            // ⚠️ 为什么不改成 `bounded()`:那会让**已订阅者**在慢消费时
                            // 丢最新的一条(send 失败),语义变差。这里改为丢**最旧**的:
                            // 快照是「当前状态」,旧的本就没有保留价值。
                            //
                            // ⚠️ 为什么阈值取 SNAPSHOT_CHANNEL_CAP 而不是 0:有真实订阅者时
                            // 队列会稳定在 0 附近,这段几乎不触发;只有在无人消费时才生效。
                            // 于是它对正常路径零影响,只兜住异常。
                            let pending = self.snapshot_rx.read().len();
                            if pending > SNAPSHOT_CHANNEL_CAP {
                                let rx = self.snapshot_rx.read();
                                for _ in 0..(pending - SNAPSHOT_CHANNEL_CAP) {
                                    if rx.try_recv().is_err() {
                                        break;
                                    }
                                }
                            }
                            log::trace!(
                                "Generated snapshot for {}: last_price={:.2}",
                                instrument_id,
                                snapshot.last_price
                            );
                        }
                    }
                }
            }
        })
    }

    /// 生成单个合约的快照
    fn generate_snapshot(&self, instrument_id: &str) -> Result<MarketSnapshot, String> {
        // 获取订单簿
        let orderbook = self
            .matching_engine
            .get_orderbook(instrument_id)
            .ok_or_else(|| format!("Orderbook not found for {}", instrument_id))?;

        // ✨ 只读快照,用读锁 —— 原为 `orderbook.write()` @yutiansut @quantaxis
        //
        // 写锁是 `ob.get_depth()`(下方已注释掉)留下的遗迹。本函数对订单簿的
        // 全部访问都是只读:`bid_queue.get_sorted_orders()` /
        // `ask_queue.get_sorted_orders()` / `lastprice`。
        //
        // 用写锁的两个代价:
        // ① **活性**:此处是全仓库唯一把「订单簿锁」和「daily_stats 锁」嵌套起来的
        //    地方(:382 `self.daily_stats.write()`),中间没有 drop,独占锁一攥 80 行。
        // ② **吞吐**:快照线程每轮对**每个合约**独占订单簿,期间该合约所有撮合
        //    (order_router.rs:964 `orderbook.write()`)全部排队;parking_lot 写优先
        //    还会连带把 `get_orderbook_snapshot` 的读者一起挡住。
        let ob = orderbook.read();

        // 获取买卖五档（qars 的 get_depth 不带参数，需要手动提取前5档）
        //ob.get_depth(); // 更新内部深度

        // 从 qars orderbook 手动提取买卖5档（聚合相同价格的订单数量）
        let bids = if let Some(bid_orders) = ob.bid_queue.get_sorted_orders() {
            use std::collections::HashMap;
            let mut price_map: HashMap<String, f64> = HashMap::new();

            // 聚合相同价格的订单数量
            for order in bid_orders.iter().take(50) {
                // 取足够多的订单以便聚合
                *price_map.entry(order.price.to_string()).or_insert(0.0) += order.volume;
            }

            // 转换为 PriceLevel 并排序
            let mut levels: Vec<super::PriceLevel> = price_map
                .into_iter()
                .filter_map(|(price_str, volume)| {
                    price_str
                        .parse::<f64>()
                        .ok()
                        .map(|price| super::PriceLevel {
                            price,
                            volume: volume as i64,
                        })
                })
                .collect();

            // 买盘按价格降序排列（价格从高到低）
            levels.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());

            // 取前5档
            levels.into_iter().take(5).collect()
        } else {
            Vec::new()
        };

        let asks = if let Some(ask_orders) = ob.ask_queue.get_sorted_orders() {
            use std::collections::HashMap;
            let mut price_map: HashMap<String, f64> = HashMap::new();

            // 聚合相同价格的订单数量
            for order in ask_orders.iter().take(50) {
                // 取足够多的订单以便聚合
                *price_map.entry(order.price.to_string()).or_insert(0.0) += order.volume;
            }

            // 转换为 PriceLevel 并排序
            let mut levels: Vec<super::PriceLevel> = price_map
                .into_iter()
                .filter_map(|(price_str, volume)| {
                    price_str
                        .parse::<f64>()
                        .ok()
                        .map(|price| super::PriceLevel {
                            price,
                            volume: volume as i64,
                        })
                })
                .collect();

            // 卖盘按价格升序排列（价格从低到高）
            levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

            // 取前5档
            levels.into_iter().take(5).collect()
        } else {
            Vec::new()
        };

        // 获取最新价（qars 的 lastprice 是 f64，不是 Option<f64>）
        let last_price = if ob.lastprice > 0.0 {
            ob.lastprice
        } else {
            0.0
        };

        // 获取日内统计
        let mut daily_stats = self.daily_stats.write();
        let stats = daily_stats
            .entry(instrument_id.to_string())
            .or_insert_with(|| {
                let pre_close = self
                    .matching_engine
                    .get_prev_close(instrument_id)
                    .unwrap_or(last_price);
                DailyStats {
                    open: last_price,
                    high: last_price,
                    low: last_price,
                    pre_close,
                    volume: 0,
                    turnover: 0.0,
                }
            });

        // 更新统计数据
        if last_price > stats.high {
            stats.high = last_price;
        }
        if last_price < stats.low || stats.low == 0.0 {
            stats.low = last_price;
        }

        // 计算涨跌幅
        let change_amount = last_price - stats.pre_close;
        let change_percent = if stats.pre_close > 0.0 {
            (change_amount / stats.pre_close) * 100.0
        } else {
            0.0
        };

        // 构建快照
        let mut snapshot = MarketSnapshot {
            instrument_id: instrument_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            trading_day: chrono::Local::now().format("%Y%m%d").to_string(),
            last_price,
            change_percent,
            change_amount,
            open: stats.open,
            high: stats.high,
            low: stats.low,
            pre_close: stats.pre_close,
            volume: stats.volume,
            turnover: stats.turnover,
            open_interest: self
                .account_manager
                .as_ref()
                .map(|mgr| mgr.get_instrument_open_interest(instrument_id))
                .unwrap_or(0),
            upper_limit: stats.pre_close * 1.10, // 假设10%涨停
            lower_limit: stats.pre_close * 0.90, // 假设10%跌停
            ..Default::default()
        };

        // 填充买五档
        if let Some(level) = bids.first() {
            snapshot.bid_price1 = level.price;
            snapshot.bid_volume1 = level.volume;
        }
        if let Some(level) = bids.get(1) {
            snapshot.bid_price2 = level.price;
            snapshot.bid_volume2 = level.volume;
        }
        if let Some(level) = bids.get(2) {
            snapshot.bid_price3 = level.price;
            snapshot.bid_volume3 = level.volume;
        }
        if let Some(level) = bids.get(3) {
            snapshot.bid_price4 = level.price;
            snapshot.bid_volume4 = level.volume;
        }
        if let Some(level) = bids.get(4) {
            snapshot.bid_price5 = level.price;
            snapshot.bid_volume5 = level.volume;
        }

        // 填充卖五档
        if let Some(level) = asks.first() {
            snapshot.ask_price1 = level.price;
            snapshot.ask_volume1 = level.volume;
        }
        if let Some(level) = asks.get(1) {
            snapshot.ask_price2 = level.price;
            snapshot.ask_volume2 = level.volume;
        }
        if let Some(level) = asks.get(2) {
            snapshot.ask_price3 = level.price;
            snapshot.ask_volume3 = level.volume;
        }
        if let Some(level) = asks.get(3) {
            snapshot.ask_price4 = level.price;
            snapshot.ask_volume4 = level.volume;
        }
        if let Some(level) = asks.get(4) {
            snapshot.ask_price5 = level.price;
            snapshot.ask_volume5 = level.volume;
        }

        Ok(snapshot)
    }

    /// 更新成交统计（由外部调用）
    pub fn update_trade_stats(&self, instrument_id: &str, volume: i64, turnover: f64) {
        let mut daily_stats = self.daily_stats.write();
        if let Some(stats) = daily_stats.get_mut(instrument_id) {
            stats.volume += volume;
            stats.turnover += turnover;
        }
    }

    /// 设置昨收盘价
    pub fn set_pre_close(&self, instrument_id: &str, pre_close: f64) {
        let mut daily_stats = self.daily_stats.write();
        let stats = daily_stats
            .entry(instrument_id.to_string())
            .or_insert_with(|| DailyStats {
                open: pre_close,
                high: pre_close,
                low: pre_close,
                pre_close,
                volume: 0,
                turnover: 0.0,
            });
        stats.pre_close = pre_close;
    }

    /// 创建新的订阅者
    pub fn subscribe(&self) -> Receiver<MarketSnapshot> {
        // 创建新的通道对
        let (tx, rx) = unbounded();

        // ✨ 订阅者计数 +1;转发线程退出时 -1。@yutiansut @quantaxis
        use std::sync::atomic::Ordering;
        self.subscriber_count.fetch_add(1, Ordering::SeqCst);
        let sub_cnt = self.subscriber_count.clone();

        // 启动转发线程
        let snapshot_rx = self.snapshot_rx.clone();
        std::thread::spawn(move || {
            // 无论从哪个分支退出都要减回来 —— 用 guard 保证
            struct DecOnDrop(Arc<std::sync::atomic::AtomicUsize>);
            impl Drop for DecOnDrop {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _dec = DecOnDrop(sub_cnt);
            loop {
                // 从主通道接收
                let rx_guard = snapshot_rx.read();
                if let Ok(snapshot) = rx_guard.try_recv() {
                    drop(rx_guard); // 尽早释放锁

                    // 转发到订阅者
                    if tx.send(snapshot).is_err() {
                        log::debug!("Snapshot subscriber disconnected");
                        break;
                    }
                } else {
                    drop(rx_guard);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });

        rx
    }

    /// 获取已生成快照数量
    pub fn get_snapshot_count(&self) -> u64 {
        *self.snapshot_count.read()
    }

    /// 通道里**尚未被消费**的快照条数。@yutiansut @quantaxis
    ///
    /// 加这个是为了让「快照通道是否无界堆积」可测,而不是靠 RSS 推断:
    /// `snapshot_tx` 发进的是 `unbounded()` 通道(:217),唯一的读取点在
    /// `subscribe()` 的转发线程里(:534),而 `MarketDataService::subscribe_snapshots`
    /// **零调用方** ⇒ 若确实没人订阅,这个值会随时间单调增长。
    ///
    /// ⚠️ 为什么不能用 RSS 判:实测理论增速仅 4.7 MB/小时(= 0.078 MB/分钟),
    /// 而生产 RSS 逐分钟标准差 **17.2 MB**(噪声/信号 220 倍);
    /// 空载实例上则因 mimalloc arena 有余量而完全不动。
    /// 直接读通道长度是唯一能一次定论的办法。
    /// 当前活跃订阅者数(诊断用)。@yutiansut @quantaxis
    pub fn subscriber_count(&self) -> usize {
        self.subscriber_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn pending_len(&self) -> usize {
        self.snapshot_rx.read().len()
    }

    /// 重置日内统计（每日开盘时调用）
    pub fn reset_daily_stats(&self) {
        let mut daily_stats = self.daily_stats.write();
        daily_stats.clear();
        log::info!("Daily stats reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_default() {
        let snapshot = MarketSnapshot::default();
        assert_eq!(snapshot.last_price, 0.0);
        assert_eq!(snapshot.volume, 0);
    }

    #[test]
    fn test_config_default() {
        let config = SnapshotGeneratorConfig::default();
        assert_eq!(config.interval_ms, 1000);
        assert!(config.enable_persistence);
    }

    /// 通道封顶仍然有效 —— 订阅者存在但**消费不过来**时不会无界堆积。
    /// @yutiansut @quantaxis
    ///
    /// ⚠️ 这个测试是从两个旧测试合并改写来的。旧的两个测「**无订阅者**时通道堆积」,
    /// 而 2026-09-06 加了「无订阅者整轮跳过生成」之后,**那个前提不存在了** ——
    /// 无订阅者时一条都不产生,自然不会堆积。旧测试不是被改坏,是**被更好的行为取代**。
    ///
    /// 现在真正需要防的是另一种情况:**有订阅者但消费慢**。
    /// 这里持有 Receiver 却不读,模拟一个卡住的消费者。
    #[test]
    fn channel_is_capped_when_consumer_stalls() {
        use std::sync::Arc;
        use std::time::Duration;

        let engine = Arc::new(crate::matching::engine::ExchangeMatchingEngine::new());
        engine.register_instrument("TESTZ2501".to_string(), 100.0).unwrap();

        let gen = Arc::new(MarketSnapshotGenerator::new(
            engine,
            SnapshotGeneratorConfig {
                interval_ms: 1,
                enable_persistence: false,
                instruments: vec!["TESTZ2501".to_string()],
            },
        ));

        // 持有 Receiver 但**从不读** —— 转发线程会把主通道搬到这个 rx 里,
        // 主通道因此不会涨;真正会涨的是订阅者自己那条。
        // 我们要断言的是**主通道有上限**,即使转发线程跟不上也不会失控。
        let _rx = gen.subscribe();
        assert_eq!(gen.subscriber_count(), 1);

        let _h = gen.clone().start();
        std::thread::sleep(Duration::from_millis(2500));
        let produced = gen.get_snapshot_count();
        let pending = gen.pending_len();

        println!(
            "  [封顶实测] 消费者卡住: 已生成 {} 条,主通道内 {} 条(上限 {})",
            produced, pending, SNAPSHOT_CHANNEL_CAP
        );
        assert!(produced > 0, "有订阅者时应当持续生成,实得 {}", produced);
        assert!(
            pending <= SNAPSHOT_CHANNEL_CAP + 8,
            "主通道应被封顶在 {} 附近,实得 {}(产量 {})",
            SNAPSHOT_CHANNEL_CAP,
            pending,
            produced
        );
    }

    /// 无订阅者时**不生成快照**;一旦有订阅者立即恢复。@yutiansut @quantaxis
    ///
    /// 判错方向不对称:
    ///   该生成却没生成 ⇒ 订阅者收不到数据(功能坏)
    ///   不该生成却生成 ⇒ 白取写锁(仅浪费)
    /// 所以测试重点在「有订阅者时必须照常生成」。
    #[test]
    fn skips_generation_when_nobody_subscribes() {
        use std::sync::Arc;
        use std::time::Duration;

        let engine = Arc::new(crate::matching::engine::ExchangeMatchingEngine::new());
        engine.register_instrument("TESTQ2501".to_string(), 100.0).unwrap();

        let gen = Arc::new(MarketSnapshotGenerator::new(
            engine,
            SnapshotGeneratorConfig {
                interval_ms: 20,
                enable_persistence: false,
                instruments: vec!["TESTQ2501".to_string()],
            },
        ));
        assert_eq!(gen.subscriber_count(), 0, "起始应无订阅者");

        let _h = gen.clone().start();
        std::thread::sleep(Duration::from_millis(400));
        let idle = gen.get_snapshot_count();
        println!("  [无订阅者] 400ms 后生成 {} 条", idle);
        assert_eq!(idle, 0, "无订阅者时不该生成,实得 {}", idle);

        // 一有订阅者立即恢复生成
        let rx = gen.subscribe();
        assert_eq!(gen.subscriber_count(), 1, "subscribe 后计数应为 1");
        std::thread::sleep(Duration::from_millis(400));
        let active = gen.get_snapshot_count();
        println!("  [有订阅者] 再过 400ms 生成 {} 条", active);
        assert!(active > 0, "有订阅者时必须生成,实得 {}", active);

        // 订阅者断开后计数归零、生成再次停止
        drop(rx);
        std::thread::sleep(Duration::from_millis(500));
        let after_drop_cnt = gen.subscriber_count();
        let a = gen.get_snapshot_count();
        std::thread::sleep(Duration::from_millis(400));
        let b = gen.get_snapshot_count();
        println!("  [断开后] 订阅者数 {}  生成量 {} → {}", after_drop_cnt, a, b);
        assert_eq!(after_drop_cnt, 0, "订阅者断开后计数应归零");
        assert_eq!(a, b, "断开后不该继续生成,实得 {} → {}", a, b);
    }
}
