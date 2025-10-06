// WAL (Write-Ahead Log) Module
//
// 高性能 Write-Ahead Log 实现
// - 性能目标: 写入延迟 P99 < 1ms
// - 吞吐量: > 100K entries/s（单品种）
// - 品种级并发: > 100K × N entries/s（N = 活跃品种数）
// - 恢复速度: > 1GB/s

pub mod record;
pub mod manager;
pub mod per_instrument;

pub use record::{WalRecord, WalEntry};
pub use manager::WalManager;
pub use per_instrument::PerInstrumentWalManager;
