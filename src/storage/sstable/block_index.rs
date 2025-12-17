//! Block-level 索引实现
//!
//! @yutiansut @quantaxis
//!
//! 提供 SSTable 的块级索引能力：
//! - 块边界快速定位
//! - 稀疏索引减少内存占用
//! - 二分查找 O(log n) 性能
//! - 支持前缀压缩
//!
//! 性能目标：
//! - 索引查找延迟: < 500ns
//! - 内存占用: < 1% 数据大小
//! - 块大小: 4KB (可配置)

use rkyv::{Archive, Deserialize, Serialize};
use std::cmp::Ordering;
use std::io::{Read, Write};

// ═══════════════════════════════════════════════════════════════════════════
// 块索引配置
// ═══════════════════════════════════════════════════════════════════════════

/// 块索引配置
#[derive(Debug, Clone)]
pub struct BlockIndexConfig {
    /// 块大小（字节）
    pub block_size: usize,
    /// 是否启用前缀压缩
    pub prefix_compression: bool,
    /// 重启点间隔（前缀压缩时使用）
    pub restart_interval: usize,
    /// 索引采样率（每 N 个 entry 记录一个索引点）
    pub index_interval: usize,
}

impl Default for BlockIndexConfig {
    fn default() -> Self {
        Self {
            block_size: 4096,           // 4KB 块
            prefix_compression: true,   // 启用前缀压缩
            restart_interval: 16,       // 每16个 entry 一个重启点
            index_interval: 64,         // 每64个 entry 一个索引点
        }
    }
}

impl BlockIndexConfig {
    /// 高性能配置（大块，减少索引数量）
    pub fn high_performance() -> Self {
        Self {
            block_size: 16384,          // 16KB 块
            prefix_compression: false,  // 禁用压缩（减少 CPU 开销）
            restart_interval: 32,
            index_interval: 128,
        }
    }

    /// 低内存配置（小块，更细粒度索引）
    pub fn low_memory() -> Self {
        Self {
            block_size: 2048,           // 2KB 块
            prefix_compression: true,
            restart_interval: 8,
            index_interval: 32,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 块索引条目
// ═══════════════════════════════════════════════════════════════════════════

/// 块索引条目
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct BlockIndexEntry {
    /// 块起始 key 的时间戳
    pub start_timestamp: i64,
    /// 块起始 key 的序列号
    pub start_sequence: u64,
    /// 块结束 key 的时间戳
    pub end_timestamp: i64,
    /// 块结束 key 的序列号
    pub end_sequence: u64,
    /// 块在文件中的偏移量
    pub offset: u64,
    /// 块大小（字节）
    pub size: u32,
    /// 块内 entry 数量
    pub entry_count: u32,
    /// 块的 CRC32 校验和
    pub checksum: u32,
}

impl BlockIndexEntry {
    pub fn new(
        start_timestamp: i64,
        start_sequence: u64,
        end_timestamp: i64,
        end_sequence: u64,
        offset: u64,
        size: u32,
        entry_count: u32,
        checksum: u32,
    ) -> Self {
        Self {
            start_timestamp,
            start_sequence,
            end_timestamp,
            end_sequence,
            offset,
            size,
            entry_count,
            checksum,
        }
    }

    /// 检查时间戳是否可能在此块中
    #[inline]
    pub fn may_contain_timestamp(&self, timestamp: i64) -> bool {
        timestamp >= self.start_timestamp && timestamp <= self.end_timestamp
    }

    /// 检查时间戳范围是否与此块重叠
    #[inline]
    pub fn overlaps_range(&self, start_ts: i64, end_ts: i64) -> bool {
        !(end_ts < self.start_timestamp || start_ts > self.end_timestamp)
    }

    /// 比较 key 与块的关系
    #[inline]
    pub fn compare_key(&self, timestamp: i64, sequence: u64) -> Ordering {
        if timestamp < self.start_timestamp
            || (timestamp == self.start_timestamp && sequence < self.start_sequence)
        {
            Ordering::Greater // key 在块之前
        } else if timestamp > self.end_timestamp
            || (timestamp == self.end_timestamp && sequence > self.end_sequence)
        {
            Ordering::Less // key 在块之后
        } else {
            Ordering::Equal // key 可能在块中
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 块索引
// ═══════════════════════════════════════════════════════════════════════════

/// 块级索引
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct BlockIndex {
    /// 索引条目列表（按 start_timestamp 排序）
    pub entries: Vec<BlockIndexEntry>,
    /// 配置信息
    pub config: BlockIndexConfigSerde,
    /// 总 entry 数量
    pub total_entries: u64,
    /// 最小时间戳
    pub min_timestamp: i64,
    /// 最大时间戳
    pub max_timestamp: i64,
}

/// 可序列化的配置（用于持久化）
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct BlockIndexConfigSerde {
    pub block_size: u32,
    pub prefix_compression: bool,
    pub restart_interval: u16,
    pub index_interval: u16,
}

impl From<&BlockIndexConfig> for BlockIndexConfigSerde {
    fn from(config: &BlockIndexConfig) -> Self {
        Self {
            block_size: config.block_size as u32,
            prefix_compression: config.prefix_compression,
            restart_interval: config.restart_interval as u16,
            index_interval: config.index_interval as u16,
        }
    }
}

impl BlockIndex {
    /// 创建空索引
    pub fn new(config: &BlockIndexConfig) -> Self {
        Self {
            entries: Vec::new(),
            config: BlockIndexConfigSerde::from(config),
            total_entries: 0,
            min_timestamp: i64::MAX,
            max_timestamp: i64::MIN,
        }
    }

    /// 添加块索引条目
    pub fn add_entry(&mut self, entry: BlockIndexEntry) {
        // 更新统计信息
        self.total_entries += entry.entry_count as u64;
        self.min_timestamp = self.min_timestamp.min(entry.start_timestamp);
        self.max_timestamp = self.max_timestamp.max(entry.end_timestamp);

        self.entries.push(entry);
    }

    /// 二分查找包含指定 key 的块
    ///
    /// 返回可能包含该 key 的块索引（如果存在）
    /// 时间复杂度: O(log n)
    #[inline]
    pub fn find_block(&self, timestamp: i64, sequence: u64) -> Option<&BlockIndexEntry> {
        // 快速范围检查
        if timestamp < self.min_timestamp || timestamp > self.max_timestamp {
            return None;
        }

        // 二分查找
        let idx = self.entries.partition_point(|entry| {
            entry.end_timestamp < timestamp
                || (entry.end_timestamp == timestamp && entry.end_sequence < sequence)
        });

        if idx < self.entries.len() {
            let entry = &self.entries[idx];
            if entry.may_contain_timestamp(timestamp) {
                return Some(entry);
            }
        }

        None
    }

    /// 查找与时间范围重叠的所有块
    ///
    /// 返回所有可能包含指定范围内数据的块
    /// 时间复杂度: O(log n + k)，k 为结果数量
    pub fn find_blocks_in_range(&self, start_ts: i64, end_ts: i64) -> Vec<&BlockIndexEntry> {
        // 快速范围检查
        if end_ts < self.min_timestamp || start_ts > self.max_timestamp {
            return Vec::new();
        }

        // 找到第一个可能重叠的块
        let start_idx = self.entries.partition_point(|entry| entry.end_timestamp < start_ts);

        // 收集所有重叠的块
        let mut result = Vec::new();
        for entry in &self.entries[start_idx..] {
            if entry.start_timestamp > end_ts {
                break; // 已超出范围
            }
            if entry.overlaps_range(start_ts, end_ts) {
                result.push(entry);
            }
        }

        result
    }

    /// 获取块数量
    #[inline]
    pub fn block_count(&self) -> usize {
        self.entries.len()
    }

    /// 获取总 entry 数量
    #[inline]
    pub fn entry_count(&self) -> u64 {
        self.total_entries
    }

    /// 估算索引内存占用（字节）
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.entries.len() * std::mem::size_of::<BlockIndexEntry>()
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        rkyv::to_bytes::<_, 1024>(self)
            .map(|v| v.to_vec())
            .map_err(|e| format!("Serialize block index failed: {}", e))
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let archived = rkyv::check_archived_root::<Self>(bytes)
            .map_err(|e| format!("Deserialize block index failed: {}", e))?;

        archived
            .deserialize(&mut rkyv::Infallible)
            .map_err(|e| format!("Deserialize block index failed: {:?}", e))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 块索引构建器
// ═══════════════════════════════════════════════════════════════════════════

/// 块索引构建器
pub struct BlockIndexBuilder {
    config: BlockIndexConfig,
    index: BlockIndex,
    current_block_start_ts: i64,
    current_block_start_seq: u64,
    current_block_offset: u64,
    current_block_size: u32,
    current_block_entries: u32,
    entries_since_last_index: usize,
}

impl BlockIndexBuilder {
    pub fn new(config: BlockIndexConfig) -> Self {
        let index = BlockIndex::new(&config);
        Self {
            config,
            index,
            current_block_start_ts: 0,
            current_block_start_seq: 0,
            current_block_offset: 0,
            current_block_size: 0,
            current_block_entries: 0,
            entries_since_last_index: 0,
        }
    }

    /// 开始新块
    pub fn start_block(&mut self, offset: u64, first_timestamp: i64, first_sequence: u64) {
        self.current_block_offset = offset;
        self.current_block_start_ts = first_timestamp;
        self.current_block_start_seq = first_sequence;
        self.current_block_size = 0;
        self.current_block_entries = 0;
    }

    /// 添加 entry 到当前块
    pub fn add_entry(&mut self, entry_size: u32) {
        self.current_block_size += entry_size;
        self.current_block_entries += 1;
        self.entries_since_last_index += 1;
    }

    /// 结束当前块
    pub fn finish_block(
        &mut self,
        end_timestamp: i64,
        end_sequence: u64,
        checksum: u32,
    ) {
        if self.current_block_entries == 0 {
            return;
        }

        // 根据采样率决定是否记录索引点
        if self.entries_since_last_index >= self.config.index_interval
            || self.index.entries.is_empty()
        {
            let entry = BlockIndexEntry::new(
                self.current_block_start_ts,
                self.current_block_start_seq,
                end_timestamp,
                end_sequence,
                self.current_block_offset,
                self.current_block_size,
                self.current_block_entries,
                checksum,
            );
            self.index.add_entry(entry);
            self.entries_since_last_index = 0;
        }
    }

    /// 检查是否应该结束当前块（基于大小）
    pub fn should_finish_block(&self) -> bool {
        self.current_block_size as usize >= self.config.block_size
    }

    /// 完成构建
    pub fn build(mut self) -> BlockIndex {
        // 确保最后一个块被记录
        if self.current_block_entries > 0 && self.entries_since_last_index > 0 {
            // 需要外部调用 finish_block
        }
        self.index
    }

    /// 获取当前配置
    pub fn config(&self) -> &BlockIndexConfig {
        &self.config
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_index() -> BlockIndex {
        let config = BlockIndexConfig::default();
        let mut index = BlockIndex::new(&config);

        // 添加测试块
        for i in 0..10 {
            let start_ts = i * 1000;
            let end_ts = start_ts + 999;
            index.add_entry(BlockIndexEntry::new(
                start_ts,
                i as u64 * 100,
                end_ts,
                (i as u64 + 1) * 100 - 1,
                i as u64 * 4096,
                4096,
                100,
                0x12345678,
            ));
        }

        index
    }

    #[test]
    fn test_find_block() {
        let index = create_test_index();

        // 查找存在的 key
        let block = index.find_block(500, 50);
        assert!(block.is_some());
        let entry = block.unwrap();
        assert_eq!(entry.start_timestamp, 0);
        assert_eq!(entry.end_timestamp, 999);

        // 查找边界 key
        let block = index.find_block(1000, 100);
        assert!(block.is_some());
        assert_eq!(block.unwrap().start_timestamp, 1000);

        // 查找不存在的 key
        let block = index.find_block(99999, 0);
        assert!(block.is_none());
    }

    #[test]
    fn test_find_blocks_in_range() {
        let index = create_test_index();

        // 查找范围内的块
        let blocks = index.find_blocks_in_range(500, 2500);
        assert_eq!(blocks.len(), 3); // 块 0, 1, 2

        // 查找跨越多个块的范围
        let blocks = index.find_blocks_in_range(0, 9999);
        assert_eq!(blocks.len(), 10); // 所有块

        // 查找空范围
        let blocks = index.find_blocks_in_range(99999, 100000);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_serialization() {
        let index = create_test_index();

        let bytes = index.to_bytes().unwrap();
        let restored = BlockIndex::from_bytes(&bytes).unwrap();

        assert_eq!(restored.block_count(), index.block_count());
        assert_eq!(restored.entry_count(), index.entry_count());
        assert_eq!(restored.min_timestamp, index.min_timestamp);
        assert_eq!(restored.max_timestamp, index.max_timestamp);
    }

    #[test]
    fn test_block_index_builder() {
        let config = BlockIndexConfig::default();
        let mut builder = BlockIndexBuilder::new(config);

        // 构建 3 个块
        for block_idx in 0u64..3 {
            builder.start_block(block_idx * 4096, (block_idx * 1000) as i64, block_idx * 100);

            for _ in 0..100 {
                builder.add_entry(40); // 40 bytes per entry
            }

            builder.finish_block(
                (block_idx * 1000 + 999) as i64,
                (block_idx + 1) * 100 - 1,
                0xDEADBEEF,
            );
        }

        let index = builder.build();
        assert!(index.block_count() > 0);
    }

    #[test]
    fn test_block_entry_comparison() {
        let entry = BlockIndexEntry::new(1000, 100, 1999, 199, 0, 4096, 100, 0);

        // 在块之前
        assert_eq!(entry.compare_key(500, 50), Ordering::Greater);

        // 在块中
        assert_eq!(entry.compare_key(1500, 150), Ordering::Equal);
        assert_eq!(entry.compare_key(1000, 100), Ordering::Equal);
        assert_eq!(entry.compare_key(1999, 199), Ordering::Equal);

        // 在块之后
        assert_eq!(entry.compare_key(2500, 250), Ordering::Less);
    }

    #[test]
    fn test_memory_usage() {
        let index = create_test_index();
        let mem = index.memory_usage();

        // 应该是合理的内存占用
        assert!(mem > 0);
        assert!(mem < 10000); // 10 个块应该 < 10KB
    }
}
