//! 集群管理模块
//!
//! @yutiansut @quantaxis
//!
//! 提供分布式集群能力：
//! - 一致性哈希分片路由
//! - 节点发现与管理
//! - 负载均衡
//! - 数据迁移

pub mod consistent_hash;

pub use consistent_hash::{
    ConsistentHashRing, PhysicalNode, ShardConfig, ShardKeyType, ShardRouter, ShardStats,
};
