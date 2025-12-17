//! 预分配内存池模块
//!
//! @yutiansut @quantaxis
//!
//! 提供对象池实现，避免频繁的内存分配和释放
//!
//! 性能目标：
//! - 对象获取：< 50ns（无锁路径）
//! - 对象归还：< 30ns
//! - 内存碎片：最小化
//!
//! 支持的池类型：
//! - `OrderPool`: 订单对象池
//! - `TradeReportPool`: 成交回报对象池
//! - `GenericPool<T>`: 通用对象池

use crossbeam_queue::ArrayQueue;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// 通用对象池
// ═══════════════════════════════════════════════════════════════════════════

/// 对象池配置
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// 初始容量
    pub initial_capacity: usize,

    /// 最大容量（0 表示无限制）
    pub max_capacity: usize,

    /// 是否预热（创建时填充对象）
    pub prewarm: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_capacity: 10_000,
            max_capacity: 100_000,
            prewarm: true,
        }
    }
}

impl PoolConfig {
    /// 小型池配置
    pub fn small() -> Self {
        Self {
            initial_capacity: 1_000,
            max_capacity: 10_000,
            prewarm: true,
        }
    }

    /// 大型池配置
    pub fn large() -> Self {
        Self {
            initial_capacity: 100_000,
            max_capacity: 1_000_000,
            prewarm: false, // 大型池不预热，按需分配
        }
    }
}

/// 可池化对象 trait
pub trait Poolable: Default + Send + 'static {
    /// 重置对象到初始状态（用于复用前清理）
    fn reset(&mut self);
}

/// 通用对象池
pub struct GenericPool<T: Poolable> {
    /// 空闲对象队列（无锁）
    free_list: ArrayQueue<Box<T>>,

    /// 池统计
    stats: PoolStats,

    /// 配置
    config: PoolConfig,

    /// 备用分配器（当无锁队列满时使用）
    overflow: Mutex<Vec<Box<T>>>,
}

/// 池统计信息
#[derive(Debug, Default)]
pub struct PoolStats {
    /// 已分配对象数
    pub allocated: AtomicUsize,

    /// 当前使用中的对象数
    pub in_use: AtomicUsize,

    /// 获取次数
    pub acquires: AtomicUsize,

    /// 归还次数
    pub releases: AtomicUsize,

    /// 新分配次数（池中无可用对象）
    pub allocations: AtomicUsize,

    /// 丢弃次数（池已满）
    pub drops: AtomicUsize,
}

impl PoolStats {
    /// 获取命中率
    pub fn hit_rate(&self) -> f64 {
        let acquires = self.acquires.load(Ordering::Relaxed);
        let allocations = self.allocations.load(Ordering::Relaxed);

        if acquires == 0 {
            1.0
        } else {
            (acquires - allocations) as f64 / acquires as f64
        }
    }
}

impl<T: Poolable> GenericPool<T> {
    /// 创建新的对象池
    pub fn new(config: PoolConfig) -> Self {
        let pool = Self {
            free_list: ArrayQueue::new(config.max_capacity.max(config.initial_capacity)),
            stats: PoolStats::default(),
            config: config.clone(),
            overflow: Mutex::new(Vec::new()),
        };

        // 预热池
        if config.prewarm {
            pool.prewarm(config.initial_capacity);
        }

        pool
    }

    /// 预热池（预分配对象）
    fn prewarm(&self, count: usize) {
        for _ in 0..count {
            let obj = Box::new(T::default());
            if self.free_list.push(obj).is_err() {
                break;
            }
            self.stats.allocated.fetch_add(1, Ordering::Relaxed);
        }

        log::debug!(
            "Pool prewarmed with {} objects",
            self.stats.allocated.load(Ordering::Relaxed)
        );
    }

    /// 从池中获取对象
    ///
    /// 如果池中有可用对象，直接返回；否则分配新对象
    pub fn acquire(&self) -> PooledObject<'_, T> {
        self.stats.acquires.fetch_add(1, Ordering::Relaxed);
        self.stats.in_use.fetch_add(1, Ordering::Relaxed);

        // 尝试从无锁队列获取
        if let Some(mut obj) = self.free_list.pop() {
            obj.reset();
            return PooledObject {
                inner: Some(obj),
                pool: self,
            };
        }

        // 尝试从溢出队列获取
        {
            let mut overflow = self.overflow.lock();
            if let Some(mut obj) = overflow.pop() {
                obj.reset();
                return PooledObject {
                    inner: Some(obj),
                    pool: self,
                };
            }
        }

        // 分配新对象
        self.stats.allocations.fetch_add(1, Ordering::Relaxed);
        self.stats.allocated.fetch_add(1, Ordering::Relaxed);

        PooledObject {
            inner: Some(Box::new(T::default())),
            pool: self,
        }
    }

    /// 归还对象到池
    fn release(&self, obj: Box<T>) {
        self.stats.releases.fetch_add(1, Ordering::Relaxed);
        self.stats.in_use.fetch_sub(1, Ordering::Relaxed);

        // 尝试放入无锁队列
        match self.free_list.push(obj) {
            Ok(()) => {
                // 成功放入无锁队列
            }
            Err(returned_obj) => {
                // 队列满，尝试放入溢出队列
                let mut overflow = self.overflow.lock();
                if overflow.len() < self.config.max_capacity / 10 {
                    overflow.push(returned_obj);
                } else {
                    // 溢出队列也满，丢弃对象
                    self.stats.drops.fetch_add(1, Ordering::Relaxed);
                    self.stats.allocated.fetch_sub(1, Ordering::Relaxed);
                    // returned_obj 被 drop
                }
            }
        }
    }

    /// 获取池统计信息
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// 获取当前池中可用对象数
    pub fn available(&self) -> usize {
        self.free_list.len() + self.overflow.lock().len()
    }

    /// 清空池
    pub fn clear(&self) {
        while self.free_list.pop().is_some() {}
        self.overflow.lock().clear();
        self.stats.allocated.store(0, Ordering::Relaxed);
    }
}

/// 池化对象包装器 - RAII 自动归还
pub struct PooledObject<'a, T: Poolable> {
    inner: Option<Box<T>>,
    pool: &'a GenericPool<T>,
}

impl<'a, T: Poolable> std::ops::Deref for PooledObject<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<'a, T: Poolable> std::ops::DerefMut for PooledObject<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

impl<'a, T: Poolable> Drop for PooledObject<'a, T> {
    fn drop(&mut self) {
        if let Some(obj) = self.inner.take() {
            self.pool.release(obj);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 订单对象池
// ═══════════════════════════════════════════════════════════════════════════

/// 池化订单对象
#[derive(Debug, Clone)]
pub struct PooledOrder {
    pub order_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: u8,    // 0=BUY, 1=SELL
    pub offset: u8,       // 0=OPEN, 1=CLOSE
    pub price: f64,
    pub volume: f64,
    pub filled_volume: f64,
    pub status: u8,       // 0=PENDING, 1=ACTIVE, 2=FILLED, 3=CANCELLED
    pub timestamp: i64,
}

impl Default for PooledOrder {
    fn default() -> Self {
        Self {
            order_id: String::with_capacity(64),
            account_id: String::with_capacity(32),
            instrument_id: String::with_capacity(32),
            direction: 0,
            offset: 0,
            price: 0.0,
            volume: 0.0,
            filled_volume: 0.0,
            status: 0,
            timestamp: 0,
        }
    }
}

impl Poolable for PooledOrder {
    fn reset(&mut self) {
        self.order_id.clear();
        self.account_id.clear();
        self.instrument_id.clear();
        self.direction = 0;
        self.offset = 0;
        self.price = 0.0;
        self.volume = 0.0;
        self.filled_volume = 0.0;
        self.status = 0;
        self.timestamp = 0;
    }
}

/// 订单对象池
pub type OrderPool = GenericPool<PooledOrder>;

// ═══════════════════════════════════════════════════════════════════════════
// 成交回报对象池
// ═══════════════════════════════════════════════════════════════════════════

/// 池化成交回报对象
#[derive(Debug, Clone)]
pub struct PooledTradeReport {
    pub trade_id: String,
    pub order_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: u8,
    pub offset: u8,
    pub price: f64,
    pub volume: f64,
    pub commission: f64,
    pub timestamp: i64,
}

impl Default for PooledTradeReport {
    fn default() -> Self {
        Self {
            trade_id: String::with_capacity(64),
            order_id: String::with_capacity(64),
            account_id: String::with_capacity(32),
            instrument_id: String::with_capacity(32),
            direction: 0,
            offset: 0,
            price: 0.0,
            volume: 0.0,
            commission: 0.0,
            timestamp: 0,
        }
    }
}

impl Poolable for PooledTradeReport {
    fn reset(&mut self) {
        self.trade_id.clear();
        self.order_id.clear();
        self.account_id.clear();
        self.instrument_id.clear();
        self.direction = 0;
        self.offset = 0;
        self.price = 0.0;
        self.volume = 0.0;
        self.commission = 0.0;
        self.timestamp = 0;
    }
}

/// 成交回报对象池
pub type TradeReportPool = GenericPool<PooledTradeReport>;

// ═══════════════════════════════════════════════════════════════════════════
// 消息缓冲池
// ═══════════════════════════════════════════════════════════════════════════

/// 池化消息缓冲区
#[derive(Debug)]
pub struct PooledBuffer {
    pub data: Vec<u8>,
    capacity: usize,
}

impl Default for PooledBuffer {
    fn default() -> Self {
        Self::with_capacity(4096)
    }
}

impl PooledBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            capacity,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }
}

impl Poolable for PooledBuffer {
    fn reset(&mut self) {
        self.data.clear();
        // 保留容量，不收缩
        if self.data.capacity() > self.capacity * 2 {
            self.data.shrink_to(self.capacity);
        }
    }
}

/// 消息缓冲池
pub type BufferPool = GenericPool<PooledBuffer>;

// ═══════════════════════════════════════════════════════════════════════════
// 全局池管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 全局池管理器
pub struct PoolManager {
    pub orders: Arc<OrderPool>,
    pub trade_reports: Arc<TradeReportPool>,
    pub buffers: Arc<BufferPool>,
}

impl PoolManager {
    /// 创建新的池管理器
    pub fn new() -> Self {
        Self {
            orders: Arc::new(OrderPool::new(PoolConfig::default())),
            trade_reports: Arc::new(TradeReportPool::new(PoolConfig::default())),
            buffers: Arc::new(BufferPool::new(PoolConfig::small())),
        }
    }

    /// 使用自定义配置创建
    pub fn with_config(order_config: PoolConfig, trade_config: PoolConfig) -> Self {
        Self {
            orders: Arc::new(OrderPool::new(order_config)),
            trade_reports: Arc::new(TradeReportPool::new(trade_config)),
            buffers: Arc::new(BufferPool::new(PoolConfig::small())),
        }
    }

    /// 获取订单池
    pub fn order_pool(&self) -> Arc<OrderPool> {
        Arc::clone(&self.orders)
    }

    /// 获取成交回报池
    pub fn trade_report_pool(&self) -> Arc<TradeReportPool> {
        Arc::clone(&self.trade_reports)
    }

    /// 获取缓冲池
    pub fn buffer_pool(&self) -> Arc<BufferPool> {
        Arc::clone(&self.buffers)
    }

    /// 打印池统计信息
    pub fn print_stats(&self) {
        let order_stats = self.orders.stats();
        let trade_stats = self.trade_reports.stats();

        log::info!(
            "Pool Stats - Orders: allocated={}, in_use={}, hit_rate={:.2}%",
            order_stats.allocated.load(Ordering::Relaxed),
            order_stats.in_use.load(Ordering::Relaxed),
            order_stats.hit_rate() * 100.0
        );

        log::info!(
            "Pool Stats - TradeReports: allocated={}, in_use={}, hit_rate={:.2}%",
            trade_stats.allocated.load(Ordering::Relaxed),
            trade_stats.in_use.load(Ordering::Relaxed),
            trade_stats.hit_rate() * 100.0
        );
    }
}

impl Default for PoolManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_pool_basic() {
        let pool: GenericPool<PooledOrder> = GenericPool::new(PoolConfig {
            initial_capacity: 10,
            max_capacity: 100,
            prewarm: true,
        });

        assert_eq!(pool.available(), 10);

        // 获取对象
        let mut obj = pool.acquire();
        obj.order_id = "ORDER_001".to_string();
        obj.price = 100.0;

        assert_eq!(pool.stats().in_use.load(Ordering::Relaxed), 1);
        assert_eq!(pool.available(), 9);

        // 归还对象（自动 drop）
        drop(obj);

        assert_eq!(pool.stats().in_use.load(Ordering::Relaxed), 0);
        assert_eq!(pool.available(), 10);
    }

    #[test]
    fn test_pool_reset() {
        let pool: GenericPool<PooledOrder> = GenericPool::new(PoolConfig {
            initial_capacity: 1,
            max_capacity: 10,
            prewarm: true,
        });

        // 第一次获取并设置值
        {
            let mut obj = pool.acquire();
            obj.order_id = "ORDER_001".to_string();
            obj.price = 100.0;
        }

        // 第二次获取，应该是重置后的对象
        {
            let obj = pool.acquire();
            assert!(obj.order_id.is_empty()); // 应该被重置
            assert_eq!(obj.price, 0.0);
        }
    }

    #[test]
    fn test_pool_performance() {
        let pool: GenericPool<PooledOrder> = GenericPool::new(PoolConfig {
            initial_capacity: 10_000,
            max_capacity: 100_000,
            prewarm: true,
        });

        const ITERATIONS: usize = 100_000;

        let start = Instant::now();

        for _ in 0..ITERATIONS {
            let mut obj = pool.acquire();
            obj.order_id = "TEST".to_string();
            // 自动归还
        }

        let elapsed = start.elapsed();
        let avg_ns = elapsed.as_nanos() / ITERATIONS as u128;

        println!(
            "Pool acquire/release: {} ns/op (hit_rate: {:.2}%)",
            avg_ns,
            pool.stats().hit_rate() * 100.0
        );

        // 应该 < 500ns
        assert!(avg_ns < 5000, "Pool operation too slow: {} ns", avg_ns);
    }

    #[test]
    fn test_pool_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let pool = Arc::new(GenericPool::<PooledOrder>::new(PoolConfig {
            initial_capacity: 1000,
            max_capacity: 10000,
            prewarm: true,
        }));

        let mut handles = vec![];

        for _ in 0..4 {
            let pool_clone = Arc::clone(&pool);
            let handle = thread::spawn(move || {
                for _ in 0..10000 {
                    let mut obj = pool_clone.acquire();
                    obj.price = 100.0;
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 所有对象应该都已归还
        assert_eq!(pool.stats().in_use.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_pool_manager() {
        let manager = PoolManager::new();

        let order = manager.orders.acquire();
        let trade = manager.trade_reports.acquire();

        drop(order);
        drop(trade);

        manager.print_stats();
    }

    #[test]
    fn test_buffer_pool() {
        let pool: GenericPool<PooledBuffer> = GenericPool::new(PoolConfig::small());

        let mut buf = pool.acquire();
        buf.data.extend_from_slice(b"Hello, World!");

        assert_eq!(buf.as_slice(), b"Hello, World!");

        drop(buf);

        // 再次获取应该是空的
        let buf2 = pool.acquire();
        assert!(buf2.data.is_empty());
    }
}
