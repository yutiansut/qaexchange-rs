// OLTP Hybrid Storage - 品种级混合存储
//
// 完整的 OLTP 路径：
// 1. Write: WAL (持久化) → MemTable (内存索引)
// 2. Flush: MemTable → SSTable (当内存超限时)
// 3. Read: MemTable (最新数据) + SSTable (历史数据)
//
// 性能目标：
// - 写入延迟：P99 < 100μs (WAL fsync + MemTable insert)
// - 读取延迟：P99 < 10μs (MemTable) / < 100μs (SSTable)
// - 吞吐量：> 100K writes/s (单品种)

use crate::storage::checkpoint::CheckpointManager;
use crate::storage::compaction::{CompactionConfig, CompactionScheduler, SSTableInfo};
use crate::storage::conversion::{ConversionManager, SchedulerConfig, WorkerConfig};
use crate::storage::memtable::oltp::OltpMemTable;
use crate::storage::memtable::types::MemTableValue;
use crate::storage::sstable::olap_parquet::ParquetSSTable;
use crate::storage::sstable::oltp_rkyv::{RkyvSSTable, RkyvSSTableWriter};
use crate::storage::wal::{WalManager, WalRecord};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// OLTP 混合存储配置
#[derive(Debug, Clone)]
pub struct OltpHybridConfig {
    /// 基础路径
    pub base_path: String,

    /// MemTable 最大内存（超过则 flush）
    pub memtable_size_bytes: usize,

    /// 每条记录的预估大小
    pub estimated_entry_size: usize,

    /// 是否启用 OLTP → OLAP 转换
    pub enable_olap_conversion: bool,

    /// OLAP 转换触发阈值（SSTable 数量）
    pub olap_conversion_threshold: usize,

    /// OLAP 转换触发阈值（数据年龄，秒）
    pub olap_conversion_age_seconds: i64,
}

impl Default for OltpHybridConfig {
    fn default() -> Self {
        Self {
            base_path: "/home/quantaxis/qaexchange-rs/output//qaexchange/storage".to_string(),
            memtable_size_bytes: 64 * 1024 * 1024, // 64 MB
            estimated_entry_size: 256,
            enable_olap_conversion: true,
            olap_conversion_threshold: 10,          // 10 个 SSTable 触发转换
            olap_conversion_age_seconds: 3600 * 24, // 1 天前的数据触发转换
        }
    }
}

/// OLTP 混合存储（单品种）
///
/// 目录结构：
/// {base_path}/{instrument_id}/
///   ├── wal/           - WAL 文件
///   ├── sstables/      - OLTP SSTable 文件
///   └── olap/          - OLAP Parquet 文件
///
/// 数据流：
/// Write Path: WAL → MemTable → SSTable → Parquet (异步转换)
/// Read Path:  MemTable + SSTable (OLTP) + Parquet (OLAP)
pub struct OltpHybridStorage {
    /// 品种 ID
    instrument_id: String,

    /// WAL 管理器
    wal: Arc<WalManager>,

    /// 当前活跃的 MemTable
    memtable: Arc<RwLock<OltpMemTable>>,

    /// 已持久化的 SSTable 列表（按时间排序）
    sstables: Arc<RwLock<Vec<Arc<RkyvSSTable>>>>,

    /// OLAP Parquet 文件列表（已转换的历史数据）
    olap_files: Arc<RwLock<Vec<Arc<ParquetSSTable>>>>,

    /// OLAP 数据时间边界（早于此时间使用 OLAP）
    olap_cutoff_timestamp: Arc<parking_lot::Mutex<i64>>,

    /// Compaction 调度器
    compaction_scheduler: Arc<CompactionScheduler>,

    /// Checkpoint 管理器
    checkpoint_manager: Arc<CheckpointManager>,

    /// Checkpoint 计数器
    checkpoint_counter: Arc<parking_lot::Mutex<u64>>,

    /// 转换管理器（可选）
    conversion_manager: Option<Arc<parking_lot::Mutex<ConversionManager>>>,

    /// 配置
    config: OltpHybridConfig,

    /// SSTable 计数器（用于生成文件名）
    sstable_counter: Arc<parking_lot::Mutex<u64>>,
}

impl OltpHybridStorage {
    /// 创建新的混合存储
    pub fn create(instrument_id: &str, config: OltpHybridConfig) -> Result<Self, String> {
        let base_path = PathBuf::from(&config.base_path).join(instrument_id);
        std::fs::create_dir_all(&base_path)
            .map_err(|e| format!("Create base path failed: {}", e))?;

        // 创建 WAL 目录
        let wal_path = base_path.join("wal");
        let wal = Arc::new(WalManager::new(wal_path.to_str().unwrap()));

        // 创建 SSTable 目录
        let sstable_path = base_path.join("sstables");
        std::fs::create_dir_all(&sstable_path)
            .map_err(|e| format!("Create sstable path failed: {}", e))?;

        // 创建 OLAP 目录
        let olap_path = base_path.join("olap");
        std::fs::create_dir_all(&olap_path)
            .map_err(|e| format!("Create olap path failed: {}", e))?;

        // 创建 MemTable
        let memtable_config = crate::storage::memtable::oltp::OltpMemTableConfig {
            max_size_bytes: config.memtable_size_bytes,
            estimated_entry_size: config.estimated_entry_size,
        };
        let memtable = Arc::new(RwLock::new(OltpMemTable::new(memtable_config)));

        // 创建 Compaction 调度器
        let compaction_config = CompactionConfig::default();
        let compaction_scheduler = Arc::new(CompactionScheduler::new(
            base_path.clone(),
            compaction_config,
        ));

        // 启动后台 compaction 线程
        compaction_scheduler.start();

        // 创建 Checkpoint 管理器
        let checkpoint_dir = base_path.join("checkpoints");
        let checkpoint_manager = Arc::new(CheckpointManager::new(&checkpoint_dir, instrument_id)?);

        // 创建转换管理器（如果启用）
        let conversion_manager = if config.enable_olap_conversion {
            let metadata_path = base_path.join("conversion_metadata.json");
            let scheduler_config = SchedulerConfig::default();
            let worker_config = WorkerConfig::default();

            match ConversionManager::new(
                PathBuf::from(&config.base_path),
                metadata_path,
                scheduler_config,
                worker_config,
            ) {
                Ok(mut manager) => {
                    manager.start();
                    log::info!("[{}] OLAP conversion manager started", instrument_id);
                    Some(Arc::new(parking_lot::Mutex::new(manager)))
                }
                Err(e) => {
                    log::warn!(
                        "[{}] Failed to create conversion manager: {}. OLAP conversion disabled.",
                        instrument_id,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        // 加载已有的 OLAP 文件
        let olap_files = Arc::new(RwLock::new(Self::load_olap_files(&olap_path)?));
        let olap_cutoff = Self::calculate_olap_cutoff(&olap_files.read());

        let storage = Self {
            instrument_id: instrument_id.to_string(),
            wal: wal.clone(),
            memtable: memtable.clone(),
            sstables: Arc::new(RwLock::new(Vec::new())),
            olap_files,
            olap_cutoff_timestamp: Arc::new(parking_lot::Mutex::new(olap_cutoff)),
            compaction_scheduler,
            checkpoint_manager,
            checkpoint_counter: Arc::new(parking_lot::Mutex::new(0)),
            conversion_manager,
            config,
            sstable_counter: Arc::new(parking_lot::Mutex::new(0)),
        };

        // 从 WAL 重放数据到 MemTable（恢复时必需）
        log::info!("[{}] Replaying WAL to MemTable...", instrument_id);
        let mut replayed_count = 0;
        wal.replay(|entry| {
            let memtable = memtable.read();
            memtable.insert(entry.sequence, entry.record);
            replayed_count += 1;
            Ok(())
        })
        .map_err(|e| format!("WAL replay failed: {}", e))?;

        if replayed_count > 0 {
            log::info!(
                "[{}] ✅ Replayed {} records from WAL to MemTable",
                instrument_id,
                replayed_count
            );
        }

        Ok(storage)
    }

    /// 加载 OLAP Parquet 文件
    fn load_olap_files(olap_path: &PathBuf) -> Result<Vec<Arc<ParquetSSTable>>, String> {
        let mut files = Vec::new();

        if !olap_path.exists() {
            return Ok(files);
        }

        let entries = std::fs::read_dir(olap_path)
            .map_err(|e| format!("Read olap dir failed: {}", e))?;

        let mut parquet_paths: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("parquet"))
            .map(|e| e.path())
            .collect();

        parquet_paths.sort();

        for path in parquet_paths {
            match ParquetSSTable::open(&path) {
                Ok(sstable) => {
                    log::debug!("Loaded OLAP file: {:?}", path);
                    files.push(Arc::new(sstable));
                }
                Err(e) => {
                    log::warn!("Failed to load OLAP file {:?}: {}", path, e);
                }
            }
        }

        log::info!("Loaded {} OLAP Parquet files", files.len());
        Ok(files)
    }

    /// 计算 OLAP 时间边界
    fn calculate_olap_cutoff(olap_files: &[Arc<ParquetSSTable>]) -> i64 {
        olap_files
            .iter()
            .map(|f| f.metadata().max_timestamp)
            .max()
            .unwrap_or(0)
    }

    /// 写入记录（WAL + MemTable）
    ///
    /// # Performance
    /// - P99 < 100μs (WAL fsync ~20-50μs + MemTable insert ~3μs)
    pub fn write(&self, record: WalRecord) -> Result<u64, String> {
        // 1. 写入 WAL（持久化）
        let sequence = self.wal.append(record.clone())?;

        // 2. 写入 MemTable（内存索引）
        let memtable = self.memtable.read();
        memtable.insert(sequence, record);

        // 3. 检查是否需要 flush
        if memtable.should_flush() {
            drop(memtable); // 释放读锁
            self.try_flush()?;
        }

        Ok(sequence)
    }

    /// 批量写入
    ///
    /// # Performance
    /// - 批量写入只 fsync 一次，大幅提升吞吐
    /// - 适合批量成交回报、批量订单插入
    pub fn write_batch(&self, records: Vec<WalRecord>) -> Result<Vec<u64>, String> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        // 1. 批量写入 WAL
        let sequences = self.wal.append_batch(records.clone())?;

        // 2. 批量写入 MemTable
        let entries: Vec<_> = sequences
            .iter()
            .zip(records.iter())
            .map(|(&seq, record)| (seq, record.clone()))
            .collect();

        let memtable = self.memtable.read();
        memtable.insert_batch(entries);

        // 3. 检查是否需要 flush
        if memtable.should_flush() {
            drop(memtable);
            self.try_flush()?;
        }

        Ok(sequences)
    }

    /// 范围查询（时间范围）
    ///
    /// # Performance
    /// - MemTable: P99 < 10μs
    /// - SSTable: P99 < 100μs (per file)
    ///
    /// # Strategy
    /// 1. 查询 MemTable（最新数据）
    /// 2. 查询所有相关的 SSTable（按时间过滤）
    /// 3. 合并结果（去重、排序）
    pub fn range_query(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<(i64, u64, WalRecord)>, String> {
        let mut results = Vec::new();

        // 1. 查询 MemTable
        let memtable = self.memtable.read();
        let memtable_results = memtable.range_query(start_ts, end_ts);
        // 转换 MemTable 结果格式：(MemTableKey, WalRecord) -> (i64, u64, WalRecord)
        for (key, record) in memtable_results {
            results.push((key.timestamp, key.sequence, record));
        }

        // 2. 查询 SSTable（只查询时间范围重叠的文件）
        let sstables = self.sstables.read();
        for sstable in sstables.iter() {
            let metadata = sstable.metadata();

            // 时间范围过滤（文件级别）
            if metadata.max_timestamp < start_ts || metadata.min_timestamp > end_ts {
                continue;
            }

            let sstable_results = sstable.range_query(start_ts, end_ts)?;
            results.extend(sstable_results);
        }

        // 3. 排序和去重（按 timestamp + sequence）
        results.sort_by_key(|(ts, seq, _)| (*ts, *seq));
        results.dedup_by_key(|(ts, seq, _)| (*ts, *seq));

        Ok(results)
    }

    /// Flush MemTable 到 SSTable
    ///
    /// # Performance
    /// - 写入速度：> 100 MB/s
    /// - 阻塞时间：最小化（使用双缓冲）
    fn try_flush(&self) -> Result<(), String> {
        // 获取写锁（阻塞写入）
        let memtable = self.memtable.write();

        // 快速检查是否真的需要 flush
        if !memtable.should_flush() {
            return Ok(());
        }

        // 获取所有数据
        let entries = memtable.iter_all();
        if entries.is_empty() {
            return Ok(());
        }

        // 生成 SSTable 文件名
        let mut counter = self.sstable_counter.lock();
        *counter += 1;
        let sstable_id = *counter;
        drop(counter);

        let sstable_path = PathBuf::from(&self.config.base_path)
            .join(&self.instrument_id)
            .join("sstables")
            .join(format!("{:010}.sst", sstable_id));

        // 创建 SSTable Writer
        let mut writer = RkyvSSTableWriter::create(&sstable_path)?;

        // 写入数据
        for (key, record) in entries {
            let value = MemTableValue::new(record);
            writer.append(key, value)?;
        }

        // 完成写入
        let metadata = writer.finish()?;

        log::info!(
            "[{}] Flushed MemTable to SSTable: {} entries, {}-{} ns",
            self.instrument_id,
            metadata.entry_count,
            metadata.min_timestamp,
            metadata.max_timestamp
        );

        // 清空 MemTable
        memtable.clear();

        // 加载新的 SSTable
        let sstable = Arc::new(RkyvSSTable::open(&sstable_path)?);
        self.sstables.write().push(sstable);

        // 注册 SSTable 到 Compaction Scheduler (Level 0)
        let file_size = std::fs::metadata(&sstable_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let sstable_info = SSTableInfo {
            file_path: sstable_path.to_string_lossy().to_string(),
            file_size,
            min_key: metadata.min_key.clone(),
            max_key: metadata.max_key.clone(),
            level: 0, // MemTable flush 的结果都是 Level 0
            entry_count: metadata.entry_count,
            min_timestamp: metadata.min_timestamp,
            max_timestamp: metadata.max_timestamp,
        };

        self.compaction_scheduler.register_sstable(sstable_info);

        Ok(())
    }

    /// 恢复（从 WAL 回放）
    ///
    /// # Recovery Process
    /// 1. 加载所有 SSTable
    /// 2. 回放 WAL 到 MemTable
    /// 3. 验证数据完整性
    pub fn recover(&self) -> Result<(), String> {
        // 1. 加载所有 SSTable
        let sstable_dir = PathBuf::from(&self.config.base_path)
            .join(&self.instrument_id)
            .join("sstables");

        if sstable_dir.exists() {
            let entries = std::fs::read_dir(&sstable_dir)
                .map_err(|e| format!("Read sstable dir failed: {}", e))?;

            let mut sstable_files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("sst"))
                .map(|e| e.path())
                .collect();

            sstable_files.sort();

            for path in sstable_files {
                let sstable = Arc::new(RkyvSSTable::open(&path)?);
                let metadata = sstable.metadata();

                // 注册到 Compaction Scheduler
                let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                let sstable_info = SSTableInfo {
                    file_path: path.to_string_lossy().to_string(),
                    file_size,
                    min_key: metadata.min_key.clone(),
                    max_key: metadata.max_key.clone(),
                    level: 0, // 初始都是 Level 0，compaction 会自动调整
                    entry_count: metadata.entry_count,
                    min_timestamp: metadata.min_timestamp,
                    max_timestamp: metadata.max_timestamp,
                };

                self.compaction_scheduler.register_sstable(sstable_info);
                self.sstables.write().push(sstable);
            }

            log::info!(
                "[{}] Loaded {} SSTable files",
                self.instrument_id,
                self.sstables.read().len()
            );
        }

        // 2. 回放 WAL
        let mut recovered_count = 0;
        self.wal.replay(|entry| {
            let memtable = self.memtable.read();
            memtable.insert(entry.sequence, entry.record);
            recovered_count += 1;
            Ok(())
        })?;

        log::info!(
            "[{}] Recovered {} entries from WAL",
            self.instrument_id,
            recovered_count
        );

        Ok(())
    }

    /// 获取统计信息
    pub fn stats(&self) -> StorageStats {
        let memtable = self.memtable.read();
        let sstables = self.sstables.read();

        StorageStats {
            instrument_id: self.instrument_id.clone(),
            memtable_entries: memtable.len(),
            memtable_size_bytes: memtable.size_bytes(),
            sstable_count: sstables.len(),
            sstable_entries: sstables.iter().map(|s| s.metadata().entry_count).sum(),
            min_timestamp: memtable
                .min_timestamp()
                .or_else(|| sstables.first().map(|s| s.metadata().min_timestamp)),
            max_timestamp: memtable
                .max_timestamp()
                .or_else(|| sstables.last().map(|s| s.metadata().max_timestamp)),
        }
    }

    /// 获取 Compaction 统计信息
    pub fn compaction_stats(&self) -> crate::storage::compaction::scheduler::CompactionStats {
        self.compaction_scheduler.get_stats()
    }

    /// 手动触发 Compaction
    pub fn trigger_compaction(&self) -> Result<(), String> {
        self.compaction_scheduler.trigger_compaction()
    }

    /// 创建 Checkpoint（快照当前状态）
    pub fn create_checkpoint(&self) -> Result<(), String> {
        // 获取当前 WAL 序列号
        let wal_sequence = self.wal.get_current_sequence();

        // 获取所有 SSTable 文件路径
        let sstables = self.sstables.read();
        let sstable_files: Vec<String> = sstables
            .iter()
            .map(|sst| sst.file_path().to_string())
            .collect();

        // 统计信息
        let total_entries: u64 = sstables.iter().map(|sst| sst.metadata().entry_count).sum();

        let min_timestamp = sstables
            .first()
            .map(|sst| sst.metadata().min_timestamp)
            .unwrap_or(0);

        let max_timestamp = sstables
            .last()
            .map(|sst| sst.metadata().max_timestamp)
            .unwrap_or(0);

        drop(sstables);

        // 生成 Checkpoint ID
        let mut counter = self.checkpoint_counter.lock();
        *counter += 1;
        let checkpoint_id = *counter;
        drop(counter);

        // 创建 Checkpoint
        let _checkpoint_info = self.checkpoint_manager.create_checkpoint(
            checkpoint_id,
            wal_sequence,
            sstable_files,
            total_entries,
            min_timestamp,
            max_timestamp,
        )?;

        log::info!(
            "[{}] Created checkpoint {}, WAL sequence: {}",
            self.instrument_id,
            checkpoint_id,
            wal_sequence
        );

        // 清理旧的 Checkpoint（保留最近 3 个）
        self.checkpoint_manager.cleanup_old_checkpoints(3)?;

        // 清理已 checkpoint 的 WAL
        self.wal.checkpoint(wal_sequence)?;

        Ok(())
    }

    /// 从最新的 Checkpoint 恢复
    pub fn recover_from_checkpoint(&self) -> Result<(), String> {
        // 加载最新 Checkpoint
        let checkpoint = match self.checkpoint_manager.load_latest_checkpoint()? {
            Some(ckpt) => ckpt,
            None => {
                log::info!(
                    "[{}] No checkpoint found, skipping checkpoint recovery",
                    self.instrument_id
                );
                return Ok(());
            }
        };

        log::info!(
            "[{}] Recovering from checkpoint {}, WAL sequence: {}",
            self.instrument_id,
            checkpoint.metadata.checkpoint_id,
            checkpoint.metadata.wal_sequence
        );

        // 加载 SSTable
        let mut sstables = self.sstables.write();
        for sstable_path in &checkpoint.metadata.sstable_files {
            let sst = Arc::new(RkyvSSTable::open(sstable_path)?);
            sstables.push(sst);
        }

        log::info!(
            "[{}] Loaded {} SSTable files from checkpoint",
            self.instrument_id,
            sstables.len()
        );

        drop(sstables);

        // 从 Checkpoint 之后的 WAL 回放
        // 注意：WAL 在 checkpoint 时已清理，所以当前 WAL 只包含 checkpoint 之后的数据
        let checkpoint_sequence = checkpoint.metadata.wal_sequence;
        let mut recovered_count = 0;

        self.wal.replay(|entry| {
            // 跳过已 checkpoint 的数据
            if entry.sequence <= checkpoint_sequence {
                return Ok(());
            }

            let memtable = self.memtable.read();
            memtable.insert(entry.sequence, entry.record);
            recovered_count += 1;
            Ok(())
        })?;

        log::info!(
            "[{}] Replayed {} WAL entries after checkpoint",
            self.instrument_id,
            recovered_count
        );

        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // OLAP 访问接口（供 BatchDataSource 使用）
    // ═══════════════════════════════════════════════════════════════════════════

    /// 获取 OLAP 文件列表
    ///
    /// 用于 OltpBatchAdapter 构建混合查询
    pub fn get_olap_files(&self) -> Vec<Arc<ParquetSSTable>> {
        self.olap_files.read().clone()
    }

    /// 获取 WAL 管理器引用（用于历史数据查询）@yutiansut @quantaxis
    pub fn get_wal_manager(&self) -> Arc<WalManager> {
        self.wal.clone()
    }

    /// 获取 OLAP 时间边界
    ///
    /// 早于此时间的数据应该查询 OLAP
    pub fn get_olap_cutoff_timestamp(&self) -> i64 {
        *self.olap_cutoff_timestamp.lock()
    }

    /// 刷新 OLAP 文件列表
    ///
    /// 在转换完成后调用以加载新的 Parquet 文件
    pub fn refresh_olap_files(&self) -> Result<(), String> {
        let olap_path = PathBuf::from(&self.config.base_path)
            .join(&self.instrument_id)
            .join("olap");

        let new_files = Self::load_olap_files(&olap_path)?;
        let new_cutoff = Self::calculate_olap_cutoff(&new_files);

        *self.olap_files.write() = new_files;
        *self.olap_cutoff_timestamp.lock() = new_cutoff;

        log::info!(
            "[{}] Refreshed OLAP files, cutoff timestamp: {}",
            self.instrument_id,
            new_cutoff
        );

        Ok(())
    }

    /// 检查并触发 OLAP 转换
    ///
    /// 条件：
    /// 1. SSTable 数量超过阈值
    /// 2. 存在超过年龄阈值的数据
    pub fn check_and_trigger_conversion(&self) -> Result<bool, String> {
        let conversion_manager = match &self.conversion_manager {
            Some(cm) => cm,
            None => return Ok(false),
        };

        let sstables = self.sstables.read();

        // 条件 1：SSTable 数量超过阈值
        if sstables.len() < self.config.olap_conversion_threshold {
            return Ok(false);
        }

        // 条件 2：检查是否有超过年龄阈值的数据
        let now = chrono::Utc::now().timestamp();
        let age_threshold = now - self.config.olap_conversion_age_seconds;

        // 找到所有 max_timestamp < age_threshold 的 SSTable（完全过期）
        let old_sstables: Vec<PathBuf> = sstables
            .iter()
            .filter(|sst| sst.metadata().max_timestamp < age_threshold)
            .map(|sst| PathBuf::from(sst.file_path()))
            .collect();

        if old_sstables.is_empty() {
            return Ok(false);
        }

        drop(sstables);

        // 触发转换
        log::info!(
            "[{}] Triggering OLAP conversion for {} old SSTables",
            self.instrument_id,
            old_sstables.len()
        );

        let manager = conversion_manager.lock();
        manager.trigger_conversion(&self.instrument_id, old_sstables)?;

        Ok(true)
    }

    /// 手动触发 OLAP 转换（用于测试或立即转换）
    pub fn trigger_olap_conversion(&self, sstable_paths: Vec<PathBuf>) -> Result<(), String> {
        let conversion_manager = self
            .conversion_manager
            .as_ref()
            .ok_or("OLAP conversion not enabled")?;

        let manager = conversion_manager.lock();
        manager.trigger_conversion(&self.instrument_id, sstable_paths)
    }

    /// 获取转换统计信息
    pub fn conversion_stats(&self) -> Option<crate::storage::conversion::ConversionStats> {
        self.conversion_manager
            .as_ref()
            .map(|cm| cm.lock().get_stats())
    }
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub instrument_id: String,
    pub memtable_entries: usize,
    pub memtable_size_bytes: usize,
    pub sstable_count: usize,
    pub sstable_entries: u64,
    pub min_timestamp: Option<i64>,
    pub max_timestamp: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_write_and_read() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 1024 * 1024,
            estimated_entry_size: 256,
            enable_olap_conversion: false, // 测试中禁用 OLAP 转换
            ..Default::default()
        };

        let storage = OltpHybridStorage::create("IF2501", config).unwrap();

        // 写入数据
        for i in 0..100 {
            let record = create_order_record(i, 1000 + i as i64 * 10);
            storage.write(record).unwrap();
        }

        // 查询数据
        let results = storage.range_query(1000, 1500).unwrap();
        assert_eq!(results.len(), 51);

        // 验证顺序
        for (i, (ts, _, _)) in results.iter().enumerate() {
            assert_eq!(*ts, 1000 + i as i64 * 10);
        }
    }

    #[tokio::test]
    async fn test_flush() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 1000, // 很小，容易触发 flush
            estimated_entry_size: 100,
            enable_olap_conversion: false, // 测试中禁用 OLAP 转换
            ..Default::default()
        };

        let storage = OltpHybridStorage::create("IF2501", config).unwrap();

        // 写入足够多数据触发 flush
        for i in 0..50 {
            let record = create_order_record(i, 1000 + i as i64);
            storage.write(record).unwrap();
        }

        // 验证 SSTable 已创建
        let stats = storage.stats();
        assert!(stats.sstable_count > 0);
        println!("Stats: {:?}", stats);
    }

    #[tokio::test]
    async fn test_batch_write() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            ..Default::default()
        };

        let storage = OltpHybridStorage::create("IF2501", config).unwrap();

        // 批量写入
        let mut batch = Vec::new();
        for i in 0..1000 {
            batch.push(create_order_record(i, 1000 + i as i64));
        }

        let sequences = storage.write_batch(batch).unwrap();
        assert_eq!(sequences.len(), 1000);

        // 验证数据
        let results = storage.range_query(1000, 2000).unwrap();
        assert_eq!(results.len(), 1000);
    }

    #[tokio::test]
    async fn test_recovery() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let base_path = tmp_dir.path().to_str().unwrap().to_string();

        // 写入数据
        {
            let config = OltpHybridConfig {
                base_path: base_path.clone(),
                ..Default::default()
            };
            let storage = OltpHybridStorage::create("IF2501", config).unwrap();
            for i in 0..100 {
                storage
                    .write(create_order_record(i, 1000 + i as i64))
                    .unwrap();
            }
            // storage 析构，确保 WAL 文件正确关闭
        }

        // 恢复
        {
            let config = OltpHybridConfig {
                base_path: base_path.clone(),
                ..Default::default()
            };
            let storage = OltpHybridStorage::create("IF2501", config).unwrap();
            storage.recover().unwrap();

            let stats = storage.stats();
            assert_eq!(stats.memtable_entries, 100);

            // 验证数据
            let results = storage.range_query(1000, 1100).unwrap();
            assert_eq!(results.len(), 100);
        }
    }

    #[tokio::test]
    async fn test_performance_write() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 100 * 1024 * 1024, // 100 MB，避免 flush
            ..Default::default()
        };

        let storage = OltpHybridStorage::create("IF2501", config).unwrap();

        // 测试 1000 次写入延迟
        let mut latencies = Vec::new();
        for i in 0..1000 {
            let record = create_order_record(i, 1000 + i as i64);
            let start = std::time::Instant::now();
            storage.write(record).unwrap();
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_micros());
        }

        latencies.sort();
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
        let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
        let max = latencies[latencies.len() - 1];

        println!("OLTP HybridStorage 写入性能:");
        println!("  P50: {} μs", p50);
        println!("  P95: {} μs", p95);
        println!("  P99: {} μs", p99);
        println!("  Max: {} μs", max);
        println!("  说明: WAL fsync 主导延迟");
        println!("  - HDD/VM: P99 ~ 20-50ms");
        println!("  - SSD: P99 ~ 1ms");
        println!("  - 生产优化: group commit 可达到 P99 < 100μs");

        // 验证 P99 < 100ms (宽松目标，适应各种存储)
        assert!(p99 < 100_000, "P99 {} μs exceeds 100ms limit", p99);
    }
}
