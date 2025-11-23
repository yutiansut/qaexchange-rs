//! 一致性哈希分片
//!
//! @yutiansut @quantaxis
//!
//! 提供数据分片路由能力：
//! - 一致性哈希环
//! - 虚拟节点映射
//! - 热点数据负载均衡
//! - 节点动态扩缩容

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

// ═══════════════════════════════════════════════════════════════════════════
// 一致性哈希核心
// ═══════════════════════════════════════════════════════════════════════════

/// 哈希函数 (使用 xxHash 风格的快速哈希)
fn hash_key(key: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// 虚拟节点
#[derive(Debug, Clone)]
struct VirtualNode {
    /// 物理节点 ID
    physical_node: String,
    /// 虚拟节点索引
    index: usize,
    /// 哈希值
    hash: u64,
}

/// 物理节点信息
#[derive(Debug, Clone)]
pub struct PhysicalNode {
    /// 节点 ID
    pub id: String,
    /// 节点地址
    pub addr: String,
    /// 权重 (影响虚拟节点数量)
    pub weight: u32,
    /// 是否活跃
    pub is_active: bool,
    /// 当前负载
    pub load: f64,
}

impl PhysicalNode {
    pub fn new(id: impl Into<String>, addr: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            addr: addr.into(),
            weight: 100,
            is_active: true,
            load: 0.0,
        }
    }

    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }
}

/// 一致性哈希环
pub struct ConsistentHashRing {
    /// 哈希环 (哈希值 -> 虚拟节点)
    ring: Arc<RwLock<BTreeMap<u64, VirtualNode>>>,
    /// 物理节点表
    nodes: Arc<DashMap<String, PhysicalNode>>,
    /// 每个物理节点的虚拟节点数量基数
    virtual_node_count: usize,
}

impl ConsistentHashRing {
    pub fn new(virtual_node_count: usize) -> Self {
        Self {
            ring: Arc::new(RwLock::new(BTreeMap::new())),
            nodes: Arc::new(DashMap::new()),
            virtual_node_count,
        }
    }

    /// 添加物理节点
    pub fn add_node(&self, node: PhysicalNode) {
        let node_id = node.id.clone();
        let weight = node.weight;

        // 保存物理节点
        self.nodes.insert(node_id.clone(), node);

        // 计算虚拟节点数量 (基于权重)
        let vnode_count = (self.virtual_node_count as u32 * weight / 100) as usize;

        // 添加虚拟节点到哈希环
        let mut ring = self.ring.write();
        for i in 0..vnode_count {
            let vnode_key = format!("{}#{}", node_id, i);
            let hash = hash_key(&vnode_key);

            ring.insert(
                hash,
                VirtualNode {
                    physical_node: node_id.clone(),
                    index: i,
                    hash,
                },
            );
        }

        log::info!(
            "Added node {} with {} virtual nodes",
            node_id,
            vnode_count
        );
    }

    /// 移除物理节点
    pub fn remove_node(&self, node_id: &str) {
        // 移除物理节点
        self.nodes.remove(node_id);

        // 移除所有虚拟节点
        let mut ring = self.ring.write();
        ring.retain(|_, vnode| vnode.physical_node != node_id);

        log::info!("Removed node {}", node_id);
    }

    /// 根据 key 获取负责的节点
    pub fn get_node(&self, key: &str) -> Option<PhysicalNode> {
        let hash = hash_key(key);
        let ring = self.ring.read();

        // 找到第一个哈希值 >= key 哈希的虚拟节点
        let vnode = ring
            .range(hash..)
            .next()
            .or_else(|| ring.iter().next())
            .map(|(_, v)| v)?;

        // 返回对应的物理节点
        self.nodes.get(&vnode.physical_node).map(|n| n.clone())
    }

    /// 获取 key 的主节点和副本节点
    pub fn get_nodes_with_replicas(&self, key: &str, replica_count: usize) -> Vec<PhysicalNode> {
        let hash = hash_key(key);
        let ring = self.ring.read();
        let mut result = Vec::with_capacity(replica_count + 1);
        let mut seen_nodes = std::collections::HashSet::new();

        // 从 key 的哈希位置开始，顺时针遍历找到足够多的不同物理节点
        let iter = ring
            .range(hash..)
            .chain(ring.iter())
            .take(ring.len());

        for (_, vnode) in iter {
            if seen_nodes.insert(vnode.physical_node.clone()) {
                if let Some(node) = self.nodes.get(&vnode.physical_node) {
                    if node.is_active {
                        result.push(node.clone());
                        if result.len() > replica_count {
                            break;
                        }
                    }
                }
            }
        }

        result
    }

    /// 获取所有活跃节点
    pub fn get_active_nodes(&self) -> Vec<PhysicalNode> {
        self.nodes
            .iter()
            .filter(|n| n.is_active)
            .map(|n| n.clone())
            .collect()
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取虚拟节点数量
    pub fn virtual_node_count(&self) -> usize {
        self.ring.read().len()
    }

    /// 更新节点状态
    pub fn update_node_status(&self, node_id: &str, is_active: bool) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            node.is_active = is_active;
        }
    }

    /// 更新节点负载
    pub fn update_node_load(&self, node_id: &str, load: f64) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            node.load = load;
        }
    }
}

impl Default for ConsistentHashRing {
    fn default() -> Self {
        Self::new(150) // 默认每个物理节点 150 个虚拟节点
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 分片路由器
// ═══════════════════════════════════════════════════════════════════════════

/// 分片键类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShardKeyType {
    /// 按账户 ID 分片
    AccountId,
    /// 按合约代码分片
    InstrumentId,
    /// 按订单 ID 分片
    OrderId,
    /// 自定义键
    Custom,
}

/// 分片配置
#[derive(Debug, Clone)]
pub struct ShardConfig {
    /// 分片键类型
    pub key_type: ShardKeyType,
    /// 副本数量
    pub replica_count: usize,
    /// 是否启用负载均衡
    pub load_balance: bool,
    /// 负载均衡阈值
    pub load_threshold: f64,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            key_type: ShardKeyType::AccountId,
            replica_count: 2,
            load_balance: true,
            load_threshold: 0.8,
        }
    }
}

/// 分片路由器
pub struct ShardRouter {
    /// 一致性哈希环
    ring: ConsistentHashRing,
    /// 配置
    config: ShardConfig,
    /// 路由缓存 (key -> node_id)
    cache: Arc<DashMap<String, String>>,
    /// 缓存 TTL (秒)
    cache_ttl: u64,
}

impl ShardRouter {
    pub fn new(config: ShardConfig) -> Self {
        Self {
            ring: ConsistentHashRing::default(),
            config,
            cache: Arc::new(DashMap::new()),
            cache_ttl: 60,
        }
    }

    /// 从合约代码提取分片键
    pub fn extract_shard_key(&self, key: &str) -> String {
        match self.config.key_type {
            ShardKeyType::AccountId | ShardKeyType::OrderId | ShardKeyType::Custom => {
                key.to_string()
            }
            ShardKeyType::InstrumentId => {
                // 提取交易所代码部分作为分片键 (例如: SHFE.cu2501 -> SHFE)
                key.split('.').next().unwrap_or(key).to_string()
            }
        }
    }

    /// 路由请求到目标节点
    pub fn route(&self, key: &str) -> Option<PhysicalNode> {
        let shard_key = self.extract_shard_key(key);

        // 先检查缓存
        if let Some(node_id) = self.cache.get(&shard_key) {
            if let Some(node) = self.ring.nodes.get(node_id.value()) {
                if node.is_active {
                    return Some(node.clone());
                }
            }
            // 缓存失效，移除
            self.cache.remove(&shard_key);
        }

        // 从哈希环获取节点
        let node = if self.config.load_balance {
            self.route_with_load_balance(&shard_key)
        } else {
            self.ring.get_node(&shard_key)
        };

        // 更新缓存
        if let Some(ref n) = node {
            self.cache.insert(shard_key, n.id.clone());
        }

        node
    }

    /// 带负载均衡的路由
    fn route_with_load_balance(&self, key: &str) -> Option<PhysicalNode> {
        let nodes = self.ring.get_nodes_with_replicas(key, self.config.replica_count);

        // 找负载最低的节点
        nodes
            .into_iter()
            .filter(|n| n.is_active && n.load < self.config.load_threshold)
            .min_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal))
            .or_else(|| self.ring.get_node(key))
    }

    /// 路由到所有副本节点
    pub fn route_to_replicas(&self, key: &str) -> Vec<PhysicalNode> {
        let shard_key = self.extract_shard_key(key);
        self.ring.get_nodes_with_replicas(&shard_key, self.config.replica_count)
    }

    /// 添加节点
    pub fn add_node(&self, node: PhysicalNode) {
        self.ring.add_node(node);
        // 清除可能受影响的缓存
        self.cache.clear();
    }

    /// 移除节点
    pub fn remove_node(&self, node_id: &str) {
        self.ring.remove_node(node_id);
        // 清除该节点的缓存
        self.cache.retain(|_, v| v != node_id);
    }

    /// 获取分片统计信息
    pub fn get_stats(&self) -> ShardStats {
        let nodes = self.ring.get_active_nodes();
        let total_load: f64 = nodes.iter().map(|n| n.load).sum();
        let avg_load = if nodes.is_empty() { 0.0 } else { total_load / nodes.len() as f64 };

        ShardStats {
            node_count: self.ring.node_count(),
            virtual_node_count: self.ring.virtual_node_count(),
            active_node_count: nodes.len(),
            cache_size: self.cache.len(),
            avg_load,
        }
    }
}

/// 分片统计信息
#[derive(Debug, Clone)]
pub struct ShardStats {
    pub node_count: usize,
    pub virtual_node_count: usize,
    pub active_node_count: usize,
    pub cache_size: usize,
    pub avg_load: f64,
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hash_ring() {
        let ring = ConsistentHashRing::new(100);

        // 添加节点
        ring.add_node(PhysicalNode::new("node1", "127.0.0.1:9001"));
        ring.add_node(PhysicalNode::new("node2", "127.0.0.1:9002"));
        ring.add_node(PhysicalNode::new("node3", "127.0.0.1:9003"));

        assert_eq!(ring.node_count(), 3);

        // 测试路由一致性
        let key = "user_12345";
        let node1 = ring.get_node(key).unwrap();
        let node2 = ring.get_node(key).unwrap();
        assert_eq!(node1.id, node2.id);

        // 测试节点移除后的再平衡
        let old_node = ring.get_node(key).unwrap();
        ring.remove_node(&old_node.id);
        assert_eq!(ring.node_count(), 2);

        // 同一个 key 应该路由到不同节点
        let new_node = ring.get_node(key).unwrap();
        assert_ne!(old_node.id, new_node.id);
    }

    #[test]
    fn test_replica_distribution() {
        let ring = ConsistentHashRing::new(100);

        ring.add_node(PhysicalNode::new("node1", "127.0.0.1:9001"));
        ring.add_node(PhysicalNode::new("node2", "127.0.0.1:9002"));
        ring.add_node(PhysicalNode::new("node3", "127.0.0.1:9003"));

        let replicas = ring.get_nodes_with_replicas("test_key", 2);
        assert_eq!(replicas.len(), 3); // 主节点 + 2 副本

        // 确保副本分布在不同节点
        let ids: std::collections::HashSet<_> = replicas.iter().map(|n| &n.id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_shard_router() {
        let router = ShardRouter::new(ShardConfig::default());

        router.add_node(PhysicalNode::new("node1", "127.0.0.1:9001"));
        router.add_node(PhysicalNode::new("node2", "127.0.0.1:9002"));

        // 测试路由
        let node = router.route("account_123").unwrap();
        assert!(node.id == "node1" || node.id == "node2");

        // 测试缓存
        let cached_node = router.route("account_123").unwrap();
        assert_eq!(node.id, cached_node.id);
    }

    #[test]
    fn test_weighted_nodes() {
        let ring = ConsistentHashRing::new(100);

        // 高权重节点应该有更多虚拟节点
        ring.add_node(PhysicalNode::new("node1", "127.0.0.1:9001").with_weight(200));
        ring.add_node(PhysicalNode::new("node2", "127.0.0.1:9002").with_weight(50));

        // 高权重节点应该承担更多负载
        let mut node1_count = 0;
        let mut node2_count = 0;

        for i in 0..1000 {
            let key = format!("key_{}", i);
            if let Some(node) = ring.get_node(&key) {
                if node.id == "node1" {
                    node1_count += 1;
                } else {
                    node2_count += 1;
                }
            }
        }

        // node1 应该承担约 4 倍的负载 (200:50)
        assert!(node1_count > node2_count * 2);
    }
}
