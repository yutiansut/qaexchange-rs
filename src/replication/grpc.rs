//! gRPC 复制网络层
//!
//! @yutiansut @quantaxis
//!
//! 提供基于 gRPC 的节点间通信：
//! - 日志复制
//! - 心跳检测
//! - 快照传输
//! - 选举投票

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;

use super::role::{NodeRole, RoleManager};
use super::replicator::LogReplicator;

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 消息类型 (手动定义，替代 proto 生成)
// ═══════════════════════════════════════════════════════════════════════════

/// 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub sequence: u64,
    pub term: u64,
    pub record_data: Vec<u8>,
    pub timestamp: i64,
    pub record_type: RecordType,
}

/// 记录类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RecordType {
    Unknown = 0,
    OrderInsert = 1,
    OrderCancel = 2,
    TradeExecuted = 3,
    AccountUpdate = 4,
    PositionUpdate = 5,
    TickData = 6,
    OrderbookSnapshot = 7,
    Checkpoint = 8,
}

/// AppendEntries 请求
#[derive(Debug, Clone)]
pub struct AppendEntriesRequest {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_sequence: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

/// AppendEntries 响应
#[derive(Debug, Clone)]
pub struct AppendEntriesResponse {
    pub term: u64,
    pub success: bool,
    pub match_sequence: u64,
    pub error: String,
}

/// 心跳请求
#[derive(Debug, Clone)]
pub struct HeartbeatRequest {
    pub term: u64,
    pub leader_id: String,
    pub leader_commit: u64,
    pub timestamp: i64,
}

/// 心跳响应
#[derive(Debug, Clone)]
pub struct HeartbeatResponse {
    pub term: u64,
    pub node_id: String,
    pub last_log_sequence: u64,
    pub healthy: bool,
    pub status: Option<NodeStatus>,
}

/// 节点状态
#[derive(Debug, Clone)]
pub struct NodeStatus {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub disk_usage: f32,
    pub pending_logs: u64,
    pub replication_lag_ms: u64,
}

/// 投票请求
#[derive(Debug, Clone)]
pub struct VoteRequest {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_sequence: u64,
    pub last_log_term: u64,
}

/// 投票响应
#[derive(Debug, Clone)]
pub struct VoteResponse {
    pub term: u64,
    pub vote_granted: bool,
    pub voter_id: String,
}

/// 快照块
#[derive(Debug, Clone)]
pub struct SnapshotChunk {
    pub term: u64,
    pub last_included_sequence: u64,
    pub last_included_term: u64,
    pub chunk_index: u64,
    pub total_chunks: u64,
    pub data: Vec<u8>,
    pub is_last: bool,
}

/// 快照响应
#[derive(Debug, Clone)]
pub struct SnapshotResponse {
    pub term: u64,
    pub success: bool,
    pub error: String,
    pub bytes_received: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 服务实现
// ═══════════════════════════════════════════════════════════════════════════

/// 复制服务配置
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    /// 监听地址
    pub listen_addr: SocketAddr,
    /// 连接超时
    pub connect_timeout: Duration,
    /// 请求超时
    pub request_timeout: Duration,
    /// 最大消息大小 (字节)
    pub max_message_size: usize,
    /// 并发流数量
    pub max_concurrent_streams: u32,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9090".parse().unwrap(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_message_size: 64 * 1024 * 1024, // 64MB
            max_concurrent_streams: 100,
        }
    }
}

/// 复制服务上下文
pub struct ReplicationContext {
    /// 节点 ID
    pub node_id: String,
    /// 角色管理器
    pub role_manager: Arc<RoleManager>,
    /// 日志复制器
    pub replicator: Arc<LogReplicator>,
    /// 当前 term
    pub current_term: Arc<RwLock<u64>>,
    /// 已投票给 (在当前 term)
    pub voted_for: Arc<RwLock<Option<String>>>,
}

impl ReplicationContext {
    pub fn new(
        node_id: String,
        role_manager: Arc<RoleManager>,
        replicator: Arc<LogReplicator>,
    ) -> Self {
        Self {
            node_id,
            role_manager,
            replicator,
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
        }
    }
}

/// 复制服务实现
pub struct ReplicationServiceImpl {
    ctx: Arc<ReplicationContext>,
}

impl ReplicationServiceImpl {
    pub fn new(ctx: Arc<ReplicationContext>) -> Self {
        Self { ctx }
    }

    /// 处理 AppendEntries 请求
    pub async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, String> {
        let mut current_term = self.ctx.current_term.write();

        // 检查 term
        if request.term < *current_term {
            return Ok(AppendEntriesResponse {
                term: *current_term,
                success: false,
                match_sequence: 0,
                error: "Term outdated".to_string(),
            });
        }

        // 更新 term
        if request.term > *current_term {
            *current_term = request.term;
            *self.ctx.voted_for.write() = None;
            self.ctx.role_manager.set_role(NodeRole::Slave);
        }

        // TODO: 实现实际的日志复制逻辑
        // 1. 检查 prev_log 一致性
        // 2. 追加日志
        // 3. 更新 commit_index

        Ok(AppendEntriesResponse {
            term: *current_term,
            success: true,
            match_sequence: request.entries.last().map(|e| e.sequence).unwrap_or(0),
            error: String::new(),
        })
    }

    /// 处理心跳请求
    pub async fn heartbeat(
        &self,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, String> {
        let current_term = *self.ctx.current_term.read();

        // 更新 term
        if request.term > current_term {
            *self.ctx.current_term.write() = request.term;
            *self.ctx.voted_for.write() = None;
            self.ctx.role_manager.set_role(NodeRole::Slave);
        }

        // 收集节点状态
        let status = NodeStatus {
            cpu_usage: 0.0,        // TODO: 实际采集
            memory_usage: 0.0,     // TODO: 实际采集
            disk_usage: 0.0,       // TODO: 实际采集
            pending_logs: 0,       // TODO: 从 replicator 获取
            replication_lag_ms: 0, // TODO: 计算
        };

        Ok(HeartbeatResponse {
            term: current_term,
            node_id: self.ctx.node_id.clone(),
            last_log_sequence: 0, // TODO: 从 replicator 获取
            healthy: true,
            status: Some(status),
        })
    }

    /// 处理投票请求
    pub async fn request_vote(
        &self,
        request: VoteRequest,
    ) -> Result<VoteResponse, String> {
        let mut current_term = self.ctx.current_term.write();
        let mut voted_for = self.ctx.voted_for.write();

        // 检查 term
        if request.term < *current_term {
            return Ok(VoteResponse {
                term: *current_term,
                vote_granted: false,
                voter_id: self.ctx.node_id.clone(),
            });
        }

        // 更新 term
        if request.term > *current_term {
            *current_term = request.term;
            *voted_for = None;
            self.ctx.role_manager.set_role(NodeRole::Slave);
        }

        // 检查是否已投票
        let vote_granted = match &*voted_for {
            None => {
                // 未投票，检查日志是否足够新
                // TODO: 实际检查日志
                *voted_for = Some(request.candidate_id.clone());
                true
            }
            Some(id) => id == &request.candidate_id,
        };

        Ok(VoteResponse {
            term: *current_term,
            vote_granted,
            voter_id: self.ctx.node_id.clone(),
        })
    }

    /// 处理快照安装
    pub async fn install_snapshot(
        &self,
        chunks: Vec<SnapshotChunk>,
    ) -> Result<SnapshotResponse, String> {
        let mut total_bytes = 0u64;

        for chunk in chunks {
            // TODO: 实际存储快照数据
            total_bytes += chunk.data.len() as u64;
        }

        Ok(SnapshotResponse {
            term: *self.ctx.current_term.read(),
            success: true,
            error: String::new(),
            bytes_received: total_bytes,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 客户端
// ═══════════════════════════════════════════════════════════════════════════

/// 复制客户端
pub struct ReplicationClient {
    /// 目标地址
    target_addr: String,
    /// 配置
    config: GrpcConfig,
}

impl ReplicationClient {
    pub fn new(target_addr: String, config: GrpcConfig) -> Self {
        Self {
            target_addr,
            config,
        }
    }

    /// 发送 AppendEntries
    pub async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, String> {
        // TODO: 使用 tonic 客户端发送请求
        // 当前返回模拟响应
        Err("Not implemented - requires tonic codegen".to_string())
    }

    /// 发送心跳
    pub async fn heartbeat(
        &self,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, String> {
        // TODO: 使用 tonic 客户端发送请求
        Err("Not implemented - requires tonic codegen".to_string())
    }

    /// 请求投票
    pub async fn request_vote(
        &self,
        request: VoteRequest,
    ) -> Result<VoteResponse, String> {
        // TODO: 使用 tonic 客户端发送请求
        Err("Not implemented - requires tonic codegen".to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 节点管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 集群节点
#[derive(Debug, Clone)]
pub struct ClusterNode {
    pub id: String,
    pub addr: String,
    pub is_active: bool,
    pub last_heartbeat: i64,
    pub match_index: u64,
    pub next_index: u64,
}

/// 集群管理器
pub struct ClusterManager {
    /// 本节点 ID
    node_id: String,
    /// 集群节点
    nodes: Arc<RwLock<Vec<ClusterNode>>>,
    /// 复制客户端缓存
    clients: Arc<dashmap::DashMap<String, Arc<ReplicationClient>>>,
    /// gRPC 配置
    config: GrpcConfig,
}

impl ClusterManager {
    pub fn new(node_id: String, config: GrpcConfig) -> Self {
        Self {
            node_id,
            nodes: Arc::new(RwLock::new(Vec::new())),
            clients: Arc::new(dashmap::DashMap::new()),
            config,
        }
    }

    /// 添加节点
    pub fn add_node(&self, node: ClusterNode) {
        let mut nodes = self.nodes.write();
        if !nodes.iter().any(|n| n.id == node.id) {
            nodes.push(node);
        }
    }

    /// 移除节点
    pub fn remove_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write();
        nodes.retain(|n| n.id != node_id);
        self.clients.remove(node_id);
    }

    /// 获取所有节点
    pub fn get_nodes(&self) -> Vec<ClusterNode> {
        self.nodes.read().clone()
    }

    /// 获取活跃节点
    pub fn get_active_nodes(&self) -> Vec<ClusterNode> {
        self.nodes
            .read()
            .iter()
            .filter(|n| n.is_active && n.id != self.node_id)
            .cloned()
            .collect()
    }

    /// 获取或创建客户端
    pub fn get_client(&self, node_id: &str) -> Option<Arc<ReplicationClient>> {
        // 先检查缓存
        if let Some(client) = self.clients.get(node_id) {
            return Some(Arc::clone(&client));
        }

        // 查找节点地址
        let addr = self.nodes.read().iter().find(|n| n.id == node_id)?.addr.clone();

        // 创建新客户端
        let client = Arc::new(ReplicationClient::new(addr, self.config.clone()));
        self.clients.insert(node_id.to_string(), Arc::clone(&client));

        Some(client)
    }

    /// 广播 AppendEntries 到所有节点
    pub async fn broadcast_append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Vec<(String, Result<AppendEntriesResponse, String>)> {
        let nodes = self.get_active_nodes();
        let mut results = Vec::with_capacity(nodes.len());

        for node in nodes {
            if let Some(client) = self.get_client(&node.id) {
                let result = client.append_entries(request.clone()).await;
                results.push((node.id, result));
            }
        }

        results
    }

    /// 广播心跳
    pub async fn broadcast_heartbeat(
        &self,
        request: HeartbeatRequest,
    ) -> Vec<(String, Result<HeartbeatResponse, String>)> {
        let nodes = self.get_active_nodes();
        let mut results = Vec::with_capacity(nodes.len());

        for node in nodes {
            if let Some(client) = self.get_client(&node.id) {
                let result = client.heartbeat(request.clone()).await;
                results.push((node.id, result));
            }
        }

        results
    }

    /// 更新节点状态
    pub fn update_node_status(&self, node_id: &str, is_active: bool, heartbeat_ts: i64) {
        let mut nodes = self.nodes.write();
        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
            node.is_active = is_active;
            node.last_heartbeat = heartbeat_ts;
        }
    }

    /// 更新复制进度
    pub fn update_replication_progress(&self, node_id: &str, match_index: u64) {
        let mut nodes = self.nodes.write();
        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
            node.match_index = match_index;
            node.next_index = match_index + 1;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_manager() {
        let manager = ClusterManager::new("node1".to_string(), GrpcConfig::default());

        // 添加节点
        manager.add_node(ClusterNode {
            id: "node2".to_string(),
            addr: "127.0.0.1:9091".to_string(),
            is_active: true,
            last_heartbeat: 0,
            match_index: 0,
            next_index: 1,
        });

        manager.add_node(ClusterNode {
            id: "node3".to_string(),
            addr: "127.0.0.1:9092".to_string(),
            is_active: true,
            last_heartbeat: 0,
            match_index: 0,
            next_index: 1,
        });

        assert_eq!(manager.get_nodes().len(), 2);
        assert_eq!(manager.get_active_nodes().len(), 2);

        // 移除节点
        manager.remove_node("node2");
        assert_eq!(manager.get_nodes().len(), 1);
    }

    #[test]
    fn test_grpc_config() {
        let config = GrpcConfig::default();
        assert_eq!(config.max_message_size, 64 * 1024 * 1024);
        assert_eq!(config.max_concurrent_streams, 100);
    }
}
