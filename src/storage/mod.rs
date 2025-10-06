//! 持久化存储模块

pub use qars::qaconnector;

// WAL (Write-Ahead Log) 模块
pub mod wal;

// MemTable 模块（内存表）
pub mod memtable;

// SSTable 模块（持久化表）
pub mod sstable;

// Hybrid Storage 模块（混合存储）
pub mod hybrid;

// Compaction 模块（压缩合并）
pub mod compaction;

// Checkpoint 模块（故障恢复）
pub mod checkpoint;

// WAL Recovery 模块（WAL恢复）
pub mod recovery;

// OLTP → OLAP 转换模块
pub mod conversion;

// Storage Subscriber (异步持久化)
pub mod subscriber;
