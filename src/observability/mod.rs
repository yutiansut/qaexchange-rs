//! 可观测性模块
//!
//! @yutiansut @quantaxis
//!
//! 提供完整的可观测性支持：
//! - Prometheus 指标导出
//! - OpenTelemetry 分布式追踪
//! - 结构化日志
//! - 性能分析

pub mod metrics;
pub mod tracing;

pub use metrics::*;
pub use tracing::{
    TracingConfig, TracingInitializer, TracingError, ExporterType, BatchExportConfig,
    init_dev_tracing, init_prod_tracing, init_test_tracing, shutdown_tracer,
};
