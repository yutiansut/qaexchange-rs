//! 主从复制模块
//!
//! 实现高可用架构：
//! - Master-Slave 复制
//! - 自动故障转移
//! - 数据一致性保证
//!
//! 架构：
//! ```text
//! Master                  Slave 1              Slave 2
//!   |                        |                    |
//!   |------ Log Entry ------>|                    |
//!   |------ Log Entry --------------------->     |
//!   |                        |                    |
//!   |<----- ACK -------------|                    |
//!   |<----- ACK ----------------------------|     |
//! ```

pub mod protocol;
pub mod replicator;
pub mod role;
pub mod heartbeat;
pub mod failover;

pub use protocol::{ReplicationMessage, LogEntry, ReplicationRequest, ReplicationResponse};
pub use replicator::{LogReplicator, ReplicationConfig};
pub use role::{NodeRole, RoleManager};
pub use heartbeat::HeartbeatManager;
pub use failover::FailoverCoordinator;
