// WAL Manager - 高性能 Write-Ahead Log 管理器
//
// 性能目标:
// - 单条写入: P99 < 1ms
// - 批量写入: > 100K entries/s
// - 恢复速度: > 1GB/s
// - 组提交延迟: < 5ms
//
// @author @yutiansut @quantaxis

use super::record::{WalEntry, WalRecord};
use parking_lot::Mutex;
use rkyv::Deserialize;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// WAL 文件 Header
#[derive(Debug, Clone)]
struct WalFileHeader {
    magic: [u8; 8], // "QAXWAL01"
    version: u32,
    start_sequence: u64,
    timestamp: i64,
    _reserved: [u8; 100], // 保留字段，总共 128 字节
}

impl WalFileHeader {
    fn new(start_sequence: u64) -> Self {
        Self {
            magic: *b"QAXWAL01",
            version: 1,
            start_sequence,
            timestamp: chrono::Utc::now().timestamp(),
            _reserved: [0u8; 100],
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.start_sequence.to_le_bytes());
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes.extend_from_slice(&self._reserved);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 128 {
            return Err("Invalid header size".to_string());
        }

        let mut magic = [0u8; 8];
        magic.copy_from_slice(&bytes[0..8]);

        if &magic != b"QAXWAL01" {
            return Err("Invalid magic".to_string());
        }

        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let start_sequence = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let timestamp = i64::from_le_bytes(bytes[20..28].try_into().unwrap());

        let mut reserved = [0u8; 100];
        reserved.copy_from_slice(&bytes[28..128]);

        Ok(Self {
            magic,
            version,
            start_sequence,
            timestamp,
            _reserved: reserved,
        })
    }
}

/// WAL 统计信息
#[derive(Debug, Default)]
pub struct WalStats {
    /// 写入计数
    pub write_count: AtomicU64,
    /// 写入字节数
    pub write_bytes: AtomicU64,
    /// fsync 次数
    pub sync_count: AtomicU64,
    /// 组提交次数
    pub group_commit_count: AtomicU64,
    /// 组提交平均大小
    pub group_commit_total_size: AtomicU64,
    /// 写入总耗时 (微秒)
    pub total_write_time_us: AtomicU64,
}

impl WalStats {
    pub fn avg_group_commit_size(&self) -> f64 {
        let count = self.group_commit_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            self.group_commit_total_size.load(Ordering::Relaxed) as f64 / count as f64
        }
    }

    pub fn avg_write_latency_us(&self) -> f64 {
        let count = self.write_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            self.total_write_time_us.load(Ordering::Relaxed) as f64 / count as f64
        }
    }
}

/// 预序列化的 WAL 条目
struct PreSerializedEntry {
    sequence: u64,
    bytes: Vec<u8>,
}

/// 组提交配置
#[derive(Clone)]
pub struct GroupCommitConfig {
    /// 最大等待时间 (毫秒)
    pub max_wait_ms: u64,
    /// 最大批量大小
    pub max_batch_size: usize,
    /// 是否启用组提交
    pub enabled: bool,
}

impl Default for GroupCommitConfig {
    fn default() -> Self {
        Self {
            max_wait_ms: 5,       // 5ms 最大等待
            max_batch_size: 1000, // 最多积累 1000 条
            enabled: true,
        }
    }
}

/// WAL Manager
pub struct WalManager {
    current_file: Arc<Mutex<BufWriter<File>>>,
    current_sequence: Arc<AtomicU64>,
    base_path: String,
    max_file_size: u64, // 单个 WAL 文件最大 1GB
    current_file_path: Arc<Mutex<String>>,
    current_file_size: Arc<AtomicU64>,
    /// 统计信息
    stats: Arc<WalStats>,
    /// 组提交配置
    group_commit_config: GroupCommitConfig,
    /// 组提交缓冲区
    group_commit_buffer: Arc<Mutex<VecDeque<PreSerializedEntry>>>,
    /// 最后刷新时间
    last_flush_time: Arc<Mutex<Instant>>,
}

impl WalManager {
    /// 创建新的 WAL Manager
    pub fn new(base_path: &str) -> Self {
        std::fs::create_dir_all(base_path).unwrap();

        // 检查是否已有 WAL 文件
        let existing_files = Self::list_wal_files_static(base_path).unwrap_or_default();

        if !existing_files.is_empty() {
            // 如果已有 WAL 文件，从最后一个文件继续
            return Self::open_existing(base_path, existing_files);
        }

        // 否则创建新文件
        let sequence = 1u64;
        let file_path = format!("{}/wal_{:020}.log", base_path, sequence);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .unwrap();

        // 写入 Header
        let header = WalFileHeader::new(sequence);
        file.write_all(&header.to_bytes()).unwrap();
        file.sync_all().unwrap();

        let current_size = file.metadata().unwrap().len();

        Self {
            current_file: Arc::new(Mutex::new(BufWriter::new(file))),
            current_sequence: Arc::new(AtomicU64::new(sequence)),
            base_path: base_path.to_string(),
            max_file_size: 1_000_000_000, // 1GB
            current_file_path: Arc::new(Mutex::new(file_path)),
            current_file_size: Arc::new(AtomicU64::new(current_size)),
            stats: Arc::new(WalStats::default()),
            group_commit_config: GroupCommitConfig::default(),
            group_commit_buffer: Arc::new(Mutex::new(VecDeque::new())),
            last_flush_time: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// 使用自定义配置创建 WAL Manager
    pub fn with_config(base_path: &str, config: GroupCommitConfig) -> Self {
        let mut manager = Self::new(base_path);
        manager.group_commit_config = config;
        manager
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> &WalStats {
        &self.stats
    }

    /// 打开已存在的 WAL（用于恢复）
    fn open_existing(base_path: &str, existing_files: Vec<String>) -> Self {
        // 找到最新的文件
        let latest_file = existing_files.last().unwrap();

        // 读取最后一个文件以获取最高序列号
        let mut max_sequence = 1u64;

        // 遍历所有文件找到最大序列号
        for file_path in &existing_files {
            match File::open(file_path) {
                Ok(mut f) => {
                    let mut header_buf = vec![0u8; 128];
                    if f.read_exact(&mut header_buf).is_ok() {
                        if let Ok(_header) = WalFileHeader::from_bytes(&header_buf) {
                            let mut reader = BufReader::new(f);
                            loop {
                                let mut len_buf = [0u8; 4];
                                if reader.read_exact(&mut len_buf).is_err() {
                                    break;
                                }
                                let length = u32::from_le_bytes(len_buf) as usize;
                                let mut entry_buf = vec![0u8; length];
                                if reader.read_exact(&mut entry_buf).is_err() {
                                    break;
                                }
                                if let Ok(archived) = WalEntry::from_bytes(&entry_buf) {
                                    max_sequence = max_sequence.max(archived.sequence);
                                }
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        // 打开最新文件用于追加
        let file = OpenOptions::new().append(true).open(latest_file).unwrap();

        let current_size = file.metadata().unwrap().len();

        Self {
            current_file: Arc::new(Mutex::new(BufWriter::new(file))),
            current_sequence: Arc::new(AtomicU64::new(max_sequence + 1)),
            base_path: base_path.to_string(),
            max_file_size: 1_000_000_000,
            current_file_path: Arc::new(Mutex::new(latest_file.clone())),
            current_file_size: Arc::new(AtomicU64::new(current_size)),
            stats: Arc::new(WalStats::default()),
            group_commit_config: GroupCommitConfig::default(),
            group_commit_buffer: Arc::new(Mutex::new(VecDeque::new())),
            last_flush_time: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Static version of list_wal_files (for use in new())
    fn list_wal_files_static(base_path: &str) -> Result<Vec<String>, String> {
        let mut files = Vec::new();

        if !Path::new(base_path).exists() {
            return Ok(files);
        }

        for entry in std::fs::read_dir(base_path).map_err(|e| format!("Read dir failed: {}", e))? {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                files.push(path.to_str().unwrap().to_string());
            }
        }

        files.sort();
        Ok(files)
    }

    /// 获取当前序列号
    pub fn get_current_sequence(&self) -> u64 {
        self.current_sequence.load(Ordering::SeqCst)
    }

    /// 追加 WAL 记录（同步写入，确保持久化）
    pub fn append(&self, record: WalRecord) -> Result<u64, String> {
        let start = Instant::now();
        let sequence = self.current_sequence.fetch_add(1, Ordering::SeqCst);

        let entry = WalEntry::new(sequence, record).with_crc32();

        let bytes = entry.to_bytes()?;
        let length = bytes.len() as u32;

        // 检查文件大小，是否需要滚动
        let current_size = self.current_file_size.load(Ordering::Relaxed);
        let new_size = current_size + (4 + length) as u64;
        if new_size > self.max_file_size {
            self.rotate_file()?;
        }
        self.current_file_size
            .fetch_add((4 + length) as u64, Ordering::Relaxed);

        {
            let mut file = self.current_file.lock();

            // 写入长度前缀 (4 bytes)
            file.write_all(&length.to_le_bytes())
                .map_err(|e| format!("WAL write failed: {}", e))?;

            // 写入数据
            file.write_all(&bytes)
                .map_err(|e| format!("WAL write failed: {}", e))?;

            // fsync 确保持久化（P99 < 1ms）
            file.flush()
                .map_err(|e| format!("WAL flush failed: {}", e))?;

            file.get_mut()
                .sync_all()
                .map_err(|e| format!("WAL sync failed: {}", e))?;
        }

        // 更新统计
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.stats.write_count.fetch_add(1, Ordering::Relaxed);
        self.stats.write_bytes.fetch_add(length as u64, Ordering::Relaxed);
        self.stats.sync_count.fetch_add(1, Ordering::Relaxed);
        self.stats.total_write_time_us.fetch_add(elapsed_us, Ordering::Relaxed);

        Ok(sequence)
    }

    /// 异步追加（组提交模式）
    ///
    /// 将记录添加到组提交缓冲区，返回序列号。
    /// 实际 fsync 延迟到缓冲区满或超时。
    pub fn append_async(&self, record: WalRecord) -> Result<u64, String> {
        if !self.group_commit_config.enabled {
            // 回退到同步写入以保证持久化
            return self.append(record);
        }

        let sequence = self.current_sequence.fetch_add(1, Ordering::SeqCst);
        let entry = WalEntry::new(sequence, record).with_crc32();
        let bytes = entry.to_bytes()?;

        // 预序列化后加入缓冲区
        {
            let mut buffer = self.group_commit_buffer.lock();
            buffer.push_back(PreSerializedEntry { sequence, bytes });
        }

        // 检查是否需要刷新
        let should_flush = {
            let buffer = self.group_commit_buffer.lock();
            let last_flush = self.last_flush_time.lock();
            buffer.len() >= self.group_commit_config.max_batch_size
                || last_flush.elapsed().as_millis() as u64 >= self.group_commit_config.max_wait_ms
        };

        if should_flush {
            self.flush_group_commit()?;
        }

        Ok(sequence)
    }

    /// 刷新组提交缓冲区
    pub fn flush_group_commit(&self) -> Result<usize, String> {
        let entries: Vec<PreSerializedEntry> = {
            let mut buffer = self.group_commit_buffer.lock();
            let entries: Vec<_> = buffer.drain(..).collect();
            entries
        };

        if entries.is_empty() {
            return Ok(0);
        }

        let start = Instant::now();
        let count = entries.len();
        let batch_bytes: u64 = entries
            .iter()
            .map(|entry| (4 + entry.bytes.len() as u32) as u64)
            .sum();
        let mut total_bytes = 0u64;

        // 检查文件大小，必要时滚动
        let current_size = self.current_file_size.load(Ordering::Relaxed);
        if current_size + batch_bytes > self.max_file_size {
            self.rotate_file()?;
        }

        {
            let mut file = self.current_file.lock();

            for entry in &entries {
                let length = entry.bytes.len() as u32;

                // 写入长度前缀
                file.write_all(&length.to_le_bytes())
                    .map_err(|e| format!("WAL group write failed: {}", e))?;

                // 写入数据
                file.write_all(&entry.bytes)
                    .map_err(|e| format!("WAL group write failed: {}", e))?;

                total_bytes += (4 + length) as u64;
            }

            // 单次 fsync
            file.flush()
                .map_err(|e| format!("WAL group flush failed: {}", e))?;
            file.get_mut()
                .sync_all()
                .map_err(|e| format!("WAL group sync failed: {}", e))?;
        }

        // 更新文件大小
        self.current_file_size.fetch_add(total_bytes, Ordering::Relaxed);

        // 更新统计
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.stats.write_count.fetch_add(count as u64, Ordering::Relaxed);
        self.stats.write_bytes.fetch_add(total_bytes, Ordering::Relaxed);
        self.stats.sync_count.fetch_add(1, Ordering::Relaxed);
        self.stats.group_commit_count.fetch_add(1, Ordering::Relaxed);
        self.stats.group_commit_total_size.fetch_add(count as u64, Ordering::Relaxed);
        self.stats.total_write_time_us.fetch_add(elapsed_us, Ordering::Relaxed);

        // 更新最后刷新时间
        *self.last_flush_time.lock() = Instant::now();

        log::debug!(
            "WAL group commit: {} entries, {} bytes, {} us",
            count,
            total_bytes,
            elapsed_us
        );

        Ok(count)
    }

    /// 异步批量追加（高吞吐场景）
    ///
    /// 优化点：
    /// - 预序列化所有记录（减少锁持有时间）
    /// - 单次 fsync（减少磁盘同步开销）
    /// - 统计追踪（监控性能）
    pub fn append_batch(&self, records: Vec<WalRecord>) -> Result<Vec<u64>, String> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        let start = Instant::now();
        let count = records.len();

        // Phase 1: 预序列化（无锁）
        let mut pre_serialized = Vec::with_capacity(count);
        for record in records {
            let sequence = self.current_sequence.fetch_add(1, Ordering::SeqCst);
            let entry = WalEntry::new(sequence, record).with_crc32();
            let bytes = entry.to_bytes()?;
            pre_serialized.push((sequence, bytes));
        }

        // Phase 2: 批量写入（持锁）
        let mut total_bytes = 0u64;
        let sequences: Vec<u64> = pre_serialized.iter().map(|(s, _)| *s).collect();

        // 预估批量大小，必要时提前滚动文件
        let batch_bytes: u64 = pre_serialized
            .iter()
            .map(|(_, bytes)| (4 + bytes.len() as u32) as u64)
            .sum();
        let current_size = self.current_file_size.load(Ordering::Relaxed);
        if current_size + batch_bytes > self.max_file_size {
            self.rotate_file()?;
        }

        {
            let mut file = self.current_file.lock();

            for (_sequence, bytes) in &pre_serialized {
                let length = bytes.len() as u32;

                // 写入长度前缀
                file.write_all(&length.to_le_bytes())
                    .map_err(|e| format!("WAL batch write failed: {}", e))?;

                // 写入数据
                file.write_all(bytes)
                    .map_err(|e| format!("WAL batch write failed: {}", e))?;

                total_bytes += (4 + length) as u64;
            }

            // 单次 fsync
            file.flush()
                .map_err(|e| format!("WAL batch flush failed: {}", e))?;
            file.get_mut()
                .sync_all()
                .map_err(|e| format!("WAL batch sync failed: {}", e))?;
        }

        // 更新文件大小
        self.current_file_size.fetch_add(total_bytes, Ordering::Relaxed);

        // 更新统计
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.stats.write_count.fetch_add(count as u64, Ordering::Relaxed);
        self.stats.write_bytes.fetch_add(total_bytes, Ordering::Relaxed);
        self.stats.sync_count.fetch_add(1, Ordering::Relaxed);
        self.stats.group_commit_count.fetch_add(1, Ordering::Relaxed);
        self.stats.group_commit_total_size.fetch_add(count as u64, Ordering::Relaxed);
        self.stats.total_write_time_us.fetch_add(elapsed_us, Ordering::Relaxed);

        log::debug!(
            "WAL batch write: {} records, {} bytes, {} us",
            count,
            total_bytes,
            elapsed_us
        );

        Ok(sequences)
    }

    /// 回放 WAL（崩溃恢复）
    pub fn replay<F>(&self, mut callback: F) -> Result<(), String>
    where
        F: FnMut(WalEntry) -> Result<(), String>,
    {
        let files = self.list_wal_files()?;

        for file_path in files {
            let mut file = File::open(&file_path).map_err(|e| format!("Open WAL failed: {}", e))?;

            // 读取 Header (128 bytes)
            let mut header_buf = vec![0u8; 128];
            file.read_exact(&mut header_buf)
                .map_err(|e| format!("Read WAL header failed: {}", e))?;

            let _header = WalFileHeader::from_bytes(&header_buf)?;

            // 读取条目
            let mut reader = BufReader::new(file);

            loop {
                // 读取长度前缀
                let mut len_buf = [0u8; 4];
                match reader.read_exact(&mut len_buf) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(format!("Read length failed: {}", e)),
                }

                let length = u32::from_le_bytes(len_buf) as usize;

                // 读取条目数据
                let mut entry_buf = vec![0u8; length];
                reader
                    .read_exact(&mut entry_buf)
                    .map_err(|e| format!("Read entry data failed: {}", e))?;

                // 反序列化（零拷贝）
                let archived = WalEntry::from_bytes(&entry_buf)?;

                // 转换为 owned
                let entry: WalEntry = archived
                    .deserialize(&mut rkyv::Infallible)
                    .map_err(|e| format!("Deserialize failed: {:?}", e))?;

                // 验证 CRC32
                if !entry.verify_crc32() {
                    log::error!("CRC32 mismatch for sequence {}", entry.sequence);
                    continue;
                }

                callback(entry)?;
            }
        }

        Ok(())
    }

    /// Checkpoint：截断旧 WAL 文件
    pub fn checkpoint(&self, sequence: u64) -> Result<(), String> {
        let files = self.list_wal_files()?;

        for file_path in files {
            if self.should_truncate(&file_path, sequence)? {
                std::fs::remove_file(&file_path)
                    .map_err(|e| format!("Truncate WAL failed: {}", e))?;

                log::info!("Removed old WAL: {}", file_path);
            }
        }

        Ok(())
    }

    /// 滚动到新文件
    fn rotate_file(&self) -> Result<(), String> {
        let new_sequence = self.current_sequence.load(Ordering::SeqCst);
        let new_file_path = format!("{}/wal_{:020}.log", self.base_path, new_sequence);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_file_path)
            .map_err(|e| format!("Rotate file failed: {}", e))?;

        // 写入 Header
        let header = WalFileHeader::new(new_sequence);
        file.write_all(&header.to_bytes())
            .map_err(|e| format!("Write header failed: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Sync header failed: {}", e))?;

        // 替换当前文件
        *self.current_file.lock() = BufWriter::new(file);
        *self.current_file_path.lock() = new_file_path.clone();
        self.current_file_size.store(128, Ordering::Relaxed); // Header size

        log::info!("Rotated to new WAL file: {}", new_file_path);

        Ok(())
    }

    fn list_wal_files(&self) -> Result<Vec<String>, String> {
        let mut files = Vec::new();

        for entry in
            std::fs::read_dir(&self.base_path).map_err(|e| format!("Read dir failed: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                files.push(path.to_str().unwrap().to_string());
            }
        }

        files.sort();
        Ok(files)
    }

    fn should_truncate(&self, file_path: &str, checkpoint_seq: u64) -> Result<bool, String> {
        // 打开文件读取 header
        let mut file = File::open(file_path)
            .map_err(|e| format!("Open file for truncate check failed: {}", e))?;
        let mut header_buf = vec![0u8; 128];
        file.read_exact(&mut header_buf)
            .map_err(|e| format!("Read header for truncate check failed: {}", e))?;

        let header = WalFileHeader::from_bytes(&header_buf)?;

        // 如果文件的起始序列号小于 checkpoint，则可以删除
        Ok(header.start_sequence < checkpoint_seq)
    }
}

impl Drop for WalManager {
    fn drop(&mut self) {
        if self.group_commit_config.enabled {
            if let Err(e) = self.flush_group_commit() {
                log::error!("Flush WAL group commit on drop failed: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_manager_append() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        let record = WalRecord::OrderInsert {
            order_id: 1,
            user_id: [2u8; 32],
            instrument_id: [3u8; 16],
            direction: 0,
            offset: 0,
            price: 100.0,
            volume: 10.0,
            timestamp: 12345,
        };

        let sequence = wal.append(record).unwrap();
        assert_eq!(sequence, 1);
    }

    #[test]
    fn test_wal_batch_append() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        let mut records = Vec::new();
        for i in 0..100 {
            records.push(WalRecord::TradeExecuted {
                trade_id: i as u64,
                order_id: i as u64,
                exchange_order_id: i as u64,
                price: 100.0 + i as f64,
                volume: 10.0,
                timestamp: 12345,
            });
        }

        let sequences = wal.append_batch(records).unwrap();
        assert_eq!(sequences.len(), 100);
        assert_eq!(sequences[0], 1);
        assert_eq!(sequences[99], 100);
    }

    #[test]
    fn test_wal_replay() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        // 写入 10 条记录
        for i in 0..10 {
            let record = WalRecord::OrderInsert {
                order_id: i as u64,
                user_id: [0u8; 32],
                instrument_id: [0u8; 16],
                direction: 0,
                offset: 0,
                price: 100.0 + i as f64,
                volume: 10.0,
                timestamp: 12345,
            };

            wal.append(record).unwrap();
        }

        // 回放
        let mut count = 0;
        wal.replay(|entry| {
            count += 1;
            assert!(entry.verify_crc32());
            Ok(())
        })
        .unwrap();

        assert_eq!(count, 10);
    }

    #[test]
    fn test_checkpoint() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        // 写入一些记录
        for i in 0..10 {
            wal.append(WalRecord::Checkpoint {
                sequence: i,
                timestamp: 12345,
            })
            .unwrap();
        }

        // 验证文件存在
        let files_before = wal.list_wal_files().unwrap();
        assert_eq!(files_before.len(), 1);

        // Checkpoint 到 sequence 0（不应该删除任何文件，因为 start_sequence=1 >= 0 是 false）
        wal.checkpoint(0).unwrap();
        let files_after = wal.list_wal_files().unwrap();
        assert_eq!(files_after.len(), 1);

        // Checkpoint 到 sequence 2（start_sequence=1 < 2，文件应该被删除）
        wal.checkpoint(2).unwrap();
        let files_deleted = wal.list_wal_files().unwrap();
        assert_eq!(files_deleted.len(), 0);
    }

    #[test]
    fn test_wal_performance() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        // 测试单次写入性能
        // 注意：P99 受 fsync 性能限制，在 SSD 上通常 < 1ms，在 HDD 或 VM 上可能达到 10-30ms
        // 生产环境优化：使用 group commit 批量 fsync 可以达到 P99 < 1ms
        let mut latencies = Vec::new();

        for i in 0..1000 {
            let record = WalRecord::OrderInsert {
                order_id: i as u64,
                user_id: [(i >> 8) as u8; 32],
                instrument_id: [(i >> 16) as u8; 16],
                direction: (i % 2) as u8,
                offset: 0,
                price: 100.0 + i as f64,
                volume: 10.0,
                timestamp: i as i64,
            };

            let start = std::time::Instant::now();
            wal.append(record).unwrap();
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_micros());
        }

        // 计算性能统计
        latencies.sort();
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
        let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
        let max = latencies[latencies.len() - 1];

        println!("WAL 单次写入性能统计:");
        println!("  P50: {} μs", p50);
        println!("  P95: {} μs", p95);
        println!("  P99: {} μs (SSD 目标 < 1000 μs, HDD/VM < 30000 μs)", p99);
        println!("  Max: {} μs", max);

        // 验证 P99 < 50ms (宽松目标，适用于各种存储)
        // 生产环境使用 SSD + group commit 可以达到 P99 < 1ms
        assert!(p99 < 50_000, "P99 latency {} μs exceeds 50ms limit", p99);
    }

    #[test]
    #[ignore] // 环境相关的性能测试，在 CI 中跳过
    fn test_batch_performance() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let wal = WalManager::new(tmp_dir.path().to_str().unwrap());

        // 测试批量写入吞吐量
        // 注意：批量写入只 fsync 一次，性能主要受存储硬件限制
        // 当前实现：1000 条 / 次，单次 fsync
        // 生产环境优化：使用更大批次 (10K+) + 并行写入可达到 > 100K entries/s
        let batch_size = 1000;
        let mut records = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            records.push(WalRecord::TradeExecuted {
                trade_id: i as u64,
                order_id: i as u64,
                exchange_order_id: i as u64,
                price: 100.0 + i as f64,
                volume: 10.0,
                timestamp: i as i64,
            });
        }

        let start = std::time::Instant::now();
        wal.append_batch(records).unwrap();
        let elapsed = start.elapsed();

        let throughput = batch_size as f64 / elapsed.as_secs_f64();

        println!("WAL 批量写入性能:");
        println!("  批次大小: {}", batch_size);
        println!("  耗时: {:?}", elapsed);
        println!(
            "  吞吐量: {:.0} entries/s (生产环境目标 > 100,000 entries/s)",
            throughput
        );
        println!(
            "  平均延迟: {:.1} μs/entry",
            elapsed.as_micros() as f64 / batch_size as f64
        );

        // 验证吞吐量 > 10K entries/s (基础目标)
        // 生产环境使用 SSD + 更大批次 + 并行写入可达到 > 100K entries/s
        assert!(
            throughput > 10_000.0,
            "Throughput {:.0} entries/s below 10K minimum",
            throughput
        );

        // 打印性能报告
        if throughput > 100_000.0 {
            println!("  ✓ 性能优秀：超过生产环境目标");
        } else if throughput > 50_000.0 {
            println!("  ✓ 性能良好：接近生产环境目标");
        } else {
            println!("  ⚠ 性能一般：建议优化（使用 SSD、增大批次、启用 group commit）");
        }
    }
}
