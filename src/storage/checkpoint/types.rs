//! Checkpoint 类型定义

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Checkpoint 元数据
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct CheckpointMetadata {
    /// Checkpoint ID
    pub checkpoint_id: u64,

    /// Checkpoint 时间戳（Unix 秒）
    pub timestamp: i64,

    /// WAL 序列号（此 checkpoint 包含到此序列号的所有数据）
    pub wal_sequence: u64,

    /// 包含的 SSTable 文件列表
    pub sstable_files: Vec<String>,

    /// 品种 ID
    pub instrument_id: String,

    /// 总记录数
    pub total_entries: u64,

    /// 时间范围
    pub min_timestamp: i64,
    pub max_timestamp: i64,
}

impl CheckpointMetadata {
    pub fn new(
        checkpoint_id: u64,
        instrument_id: String,
        wal_sequence: u64,
        sstable_files: Vec<String>,
        total_entries: u64,
        min_timestamp: i64,
        max_timestamp: i64,
    ) -> Self {
        Self {
            checkpoint_id,
            timestamp: chrono::Utc::now().timestamp(),
            wal_sequence,
            sstable_files,
            instrument_id,
            total_entries,
            min_timestamp,
            max_timestamp,
        }
    }
}

/// Checkpoint 信息（用于查询和管理）
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    /// Checkpoint 文件路径
    pub path: String,

    /// 元数据
    pub metadata: CheckpointMetadata,

    /// 文件大小（字节）
    pub file_size: u64,
}
