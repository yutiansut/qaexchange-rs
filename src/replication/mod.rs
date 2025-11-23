//! 主从复制模块
//!
//! @yutiansut @quantaxis
//!
//! 实现高可用架构：
//! - Master-Slave 复制
//! - 自动故障转移
//! - 数据一致性保证
//! - gRPC 网络层通信
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

pub mod failover;
pub mod grpc;
pub mod heartbeat;
pub mod protocol;
pub mod replicator;
pub mod role;

pub use failover::FailoverCoordinator;
pub use grpc::{
    ClusterManager, ClusterNode, GrpcConfig, ReplicationClient, ReplicationContext,
    ReplicationServiceImpl,
};
pub use heartbeat::HeartbeatManager;
pub use protocol::{LogEntry, ReplicationMessage, ReplicationRequest, ReplicationResponse};
pub use replicator::{LogReplicator, ReplicationConfig};
pub use role::{NodeRole, RoleManager};
