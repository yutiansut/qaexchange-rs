# 集群管理系统

**版本**: v1.0.0
**作者**: @yutiansut @quantaxis
**最后更新**: 2025-11-24

---

## 概述

集群管理系统提供分布式部署能力，支持多节点水平扩展。核心功能包括：

- **一致性哈希分片**: 数据均匀分布，节点动态扩缩容
- **虚拟节点映射**: 负载均衡，避免数据倾斜
- **节点发现与管理**: 心跳检测，自动故障转移
- **路由缓存**: 降低路由计算开销

```
┌─────────────────────────────────────────────────────────────┐
│                    集群架构                                  │
│                                                             │
│    Client Request                                           │
│          │                                                  │
│          ▼                                                  │
│    ┌─────────────┐                                         │
│    │ ShardRouter │ ◀─── 路由缓存                            │
│    └─────────────┘                                         │
│          │                                                  │
│          ▼                                                  │
│    ┌─────────────────────────────┐                         │
│    │   ConsistentHashRing        │                         │
│    │                             │                         │
│    │  0 ────────────── 2^64      │                         │
│    │  │  N1  N2  N3  N1  N2  N3  │ ◀─── 虚拟节点            │
│    │  └─────────────────────────┘                         │
│    └─────────────────────────────┘                         │
│          │                                                  │
│          ▼                                                  │
│    ┌─────────┐  ┌─────────┐  ┌─────────┐                   │
│    │  Node1  │  │  Node2  │  │  Node3  │                   │
│    │ (主节点) │  │ (从节点) │  │ (从节点) │                   │
│    └─────────┘  └─────────┘  └─────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

---

## 核心组件

### 1. 一致性哈希环 (`ConsistentHashRing`)

使用一致性哈希算法实现数据分片，确保节点变更时最小化数据迁移：

```rust
use qaexchange::cluster::{ConsistentHashRing, PhysicalNode};

// 创建哈希环 (每个物理节点 150 个虚拟节点)
let ring = ConsistentHashRing::new(150);

// 添加物理节点
ring.add_node(PhysicalNode::new("node1", "192.168.1.1:9001"));
ring.add_node(PhysicalNode::new("node2", "192.168.1.2:9001"));
ring.add_node(PhysicalNode::new("node3", "192.168.1.3:9001"));

// 路由请求
let key = "user_12345";
if let Some(node) = ring.get_node(key) {
    println!("Route to: {} @ {}", node.id, node.addr);
}

// 一致性验证 - 同一 key 总是路由到同一节点
let node1 = ring.get_node(key).unwrap();
let node2 = ring.get_node(key).unwrap();
assert_eq!(node1.id, node2.id);
```

#### 虚拟节点机制

虚拟节点解决了物理节点数量少时负载不均的问题：

```
物理节点: 3 个
虚拟节点: 3 × 150 = 450 个

哈希环分布:
0         25%        50%        75%       100%
├─N1─N2─N3─N1─N2─N3─N1─N2─N3─N1─N2─N3─...─┤
```

---

### 2. 带权重节点

支持节点权重配置，高性能节点承担更多负载：

```rust
// 高性能节点 - 200% 权重
ring.add_node(
    PhysicalNode::new("node1", "192.168.1.1:9001")
        .with_weight(200)  // 2倍虚拟节点数
);

// 普通节点 - 100% 权重 (默认)
ring.add_node(PhysicalNode::new("node2", "192.168.1.2:9001"));

// 低配节点 - 50% 权重
ring.add_node(
    PhysicalNode::new("node3", "192.168.1.3:9001")
        .with_weight(50)   // 0.5倍虚拟节点数
);

// 负载分布: node1 ≈ 50%, node2 ≈ 33%, node3 ≈ 17%
```

---

### 3. 副本分布

支持多副本部署，提高数据可靠性：

```rust
// 获取主节点 + 2 个副本节点
let replicas = ring.get_nodes_with_replicas("order_98765", 2);

// replicas 包含 3 个不同的物理节点
// [主节点, 副本1, 副本2]
for node in &replicas {
    println!("{}: {}", node.id, node.addr);
}

// 写入时同步到所有副本
for node in replicas {
    send_to_node(&node.addr, data).await?;
}
```

---

### 4. 分片路由器 (`ShardRouter`)

高级路由器，支持缓存和负载均衡：

```rust
use qaexchange::cluster::{ShardRouter, ShardConfig, ShardKeyType};

// 配置路由器
let config = ShardConfig {
    key_type: ShardKeyType::AccountId,  // 按账户 ID 分片
    replica_count: 2,                    // 2 副本
    load_balance: true,                  // 启用负载均衡
    load_threshold: 0.8,                 // 80% 负载阈值
};

let router = ShardRouter::new(config);

// 添加节点
router.add_node(PhysicalNode::new("node1", "192.168.1.1:9001"));
router.add_node(PhysicalNode::new("node2", "192.168.1.2:9001"));

// 路由请求 (带缓存)
if let Some(node) = router.route("account_123") {
    println!("Route to: {}", node.id);
}

// 获取统计信息
let stats = router.get_stats();
println!("节点数: {}", stats.node_count);
println!("虚拟节点数: {}", stats.virtual_node_count);
println!("活跃节点: {}", stats.active_node_count);
println!("缓存命中: {}", stats.cache_size);
println!("平均负载: {:.2}%", stats.avg_load * 100.0);
```

---

### 5. 分片键类型

支持多种分片策略：

```rust
pub enum ShardKeyType {
    /// 按账户 ID 分片 - 同一账户的所有操作路由到同一节点
    AccountId,

    /// 按合约代码分片 - 同一合约的订单路由到同一节点
    InstrumentId,

    /// 按订单 ID 分片 - 适用于订单查询
    OrderId,

    /// 自定义键 - 直接使用传入的 key
    Custom,
}
```

#### 合约分片示例

```rust
let config = ShardConfig {
    key_type: ShardKeyType::InstrumentId,
    ..Default::default()
};
let router = ShardRouter::new(config);

// SHFE.cu2501 和 SHFE.cu2502 会被路由到同一节点
// 因为提取交易所代码 "SHFE" 作为分片键
router.route("SHFE.cu2501"); // -> node1
router.route("SHFE.cu2502"); // -> node1
router.route("DCE.i2501");   // -> node2 (不同交易所)
```

---

### 6. 节点状态管理

支持节点上下线和负载更新：

```rust
// 节点下线
ring.update_node_status("node2", false);

// 下线后的路由会跳过该节点
let node = ring.get_node("user_123"); // 不会返回 node2

// 节点恢复
ring.update_node_status("node2", true);

// 更新节点负载 (用于负载均衡决策)
ring.update_node_load("node1", 0.85); // 85% 负载
ring.update_node_load("node2", 0.30); // 30% 负载

// 负载均衡路由会优先选择低负载节点
```

---

## 与复制系统集成

集群管理与主从复制系统协同工作：

```rust
use qaexchange::cluster::ShardRouter;
use qaexchange::replication::grpc::{ClusterManager, GrpcConfig};

// 创建集群管理器
let cluster = ClusterManager::new(GrpcConfig {
    bind_addr: "0.0.0.0:50051".to_string(),
    peers: vec![
        "192.168.1.2:50051".to_string(),
        "192.168.1.3:50051".to_string(),
    ],
});

// 初始化路由器
let router = ShardRouter::new(ShardConfig::default());

// 同步节点状态
for node in cluster.get_active_nodes() {
    router.add_node(PhysicalNode::new(&node.id, &node.addr));
}

// 监听节点变更
cluster.on_node_change(|event| {
    match event {
        NodeEvent::Added(node) => router.add_node(node),
        NodeEvent::Removed(id) => router.remove_node(&id),
        NodeEvent::StatusChanged(id, active) => {
            ring.update_node_status(&id, active);
        }
    }
});
```

---

## 性能指标

| 操作 | 延迟 | 说明 |
|------|------|------|
| `get_node()` | ~100 ns | 哈希计算 + BTreeMap 查找 |
| `route()` (缓存命中) | ~20 ns | DashMap 读取 |
| `route()` (缓存未命中) | ~150 ns | 哈希计算 + 缓存写入 |
| `add_node()` | ~1 ms | 生成虚拟节点并插入 |
| `remove_node()` | ~500 μs | 清理虚拟节点 |

---

## 配置建议

| 场景 | 虚拟节点数 | 副本数 | 负载阈值 |
|------|-----------|--------|---------|
| 小规模 (3-5 节点) | 150 | 2 | 0.8 |
| 中规模 (10-20 节点) | 100 | 2-3 | 0.75 |
| 大规模 (50+ 节点) | 50 | 3 | 0.7 |

---

## 文件结构

```
src/cluster/
├── mod.rs              # 模块导出
└── consistent_hash.rs  # 一致性哈希实现
    ├── ConsistentHashRing   # 哈希环
    ├── PhysicalNode         # 物理节点
    ├── VirtualNode          # 虚拟节点
    ├── ShardRouter          # 分片路由器
    ├── ShardConfig          # 路由配置
    └── ShardStats           # 路由统计
```

---

## 相关文档

- [复制系统](../storage/replication.md) - 主从复制与故障转移
- [系统架构](../../02_architecture/system_overview.md) - 整体架构
- [高性能设计](../../02_architecture/high_performance.md) - 性能优化
