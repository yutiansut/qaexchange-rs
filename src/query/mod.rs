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
pub use hybrid::*;
pub use router::*;
pub use scanner::SSTableScanner;
pub use types::*;
pub use unified::{
    unified_query_engine, UnifiedQueryConfig, UnifiedQueryEngine, UnifiedQueryEngineBuilder,
    UnifiedQueryError, UnifiedQueryStats,
};
