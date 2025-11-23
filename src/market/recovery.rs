//! 行情数据恢复模块
//!
//! 从WAL恢复Tick和OrderBook数据到缓存

use crate::market::{MarketDataCache, OrderBookSnapshot, PriceLevel, TickData};
use crate::storage::hybrid::OltpHybridStorage;
use crate::storage::wal::record::WalRecord;
use crate::ExchangeError;
use std::collections::HashMap;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// 恢复的行情数据
#[derive(Debug, Clone)]
pub struct RecoveredMarketData {
    /// 恢复的Tick数据 (instrument_id -> TickData)
    pub ticks: HashMap<String, TickData>,

    /// 恢复的订单簿快照 (instrument_id -> OrderBookSnapshot)
    pub orderbook_snapshots: HashMap<String, OrderBookSnapshot>,

    /// 统计信息
    pub stats: RecoveryStats,
}

/// 恢复统计
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    pub total_records: usize,
    pub tick_records: usize,
    pub orderbook_records: usize,
    pub delta_records: usize,
    pub recovery_time_ms: u128,
}

/// 行情数据恢复器
pub struct MarketDataRecovery {
    storage: Arc<OltpHybridStorage>,
    cache: Arc<MarketDataCache>,
}

impl MarketDataRecovery {
    /// 创建恢复器
    pub fn new(storage: Arc<OltpHybridStorage>, cache: Arc<MarketDataCache>) -> Self {
        Self { storage, cache }
    }

    /// 从WAL恢复行情数据
    pub fn recover_market_data(&self, start_ts: i64, end_ts: i64) -> Result<RecoveredMarketData> {
        let start_time = std::time::Instant::now();

        let mut ticks: HashMap<String, TickData> = HashMap::new();
        let mut orderbook_snapshots: HashMap<String, OrderBookSnapshot> = HashMap::new();
        let mut stats = RecoveryStats::default();

        // 从WAL读取记录
        let records = self
            .storage
            .range_query(start_ts, end_ts)
            .map_err(|e| ExchangeError::InternalError(format!("Failed to query WAL: {}", e)))?;

        log::info!(
            "Recovering market data from {} records (ts range: {} - {})",
            records.len(),
            start_ts,
            end_ts
        );

        for (timestamp, _sequence, record) in records {
            stats.total_records += 1;

            match record {
                WalRecord::TickData {
                    instrument_id,
                    last_price,
                    bid_price,
                    ask_price,
                    volume,
                    timestamp,
                } => {
                    stats.tick_records += 1;

                    let inst_str = WalRecord::from_fixed_array(&instrument_id);
                    let tick = TickData {
                        instrument_id: inst_str.clone(),
                        timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                        last_price,
                        bid_price: if bid_price > 0.0 {
                            Some(bid_price)
                        } else {
                            None
                        },
                        ask_price: if ask_price > 0.0 {
                            Some(ask_price)
                        } else {
                            None
                        },
                        volume,
                    };

                    // 保留最新的Tick（按时间戳）
                    ticks
                        .entry(inst_str)
                        .and_modify(|existing| {
                            if tick.timestamp > existing.timestamp {
                                *existing = tick.clone();
                            }
                        })
                        .or_insert(tick);
                }

                WalRecord::OrderBookSnapshot {
                    instrument_id,
                    bids,
                    asks,
                    last_price,
                    timestamp,
                } => {
                    stats.orderbook_records += 1;

                    let inst_str = WalRecord::from_fixed_array(&instrument_id);

                    // 转换固定数组为Vec
                    let bids_vec: Vec<PriceLevel> = bids
                        .iter()
                        .filter(|(price, _)| *price > 0.0)
                        .map(|(price, volume)| PriceLevel {
                            price: *price,
                            volume: *volume,
                        })
                        .collect();

                    let asks_vec: Vec<PriceLevel> = asks
                        .iter()
                        .filter(|(price, _)| *price > 0.0)
                        .map(|(price, volume)| PriceLevel {
                            price: *price,
                            volume: *volume,
                        })
                        .collect();

                    let snapshot = OrderBookSnapshot {
                        instrument_id: inst_str.clone(),
                        timestamp: timestamp / 1_000_000, // 纳秒转毫秒
                        bids: bids_vec,
                        asks: asks_vec,
                        last_price: if last_price > 0.0 {
                            Some(last_price)
                        } else {
                            None
                        },
                    };

                    // 保留最新的快照
                    orderbook_snapshots
                        .entry(inst_str)
                        .and_modify(|existing| {
                            if snapshot.timestamp > existing.timestamp {
                                *existing = snapshot.clone();
                            }
                        })
                        .or_insert(snapshot);
                }

                WalRecord::OrderBookDelta { .. } => {
                    stats.delta_records += 1;
                    // Delta记录可以用于增量恢复（未来实现）
                }

                _ => {
                    // 忽略其他类型的记录
                }
            }
        }

        stats.recovery_time_ms = start_time.elapsed().as_millis();

        log::info!(
            "Market data recovery completed: {} ticks, {} orderbooks in {}ms",
            stats.tick_records,
            stats.orderbook_records,
            stats.recovery_time_ms
        );

        Ok(RecoveredMarketData {
            ticks,
            orderbook_snapshots,
            stats,
        })
    }

    /// 恢复并填充到缓存
    pub fn recover_to_cache(&self, start_ts: i64, end_ts: i64) -> Result<RecoveryStats> {
        let recovered = self.recover_market_data(start_ts, end_ts)?;

        // 填充Tick缓存
        for (instrument_id, tick) in recovered.ticks {
            self.cache.update_tick(instrument_id, tick);
        }

        // 填充订单簿缓存
        for (instrument_id, snapshot) in recovered.orderbook_snapshots {
            self.cache.update_orderbook(instrument_id, snapshot);
        }

        log::info!(
            "Market data cache populated: {} instruments",
            recovered.stats.tick_records + recovered.stats.orderbook_records
        );

        Ok(recovered.stats)
    }

    /// 恢复最近N分钟的行情数据
    pub fn recover_recent_minutes(&self, minutes: i64) -> Result<RecoveryStats> {
        let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let start_ts = end_ts - (minutes * 60 * 1_000_000_000);

        self.recover_to_cache(start_ts, end_ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_stats() {
        let stats = RecoveryStats {
            total_records: 1000,
            tick_records: 800,
            orderbook_records: 150,
            delta_records: 50,
            recovery_time_ms: 123,
        };

        assert_eq!(stats.total_records, 1000);
        assert_eq!(stats.tick_records, 800);
        assert_eq!(stats.orderbook_records, 150);
    }
}
