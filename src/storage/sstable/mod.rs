// SSTable Module - Sorted String Table（持久化存储）
//
// 双体系架构：
// 1. OLTP SSTable (rkyv) - 零拷贝读取
// 2. OLAP SSTable (Parquet) - 列式压缩存储
//
// 高级特性：
// - Block-level 索引: O(log n) 块定位
// - SIMD 优化: 向量化批量操作
// - 按数据类型自动选择最优压缩算法
// - 支持 Uncompressed/Snappy/LZ4/ZSTD 多级别
//
// @yutiansut @quantaxis

pub mod bloom;
pub mod block_index;
pub mod compression;
pub mod mmap_reader;
pub mod olap_parquet;
pub mod oltp_rkyv;
pub mod simd;
pub mod types;

pub use bloom::BloomFilter;
pub use block_index::{BlockIndex, BlockIndexBuilder, BlockIndexConfig, BlockIndexEntry};
pub use compression::{CompressionAlgorithm, CompressionStats, CompressionStrategy, CompressionStrategyBuilder};
pub use mmap_reader::MmapSSTableReader;
pub use olap_parquet::{ParquetSSTable, ParquetSSTableWriter};
pub use oltp_rkyv::RkyvSSTable;
pub use simd::{SimdCapability, detect_simd_capability, batch_timestamp_in_range, batch_sum_f64, batch_max_i64, batch_min_i64, bytes_equal};
pub use types::{SSTableIterator, SSTableMetadata};
