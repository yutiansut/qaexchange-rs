//! 高性能撮合引擎
//!
//! @yutiansut @quantaxis
//!
//! Phase 5.2 性能优化集成：
//! - CPU 亲和性绑定（撮合线程固定到核心 0）
//! - 预分配内存池（订单/成交回报对象复用）
//! - SPSC 无锁队列（订单输入/成交输出）
//!
//! 性能目标：
//! - 撮合延迟：P99 < 50μs
//! - 订单吞吐量：> 500K orders/sec
//! - 零内存分配（热路径）

use crate::matching::engine::InstrumentAsset;
use crate::matching::Orderbook;
use crate::perf::{
    bind_to_core, spawn_on_core, spsc_channel, CpuAffinityConfig, OrderPool, PerfConfig,
    PerfContext, PoolConfig, PooledOrder, PooledTradeReport, SpscReceiver, SpscSender,
    TradeReportPool,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// 高性能撮合引擎配置
// ═══════════════════════════════════════════════════════════════════════════

/// 高性能撮合引擎配置
#[derive(Debug, Clone)]
pub struct HighPerfMatchingConfig {
    /// 撮合引擎绑定的 CPU 核心
    pub matching_core: usize,

    /// 订单队列容量
    pub order_queue_capacity: usize,

    /// 成交队列容量
    pub trade_queue_capacity: usize,

    /// 订单池配置
    pub order_pool_config: PoolConfig,

    /// 成交池配置
    pub trade_pool_config: PoolConfig,

    /// 批量处理大小
    pub batch_size: usize,

    /// 是否启用 CPU 亲和性
    pub enable_cpu_affinity: bool,

    /// 撮合超时（微秒）
    pub matching_timeout_us: u64,
}

impl Default for HighPerfMatchingConfig {
    fn default() -> Self {
        Self {
            matching_core: 0,
            order_queue_capacity: 100_000,
            trade_queue_capacity: 100_000,
            order_pool_config: PoolConfig {
                initial_capacity: 50_000,
                max_capacity: 500_000,
                prewarm: true,
            },
            trade_pool_config: PoolConfig {
                initial_capacity: 50_000,
                max_capacity: 500_000,
                prewarm: true,
            },
            batch_size: 100,
            enable_cpu_affinity: true,
            matching_timeout_us: 100,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 撮合引擎统计
// ═══════════════════════════════════════════════════════════════════════════

/// 撮合引擎统计信息
#[derive(Debug, Default)]
pub struct MatchingStats {
    /// 处理的订单数
    pub orders_processed: AtomicU64,

    /// 产生的成交数
    pub trades_generated: AtomicU64,

    /// 总撮合延迟（纳秒）
    pub total_latency_ns: AtomicU64,

    /// 最大撮合延迟（纳秒）
    pub max_latency_ns: AtomicU64,

    /// 最小撮合延迟（纳秒）
    pub min_latency_ns: AtomicU64,

    /// 队列满导致的丢弃
    pub queue_drops: AtomicU64,
}

impl MatchingStats {
    /// 记录撮合延迟
    pub fn record_latency(&self, latency_ns: u64) {
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);

        // 更新最大值（无锁 CAS）
        let mut current_max = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match self.max_latency_ns.compare_exchange_weak(
                current_max,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        // 更新最小值
        let mut current_min = self.min_latency_ns.load(Ordering::Relaxed);
        if current_min == 0 {
            self.min_latency_ns
                .compare_exchange(0, latency_ns, Ordering::Relaxed, Ordering::Relaxed)
                .ok();
        } else {
            while latency_ns < current_min {
                match self.min_latency_ns.compare_exchange_weak(
                    current_min,
                    latency_ns,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(x) => current_min = x,
                }
            }
        }
    }

    /// 获取平均延迟（纳秒）
    pub fn avg_latency_ns(&self) -> f64 {
        let count = self.orders_processed.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            self.total_latency_ns.load(Ordering::Relaxed) as f64 / count as f64
        }
    }

    /// 打印统计信息
    pub fn print_stats(&self) {
        let orders = self.orders_processed.load(Ordering::Relaxed);
        let trades = self.trades_generated.load(Ordering::Relaxed);
        let avg_ns = self.avg_latency_ns();
        let max_ns = self.max_latency_ns.load(Ordering::Relaxed);
        let min_ns = self.min_latency_ns.load(Ordering::Relaxed);
        let drops = self.queue_drops.load(Ordering::Relaxed);

        log::info!(
            "MatchingStats: orders={}, trades={}, latency(avg/min/max)={:.0}/{}/{} ns, drops={}",
            orders,
            trades,
            avg_ns,
            min_ns,
            max_ns,
            drops
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 高性能撮合引擎
// ═══════════════════════════════════════════════════════════════════════════

/// 高性能撮合引擎
///
/// 特点：
/// - CPU 亲和性绑定
/// - 预分配内存池
/// - SPSC 无锁队列
/// - 批量处理优化
pub struct HighPerfMatchingEngine {
    /// 配置
    config: HighPerfMatchingConfig,

    /// 订单簿池
    orderbooks: Arc<DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>>,

    /// 订单池
    order_pool: Arc<OrderPool>,

    /// 成交池
    trade_pool: Arc<TradeReportPool>,

    /// 订单输入队列发送端
    order_sender: SpscSender<PooledOrder>,

    /// 订单输入队列接收端（移交给撮合线程）
    order_receiver: Option<SpscReceiver<PooledOrder>>,

    /// 成交输出队列发送端（移交给撮合线程）
    trade_sender: Option<SpscSender<PooledTradeReport>>,

    /// 成交输出队列接收端
    trade_receiver: SpscReceiver<PooledTradeReport>,

    /// 统计信息
    stats: Arc<MatchingStats>,

    /// 运行标志
    running: Arc<AtomicBool>,

    /// 撮合线程句柄
    worker_handle: Option<JoinHandle<()>>,
}

impl HighPerfMatchingEngine {
    /// 创建高性能撮合引擎
    pub fn new(config: HighPerfMatchingConfig) -> Self {
        // 创建订单池
        let order_pool = Arc::new(OrderPool::new(config.order_pool_config.clone()));

        // 创建成交池
        let trade_pool = Arc::new(TradeReportPool::new(config.trade_pool_config.clone()));

        // 创建订单队列
        let (order_sender, order_receiver) = spsc_channel(config.order_queue_capacity);

        // 创建成交队列
        let (trade_sender, trade_receiver) = spsc_channel(config.trade_queue_capacity);

        Self {
            config,
            orderbooks: Arc::new(DashMap::new()),
            order_pool,
            trade_pool,
            order_sender,
            order_receiver: Some(order_receiver),
            trade_sender: Some(trade_sender),
            trade_receiver,
            stats: Arc::new(MatchingStats::default()),
            running: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }

    /// 注册品种
    pub fn register_instrument(&self, instrument_id: &str, init_price: f64) {
        let orderbook = Orderbook::new(InstrumentAsset::from_code(instrument_id), init_price);
        self.orderbooks
            .insert(instrument_id.to_string(), Arc::new(RwLock::new(orderbook)));
        log::info!("HighPerfMatchingEngine: registered {}", instrument_id);
    }

    /// 启动撮合引擎
    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Engine already running".to_string());
        }

        // 取出接收端（只能启动一次）
        let order_receiver = self
            .order_receiver
            .take()
            .ok_or("Order receiver already taken")?;
        let trade_sender = self
            .trade_sender
            .take()
            .ok_or("Trade sender already taken")?;

        let orderbooks = Arc::clone(&self.orderbooks);
        let trade_pool = Arc::clone(&self.trade_pool);
        let stats = Arc::clone(&self.stats);
        let running = Arc::clone(&self.running);
        let config = self.config.clone();

        running.store(true, Ordering::SeqCst);

        // 启动撮合线程（绑定 CPU 核心）
        let handle = if config.enable_cpu_affinity {
            spawn_on_core(config.matching_core, "matching-engine", move || {
                run_matching_loop(
                    orderbooks,
                    order_receiver,
                    trade_sender,
                    trade_pool,
                    stats,
                    running,
                    config,
                )
            })
            .map_err(|e| format!("Failed to spawn thread: {}", e))?
        } else {
            thread::Builder::new()
                .name("matching-engine".to_string())
                .spawn(move || {
                    run_matching_loop(
                        orderbooks,
                        order_receiver,
                        trade_sender,
                        trade_pool,
                        stats,
                        running,
                        config,
                    )
                })
                .map_err(|e| format!("Failed to spawn thread: {}", e))?
        };

        self.worker_handle = Some(handle);
        log::info!("HighPerfMatchingEngine started on core {}", self.config.matching_core);

        Ok(())
    }

    /// 停止撮合引擎
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.worker_handle.take() {
            handle.join().ok();
        }

        log::info!("HighPerfMatchingEngine stopped");
    }

    /// 提交订单（非阻塞）
    pub fn submit_order(&self, order: PooledOrder) -> Result<(), PooledOrder> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(order);
        }

        self.order_sender.try_send(order)
    }

    /// 从池中获取订单对象
    pub fn acquire_order(&self) -> crate::perf::PooledObject<'_, PooledOrder> {
        self.order_pool.acquire()
    }

    /// 接收成交回报（非阻塞）
    pub fn try_recv_trade(&self) -> Option<PooledTradeReport> {
        self.trade_receiver.try_recv()
    }

    /// 批量接收成交回报
    pub fn recv_trades(&self, batch: &mut Vec<PooledTradeReport>, max_count: usize) -> usize {
        self.trade_receiver.recv_batch(batch, max_count)
    }

    /// 获取统计信息
    pub fn stats(&self) -> &MatchingStats {
        &self.stats
    }

    /// 获取队列状态
    pub fn queue_status(&self) -> (usize, usize) {
        (self.order_sender.len(), self.trade_receiver.len())
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for HighPerfMatchingEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 撮合主循环
// ═══════════════════════════════════════════════════════════════════════════

/// 撮合主循环（运行在独立线程中）
fn run_matching_loop(
    orderbooks: Arc<DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>>,
    order_receiver: SpscReceiver<PooledOrder>,
    trade_sender: SpscSender<PooledTradeReport>,
    trade_pool: Arc<TradeReportPool>,
    stats: Arc<MatchingStats>,
    running: Arc<AtomicBool>,
    config: HighPerfMatchingConfig,
) {
    log::info!("Matching loop started");

    let mut batch = Vec::with_capacity(config.batch_size);

    while running.load(Ordering::Relaxed) {
        // 批量接收订单
        batch.clear();
        let count = order_receiver.recv_batch(&mut batch, config.batch_size);

        if count == 0 {
            // 无订单，短暂休眠
            std::hint::spin_loop();
            continue;
        }

        // 处理批次中的订单
        for order in batch.drain(..) {
            let start = Instant::now();

            // 执行撮合
            process_single_order(&order, &orderbooks, &trade_sender, &trade_pool, &stats);

            // 记录延迟
            let latency_ns = start.elapsed().as_nanos() as u64;
            stats.record_latency(latency_ns);
            stats.orders_processed.fetch_add(1, Ordering::Relaxed);
        }
    }

    log::info!("Matching loop stopped");
}

/// 处理单个订单
fn process_single_order(
    order: &PooledOrder,
    orderbooks: &DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>,
    trade_sender: &SpscSender<PooledTradeReport>,
    _trade_pool: &TradeReportPool,
    stats: &MatchingStats,
) {
    // 获取订单簿
    let orderbook = match orderbooks.get(&order.instrument_id) {
        Some(ob) => ob,
        None => {
            log::warn!("Instrument not found: {}", order.instrument_id);
            return;
        }
    };

    // 转换方向
    use crate::matching::{orders, OrderDirection, Success};

    let direction = if order.direction == 0 {
        OrderDirection::BUY
    } else {
        OrderDirection::SELL
    };

    // 创建订单请求
    let asset = InstrumentAsset::from_code(&order.instrument_id);
    let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let order_request = orders::new_limit_order_request(
        asset,
        direction,
        order.price,
        order.volume,
        timestamp,
    );

    // 执行撮合
    let mut ob = orderbook.write();
    let results = ob.process_order(order_request);

    // 处理撮合结果
    for result in results {
        match result {
            Ok(Success::Filled { price, volume, ts, .. }) |
            Ok(Success::PartiallyFilled { price, volume, ts, .. }) => {
                // 生成成交回报
                let trade_report = PooledTradeReport {
                    trade_id: format!("T{}", ts),
                    order_id: order.order_id.clone(),
                    account_id: order.account_id.clone(),
                    instrument_id: order.instrument_id.clone(),
                    direction: order.direction,
                    offset: order.offset,
                    price,
                    volume,
                    commission: 0.0,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };

                if trade_sender.try_send(trade_report).is_err() {
                    stats.queue_drops.fetch_add(1, Ordering::Relaxed);
                } else {
                    stats.trades_generated.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(Success::Accepted { .. }) => {
                // 订单被接受，等待撮合
            }
            Ok(Success::Cancelled { .. }) => {
                // 订单被取消
            }
            Ok(Success::Amended { .. }) => {
                // 订单被修改
            }
            Err(failed) => {
                log::debug!("Order failed: {:?}", failed);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_high_perf_engine_basic() {
        let config = HighPerfMatchingConfig {
            enable_cpu_affinity: false, // 测试时禁用
            ..Default::default()
        };

        let mut engine = HighPerfMatchingEngine::new(config);

        // 注册品种
        engine.register_instrument("TEST001", 100.0);

        // 启动引擎
        engine.start().unwrap();

        // 提交买单
        let mut buy_order = engine.acquire_order();
        buy_order.order_id = "BUY001".to_string();
        buy_order.account_id = "ACC001".to_string();
        buy_order.instrument_id = "TEST001".to_string();
        buy_order.direction = 0; // BUY
        buy_order.offset = 0;    // OPEN
        buy_order.price = 100.0;
        buy_order.volume = 10.0;

        // 需要复制一份，因为 acquire 返回的是 PooledObject
        let buy = PooledOrder {
            order_id: buy_order.order_id.clone(),
            account_id: buy_order.account_id.clone(),
            instrument_id: buy_order.instrument_id.clone(),
            direction: buy_order.direction,
            offset: buy_order.offset,
            price: buy_order.price,
            volume: buy_order.volume,
            filled_volume: 0.0,
            status: 0,
            timestamp: 0,
        };
        drop(buy_order);

        engine.submit_order(buy).unwrap();

        // 提交卖单（应该成交）
        let sell = PooledOrder {
            order_id: "SELL001".to_string(),
            account_id: "ACC002".to_string(),
            instrument_id: "TEST001".to_string(),
            direction: 1, // SELL
            offset: 0,
            price: 100.0,
            volume: 10.0,
            filled_volume: 0.0,
            status: 0,
            timestamp: 0,
        };

        engine.submit_order(sell).unwrap();

        // 等待处理
        thread::sleep(Duration::from_millis(100));

        // 接收成交
        let mut trades = Vec::new();
        engine.recv_trades(&mut trades, 10);

        println!("Received {} trades", trades.len());
        engine.stats().print_stats();

        // 停止引擎
        engine.stop();

        // 应该有成交
        assert!(trades.len() > 0 || engine.stats().trades_generated.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn test_high_perf_engine_throughput() {
        let config = HighPerfMatchingConfig {
            enable_cpu_affinity: false,
            order_queue_capacity: 100_000,
            trade_queue_capacity: 100_000,
            batch_size: 100,
            ..Default::default()
        };

        let mut engine = HighPerfMatchingEngine::new(config);
        engine.register_instrument("PERF001", 100.0);
        engine.start().unwrap();

        const NUM_ORDERS: usize = 10_000;

        let start = Instant::now();

        // 提交大量订单
        for i in 0..NUM_ORDERS {
            let order = PooledOrder {
                order_id: format!("ORD{:06}", i),
                account_id: format!("ACC{:03}", i % 100),
                instrument_id: "PERF001".to_string(),
                direction: (i % 2) as u8,
                offset: 0,
                price: 100.0 + (i % 10) as f64 * 0.1,
                volume: 10.0,
                filled_volume: 0.0,
                status: 0,
                timestamp: 0,
            };

            // 忽略队列满的情况
            engine.submit_order(order).ok();
        }

        // 等待处理完成
        thread::sleep(Duration::from_millis(500));

        let elapsed = start.elapsed();
        let orders_per_sec = NUM_ORDERS as f64 / elapsed.as_secs_f64();

        println!("Throughput: {:.0} orders/sec", orders_per_sec);
        engine.stats().print_stats();

        engine.stop();
    }
}
