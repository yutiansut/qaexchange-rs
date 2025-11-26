// SSTable Module - Sorted String Table（持久化存储）
//
// 双体系架构：
// 1. OLTP SSTable (rkyv) - 零拷贝读取
// 2. OLAP SSTable (Parquet) - 列式压缩存储
//
// 压缩策略：
// - 按数据类型自动选择最优压缩算法
// - 支持 Uncompressed/Snappy/LZ4/ZSTD 多级别
//
// @yutiansut @quantaxis

pub mod bloom;
pub mod compression;
pub mod mmap_reader;
pub mod olap_parquet;
pub mod oltp_rkyv;
pub mod types;

pub use bloom::BloomFilter;
pub use compression::{CompressionAlgorithm, CompressionStats, CompressionStrategy, CompressionStrategyBuilder};
pub use mmap_reader::MmapSSTableReader;
pub use olap_parquet::{ParquetSSTable, ParquetSSTableWriter};
pub use oltp_rkyv::RkyvSSTable;
pub use types::{SSTableIterator, SSTableMetadata};
