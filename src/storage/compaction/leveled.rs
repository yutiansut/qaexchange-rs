//! Leveled Compaction 策略实现

use super::{CompactionConfig, SSTableInfo};
use crate::storage::memtable::types::{MemTableKey, MemTableValue};
use crate::storage::sstable::oltp_rkyv::{RkyvSSTable, RkyvSSTableWriter};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Compaction 任务
#[derive(Debug, Clone)]
pub struct CompactionTask {
    /// 源层级
    pub source_level: usize,

    /// 目标层级
    pub target_level: usize,

    /// 需要合并的 SSTable 列表
    pub sstables: Vec<SSTableInfo>,

    /// 输出文件路径
    pub output_path: String,
}

/// Compaction 结果
#[derive(Debug)]
pub struct CompactionResult {
    /// 新生成的 SSTable
    pub new_sstable: SSTableInfo,

    /// 需要删除的旧 SSTable
    pub obsolete_sstables: Vec<String>,

    /// 合并的记录数
    pub merged_entries: u64,

    /// 删除的过期记录数
    pub deleted_entries: u64,
}

/// Leveled Compaction 实现
pub struct LeveledCompaction {
    config: CompactionConfig,
    base_path: PathBuf,
}

impl LeveledCompaction {
    pub fn new(base_path: impl AsRef<Path>, config: CompactionConfig) -> Self {
        Self {
            config,
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// 检查是否需要触发 compaction
    pub fn should_compact(
        &self,
        level_sstables: &HashMap<usize, Vec<SSTableInfo>>,
    ) -> Option<CompactionTask> {
        // 检查 Level 0 是否超过文件数限制
        if let Some(level0_files) = level_sstables.get(&0) {
            if level0_files.len() >= self.config.level0_max_files {
                return Some(
                    self.create_level0_compaction_task(level0_files, level_sstables.get(&1)),
                );
            }
        }

        // 检查其他层是否超过大小限制
        for level in 1..self.config.max_levels {
            if let Some(level_files) = level_sstables.get(&level) {
                let total_size: u64 = level_files.iter().map(|f| f.file_size).sum();
                let max_size = self.level_max_size(level);

                if total_size > max_size {
                    return Some(self.create_level_compaction_task(
                        level,
                        level_files,
                        level_sstables.get(&(level + 1)),
                    ));
                }
            }
        }

        None
    }

    /// 创建 Level 0 → Level 1 的 compaction 任务
    fn create_level0_compaction_task(
        &self,
        level0_files: &[SSTableInfo],
        level1_files: Option<&Vec<SSTableInfo>>,
    ) -> CompactionTask {
        let mut sstables = level0_files.to_vec();

        // Level 0 的文件可能有重叠，需要合并所有文件
        // 同时需要找到 Level 1 中与之重叠的文件
        if let Some(l1_files) = level1_files {
            for l1_file in l1_files {
                // 检查是否与任何 Level 0 文件重叠
                if level0_files.iter().any(|l0| l0.overlaps(l1_file)) {
                    sstables.push(l1_file.clone());
                }
            }
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let output_path = self
            .base_path
            .join("sstables")
            .join(format!("l1_{}.sst", timestamp))
            .to_string_lossy()
            .to_string();

        CompactionTask {
            source_level: 0,
            target_level: 1,
            sstables,
            output_path,
        }
    }

    /// 创建 Level N → Level N+1 的 compaction 任务
    fn create_level_compaction_task(
        &self,
        level: usize,
        level_files: &[SSTableInfo],
        next_level_files: Option<&Vec<SSTableInfo>>,
    ) -> CompactionTask {
        // 选择第一个文件进行 compaction（简单策略）
        let mut sstables = vec![level_files[0].clone()];

        // 找到下一层中与之重叠的文件
        if let Some(next_files) = next_level_files {
            for next_file in next_files {
                if level_files[0].overlaps(next_file) {
                    sstables.push(next_file.clone());
                }
            }
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        let output_path = self
            .base_path
            .join("sstables")
            .join(format!("l{}_{}.sst", level + 1, timestamp))
            .to_string_lossy()
            .to_string();

        CompactionTask {
            source_level: level,
            target_level: level + 1,
            sstables,
            output_path,
        }
    }

    /// 执行 compaction 任务
    pub fn execute_compaction(&self, task: CompactionTask) -> Result<CompactionResult, String> {
        log::info!(
            "Starting compaction: L{} → L{}, merging {} files",
            task.source_level,
            task.target_level,
            task.sstables.len()
        );

        // 打开所有 SSTable
        let mut sstables = Vec::new();
        for info in &task.sstables {
            let sst = RkyvSSTable::open(&info.file_path)
                .map_err(|e| format!("Failed to open SSTable {}: {}", info.file_path, e))?;
            sstables.push(sst);
        }

        // 合并迭代器 - 使用优先队列合并多个有序流
        let mut entries: Vec<(MemTableKey, MemTableValue)> = Vec::new();
        let mut seen_keys: HashMap<Vec<u8>, i64> = HashMap::new();

        for sst in &sstables {
            // 使用 range_query 获取所有记录
            let sst_entries = sst
                .range_query(i64::MIN, i64::MAX)
                .map_err(|e| format!("Range query failed: {}", e))?;

            for (timestamp, sequence, record) in sst_entries {
                let key = MemTableKey::new(timestamp, sequence);
                let value = MemTableValue::new(record);
                let key_bytes = key.to_bytes();

                // 去重：只保留最新的值（按时间戳）
                if let Some(&existing_ts) = seen_keys.get(&key_bytes) {
                    if timestamp > existing_ts {
                        // 更新为更新的值
                        entries
                            .retain(|e: &(MemTableKey, MemTableValue)| e.0.to_bytes() != key_bytes);
                        entries.push((key.clone(), value.clone()));
                        seen_keys.insert(key_bytes.clone(), timestamp);
                    }
                } else {
                    entries.push((key.clone(), value.clone()));
                    seen_keys.insert(key_bytes, timestamp);
                }
            }
        }

        // 按 key 排序
        entries.sort_by(|a, b| a.0.to_bytes().cmp(&b.0.to_bytes()));

        let merged_count = entries.len() as u64;
        let deleted_count = 0u64; // TODO: 统计删除的记录

        // 写入新的 SSTable
        let mut writer = RkyvSSTableWriter::create(&task.output_path)
            .map_err(|e| format!("Failed to create SSTable writer: {}", e))?;

        for (key, value) in &entries {
            writer
                .append(key.clone(), value.clone())
                .map_err(|e| format!("Failed to append entry: {}", e))?;
        }

        let metadata = writer
            .finish()
            .map_err(|e| format!("Failed to finish SSTable: {}", e))?;

        // 生成新的 SSTableInfo
        let new_sstable = SSTableInfo {
            file_path: task.output_path.clone(),
            file_size: std::fs::metadata(&task.output_path)
                .map(|m| m.len())
                .unwrap_or(0),
            min_key: metadata.min_key.clone(),
            max_key: metadata.max_key.clone(),
            level: task.target_level,
            entry_count: metadata.entry_count,
            min_timestamp: metadata.min_timestamp,
            max_timestamp: metadata.max_timestamp,
        };

        let obsolete_sstables: Vec<String> = task
            .sstables
            .iter()
            .map(|info| info.file_path.clone())
            .collect();

        log::info!(
            "Compaction completed: {} entries merged, {} obsolete files",
            merged_count,
            obsolete_sstables.len()
        );

        Ok(CompactionResult {
            new_sstable,
            obsolete_sstables,
            merged_entries: merged_count,
            deleted_entries: deleted_count,
        })
    }

    /// 计算层级的最大大小
    fn level_max_size(&self, level: usize) -> u64 {
        if level == 0 {
            u64::MAX // Level 0 只限制文件数，不限制大小
        } else {
            self.config.level1_max_bytes * self.config.level_size_multiplier.pow((level - 1) as u32)
        }
    }
}
