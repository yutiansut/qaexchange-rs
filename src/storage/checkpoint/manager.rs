//! Checkpoint 管理器

use super::types::{CheckpointMetadata, CheckpointInfo};
use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, Read};
use rkyv::{Deserialize as RkyvDeserialize};

/// Checkpoint 管理器
pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
    instrument_id: String,
}

impl CheckpointManager {
    /// 创建新的 Checkpoint 管理器
    pub fn new(checkpoint_dir: impl AsRef<Path>, instrument_id: &str) -> Result<Self, String> {
        let checkpoint_dir = checkpoint_dir.as_ref().join(instrument_id);
        std::fs::create_dir_all(&checkpoint_dir)
            .map_err(|e| format!("Create checkpoint dir failed: {}", e))?;

        Ok(Self {
            checkpoint_dir,
            instrument_id: instrument_id.to_string(),
        })
    }

    /// 创建新的 Checkpoint
    pub fn create_checkpoint(
        &self,
        checkpoint_id: u64,
        wal_sequence: u64,
        sstable_files: Vec<String>,
        total_entries: u64,
        min_timestamp: i64,
        max_timestamp: i64,
    ) -> Result<CheckpointInfo, String> {
        let metadata = CheckpointMetadata::new(
            checkpoint_id,
            self.instrument_id.clone(),
            wal_sequence,
            sstable_files,
            total_entries,
            min_timestamp,
            max_timestamp,
        );

        // 生成 Checkpoint 文件名
        let checkpoint_file = self.checkpoint_dir
            .join(format!("checkpoint_{:010}.ckpt", checkpoint_id));

        // 序列化元数据
        let metadata_bytes = rkyv::to_bytes::<_, 4096>(&metadata)
            .map_err(|e| format!("Serialize metadata failed: {}", e))?;

        // 写入文件
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&checkpoint_file)
            .map_err(|e| format!("Create checkpoint file failed: {}", e))?;

        file.write_all(&metadata_bytes)
            .map_err(|e| format!("Write checkpoint failed: {}", e))?;

        file.flush()
            .map_err(|e| format!("Flush checkpoint failed: {}", e))?;

        let file_size = std::fs::metadata(&checkpoint_file)
            .map(|m| m.len())
            .unwrap_or(0);

        log::info!(
            "[{}] Created checkpoint {}, WAL sequence: {}, entries: {}",
            self.instrument_id,
            checkpoint_id,
            wal_sequence,
            total_entries
        );

        Ok(CheckpointInfo {
            path: checkpoint_file.to_string_lossy().to_string(),
            metadata,
            file_size,
        })
    }

    /// 加载最新的 Checkpoint
    pub fn load_latest_checkpoint(&self) -> Result<Option<CheckpointInfo>, String> {
        let mut checkpoints = self.list_checkpoints()?;

        if checkpoints.is_empty() {
            return Ok(None);
        }

        // 按 checkpoint_id 降序排序
        checkpoints.sort_by(|a, b| b.metadata.checkpoint_id.cmp(&a.metadata.checkpoint_id));

        Ok(Some(checkpoints.into_iter().next().unwrap()))
    }

    /// 列出所有 Checkpoint
    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointInfo>, String> {
        let entries = std::fs::read_dir(&self.checkpoint_dir)
            .map_err(|e| format!("Read checkpoint dir failed: {}", e))?;

        let mut checkpoints = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("ckpt") {
                continue;
            }

            match self.load_checkpoint(&path) {
                Ok(info) => checkpoints.push(info),
                Err(e) => {
                    log::warn!("Failed to load checkpoint {:?}: {}", path, e);
                }
            }
        }

        Ok(checkpoints)
    }

    /// 加载指定的 Checkpoint
    fn load_checkpoint(&self, path: &Path) -> Result<CheckpointInfo, String> {
        let mut file = File::open(path)
            .map_err(|e| format!("Open checkpoint file failed: {}", e))?;

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| format!("Read checkpoint failed: {}", e))?;

        let archived = rkyv::check_archived_root::<CheckpointMetadata>(&bytes)
            .map_err(|e| format!("Deserialize checkpoint failed: {}", e))?;

        let metadata: CheckpointMetadata = archived
            .deserialize(&mut rkyv::Infallible)
            .map_err(|e| format!("Deserialize checkpoint failed: {:?}", e))?;

        let file_size = std::fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(CheckpointInfo {
            path: path.to_string_lossy().to_string(),
            metadata,
            file_size,
        })
    }

    /// 删除旧的 Checkpoint（保留最近 N 个）
    pub fn cleanup_old_checkpoints(&self, keep_count: usize) -> Result<usize, String> {
        let mut checkpoints = self.list_checkpoints()?;

        if checkpoints.len() <= keep_count {
            return Ok(0);
        }

        // 按 checkpoint_id 降序排序
        checkpoints.sort_by(|a, b| b.metadata.checkpoint_id.cmp(&a.metadata.checkpoint_id));

        let mut deleted_count = 0;

        for checkpoint in checkpoints.into_iter().skip(keep_count) {
            if let Err(e) = std::fs::remove_file(&checkpoint.path) {
                log::warn!("Failed to delete checkpoint {}: {}", checkpoint.path, e);
            } else {
                log::info!(
                    "[{}] Deleted old checkpoint {}",
                    self.instrument_id,
                    checkpoint.metadata.checkpoint_id
                );
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }

    /// 获取 Checkpoint 目录
    pub fn checkpoint_dir(&self) -> &Path {
        &self.checkpoint_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_checkpoint_create_and_load() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path(), "IF888").unwrap();

        // 创建 checkpoint
        let sstables = vec!["sstable_001.sst".to_string()];
        let info = manager
            .create_checkpoint(1, 1000, sstables, 500, 0, 1000000)
            .unwrap();

        assert_eq!(info.metadata.checkpoint_id, 1);
        assert_eq!(info.metadata.wal_sequence, 1000);

        // 加载最新 checkpoint
        let loaded = manager.load_latest_checkpoint().unwrap().unwrap();
        assert_eq!(loaded.metadata.checkpoint_id, 1);
    }

    #[test]
    fn test_checkpoint_cleanup() {
        let dir = tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path(), "IF888").unwrap();

        // 创建多个 checkpoint
        for i in 1..=5 {
            manager
                .create_checkpoint(i, i * 1000, Vec::new(), 0, 0, 0)
                .unwrap();
        }

        // 保留最近 2 个
        let deleted = manager.cleanup_old_checkpoints(2).unwrap();
        assert_eq!(deleted, 3);

        // 验证剩余数量
        let remaining = manager.list_checkpoints().unwrap();
        assert_eq!(remaining.len(), 2);
    }
}
