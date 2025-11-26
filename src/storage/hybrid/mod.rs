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
// QueryFilter：
// - 高性能零拷贝过滤器
// - 位掩码类型过滤（O(1)）
// - 支持时间/类型/合约/价格多维过滤
//
// @yutiansut @quantaxis

pub mod batch_source;
pub mod oltp;
pub mod query_filter;

pub use batch_source::OltpBatchAdapter;
pub use oltp::OltpHybridStorage;
pub use query_filter::{QueryFilter, RecordType, RecordTypeSet, RecordCategory};
