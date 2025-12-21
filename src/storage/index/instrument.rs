//! 合约索引
//!
//! @yutiansut @quantaxis
//!
//! 设计理念：
//! - 按合约ID分区的时序索引
//! - 支持快速按合约查询
//! - 内存高效：使用字符串驻留池

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::time_series::TimeRange;

// ═══════════════════════════════════════════════════════════════════════════
// 合约索引
// ═══════════════════════════════════════════════════════════════════════════

/// 单个合约的索引数据
#[derive(Debug, Clone)]
struct InstrumentData {
    /// 时间戳 → 偏移量（有序）
    entries: BTreeMap<i64, Vec<u64>>,
    /// 时间范围
    time_range: Option<TimeRange>,
    /// 条目数量
    entry_count: u64,
}

impl InstrumentData {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            time_range: None,
            entry_count: 0,
        }
    }

    fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_default()
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
}

/// 合约索引
///
/// 按合约ID分区的时序索引
pub struct InstrumentIndex {
    /// 合约ID → 索引数据
    instruments: HashMap<Arc<str>, InstrumentData>,
    /// 字符串驻留池（减少内存分配）
    string_pool: HashMap<String, Arc<str>>,
    /// 总条目数
    total_entries: u64,
}

impl InstrumentIndex {
    pub fn new() -> Self {
        Self {
            instruments: HashMap::new(),
            string_pool: HashMap::new(),
            total_entries: 0,
        }
    }

    /// 获取或创建驻留字符串
    #[inline]
    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(interned) = self.string_pool.get(s) {
            return Arc::clone(interned);
        }

        let interned: Arc<str> = Arc::from(s);
        self.string_pool.insert(s.to_string(), Arc::clone(&interned));
        interned
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, instrument_id: &str, timestamp: i64, offset: u64) {
        let key = self.intern(instrument_id);

        let data = self
            .instruments
            .entry(key)
            .or_insert_with(InstrumentData::new);

        data.add(timestamp, offset);
        self.total_entries += 1;
    }

    /// 范围查询
    pub fn query_range(&self, instrument_id: &str, start_ts: i64, end_ts: i64) -> Vec<u64> {
        let key: Arc<str> = Arc::from(instrument_id);

        self.instruments
            .get(&key)
            .map(|data| data.query_range(start_ts, end_ts))
            .unwrap_or_default()
    }

    /// 获取合约的时间范围
    pub fn get_time_range(&self, instrument_id: &str) -> Option<TimeRange> {
        let key: Arc<str> = Arc::from(instrument_id);
        self.instruments.get(&key).and_then(|d| d.time_range)
    }

    /// 获取所有合约ID
    pub fn list_instruments(&self) -> Vec<&str> {
        self.instruments.keys().map(|k| k.as_ref()).collect()
    }

    /// 获取合约数量
    pub fn instrument_count(&self) -> usize {
        self.instruments.len()
    }

    /// 获取总条目数
    pub fn total_entries(&self) -> u64 {
        self.total_entries
    }

    /// 检查合约是否存在
    pub fn contains(&self, instrument_id: &str) -> bool {
        let key: Arc<str> = Arc::from(instrument_id);
        self.instruments.contains_key(&key)
    }

    /// 清空索引
    pub fn clear(&mut self) {
        self.instruments.clear();
        self.string_pool.clear();
        self.total_entries = 0;
    }

    /// 估算内存使用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        let mut size = 0;

        // 字符串池
        for s in self.string_pool.keys() {
            size += s.len() + std::mem::size_of::<Arc<str>>();
        }

        // 每个合约的数据
        for data in self.instruments.values() {
            size += data.entries.len() * (std::mem::size_of::<i64>() + std::mem::size_of::<Vec<u64>>());
            size += data.entry_count as usize * std::mem::size_of::<u64>();
        }

        size
    }
}

impl Default for InstrumentIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_index() {
        let mut index = InstrumentIndex::new();

        // 添加多个合约的数据
        index.add("cu2501", 1000, 0);
        index.add("cu2501", 2000, 1);
        index.add("au2501", 1500, 2);
        index.add("au2501", 2500, 3);

        assert_eq!(index.instrument_count(), 2);
        assert_eq!(index.total_entries(), 4);

        // 查询 cu2501
        let cu_result = index.query_range("cu2501", 0, 3000);
        assert_eq!(cu_result.len(), 2);

        // 查询 au2501
        let au_result = index.query_range("au2501", 0, 3000);
        assert_eq!(au_result.len(), 2);

        // 查询不存在的合约
        let empty = index.query_range("ag2501", 0, 3000);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_instrument_time_range() {
        let mut index = InstrumentIndex::new();

        index.add("cu2501", 1000, 0);
        index.add("cu2501", 3000, 1);
        index.add("cu2501", 2000, 2);

        let range = index.get_time_range("cu2501").unwrap();
        assert_eq!(range.start, 1000);
        assert_eq!(range.end, 3000);
    }

    #[test]
    fn test_string_interning() {
        let mut index = InstrumentIndex::new();

        // 多次添加相同合约
        index.add("cu2501", 1000, 0);
        index.add("cu2501", 2000, 1);
        index.add("cu2501", 3000, 2);

        // 字符串池应该只有一个条目
        assert_eq!(index.string_pool.len(), 1);
    }
}
