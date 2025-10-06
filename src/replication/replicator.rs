//! 日志复制器

use super::protocol::{LogEntry, ReplicationRequest, ReplicationResponse};
use super::role::RoleManager;
use crate::storage::wal::WalRecord;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// 复制配置
#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    /// 复制超时（毫秒）
    pub replication_timeout_ms: u64,

    /// 批量大小
    pub batch_size: usize,

    /// 最大重试次数
    pub max_retries: usize,

    /// 心跳间隔（毫秒）
    pub heartbeat_interval_ms: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            replication_timeout_ms: 1000,
            batch_size: 100,
            max_retries: 3,
            heartbeat_interval_ms: 100,
        }
    }
}

/// 日志复制器
pub struct LogReplicator {
    /// 角色管理器
    role_manager: Arc<RoleManager>,

    /// 配置
    config: ReplicationConfig,

    /// 日志缓冲区（待复制的日志）
    pending_logs: Arc<RwLock<Vec<LogEntry>>>,

    /// Slave的匹配序列号
    slave_match_index: Arc<RwLock<HashMap<String, u64>>>,

    /// Slave的下一个序列号
    slave_next_index: Arc<RwLock<HashMap<String, u64>>>,

    /// commit序列号
    commit_index: Arc<RwLock<u64>>,

    /// 复制响应通道
    response_tx: mpsc::UnboundedSender<(String, ReplicationResponse)>,
    response_rx: Arc<parking_lot::Mutex<mpsc::UnboundedReceiver<(String, ReplicationResponse)>>>,
}

impl LogReplicator {
    pub fn new(role_manager: Arc<RoleManager>, config: ReplicationConfig) -> Self {
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        Self {
            role_manager,
            config,
            pending_logs: Arc::new(RwLock::new(Vec::new())),
            slave_match_index: Arc::new(RwLock::new(HashMap::new())),
            slave_next_index: Arc::new(RwLock::new(HashMap::new())),
            commit_index: Arc::new(RwLock::new(0)),
            response_tx,
            response_rx: Arc::new(parking_lot::Mutex::new(response_rx)),
        }
    }

    /// 添加日志到复制队列（Master调用）
    pub fn append_log(&self, sequence: u64, record: WalRecord) -> Result<(), String> {
        if !self.role_manager.is_master() {
            return Err("Only master can append logs".to_string());
        }

        let entry = LogEntry {
            sequence,
            term: self.role_manager.get_term(),
            record,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.pending_logs.write().push(entry);

        log::debug!(
            "[{}] Log appended: sequence {}",
            self.role_manager.node_id(),
            sequence
        );

        Ok(())
    }

    /// 创建复制请求（Master调用）
    pub fn create_replication_request(&self, slave_id: &str) -> Option<ReplicationRequest> {
        if !self.role_manager.is_master() {
            return None;
        }

        let next_index = self.slave_next_index.read().get(slave_id).cloned().unwrap_or(1);
        let pending = self.pending_logs.read();

        // 查找从next_index开始的日志
        let entries: Vec<LogEntry> = pending
            .iter()
            .filter(|e| e.sequence >= next_index)
            .take(self.config.batch_size)
            .cloned()
            .collect();

        if entries.is_empty() {
            return None; // 没有新日志
        }

        // 前一个日志的信息
        let (prev_log_sequence, prev_log_term) = if next_index > 1 {
            pending
                .iter()
                .find(|e| e.sequence == next_index - 1)
                .map(|e| (e.sequence, e.term))
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };

        Some(ReplicationRequest {
            term: self.role_manager.get_term(),
            leader_id: self.role_manager.node_id().to_string(),
            prev_log_sequence,
            prev_log_term,
            entries,
            leader_commit: *self.commit_index.read(),
        })
    }

    /// 处理复制响应（Master调用）
    pub fn handle_replication_response(
        &self,
        slave_id: String,
        response: ReplicationResponse,
    ) -> Result<(), String> {
        if !self.role_manager.is_master() {
            return Ok(());
        }

        if response.term > self.role_manager.get_term() {
            // Slave的term更高，降级为Slave
            self.role_manager.set_term(response.term);
            self.role_manager.set_role(super::role::NodeRole::Slave);
            log::warn!(
                "[{}] Stepped down due to higher term from {}",
                self.role_manager.node_id(),
                slave_id
            );
            return Ok(());
        }

        if response.success {
            // 更新匹配序列号
            self.slave_match_index
                .write()
                .insert(slave_id.clone(), response.match_sequence);

            // 更新下一个序列号
            self.slave_next_index
                .write()
                .insert(slave_id.clone(), response.match_sequence + 1);

            log::debug!(
                "[{}] Slave {} replicated up to sequence {}",
                self.role_manager.node_id(),
                slave_id,
                response.match_sequence
            );

            // 更新commit索引
            self.update_commit_index();
        } else {
            // 复制失败，减小next_index重试
            let mut next_index = self.slave_next_index.write();
            let current = next_index.get(&slave_id).cloned().unwrap_or(1);
            if current > 1 {
                next_index.insert(slave_id.clone(), current - 1);
            }

            log::warn!(
                "[{}] Replication to {} failed: {:?}, retrying from {}",
                self.role_manager.node_id(),
                slave_id,
                response.error,
                current - 1
            );
        }

        Ok(())
    }

    /// 应用日志（Slave调用）
    pub fn apply_logs(&self, request: ReplicationRequest) -> ReplicationResponse {
        let current_term = self.role_manager.get_term();

        // 检查term
        if request.term < current_term {
            return ReplicationResponse {
                term: current_term,
                success: false,
                match_sequence: 0,
                error: Some("Stale term".to_string()),
            };
        }

        // 更新term和leader
        if request.term > current_term {
            self.role_manager.set_term(request.term);
        }

        self.role_manager.become_slave(request.leader_id.clone());

        // 应用日志到pending buffer（实际应该写入WAL）
        let mut pending = self.pending_logs.write();
        for entry in &request.entries {
            pending.push(entry.clone());
        }

        let last_sequence = request.entries.last().map(|e| e.sequence).unwrap_or(0);

        // 更新commit
        if request.leader_commit > *self.commit_index.read() {
            let new_commit = request.leader_commit.min(last_sequence);
            *self.commit_index.write() = new_commit;
        }

        ReplicationResponse {
            term: current_term,
            success: true,
            match_sequence: last_sequence,
            error: None,
        }
    }

    /// 更新commit索引（基于多数派）
    fn update_commit_index(&self) {
        let match_indices = self.slave_match_index.read();
        let mut indices: Vec<u64> = match_indices.values().cloned().collect();
        indices.sort();

        // 计算中位数（多数派已复制）
        if !indices.is_empty() {
            let majority_index = indices[indices.len() / 2];
            let mut commit = self.commit_index.write();
            if majority_index > *commit {
                *commit = majority_index;
                log::info!(
                    "[{}] Commit index updated to {}",
                    self.role_manager.node_id(),
                    majority_index
                );
            }
        }
    }

    /// 注册Slave
    pub fn register_slave(&self, slave_id: String) {
        self.slave_match_index.write().insert(slave_id.clone(), 0);
        self.slave_next_index.write().insert(slave_id.clone(), 1);
        log::info!(
            "[{}] Slave {} registered",
            self.role_manager.node_id(),
            slave_id
        );
    }

    /// 注销Slave
    pub fn unregister_slave(&self, slave_id: &str) {
        self.slave_match_index.write().remove(slave_id);
        self.slave_next_index.write().remove(slave_id);
        log::info!(
            "[{}] Slave {} unregistered",
            self.role_manager.node_id(),
            slave_id
        );
    }

    /// 获取commit序列号
    pub fn get_commit_index(&self) -> u64 {
        *self.commit_index.read()
    }

    /// 获取待复制日志数量
    pub fn pending_count(&self) -> usize {
        self.pending_logs.read().len()
    }
}
