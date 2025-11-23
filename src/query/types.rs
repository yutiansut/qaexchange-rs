// 查询引擎类型定义

use serde::{Deserialize, Serialize};

/// 查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// 查询类型
    pub query_type: QueryType,

    /// 时间范围（可选）
    pub time_range: Option<TimeRange>,

    /// 过滤条件（可选）
    pub filters: Option<Vec<Filter>>,

    /// 聚合函数（可选）
    pub aggregations: Option<Vec<Aggregation>>,

    /// 排序（可选）
    pub order_by: Option<Vec<OrderBy>>,

    /// 限制返回行数（可选）
    pub limit: Option<usize>,
}

/// 查询类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryType {
    /// SQL 查询
    Sql { query: String },

    /// 结构化查询（类似 Elasticsearch）
    Structured {
        /// 投影列
        select: Vec<String>,
        /// 表名/数据源
        from: String,
    },

    /// 时间序列查询（专门针对交易数据）
    TimeSeries {
        /// 指标列
        metrics: Vec<String>,
        /// 维度列
        dimensions: Vec<String>,
        /// 时间粒度（秒）
        granularity: Option<i64>,
    },
}

/// 时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    /// 起始时间戳（纳秒）
    pub start: i64,
    /// 结束时间戳（纳秒）
    pub end: i64,
}

/// 过滤条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// 列名
    pub column: String,
    /// 操作符
    pub op: FilterOp,
    /// 值
    pub value: FilterValue,
}

/// 过滤操作符
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,    // ==
    Ne,    // !=
    Gt,    // >
    Gte,   // >=
    Lt,    // <
    Lte,   // <=
    In,    // IN
    NotIn, // NOT IN
}

/// 过滤值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterValue {
    Int(i64),
    Float(f64),
    String(String),
    IntList(Vec<i64>),
    StringList(Vec<String>),
}

/// 聚合函数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregation {
    /// 聚合类型
    pub agg_type: AggType,
    /// 列名
    pub column: String,
    /// 别名
    pub alias: Option<String>,
}

/// 聚合类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggType {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    First,
    Last,
}

/// 排序
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBy {
    /// 列名
    pub column: String,
    /// 是否降序
    pub descending: bool,
}

/// 查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// 列名
    pub columns: Vec<String>,

    /// 数据类型
    pub dtypes: Vec<String>,

    /// 数据（JSON 格式）
    pub data: serde_json::Value,

    /// 返回行数
    pub row_count: usize,

    /// 查询耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    /// 指标名
    pub metric: String,

    /// 聚合值
    pub value: f64,
}

/// 时间序列查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesResult {
    /// 时间戳
    pub timestamp: i64,

    /// 维度值
    pub dimensions: Vec<(String, String)>,

    /// 指标值
    pub metrics: Vec<(String, f64)>,
}
