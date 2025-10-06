// SSTable 类型定义

use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};
use super::bloom::BloomFilter;

/// SSTable 元数据
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct SSTableMetadata {
    /// SSTable 文件版本
    pub version: u32,

    /// 记录数量
    pub entry_count: u64,

    /// 最小时间戳（纳秒）
    pub min_timestamp: i64,

    /// 最大时间戳（纳秒）
    pub max_timestamp: i64,

    /// 最小 key（用于 compaction）
    pub min_key: Vec<u8>,

    /// 最大 key（用于 compaction）
    pub max_key: Vec<u8>,

    /// 文件大小（字节）
    pub file_size: u64,

    /// 数据块偏移量列表（用于快速定位）
    pub block_offsets: Vec<u64>,

    /// Bloom Filter（可选，用于快速判断 key 是否存在）
    pub bloom_filter: Option<BloomFilter>,

    /// 创建时间戳（Unix 秒）
    pub created_at: i64,
}

impl SSTableMetadata {
    pub fn new(entry_count: u64, min_timestamp: i64, max_timestamp: i64) -> Self {
        Self {
            version: 1,
            entry_count,
            min_timestamp,
            max_timestamp,
            min_key: Vec::new(),
            max_key: Vec::new(),
            file_size: 0,
            block_offsets: Vec::new(),
            bloom_filter: None,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn with_key_range(mut self, min_key: Vec<u8>, max_key: Vec<u8>) -> Self {
        self.min_key = min_key;
        self.max_key = max_key;
        self
    }

    pub fn with_bloom_filter(mut self, bloom_filter: BloomFilter) -> Self {
        self.bloom_filter = Some(bloom_filter);
        self
    }
}

/// SSTable 迭代器（抽象接口）
/// 暂时未使用，保留用于未来扩展
#[allow(dead_code)]
pub struct SSTableIterator;
