// OLTP → OLAP 转换工作线程
//
// 职责：
// 1. 从任务队列消费转换任务
// 2. 批量读取 OLTP SSTable
// 3. 构建 OLAP MemTable 并写入 Parquet
// 4. 校验数据完整性
// 5. 原子性提交（临时文件 + rename）
// 6. 错误恢复和回滚

use super::metadata::{ConversionMetadata, ConversionRecord};
use super::scheduler::ConversionTask;
use crate::storage::memtable::types::MemTableKey;
use crate::storage::memtable::olap::OlapMemTable;
use crate::storage::sstable::olap_parquet::ParquetSSTableWriter;
use crate::storage::sstable::oltp_rkyv::RkyvSSTable;
use crate::storage::wal::record::WalRecord;
use crossbeam::channel::Receiver;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// Worker 配置
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Worker 数量
    pub worker_count: usize,

    /// 批量读取大小（每次从 SSTable 读取多少记录）
    pub batch_read_size: usize,

    /// 是否在转换成功后删除源文件
    pub delete_source_after_success: bool,

    /// 源文件保留时间（秒，0 表示立即删除）
    pub source_retention_secs: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_count: 4,                    // 4 个 worker
            batch_read_size: 10000,             // 每批 10K 记录
            delete_source_after_success: true,  // 转换成功后删除源文件
            source_retention_secs: 3600,        // 保留 1 小时
        }
    }
}

/// 转换工作线程
pub struct ConversionWorker {
    worker_id: usize,
    config: WorkerConfig,
    metadata: Arc<Mutex<ConversionMetadata>>,
    task_receiver: Receiver<ConversionTask>,
    shutdown: Arc<AtomicBool>,
}

impl ConversionWorker {
    /// 创建新的 Worker
    pub fn new(
        worker_id: usize,
        config: WorkerConfig,
        metadata: Arc<Mutex<ConversionMetadata>>,
        task_receiver: Receiver<ConversionTask>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            worker_id,
            config,
            metadata,
            task_receiver,
            shutdown,
        }
    }

    /// 运行 Worker（阻塞，应在独立线程中运行）
    pub fn run(&self) {
        log::info!("Conversion worker {} started", self.worker_id);

        while !self.shutdown.load(Ordering::Relaxed) {
            match self.task_receiver.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(task) => {
                    if let Err(e) = self.process_task(task) {
                        log::error!("Worker {} failed to process task: {}", self.worker_id, e);
                    }
                }
                Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                    // 超时，继续等待
                    continue;
                }
                Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                    log::info!("Worker {} channel disconnected, exiting", self.worker_id);
                    break;
                }
            }
        }

        log::info!("Conversion worker {} stopped", self.worker_id);
    }

    /// 处理转换任务
    fn process_task(&self, task: ConversionTask) -> Result<(), String> {
        let mut record = task.record;

        log::info!(
            "Worker {} processing conversion task {}: instrument={}, {} SSTables",
            self.worker_id,
            record.id,
            record.instrument_id,
            record.oltp_sstables.len()
        );

        // 1. 更新状态为 Converting
        record.mark_converting();
        {
            let mut metadata = self.metadata.lock().unwrap();
            metadata.update_record(record.clone())?;
        }

        // 2. 执行转换
        match self.do_conversion(&record) {
            Ok((entry_count, min_ts, max_ts)) => {
                // 3. 转换成功
                log::info!(
                    "Worker {} completed conversion task {}: {} entries, time range [{}, {}]",
                    self.worker_id,
                    record.id,
                    entry_count,
                    min_ts,
                    max_ts
                );

                record.mark_success(entry_count, min_ts, max_ts);

                let mut metadata = self.metadata.lock().unwrap();
                metadata.update_record(record.clone())?;

                // 4. 归档源文件
                if self.config.delete_source_after_success {
                    drop(metadata); // 释放锁
                    self.archive_source_files(&record)?;
                }

                Ok(())
            }
            Err(e) => {
                // 5. 转换失败
                log::error!(
                    "Worker {} failed conversion task {}: {}",
                    self.worker_id,
                    record.id,
                    e
                );

                record.mark_failed(e.clone());

                let mut metadata = self.metadata.lock().unwrap();
                metadata.update_record(record.clone())?;

                // 6. 清理临时文件
                let tmp_path = record.temp_file_path();
                if tmp_path.exists() {
                    if let Err(e) = std::fs::remove_file(&tmp_path) {
                        log::warn!("Failed to remove temp file {:?}: {}", tmp_path, e);
                    }
                }

                Err(e)
            }
        }
    }

    /// 执行转换（核心逻辑）
    fn do_conversion(&self, record: &ConversionRecord) -> Result<(u64, i64, i64), String> {
        log::debug!("Worker {} reading OLTP SSTables...", self.worker_id);

        // 1. 批量读取所有 OLTP SSTable
        let all_records = self.read_all_sstables(record)?;

        if all_records.is_empty() {
            return Err("No records found in OLTP SSTables".to_string());
        }

        log::debug!(
            "Worker {} read {} records from {} SSTables",
            self.worker_id,
            all_records.len(),
            record.oltp_sstables.len()
        );

        // 2. 构建 OLAP MemTable
        log::debug!("Worker {} building OLAP MemTable...", self.worker_id);
        let olap_memtable = OlapMemTable::from_records(all_records.clone());

        // 3. 写入 Parquet（临时文件）
        let tmp_path = record.temp_file_path();
        log::debug!("Worker {} writing Parquet to {:?}...", self.worker_id, tmp_path);

        let schema = Arc::new(crate::storage::memtable::olap::create_olap_schema());
        let mut writer = ParquetSSTableWriter::create(&tmp_path, schema)?;

        writer.write_chunk(olap_memtable.chunk())?;

        let metadata = writer.finish()?;

        log::debug!(
            "Worker {} wrote {} entries to Parquet",
            self.worker_id,
            metadata.entry_count
        );

        // 4. 校验数据完整性
        if metadata.entry_count != all_records.len() as u64 {
            return Err(format!(
                "Entry count mismatch: expected {}, got {}",
                all_records.len(),
                metadata.entry_count
            ));
        }

        // 5. 原子性 rename
        log::debug!(
            "Worker {} renaming {:?} to {:?}...",
            self.worker_id,
            tmp_path,
            record.olap_parquet
        );

        std::fs::rename(&tmp_path, &record.olap_parquet)
            .map_err(|e| format!("Rename parquet file failed: {}", e))?;

        Ok((
            metadata.entry_count,
            metadata.min_timestamp,
            metadata.max_timestamp,
        ))
    }

    /// 批量读取所有 OLTP SSTable
    fn read_all_sstables(
        &self,
        record: &ConversionRecord,
    ) -> Result<Vec<(MemTableKey, WalRecord)>, String> {
        let mut all_records = Vec::new();

        for sstable_path in &record.oltp_sstables {
            log::debug!("Worker {} reading {:?}...", self.worker_id, sstable_path);

            // 打开 SSTable
            let sstable = RkyvSSTable::open(sstable_path)?;

            // 全量扫描 (使用最大范围)
            let records = sstable.range_query(i64::MIN, i64::MAX)?;

            // 转换格式：(i64, u64, WalRecord) → (MemTableKey, WalRecord)
            for (timestamp, sequence, record) in records {
                let key = MemTableKey { timestamp, sequence };
                all_records.push((key, record));
            }

            log::debug!(
                "Worker {} read {} records from {:?}",
                self.worker_id,
                all_records.len(),
                sstable_path
            );
        }

        // 按时间戳排序（保证顺序）
        all_records.sort_by_key(|(key, _)| (key.timestamp, key.sequence));

        Ok(all_records)
    }

    /// 归档源文件（延迟删除或移动到备份目录）
    fn archive_source_files(&self, record: &ConversionRecord) -> Result<(), String> {
        if self.config.source_retention_secs == 0 {
            // 立即删除
            for sstable_path in &record.oltp_sstables {
                log::info!("Worker {} deleting source file {:?}", self.worker_id, sstable_path);
                if let Err(e) = std::fs::remove_file(sstable_path) {
                    log::warn!("Failed to remove source file {:?}: {}", sstable_path, e);
                }
            }
        } else {
            // 延迟删除：标记文件为 .archived，后台定期清理
            for sstable_path in &record.oltp_sstables {
                let archived_path = sstable_path.with_extension("archived");
                log::info!(
                    "Worker {} marking source file {:?} as archived",
                    self.worker_id,
                    sstable_path
                );

                if let Err(e) = std::fs::rename(sstable_path, &archived_path) {
                    log::warn!(
                        "Failed to mark source file {:?} as archived: {}",
                        sstable_path,
                        e
                    );
                }
            }
        }

        Ok(())
    }
}

/// Worker 线程池
pub struct WorkerPool {
    config: WorkerConfig,
    metadata: Arc<Mutex<ConversionMetadata>>,
    task_receiver: Receiver<ConversionTask>,
    shutdown: Arc<AtomicBool>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl WorkerPool {
    /// 创建 Worker 线程池
    pub fn new(
        config: WorkerConfig,
        metadata: Arc<Mutex<ConversionMetadata>>,
        task_receiver: Receiver<ConversionTask>,
    ) -> Self {
        Self {
            config,
            metadata,
            task_receiver,
            shutdown: Arc::new(AtomicBool::new(false)),
            handles: Vec::new(),
        }
    }

    /// 启动所有 Worker
    pub fn start(&mut self) {
        log::info!("Starting {} workers...", self.config.worker_count);

        for worker_id in 0..self.config.worker_count {
            let worker = ConversionWorker::new(
                worker_id,
                self.config.clone(),
                self.metadata.clone(),
                self.task_receiver.clone(),
                self.shutdown.clone(),
            );

            let handle = std::thread::Builder::new()
                .name(format!("conversion-worker-{}", worker_id))
                .spawn(move || {
                    worker.run();
                })
                .expect("Failed to spawn worker thread");

            self.handles.push(handle);
        }
    }

    /// 停止所有 Worker（阻塞直到所有线程退出）
    pub fn stop(self) {
        log::info!("Stopping worker pool...");

        self.shutdown.store(true, Ordering::Relaxed);

        for handle in self.handles {
            if let Err(e) = handle.join() {
                log::error!("Worker thread panicked: {:?}", e);
            }
        }

        log::info!("Worker pool stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memtable::oltp::OltpMemTable;
    use crate::storage::wal::record::WalRecord;
    use tempfile::tempdir;

    fn create_test_sstable(path: &std::path::Path, count: usize, timestamp_offset: i64) -> Result<(), String> {
        use crate::storage::sstable::oltp_rkyv::RkyvSSTableWriter;

        // 创建 SSTable Writer
        let mut writer = RkyvSSTableWriter::create(path)?;

        for i in 0..count {
            let key = MemTableKey {
                timestamp: 1000 + timestamp_offset + i as i64,
                sequence: i as u64,
            };

            let record = WalRecord::OrderInsert {
                order_id: i as u64,
                user_id: [1u8; 32],
                instrument_id: [2u8; 16],
                direction: 0,
                offset: 0,
                price: 100.0 + i as f64,
                volume: 10.0,
                timestamp: key.timestamp,
            };

            // 直接写入 SSTable
            use crate::storage::memtable::types::MemTableValue;
            writer.append(key, MemTableValue::new(record))?;
        }

        // 完成写入
        writer.finish()?;

        Ok(())
    }

    #[test]
    fn test_worker_conversion() {
        use crate::storage::conversion::metadata::ConversionMetadata;

        let tmp_dir = tempdir().unwrap();

        // 创建测试 SSTable
        let sstable1_path = tmp_dir.path().join("sstable_1.rkyv");
        let sstable2_path = tmp_dir.path().join("sstable_2.rkyv");

        create_test_sstable(&sstable1_path, 50, 0).unwrap();   // timestamps 1000-1049
        create_test_sstable(&sstable2_path, 50, 50).unwrap();  // timestamps 1050-1099

        // 创建转换记录
        let metadata_path = tmp_dir.path().join("metadata.json");
        let metadata = Arc::new(Mutex::new(ConversionMetadata::new(metadata_path)));

        let record = ConversionRecord::new(
            1,
            "rb2501".to_string(),
            vec![sstable1_path, sstable2_path],
            tmp_dir.path().join("output.parquet"),
        );

        // 模拟转换
        let config = WorkerConfig {
            worker_count: 1,
            batch_read_size: 10000,
            delete_source_after_success: false, // 测试中不删除
            source_retention_secs: 0,
        };

        let worker = ConversionWorker {
            worker_id: 0,
            config,
            metadata: metadata.clone(),
            task_receiver: crossbeam::channel::unbounded().1, // 不使用
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        let result = worker.do_conversion(&record);
        assert!(result.is_ok());

        let (entry_count, min_ts, max_ts) = result.unwrap();
        assert_eq!(entry_count, 100); // 2 * 50
        assert_eq!(min_ts, 1000);
        assert_eq!(max_ts, 1099);

        // 验证 Parquet 文件存在
        assert!(record.olap_parquet.exists());
    }
}
