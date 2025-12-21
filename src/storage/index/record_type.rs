//! 记录类型索引
//!
//! @yutiansut @quantaxis
//!
//! 设计理念：
//! - 按记录类型分区的时序索引
//! - 支持类型集合查询（多类型联合）
//! - 使用位掩码实现 O(1) 类型匹配

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::storage::hybrid::query_filter::{RecordType, RecordTypeSet};

use super::time_series::TimeRange;

// ═══════════════════════════════════════════════════════════════════════════
// 记录类型索引
// ═══════════════════════════════════════════════════════════════════════════

/// 单个类型的索引数据
#[derive(Debug, Clone)]
struct TypeData {
    /// 时间戳 → 偏移量（有序）
    entries: BTreeMap<i64, Vec<u64>>,
    /// 偏移量集合（用于快速查找）
    offset_set: HashSet<u64>,
    /// 时间范围
    time_range: Option<TimeRange>,
    /// 条目数量
    entry_count: u64,
}

impl TypeData {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            offset_set: HashSet::new(),
            time_range: None,
            entry_count: 0,
        }
    }

    fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_default()
            .push(offset);

        self.offset_set.insert(offset);
        self.entry_count += 1;

        // 更新时间范围
        match &mut self.time_range {
            Some(range) => {
                if timestamp < range.start {
                    range.start = timestamp;
                }
                if timestamp > range.end {
                    range.end = timestamp;
                }
            }
            None => {
                self.time_range = Some(TimeRange::new(timestamp, timestamp));
            }
        }
    }

    fn query_range(&self, start_ts: i64, end_ts: i64) -> Vec<u64> {
        // 快速路径
        if let Some(range) = &self.time_range {
            if end_ts < range.start || start_ts > range.end {
                return Vec::new();
            }
        }

        let mut result = Vec::new();
        for (_ts, offsets) in self.entries.range(start_ts..=end_ts) {
            result.extend(offsets.iter().copied());
        }
        result
    }

    fn contains_offset(&self, offset: u64) -> bool {
        self.offset_set.contains(&offset)
    }
}

/// 记录类型索引
///
/// 按记录类型分区的时序索引
pub struct RecordTypeIndex {
    /// 类型 → 索引数据
    types: HashMap<RecordType, TypeData>,
    /// 总条目数
    total_entries: u64,
}

impl RecordTypeIndex {
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
            total_entries: 0,
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, record_type: RecordType, timestamp: i64, offset: u64) {
        let data = self.types.entry(record_type).or_insert_with(TypeData::new);

        data.add(timestamp, offset);
        self.total_entries += 1;
    }

    /// 范围查询（单个类型）
    pub fn query_range(&self, record_type: RecordType, start_ts: i64, end_ts: i64) -> Vec<u64> {
        self.types
            .get(&record_type)
            .map(|data| data.query_range(start_ts, end_ts))
            .unwrap_or_default()
    }

    /// 范围查询（多个类型）
    pub fn query_range_for_types(
        &self,
        start_ts: i64,
        end_ts: i64,
        types: &RecordTypeSet,
    ) -> Vec<u64> {
        let mut result = Vec::new();

        // 遍历所有匹配的类型
        for (record_type, data) in &self.types {
            if types.contains(*record_type) {
                result.extend(data.query_range(start_ts, end_ts));
            }
        }

        // 按偏移量排序（保证顺序性）
        result.sort_unstable();
        result
    }

    /// 检查偏移量是否属于指定类型集合
    pub fn contains_offset_in_types(&self, offset: u64, types: &RecordTypeSet) -> bool {
        for (record_type, data) in &self.types {
            if types.contains(*record_type) && data.contains_offset(offset) {
                return true;
            }
        }
        false
    }

    /// 获取类型的时间范围
    pub fn get_time_range(&self, record_type: RecordType) -> Option<TimeRange> {
        self.types.get(&record_type).and_then(|d| d.time_range)
    }

    /// 获取所有有索引的类型
    pub fn list_types(&self) -> Vec<RecordType> {
        self.types.keys().copied().collect()
    }

    /// 获取类型数量
    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    /// 获取总条目数
    pub fn total_entries(&self) -> u64 {
        self.total_entries
    }

    /// 检查类型是否存在
    pub fn contains(&self, record_type: RecordType) -> bool {
        self.types.contains_key(&record_type)
    }

    /// 清空索引
    pub fn clear(&mut self) {
        self.types.clear();
        self.total_entries = 0;
    }

    /// 估算内存使用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        let mut size = 0;

        for data in self.types.values() {
            // BTreeMap 条目
            size += data.entries.len() * (std::mem::size_of::<i64>() + std::mem::size_of::<Vec<u64>>());
            // Vec 内容
            size += data.entry_count as usize * std::mem::size_of::<u64>();
            // HashSet
            size += data.offset_set.len() * std::mem::size_of::<u64>();
        }

        size
    }
}

impl Default for RecordTypeIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_index() {
        let mut index = RecordTypeIndex::new();

        // 添加不同类型的数据
        index.add(RecordType::TickData, 1000, 0);
        index.add(RecordType::TickData, 2000, 1);
        index.add(RecordType::KLineFinished, 1500, 2);
        index.add(RecordType::OrderInsert, 2500, 3);

        assert_eq!(index.type_count(), 3);
        assert_eq!(index.total_entries(), 4);

        // 查询 TickData
        let tick_result = index.query_range(RecordType::TickData, 0, 3000);
        assert_eq!(tick_result.len(), 2);

        // 查询 KLineFinished
        let kline_result = index.query_range(RecordType::KLineFinished, 0, 3000);
        assert_eq!(kline_result.len(), 1);
    }

    #[test]
    fn test_multi_type_query() {
        let mut index = RecordTypeIndex::new();

        index.add(RecordType::TickData, 1000, 0);
        index.add(RecordType::OrderBookSnapshot, 1500, 1);
        index.add(RecordType::KLineFinished, 2000, 2);
        index.add(RecordType::OrderInsert, 2500, 3);

        // 查询市场数据类型（TickData + OrderBookSnapshot + KLineFinished）
        let market_types = RecordTypeSet::new()
            .insert(RecordType::TickData)
            .insert(RecordType::OrderBookSnapshot)
            .insert(RecordType::KLineFinished);

        let result = index.query_range_for_types(0, 3000, &market_types);
        assert_eq!(result.len(), 3); // 0, 1, 2
    }

    #[test]
    fn test_contains_offset() {
        let mut index = RecordTypeIndex::new();

        index.add(RecordType::TickData, 1000, 42);
        index.add(RecordType::OrderInsert, 1500, 100);

        let tick_types = RecordTypeSet::new().insert(RecordType::TickData);

        assert!(index.contains_offset_in_types(42, &tick_types));
        assert!(!index.contains_offset_in_types(100, &tick_types));
    }

    #[test]
    fn test_time_range() {
        let mut index = RecordTypeIndex::new();

        index.add(RecordType::TickData, 1000, 0);
        index.add(RecordType::TickData, 3000, 1);
        index.add(RecordType::TickData, 2000, 2);

        let range = index.get_time_range(RecordType::TickData).unwrap();
        assert_eq!(range.start, 1000);
        assert_eq!(range.end, 3000);
    }
}
