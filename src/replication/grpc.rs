//! gRPC 复制网络层
//!
//! @yutiansut @quantaxis
//!
//! 提供基于 gRPC 的节点间通信：
//! - 日志复制 (高性能批量传输)
//! - 心跳检测 (亚毫秒级延迟)
//! - 快照传输 (流式分块)
//! - 选举投票 (Raft 协议)
//!
//! 性能目标：
//! - 日志复制延迟: P99 < 10ms
//! - 心跳延迟: P99 < 5ms
//! - 吞吐量: > 100K ops/s

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use sysinfo::{Disks, System};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

use super::protocol::{LogEntry as InternalLogEntry, ReplicationRequest as InternalReplicationRequest};
use super::replicator::LogReplicator;
use super::role::{NodeRole, RoleManager};

// ═══════════════════════════════════════════════════════════════════════════
// Proto 生成模块 (tonic 自动生成)
// ═══════════════════════════════════════════════════════════════════════════

pub mod proto {
    tonic::include_proto!("qaexchange.replication");
}

pub use proto::replication_service_client::ReplicationServiceClient;
pub use proto::replication_service_server::{ReplicationService, ReplicationServiceServer};
pub use proto::{
    AppendEntriesRequest, AppendEntriesResponse, HeartbeatRequest, HeartbeatResponse,
    LogEntry, NodeStatus, RecordType, SnapshotChunk, SnapshotResponse, VoteRequest, VoteResponse,
};

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 服务配置
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
    /// 心跳间隔
    pub heartbeat_interval: Duration,
    /// 重试次数
    pub max_retries: u32,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9090".parse().unwrap(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_message_size: 64 * 1024 * 1024, // 64MB
            max_concurrent_streams: 100,
            heartbeat_interval: Duration::from_millis(100),
            max_retries: 3,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 复制上下文 (共享状态)
// ═══════════════════════════════════════════════════════════════════════════

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
    /// 已提交的日志序列号
    pub commit_index: Arc<RwLock<u64>>,
    /// 最后应用的日志序列号
    pub last_applied: Arc<RwLock<u64>>,
    /// 日志存储 (sequence -> LogEntry)
    log_store: Arc<RwLock<Vec<InternalLogEntry>>>,
    /// 系统信息采集器
    sys_info: Arc<RwLock<System>>,
    /// 快照存储路径 @yutiansut @quantaxis
    snapshot_dir: std::path::PathBuf,
}

impl ReplicationContext {
    pub fn new(
        node_id: String,
        role_manager: Arc<RoleManager>,
        replicator: Arc<LogReplicator>,
    ) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        // 创建快照存储目录 @yutiansut @quantaxis
        let snapshot_dir = std::path::PathBuf::from(format!("data/snapshots/{}", node_id));
        if let Err(e) = std::fs::create_dir_all(&snapshot_dir) {
            log::warn!("Failed to create snapshot directory: {}", e);
        }

        Self {
            node_id,
            role_manager,
            replicator,
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            commit_index: Arc::new(RwLock::new(0)),
            last_applied: Arc::new(RwLock::new(0)),
            log_store: Arc::new(RwLock::new(Vec::new())),
            sys_info: Arc::new(RwLock::new(sys)),
            snapshot_dir,
        }
    }

    /// 带自定义快照路径的构造函数 @yutiansut @quantaxis
    pub fn with_snapshot_dir(
        node_id: String,
        role_manager: Arc<RoleManager>,
        replicator: Arc<LogReplicator>,
        snapshot_dir: std::path::PathBuf,
    ) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        if let Err(e) = std::fs::create_dir_all(&snapshot_dir) {
            log::warn!("Failed to create snapshot directory: {}", e);
        }

        Self {
            node_id,
            role_manager,
            replicator,
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            commit_index: Arc::new(RwLock::new(0)),
            last_applied: Arc::new(RwLock::new(0)),
            log_store: Arc::new(RwLock::new(Vec::new())),
            sys_info: Arc::new(RwLock::new(sys)),
            snapshot_dir,
        }
    }

    /// 写入快照数据块 @yutiansut @quantaxis
    pub fn write_snapshot_chunk(
        &self,
        chunk_index: u64,
        data: &[u8],
        is_last: bool,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;

        let snapshot_file = self.snapshot_dir.join("snapshot.dat");
        let mut file = if chunk_index == 0 {
            // 第一个块：创建新文件
            std::fs::File::create(&snapshot_file)?
        } else {
            // 后续块：追加写入
            std::fs::OpenOptions::new()
                .append(true)
                .open(&snapshot_file)?
        };

        file.write_all(data)?;
        file.flush()?;

        if is_last {
            // 最后一块：写入元数据
            let meta_file = self.snapshot_dir.join("snapshot.meta");
            let meta = format!(
                "term={}\nsequence={}\ntimestamp={}\n",
                *self.current_term.read(),
                *self.commit_index.read(),
                chrono::Utc::now().timestamp()
            );
            std::fs::write(meta_file, meta)?;
            log::info!(
                "[{}] Snapshot written to {:?}",
                self.node_id,
                snapshot_file
            );
        }

        Ok(())
    }

    /// 获取快照文件路径 @yutiansut @quantaxis
    pub fn get_snapshot_path(&self) -> std::path::PathBuf {
        self.snapshot_dir.join("snapshot.dat")
    }

    /// 获取最后一条日志的序列号和 term
    pub fn get_last_log_info(&self) -> (u64, u64) {
        let logs = self.log_store.read();
        logs.last()
            .map(|entry| (entry.sequence, entry.term))
            .unwrap_or((0, 0))
    }

    /// 检查日志一致性
    pub fn check_log_consistency(&self, prev_sequence: u64, prev_term: u64) -> bool {
        if prev_sequence == 0 {
            return true; // 空日志
        }

        let logs = self.log_store.read();
        logs.iter()
            .find(|e| e.sequence == prev_sequence)
            .map(|e| e.term == prev_term)
            .unwrap_or(false)
    }

    /// 追加日志条目
    pub fn append_entries(&self, entries: Vec<InternalLogEntry>) -> u64 {
        let mut logs = self.log_store.write();
        let mut last_sequence = 0;

        for entry in entries {
            // 删除冲突的日志
            logs.retain(|e| e.sequence < entry.sequence || e.term == entry.term);
            last_sequence = entry.sequence;
            logs.push(entry);
        }

        // 保持日志有序
        logs.sort_by_key(|e| e.sequence);
        last_sequence
    }

    /// 更新提交索引
    pub fn update_commit_index(&self, leader_commit: u64) {
        let (last_sequence, _) = self.get_last_log_info();
        let new_commit = leader_commit.min(last_sequence);

        let mut commit = self.commit_index.write();
        if new_commit > *commit {
            *commit = new_commit;
            log::info!("[{}] Commit index updated to {}", self.node_id, new_commit);
        }
    }

    /// 采集系统状态
    pub fn collect_node_status(&self) -> NodeStatus {
        let mut sys = self.sys_info.write();
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu_usage = sys.global_cpu_usage();
        let total_memory = sys.total_memory() as f32;
        let used_memory = sys.used_memory() as f32;
        let memory_usage = if total_memory > 0.0 {
            (used_memory / total_memory) * 100.0
        } else {
            0.0
        };

        // ✨ 采集磁盘使用率 (所有磁盘的平均使用率) @yutiansut @quantaxis
        let disks = Disks::new_with_refreshed_list();
        let disk_usage = if disks.list().is_empty() {
            0.0
        } else {
            let mut total_space: u64 = 0;
            let mut available_space: u64 = 0;
            for disk in disks.list() {
                total_space += disk.total_space();
                available_space += disk.available_space();
            }
            if total_space > 0 {
                let used_space = total_space - available_space;
                (used_space as f64 / total_space as f64 * 100.0) as f32
            } else {
                0.0
            }
        };

        // 获取待复制日志数量
        let pending_logs = self.replicator.pending_count() as u64;

        // 计算复制延迟 (基于 commit_index 和 last_applied 的差距)
        let commit = *self.commit_index.read();
        let applied = *self.last_applied.read();
        let replication_lag_ms = (commit.saturating_sub(applied)) * 10; // 估算

        NodeStatus {
            cpu_usage,
            memory_usage,
            disk_usage,
            pending_logs,
            replication_lag_ms,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 服务实现 (服务端)
// ═══════════════════════════════════════════════════════════════════════════

/// 复制服务实现
pub struct ReplicationServiceImpl {
    ctx: Arc<ReplicationContext>,
}

impl ReplicationServiceImpl {
    pub fn new(ctx: Arc<ReplicationContext>) -> Self {
        Self { ctx }
    }

    /// 启动 gRPC 服务器
    pub async fn serve(ctx: Arc<ReplicationContext>, config: GrpcConfig) -> Result<(), tonic::transport::Error> {
        let service = ReplicationServiceImpl::new(ctx);

        log::info!("Starting gRPC replication server at {}", config.listen_addr);

        Server::builder()
            .max_frame_size(Some(config.max_message_size as u32))
            .concurrency_limit_per_connection(config.max_concurrent_streams as usize)
            .add_service(ReplicationServiceServer::new(service))
            .serve(config.listen_addr)
            .await
    }
}

#[tonic::async_trait]
impl ReplicationService for ReplicationServiceImpl {
    /// 处理 AppendEntries 请求 (日志复制)
    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesResponse>, Status> {
        let req = request.into_inner();
        let mut current_term = self.ctx.current_term.write();

        // 1. 检查 term
        if req.term < *current_term {
            return Ok(Response::new(AppendEntriesResponse {
                term: *current_term,
                success: false,
                match_sequence: 0,
                error: "Term outdated".to_string(),
            }));
        }

        // 2. 更新 term 并转为 Slave
        if req.term > *current_term {
            *current_term = req.term;
            *self.ctx.voted_for.write() = None;
            self.ctx.role_manager.become_slave(req.leader_id.clone());
        }

        // 释放锁
        let term = *current_term;
        drop(current_term);

        // 3. 检查日志一致性
        if !self.ctx.check_log_consistency(req.prev_log_sequence, req.prev_log_term) {
            log::warn!(
                "[{}] Log inconsistency: prev_seq={}, prev_term={}",
                self.ctx.node_id,
                req.prev_log_sequence,
                req.prev_log_term
            );
            return Ok(Response::new(AppendEntriesResponse {
                term,
                success: false,
                match_sequence: 0,
                error: "Log inconsistency".to_string(),
            }));
        }

        // 4. 追加日志条目
        let entries: Vec<InternalLogEntry> = req
            .entries
            .into_iter()
            .map(|e| proto_to_internal_log_entry(e))
            .collect();

        let match_sequence = if entries.is_empty() {
            self.ctx.get_last_log_info().0
        } else {
            self.ctx.append_entries(entries)
        };

        // 5. 更新提交索引
        self.ctx.update_commit_index(req.leader_commit);

        log::debug!(
            "[{}] AppendEntries success: match_seq={}, commit={}",
            self.ctx.node_id,
            match_sequence,
            req.leader_commit
        );

        Ok(Response::new(AppendEntriesResponse {
            term,
            success: true,
            match_sequence,
            error: String::new(),
        }))
    }

    /// 处理心跳请求
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let current_term = *self.ctx.current_term.read();

        // 更新 term 和 leader
        if req.term > current_term {
            *self.ctx.current_term.write() = req.term;
            *self.ctx.voted_for.write() = None;
            self.ctx.role_manager.become_slave(req.leader_id.clone());
        }

        // 更新提交索引
        self.ctx.update_commit_index(req.leader_commit);

        // 采集节点状态
        let status = Some(self.ctx.collect_node_status());
        let (last_log_sequence, _) = self.ctx.get_last_log_info();

        Ok(Response::new(HeartbeatResponse {
            term: current_term,
            node_id: self.ctx.node_id.clone(),
            last_log_sequence,
            healthy: true,
            status,
        }))
    }

    /// 处理投票请求
    async fn request_vote(
        &self,
        request: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        let req = request.into_inner();
        let mut current_term = self.ctx.current_term.write();
        let mut voted_for = self.ctx.voted_for.write();

        // 1. 检查 term
        if req.term < *current_term {
            return Ok(Response::new(VoteResponse {
                term: *current_term,
                vote_granted: false,
                voter_id: self.ctx.node_id.clone(),
            }));
        }

        // 2. 更新 term
        if req.term > *current_term {
            *current_term = req.term;
            *voted_for = None;
            self.ctx.role_manager.set_role(NodeRole::Slave);
        }

        // 3. 检查是否已投票
        let vote_granted = match &*voted_for {
            None => {
                // 检查日志是否足够新
                let (last_seq, last_term) = self.ctx.get_last_log_info();
                let log_ok = req.last_log_term > last_term
                    || (req.last_log_term == last_term && req.last_log_sequence >= last_seq);

                if log_ok {
                    *voted_for = Some(req.candidate_id.clone());
                    log::info!(
                        "[{}] Voted for {} in term {}",
                        self.ctx.node_id,
                        req.candidate_id,
                        req.term
                    );
                    true
                } else {
                    log::info!(
                        "[{}] Rejected vote for {} (log not up-to-date)",
                        self.ctx.node_id,
                        req.candidate_id
                    );
                    false
                }
            }
            Some(id) => id == &req.candidate_id,
        };

        Ok(Response::new(VoteResponse {
            term: *current_term,
            vote_granted,
            voter_id: self.ctx.node_id.clone(),
        }))
    }

    /// 处理快照安装 (流式接收) @yutiansut @quantaxis
    async fn install_snapshot(
        &self,
        request: Request<tonic::Streaming<SnapshotChunk>>,
    ) -> Result<Response<SnapshotResponse>, Status> {
        use tokio_stream::StreamExt;

        let mut stream = request.into_inner();
        let mut total_bytes = 0u64;
        let mut last_sequence = 0u64;
        #[allow(unused_assignments)]
        let mut last_term = 0u64;
        let mut chunk_index = 0u64;
        let mut write_error: Option<String> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;

            // 更新 term
            if chunk.term > *self.ctx.current_term.read() {
                *self.ctx.current_term.write() = chunk.term;
            }

            total_bytes += chunk.data.len() as u64;
            last_sequence = chunk.last_included_sequence;
            last_term = chunk.last_included_term;

            // 将快照数据写入存储 @yutiansut @quantaxis
            if let Err(e) = self.ctx.write_snapshot_chunk(chunk_index, &chunk.data, chunk.is_last) {
                log::error!(
                    "[{}] Failed to write snapshot chunk {}: {}",
                    self.ctx.node_id,
                    chunk_index,
                    e
                );
                write_error = Some(e.to_string());
            }
            chunk_index += 1;

            if chunk.is_last {
                log::info!(
                    "[{}] Snapshot installed: {} bytes, {} chunks, last_seq={}, last_term={}",
                    self.ctx.node_id,
                    total_bytes,
                    chunk_index,
                    last_sequence,
                    last_term
                );
                break;
            }
        }

        // 更新状态
        *self.ctx.commit_index.write() = last_sequence;
        *self.ctx.last_applied.write() = last_sequence;

        // 清除日志存储（快照之前的日志不再需要）
        {
            let mut logs = self.ctx.log_store.write();
            logs.retain(|e| e.sequence > last_sequence);
            log::info!(
                "[{}] Truncated logs before sequence {}, remaining {} entries",
                self.ctx.node_id,
                last_sequence,
                logs.len()
            );
        }

        Ok(Response::new(SnapshotResponse {
            term: *self.ctx.current_term.read(),
            success: write_error.is_none(),
            error: write_error.unwrap_or_default(),
            bytes_received: total_bytes,
        }))
    }

    /// 流式日志复制 (双向流)
    type StreamAppendEntriesStream = ReceiverStream<Result<AppendEntriesResponse, Status>>;

    async fn stream_append_entries(
        &self,
        request: Request<tonic::Streaming<AppendEntriesRequest>>,
    ) -> Result<Response<Self::StreamAppendEntriesStream>, Status> {
        use tokio_stream::StreamExt;

        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);
        let ctx = self.ctx.clone();

        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                let response = match req {
                    Ok(req) => {
                        let current_term = *ctx.current_term.read();

                        // 简化处理：直接追加日志
                        if req.term >= current_term {
                            if req.term > current_term {
                                *ctx.current_term.write() = req.term;
                                ctx.role_manager.become_slave(req.leader_id.clone());
                            }

                            let entries: Vec<InternalLogEntry> = req
                                .entries
                                .into_iter()
                                .map(|e| proto_to_internal_log_entry(e))
                                .collect();

                            let match_sequence = ctx.append_entries(entries);
                            ctx.update_commit_index(req.leader_commit);

                            Ok(AppendEntriesResponse {
                                term: current_term,
                                success: true,
                                match_sequence,
                                error: String::new(),
                            })
                        } else {
                            Ok(AppendEntriesResponse {
                                term: current_term,
                                success: false,
                                match_sequence: 0,
                                error: "Term outdated".to_string(),
                            })
                        }
                    }
                    Err(e) => Err(Status::internal(e.to_string())),
                };

                if tx.send(response).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// gRPC 客户端
// ═══════════════════════════════════════════════════════════════════════════

/// 复制客户端 (高性能)
pub struct ReplicationClient {
    /// 目标地址
    target_addr: String,
    /// 配置
    config: GrpcConfig,
    /// 连接 (延迟初始化)
    client: Arc<RwLock<Option<ReplicationServiceClient<tonic::transport::Channel>>>>,
}

impl ReplicationClient {
    pub fn new(target_addr: String, config: GrpcConfig) -> Self {
        Self {
            target_addr,
            config,
            client: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取或创建连接
    async fn get_client(&self) -> Result<ReplicationServiceClient<tonic::transport::Channel>, String> {
        // 检查现有连接
        {
            let client = self.client.read();
            if let Some(c) = client.as_ref() {
                return Ok(c.clone());
            }
        }

        // 创建新连接
        let endpoint = tonic::transport::Channel::from_shared(format!("http://{}", self.target_addr))
            .map_err(|e| format!("Invalid address: {}", e))?
            .connect_timeout(self.config.connect_timeout)
            .timeout(self.config.request_timeout);

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;

        let new_client = ReplicationServiceClient::new(channel)
            .max_decoding_message_size(self.config.max_message_size)
            .max_encoding_message_size(self.config.max_message_size);

        *self.client.write() = Some(new_client.clone());
        Ok(new_client)
    }

    /// 发送 AppendEntries (带重试)
    pub async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, String> {
        let mut retries = 0;
        let mut last_error = String::new();

        while retries < self.config.max_retries {
            match self.get_client().await {
                Ok(mut client) => {
                    match client.append_entries(Request::new(request.clone())).await {
                        Ok(response) => return Ok(response.into_inner()),
                        Err(e) => {
                            last_error = format!("AppendEntries failed: {}", e);
                            // 连接可能失效，清除缓存
                            *self.client.write() = None;
                        }
                    }
                }
                Err(e) => {
                    last_error = e;
                }
            }

            retries += 1;
            if retries < self.config.max_retries {
                tokio::time::sleep(Duration::from_millis(100 * retries as u64)).await;
            }
        }

        Err(last_error)
    }

    /// 发送心跳 (带重试)
    pub async fn heartbeat(&self, request: HeartbeatRequest) -> Result<HeartbeatResponse, String> {
        let mut retries = 0;
        let mut last_error = String::new();

        while retries < self.config.max_retries {
            match self.get_client().await {
                Ok(mut client) => {
                    match client.heartbeat(Request::new(request.clone())).await {
                        Ok(response) => return Ok(response.into_inner()),
                        Err(e) => {
                            last_error = format!("Heartbeat failed: {}", e);
                            *self.client.write() = None;
                        }
                    }
                }
                Err(e) => {
                    last_error = e;
                }
            }

            retries += 1;
            if retries < self.config.max_retries {
                tokio::time::sleep(Duration::from_millis(50 * retries as u64)).await;
            }
        }

        Err(last_error)
    }

    /// 请求投票 (带重试)
    pub async fn request_vote(&self, request: VoteRequest) -> Result<VoteResponse, String> {
        let mut retries = 0;
        let mut last_error = String::new();

        while retries < self.config.max_retries {
            match self.get_client().await {
                Ok(mut client) => {
                    match client.request_vote(Request::new(request.clone())).await {
                        Ok(response) => return Ok(response.into_inner()),
                        Err(e) => {
                            last_error = format!("RequestVote failed: {}", e);
                            *self.client.write() = None;
                        }
                    }
                }
                Err(e) => {
                    last_error = e;
                }
            }

            retries += 1;
            if retries < self.config.max_retries {
                tokio::time::sleep(Duration::from_millis(100 * retries as u64)).await;
            }
        }

        Err(last_error)
    }

    /// 发送快照 (流式)
    pub async fn install_snapshot(
        &self,
        chunks: Vec<SnapshotChunk>,
    ) -> Result<SnapshotResponse, String> {
        let mut client = self.get_client().await?;

        let stream = tokio_stream::iter(chunks);

        client
            .install_snapshot(stream)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| format!("InstallSnapshot failed: {}", e))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 集群管理器
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
            log::info!("Node {} added to cluster: {}", node.id, node.addr);
            nodes.push(node);
        }
    }

    /// 移除节点
    pub fn remove_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write();
        nodes.retain(|n| n.id != node_id);
        self.clients.remove(node_id);
        log::info!("Node {} removed from cluster", node_id);
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

    /// 广播 AppendEntries 到所有节点 (并行)
    pub async fn broadcast_append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Vec<(String, Result<AppendEntriesResponse, String>)> {
        let nodes = self.get_active_nodes();
        let mut handles = Vec::with_capacity(nodes.len());

        for node in nodes {
            if let Some(client) = self.get_client(&node.id) {
                let req = request.clone();
                let node_id = node.id.clone();
                handles.push(tokio::spawn(async move {
                    let result = client.append_entries(req).await;
                    (node_id, result)
                }));
            }
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        results
    }

    /// 广播心跳 (并行)
    pub async fn broadcast_heartbeat(
        &self,
        request: HeartbeatRequest,
    ) -> Vec<(String, Result<HeartbeatResponse, String>)> {
        let nodes = self.get_active_nodes();
        let mut handles = Vec::with_capacity(nodes.len());

        for node in nodes {
            if let Some(client) = self.get_client(&node.id) {
                let req = request.clone();
                let node_id = node.id.clone();
                handles.push(tokio::spawn(async move {
                    let result = client.heartbeat(req).await;
                    (node_id, result)
                }));
            }
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
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
// 辅助函数
// ═══════════════════════════════════════════════════════════════════════════

/// Proto LogEntry -> Internal LogEntry
fn proto_to_internal_log_entry(proto: LogEntry) -> InternalLogEntry {
    use crate::storage::wal::WalRecord;
    use rkyv::Deserialize as RkyvDeserialize;

    // 尝试反序列化 WAL 记录
    let record = if proto.record_data.is_empty() {
        // 空记录，创建占位符
        WalRecord::Checkpoint {
            sequence: proto.sequence,
            timestamp: proto.timestamp,
        }
    } else {
        match rkyv::check_archived_root::<WalRecord>(&proto.record_data) {
            Ok(archived) => {
                RkyvDeserialize::deserialize(archived, &mut rkyv::Infallible).unwrap_or_else(|_| {
                    WalRecord::Checkpoint {
                        sequence: proto.sequence,
                        timestamp: proto.timestamp,
                    }
                })
            }
            Err(_) => WalRecord::Checkpoint {
                sequence: proto.sequence,
                timestamp: proto.timestamp,
            },
        }
    };

    InternalLogEntry {
        sequence: proto.sequence,
        term: proto.term,
        record,
        timestamp: proto.timestamp,
    }
}

/// Internal LogEntry -> Proto LogEntry
pub fn internal_to_proto_log_entry(internal: &InternalLogEntry) -> LogEntry {
    let record_data = rkyv::to_bytes::<_, 2048>(&internal.record)
        .map(|b| b.to_vec())
        .unwrap_or_default();

    let record_type = match &internal.record {
        crate::storage::wal::WalRecord::OrderInsert { .. } => RecordType::OrderInsert,
        crate::storage::wal::WalRecord::TradeExecuted { .. } => RecordType::TradeExecuted,
        crate::storage::wal::WalRecord::AccountUpdate { .. } => RecordType::AccountUpdate,
        crate::storage::wal::WalRecord::TickData { .. } => RecordType::TickData,
        crate::storage::wal::WalRecord::OrderBookSnapshot { .. } => RecordType::OrderbookSnapshot,
        crate::storage::wal::WalRecord::Checkpoint { .. } => RecordType::Checkpoint,
        // 其他类型映射为 Unknown
        _ => RecordType::Unknown,
    };

    LogEntry {
        sequence: internal.sequence,
        term: internal.term,
        record_data,
        timestamp: internal.timestamp,
        record_type: record_type.into(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::replicator::ReplicationConfig;
    use tempfile::tempdir;

    #[test]
    fn test_grpc_config_default() {
        let config = GrpcConfig::default();
        assert_eq!(config.max_message_size, 64 * 1024 * 1024);
        assert_eq!(config.max_concurrent_streams, 100);
    }

    #[test]
    fn test_cluster_manager() {
        let manager = ClusterManager::new("node1".to_string(), GrpcConfig::default());

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

        manager.remove_node("node2");
        assert_eq!(manager.get_nodes().len(), 1);
    }

    #[tokio::test]
    async fn test_replication_context() {
        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::new("test_node".to_string(), role_mgr, replicator);

        // 测试空日志状态
        let (seq, term) = ctx.get_last_log_info();
        assert_eq!(seq, 0);
        assert_eq!(term, 0);

        // 测试日志一致性检查 (空日志)
        assert!(ctx.check_log_consistency(0, 0));
    }

    // ==================== 快照写入测试 @yutiansut @quantaxis ====================

    /// 测试快照数据块写入 - 单块写入
    /// 业务场景: Slave 节点接收 Master 发送的小型快照 (单块)
    ///
    /// 写入流程:
    ///   1. chunk_index=0 时创建新文件
    ///   2. write_all(data) 写入数据
    ///   3. is_last=true 时写入元数据文件
    ///
    /// 元数据格式 (snapshot.meta):
    ///   term=<当前term>
    ///   sequence=<commit_index>
    ///   timestamp=<写入时间戳>
    #[test]
    fn test_write_snapshot_single_chunk() {
        // 使用临时目录避免污染测试环境
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        // 设置 term 和 commit_index (会写入元数据)
        *ctx.current_term.write() = 5;
        *ctx.commit_index.write() = 100;

        // 准备测试数据: 模拟账户快照数据
        // 实际场景中可能是序列化的 QIFI 账户结构
        let snapshot_data = b"account_id:55550081\nbalance:60470421.77\nmargin:16429881.60\n";

        // 写入单块快照 (chunk_index=0, is_last=true)
        let result = ctx.write_snapshot_chunk(0, snapshot_data, true);
        assert!(result.is_ok(), "快照写入应成功: {:?}", result);

        // 验证快照文件存在
        let snapshot_path = ctx.get_snapshot_path();
        assert!(snapshot_path.exists(), "快照文件应存在");

        // 验证文件内容
        let file_content = std::fs::read(&snapshot_path).expect("读取快照文件失败");
        assert_eq!(file_content, snapshot_data, "快照内容应匹配");

        // 验证元数据文件
        let meta_path = snapshot_dir.join("snapshot.meta");
        assert!(meta_path.exists(), "元数据文件应存在");

        let meta_content = std::fs::read_to_string(&meta_path).expect("读取元数据失败");
        assert!(meta_content.contains("term=5"), "元数据应包含 term");
        assert!(meta_content.contains("sequence=100"), "元数据应包含 sequence");
        assert!(meta_content.contains("timestamp="), "元数据应包含 timestamp");
    }

    /// 测试快照数据块写入 - 多块顺序写入
    /// 业务场景: Master 发送大型快照，分多个数据块传输
    ///
    /// 多块写入流程:
    ///   1. chunk_index=0: 创建新文件，写入第一块
    ///   2. chunk_index=1,2,...: 追加写入
    ///   3. 最后一块 is_last=true: 写入元数据
    ///
    /// 注意: 块必须按顺序接收，乱序会导致数据损坏
    #[test]
    fn test_write_snapshot_multiple_chunks() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        *ctx.current_term.write() = 3;
        *ctx.commit_index.write() = 50;

        // 模拟分块快照数据
        // 实际场景: 大型快照按 64MB 分块传输
        let chunk1 = b"=== SNAPSHOT HEADER ===\n";
        let chunk2 = b"position:SHFE.cu2501,volume:24\n";
        let chunk3 = b"position:CFFEX.IF2512,volume:5\n";
        let chunk4 = b"=== SNAPSHOT FOOTER ===\n";

        // 按顺序写入 4 个块
        // 前 3 个块: is_last=false
        assert!(ctx.write_snapshot_chunk(0, chunk1, false).is_ok());
        assert!(ctx.write_snapshot_chunk(1, chunk2, false).is_ok());
        assert!(ctx.write_snapshot_chunk(2, chunk3, false).is_ok());
        // 最后一块: is_last=true
        assert!(ctx.write_snapshot_chunk(3, chunk4, true).is_ok());

        // 验证合并后的文件内容
        let snapshot_path = ctx.get_snapshot_path();
        let file_content = std::fs::read(&snapshot_path).expect("读取快照失败");

        // 预期内容: chunk1 + chunk2 + chunk3 + chunk4
        let expected: Vec<u8> = [chunk1.as_slice(), chunk2, chunk3, chunk4].concat();
        assert_eq!(file_content, expected, "多块快照内容应正确合并");

        // 验证元数据 (只在最后一块时写入)
        let meta_path = snapshot_dir.join("snapshot.meta");
        assert!(meta_path.exists(), "元数据文件应在最后一块时写入");

        // 验证文件大小
        let expected_size = chunk1.len() + chunk2.len() + chunk3.len() + chunk4.len();
        assert_eq!(file_content.len(), expected_size, "文件大小应等于所有块之和");
    }

    /// 测试快照覆盖写入
    /// 业务场景: 接收新快照时覆盖旧快照
    ///
    /// 业务规则:
    ///   - chunk_index=0 时总是创建新文件 (覆盖旧文件)
    ///   - 新快照完全替换旧快照
    ///   - 元数据文件也被覆盖
    #[test]
    fn test_write_snapshot_overwrite() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        // 第一次写入
        *ctx.current_term.write() = 1;
        *ctx.commit_index.write() = 10;
        let old_data = b"old snapshot data version 1";
        ctx.write_snapshot_chunk(0, old_data, true).expect("第一次写入失败");

        let snapshot_path = ctx.get_snapshot_path();
        let old_content = std::fs::read(&snapshot_path).expect("读取旧快照失败");
        assert_eq!(old_content, old_data.to_vec());

        // 第二次写入 (覆盖)
        *ctx.current_term.write() = 2;
        *ctx.commit_index.write() = 20;
        let new_data = b"new snapshot data version 2 with more content";
        ctx.write_snapshot_chunk(0, new_data, true).expect("第二次写入失败");

        // 验证内容已被覆盖
        let new_content = std::fs::read(&snapshot_path).expect("读取新快照失败");
        assert_eq!(new_content, new_data.to_vec(), "快照应被新内容覆盖");
        assert_ne!(new_content, old_content, "新内容应与旧内容不同");

        // 验证元数据已更新
        let meta_path = snapshot_dir.join("snapshot.meta");
        let meta = std::fs::read_to_string(&meta_path).expect("读取元数据失败");
        assert!(meta.contains("term=2"), "元数据 term 应更新为 2");
        assert!(meta.contains("sequence=20"), "元数据 sequence 应更新为 20");
    }

    /// 测试获取快照路径
    /// 验证 get_snapshot_path() 返回正确的文件路径
    #[test]
    fn test_get_snapshot_path() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        let path = ctx.get_snapshot_path();

        // 验证路径组成
        assert!(path.ends_with("snapshot.dat"), "路径应以 snapshot.dat 结尾");
        assert!(path.starts_with(&snapshot_dir), "路径应在 snapshot_dir 下");
    }

    /// 测试空数据块写入
    /// 边界情况: 处理空数据块
    #[test]
    fn test_write_snapshot_empty_chunk() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        // 写入空数据块
        let empty_data: &[u8] = b"";
        let result = ctx.write_snapshot_chunk(0, empty_data, true);
        assert!(result.is_ok(), "空数据块写入应成功");

        // 验证文件存在但为空
        let path = ctx.get_snapshot_path();
        assert!(path.exists(), "快照文件应存在");
        let content = std::fs::read(&path).expect("读取失败");
        assert!(content.is_empty(), "文件内容应为空");
    }

    /// 测试大数据块写入
    /// 性能测试: 写入 1MB 数据块
    #[test]
    fn test_write_snapshot_large_chunk() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir,
        );

        // 生成 1MB 测试数据
        let large_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();

        let result = ctx.write_snapshot_chunk(0, &large_data, true);
        assert!(result.is_ok(), "大数据块写入应成功");

        // 验证数据完整性
        let path = ctx.get_snapshot_path();
        let file_content = std::fs::read(&path).expect("读取失败");
        assert_eq!(file_content.len(), 1024 * 1024, "文件大小应为 1MB");
        assert_eq!(file_content, large_data, "文件内容应完整");
    }

    /// 测试快照元数据格式
    /// 验证元数据文件的解析能力
    #[test]
    fn test_snapshot_metadata_format() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        *ctx.current_term.write() = 42;
        *ctx.commit_index.write() = 12345;

        ctx.write_snapshot_chunk(0, b"test", true).expect("写入失败");

        // 解析元数据
        let meta_path = snapshot_dir.join("snapshot.meta");
        let meta_content = std::fs::read_to_string(&meta_path).expect("读取元数据失败");

        // 验证可解析性
        let mut term_found = false;
        let mut sequence_found = false;
        let mut timestamp_found = false;

        for line in meta_content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "term" => {
                        assert_eq!(value, "42");
                        term_found = true;
                    }
                    "sequence" => {
                        assert_eq!(value, "12345");
                        sequence_found = true;
                    }
                    "timestamp" => {
                        // 验证是有效的时间戳
                        let ts: i64 = value.parse().expect("timestamp 应是数字");
                        assert!(ts > 0, "timestamp 应为正数");
                        timestamp_found = true;
                    }
                    _ => {}
                }
            }
        }

        assert!(term_found, "元数据应包含 term");
        assert!(sequence_found, "元数据应包含 sequence");
        assert!(timestamp_found, "元数据应包含 timestamp");
    }

    /// 测试快照传输流程模拟
    /// 业务场景: 模拟完整的 Master→Slave 快照传输
    ///
    /// 流程:
    ///   1. Master 将状态序列化为多个 SnapshotChunk
    ///   2. 通过 gRPC 流式传输到 Slave
    ///   3. Slave 调用 write_snapshot_chunk 写入
    ///   4. 最后更新 commit_index 和 last_applied
    #[test]
    fn test_snapshot_transfer_simulation() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("slave_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "slave_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir.clone(),
        );

        // 模拟 Master 发送的快照块
        // 实际场景中这些块通过 install_snapshot gRPC 流接收
        //
        // SnapshotChunk 结构体字段说明:
        //   - term: Leader 的任期号
        //   - last_included_sequence: 快照包含的最后日志序列号
        //   - last_included_term: 快照包含的最后日志任期
        //   - chunk_index: 当前块索引 (从 0 开始)
        //   - total_chunks: 总块数 (用于进度显示)
        //   - data: 块数据
        //   - is_last: 是否为最后一块
        let chunks = vec![
            SnapshotChunk {
                term: 10,
                last_included_sequence: 500,
                last_included_term: 9,
                chunk_index: 0,
                total_chunks: 3,  // 共 3 个块
                data: b"QIFI_ACCOUNT_START\n".to_vec(),
                is_last: false,
            },
            SnapshotChunk {
                term: 10,
                last_included_sequence: 500,
                last_included_term: 9,
                chunk_index: 1,
                total_chunks: 3,
                data: b"account_cookie:55550081\nbalance:60470421.77\n".to_vec(),
                is_last: false,
            },
            SnapshotChunk {
                term: 10,
                last_included_sequence: 500,
                last_included_term: 9,
                chunk_index: 2,
                total_chunks: 3,
                data: b"QIFI_ACCOUNT_END\n".to_vec(),
                is_last: true,
            },
        ];

        // 模拟接收处理 (install_snapshot 服务端逻辑)
        let mut total_bytes = 0u64;
        for chunk in &chunks {
            // 更新 term
            if chunk.term > *ctx.current_term.read() {
                *ctx.current_term.write() = chunk.term;
            }

            total_bytes += chunk.data.len() as u64;

            // 写入快照块
            ctx.write_snapshot_chunk(chunk.chunk_index, &chunk.data, chunk.is_last)
                .expect("写入快照块失败");
        }

        // 更新状态 (模拟 install_snapshot 返回后的处理)
        let last_chunk = chunks.last().unwrap();
        *ctx.commit_index.write() = last_chunk.last_included_sequence;
        *ctx.last_applied.write() = last_chunk.last_included_sequence;

        // 验证结果
        assert_eq!(*ctx.current_term.read(), 10, "term 应更新为 10");
        assert_eq!(*ctx.commit_index.read(), 500, "commit_index 应更新为 500");
        assert_eq!(*ctx.last_applied.read(), 500, "last_applied 应更新为 500");

        // 验证快照文件
        let snapshot_path = ctx.get_snapshot_path();
        let content = std::fs::read_to_string(&snapshot_path).expect("读取快照失败");
        assert!(content.contains("QIFI_ACCOUNT_START"));
        assert!(content.contains("account_cookie:55550081"));
        assert!(content.contains("balance:60470421.77"));
        assert!(content.contains("QIFI_ACCOUNT_END"));

        // 验证总字节数
        let expected_bytes: u64 = chunks.iter().map(|c| c.data.len() as u64).sum();
        assert_eq!(total_bytes, expected_bytes);
    }

    /// 测试节点状态采集
    /// 验证 collect_node_status() 返回合理的系统指标
    #[test]
    fn test_collect_node_status() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let snapshot_dir = temp_dir.path().to_path_buf();

        let role_mgr = Arc::new(RoleManager::new("test_node".to_string(), NodeRole::Slave));
        let replicator = Arc::new(LogReplicator::new(role_mgr.clone(), ReplicationConfig::default()));
        let ctx = ReplicationContext::with_snapshot_dir(
            "test_node".to_string(),
            role_mgr,
            replicator,
            snapshot_dir,
        );

        // 采集节点状态
        let status = ctx.collect_node_status();

        // 验证 CPU 使用率在合理范围 (0-100%)
        assert!(status.cpu_usage >= 0.0, "CPU 使用率应 >= 0");
        assert!(status.cpu_usage <= 100.0, "CPU 使用率应 <= 100");

        // 验证内存使用率在合理范围
        assert!(status.memory_usage >= 0.0, "内存使用率应 >= 0");
        assert!(status.memory_usage <= 100.0, "内存使用率应 <= 100");

        // 验证磁盘使用率在合理范围
        // 注意: 某些容器环境可能获取不到磁盘信息，此时为 0
        assert!(status.disk_usage >= 0.0, "磁盘使用率应 >= 0");
        assert!(status.disk_usage <= 100.0, "磁盘使用率应 <= 100");
    }
}
