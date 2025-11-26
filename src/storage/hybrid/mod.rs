// Hybrid Storage Module - 混合存储管理器
//
// OLTP 路径：WAL → MemTable → SSTable
// - 实时写入、低延迟查询
// - 品种级并发
//
// BatchDataSource 适配器：
// - 连接 OltpHybridStorage 与 HybridQueryEngine
// - 支持 OLTP + OLAP 混合查询
// - 自动路由（根据时间范围）
//
// @yutiansut @quantaxis

pub mod batch_source;
pub mod oltp;

pub use batch_source::OltpBatchAdapter;
pub use oltp::OltpHybridStorage;
