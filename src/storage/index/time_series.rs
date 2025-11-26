//! 时间序列索引
//!
//! @yutiansut @quantaxis
//!
//! 设计理念：
//! - 基于 B+ 树的时间戳索引
//! - 支持高效范围查询 O(log n + k)
//! - 内存紧凑：使用 Vec 存储有序条目
//! - 支持批量插入优化

use std::collections::BTreeMap;

// ═══════════════════════════════════════════════════════════════════════════
// 基础类型
// ═══════════════════════════════════════════════════════════════════════════

/// 时间范围
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}

impl TimeRange {
    pub fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }

    /// 检查是否与另一个范围重叠
    #[inline]
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start <= other.end && self.end >= other.start
    }

    /// 检查是否包含某个时间戳
    #[inline]
    pub fn contains(&self, timestamp: i64) -> bool {
        timestamp >= self.start && timestamp <= self.end
    }

    /// 合并两个范围
    pub fn merge(&self, other: &TimeRange) -> TimeRange {
        TimeRange {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// 索引条目
#[derive(Debug, Clone, Copy)]
pub struct IndexEntry {
    /// 时间戳（纳秒）
    pub timestamp: i64,
    /// 文件/记录偏移量
    pub offset: u64,
}

impl IndexEntry {
    pub fn new(timestamp: i64, offset: u64) -> Self {
        Self { timestamp, offset }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 时间序列索引
// ═══════════════════════════════════════════════════════════════════════════

/// 时间序列索引
///
/// 使用 BTreeMap 实现高效的时间范围查询
pub struct TimeSeriesIndex {
    /// 时间戳 → 偏移量映射
    /// BTreeMap 保证 O(log n) 查找和有序遍历
    entries: BTreeMap<i64, Vec<u64>>,
    /// 条目总数
    entry_count: u64,
    /// 时间范围
    time_range: Option<TimeRange>,
}

impl TimeSeriesIndex {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            entry_count: 0,
            time_range: None,
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(offset);

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

    /// 批量添加（优化性能）
    pub fn add_batch(&mut self, entries: &[(i64, u64)]) {
        for (timestamp, offset) in entries {
            self.add(*timestamp, *offset);
        }
    }

    /// 范围查询
    ///
    /// 返回时间范围内的所有偏移量
    pub fn query_range(&self, start_ts: i64, end_ts: i64) -> Vec<u64> {
        // 快速路径：检查是否在索引范围内
        if let Some(range) = &self.time_range {
            if end_ts < range.start || start_ts > range.end {
                return Vec::new();
            }
        }

        let mut result = Vec::new();

        // 使用 BTreeMap 的 range 方法进行高效查询
        for (_ts, offsets) in self.entries.range(start_ts..=end_ts) {
            result.extend(offsets.iter().copied());
        }

        result
    }

    /// 点查询（精确时间戳）
    pub fn query_exact(&self, timestamp: i64) -> Vec<u64> {
        self.entries
            .get(&timestamp)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// 获取时间范围
    pub fn time_range(&self) -> Option<TimeRange> {
        self.time_range
    }

    /// 获取条目数量
    pub fn len(&self) -> u64 {
        self.entry_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// 清空索引
    pub fn clear(&mut self) {
        self.entries.clear();
        self.entry_count = 0;
        self.time_range = None;
    }

    /// 获取第一个时间戳
    pub fn first_timestamp(&self) -> Option<i64> {
        self.entries.keys().next().copied()
    }

    /// 获取最后一个时间戳
    pub fn last_timestamp(&self) -> Option<i64> {
        self.entries.keys().next_back().copied()
    }

    /// 估算内存使用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        // BTreeMap 节点开销 + Vec 开销
        let entry_size = std::mem::size_of::<i64>() + std::mem::size_of::<Vec<u64>>();
        let offset_size = std::mem::size_of::<u64>();

        self.entries.len() * entry_size + self.entry_count as usize * offset_size
    }
}

impl Default for TimeSeriesIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_range_operations() {
        let r1 = TimeRange::new(100, 200);
        let r2 = TimeRange::new(150, 250);
        let r3 = TimeRange::new(300, 400);

        assert!(r1.overlaps(&r2));
        assert!(!r1.overlaps(&r3));
        assert!(r1.contains(150));
        assert!(!r1.contains(250));

        let merged = r1.merge(&r2);
        assert_eq!(merged.start, 100);
        assert_eq!(merged.end, 250);
    }

    #[test]
    fn test_time_series_index() {
        let mut index = TimeSeriesIndex::new();

        // 添加条目
        index.add(1000, 0);
        index.add(1500, 1);
        index.add(2000, 2);
        index.add(2000, 3); // 同一时间戳多个条目
        index.add(3000, 4);

        assert_eq!(index.len(), 5);

        // 范围查询
        let result = index.query_range(1000, 2000);
        assert_eq!(result.len(), 4); // 包含 0, 1, 2, 3

        // 点查询
        let exact = index.query_exact(2000);
        assert_eq!(exact.len(), 2);

        // 边界外查询
        let empty = index.query_range(5000, 6000);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_time_series_index_time_range() {
        let mut index = TimeSeriesIndex::new();

        index.add(500, 0);
        index.add(1000, 1);
        index.add(200, 2);

        let range = index.time_range().unwrap();
        assert_eq!(range.start, 200);
        assert_eq!(range.end, 1000);
    }
}
