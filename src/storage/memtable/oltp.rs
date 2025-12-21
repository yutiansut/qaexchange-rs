// OLTP MemTable - 基于 SkipMap 的低延迟内存表
//
// 性能目标：
// - 写入延迟：P99 < 10μs
// - 读取延迟：P99 < 5μs
// - 并发吞吐：> 1M ops/s
//
// 特性：
// - 无锁并发（Lock-free SkipMap）
// - 自动排序（按时间戳 + 序列号）
// - 范围查询（高效时间范围扫描）
// - 内存限制（自动触发 flush）

use super::types::{MemTableEntry, MemTableKey, MemTableValue};
use crate::storage::wal::WalRecord;
use crossbeam_skiplist::SkipMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// OLTP MemTable 配置
#[derive(Debug, Clone)]
pub struct OltpMemTableConfig {
    /// 最大内存大小（字节），超过则触发 flush
    pub max_size_bytes: usize,

    /// 预估每条记录大小（用于快速内存估算）
    pub estimated_entry_size: usize,
}

impl Default for OltpMemTableConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 64 * 1024 * 1024, // 64 MB
            estimated_entry_size: 256,        // 256 bytes/entry
        }
    }
}

/// OLTP MemTable - 无锁并发内存表
pub struct OltpMemTable {
    /// SkipMap：自动排序的并发键值存储
    /// Key: (timestamp, sequence)
    /// Value: WalRecord
    data: Arc<SkipMap<MemTableKey, MemTableValue>>,

    /// 当前内存使用（近似值）
    size_bytes: Arc<AtomicUsize>,

    /// 配置
    config: OltpMemTableConfig,
}

impl OltpMemTable {
    /// 创建新的 OLTP MemTable
    pub fn new(config: OltpMemTableConfig) -> Self {
        Self {
            data: Arc::new(SkipMap::new()),
            size_bytes: Arc::new(AtomicUsize::new(0)),
            config,
        }
    }

    /// 使用默认配置创建
    pub fn with_default() -> Self {
        Self::new(OltpMemTableConfig::default())
    }

    /// 插入记录（从 WAL 恢复或实时写入）
    ///
    /// # Performance
    /// - P99 < 10μs（无锁并发）
    /// - 自动排序（O(log N)）
    ///
    /// # Arguments
    /// * `sequence` - WAL 序列号
    /// * `record` - WAL 记录
    ///
    /// # Returns
    /// 插入前的内存大小
    pub fn insert(&self, sequence: u64, record: WalRecord) -> usize {
        let entry = MemTableEntry::from_wal(sequence, record);

        // 插入 SkipMap（无锁并发）
        self.data.insert(entry.key, entry.value);

        // 更新内存使用（近似估算）
        

        self
            .size_bytes
            .fetch_add(self.config.estimated_entry_size, Ordering::Relaxed)
    }

    /// 批量插入（更高效）
    ///
    /// # Performance
    /// - 批量操作减少原子操作开销
    pub fn insert_batch(&self, entries: Vec<(u64, WalRecord)>) -> usize {
        for (sequence, record) in entries {
            let entry = MemTableEntry::from_wal(sequence, record);
            self.data.insert(entry.key, entry.value);
        }

        // 批量更新内存大小
        let added_size = self.config.estimated_entry_size * self.data.len();
        self.size_bytes.store(added_size, Ordering::Relaxed);

        added_size
    }

    /// 范围查询：按时间范围查询
    ///
    /// # Performance
    /// - P99 < 5μs（单次查找）
    /// - 范围扫描：~1M entries/s
    ///
    /// # Arguments
    /// * `start_ts` - 起始时间戳（纳秒）
    /// * `end_ts` - 结束时间戳（纳秒）
    ///
    /// # Returns
    /// 符合条件的记录列表
    pub fn range_query(&self, start_ts: i64, end_ts: i64) -> Vec<(MemTableKey, WalRecord)> {
        let start_key = MemTableKey::from_timestamp(start_ts);
        let end_key = MemTableKey::to_timestamp(end_ts);

        self.data
            .range(start_key..=end_key)
            .map(|entry| (*entry.key(), entry.value().record.clone()))
            .collect()
    }

    /// 点查询：查询指定序列号的记录
    ///
    /// # Performance
    /// - P99 < 5μs
    pub fn get(&self, timestamp: i64, sequence: u64) -> Option<WalRecord> {
        let key = MemTableKey::new(timestamp, sequence);
        self.data
            .get(&key)
            .map(|entry| entry.value().record.clone())
    }

    /// 获取所有记录（用于 flush 到 SSTable）
    ///
    /// # Performance
    /// - 已排序，无需额外排序开销
    /// - 按时间戳顺序返回
    pub fn iter_all(&self) -> Vec<(MemTableKey, WalRecord)> {
        self.data
            .iter()
            .map(|entry| (*entry.key(), entry.value().record.clone()))
            .collect()
    }

    /// 检查是否需要 flush（内存超限）
    pub fn should_flush(&self) -> bool {
        self.size_bytes.load(Ordering::Relaxed) >= self.config.max_size_bytes
    }

    /// 获取当前内存使用
    pub fn size_bytes(&self) -> usize {
        self.size_bytes.load(Ordering::Relaxed)
    }

    /// 获取记录数量
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 清空 MemTable（flush 后调用）
    pub fn clear(&self) {
        self.data.clear();
        self.size_bytes.store(0, Ordering::Relaxed);
    }

    /// 获取最小时间戳
    pub fn min_timestamp(&self) -> Option<i64> {
        self.data.front().map(|entry| entry.key().timestamp)
    }

    /// 获取最大时间戳
    pub fn max_timestamp(&self) -> Option<i64> {
        self.data.back().map(|entry| entry.key().timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::WalRecord;

    fn create_order_record(order_id: u64, timestamp: i64) -> WalRecord {
        WalRecord::OrderInsert {
            order_id,
            user_id: [1u8; 32],
            instrument_id: [1u8; 16],
            direction: 0,
            offset: 0,
            price: 4000.0 + order_id as f64,
            volume: 10.0,
            timestamp,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let memtable = OltpMemTable::with_default();

        let record = create_order_record(1, 1000);
        memtable.insert(1, record.clone());

        let retrieved = memtable.get(1000, 1);
        assert!(retrieved.is_some());

        assert_eq!(memtable.len(), 1);
    }

    #[test]
    fn test_range_query() {
        let memtable = OltpMemTable::with_default();

        // 插入多条记录，时间戳递增
        for i in 0..10 {
            let record = create_order_record(i, 1000 + i as i64 * 100);
            memtable.insert(i, record);
        }

        // 范围查询：1000 ~ 1500
        let results = memtable.range_query(1000, 1500);
        assert_eq!(results.len(), 6); // 1000, 1100, 1200, 1300, 1400, 1500

        // 验证顺序
        for (i, (key, _)) in results.iter().enumerate() {
            assert_eq!(key.timestamp, 1000 + i as i64 * 100);
        }
    }

    #[test]
    fn test_ordering() {
        let memtable = OltpMemTable::with_default();

        // 乱序插入
        memtable.insert(3, create_order_record(3, 3000));
        memtable.insert(1, create_order_record(1, 1000));
        memtable.insert(2, create_order_record(2, 2000));

        // 验证自动排序
        let all = memtable.iter_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].0.timestamp, 1000);
        assert_eq!(all[1].0.timestamp, 2000);
        assert_eq!(all[2].0.timestamp, 3000);
    }

    #[test]
    fn test_same_timestamp_ordering() {
        let memtable = OltpMemTable::with_default();

        // 同一时间戳，不同 sequence
        memtable.insert(3, create_order_record(3, 1000));
        memtable.insert(1, create_order_record(1, 1000));
        memtable.insert(2, create_order_record(2, 1000));

        let all = memtable.iter_all();
        // 按 sequence 排序
        assert_eq!(all[0].0.sequence, 1);
        assert_eq!(all[1].0.sequence, 2);
        assert_eq!(all[2].0.sequence, 3);
    }

    #[test]
    fn test_batch_insert() {
        let memtable = OltpMemTable::with_default();

        let mut batch = Vec::new();
        for i in 0..100 {
            batch.push((i, create_order_record(i, 1000 + i as i64)));
        }

        memtable.insert_batch(batch);
        assert_eq!(memtable.len(), 100);
    }

    #[test]
    fn test_should_flush() {
        let config = OltpMemTableConfig {
            max_size_bytes: 1000,
            estimated_entry_size: 100,
        };
        let memtable = OltpMemTable::new(config);

        // 插入 5 条记录，应该不触发 flush
        for i in 0..5 {
            memtable.insert(i, create_order_record(i, 1000 + i as i64));
        }
        assert!(!memtable.should_flush());

        // 插入第 10 条，应该触发 flush
        for i in 5..10 {
            memtable.insert(i, create_order_record(i, 1000 + i as i64));
        }
        assert!(memtable.should_flush());
    }

    #[test]
    fn test_min_max_timestamp() {
        let memtable = OltpMemTable::with_default();

        memtable.insert(1, create_order_record(1, 5000));
        memtable.insert(2, create_order_record(2, 1000));
        memtable.insert(3, create_order_record(3, 3000));

        assert_eq!(memtable.min_timestamp(), Some(1000));
        assert_eq!(memtable.max_timestamp(), Some(5000));
    }

    #[test]
    fn test_concurrent_insert() {
        use std::thread;

        let memtable = Arc::new(OltpMemTable::with_default());

        // 4 个线程并发插入
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let mt = memtable.clone();
                thread::spawn(move || {
                    for i in 0..1000 {
                        let seq = thread_id * 1000 + i;
                        mt.insert(seq, create_order_record(seq, 1000 + seq as i64));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有数据都插入了
        assert_eq!(memtable.len(), 4000);
    }

    #[test]
    #[ignore] // 环境相关的性能测试，在 CI 中跳过
    fn test_performance() {
        let memtable = OltpMemTable::with_default();

        // 测试 10K 写入延迟
        let mut latencies = Vec::new();
        for i in 0..10000 {
            let start = std::time::Instant::now();
            memtable.insert(i, create_order_record(i, 1000 + i as i64));
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_nanos());
        }

        latencies.sort();
        let p50 = latencies[latencies.len() / 2];
        let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

        println!("OLTP MemTable 写入性能:");
        println!("  P50: {} ns", p50);
        println!("  P99: {} ns (目标 < 10,000 ns)", p99);

        // 验证 P99 < 10μs
        assert!(p99 < 10_000, "P99 {} ns exceeds 10μs target", p99);
    }
}
