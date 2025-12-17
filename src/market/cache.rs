//! 行情数据缓存
//!
//! 三级缓存架构:
//! - L1: 内存缓存 (DashMap) - < 10μs
//! - L2: MemTable (SkipMap) - < 50μs
//! - L3: SSTable (mmap) - < 200μs

use super::{OrderBookSnapshot, TickData};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// L1 行情缓存 (热数据)
pub struct MarketDataCache {
    /// Tick 缓存 (instrument_id -> CachedTick)
    tick_cache: Arc<DashMap<String, CachedTick>>,

    /// 订单簿缓存 (instrument_id -> CachedOrderBook)
    orderbook_cache: Arc<DashMap<String, CachedOrderBook>>,

    /// 缓存 TTL (生存时间)
    ttl: Duration,

    /// 缓存统计
    stats: Arc<CacheStats>,
}

/// 缓存的 Tick 数据
#[derive(Clone)]
struct CachedTick {
    data: TickData,
    cached_at: Instant,
}

/// 缓存的订单簿数据
#[derive(Clone)]
struct CachedOrderBook {
    data: OrderBookSnapshot,
    cached_at: Instant,
}

/// 缓存统计
#[derive(Default)]
pub struct CacheStats {
    /// Tick 命中次数
    pub tick_hits: std::sync::atomic::AtomicU64,
    /// Tick 未命中次数
    pub tick_misses: std::sync::atomic::AtomicU64,
    /// 订单簿命中次数
    pub orderbook_hits: std::sync::atomic::AtomicU64,
    /// 订单簿未命中次数
    pub orderbook_misses: std::sync::atomic::AtomicU64,
}

impl MarketDataCache {
    /// 创建新的缓存
    ///
    /// # 参数
    /// - `ttl_ms`: 缓存生存时间 (毫秒)
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            tick_cache: Arc::new(DashMap::new()),
            orderbook_cache: Arc::new(DashMap::new()),
            ttl: Duration::from_millis(ttl_ms),
            stats: Arc::new(CacheStats::default()),
        }
    }

    /// 获取 Tick (带缓存)
    pub fn get_tick(&self, instrument_id: &str) -> Option<TickData> {
        if let Some(cached) = self.tick_cache.get(instrument_id) {
            if cached.cached_at.elapsed() < self.ttl {
                // 缓存命中
                self.stats
                    .tick_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(cached.data.clone());
            }
            // 缓存过期，删除
            drop(cached);
            self.tick_cache.remove(instrument_id);
        }

        // 缓存未命中
        self.stats
            .tick_misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 更新 Tick 缓存 (在成交时调用)
    pub fn update_tick(&self, instrument_id: String, tick: TickData) {
        self.tick_cache.insert(
            instrument_id,
            CachedTick {
                data: tick,
                cached_at: Instant::now(),
            },
        );
    }

    /// 获取订单簿 (带缓存)
    pub fn get_orderbook(&self, instrument_id: &str) -> Option<OrderBookSnapshot> {
        if let Some(cached) = self.orderbook_cache.get(instrument_id) {
            if cached.cached_at.elapsed() < self.ttl {
                // 缓存命中
                self.stats
                    .orderbook_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(cached.data.clone());
            }
            // 缓存过期，删除
            drop(cached);
            self.orderbook_cache.remove(instrument_id);
        }

        // 缓存未命中
        self.stats
            .orderbook_misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 更新订单簿缓存 (在快照广播时调用)
    pub fn update_orderbook(&self, instrument_id: String, orderbook: OrderBookSnapshot) {
        self.orderbook_cache.insert(
            instrument_id,
            CachedOrderBook {
                data: orderbook,
                cached_at: Instant::now(),
            },
        );
    }

    /// 使缓存失效
    pub fn invalidate_tick(&self, instrument_id: &str) {
        self.tick_cache.remove(instrument_id);
    }

    /// 使订单簿缓存失效
    pub fn invalidate_orderbook(&self, instrument_id: &str) {
        self.orderbook_cache.remove(instrument_id);
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        self.tick_cache.clear();
        self.orderbook_cache.clear();
    }

    /// 获取缓存统计信息
    pub fn get_stats(&self) -> CacheStatsSnapshot {
        CacheStatsSnapshot {
            tick_hits: self
                .stats
                .tick_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            tick_misses: self
                .stats
                .tick_misses
                .load(std::sync::atomic::Ordering::Relaxed),
            orderbook_hits: self
                .stats
                .orderbook_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            orderbook_misses: self
                .stats
                .orderbook_misses
                .load(std::sync::atomic::Ordering::Relaxed),
            tick_cache_size: self.tick_cache.len(),
            orderbook_cache_size: self.orderbook_cache.len(),
        }
    }

    /// 计算缓存命中率
    pub fn hit_rate(&self) -> f64 {
        let total_hits = self
            .stats
            .tick_hits
            .load(std::sync::atomic::Ordering::Relaxed)
            + self
                .stats
                .orderbook_hits
                .load(std::sync::atomic::Ordering::Relaxed);
        let total_misses = self
            .stats
            .tick_misses
            .load(std::sync::atomic::Ordering::Relaxed)
            + self
                .stats
                .orderbook_misses
                .load(std::sync::atomic::Ordering::Relaxed);

        if total_hits + total_misses == 0 {
            return 0.0;
        }

        total_hits as f64 / (total_hits + total_misses) as f64
    }
}

/// 缓存统计快照
#[derive(Debug, Clone)]
pub struct CacheStatsSnapshot {
    pub tick_hits: u64,
    pub tick_misses: u64,
    pub orderbook_hits: u64,
    pub orderbook_misses: u64,
    pub tick_cache_size: usize,
    pub orderbook_cache_size: usize,
}

impl CacheStatsSnapshot {
    /// 计算 Tick 命中率
    pub fn tick_hit_rate(&self) -> f64 {
        if self.tick_hits + self.tick_misses == 0 {
            return 0.0;
        }
        self.tick_hits as f64 / (self.tick_hits + self.tick_misses) as f64
    }

    /// 计算订单簿命中率
    pub fn orderbook_hit_rate(&self) -> f64 {
        if self.orderbook_hits + self.orderbook_misses == 0 {
            return 0.0;
        }
        self.orderbook_hits as f64 / (self.orderbook_hits + self.orderbook_misses) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ==================== 辅助函数 @yutiansut @quantaxis ====================

    fn create_test_tick(instrument_id: &str, price: f64, volume: i64) -> TickData {
        TickData {
            instrument_id: instrument_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            last_price: price,
            bid_price: Some(price - 0.5),
            ask_price: Some(price + 0.5),
            volume,
        }
    }

    fn create_test_orderbook(instrument_id: &str, last_price: f64) -> OrderBookSnapshot {
        use crate::market::PriceLevel;
        OrderBookSnapshot {
            instrument_id: instrument_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            bids: vec![
                PriceLevel { price: last_price - 0.5, volume: 100 },
                PriceLevel { price: last_price - 1.0, volume: 200 },
            ],
            asks: vec![
                PriceLevel { price: last_price + 0.5, volume: 150 },
                PriceLevel { price: last_price + 1.0, volume: 250 },
            ],
            last_price: Some(last_price),
        }
    }

    // ==================== MarketDataCache 创建测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_market_data_cache_new() {
        let cache = MarketDataCache::new(100);

        // 验证初始状态
        let stats = cache.get_stats();
        assert_eq!(stats.tick_hits, 0);
        assert_eq!(stats.tick_misses, 0);
        assert_eq!(stats.orderbook_hits, 0);
        assert_eq!(stats.orderbook_misses, 0);
        assert_eq!(stats.tick_cache_size, 0);
        assert_eq!(stats.orderbook_cache_size, 0);
    }

    #[test]
    fn test_market_data_cache_custom_ttl() {
        let cache = MarketDataCache::new(5000); // 5秒 TTL

        let tick = create_test_tick("IF2501", 4000.0, 10);
        cache.update_tick("IF2501".to_string(), tick);

        // 短时间内应该能命中
        std::thread::sleep(Duration::from_millis(100));
        assert!(cache.get_tick("IF2501").is_some());
    }

    // ==================== Tick 缓存测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_tick_cache() {
        let cache = MarketDataCache::new(100); // 100ms TTL

        let tick = TickData {
            instrument_id: "IF2501".to_string(),
            timestamp: Utc::now().timestamp_millis(),
            last_price: 4000.0,
            bid_price: Some(3999.5),
            ask_price: Some(4000.5),
            volume: 10,
        };

        // 缓存未命中
        assert!(cache.get_tick("IF2501").is_none());

        // 更新缓存
        cache.update_tick("IF2501".to_string(), tick.clone());

        // 缓存命中
        let cached = cache.get_tick("IF2501").unwrap();
        assert_eq!(cached.last_price, 4000.0);

        // 等待缓存过期
        std::thread::sleep(Duration::from_millis(150));

        // 缓存过期，未命中
        assert!(cache.get_tick("IF2501").is_none());
    }

    /// 测试 Tick 缓存更新覆盖
    #[test]
    fn test_tick_cache_update_overwrite() {
        let cache = MarketDataCache::new(1000);

        let tick1 = create_test_tick("IF2501", 4000.0, 10);
        cache.update_tick("IF2501".to_string(), tick1);

        let tick2 = create_test_tick("IF2501", 4100.0, 20);
        cache.update_tick("IF2501".to_string(), tick2);

        let cached = cache.get_tick("IF2501").unwrap();
        assert_eq!(cached.last_price, 4100.0);
        assert_eq!(cached.volume, 20);
    }

    /// 测试多合约 Tick 缓存
    #[test]
    fn test_tick_cache_multiple_instruments() {
        let cache = MarketDataCache::new(1000);

        cache.update_tick("IF2501".to_string(), create_test_tick("IF2501", 4000.0, 10));
        cache.update_tick("IC2501".to_string(), create_test_tick("IC2501", 7000.0, 5));
        cache.update_tick("IH2501".to_string(), create_test_tick("IH2501", 3000.0, 8));

        assert_eq!(cache.get_tick("IF2501").unwrap().last_price, 4000.0);
        assert_eq!(cache.get_tick("IC2501").unwrap().last_price, 7000.0);
        assert_eq!(cache.get_tick("IH2501").unwrap().last_price, 3000.0);

        let stats = cache.get_stats();
        assert_eq!(stats.tick_cache_size, 3);
    }

    /// 测试 Tick 缓存字段完整性
    #[test]
    fn test_tick_cache_fields() {
        let cache = MarketDataCache::new(1000);

        let tick = TickData {
            instrument_id: "IF2501".to_string(),
            timestamp: 1703000000000,
            last_price: 4123.45,
            bid_price: Some(4123.0),
            ask_price: Some(4124.0),
            volume: 999,
        };

        cache.update_tick("IF2501".to_string(), tick);
        let cached = cache.get_tick("IF2501").unwrap();

        assert_eq!(cached.instrument_id, "IF2501");
        assert_eq!(cached.timestamp, 1703000000000);
        assert_eq!(cached.last_price, 4123.45);
        assert_eq!(cached.bid_price, Some(4123.0));
        assert_eq!(cached.ask_price, Some(4124.0));
        assert_eq!(cached.volume, 999);
    }

    // ==================== OrderBook 缓存测试 @yutiansut @quantaxis ====================

    /// 测试订单簿缓存基本功能
    #[test]
    fn test_orderbook_cache_basic() {
        let cache = MarketDataCache::new(100); // 100ms TTL

        // 缓存未命中
        assert!(cache.get_orderbook("IF2501").is_none());

        // 更新缓存
        let orderbook = create_test_orderbook("IF2501", 4000.0);
        cache.update_orderbook("IF2501".to_string(), orderbook);

        // 缓存命中
        let cached = cache.get_orderbook("IF2501").unwrap();
        assert_eq!(cached.last_price, Some(4000.0));
        assert_eq!(cached.bids.len(), 2);
        assert_eq!(cached.asks.len(), 2);
    }

    /// 测试订单簿缓存过期
    #[test]
    fn test_orderbook_cache_expiry() {
        let cache = MarketDataCache::new(50); // 50ms TTL

        let orderbook = create_test_orderbook("IF2501", 4000.0);
        cache.update_orderbook("IF2501".to_string(), orderbook);

        // 缓存命中
        assert!(cache.get_orderbook("IF2501").is_some());

        // 等待过期
        std::thread::sleep(Duration::from_millis(100));

        // 缓存过期
        assert!(cache.get_orderbook("IF2501").is_none());
    }

    /// 测试订单簿缓存失效
    #[test]
    fn test_invalidate_orderbook() {
        let cache = MarketDataCache::new(1000);

        let orderbook = create_test_orderbook("IF2501", 4000.0);
        cache.update_orderbook("IF2501".to_string(), orderbook);
        assert!(cache.get_orderbook("IF2501").is_some());

        // 主动失效
        cache.invalidate_orderbook("IF2501");
        assert!(cache.get_orderbook("IF2501").is_none());
    }

    /// 测试多合约订单簿缓存
    #[test]
    fn test_orderbook_cache_multiple_instruments() {
        let cache = MarketDataCache::new(1000);

        cache.update_orderbook("IF2501".to_string(), create_test_orderbook("IF2501", 4000.0));
        cache.update_orderbook("IC2501".to_string(), create_test_orderbook("IC2501", 7000.0));

        assert_eq!(cache.get_orderbook("IF2501").unwrap().last_price, Some(4000.0));
        assert_eq!(cache.get_orderbook("IC2501").unwrap().last_price, Some(7000.0));

        let stats = cache.get_stats();
        assert_eq!(stats.orderbook_cache_size, 2);
    }

    // ==================== 缓存统计测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_cache_stats() {
        let cache = MarketDataCache::new(1000);

        let tick = TickData {
            instrument_id: "IF2501".to_string(),
            timestamp: Utc::now().timestamp_millis(),
            last_price: 4000.0,
            bid_price: Some(3999.5),
            ask_price: Some(4000.5),
            volume: 10,
        };

        // 未命中
        cache.get_tick("IF2501");

        // 更新缓存
        cache.update_tick("IF2501".to_string(), tick.clone());

        // 命中
        cache.get_tick("IF2501");
        cache.get_tick("IF2501");

        let stats = cache.get_stats();
        assert_eq!(stats.tick_hits, 2);
        assert_eq!(stats.tick_misses, 1);
        assert_eq!(stats.tick_hit_rate(), 2.0 / 3.0);
    }

    /// 测试订单簿缓存统计
    #[test]
    fn test_orderbook_cache_stats() {
        let cache = MarketDataCache::new(1000);

        // 未命中
        cache.get_orderbook("IF2501");
        cache.get_orderbook("IF2501");

        // 更新缓存
        let orderbook = create_test_orderbook("IF2501", 4000.0);
        cache.update_orderbook("IF2501".to_string(), orderbook);

        // 命中
        cache.get_orderbook("IF2501");

        let stats = cache.get_stats();
        assert_eq!(stats.orderbook_hits, 1);
        assert_eq!(stats.orderbook_misses, 2);
        assert_eq!(stats.orderbook_hit_rate(), 1.0 / 3.0);
    }

    /// 测试混合缓存统计
    #[test]
    fn test_mixed_cache_stats() {
        let cache = MarketDataCache::new(1000);

        // Tick: 1 miss, 2 hits
        cache.get_tick("IF2501");
        cache.update_tick("IF2501".to_string(), create_test_tick("IF2501", 4000.0, 10));
        cache.get_tick("IF2501");
        cache.get_tick("IF2501");

        // Orderbook: 1 miss, 1 hit
        cache.get_orderbook("IF2501");
        cache.update_orderbook("IF2501".to_string(), create_test_orderbook("IF2501", 4000.0));
        cache.get_orderbook("IF2501");

        let stats = cache.get_stats();
        assert_eq!(stats.tick_hits, 2);
        assert_eq!(stats.tick_misses, 1);
        assert_eq!(stats.orderbook_hits, 1);
        assert_eq!(stats.orderbook_misses, 1);
    }

    #[test]
    fn test_invalidate() {
        let cache = MarketDataCache::new(1000);

        let tick = TickData {
            instrument_id: "IF2501".to_string(),
            timestamp: Utc::now().timestamp_millis(),
            last_price: 4000.0,
            bid_price: Some(3999.5),
            ask_price: Some(4000.5),
            volume: 10,
        };

        cache.update_tick("IF2501".to_string(), tick.clone());
        assert!(cache.get_tick("IF2501").is_some());

        // 主动失效
        cache.invalidate_tick("IF2501");
        assert!(cache.get_tick("IF2501").is_none());
    }

    /// 测试失效不存在的合约
    #[test]
    fn test_invalidate_nonexistent() {
        let cache = MarketDataCache::new(1000);

        // 不应该报错
        cache.invalidate_tick("NONEXISTENT");
        cache.invalidate_orderbook("NONEXISTENT");
    }

    // ==================== 清空缓存测试 @yutiansut @quantaxis ====================

    /// 测试清空所有缓存
    #[test]
    fn test_clear_cache() {
        let cache = MarketDataCache::new(1000);

        // 填充缓存
        cache.update_tick("IF2501".to_string(), create_test_tick("IF2501", 4000.0, 10));
        cache.update_tick("IC2501".to_string(), create_test_tick("IC2501", 7000.0, 5));
        cache.update_orderbook("IF2501".to_string(), create_test_orderbook("IF2501", 4000.0));

        let stats_before = cache.get_stats();
        assert_eq!(stats_before.tick_cache_size, 2);
        assert_eq!(stats_before.orderbook_cache_size, 1);

        // 清空缓存
        cache.clear();

        let stats_after = cache.get_stats();
        assert_eq!(stats_after.tick_cache_size, 0);
        assert_eq!(stats_after.orderbook_cache_size, 0);

        // 缓存应为空
        assert!(cache.get_tick("IF2501").is_none());
        assert!(cache.get_orderbook("IF2501").is_none());
    }

    // ==================== 命中率测试 @yutiansut @quantaxis ====================

    /// 测试全局命中率
    #[test]
    fn test_hit_rate() {
        let cache = MarketDataCache::new(1000);

        // 初始为 0
        assert_eq!(cache.hit_rate(), 0.0);

        // 2 misses
        cache.get_tick("IF2501");
        cache.get_orderbook("IF2501");

        // 更新缓存
        cache.update_tick("IF2501".to_string(), create_test_tick("IF2501", 4000.0, 10));
        cache.update_orderbook("IF2501".to_string(), create_test_orderbook("IF2501", 4000.0));

        // 4 hits
        cache.get_tick("IF2501");
        cache.get_tick("IF2501");
        cache.get_orderbook("IF2501");
        cache.get_orderbook("IF2501");

        // 命中率: 4 / 6 = 0.666...
        let rate = cache.hit_rate();
        assert!((rate - 0.666666).abs() < 0.01);
    }

    /// 测试 100% 命中率
    #[test]
    fn test_hit_rate_100_percent() {
        let cache = MarketDataCache::new(1000);

        cache.update_tick("IF2501".to_string(), create_test_tick("IF2501", 4000.0, 10));

        // 只有命中
        cache.get_tick("IF2501");
        cache.get_tick("IF2501");
        cache.get_tick("IF2501");

        assert_eq!(cache.hit_rate(), 1.0);
    }

    /// 测试 0% 命中率
    #[test]
    fn test_hit_rate_0_percent() {
        let cache = MarketDataCache::new(1000);

        // 只有未命中
        cache.get_tick("IF2501");
        cache.get_tick("IC2501");
        cache.get_orderbook("IF2501");

        assert_eq!(cache.hit_rate(), 0.0);
    }

    // ==================== CacheStatsSnapshot 测试 @yutiansut @quantaxis ====================

    /// 测试 CacheStatsSnapshot 结构
    #[test]
    fn test_cache_stats_snapshot_structure() {
        let snapshot = CacheStatsSnapshot {
            tick_hits: 100,
            tick_misses: 50,
            orderbook_hits: 80,
            orderbook_misses: 20,
            tick_cache_size: 10,
            orderbook_cache_size: 5,
        };

        assert_eq!(snapshot.tick_hits, 100);
        assert_eq!(snapshot.tick_misses, 50);
        assert_eq!(snapshot.orderbook_hits, 80);
        assert_eq!(snapshot.orderbook_misses, 20);
        assert_eq!(snapshot.tick_cache_size, 10);
        assert_eq!(snapshot.orderbook_cache_size, 5);
    }

    /// 测试 tick_hit_rate 边界情况
    #[test]
    fn test_tick_hit_rate_edge_cases() {
        // 全为 0
        let snapshot_zero = CacheStatsSnapshot {
            tick_hits: 0,
            tick_misses: 0,
            orderbook_hits: 0,
            orderbook_misses: 0,
            tick_cache_size: 0,
            orderbook_cache_size: 0,
        };
        assert_eq!(snapshot_zero.tick_hit_rate(), 0.0);

        // 只有命中
        let snapshot_all_hits = CacheStatsSnapshot {
            tick_hits: 100,
            tick_misses: 0,
            orderbook_hits: 0,
            orderbook_misses: 0,
            tick_cache_size: 0,
            orderbook_cache_size: 0,
        };
        assert_eq!(snapshot_all_hits.tick_hit_rate(), 1.0);

        // 只有未命中
        let snapshot_all_misses = CacheStatsSnapshot {
            tick_hits: 0,
            tick_misses: 100,
            orderbook_hits: 0,
            orderbook_misses: 0,
            tick_cache_size: 0,
            orderbook_cache_size: 0,
        };
        assert_eq!(snapshot_all_misses.tick_hit_rate(), 0.0);
    }

    /// 测试 orderbook_hit_rate 边界情况
    #[test]
    fn test_orderbook_hit_rate_edge_cases() {
        // 全为 0
        let snapshot_zero = CacheStatsSnapshot {
            tick_hits: 0,
            tick_misses: 0,
            orderbook_hits: 0,
            orderbook_misses: 0,
            tick_cache_size: 0,
            orderbook_cache_size: 0,
        };
        assert_eq!(snapshot_zero.orderbook_hit_rate(), 0.0);

        // 只有命中
        let snapshot_all_hits = CacheStatsSnapshot {
            tick_hits: 0,
            tick_misses: 0,
            orderbook_hits: 100,
            orderbook_misses: 0,
            tick_cache_size: 0,
            orderbook_cache_size: 0,
        };
        assert_eq!(snapshot_all_hits.orderbook_hit_rate(), 1.0);
    }

    // ==================== CacheStats Default 测试 @yutiansut @quantaxis ====================

    /// 测试 CacheStats Default trait
    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();

        assert_eq!(stats.tick_hits.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(stats.tick_misses.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(stats.orderbook_hits.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(stats.orderbook_misses.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发读写 Tick 缓存
    #[test]
    fn test_concurrent_tick_cache() {
        use std::thread;

        let cache = Arc::new(MarketDataCache::new(1000));
        let mut handles = vec![];

        // 写线程
        for i in 0..5 {
            let cache_clone = cache.clone();
            handles.push(thread::spawn(move || {
                let tick = create_test_tick(&format!("INST{}", i), 1000.0 + i as f64, i as i64);
                cache_clone.update_tick(format!("INST{}", i), tick);
            }));
        }

        // 等待写入完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 读线程
        let mut read_handles = vec![];
        for i in 0..5 {
            let cache_clone = cache.clone();
            read_handles.push(thread::spawn(move || {
                let tick = cache_clone.get_tick(&format!("INST{}", i));
                assert!(tick.is_some());
            }));
        }

        for handle in read_handles {
            handle.join().unwrap();
        }

        let stats = cache.get_stats();
        assert_eq!(stats.tick_cache_size, 5);
    }

    /// 测试并发读写 Orderbook 缓存
    #[test]
    fn test_concurrent_orderbook_cache() {
        use std::thread;

        let cache = Arc::new(MarketDataCache::new(1000));
        let mut handles = vec![];

        // 写线程
        for i in 0..3 {
            let cache_clone = cache.clone();
            handles.push(thread::spawn(move || {
                let ob = create_test_orderbook(&format!("INST{}", i), 1000.0 + i as f64 * 100.0);
                cache_clone.update_orderbook(format!("INST{}", i), ob);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有写入
        for i in 0..3 {
            let ob = cache.get_orderbook(&format!("INST{}", i));
            assert!(ob.is_some());
        }

        let stats = cache.get_stats();
        assert_eq!(stats.orderbook_cache_size, 3);
    }

    /// 测试高并发混合操作
    #[test]
    fn test_high_concurrency_mixed_operations() {
        use std::thread;

        let cache = Arc::new(MarketDataCache::new(1000));
        let mut handles = vec![];

        // 混合读写线程
        for i in 0..20 {
            let cache_clone = cache.clone();
            handles.push(thread::spawn(move || {
                let inst_id = format!("INST{}", i % 5);
                if i % 2 == 0 {
                    // 写操作
                    cache_clone.update_tick(inst_id.clone(), create_test_tick(&inst_id, 100.0, 1));
                } else {
                    // 读操作
                    let _ = cache_clone.get_tick(&inst_id);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证缓存状态正常
        let stats = cache.get_stats();
        assert!(stats.tick_cache_size <= 5);
    }

    // ==================== 边界条件测试 @yutiansut @quantaxis ====================

    /// 测试零 TTL
    #[test]
    fn test_zero_ttl() {
        let cache = MarketDataCache::new(0);

        let tick = create_test_tick("IF2501", 4000.0, 10);
        cache.update_tick("IF2501".to_string(), tick);

        // TTL 为 0，应该立即过期
        assert!(cache.get_tick("IF2501").is_none());
    }

    /// 测试空合约 ID
    #[test]
    fn test_empty_instrument_id() {
        let cache = MarketDataCache::new(1000);

        let tick = create_test_tick("", 4000.0, 10);
        cache.update_tick("".to_string(), tick);

        let cached = cache.get_tick("");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().last_price, 4000.0);
    }

    /// 测试特殊字符合约 ID
    #[test]
    fn test_special_chars_instrument_id() {
        let cache = MarketDataCache::new(1000);

        let inst_id = "SHFE.cu2501";
        let tick = create_test_tick(inst_id, 85000.0, 100);
        cache.update_tick(inst_id.to_string(), tick);

        let cached = cache.get_tick(inst_id);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().last_price, 85000.0);
    }

    /// 测试大量合约缓存
    #[test]
    fn test_large_number_of_instruments() {
        let cache = MarketDataCache::new(1000);

        // 添加 100 个合约
        for i in 0..100 {
            let inst_id = format!("INST_{:04}", i);
            cache.update_tick(inst_id.clone(), create_test_tick(&inst_id, 1000.0 + i as f64, 1));
        }

        let stats = cache.get_stats();
        assert_eq!(stats.tick_cache_size, 100);

        // 验证随机访问
        assert!(cache.get_tick("INST_0050").is_some());
        assert!(cache.get_tick("INST_0099").is_some());
    }
}
