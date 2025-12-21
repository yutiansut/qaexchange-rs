//! Compaction 模块 - LSM-Tree 压缩合并
//!
//! 实现 Leveled Compaction 策略：
//! - Level 0: MemTable flush 的直接结果（可能有重叠 key）
//! - Level 1-N: 每层容量是上一层的 10 倍，层内 SSTable key range 不重叠
//!
//! 触发条件：
//! - Level 0 超过 4 个 SSTable
//! - Level N 总大小超过阈值（Level 1: 10MB, Level 2: 100MB...）

pub mod leveled;
pub mod scheduler;

pub use leveled::{CompactionResult, CompactionTask, LeveledCompaction};
pub use scheduler::CompactionScheduler;

/// Compaction 配置
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Level 0 最大 SSTable 数量（超过则触发 compaction）
    pub level0_max_files: usize,

    /// Level 1 最大大小（bytes）
    pub level1_max_bytes: u64,

    /// 每层大小的倍数（默认 10）
    pub level_size_multiplier: u64,

    /// 最大层数
    pub max_levels: usize,

    /// Compaction 线程数
    pub compaction_threads: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            level0_max_files: 4,
            level1_max_bytes: 10 * 1024 * 1024, // 10 MB
            level_size_multiplier: 10,
            max_levels: 7,
            compaction_threads: 2,
        }
    }
}

/// SSTable 元信息（用于 compaction 决策）
#[derive(Debug, Clone)]
pub struct SSTableInfo {
    /// 文件路径
    pub file_path: String,

    /// 文件大小（bytes）
    pub file_size: u64,

    /// 最小 key
    pub min_key: Vec<u8>,

    /// 最大 key
    pub max_key: Vec<u8>,

    /// 所属层级
    pub level: usize,

    /// 记录数
    pub entry_count: u64,

    /// 时间范围
    pub min_timestamp: i64,
    pub max_timestamp: i64,
}

impl SSTableInfo {
    /// 检查 key range 是否重叠
    pub fn overlaps(&self, other: &SSTableInfo) -> bool {
        // 如果 self.max_key < other.min_key 或 self.min_key > other.max_key，则不重叠
        !(self.max_key < other.min_key || self.min_key > other.max_key)
    }
}
