//! 故障转移协调器

use super::role::{RoleManager, NodeRole};
use super::heartbeat::HeartbeatManager;
use super::replicator::LogReplicator;
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use tokio::time::{interval, Duration};

/// 故障转移配置
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    /// 选举超时范围（毫秒）- 随机化避免split vote
    pub election_timeout_min_ms: u64,
    pub election_timeout_max_ms: u64,

    /// 最小选举票数（通常是节点数的一半+1）
    pub min_votes_required: usize,

    /// 故障检测间隔（毫秒）
    pub check_interval_ms: u64,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            election_timeout_min_ms: 150,
            election_timeout_max_ms: 300,
            min_votes_required: 2, // 假设3节点集群
            check_interval_ms: 100,
        }
    }
}

/// 故障转移协调器
pub struct FailoverCoordinator {
    /// 角色管理器
    role_manager: Arc<RoleManager>,

    /// 心跳管理器
    heartbeat_manager: Arc<HeartbeatManager>,

    /// 日志复制器
    log_replicator: Arc<LogReplicator>,

    /// 配置
    config: FailoverConfig,

    /// 选票记录（term -> voter_id）
    votes_received: Arc<RwLock<HashMap<u64, Vec<String>>>>,

    /// 集群节点列表
    cluster_nodes: Arc<RwLock<Vec<String>>>,
}

impl FailoverCoordinator {
    pub fn new(
        role_manager: Arc<RoleManager>,
        heartbeat_manager: Arc<HeartbeatManager>,
        log_replicator: Arc<LogReplicator>,
        config: FailoverConfig,
    ) -> Self {
        Self {
            role_manager,
            heartbeat_manager,
            log_replicator,
            config,
            votes_received: Arc::new(RwLock::new(HashMap::new())),
            cluster_nodes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 设置集群节点
    pub fn set_cluster_nodes(&self, nodes: Vec<String>) {
        *self.cluster_nodes.write() = nodes;
    }

    /// 启动故障检测
    pub fn start_failover_detector(&self) {
        let role_manager = self.role_manager.clone();
        let heartbeat_manager = self.heartbeat_manager.clone();
        let check_interval = Duration::from_millis(self.config.check_interval_ms);

        tokio::spawn(async move {
            let mut ticker = interval(check_interval);

            loop {
                ticker.tick().await;

                // 只有Slave需要检测故障
                if !role_manager.is_slave() {
                    continue;
                }

                // 检查Master心跳超时
                if heartbeat_manager.is_master_timeout() {
                    log::warn!(
                        "[{}] Master timeout detected, initiating failover",
                        role_manager.node_id()
                    );

                    // 开始选举
                    role_manager.become_candidate();
                }
            }
        });
    }

    /// 开始选举
    pub fn start_election(&self) {
        if !matches!(self.role_manager.get_role(), NodeRole::Candidate) {
            return;
        }

        let current_term = self.role_manager.get_term();

        log::info!(
            "[{}] Starting election for term {}",
            self.role_manager.node_id(),
            current_term
        );

        // 清除之前的投票记录
        self.votes_received.write().clear();

        // 投票给自己
        let mut votes = self.votes_received.write();
        votes.insert(current_term, vec![self.role_manager.node_id().to_string()]);
        drop(votes);

        // 向其他节点请求投票（实际实现需要网络通信）
        // 这里简化为日志输出
        log::info!(
            "[{}] Requesting votes from cluster nodes",
            self.role_manager.node_id()
        );

        // 检查是否赢得选举
        self.check_election_result(current_term);
    }

    /// 处理投票请求
    pub fn handle_vote_request(
        &self,
        candidate_id: &str,
        candidate_term: u64,
        last_log_sequence: u64,
    ) -> (bool, u64) {
        let current_term = self.role_manager.get_term();

        // 如果候选人term更高，更新自己的term
        if candidate_term > current_term {
            self.role_manager.set_term(candidate_term);
            self.role_manager.set_role(NodeRole::Slave);
        }

        let current_term = self.role_manager.get_term();

        // 检查是否可以投票
        let can_vote = candidate_term >= current_term
            && self.role_manager.get_voted_for().is_none()
            && last_log_sequence >= self.log_replicator.get_commit_index();

        if can_vote {
            self.role_manager.vote_for(candidate_id);
            log::info!(
                "[{}] Voted for {} in term {}",
                self.role_manager.node_id(),
                candidate_id,
                current_term
            );
            (true, current_term)
        } else {
            log::info!(
                "[{}] Rejected vote for {} in term {}",
                self.role_manager.node_id(),
                candidate_id,
                current_term
            );
            (false, current_term)
        }
    }

    /// 处理投票响应
    pub fn handle_vote_response(
        &self,
        voter_id: String,
        granted: bool,
        term: u64,
    ) {
        if !matches!(self.role_manager.get_role(), NodeRole::Candidate) {
            return;
        }

        let current_term = self.role_manager.get_term();

        // 如果响应的term更高，降级为Slave
        if term > current_term {
            self.role_manager.set_term(term);
            self.role_manager.set_role(NodeRole::Slave);
            log::warn!(
                "[{}] Stepped down due to higher term {}",
                self.role_manager.node_id(),
                term
            );
            return;
        }

        // 记录投票
        if granted && term == current_term {
            let mut votes = self.votes_received.write();
            votes
                .entry(current_term)
                .or_insert_with(Vec::new)
                .push(voter_id.clone());

            log::info!(
                "[{}] Received vote from {} for term {}",
                self.role_manager.node_id(),
                voter_id,
                current_term
            );

            drop(votes);

            // 检查选举结果
            self.check_election_result(current_term);
        }
    }

    /// 检查选举结果
    fn check_election_result(&self, term: u64) {
        let votes = self.votes_received.read();
        let vote_count = votes.get(&term).map(|v| v.len()).unwrap_or(0);

        log::debug!(
            "[{}] Election status: {} votes (need {})",
            self.role_manager.node_id(),
            vote_count,
            self.config.min_votes_required
        );

        // 检查是否获得多数票
        if vote_count >= self.config.min_votes_required {
            drop(votes);

            log::info!(
                "[{}] Won election for term {} with {} votes",
                self.role_manager.node_id(),
                term,
                vote_count
            );

            // 成为Master
            self.role_manager.become_master();

            // 重新初始化所有Slave的next_index
            let cluster_nodes = self.cluster_nodes.read().clone();
            for node in cluster_nodes {
                if node != self.role_manager.node_id() {
                    self.log_replicator.register_slave(node);
                }
            }
        }
    }

    /// 启动选举超时检查
    pub fn start_election_timeout(&self) {
        let role_manager = self.role_manager.clone();
        let coordinator = Arc::new(self.clone_for_timeout());
        let min_timeout = self.config.election_timeout_min_ms;
        let max_timeout = self.config.election_timeout_max_ms;

        tokio::spawn(async move {
            loop {
                // 随机选举超时（避免split vote）
                let timeout = rand::random::<u64>() % (max_timeout - min_timeout) + min_timeout;
                tokio::time::sleep(Duration::from_millis(timeout)).await;

                // 只有Candidate需要超时重试
                if matches!(role_manager.get_role(), NodeRole::Candidate) {
                    log::warn!(
                        "[{}] Election timeout, retrying",
                        role_manager.node_id()
                    );
                    coordinator.start_election();
                }
            }
        });
    }

    /// 克隆用于超时任务（需要实现Clone或使用Arc）
    fn clone_for_timeout(&self) -> FailoverCoordinator {
        FailoverCoordinator {
            role_manager: self.role_manager.clone(),
            heartbeat_manager: self.heartbeat_manager.clone(),
            log_replicator: self.log_replicator.clone(),
            config: self.config.clone(),
            votes_received: self.votes_received.clone(),
            cluster_nodes: self.cluster_nodes.clone(),
        }
    }

    /// 获取集群状态
    pub fn get_cluster_status(&self) -> ClusterStatus {
        let role = self.role_manager.get_role();
        let term = self.role_manager.get_term();
        let master_id = self.role_manager.get_master();
        let commit_index = self.log_replicator.get_commit_index();

        ClusterStatus {
            node_id: self.role_manager.node_id().to_string(),
            role,
            term,
            master_id,
            commit_index,
            is_healthy: true,
        }
    }
}

/// 集群状态
#[derive(Debug, Clone)]
pub struct ClusterStatus {
    pub node_id: String,
    pub role: NodeRole,
    pub term: u64,
    pub master_id: Option<String>,
    pub commit_index: u64,
    pub is_healthy: bool,
}
