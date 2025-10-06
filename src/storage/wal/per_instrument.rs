// Per-Instrument WAL Manager - 品种级并发 WAL
//
// 核心优化：
// 1. 每个品种独立的 WAL 文件和管理器
// 2. 并发写入：不同品种可以并行写入，吞吐量提升 N 倍（N = 活跃品种数）
// 3. 独立恢复：某个品种崩溃不影响其他品种
// 4. 更小的 WAL 文件：每个品种的数据量更小，更容易管理
//
// 性能提升：
// - 吞吐量：单一 WAL (78K entries/s) → 品种级并发 (78K × N entries/s)
// - 延迟：无竞争，减少锁等待

use super::manager::WalManager;
use super::record::WalRecord;
use dashmap::DashMap;
use std::sync::Arc;

/// 品种级并发 WAL 管理器
pub struct PerInstrumentWalManager {
    /// 基础路径，每个品种创建子目录: {base_path}/{instrument_id}/
    base_path: String,

    /// 每个品种的 WAL 管理器（无锁并发访问）
    /// Key: instrument_id (String)
    /// Value: Arc<WalManager>
    managers: Arc<DashMap<String, Arc<WalManager>>>,
}

impl PerInstrumentWalManager {
    /// 创建新的品种级 WAL 管理器
    ///
    /// # Arguments
    /// * `base_path` - 基础路径，每个品种创建子目录
    ///
    /// # Example
    /// ```no_run
    /// use qaexchange::storage::wal::PerInstrumentWalManager;
    ///
    /// let mgr = PerInstrumentWalManager::new("/data/wal");
    /// // 品种 "IF2501" 的数据会写入 /data/wal/IF2501/
    /// // 品种 "IC2501" 的数据会写入 /data/wal/IC2501/
    /// ```
    pub fn new(base_path: &str) -> Self {
        std::fs::create_dir_all(base_path).unwrap();

        Self {
            base_path: base_path.to_string(),
            managers: Arc::new(DashMap::new()),
        }
    }

    /// 获取或创建指定品种的 WAL 管理器
    fn get_or_create_manager(&self, instrument_id: &str) -> Arc<WalManager> {
        // 使用 DashMap 的 entry API 实现无锁并发插入
        self.managers
            .entry(instrument_id.to_string())
            .or_insert_with(|| {
                let wal_path = format!("{}/{}", self.base_path, instrument_id);
                Arc::new(WalManager::new(&wal_path))
            })
            .clone()
    }

    /// 追加 WAL 记录（自动路由到对应品种的 WAL）
    ///
    /// # Arguments
    /// * `instrument_id` - 品种 ID
    /// * `record` - WAL 记录
    ///
    /// # Returns
    /// 品种内的序列号（从 1 开始递增）
    ///
    /// # Performance
    /// - 不同品种的写入完全并发，无竞争
    /// - 同一品种的写入串行（由 WalManager 内部保证）
    pub fn append(&self, instrument_id: &str, record: WalRecord) -> Result<u64, String> {
        let manager = self.get_or_create_manager(instrument_id);
        manager.append(record)
    }

    /// 批量追加 WAL 记录（同一品种）
    ///
    /// # Arguments
    /// * `instrument_id` - 品种 ID
    /// * `records` - WAL 记录列表
    ///
    /// # Returns
    /// 品种内的序列号列表
    ///
    /// # Performance
    /// - 批量写入只 fsync 一次，高吞吐
    /// - 适合批量成交回报、批量订单插入等场景
    pub fn append_batch(
        &self,
        instrument_id: &str,
        records: Vec<WalRecord>,
    ) -> Result<Vec<u64>, String> {
        let manager = self.get_or_create_manager(instrument_id);
        manager.append_batch(records)
    }

    /// 回放指定品种的 WAL（崩溃恢复）
    ///
    /// # Arguments
    /// * `instrument_id` - 品种 ID
    /// * `callback` - 每条记录的回调函数
    ///
    /// # Example
    /// ```no_run
    /// # use qaexchange::storage::wal::PerInstrumentWalManager;
    /// let mgr = PerInstrumentWalManager::new("/data/wal");
    /// mgr.replay("IF2501", |entry| {
    ///     println!("Recovered: {:?}", entry);
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn replay<F>(&self, instrument_id: &str, callback: F) -> Result<(), String>
    where
        F: FnMut(super::record::WalEntry) -> Result<(), String>,
    {
        let manager = self.get_or_create_manager(instrument_id);
        manager.replay(callback)
    }

    /// 回放所有品种的 WAL（完整恢复）
    ///
    /// # Arguments
    /// * `callback` - 每条记录的回调函数（接收 instrument_id 和 entry）
    ///
    /// # Performance
    /// - 可以并发回放多个品种（由调用者实现）
    /// - 当前实现为串行回放，生产环境建议使用 rayon 并发
    pub fn replay_all<F>(&self, mut callback: F) -> Result<(), String>
    where
        F: FnMut(&str, super::record::WalEntry) -> Result<(), String>,
    {
        // 扫描所有品种目录
        let entries = std::fs::read_dir(&self.base_path)
            .map_err(|e| format!("Read base path failed: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(instrument_id) = path.file_name().and_then(|s| s.to_str()) {
                    // 回放该品种的 WAL
                    self.replay(instrument_id, |wal_entry| {
                        callback(instrument_id, wal_entry)
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Checkpoint：截断指定品种的旧 WAL 文件
    ///
    /// # Arguments
    /// * `instrument_id` - 品种 ID
    /// * `sequence` - 截断点（< sequence 的数据会被删除）
    pub fn checkpoint(&self, instrument_id: &str, sequence: u64) -> Result<(), String> {
        if let Some(manager) = self.managers.get(instrument_id) {
            manager.checkpoint(sequence)
        } else {
            Ok(()) // 品种不存在，无需操作
        }
    }

    /// Checkpoint 所有品种
    ///
    /// # Arguments
    /// * `sequences` - 每个品种的截断点
    pub fn checkpoint_all(&self, sequences: &std::collections::HashMap<String, u64>) -> Result<(), String> {
        for (instrument_id, sequence) in sequences {
            self.checkpoint(instrument_id, *sequence)?;
        }
        Ok(())
    }

    /// 获取当前活跃的品种列表
    pub fn active_instruments(&self) -> Vec<String> {
        self.managers.iter().map(|kv| kv.key().clone()).collect()
    }

    /// 获取指定品种的当前序列号
    pub fn current_sequence(&self, instrument_id: &str) -> Option<u64> {
        self.managers.get(instrument_id).map(|mgr| {
            mgr.get_current_sequence()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::record::WalRecord;

    #[test]
    fn test_per_instrument_wal() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let mgr = PerInstrumentWalManager::new(tmp_dir.path().to_str().unwrap());

        // 写入不同品种的数据
        let seq1 = mgr.append("IF2501", WalRecord::OrderInsert {
            order_id: 1,
            user_id: [1u8; 32],
            instrument_id: [1u8; 16],
            direction: 0,
            offset: 0,
            price: 4000.0,
            volume: 10.0,
            timestamp: 12345,
        }).unwrap();

        let seq2 = mgr.append("IC2501", WalRecord::OrderInsert {
            order_id: 1,  // 不同品种可以有相同的 order_id
            user_id: [2u8; 32],
            instrument_id: [2u8; 16],
            direction: 0,
            offset: 0,
            price: 6000.0,
            volume: 10.0,
            timestamp: 12345,
        }).unwrap();

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 1);  // 每个品种独立计数

        // 验证活跃品种列表
        let instruments = mgr.active_instruments();
        assert_eq!(instruments.len(), 2);
        assert!(instruments.contains(&"IF2501".to_string()));
        assert!(instruments.contains(&"IC2501".to_string()));
    }

    #[test]
    fn test_batch_append_per_instrument() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let mgr = PerInstrumentWalManager::new(tmp_dir.path().to_str().unwrap());

        // 批量写入同一品种
        let mut records = Vec::new();
        for i in 0..100 {
            records.push(WalRecord::TradeExecuted {
                trade_id: i,
                order_id: i,
                exchange_order_id: i,
                price: 4000.0 + i as f64,
                volume: 10.0,
                timestamp: 12345,
            });
        }

        let sequences = mgr.append_batch("IF2501", records).unwrap();
        assert_eq!(sequences.len(), 100);
        assert_eq!(sequences[0], 1);
        assert_eq!(sequences[99], 100);
    }

    #[test]
    fn test_replay_per_instrument() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let mgr = PerInstrumentWalManager::new(tmp_dir.path().to_str().unwrap());

        // 写入多个品种的数据
        for i in 0..10 {
            mgr.append("IF2501", WalRecord::OrderInsert {
                order_id: i,
                user_id: [0u8; 32],
                instrument_id: [0u8; 16],
                direction: 0,
                offset: 0,
                price: 4000.0 + i as f64,
                volume: 10.0,
                timestamp: 12345,
            }).unwrap();
        }

        for i in 0..5 {
            mgr.append("IC2501", WalRecord::OrderInsert {
                order_id: i,
                user_id: [0u8; 32],
                instrument_id: [0u8; 16],
                direction: 0,
                offset: 0,
                price: 6000.0 + i as f64,
                volume: 10.0,
                timestamp: 12345,
            }).unwrap();
        }

        // 回放 IF2501
        let mut count_if = 0;
        mgr.replay("IF2501", |_entry| {
            count_if += 1;
            Ok(())
        }).unwrap();
        assert_eq!(count_if, 10);

        // 回放 IC2501
        let mut count_ic = 0;
        mgr.replay("IC2501", |_entry| {
            count_ic += 1;
            Ok(())
        }).unwrap();
        assert_eq!(count_ic, 5);

        // 回放所有品种
        let mut total_count = 0;
        mgr.replay_all(|_inst, _entry| {
            total_count += 1;
            Ok(())
        }).unwrap();
        assert_eq!(total_count, 15);
    }

    #[test]
    fn test_concurrent_writes() {
        use std::thread;

        let tmp_dir = tempfile::tempdir().unwrap();
        let mgr = Arc::new(PerInstrumentWalManager::new(tmp_dir.path().to_str().unwrap()));

        // 并发写入不同品种
        let handles: Vec<_> = (0..4).map(|inst_idx| {
            let mgr_clone = mgr.clone();
            let inst_id = format!("INST{}", inst_idx);

            thread::spawn(move || {
                for i in 0..100 {
                    mgr_clone.append(&inst_id, WalRecord::OrderInsert {
                        order_id: i,
                        user_id: [inst_idx as u8; 32],
                        instrument_id: [0u8; 16],
                        direction: 0,
                        offset: 0,
                        price: 100.0 + i as f64,
                        volume: 10.0,
                        timestamp: 12345,
                    }).unwrap();
                }
            })
        }).collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证每个品种都写入了 100 条记录
        for inst_idx in 0..4 {
            let inst_id = format!("INST{}", inst_idx);
            let seq = mgr.current_sequence(&inst_id).unwrap();
            // sequence 从 1 开始，写了 100 条，所以下一个 sequence 是 101
            assert_eq!(seq, 101);
        }
    }
}
