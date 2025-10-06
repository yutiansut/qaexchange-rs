//! Checkpoint 模块 - 定期保存系统状态快照
//!
//! Checkpoint 用于加速故障恢复：
//! - 定期保存 MemTable 到 SSTable
//! - 清理已持久化的 WAL
//! - 记录恢复点
//!
//! 恢复流程：
//! 1. 从最近的 Checkpoint 加载 SSTable
//! 2. 回放 Checkpoint 之后的 WAL
//! 3. 重建 MemTable

pub mod manager;
pub mod types;

pub use manager::CheckpointManager;
pub use types::{CheckpointMetadata, CheckpointInfo};
