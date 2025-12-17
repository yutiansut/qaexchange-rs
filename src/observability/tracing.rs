//! OpenTelemetry 分布式追踪模块
//!
//! @yutiansut @quantaxis
//!
//! 提供完整的分布式追踪能力：
//! - OTLP 导出器
//! - 自动 span 传播
//! - 关键路径性能分析
//! - 错误追踪和上下文关联
//!
//! 性能目标：
//! - Span 创建开销: < 100ns
//! - 批量导出: 异步非阻塞
//! - 采样率: 可配置（生产环境建议 1-10%）

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, TracerProvider};
use opentelemetry_sdk::Resource;
use std::time::Duration;
use thiserror::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

// ═══════════════════════════════════════════════════════════════════════════
// 错误类型
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum TracingError {
    #[error("Tracer initialization failed: {0}")]
    InitError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// ═══════════════════════════════════════════════════════════════════════════
// 追踪配置
// ═══════════════════════════════════════════════════════════════════════════

/// 追踪配置
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// 是否启用追踪
    pub enabled: bool,
    /// 服务名称
    pub service_name: String,
    /// 服务版本
    pub service_version: String,
    /// 环境标识（dev/staging/prod）
    pub environment: String,
    /// 导出器类型
    pub exporter: ExporterType,
    /// OTLP 端点
    pub endpoint: String,
    /// 采样率 (0.0 - 1.0)
    pub sampling_rate: f64,
    /// 批量导出配置
    pub batch_config: BatchExportConfig,
    /// 日志级别过滤
    pub log_filter: String,
    /// 是否导出到控制台
    pub console_export: bool,
}

/// 导出器类型
#[derive(Debug, Clone, PartialEq)]
pub enum ExporterType {
    /// OTLP (gRPC/HTTP)
    Otlp,
    /// 仅控制台输出
    Console,
    /// 禁用导出
    None,
}

/// 批量导出配置
#[derive(Debug, Clone)]
pub struct BatchExportConfig {
    /// 最大队列大小
    pub max_queue_size: usize,
    /// 批量导出间隔
    pub scheduled_delay: Duration,
    /// 单批最大导出数量
    pub max_export_batch_size: usize,
    /// 导出超时
    pub max_export_timeout: Duration,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_name: "qaexchange".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: "development".to_string(),
            exporter: ExporterType::Console, // 默认使用控制台
            endpoint: "http://localhost:4317".to_string(),
            sampling_rate: 1.0, // 开发环境 100% 采样
            batch_config: BatchExportConfig::default(),
            log_filter: "info,qaexchange=debug".to_string(),
            console_export: true,
        }
    }
}

impl Default for BatchExportConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 65536,
            scheduled_delay: Duration::from_secs(5),
            max_export_batch_size: 512,
            max_export_timeout: Duration::from_secs(30),
        }
    }
}

impl TracingConfig {
    /// 生产环境配置
    pub fn production(endpoint: impl Into<String>) -> Self {
        Self {
            enabled: true,
            environment: "production".to_string(),
            exporter: ExporterType::Otlp,
            endpoint: endpoint.into(),
            sampling_rate: 0.1, // 生产环境 10% 采样
            log_filter: "warn,qaexchange=info".to_string(),
            console_export: false,
            ..Default::default()
        }
    }

    /// 开发环境配置
    pub fn development() -> Self {
        Self::default()
    }

    /// 测试环境配置（禁用导出）
    pub fn test() -> Self {
        Self {
            enabled: true,
            exporter: ExporterType::Console,
            sampling_rate: 1.0,
            console_export: true,
            ..Default::default()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 追踪初始化器
// ═══════════════════════════════════════════════════════════════════════════

/// 追踪系统初始化器
pub struct TracingInitializer {
    config: TracingConfig,
}

impl TracingInitializer {
    pub fn new(config: TracingConfig) -> Self {
        Self { config }
    }

    /// 初始化追踪系统
    pub fn init(&self) -> Result<(), TracingError> {
        if !self.config.enabled {
            // 仅初始化基本日志
            self.init_basic_logging()?;
            return Ok(());
        }

        match self.config.exporter {
            ExporterType::Otlp => self.init_otlp()?,
            ExporterType::Console => self.init_console()?,
            ExporterType::None => self.init_basic_logging()?,
        }

        Ok(())
    }

    /// 初始化 OTLP 导出器
    #[cfg(feature = "otlp")]
    fn init_otlp(&self) -> Result<(), TracingError> {
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::runtime::Tokio;

        let resource = self.create_resource();

        // 创建 OTLP 导出器
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(&self.config.endpoint)
            .with_timeout(self.config.batch_config.max_export_timeout);

        let sampler = self.create_sampler();

        // 创建 TracerProvider
        let tracer_provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(
                opentelemetry_sdk::trace::Config::default()
                    .with_sampler(sampler)
                    .with_id_generator(RandomIdGenerator::default())
                    .with_resource(resource),
            )
            .install_batch(Tokio)
            .map_err(|e| TracingError::InitError(e.to_string()))?;

        let tracer = tracer_provider.tracer(self.config.service_name.clone());
        global::set_tracer_provider(tracer_provider);

        self.setup_subscriber_with_telemetry(tracer)?;

        log::info!(
            "OpenTelemetry OTLP tracer initialized: endpoint={}",
            self.config.endpoint
        );

        Ok(())
    }

    /// 初始化 OTLP 导出器（无 otlp feature 时的 fallback）
    #[cfg(not(feature = "otlp"))]
    fn init_otlp(&self) -> Result<(), TracingError> {
        log::warn!("OTLP feature not enabled, falling back to console tracing");
        self.init_console()
    }

    /// 初始化控制台导出器
    fn init_console(&self) -> Result<(), TracingError> {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.log_filter));

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);

        Registry::default()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()
            .map_err(|e| TracingError::InitError(e.to_string()))?;

        log::info!("Console tracing initialized");

        Ok(())
    }

    /// 初始化基本日志（无追踪）
    fn init_basic_logging(&self) -> Result<(), TracingError> {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.log_filter));

        let fmt_layer = tracing_subscriber::fmt::layer().with_target(true);

        Registry::default()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()
            .map_err(|e| TracingError::InitError(e.to_string()))?;

        Ok(())
    }

    /// 创建资源标识
    fn create_resource(&self) -> Resource {
        Resource::new(vec![
            KeyValue::new("service.name", self.config.service_name.clone()),
            KeyValue::new("service.version", self.config.service_version.clone()),
            KeyValue::new("deployment.environment", self.config.environment.clone()),
            KeyValue::new("service.namespace", "qaexchange"),
        ])
    }

    /// 创建采样器
    fn create_sampler(&self) -> Sampler {
        if self.config.sampling_rate >= 1.0 {
            Sampler::AlwaysOn
        } else if self.config.sampling_rate <= 0.0 {
            Sampler::AlwaysOff
        } else {
            Sampler::TraceIdRatioBased(self.config.sampling_rate)
        }
    }

    /// 设置带 telemetry 的 subscriber
    #[cfg(feature = "otlp")]
    fn setup_subscriber_with_telemetry(
        &self,
        tracer: opentelemetry_sdk::trace::Tracer,
    ) -> Result<(), TracingError> {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.log_filter));

        let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        if self.config.console_export {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true);

            Registry::default()
                .with(env_filter)
                .with(telemetry_layer)
                .with(fmt_layer)
                .try_init()
                .map_err(|e| TracingError::InitError(e.to_string()))?;
        } else {
            Registry::default()
                .with(env_filter)
                .with(telemetry_layer)
                .try_init()
                .map_err(|e| TracingError::InitError(e.to_string()))?;
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 追踪宏和工具函数
// ═══════════════════════════════════════════════════════════════════════════

/// 关闭追踪系统（优雅退出）
pub fn shutdown_tracer() {
    global::shutdown_tracer_provider();
    log::info!("Tracer provider shut down");
}

/// 快速初始化（开发环境）
pub fn init_dev_tracing() -> Result<(), TracingError> {
    TracingInitializer::new(TracingConfig::development()).init()
}

/// 快速初始化（生产环境）
pub fn init_prod_tracing(endpoint: impl Into<String>) -> Result<(), TracingError> {
    TracingInitializer::new(TracingConfig::production(endpoint)).init()
}

/// 快速初始化（测试环境）
pub fn init_test_tracing() -> Result<(), TracingError> {
    TracingInitializer::new(TracingConfig::test()).init()
}

// ═══════════════════════════════════════════════════════════════════════════
// Span 工具宏
// ═══════════════════════════════════════════════════════════════════════════

/// 创建带自动计时的 span
#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {
        tracing::info_span!($name)
    };
    ($name:expr, $($field:tt)*) => {
        tracing::info_span!($name, $($field)*)
    };
}

/// 记录关键操作
#[macro_export]
macro_rules! trace_operation {
    ($name:expr, $op:expr) => {{
        let span = tracing::info_span!($name);
        let _guard = span.enter();
        let start = std::time::Instant::now();
        let result = $op;
        let elapsed = start.elapsed();
        tracing::info!(elapsed_us = elapsed.as_micros() as u64, "operation completed");
        result
    }};
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert!(config.enabled);
        assert_eq!(config.service_name, "qaexchange");
        assert_eq!(config.sampling_rate, 1.0);
    }

    #[test]
    fn test_tracing_config_production() {
        let config = TracingConfig::production("http://jaeger:4317");
        assert!(config.enabled);
        assert_eq!(config.environment, "production");
        assert_eq!(config.sampling_rate, 0.1);
        assert!(!config.console_export);
    }

    #[test]
    fn test_tracing_config_test() {
        let config = TracingConfig::test();
        assert!(config.enabled);
        assert_eq!(config.exporter, ExporterType::Console);
    }

    #[test]
    fn test_batch_config_default() {
        let config = BatchExportConfig::default();
        assert_eq!(config.max_queue_size, 65536);
        assert_eq!(config.max_export_batch_size, 512);
    }
}
