// SSTable Module - Sorted String Table（持久化存储）
//
// 双体系架构：
// 1. OLTP SSTable (rkyv) - 零拷贝读取
// 2. OLAP SSTable (Parquet) - 列式压缩存储

pub mod oltp_rkyv;
pub mod olap_parquet;
pub mod types;
pub mod bloom;
pub mod mmap_reader;

pub use oltp_rkyv::RkyvSSTable;
pub use olap_parquet::{ParquetSSTable, ParquetSSTableWriter};
pub use types::{SSTableMetadata, SSTableIterator};
pub use bloom::BloomFilter;
pub use mmap_reader::MmapSSTableReader;
