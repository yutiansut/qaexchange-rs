//! 因子依赖 DAG 管理器
//!
//! @yutiansut @quantaxis
//!
//! 提供因子依赖图的管理功能：
//! - 拓扑排序确定计算顺序
//! - 增量传播更新
//! - 循环依赖检测
//! - 并行计算调度

use dashmap::DashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// 因子节点定义
// ═══════════════════════════════════════════════════════════════════════════

/// 因子节点 ID
pub type FactorId = String;

/// 因子节点类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactorNodeType {
    /// 原始数据源 (如 price, volume)
    Source,
    /// 派生因子 (如 ma_20, rsi_14)
    Derived,
    /// 复合因子 (多因子组合)
    Composite,
}

/// 因子节点
#[derive(Debug, Clone)]
pub struct FactorNode {
    /// 因子 ID
    pub id: FactorId,
    /// 因子名称
    pub name: String,
    /// 节点类型
    pub node_type: FactorNodeType,
    /// 依赖的因子列表
    pub dependencies: Vec<FactorId>,
    /// 被依赖的因子列表 (反向边)
    pub dependents: Vec<FactorId>,
    /// 计算函数名称
    pub compute_fn: Option<String>,
    /// 参数
    pub params: HashMap<String, String>,
    /// 拓扑深度 (用于并行调度)
    pub depth: usize,
}

impl FactorNode {
    pub fn source(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            node_type: FactorNodeType::Source,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            compute_fn: None,
            params: HashMap::new(),
            depth: 0,
        }
    }

    pub fn derived(
        id: impl Into<String>,
        name: impl Into<String>,
        dependencies: Vec<FactorId>,
        compute_fn: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            node_type: FactorNodeType::Derived,
            dependencies,
            dependents: Vec::new(),
            compute_fn: Some(compute_fn.into()),
            params: HashMap::new(),
            depth: 0,
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn is_source(&self) -> bool {
        self.node_type == FactorNodeType::Source
    }

    pub fn is_derived(&self) -> bool {
        self.node_type == FactorNodeType::Derived
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DAG 错误类型
// ═══════════════════════════════════════════════════════════════════════════

/// DAG 错误
#[derive(Debug)]
pub enum DagError {
    /// 节点已存在
    NodeExists(FactorId),
    /// 节点不存在
    NodeNotFound(FactorId),
    /// 检测到循环依赖
    CycleDetected(Vec<FactorId>),
    /// 依赖节点不存在
    DependencyNotFound { node: FactorId, dependency: FactorId },
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DagError::NodeExists(id) => write!(f, "Node already exists: {}", id),
            DagError::NodeNotFound(id) => write!(f, "Node not found: {}", id),
            DagError::CycleDetected(path) => write!(f, "Cycle detected: {:?}", path),
            DagError::DependencyNotFound { node, dependency } => {
                write!(f, "Dependency {} not found for node {}", dependency, node)
            }
        }
    }
}

impl std::error::Error for DagError {}

pub type DagResult<T> = Result<T, DagError>;

// ═══════════════════════════════════════════════════════════════════════════
// 因子 DAG 管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 因子 DAG 管理器
pub struct FactorDag {
    /// 节点映射
    nodes: DashMap<FactorId, FactorNode>,
    /// 拓扑排序缓存
    topo_order: parking_lot::RwLock<Vec<FactorId>>,
    /// 是否需要重新排序
    dirty: std::sync::atomic::AtomicBool,
}

impl FactorDag {
    pub fn new() -> Self {
        Self {
            nodes: DashMap::new(),
            topo_order: parking_lot::RwLock::new(Vec::new()),
            dirty: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// 添加节点
    pub fn add_node(&self, node: FactorNode) -> DagResult<()> {
        let id = node.id.clone();

        // 检查节点是否已存在
        if self.nodes.contains_key(&id) {
            return Err(DagError::NodeExists(id));
        }

        // 检查依赖是否存在
        for dep in &node.dependencies {
            if !self.nodes.contains_key(dep) {
                return Err(DagError::DependencyNotFound {
                    node: id.clone(),
                    dependency: dep.clone(),
                });
            }
        }

        // 更新依赖节点的 dependents
        for dep in &node.dependencies {
            if let Some(mut dep_node) = self.nodes.get_mut(dep) {
                dep_node.dependents.push(id.clone());
            }
        }

        self.nodes.insert(id, node);
        self.dirty.store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// 添加源节点
    pub fn add_source(&self, id: impl Into<String>, name: impl Into<String>) -> DagResult<()> {
        self.add_node(FactorNode::source(id, name))
    }

    /// 添加派生节点
    pub fn add_derived(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        dependencies: Vec<FactorId>,
        compute_fn: impl Into<String>,
    ) -> DagResult<()> {
        self.add_node(FactorNode::derived(id, name, dependencies, compute_fn))
    }

    /// 移除节点
    pub fn remove_node(&self, id: &str) -> DagResult<()> {
        let node = self
            .nodes
            .remove(id)
            .ok_or_else(|| DagError::NodeNotFound(id.to_string()))?;

        // 清理依赖关系
        for dep in &node.1.dependencies {
            if let Some(mut dep_node) = self.nodes.get_mut(dep) {
                dep_node.dependents.retain(|x| x != id);
            }
        }

        // 清理被依赖关系
        for dependent in &node.1.dependents {
            if let Some(mut dep_node) = self.nodes.get_mut(dependent) {
                dep_node.dependencies.retain(|x| x != id);
            }
        }

        self.dirty.store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// 获取节点
    pub fn get_node(&self, id: &str) -> Option<FactorNode> {
        self.nodes.get(id).map(|r| r.clone())
    }

    /// 获取所有源节点
    pub fn get_sources(&self) -> Vec<FactorId> {
        self.nodes
            .iter()
            .filter(|r| r.is_source())
            .map(|r| r.id.clone())
            .collect()
    }

    /// 获取节点的直接依赖
    pub fn get_dependencies(&self, id: &str) -> Option<Vec<FactorId>> {
        self.nodes.get(id).map(|r| r.dependencies.clone())
    }

    /// 获取节点的所有依赖 (递归)
    pub fn get_all_dependencies(&self, id: &str) -> Option<HashSet<FactorId>> {
        let node = self.nodes.get(id)?;
        let mut deps = HashSet::new();
        let mut queue = VecDeque::from(node.dependencies.clone());

        while let Some(dep_id) = queue.pop_front() {
            if deps.insert(dep_id.clone()) {
                if let Some(dep_node) = self.nodes.get(&dep_id) {
                    queue.extend(dep_node.dependencies.clone());
                }
            }
        }

        Some(deps)
    }

    /// 获取节点的直接被依赖者
    pub fn get_dependents(&self, id: &str) -> Option<Vec<FactorId>> {
        self.nodes.get(id).map(|r| r.dependents.clone())
    }

    /// 获取受影响的节点 (当某节点更新时)
    pub fn get_affected_nodes(&self, source_id: &str) -> Vec<FactorId> {
        let mut affected = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(node) = self.nodes.get(source_id) {
            queue.extend(node.dependents.clone());
        }

        while let Some(id) = queue.pop_front() {
            if affected.insert(id.clone()) {
                if let Some(node) = self.nodes.get(&id) {
                    queue.extend(node.dependents.clone());
                }
            }
        }

        // 返回拓扑排序后的结果
        let topo = self.topological_sort();
        topo.into_iter()
            .filter(|id| affected.contains(id))
            .collect()
    }

    /// 拓扑排序 (Kahn's Algorithm)
    pub fn topological_sort(&self) -> Vec<FactorId> {
        // 检查缓存
        if !self.dirty.load(std::sync::atomic::Ordering::SeqCst) {
            return self.topo_order.read().clone();
        }

        let mut in_degree: HashMap<FactorId, usize> = HashMap::new();
        let mut graph: HashMap<FactorId, Vec<FactorId>> = HashMap::new();

        // 构建图
        for node in self.nodes.iter() {
            in_degree.entry(node.id.clone()).or_insert(0);
            graph.entry(node.id.clone()).or_insert_with(Vec::new);

            for dep in &node.dependencies {
                *in_degree.entry(node.id.clone()).or_insert(0) += 1;
                graph
                    .entry(dep.clone())
                    .or_insert_with(Vec::new)
                    .push(node.id.clone());
            }
        }

        // Kahn's Algorithm
        let mut queue: VecDeque<FactorId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();
        let mut depth: HashMap<FactorId, usize> = HashMap::new();

        while let Some(id) = queue.pop_front() {
            let current_depth = *depth.get(&id).unwrap_or(&0);
            result.push(id.clone());

            if let Some(dependents) = graph.get(&id) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        depth.insert(dep.clone(), current_depth + 1);
                        if *deg == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        // 更新节点深度
        for (id, d) in &depth {
            if let Some(mut node) = self.nodes.get_mut(id) {
                node.depth = *d;
            }
        }

        // 更新缓存
        *self.topo_order.write() = result.clone();
        self.dirty.store(false, std::sync::atomic::Ordering::SeqCst);

        result
    }

    /// 检测循环依赖
    pub fn detect_cycle(&self) -> Option<Vec<FactorId>> {
        let topo = self.topological_sort();

        // 如果拓扑排序结果不包含所有节点，说明存在循环
        if topo.len() != self.nodes.len() {
            // 找出循环路径
            let topo_set: HashSet<_> = topo.into_iter().collect();
            let cycle_nodes: Vec<_> = self
                .nodes
                .iter()
                .filter(|r| !topo_set.contains(&r.id))
                .map(|r| r.id.clone())
                .collect();

            if !cycle_nodes.is_empty() {
                return Some(cycle_nodes);
            }
        }

        None
    }

    /// 获取并行计算层级
    pub fn get_parallel_levels(&self) -> Vec<Vec<FactorId>> {
        let topo = self.topological_sort();
        let mut levels: HashMap<usize, Vec<FactorId>> = HashMap::new();

        for id in topo {
            if let Some(node) = self.nodes.get(&id) {
                levels.entry(node.depth).or_insert_with(Vec::new).push(id);
            }
        }

        let mut result: Vec<(usize, Vec<FactorId>)> = levels.into_iter().collect();
        result.sort_by_key(|(level, _)| *level);
        result.into_iter().map(|(_, nodes)| nodes).collect()
    }

    /// 节点数量
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 清空 DAG
    pub fn clear(&self) {
        self.nodes.clear();
        self.topo_order.write().clear();
        self.dirty.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for FactorDag {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 预定义因子 DAG 模板
// ═══════════════════════════════════════════════════════════════════════════

/// 创建标准技术指标 DAG
pub fn create_standard_factor_dag() -> DagResult<FactorDag> {
    let dag = FactorDag::new();

    // 源节点
    dag.add_source("price", "Last Price")?;
    dag.add_source("volume", "Volume")?;
    dag.add_source("high", "High Price")?;
    dag.add_source("low", "Low Price")?;
    dag.add_source("close", "Close Price")?;

    // 一级派生因子
    dag.add_derived("ma_5", "MA(5)", vec!["price".to_string()], "rolling_mean")?;
    dag.add_derived("ma_10", "MA(10)", vec!["price".to_string()], "rolling_mean")?;
    dag.add_derived("ma_20", "MA(20)", vec!["price".to_string()], "rolling_mean")?;
    dag.add_derived("ma_60", "MA(60)", vec!["price".to_string()], "rolling_mean")?;

    dag.add_derived("std_20", "STD(20)", vec!["price".to_string()], "rolling_std")?;

    dag.add_derived("ema_12", "EMA(12)", vec!["price".to_string()], "ema")?;
    dag.add_derived("ema_26", "EMA(26)", vec!["price".to_string()], "ema")?;

    dag.add_derived("rsi_14", "RSI(14)", vec!["price".to_string()], "rsi")?;

    dag.add_derived(
        "atr_14",
        "ATR(14)",
        vec!["high".to_string(), "low".to_string(), "close".to_string()],
        "atr",
    )?;

    // 二级派生因子
    dag.add_derived(
        "macd_line",
        "MACD Line",
        vec!["ema_12".to_string(), "ema_26".to_string()],
        "subtract",
    )?;

    dag.add_derived(
        "bollinger_upper",
        "Bollinger Upper",
        vec!["ma_20".to_string(), "std_20".to_string()],
        "bollinger_upper",
    )?;

    dag.add_derived(
        "bollinger_lower",
        "Bollinger Lower",
        vec!["ma_20".to_string(), "std_20".to_string()],
        "bollinger_lower",
    )?;

    // 三级派生因子
    dag.add_derived(
        "macd_signal",
        "MACD Signal",
        vec!["macd_line".to_string()],
        "ema",
    )?;

    dag.add_derived(
        "macd_histogram",
        "MACD Histogram",
        vec!["macd_line".to_string(), "macd_signal".to_string()],
        "subtract",
    )?;

    Ok(dag)
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_dag() {
        let dag = FactorDag::new();

        // 添加源节点
        dag.add_source("price", "Price").unwrap();
        dag.add_source("volume", "Volume").unwrap();

        // 添加派生节点
        dag.add_derived("ma_20", "MA(20)", vec!["price".to_string()], "rolling_mean")
            .unwrap();

        assert_eq!(dag.len(), 3);
    }

    #[test]
    fn test_topological_sort() {
        let dag = FactorDag::new();

        dag.add_source("A", "Source A").unwrap();
        dag.add_derived("B", "Derived B", vec!["A".to_string()], "fn_b")
            .unwrap();
        dag.add_derived("C", "Derived C", vec!["A".to_string()], "fn_c")
            .unwrap();
        dag.add_derived(
            "D",
            "Derived D",
            vec!["B".to_string(), "C".to_string()],
            "fn_d",
        )
        .unwrap();

        let topo = dag.topological_sort();

        // A 必须在 B 和 C 之前
        let pos_a = topo.iter().position(|x| x == "A").unwrap();
        let pos_b = topo.iter().position(|x| x == "B").unwrap();
        let pos_c = topo.iter().position(|x| x == "C").unwrap();
        let pos_d = topo.iter().position(|x| x == "D").unwrap();

        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_affected_nodes() {
        let dag = FactorDag::new();

        dag.add_source("price", "Price").unwrap();
        dag.add_derived("ma_20", "MA(20)", vec!["price".to_string()], "rolling_mean")
            .unwrap();
        dag.add_derived(
            "ma_diff",
            "MA Diff",
            vec!["ma_20".to_string()],
            "diff",
        )
        .unwrap();

        let affected = dag.get_affected_nodes("price");

        assert!(affected.contains(&"ma_20".to_string()));
        assert!(affected.contains(&"ma_diff".to_string()));
    }

    #[test]
    fn test_parallel_levels() {
        let dag = FactorDag::new();

        dag.add_source("A", "A").unwrap();
        dag.add_source("B", "B").unwrap();
        dag.add_derived("C", "C", vec!["A".to_string()], "fn")
            .unwrap();
        dag.add_derived("D", "D", vec!["B".to_string()], "fn")
            .unwrap();
        dag.add_derived(
            "E",
            "E",
            vec!["C".to_string(), "D".to_string()],
            "fn",
        )
        .unwrap();

        let levels = dag.get_parallel_levels();

        // Level 0: A, B
        // Level 1: C, D
        // Level 2: E
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].len(), 2);
        assert_eq!(levels[1].len(), 2);
        assert_eq!(levels[2].len(), 1);
    }

    #[test]
    fn test_standard_factor_dag() {
        let dag = create_standard_factor_dag().unwrap();

        assert!(dag.len() > 10);
        assert!(dag.detect_cycle().is_none());

        // 验证 MACD 依赖链
        let macd_deps = dag.get_all_dependencies("macd_histogram").unwrap();
        assert!(macd_deps.contains(&"price".to_string()));
        assert!(macd_deps.contains(&"ema_12".to_string()));
        assert!(macd_deps.contains(&"ema_26".to_string()));
    }
}
