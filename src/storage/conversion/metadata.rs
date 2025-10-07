// OLTP → OLAP 转换元数据
//
// 设计原则：
// 1. 持久化转换状态，支持崩溃恢复
// 2. 记录详细错误信息，便于调试
// 3. 支持重试机制
// 4. 原子性操作：临时文件 + rename

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use chrono::Utc;

/// 转换状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversionStatus {
    /// 待转换
    Pending,
    /// 转换中
    Converting,
    /// 转换成功
    Success,
    /// 转换失败
    Failed,
}

/// 单次转换记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRecord {
    /// 转换 ID（唯一标识）
    pub id: u64,

    /// 品种 ID
    pub instrument_id: String,

    /// 源 OLTP SSTable 文件列表
    pub oltp_sstables: Vec<PathBuf>,

    /// 目标 OLAP Parquet 文件
    pub olap_parquet: PathBuf,

    /// 转换状态
    pub status: ConversionStatus,

    /// 记录数（用于校验）
    pub entry_count: u64,

    /// 时间戳范围（用于校验）
    pub min_timestamp: i64,
    pub max_timestamp: i64,

    /// 开始时间（Unix 时间戳，秒）
    pub start_time: i64,

    /// 结束时间（Unix 时间戳，秒）
    pub end_time: Option<i64>,

    /// 错误信息
    pub error_message: Option<String>,

    /// 重试次数
    pub retry_count: u32,

    /// 创建时间
    pub created_at: i64,
}

impl ConversionRecord {
    /// 创建新的转换记录
    pub fn new(
        id: u64,
        instrument_id: String,
        oltp_sstables: Vec<PathBuf>,
        olap_parquet: PathBuf,
    ) -> Self {
        let now = Utc::now().timestamp();
        Self {
            id,
            instrument_id,
            oltp_sstables,
            olap_parquet,
            status: ConversionStatus::Pending,
            entry_count: 0,
            min_timestamp: 0,
            max_timestamp: 0,
            start_time: now,
            end_time: None,
            error_message: None,
            retry_count: 0,
            created_at: now,
        }
    }

    /// 标记为转换中
    pub fn mark_converting(&mut self) {
        self.status = ConversionStatus::Converting;
        self.start_time = Utc::now().timestamp();
    }

    /// 标记为成功
    pub fn mark_success(&mut self, entry_count: u64, min_ts: i64, max_ts: i64) {
        self.status = ConversionStatus::Success;
        self.entry_count = entry_count;
        self.min_timestamp = min_ts;
        self.max_timestamp = max_ts;
        self.end_time = Some(Utc::now().timestamp());
        self.error_message = None;
    }

    /// 标记为失败
    pub fn mark_failed(&mut self, error: String) {
        self.status = ConversionStatus::Failed;
        self.end_time = Some(Utc::now().timestamp());
        self.error_message = Some(error);
        self.retry_count += 1;
    }

    /// 是否可以重试
    pub fn can_retry(&self, max_retries: u32) -> bool {
        // Only Failed records can be retried
        self.status == ConversionStatus::Failed && self.retry_count < max_retries
    }

    /// 获取临时文件路径
    pub fn temp_file_path(&self) -> PathBuf {
        let mut path = self.olap_parquet.clone();
        path.set_extension("tmp");
        path
    }

    /// 转换耗时（秒）
    pub fn duration_secs(&self) -> Option<i64> {
        self.end_time.map(|end| end - self.start_time)
    }
}

/// 转换元数据管理器
#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionMetadata {
    /// 所有转换记录
    pub records: Vec<ConversionRecord>,

    /// 下一个转换 ID
    pub next_id: u64,

    /// 元数据文件路径
    #[serde(skip)]
    pub metadata_path: PathBuf,
}

impl ConversionMetadata {
    /// 创建新的元数据管理器
    pub fn new(metadata_path: PathBuf) -> Self {
        Self {
            records: Vec::new(),
            next_id: 1,
            metadata_path,
        }
    }

    /// 从磁盘加载元数据
    pub fn load(metadata_path: PathBuf) -> Result<Self, String> {
        if !metadata_path.exists() {
            return Ok(Self::new(metadata_path));
        }

        let mut file = File::open(&metadata_path)
            .map_err(|e| format!("Open metadata file failed: {}", e))?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("Read metadata file failed: {}", e))?;

        let mut metadata: ConversionMetadata = serde_json::from_str(&content)
            .map_err(|e| format!("Parse metadata failed: {}", e))?;

        metadata.metadata_path = metadata_path;
        Ok(metadata)
    }

    /// 保存元数据到磁盘（原子性）
    pub fn save(&self) -> Result<(), String> {
        // 确保目录存在
        if let Some(parent) = self.metadata_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Create metadata dir failed: {}", e))?;
        }

        // 序列化
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialize metadata failed: {}", e))?;

        // 写入临时文件
        let tmp_path = self.metadata_path.with_extension("tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| format!("Create temp metadata file failed: {}", e))?;

        file.write_all(json.as_bytes())
            .map_err(|e| format!("Write metadata failed: {}", e))?;

        file.sync_all()
            .map_err(|e| format!("Sync metadata failed: {}", e))?;

        drop(file);

        // 原子性 rename
        std::fs::rename(&tmp_path, &self.metadata_path)
            .map_err(|e| format!("Rename metadata failed: {}", e))?;

        Ok(())
    }

    /// 添加新的转换记录
    pub fn add_record(&mut self, record: ConversionRecord) -> Result<(), String> {
        self.records.push(record);
        self.save()
    }

    /// 更新转换记录
    pub fn update_record(&mut self, record: ConversionRecord) -> Result<(), String> {
        if let Some(r) = self.records.iter_mut().find(|r| r.id == record.id) {
            *r = record;
            self.save()
        } else {
            Err(format!("Conversion record {} not found", record.id))
        }
    }

    /// 分配新的转换 ID
    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 获取待转换的记录
    pub fn get_pending_records(&self) -> Vec<&ConversionRecord> {
        self.records
            .iter()
            .filter(|r| r.status == ConversionStatus::Pending)
            .collect()
    }

    /// 获取可重试的记录
    pub fn get_retryable_records(&self, max_retries: u32) -> Vec<&ConversionRecord> {
        self.records
            .iter()
            .filter(|r| r.can_retry(max_retries))
            .collect()
    }

    /// 获取转换中的记录（用于检测僵尸任务）
    pub fn get_converting_records(&self) -> Vec<&ConversionRecord> {
        self.records
            .iter()
            .filter(|r| r.status == ConversionStatus::Converting)
            .collect()
    }

    /// 获取成功的记录
    pub fn get_success_records(&self) -> Vec<&ConversionRecord> {
        self.records
            .iter()
            .filter(|r| r.status == ConversionStatus::Success)
            .collect()
    }

    /// 清理旧的成功记录（保留最近 N 条）
    pub fn cleanup_success_records(&mut self, keep_recent: usize) -> Result<(), String> {
        let mut success_records: Vec<_> = self.records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.status == ConversionStatus::Success)
            .collect();

        // 按创建时间排序（最新的在前）
        success_records.sort_by(|(_, a), (_, b)| b.created_at.cmp(&a.created_at));

        // 删除超过保留数量的记录
        if success_records.len() > keep_recent {
            let indices_to_remove: Vec<_> = success_records[keep_recent..]
                .iter()
                .map(|(i, _)| *i)
                .collect();

            // 从后往前删除，避免索引偏移
            for idx in indices_to_remove.iter().rev() {
                self.records.remove(*idx);
            }

            self.save()?;
        }

        Ok(())
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> ConversionStats {
        let mut stats = ConversionStats::default();

        for record in &self.records {
            stats.total += 1;
            match record.status {
                ConversionStatus::Pending => stats.pending += 1,
                ConversionStatus::Converting => stats.converting += 1,
                ConversionStatus::Success => stats.success += 1,
                ConversionStatus::Failed => stats.failed += 1,
            }

            if let Some(duration) = record.duration_secs() {
                stats.total_duration_secs += duration;
            }
        }

        if stats.success > 0 {
            stats.avg_duration_secs = stats.total_duration_secs / stats.success as i64;
        }

        stats
    }
}

/// 转换统计信息
#[derive(Debug, Default, Clone)]
pub struct ConversionStats {
    pub total: usize,
    pub pending: usize,
    pub converting: usize,
    pub success: usize,
    pub failed: usize,
    pub total_duration_secs: i64,
    pub avg_duration_secs: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_conversion_record_lifecycle() {
        let mut record = ConversionRecord::new(
            1,
            "rb2501".to_string(),
            vec![PathBuf::from("/home/quantaxis/qaexchange-rs/output//sstable_1.rkyv")],
            PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_1.parquet"),
        );

        assert_eq!(record.status, ConversionStatus::Pending);
        assert_eq!(record.retry_count, 0);

        record.mark_converting();
        assert_eq!(record.status, ConversionStatus::Converting);

        record.mark_success(100, 1000, 2000);
        assert_eq!(record.status, ConversionStatus::Success);
        assert_eq!(record.entry_count, 100);
        assert_eq!(record.min_timestamp, 1000);
        assert_eq!(record.max_timestamp, 2000);
        assert!(record.end_time.is_some());
        assert!(record.duration_secs().unwrap() >= 0);
    }

    #[test]
    fn test_conversion_record_retry() {
        let mut record = ConversionRecord::new(
            1,
            "rb2501".to_string(),
            vec![PathBuf::from("/home/quantaxis/qaexchange-rs/output//sstable_1.rkyv")],
            PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_1.parquet"),
        );

        // Pending records are not "retryable" - they haven't been tried yet
        assert!(!record.can_retry(5));

        record.mark_failed("Test error".to_string());
        assert_eq!(record.status, ConversionStatus::Failed);
        assert_eq!(record.retry_count, 1);
        assert!(record.can_retry(5));

        // 模拟多次重试
        for _ in 0..4 {
            record.mark_failed("Test error".to_string());
        }

        assert_eq!(record.retry_count, 5);
        assert!(!record.can_retry(5));
    }

    #[test]
    fn test_metadata_persistence() {
        let tmp_dir = tempdir().unwrap();
        let metadata_path = tmp_dir.path().join("conversion_metadata.json");

        // 创建并保存
        {
            let mut metadata = ConversionMetadata::new(metadata_path.clone());

            let record = ConversionRecord::new(
                metadata.allocate_id(),
                "rb2501".to_string(),
                vec![PathBuf::from("/home/quantaxis/qaexchange-rs/output//sstable_1.rkyv")],
                PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_1.parquet"),
            );

            metadata.add_record(record).unwrap();
            assert_eq!(metadata.records.len(), 1);
            assert_eq!(metadata.next_id, 2);
        }

        // 加载并验证
        {
            let metadata = ConversionMetadata::load(metadata_path).unwrap();
            assert_eq!(metadata.records.len(), 1);
            assert_eq!(metadata.next_id, 2);
            assert_eq!(metadata.records[0].instrument_id, "rb2501");
        }
    }

    #[test]
    fn test_metadata_queries() {
        let tmp_dir = tempdir().unwrap();
        let metadata_path = tmp_dir.path().join("conversion_metadata.json");
        let mut metadata = ConversionMetadata::new(metadata_path);

        // 添加不同状态的记录
        let mut record1 = ConversionRecord::new(
            metadata.allocate_id(),
            "rb2501".to_string(),
            vec![],
            PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_1.parquet"),
        );
        record1.mark_success(100, 1000, 2000);
        metadata.add_record(record1).unwrap();

        let record2 = ConversionRecord::new(
            metadata.allocate_id(),
            "rb2502".to_string(),
            vec![],
            PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_2.parquet"),
        );
        metadata.add_record(record2).unwrap();

        let mut record3 = ConversionRecord::new(
            metadata.allocate_id(),
            "rb2503".to_string(),
            vec![],
            PathBuf::from("/home/quantaxis/qaexchange-rs/output//olap_3.parquet"),
        );
        record3.mark_failed("Test error".to_string());
        metadata.add_record(record3).unwrap();

        // 查询
        assert_eq!(metadata.get_pending_records().len(), 1);
        assert_eq!(metadata.get_success_records().len(), 1);
        assert_eq!(metadata.get_retryable_records(5).len(), 1);

        // 统计
        let stats = metadata.get_stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.failed, 1);
    }
}
