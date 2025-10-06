// MemTable Module - 内存表
//
// 双体系架构：
// 1. OLTP MemTable (SkipMap) - 低延迟写入
// 2. OLAP MemTable (Arrow2) - 高效查询

pub mod oltp;
pub mod olap;
pub mod types;

pub use oltp::OltpMemTable;
pub use olap::OlapMemTable;
pub use types::{MemTableKey, MemTableValue, MemTableEntry};
