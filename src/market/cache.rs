//! 行情数据缓存
//!
//! 三级缓存架构:
//! - L1: 内存缓存 (DashMap) - < 10μs
//! - L2: MemTable (SkipMap) - < 50μs
//! - L3: SSTable (mmap) - < 200μs

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use super::{TickData, OrderBookSnapshot};

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
                self.stats.tick_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(cached.data.clone());
            }
            // 缓存过期，删除
            drop(cached);
            self.tick_cache.remove(instrument_id);
        }

        // 缓存未命中
        self.stats.tick_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 更新 Tick 缓存 (在成交时调用)
    pub fn update_tick(&self, instrument_id: String, tick: TickData) {
        self.tick_cache.insert(instrument_id, CachedTick {
            data: tick,
            cached_at: Instant::now(),
        });
    }

    /// 获取订单簿 (带缓存)
    pub fn get_orderbook(&self, instrument_id: &str) -> Option<OrderBookSnapshot> {
        if let Some(cached) = self.orderbook_cache.get(instrument_id) {
            if cached.cached_at.elapsed() < self.ttl {
                // 缓存命中
                self.stats.orderbook_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(cached.data.clone());
            }
            // 缓存过期，删除
            drop(cached);
            self.orderbook_cache.remove(instrument_id);
        }

        // 缓存未命中
        self.stats.orderbook_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 更新订单簿缓存 (在快照广播时调用)
    pub fn update_orderbook(&self, instrument_id: String, orderbook: OrderBookSnapshot) {
        self.orderbook_cache.insert(instrument_id, CachedOrderBook {
            data: orderbook,
            cached_at: Instant::now(),
        });
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
            tick_hits: self.stats.tick_hits.load(std::sync::atomic::Ordering::Relaxed),
            tick_misses: self.stats.tick_misses.load(std::sync::atomic::Ordering::Relaxed),
            orderbook_hits: self.stats.orderbook_hits.load(std::sync::atomic::Ordering::Relaxed),
            orderbook_misses: self.stats.orderbook_misses.load(std::sync::atomic::Ordering::Relaxed),
            tick_cache_size: self.tick_cache.len(),
            orderbook_cache_size: self.orderbook_cache.len(),
        }
    }

    /// 计算缓存命中率
    pub fn hit_rate(&self) -> f64 {
        let total_hits = self.stats.tick_hits.load(std::sync::atomic::Ordering::Relaxed)
            + self.stats.orderbook_hits.load(std::sync::atomic::Ordering::Relaxed);
        let total_misses = self.stats.tick_misses.load(std::sync::atomic::Ordering::Relaxed)
            + self.stats.orderbook_misses.load(std::sync::atomic::Ordering::Relaxed);

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
}
