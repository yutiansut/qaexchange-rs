// 查询引擎模块 - Phase 8
//
// 架构：
// ┌─────────────────────────────────────────────────────────────┐
// │                      Query Layer                            │
// │                                                             │
// │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
// │  │ QueryEngine │  │HybridQuery  │  │UnifiedQuery │         │
// │  │ (SQL/Polars)│  │(Stream+Batch)│ │(Stream+OLTP │         │
// │  │             │  │             │  │    +OLAP)   │         │
// │  └─────────────┘  └─────────────┘  └─────────────┘         │
// │         │                │                │                 │
// │         └────────────────┴────────────────┘                │
// │                          │                                  │
// │                 ┌────────▼────────┐                        │
// │                 │  SSTableScanner │                        │
// │                 │  (OLTP + OLAP)  │                        │
// │                 └─────────────────┘                        │
// └─────────────────────────────────────────────────────────────┘
//
// @yutiansut @quantaxis

pub mod engine;
pub mod hybrid;
pub mod router;
pub mod scanner;
pub mod types;
pub mod unified;

pub use engine::QueryEngine;
pub use scanner::SSTableScanner;

// hybrid 模块导出（避免与 types/router 冲突）
pub use hybrid::{
    AggregateOp, AggregateResult, Aggregation as HybridAggregation, BatchDataSource,
    BatchQueryError, DataSource, HybridConfig, HybridQueryEngine, HybridQueryError,
    MergeStrategy, QueryResult, Record, RecordValue, StreamBuffer,
};

// router 模块导出（避免与 types 冲突）
pub use router::{
    AggregationOp, ConditionOp, OrderByField, QueryCondition, QueryHistory,
    QueryRequest as RouterQueryRequest, QueryRouter, QueryTarget, QueryType as RouterQueryType,
    QueryValue, RouterConfig, RoutingDecision, TableStats, TimeRange as RouterTimeRange,
};

// types 模块导出
pub use types::{
    AggType, Aggregation, AggregationResult, Filter, FilterOp, FilterValue, OrderBy,
    QueryRequest, QueryResponse, QueryType, TimeRange, TimeSeriesResult,
};

pub use unified::{
    unified_query_engine, UnifiedQueryConfig, UnifiedQueryEngine, UnifiedQueryEngineBuilder,
    UnifiedQueryError, UnifiedQueryStats,
};
