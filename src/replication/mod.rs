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
//!
//! ## gRPC 服务启动示例
//!
//! ```rust,no_run
//! use qaexchange::replication::{
//!     GrpcConfig, ReplicationContext, ReplicationServiceImpl,
//!     RoleManager, LogReplicator, ReplicationConfig, NodeRole,
//! };
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let role_mgr = Arc::new(RoleManager::new("node1".to_string(), NodeRole::Master));
//!     let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
//!     let ctx = Arc::new(ReplicationContext::new("node1".to_string(), role_mgr, replicator));
//!
//!     let config = GrpcConfig::default();
//!     ReplicationServiceImpl::serve(ctx, config).await.unwrap();
//! }
//! ```

pub mod failover;
pub mod grpc;
pub mod heartbeat;
pub mod protocol;
pub mod replicator;
pub mod role;
pub mod tls;

pub use failover::FailoverCoordinator;
pub use grpc::{
    ClusterManager, ClusterNode, GrpcConfig, ReplicationClient, ReplicationContext,
    ReplicationServiceImpl, internal_to_proto_log_entry,
    // Proto types re-export
    proto, AppendEntriesRequest, AppendEntriesResponse, HeartbeatRequest, HeartbeatResponse,
    VoteRequest, VoteResponse, SnapshotChunk, SnapshotResponse, NodeStatus, RecordType,
    ReplicationService, ReplicationServiceServer, ReplicationServiceClient,
};
pub use heartbeat::HeartbeatManager;
pub use protocol::{LogEntry, ReplicationMessage, ReplicationRequest, ReplicationResponse};
pub use replicator::{LogReplicator, ReplicationConfig};
pub use role::{NodeRole, RoleManager};
pub use tls::{TlsConfig, TlsConfigBuilder, TlsError, CertificateGenerator, CertificatePaths};
