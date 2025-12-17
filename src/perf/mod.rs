//! 性能优化模块
//!
//! @yutiansut @quantaxis
//!
//! Phase 5.2 性能优化实现：
//! - CPU 亲和性绑定
//! - 预分配内存池
//! - SPSC 无锁队列
//!
//! 性能目标：
//! - 订单处理延迟：P99 < 100μs
//! - 撮合延迟：P99 < 50μs
//! - 内存分配开销：最小化

pub mod cpu_affinity;
pub mod memory_pool;
pub mod spsc;

pub use cpu_affinity::{
    bind_to_core, get_available_cores, get_core_count, spawn_on_core, AffinityError,
    AffinityGuard, CpuAffinityConfig,
};

pub use memory_pool::{
    BufferPool, GenericPool, OrderPool, PoolConfig, PoolManager, PoolStats, Poolable,
    PooledBuffer, PooledObject, PooledOrder, PooledTradeReport, TradeReportPool,
};

pub use spsc::{spsc_channel, QueueStats, SpscQueue, SpscReceiver, SpscSender};

// ═══════════════════════════════════════════════════════════════════════════
// 性能配置
// ═══════════════════════════════════════════════════════════════════════════

/// 系统性能配置
#[derive(Debug, Clone)]
pub struct PerfConfig {
    /// CPU 亲和性配置
    pub cpu_affinity: CpuAffinityConfig,

    /// 订单池配置
    pub order_pool: PoolConfig,

    /// 成交回报池配置
    pub trade_report_pool: PoolConfig,

    /// 订单队列容量
    pub order_queue_capacity: usize,

    /// 成交回报队列容量
    pub trade_queue_capacity: usize,

    /// 是否启用性能优化
    pub enabled: bool,
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            cpu_affinity: CpuAffinityConfig::auto_detect(),
            order_pool: PoolConfig::default(),
            trade_report_pool: PoolConfig::default(),
            order_queue_capacity: 100_000,
            trade_queue_capacity: 100_000,
            enabled: true,
        }
    }
}

impl PerfConfig {
    /// 创建高性能配置
    pub fn high_performance() -> Self {
        Self {
            cpu_affinity: CpuAffinityConfig::auto_detect(),
            order_pool: PoolConfig {
                initial_capacity: 100_000,
                max_capacity: 1_000_000,
                prewarm: true,
            },
            trade_report_pool: PoolConfig {
                initial_capacity: 100_000,
                max_capacity: 1_000_000,
                prewarm: true,
            },
            order_queue_capacity: 500_000,
            trade_queue_capacity: 500_000,
            enabled: true,
        }
    }

    /// 创建低内存配置
    pub fn low_memory() -> Self {
        Self {
            cpu_affinity: CpuAffinityConfig::disabled(),
            order_pool: PoolConfig::small(),
            trade_report_pool: PoolConfig::small(),
            order_queue_capacity: 10_000,
            trade_queue_capacity: 10_000,
            enabled: false,
        }
    }

    /// 禁用性能优化（用于测试）
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            cpu_affinity: CpuAffinityConfig::disabled(),
            ..Self::default()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 性能上下文
// ═══════════════════════════════════════════════════════════════════════════

use std::sync::Arc;

/// 性能上下文 - 集中管理所有性能相关资源
pub struct PerfContext {
    /// 配置
    pub config: PerfConfig,

    /// 池管理器
    pub pools: PoolManager,

    /// 订单队列（撮合引擎输入）
    order_queue: Option<(SpscSender<PooledOrder>, SpscReceiver<PooledOrder>)>,

    /// 成交队列（撮合引擎输出）
    trade_queue: Option<(SpscSender<PooledTradeReport>, SpscReceiver<PooledTradeReport>)>,
}

impl PerfContext {
    /// 创建新的性能上下文
    pub fn new(config: PerfConfig) -> Self {
        let pools = PoolManager::with_config(
            config.order_pool.clone(),
            config.trade_report_pool.clone(),
        );

        let order_queue = if config.enabled {
            Some(spsc_channel(config.order_queue_capacity))
        } else {
            None
        };

        let trade_queue = if config.enabled {
            Some(spsc_channel(config.trade_queue_capacity))
        } else {
            None
        };

        Self {
            config,
            pools,
            order_queue,
            trade_queue,
        }
    }

    /// 使用默认配置创建
    pub fn default_context() -> Self {
        Self::new(PerfConfig::default())
    }

    /// 获取订单发送端
    pub fn order_sender(&self) -> Option<SpscSender<PooledOrder>> {
        self.order_queue.as_ref().map(|(tx, _)| tx.clone())
    }

    /// 获取订单接收端（注意：只能有一个消费者）
    pub fn take_order_receiver(&mut self) -> Option<SpscReceiver<PooledOrder>> {
        self.order_queue.take().map(|(_, rx)| rx)
    }

    /// 获取成交发送端
    pub fn trade_sender(&self) -> Option<SpscSender<PooledTradeReport>> {
        self.trade_queue.as_ref().map(|(tx, _)| tx.clone())
    }

    /// 获取成交接收端（注意：只能有一个消费者）
    pub fn take_trade_receiver(&mut self) -> Option<SpscReceiver<PooledTradeReport>> {
        self.trade_queue.take().map(|(_, rx)| rx)
    }

    /// 获取池管理器
    pub fn pool_manager(&self) -> &PoolManager {
        &self.pools
    }

    /// 绑定撮合引擎线程
    pub fn bind_matching_engine(&self) -> Result<(), AffinityError> {
        self.config.cpu_affinity.bind_matching_engine_thread()
    }

    /// 绑定行情处理线程
    pub fn bind_market_data(&self) -> Result<(), AffinityError> {
        self.config.cpu_affinity.bind_market_data_thread()
    }

    /// 打印统计信息
    pub fn print_stats(&self) {
        self.pools.print_stats();

        if let Some((tx, _)) = &self.order_queue {
            log::info!("Order Queue: len={}", tx.len());
        }

        if let Some((tx, _)) = &self.trade_queue {
            log::info!("Trade Queue: len={}", tx.len());
        }
    }
}

impl Default for PerfContext {
    fn default() -> Self {
        Self::default_context()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 全局性能上下文（可选）
// ═══════════════════════════════════════════════════════════════════════════

use once_cell::sync::Lazy;
use parking_lot::RwLock;

/// 全局性能上下文
static GLOBAL_PERF_CONTEXT: Lazy<RwLock<Option<Arc<PerfContext>>>> =
    Lazy::new(|| RwLock::new(None));

/// 初始化全局性能上下文
pub fn init_global_perf_context(config: PerfConfig) {
    let mut ctx = GLOBAL_PERF_CONTEXT.write();
    *ctx = Some(Arc::new(PerfContext::new(config)));
    log::info!("Global performance context initialized");
}

/// 获取全局性能上下文
pub fn get_global_perf_context() -> Option<Arc<PerfContext>> {
    GLOBAL_PERF_CONTEXT.read().clone()
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_config_default() {
        let config = PerfConfig::default();
        assert!(config.enabled || !config.cpu_affinity.enabled);
    }

    #[test]
    fn test_perf_config_high_performance() {
        let config = PerfConfig::high_performance();
        assert!(config.enabled);
        assert_eq!(config.order_pool.initial_capacity, 100_000);
    }

    #[test]
    fn test_perf_context() {
        let ctx = PerfContext::default_context();

        // 获取池
        let pools = ctx.pool_manager();

        // 从池中获取订单
        let mut order = pools.orders.acquire();
        order.order_id = "TEST_001".to_string();
        order.price = 100.0;

        // 自动归还
        drop(order);

        ctx.print_stats();
    }

    #[test]
    fn test_perf_context_queues() {
        let mut ctx = PerfContext::new(PerfConfig::default());

        if let Some(tx) = ctx.order_sender() {
            let mut order = PooledOrder::default();
            order.order_id = "TEST".to_string();
            tx.try_send(order).unwrap();
        }

        if let Some(rx) = ctx.take_order_receiver() {
            let order = rx.try_recv().unwrap();
            assert_eq!(order.order_id, "TEST");
        }
    }

    #[test]
    fn test_global_context() {
        init_global_perf_context(PerfConfig::disabled());

        let ctx = get_global_perf_context();
        assert!(ctx.is_some());
    }
}
