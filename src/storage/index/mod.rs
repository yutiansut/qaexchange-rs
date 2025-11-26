//! 二级索引模块
//!
//! @yutiansut @quantaxis
//!
//! 设计理念：
//! - 为常用查询模式提供快速索引
//! - 支持 instrument_id、record_type、时间范围复合查询
//! - 内存高效：使用紧凑数据结构
//! - 支持持久化和恢复
//!
//! 索引类型：
//! - InstrumentIndex: 合约ID → 时间戳范围
//! - RecordTypeIndex: 记录类型 → 时间戳范围
//! - TimeIndex: 时间戳 → 文件偏移（用于精确定位）

pub mod time_series;
pub mod instrument;
pub mod record_type;

pub use time_series::{TimeSeriesIndex, TimeRange, IndexEntry};
pub use instrument::InstrumentIndex;
pub use record_type::RecordTypeIndex;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::storage::hybrid::query_filter::{RecordType, RecordTypeSet};

// ═══════════════════════════════════════════════════════════════════════════
// 复合索引管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 复合索引管理器
///
/// 统一管理所有二级索引，支持复合查询优化
pub struct CompositeIndexManager {
    /// 合约索引
    pub instrument_index: Arc<RwLock<InstrumentIndex>>,
    /// 记录类型索引
    pub record_type_index: Arc<RwLock<RecordTypeIndex>>,
    /// 时间序列索引（主索引）
    pub time_index: Arc<RwLock<TimeSeriesIndex>>,
    /// 索引统计
    stats: IndexStats,
}

/// 索引统计信息
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    /// 索引条目总数
    pub total_entries: u64,
    /// 索引命中次数
    pub hits: u64,
    /// 索引未命中次数
    pub misses: u64,
    /// 最后更新时间戳
    pub last_update_ts: i64,
}

impl CompositeIndexManager {
    /// 创建新的索引管理器
    pub fn new() -> Self {
        Self {
            instrument_index: Arc::new(RwLock::new(InstrumentIndex::new())),
            record_type_index: Arc::new(RwLock::new(RecordTypeIndex::new())),
            time_index: Arc::new(RwLock::new(TimeSeriesIndex::new())),
            stats: IndexStats::default(),
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add_entry(
        &mut self,
        timestamp: i64,
        instrument_id: Option<&str>,
        record_type: RecordType,
        offset: u64,
    ) {
        // 更新时间索引
        {
            let mut time_idx = self.time_index.write();
            time_idx.add(timestamp, offset);
        }

        // 更新合约索引
        if let Some(inst) = instrument_id {
            let mut inst_idx = self.instrument_index.write();
            inst_idx.add(inst, timestamp, offset);
        }

        // 更新类型索引
        {
            let mut type_idx = self.record_type_index.write();
            type_idx.add(record_type, timestamp, offset);
        }

        self.stats.total_entries += 1;
        self.stats.last_update_ts = timestamp;
    }

    /// 查询时间范围内的偏移量
    ///
    /// 使用最优索引路径
    pub fn query_offsets(
        &self,
        start_ts: i64,
        end_ts: i64,
        instrument_id: Option<&str>,
        record_types: Option<&RecordTypeSet>,
    ) -> Vec<u64> {
        // 选择最优索引路径
        let use_instrument_index = instrument_id.is_some();
        let use_type_index = record_types.is_some();

        // 优先使用合约索引（通常选择性更高）
        if use_instrument_index {
            let inst_idx = self.instrument_index.read();
            if let Some(inst) = instrument_id {
                let entries = inst_idx.query_range(inst, start_ts, end_ts);

                // 如果还有类型过滤，进一步筛选
                if let Some(types) = record_types {
                    let type_idx = self.record_type_index.read();
                    return entries
                        .into_iter()
                        .filter(|offset| {
                            type_idx.contains_offset_in_types(*offset, types)
                        })
                        .collect();
                }

                return entries;
            }
        }

        // 使用类型索引
        if use_type_index {
            if let Some(types) = record_types {
                let type_idx = self.record_type_index.read();
                return type_idx.query_range_for_types(start_ts, end_ts, types);
            }
        }

        // 回退到时间索引
        let time_idx = self.time_index.read();
        time_idx.query_range(start_ts, end_ts)
    }

    /// 获取合约的时间范围
    pub fn get_instrument_time_range(&self, instrument_id: &str) -> Option<TimeRange> {
        let inst_idx = self.instrument_index.read();
        inst_idx.get_time_range(instrument_id)
    }

    /// 获取记录类型的时间范围
    pub fn get_record_type_time_range(&self, record_type: RecordType) -> Option<TimeRange> {
        let type_idx = self.record_type_index.read();
        type_idx.get_time_range(record_type)
    }

    /// 获取索引统计
    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }

    /// 清空所有索引
    pub fn clear(&mut self) {
        self.instrument_index.write().clear();
        self.record_type_index.write().clear();
        self.time_index.write().clear();
        self.stats = IndexStats::default();
    }
}

impl Default for CompositeIndexManager {
    fn default() -> Self {
        Self::new()
    }
}
