//! 节点角色管理

use std::sync::Arc;
use parking_lot::RwLock;

/// 节点角色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// Master节点（接受写入）
    Master,

    /// Slave节点（只读，复制数据）
    Slave,

    /// Candidate节点（选举中）
    Candidate,
}

/// 角色管理器
pub struct RoleManager {
    /// 当前角色
    role: Arc<RwLock<NodeRole>>,

    /// 节点ID
    node_id: String,

    /// 当前term（选举轮次）
    current_term: Arc<RwLock<u64>>,

    /// 投票给谁（在当前term中）
    voted_for: Arc<RwLock<Option<String>>>,

    /// Master ID（如果是Slave）
    master_id: Arc<RwLock<Option<String>>>,
}

impl RoleManager {
    pub fn new(node_id: String, initial_role: NodeRole) -> Self {
        Self {
            role: Arc::new(RwLock::new(initial_role)),
            node_id,
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            master_id: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取当前角色
    pub fn get_role(&self) -> NodeRole {
        *self.role.read()
    }

    /// 设置角色
    pub fn set_role(&self, role: NodeRole) {
        let mut r = self.role.write();
        let old_role = *r;
        *r = role;

        log::info!(
            "[{}] Role changed: {:?} -> {:?}",
            self.node_id,
            old_role,
            role
        );
    }

    /// 是否是Master
    pub fn is_master(&self) -> bool {
        *self.role.read() == NodeRole::Master
    }

    /// 是否是Slave
    pub fn is_slave(&self) -> bool {
        *self.role.read() == NodeRole::Slave
    }

    /// 获取节点ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// 获取当前term
    pub fn get_term(&self) -> u64 {
        *self.current_term.read()
    }

    /// 设置term
    pub fn set_term(&self, term: u64) {
        let mut t = self.current_term.write();
        if term > *t {
            *t = term;
            // 新的term，清除投票记录
            *self.voted_for.write() = None;
            log::info!("[{}] Term updated to {}", self.node_id, term);
        }
    }

    /// 增加term（用于开始选举）
    pub fn increment_term(&self) -> u64 {
        let mut t = self.current_term.write();
        *t += 1;
        let new_term = *t;

        // 新term，清除投票
        *self.voted_for.write() = None;

        log::info!("[{}] Term incremented to {}", self.node_id, new_term);
        new_term
    }

    /// 投票
    pub fn vote_for(&self, candidate_id: &str) -> bool {
        let mut voted = self.voted_for.write();
        if voted.is_none() {
            *voted = Some(candidate_id.to_string());
            log::info!(
                "[{}] Voted for {} in term {}",
                self.node_id,
                candidate_id,
                self.get_term()
            );
            true
        } else {
            false
        }
    }

    /// 获取已投票的候选人
    pub fn get_voted_for(&self) -> Option<String> {
        self.voted_for.read().clone()
    }

    /// 设置Master ID
    pub fn set_master(&self, master_id: Option<String>) {
        *self.master_id.write() = master_id.clone();
        if let Some(id) = master_id {
            log::info!("[{}] Master set to {}", self.node_id, id);
        }
    }

    /// 获取Master ID
    pub fn get_master(&self) -> Option<String> {
        self.master_id.read().clone()
    }

    /// 转换为Master
    pub fn become_master(&self) {
        self.set_role(NodeRole::Master);
        self.set_master(Some(self.node_id.clone()));
    }

    /// 转换为Slave
    pub fn become_slave(&self, master_id: String) {
        self.set_role(NodeRole::Slave);
        self.set_master(Some(master_id));
    }

    /// 转换为Candidate
    pub fn become_candidate(&self) {
        self.set_role(NodeRole::Candidate);
        self.increment_term();
        self.vote_for(&self.node_id); // 投票给自己
    }
}
