//! 心跳管理器

use super::protocol::{HeartbeatRequest, HeartbeatResponse};
use super::role::RoleManager;
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use tokio::time::{interval, Duration, Instant};

/// 心跳管理器
pub struct HeartbeatManager {
    /// 角色管理器
    role_manager: Arc<RoleManager>,

    /// 心跳间隔
    heartbeat_interval: Duration,

    /// 心跳超时
    heartbeat_timeout: Duration,

    /// Slave最后心跳时间
    slave_last_heartbeat: Arc<RwLock<HashMap<String, Instant>>>,

    /// Master最后心跳时间
    master_last_heartbeat: Arc<RwLock<Option<Instant>>>,
}

impl HeartbeatManager {
    pub fn new(
        role_manager: Arc<RoleManager>,
        heartbeat_interval_ms: u64,
        heartbeat_timeout_ms: u64,
    ) -> Self {
        Self {
            role_manager,
            heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
            heartbeat_timeout: Duration::from_millis(heartbeat_timeout_ms),
            slave_last_heartbeat: Arc::new(RwLock::new(HashMap::new())),
            master_last_heartbeat: Arc::new(RwLock::new(None)),
        }
    }

    /// 启动心跳发送（Master调用）
    pub fn start_heartbeat_sender(&self, commit_index: Arc<RwLock<u64>>) {
        let role_manager = self.role_manager.clone();
        let heartbeat_interval = self.heartbeat_interval;
        let commit_index = commit_index.clone();

        tokio::spawn(async move {
            let mut ticker = interval(heartbeat_interval);

            loop {
                ticker.tick().await;

                if !role_manager.is_master() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                // 创建心跳请求
                let request = HeartbeatRequest {
                    term: role_manager.get_term(),
                    leader_id: role_manager.node_id().to_string(),
                    leader_commit: *commit_index.read(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };

                // 实际应该发送到所有Slave
                log::trace!(
                    "[{}] Sending heartbeat, term: {}, commit: {}",
                    role_manager.node_id(),
                    request.term,
                    request.leader_commit
                );
            }
        });
    }

    /// 处理心跳请求（Slave调用）
    pub fn handle_heartbeat_request(
        &self,
        request: HeartbeatRequest,
        last_log_sequence: u64,
    ) -> HeartbeatResponse {
        let current_term = self.role_manager.get_term();

        // 更新term
        if request.term > current_term {
            self.role_manager.set_term(request.term);
        }

        // 如果term >= current_term，确认这是有效的Master
        if request.term >= current_term {
            self.role_manager.become_slave(request.leader_id.clone());

            // 更新Master心跳时间
            *self.master_last_heartbeat.write() = Some(Instant::now());

            log::trace!(
                "[{}] Received heartbeat from master {}",
                self.role_manager.node_id(),
                request.leader_id
            );
        }

        HeartbeatResponse {
            term: self.role_manager.get_term(),
            node_id: self.role_manager.node_id().to_string(),
            last_log_sequence,
            healthy: true,
        }
    }

    /// 处理心跳响应（Master调用）
    pub fn handle_heartbeat_response(&self, slave_id: String, response: HeartbeatResponse) {
        if !self.role_manager.is_master() {
            return;
        }

        // 更新Slave心跳时间
        self.slave_last_heartbeat
            .write()
            .insert(slave_id.clone(), Instant::now());

        log::trace!(
            "[{}] Received heartbeat from slave {}, sequence: {}",
            self.role_manager.node_id(),
            slave_id,
            response.last_log_sequence
        );

        // 检查term
        if response.term > self.role_manager.get_term() {
            self.role_manager.set_term(response.term);
            self.role_manager.set_role(super::role::NodeRole::Slave);
            log::warn!(
                "[{}] Stepped down due to higher term from {}",
                self.role_manager.node_id(),
                slave_id
            );
        }
    }

    /// 检查Master是否超时（Slave调用）
    pub fn is_master_timeout(&self) -> bool {
        if self.role_manager.is_master() {
            return false;
        }

        let last_heartbeat = self.master_last_heartbeat.read();
        match *last_heartbeat {
            Some(last) => last.elapsed() > self.heartbeat_timeout,
            None => true, // 从未收到心跳
        }
    }

    /// 检查Slave是否超时（Master调用）
    pub fn get_timeout_slaves(&self) -> Vec<String> {
        if !self.role_manager.is_master() {
            return Vec::new();
        }

        let heartbeats = self.slave_last_heartbeat.read();
        let now = Instant::now();

        heartbeats
            .iter()
            .filter(|(_, last)| now.duration_since(**last) > self.heartbeat_timeout)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 注册Slave
    pub fn register_slave(&self, slave_id: String) {
        self.slave_last_heartbeat
            .write()
            .insert(slave_id, Instant::now());
    }

    /// 注销Slave
    pub fn unregister_slave(&self, slave_id: &str) {
        self.slave_last_heartbeat.write().remove(slave_id);
    }

    /// 启动心跳超时检查（Slave调用）
    pub fn start_timeout_checker(&self) {
        let role_manager = self.role_manager.clone();
        let master_last_heartbeat = self.master_last_heartbeat.clone();
        let timeout = self.heartbeat_timeout;

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(100));

            loop {
                ticker.tick().await;

                if role_manager.is_master() || role_manager.get_role() == super::role::NodeRole::Candidate {
                    continue;
                }

                // 检查Master心跳超时
                let last = master_last_heartbeat.read();
                let is_timeout = match *last {
                    Some(t) => t.elapsed() > timeout,
                    None => false, // 刚启动，给一些时间
                };

                if is_timeout {
                    log::warn!(
                        "[{}] Master heartbeat timeout, starting election",
                        role_manager.node_id()
                    );

                    // 开始选举
                    role_manager.become_candidate();
                }
            }
        });
    }
}
