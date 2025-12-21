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
        let instruments = self.config.instruments.clone();

        std::thread::spawn(move || {
            log::info!(
                "Market snapshot generator started (interval: {}ms, instruments: {})",
                self.config.interval_ms,
                instruments.len()
            );

            loop {
                std::thread::sleep(interval);

                // 为每个合约生成快照
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

        let mut ob = orderbook.write();

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

        // 启动转发线程
        let snapshot_rx = self.snapshot_rx.clone();
        std::thread::spawn(move || {
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
}
