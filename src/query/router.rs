//! 查询路由器
//!
//! @yutiansut @quantaxis
//!
//! 提供查询请求的智能路由功能：
//! - 根据查询类型路由到 OLTP/OLAP 引擎
//! - 时间范围查询优化
//! - 缓存命中检测
//! - 负载均衡 (集群模式)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::RwLock;

// ═══════════════════════════════════════════════════════════════════════════
// 查询类型定义
// ═══════════════════════════════════════════════════════════════════════════

/// 查询类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryType {
    /// 点查询 (按主键)
    PointLookup,
    /// 范围查询
    RangeScan,
    /// 聚合查询
    Aggregation,
    /// 时序查询
    TimeSeries,
    /// 因子查询
    Factor,
    /// 复合查询
    Complex,
}

/// 查询目标
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryTarget {
    /// OLTP 存储 (低延迟)
    OLTP,
    /// OLAP 存储 (大批量)
    OLAP,
    /// 内存缓存
    Cache,
    /// 混合查询
    Hybrid,
}

/// 路由决策
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub target: QueryTarget,
    pub priority: u8,
    pub estimated_latency_ms: u32,
    pub use_cache: bool,
    pub parallel_degree: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// 查询请求
// ═══════════════════════════════════════════════════════════════════════════

/// 查询请求
#[derive(Debug, Clone)]
pub struct QueryRequest {
    /// 请求 ID
    pub request_id: String,
    /// 查询类型
    pub query_type: QueryType,
    /// 目标表/集合
    pub table: String,
    /// 查询条件
    pub conditions: Vec<QueryCondition>,
    /// 时间范围
    pub time_range: Option<TimeRange>,
    /// 投影字段
    pub select_fields: Vec<String>,
    /// 聚合操作
    pub aggregations: Vec<AggregationOp>,
    /// 分组字段
    pub group_by: Vec<String>,
    /// 排序字段
    pub order_by: Vec<OrderByField>,
    /// 限制数量
    pub limit: Option<usize>,
    /// 偏移量
    pub offset: Option<usize>,
    /// 超时时间
    pub timeout: Duration,
}

impl QueryRequest {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            query_type: QueryType::PointLookup,
            table: table.into(),
            conditions: Vec::new(),
            time_range: None,
            select_fields: Vec::new(),
            aggregations: Vec::new(),
            group_by: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_type(mut self, query_type: QueryType) -> Self {
        self.query_type = query_type;
        self
    }

    pub fn with_condition(mut self, condition: QueryCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn with_time_range(mut self, start: i64, end: i64) -> Self {
        self.time_range = Some(TimeRange { start, end });
        self.query_type = QueryType::TimeSeries;
        self
    }

    pub fn with_aggregation(mut self, op: AggregationOp) -> Self {
        self.aggregations.push(op);
        self.query_type = QueryType::Aggregation;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// 查询条件
#[derive(Debug, Clone)]
pub struct QueryCondition {
    pub field: String,
    pub operator: ConditionOp,
    pub value: QueryValue,
}

/// 条件操作符
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
    Like,
    Between,
}

/// 查询值
#[derive(Debug, Clone)]
pub enum QueryValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<QueryValue>),
}

/// 时间范围
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}

/// 聚合操作
#[derive(Debug, Clone)]
pub enum AggregationOp {
    Count,
    Sum(String),
    Avg(String),
    Min(String),
    Max(String),
    First(String),
    Last(String),
    StdDev(String),
    Variance(String),
}

/// 排序字段
#[derive(Debug, Clone)]
pub struct OrderByField {
    pub field: String,
    pub desc: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
// 查询路由器
// ═══════════════════════════════════════════════════════════════════════════

/// 路由器配置
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// 点查询阈值 (行数)
    pub point_query_threshold: usize,
    /// 时序查询使用 OLAP 的时间跨度阈值 (秒)
    pub olap_time_span_threshold: i64,
    /// 聚合查询行数阈值
    pub aggregation_row_threshold: usize,
    /// 缓存 TTL
    pub cache_ttl: Duration,
    /// 最大并行度
    pub max_parallel_degree: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            point_query_threshold: 100,
            olap_time_span_threshold: 3600, // 1 小时
            aggregation_row_threshold: 10000,
            cache_ttl: Duration::from_secs(60),
            max_parallel_degree: 4,
        }
    }
}

/// 查询路由器
pub struct QueryRouter {
    config: RouterConfig,
    /// 表统计信息缓存
    table_stats: DashMap<String, TableStats>,
    /// 路由缓存
    routing_cache: DashMap<String, (RoutingDecision, Instant)>,
    /// 查询历史 (用于自适应路由)
    query_history: RwLock<QueryHistory>,
}

/// 表统计信息
#[derive(Debug, Clone, Default)]
pub struct TableStats {
    pub row_count: u64,
    pub avg_row_size: usize,
    pub index_columns: Vec<String>,
    pub time_column: Option<String>,
    pub last_updated: Option<Instant>,
}

/// 查询历史
#[derive(Debug, Default)]
pub struct QueryHistory {
    pub total_queries: u64,
    pub oltp_queries: u64,
    pub olap_queries: u64,
    pub cache_hits: u64,
    pub avg_latency_ms: f64,
}

impl QueryRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            table_stats: DashMap::new(),
            routing_cache: DashMap::new(),
            query_history: RwLock::new(QueryHistory::default()),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RouterConfig::default())
    }

    /// 路由查询
    pub fn route(&self, request: &QueryRequest) -> RoutingDecision {
        // 检查路由缓存
        let cache_key = self.compute_cache_key(request);
        if let Some(entry) = self.routing_cache.get(&cache_key) {
            let (decision, created_at) = entry.value();
            if created_at.elapsed() < self.config.cache_ttl {
                return decision.clone();
            }
        }

        // 计算路由决策
        let decision = self.compute_routing_decision(request);

        // 更新缓存
        self.routing_cache
            .insert(cache_key, (decision.clone(), Instant::now()));

        // 更新历史
        self.update_history(&decision);

        decision
    }

    /// 计算路由决策
    fn compute_routing_decision(&self, request: &QueryRequest) -> RoutingDecision {
        match request.query_type {
            QueryType::PointLookup => self.route_point_lookup(request),
            QueryType::RangeScan => self.route_range_scan(request),
            QueryType::Aggregation => self.route_aggregation(request),
            QueryType::TimeSeries => self.route_time_series(request),
            QueryType::Factor => self.route_factor(request),
            QueryType::Complex => self.route_complex(request),
        }
    }

    /// 点查询路由
    fn route_point_lookup(&self, _request: &QueryRequest) -> RoutingDecision {
        RoutingDecision {
            target: QueryTarget::OLTP,
            priority: 1,
            estimated_latency_ms: 1,
            use_cache: true,
            parallel_degree: 1,
        }
    }

    /// 范围扫描路由
    fn route_range_scan(&self, request: &QueryRequest) -> RoutingDecision {
        let stats = self.table_stats.get(&request.table);
        let row_count = stats.map(|s| s.row_count).unwrap_or(0);

        // 小范围用 OLTP，大范围用 OLAP
        if row_count < self.config.point_query_threshold as u64 {
            RoutingDecision {
                target: QueryTarget::OLTP,
                priority: 2,
                estimated_latency_ms: 5,
                use_cache: true,
                parallel_degree: 1,
            }
        } else {
            RoutingDecision {
                target: QueryTarget::OLAP,
                priority: 3,
                estimated_latency_ms: 50,
                use_cache: false,
                parallel_degree: self.config.max_parallel_degree.min(4),
            }
        }
    }

    /// 聚合查询路由
    fn route_aggregation(&self, request: &QueryRequest) -> RoutingDecision {
        let stats = self.table_stats.get(&request.table);
        let row_count = stats.map(|s| s.row_count).unwrap_or(0);

        if row_count > self.config.aggregation_row_threshold as u64 {
            // 大数据量聚合走 OLAP
            RoutingDecision {
                target: QueryTarget::OLAP,
                priority: 3,
                estimated_latency_ms: 100,
                use_cache: true,
                parallel_degree: self.config.max_parallel_degree,
            }
        } else {
            // 小数据量聚合走 OLTP
            RoutingDecision {
                target: QueryTarget::OLTP,
                priority: 2,
                estimated_latency_ms: 10,
                use_cache: true,
                parallel_degree: 1,
            }
        }
    }

    /// 时序查询路由
    fn route_time_series(&self, request: &QueryRequest) -> RoutingDecision {
        let time_span = request
            .time_range
            .as_ref()
            .map(|r| r.end - r.start)
            .unwrap_or(0);

        if time_span > self.config.olap_time_span_threshold {
            // 长时间跨度走 OLAP
            RoutingDecision {
                target: QueryTarget::OLAP,
                priority: 3,
                estimated_latency_ms: 200,
                use_cache: false,
                parallel_degree: self.config.max_parallel_degree,
            }
        } else {
            // 短时间跨度走 OLTP/Cache
            RoutingDecision {
                target: QueryTarget::Hybrid,
                priority: 2,
                estimated_latency_ms: 20,
                use_cache: true,
                parallel_degree: 2,
            }
        }
    }

    /// 因子查询路由
    fn route_factor(&self, _request: &QueryRequest) -> RoutingDecision {
        // 因子查询优先走缓存
        RoutingDecision {
            target: QueryTarget::Cache,
            priority: 1,
            estimated_latency_ms: 1,
            use_cache: true,
            parallel_degree: 1,
        }
    }

    /// 复杂查询路由
    fn route_complex(&self, request: &QueryRequest) -> RoutingDecision {
        // 复杂查询走混合路径
        let has_aggregation = !request.aggregations.is_empty();
        let has_time_range = request.time_range.is_some();

        if has_aggregation && has_time_range {
            RoutingDecision {
                target: QueryTarget::OLAP,
                priority: 4,
                estimated_latency_ms: 500,
                use_cache: false,
                parallel_degree: self.config.max_parallel_degree,
            }
        } else {
            RoutingDecision {
                target: QueryTarget::Hybrid,
                priority: 3,
                estimated_latency_ms: 100,
                use_cache: true,
                parallel_degree: 2,
            }
        }
    }

    /// 计算缓存键
    fn compute_cache_key(&self, request: &QueryRequest) -> String {
        format!(
            "{}:{}:{:?}:{:?}",
            request.table,
            request.query_type as u8,
            request.time_range,
            request.aggregations.len()
        )
    }

    /// 更新查询历史
    fn update_history(&self, decision: &RoutingDecision) {
        let mut history = self.query_history.write();
        history.total_queries += 1;

        match decision.target {
            QueryTarget::OLTP => history.oltp_queries += 1,
            QueryTarget::OLAP => history.olap_queries += 1,
            QueryTarget::Cache => history.cache_hits += 1,
            QueryTarget::Hybrid => {
                history.oltp_queries += 1;
                history.olap_queries += 1;
            }
        }
    }

    /// 更新表统计信息
    pub fn update_table_stats(&self, table: &str, stats: TableStats) {
        self.table_stats.insert(table.to_string(), stats);
    }

    /// 获取查询历史统计
    pub fn get_stats(&self) -> QueryHistory {
        self.query_history.read().clone()
    }

    /// 清理过期缓存
    pub fn cleanup_cache(&self) {
        self.routing_cache.retain(|_, (_, created_at)| {
            created_at.elapsed() < self.config.cache_ttl
        });
    }
}

impl Default for QueryRouter {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_lookup_routing() {
        let router = QueryRouter::with_defaults();

        let request = QueryRequest::new("orders")
            .with_type(QueryType::PointLookup)
            .with_condition(QueryCondition {
                field: "order_id".to_string(),
                operator: ConditionOp::Eq,
                value: QueryValue::String("123".to_string()),
            });

        let decision = router.route(&request);

        assert_eq!(decision.target, QueryTarget::OLTP);
        assert!(decision.use_cache);
    }

    #[test]
    fn test_time_series_routing() {
        let router = QueryRouter::with_defaults();

        // 短时间跨度
        let request = QueryRequest::new("ticks")
            .with_time_range(1000, 2000); // 1000 秒

        let decision = router.route(&request);
        assert_eq!(decision.target, QueryTarget::Hybrid);

        // 长时间跨度
        let request = QueryRequest::new("ticks")
            .with_time_range(1000, 10000); // 9000 秒

        let decision = router.route(&request);
        assert_eq!(decision.target, QueryTarget::OLAP);
    }

    #[test]
    fn test_aggregation_routing() {
        let router = QueryRouter::with_defaults();

        // 更新表统计
        router.update_table_stats("trades", TableStats {
            row_count: 1_000_000,
            avg_row_size: 100,
            index_columns: vec!["trade_id".to_string()],
            time_column: Some("timestamp".to_string()),
            last_updated: Some(Instant::now()),
        });

        let request = QueryRequest::new("trades")
            .with_aggregation(AggregationOp::Sum("volume".to_string()));

        let decision = router.route(&request);

        assert_eq!(decision.target, QueryTarget::OLAP);
        assert!(decision.parallel_degree > 1);
    }

    #[test]
    fn test_cache_key() {
        let router = QueryRouter::with_defaults();

        let request1 = QueryRequest::new("orders")
            .with_type(QueryType::PointLookup);

        let request2 = QueryRequest::new("orders")
            .with_type(QueryType::PointLookup);

        // 相同查询应该产生相同的缓存键
        let key1 = router.compute_cache_key(&request1);
        let key2 = router.compute_cache_key(&request2);

        assert_eq!(key1, key2);
    }
}
