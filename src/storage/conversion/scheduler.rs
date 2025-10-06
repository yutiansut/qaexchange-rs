// OLTP → OLAP 转换调度器
//
// 职责：
// 1. 定期扫描 OLTP SSTable 目录
// 2. 按品种分组待转换文件
// 3. 提交转换任务到队列
// 4. 检测和恢复僵尸任务

use super::metadata::{ConversionMetadata, ConversionRecord, ConversionStatus};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::HashMap;
use crossbeam::channel::{Sender, Receiver, unbounded};

/// 转换任务
#[derive(Debug, Clone)]
pub struct ConversionTask {
    pub record: ConversionRecord,
}

/// 调度器配置
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 扫描间隔（秒）
    pub scan_interval_secs: u64,

    /// 每批次最少 SSTable 数量
    pub min_sstables_per_batch: usize,

    /// 每批次最多 SSTable 数量
    pub max_sstables_per_batch: usize,

    /// SSTable 最小年龄（秒，避免转换正在写入的文件）
    pub min_sstable_age_secs: u64,

    /// 最大重试次数
    pub max_retries: u32,

    /// 僵尸任务超时（秒，超过此时间仍在 Converting 状态视为僵尸）
    pub zombie_timeout_secs: i64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 300,           // 5 分钟扫描一次
            min_sstables_per_batch: 3,         // 至少 3 个文件才批量转换
            max_sstables_per_batch: 20,        // 最多 20 个文件一批
            min_sstable_age_secs: 60,          // 文件至少 1 分钟未修改
            max_retries: 5,                     // 最多重试 5 次
            zombie_timeout_secs: 3600,          // 1 小时未完成视为僵尸
        }
    }
}

/// 转换调度器
pub struct ConversionScheduler {
    config: SchedulerConfig,
    metadata: Arc<Mutex<ConversionMetadata>>,
    task_sender: Sender<ConversionTask>,
    task_receiver: Receiver<ConversionTask>,
    pub(crate) storage_base_path: PathBuf,
}

impl ConversionScheduler {
    /// 创建新的调度器
    pub fn new(
        config: SchedulerConfig,
        metadata: Arc<Mutex<ConversionMetadata>>,
        storage_base_path: PathBuf,
    ) -> Self {
        let (task_sender, task_receiver) = unbounded();

        Self {
            config,
            metadata,
            task_sender,
            task_receiver,
            storage_base_path,
        }
    }

    /// 获取任务发送端（用于外部提交任务）
    pub fn task_sender(&self) -> Sender<ConversionTask> {
        self.task_sender.clone()
    }

    /// 获取任务接收端（用于 Worker 消费）
    pub fn task_receiver(&self) -> Receiver<ConversionTask> {
        self.task_receiver.clone()
    }

    /// 运行调度器（阻塞，应在独立线程中运行）
    pub fn run(&self) {
        log::info!("Conversion scheduler started with config: {:?}", self.config);

        loop {
            // 1. 扫描并提交新任务
            if let Err(e) = self.scan_and_schedule() {
                log::error!("Scan and schedule failed: {}", e);
            }

            // 2. 恢复可重试任务
            if let Err(e) = self.schedule_retries() {
                log::error!("Schedule retries failed: {}", e);
            }

            // 3. 检测并恢复僵尸任务
            if let Err(e) = self.recover_zombie_tasks() {
                log::error!("Recover zombie tasks failed: {}", e);
            }

            // 4. 清理临时文件
            if let Err(e) = self.cleanup_temp_files() {
                log::error!("Cleanup temp files failed: {}", e);
            }

            // 5. 休眠到下次扫描
            std::thread::sleep(Duration::from_secs(self.config.scan_interval_secs));
        }
    }

    /// 扫描并调度新的转换任务
    fn scan_and_schedule(&self) -> Result<(), String> {
        log::debug!("Scanning for new conversion tasks...");

        // 扫描所有品种目录
        let instruments = self.scan_instruments()?;

        for instrument_id in instruments {
            let oltp_dir = self.storage_base_path
                .join(&instrument_id)
                .join("oltp");

            if !oltp_dir.exists() {
                continue;
            }

            // 扫描该品种的 OLTP SSTable 文件
            let sstables = self.scan_oltp_sstables(&oltp_dir)?;

            if sstables.len() < self.config.min_sstables_per_batch {
                log::debug!(
                    "Instrument {} has only {} SSTables, skipping (min: {})",
                    instrument_id,
                    sstables.len(),
                    self.config.min_sstables_per_batch
                );
                continue;
            }

            // 批量分组
            for batch in sstables.chunks(self.config.max_sstables_per_batch) {
                self.create_conversion_task(&instrument_id, batch.to_vec())?;
            }
        }

        Ok(())
    }

    /// 扫描所有品种目录
    fn scan_instruments(&self) -> Result<Vec<String>, String> {
        let mut instruments = Vec::new();

        if !self.storage_base_path.exists() {
            return Ok(instruments);
        }

        let entries = std::fs::read_dir(&self.storage_base_path)
            .map_err(|e| format!("Read storage dir failed: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(name) = path.file_name() {
                    if let Some(name_str) = name.to_str() {
                        instruments.push(name_str.to_string());
                    }
                }
            }
        }

        Ok(instruments)
    }

    /// 扫描 OLTP SSTable 文件
    fn scan_oltp_sstables(&self, oltp_dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut sstables = Vec::new();
        let now = std::time::SystemTime::now();

        let entries = std::fs::read_dir(oltp_dir)
            .map_err(|e| format!("Read OLTP dir failed: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            // 只处理 .rkyv 文件
            if !path.extension().map(|e| e == "rkyv").unwrap_or(false) {
                continue;
            }

            // 检查文件年龄
            let metadata = std::fs::metadata(&path)
                .map_err(|e| format!("Get metadata failed: {}", e))?;

            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age.as_secs() < self.config.min_sstable_age_secs {
                        log::debug!("SSTable {:?} too young, skipping", path);
                        continue;
                    }
                }
            }

            sstables.push(path);
        }

        // 按文件名排序（时间顺序）
        sstables.sort();

        Ok(sstables)
    }

    /// 创建转换任务
    fn create_conversion_task(
        &self,
        instrument_id: &str,
        sstables: Vec<PathBuf>,
    ) -> Result<(), String> {
        let mut metadata = self.metadata.lock().unwrap();

        // 检查是否已存在待转换任务（避免重复）
        for record in metadata.get_pending_records() {
            if record.instrument_id == instrument_id
                && record.oltp_sstables == sstables
            {
                log::debug!("Task already exists for {:?}, skipping", sstables);
                return Ok(());
            }
        }

        // 生成 OLAP Parquet 文件名
        let olap_dir = self.storage_base_path
            .join(instrument_id)
            .join("olap");

        std::fs::create_dir_all(&olap_dir)
            .map_err(|e| format!("Create OLAP dir failed: {}", e))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let olap_parquet = olap_dir.join(format!("batch_{}.parquet", timestamp));

        // 创建转换记录
        let id = metadata.allocate_id();
        let record = ConversionRecord::new(
            id,
            instrument_id.to_string(),
            sstables.clone(),
            olap_parquet,
        );

        log::info!(
            "Creating conversion task {}: {} SSTables for instrument {}",
            id,
            sstables.len(),
            instrument_id
        );

        // 保存记录
        metadata.add_record(record.clone())?;

        // 提交任务
        self.task_sender
            .send(ConversionTask { record })
            .map_err(|e| format!("Send task failed: {}", e))?;

        Ok(())
    }

    /// 调度可重试任务
    fn schedule_retries(&self) -> Result<(), String> {
        // 1. 先收集需要重试的任务（克隆记录）
        let records_to_retry = {
            let metadata = self.metadata.lock().unwrap();
            let retryable = metadata.get_retryable_records(self.config.max_retries);

            if retryable.is_empty() {
                return Ok(());
            }

            log::info!("Found {} retryable tasks", retryable.len());

            // 过滤需要重试的任务
            retryable
                .iter()
                .filter_map(|record| {
                    // 计算指数退避延迟
                    let delay_secs = 2u64.pow(record.retry_count.min(10));
                    let elapsed = chrono::Utc::now().timestamp() - record.end_time.unwrap_or(0);

                    if elapsed < delay_secs as i64 {
                        log::debug!(
                            "Task {} needs to wait {} more seconds before retry",
                            record.id,
                            delay_secs as i64 - elapsed
                        );
                        None
                    } else {
                        Some((*record).clone())
                    }
                })
                .collect::<Vec<_>>()
        }; // metadata lock 在这里释放

        // 2. 处理每个重试任务
        for record in records_to_retry {
            log::info!(
                "Retrying task {} (attempt {}/{})",
                record.id,
                record.retry_count + 1,
                self.config.max_retries
            );

            // 重新标记为 Pending
            let mut record_clone = record.clone();
            record_clone.status = ConversionStatus::Pending;

            // 更新记录
            {
                let mut metadata = self.metadata.lock().unwrap();
                metadata.update_record(record_clone.clone())?;
            }

            // 提交任务
            self.task_sender
                .send(ConversionTask { record: record_clone })
                .map_err(|e| format!("Send retry task failed: {}", e))?;
        }

        Ok(())
    }

    /// 恢复僵尸任务
    fn recover_zombie_tasks(&self) -> Result<(), String> {
        // 1. 先收集僵尸任务（克隆记录）
        let zombie_records = {
            let metadata = self.metadata.lock().unwrap();
            let converting = metadata.get_converting_records();

            if converting.is_empty() {
                return Ok(());
            }

            let now = chrono::Utc::now().timestamp();

            converting
                .iter()
                .filter_map(|record| {
                    let elapsed = now - record.start_time;

                    if elapsed > self.config.zombie_timeout_secs {
                        log::warn!(
                            "Detected zombie task {} (running for {} seconds), marking as failed",
                            record.id,
                            elapsed
                        );

                        let mut record_clone = (*record).clone();
                        record_clone.mark_failed(format!("Zombie task timeout after {} seconds", elapsed));
                        Some(record_clone)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        }; // metadata lock 在这里释放

        // 2. 更新僵尸任务状态
        for record_clone in zombie_records {
            let mut metadata = self.metadata.lock().unwrap();
            metadata.update_record(record_clone)?;
        }

        Ok(())
    }

    /// 清理临时文件
    fn cleanup_temp_files(&self) -> Result<(), String> {
        let metadata = self.metadata.lock().unwrap();

        for record in &metadata.records {
            let tmp_path = record.temp_file_path();

            if tmp_path.exists() {
                // 只清理失败任务的临时文件
                if record.status == ConversionStatus::Failed {
                    log::info!("Cleaning up temp file: {:?}", tmp_path);
                    if let Err(e) = std::fs::remove_file(&tmp_path) {
                        log::warn!("Failed to remove temp file {:?}: {}", tmp_path, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> HashMap<String, usize> {
        let metadata = self.metadata.lock().unwrap();
        let stats = metadata.get_stats();

        let mut map = HashMap::new();
        map.insert("total".to_string(), stats.total);
        map.insert("pending".to_string(), stats.pending);
        map.insert("converting".to_string(), stats.converting);
        map.insert("success".to_string(), stats.success);
        map.insert("failed".to_string(), stats.failed);
        map.insert("avg_duration_secs".to_string(), stats.avg_duration_secs as usize);

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scan_instruments() {
        let tmp_dir = tempdir().unwrap();
        let storage_path = tmp_dir.path().to_path_buf();

        // 创建品种目录
        std::fs::create_dir_all(storage_path.join("rb2501")).unwrap();
        std::fs::create_dir_all(storage_path.join("rb2502")).unwrap();
        std::fs::create_dir_all(storage_path.join("rb2503")).unwrap();

        let metadata_path = tmp_dir.path().join("metadata.json");
        let metadata = Arc::new(Mutex::new(ConversionMetadata::new(metadata_path)));

        let config = SchedulerConfig::default();
        let scheduler = ConversionScheduler::new(config, metadata, storage_path);

        let instruments = scheduler.scan_instruments().unwrap();
        assert_eq!(instruments.len(), 3);
        assert!(instruments.contains(&"rb2501".to_string()));
    }

    #[test]
    fn test_scan_oltp_sstables() {
        let tmp_dir = tempdir().unwrap();
        let oltp_dir = tmp_dir.path().join("oltp");
        std::fs::create_dir_all(&oltp_dir).unwrap();

        // 创建测试文件
        std::fs::write(oltp_dir.join("sstable_1.rkyv"), b"test").unwrap();
        std::fs::write(oltp_dir.join("sstable_2.rkyv"), b"test").unwrap();
        std::fs::write(oltp_dir.join("other.txt"), b"test").unwrap();

        // 等待文件年龄超过阈值
        std::thread::sleep(Duration::from_secs(1));

        let metadata_path = tmp_dir.path().join("metadata.json");
        let metadata = Arc::new(Mutex::new(ConversionMetadata::new(metadata_path)));

        let mut config = SchedulerConfig::default();
        config.min_sstable_age_secs = 0; // 不检查年龄

        let scheduler = ConversionScheduler::new(config, metadata, tmp_dir.path().to_path_buf());

        let sstables = scheduler.scan_oltp_sstables(&oltp_dir).unwrap();
        assert_eq!(sstables.len(), 2); // 只有 .rkyv 文件
    }
}
