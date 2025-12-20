# 可观测性集成指南

> **版本**: v1.0.0
> **最后更新**: 2025-12-18
> **维护者**: @yutiansut @quantaxis

## 目录

- [概述](#概述)
- [OpenTelemetry 追踪](#opentelemetry-追踪)
- [Prometheus 指标](#prometheus-指标)
- [结构化日志](#结构化日志)
- [Grafana 大盘](#grafana-大盘)
- [告警配置](#告警配置)
- [最佳实践](#最佳实践)

---

## 概述

可观测性是生产系统的三大支柱：

```
┌─────────────────────────────────────────────────────────────┐
│                   Observability Pillars                      │
│                                                              │
│   ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐│
│   │    Tracing      │  │    Metrics      │  │   Logging   ││
│   │                 │  │                 │  │             ││
│   │ • Request flow  │  │ • Counters      │  │ • Events    ││
│   │ • Latency       │  │ • Gauges        │  │ • Errors    ││
│   │ • Dependencies  │  │ • Histograms    │  │ • Context   ││
│   │                 │  │                 │  │             ││
│   │ OpenTelemetry   │  │ Prometheus      │  │ tracing-    ││
│   │ + Jaeger        │  │ + Grafana       │  │ subscriber  ││
│   └─────────────────┘  └─────────────────┘  └─────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### 依赖组件

| 组件 | 版本 | 用途 |
|------|------|------|
| tracing | 0.1 | Rust 追踪框架 |
| tracing-subscriber | 0.3 | 追踪订阅器 |
| tracing-opentelemetry | 0.27 | OpenTelemetry 桥接 |
| opentelemetry | 0.26 | 可观测性标准 |
| opentelemetry-otlp | 0.26 | OTLP 导出器 |
| prometheus | 0.13 | 指标收集 |

---

## OpenTelemetry 追踪

### 初始化

#### 开发环境

```rust
use qaexchange::observability::tracing::{
    TracingInitializer, TracingConfig, init_dev_tracing
};

// 方式 1: 快速初始化
init_dev_tracing()?;

// 方式 2: 自定义配置
let config = TracingConfig::development();
TracingInitializer::new(config).init()?;
```

#### 生产环境

```rust
use qaexchange::observability::tracing::{
    TracingInitializer, TracingConfig, ExporterType, BatchExportConfig
};
use std::time::Duration;

let config = TracingConfig {
    enabled: true,
    service_name: "qaexchange".to_string(),
    service_version: env!("CARGO_PKG_VERSION").to_string(),
    environment: "production".to_string(),
    exporter: ExporterType::Otlp,
    endpoint: "http://jaeger:4317".to_string(),
    sampling_rate: 0.1,  // 10% 采样
    batch_config: BatchExportConfig {
        max_queue_size: 65536,
        scheduled_delay: Duration::from_secs(5),
        max_export_batch_size: 512,
        max_export_timeout: Duration::from_secs(30),
    },
    log_filter: "warn,qaexchange=info".to_string(),
    console_export: false,
};

TracingInitializer::new(config).init()?;
```

### Span 使用

#### 基础 Span

```rust
use tracing::{info_span, info, warn, error};

fn process_order(order: &Order) -> Result<Trade, Error> {
    let span = info_span!(
        "process_order",
        order_id = %order.id,
        instrument = %order.instrument_id,
        direction = ?order.direction,
    );
    let _guard = span.enter();

    info!("Processing order");

    match validate_order(order) {
        Ok(_) => {
            info!("Order validated");
            match_order(order)
        }
        Err(e) => {
            error!(error = %e, "Order validation failed");
            Err(e)
        }
    }
}
```

#### 异步 Span

```rust
use tracing::Instrument;

async fn submit_order(order: Order) -> Result<OrderId, Error> {
    let span = info_span!(
        "submit_order",
        order_id = %order.id,
    );

    async move {
        let validated = validate_order(&order).await?;
        let result = execute_order(validated).await?;
        Ok(result)
    }
    .instrument(span)
    .await
}
```

#### 使用宏简化

```rust
use qaexchange::{trace_span, trace_operation};

// 简单 span
let result = {
    let span = trace_span!("my_operation");
    let _guard = span.enter();
    do_something()
};

// 自动计时操作
let result = trace_operation!("expensive_operation", {
    compute_something_expensive()
});
// 自动记录: elapsed_us = xxx
```

### Span 属性规范

#### 交易系统属性

| 属性 | 类型 | 说明 |
|------|------|------|
| `order.id` | string | 订单 ID |
| `order.instrument` | string | 合约代码 |
| `order.direction` | string | BUY/SELL |
| `order.price` | float | 委托价格 |
| `order.volume` | int | 委托数量 |
| `trade.id` | string | 成交 ID |
| `trade.price` | float | 成交价格 |
| `trade.volume` | int | 成交数量 |
| `user.id` | string | 用户 ID |
| `account.id` | string | 账户 ID |

#### 存储系统属性

| 属性 | 类型 | 说明 |
|------|------|------|
| `wal.record_type` | string | 记录类型 |
| `wal.sequence` | int | 序列号 |
| `memtable.size_bytes` | int | MemTable 大小 |
| `sstable.file` | string | 文件路径 |
| `query.type` | string | 查询类型 |
| `query.rows` | int | 返回行数 |

### 上下文传播

```rust
use tracing_opentelemetry::OpenTelemetrySpanExt;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;

// 注入上下文到请求头
fn inject_context(span: &tracing::Span, headers: &mut HeaderMap) {
    let propagator = TraceContextPropagator::new();
    let context = span.context();

    propagator.inject_context(&context, &mut HeaderInjector(headers));
}

// 从请求头提取上下文
fn extract_context(headers: &HeaderMap) -> Context {
    let propagator = TraceContextPropagator::new();
    propagator.extract(&HeaderExtractor(headers))
}
```

---

## Prometheus 指标

### 指标定义

```rust
use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec,
    Histogram, HistogramVec, HistogramOpts,
    Opts, Registry,
};
use lazy_static::lazy_static;

lazy_static! {
    // 计数器
    pub static ref ORDERS_TOTAL: CounterVec = CounterVec::new(
        Opts::new("qaexchange_orders_total", "Total number of orders"),
        &["instrument", "direction", "status"]
    ).unwrap();

    pub static ref TRADES_TOTAL: CounterVec = CounterVec::new(
        Opts::new("qaexchange_trades_total", "Total number of trades"),
        &["instrument"]
    ).unwrap();

    // 直方图 (延迟分布)
    pub static ref ORDER_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "qaexchange_order_latency_seconds",
            "Order processing latency"
        ).buckets(vec![
            0.00001, 0.00005, 0.0001, 0.0005, 0.001,
            0.005, 0.01, 0.05, 0.1, 0.5, 1.0
        ]),
        &["operation"]
    ).unwrap();

    // 仪表盘 (当前值)
    pub static ref ACTIVE_CONNECTIONS: Gauge = Gauge::new(
        "qaexchange_active_connections",
        "Number of active WebSocket connections"
    ).unwrap();

    pub static ref MEMTABLE_SIZE_BYTES: GaugeVec = GaugeVec::new(
        Opts::new("qaexchange_memtable_size_bytes", "MemTable size in bytes"),
        &["type"]
    ).unwrap();

    // 注册表
    pub static ref REGISTRY: Registry = {
        let registry = Registry::new();
        registry.register(Box::new(ORDERS_TOTAL.clone())).unwrap();
        registry.register(Box::new(TRADES_TOTAL.clone())).unwrap();
        registry.register(Box::new(ORDER_LATENCY.clone())).unwrap();
        registry.register(Box::new(ACTIVE_CONNECTIONS.clone())).unwrap();
        registry.register(Box::new(MEMTABLE_SIZE_BYTES.clone())).unwrap();
        registry
    };
}
```

### 指标采集

```rust
use std::time::Instant;

// 计数器
pub fn record_order(instrument: &str, direction: &str, status: &str) {
    ORDERS_TOTAL
        .with_label_values(&[instrument, direction, status])
        .inc();
}

// 直方图
pub fn record_order_latency(operation: &str, start: Instant) {
    let duration = start.elapsed().as_secs_f64();
    ORDER_LATENCY
        .with_label_values(&[operation])
        .observe(duration);
}

// 仪表盘
pub fn set_active_connections(count: i64) {
    ACTIVE_CONNECTIONS.set(count as f64);
}

// 使用示例
async fn process_order(order: Order) -> Result<Trade, Error> {
    let start = Instant::now();

    let result = do_process_order(order).await;

    record_order_latency("process", start);

    match &result {
        Ok(trade) => {
            record_order(&order.instrument_id, "BUY", "filled");
            TRADES_TOTAL.with_label_values(&[&order.instrument_id]).inc();
        }
        Err(_) => {
            record_order(&order.instrument_id, "BUY", "rejected");
        }
    }

    result
}
```

### HTTP 端点

```rust
use actix_web::{web, HttpResponse};
use prometheus::Encoder;

async fn metrics() -> HttpResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(buffer)
}

// 注册路由
fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/metrics", web::get().to(metrics));
}
```

### 核心指标清单

#### 交易指标

| 指标名 | 类型 | 标签 | 说明 |
|--------|------|------|------|
| `qaexchange_orders_total` | Counter | instrument, direction, status | 订单总数 |
| `qaexchange_trades_total` | Counter | instrument | 成交总数 |
| `qaexchange_order_latency_seconds` | Histogram | operation | 订单延迟 |
| `qaexchange_order_queue_size` | Gauge | - | 订单队列长度 |
| `qaexchange_order_book_depth` | Gauge | instrument, side | 订单簿深度 |

#### 存储指标

| 指标名 | 类型 | 标签 | 说明 |
|--------|------|------|------|
| `qaexchange_wal_writes_total` | Counter | - | WAL 写入次数 |
| `qaexchange_wal_bytes_written` | Counter | - | WAL 写入字节 |
| `qaexchange_memtable_size_bytes` | Gauge | type | MemTable 大小 |
| `qaexchange_sstable_files` | Gauge | level | SSTable 文件数 |
| `qaexchange_compaction_duration_seconds` | Histogram | - | Compaction 耗时 |

#### 系统指标

| 指标名 | 类型 | 标签 | 说明 |
|--------|------|------|------|
| `qaexchange_active_connections` | Gauge | - | 活跃连接数 |
| `qaexchange_memory_usage_bytes` | Gauge | - | 内存使用 |
| `qaexchange_cpu_usage_percent` | Gauge | - | CPU 使用率 |
| `qaexchange_disk_usage_bytes` | Gauge | mount | 磁盘使用 |

---

## 结构化日志

### 配置

```rust
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    fmt::format::FmtSpan,
};

fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,qaexchange=debug"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()                           // JSON 格式
        .with_target(true)                // 包含模块路径
        .with_thread_ids(true)            // 包含线程 ID
        .with_file(true)                  // 包含文件名
        .with_line_number(true)           // 包含行号
        .with_span_events(FmtSpan::CLOSE) // 记录 span 关闭
        .with_current_span(true);         // 包含当前 span

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}
```

### 日志输出示例

```json
{
  "timestamp": "2025-12-18T10:30:45.123456Z",
  "level": "INFO",
  "target": "qaexchange::matching::engine",
  "filename": "src/matching/engine.rs",
  "line_number": 156,
  "thread_id": "ThreadId(3)",
  "span": {
    "name": "process_order",
    "order_id": "ORD-001",
    "instrument": "IF2501"
  },
  "message": "Order matched",
  "fields": {
    "trade_id": "TRD-001",
    "price": 4500.0,
    "volume": 10
  }
}
```

### 日志级别指南

| 级别 | 用途 | 示例 |
|------|------|------|
| ERROR | 需要立即关注的错误 | 数据库连接失败、关键业务异常 |
| WARN | 潜在问题、异常情况 | 重试操作、性能降级 |
| INFO | 重要业务事件 | 订单成交、用户登录、系统启动 |
| DEBUG | 调试信息 | 内部状态、计算过程 |
| TRACE | 详细跟踪 | 函数进出、变量值 |

---

## Grafana 大盘

### 交易概览面板

```json
{
  "title": "QAExchange Trading Overview",
  "panels": [
    {
      "title": "Order Rate",
      "type": "timeseries",
      "targets": [
        {
          "expr": "sum(rate(qaexchange_orders_total[1m])) by (status)",
          "legendFormat": "{{status}}"
        }
      ]
    },
    {
      "title": "Trade Rate",
      "type": "timeseries",
      "targets": [
        {
          "expr": "sum(rate(qaexchange_trades_total[1m]))",
          "legendFormat": "trades/min"
        }
      ]
    },
    {
      "title": "Order Latency P99",
      "type": "gauge",
      "targets": [
        {
          "expr": "histogram_quantile(0.99, rate(qaexchange_order_latency_seconds_bucket[5m]))"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "thresholds": {
            "steps": [
              {"value": 0, "color": "green"},
              {"value": 0.001, "color": "yellow"},
              {"value": 0.01, "color": "red"}
            ]
          }
        }
      }
    }
  ]
}
```

### 存储状态面板

```json
{
  "title": "Storage Status",
  "panels": [
    {
      "title": "WAL Write Rate",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(qaexchange_wal_writes_total[1m])",
          "legendFormat": "writes/s"
        }
      ]
    },
    {
      "title": "MemTable Size",
      "type": "timeseries",
      "targets": [
        {
          "expr": "qaexchange_memtable_size_bytes",
          "legendFormat": "{{type}}"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes"
        }
      }
    },
    {
      "title": "SSTable Files by Level",
      "type": "bargauge",
      "targets": [
        {
          "expr": "qaexchange_sstable_files",
          "legendFormat": "Level {{level}}"
        }
      ]
    }
  ]
}
```

### 系统健康面板

```json
{
  "title": "System Health",
  "panels": [
    {
      "title": "Active Connections",
      "type": "stat",
      "targets": [
        {
          "expr": "qaexchange_active_connections"
        }
      ]
    },
    {
      "title": "Memory Usage",
      "type": "gauge",
      "targets": [
        {
          "expr": "qaexchange_memory_usage_bytes / 1024 / 1024 / 1024"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "GB",
          "max": 64
        }
      }
    },
    {
      "title": "CPU Usage",
      "type": "timeseries",
      "targets": [
        {
          "expr": "qaexchange_cpu_usage_percent",
          "legendFormat": "CPU %"
        }
      ]
    }
  ]
}
```

---

## 告警配置

### Prometheus 告警规则

```yaml
# /etc/prometheus/rules/qaexchange.yml
groups:
  - name: qaexchange_trading
    rules:
      - alert: HighOrderLatency
        expr: histogram_quantile(0.99, rate(qaexchange_order_latency_seconds_bucket[5m])) > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Order latency P99 > 10ms"
          description: "Order processing latency is {{ $value | humanizeDuration }}"

      - alert: OrderRejectionRateHigh
        expr: |
          sum(rate(qaexchange_orders_total{status="rejected"}[5m]))
          / sum(rate(qaexchange_orders_total[5m])) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Order rejection rate > 5%"

  - name: qaexchange_storage
    rules:
      - alert: WALWriteFailure
        expr: increase(qaexchange_wal_write_errors_total[5m]) > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "WAL write failures detected"

      - alert: MemTableSizeHigh
        expr: qaexchange_memtable_size_bytes > 1073741824  # 1GB
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "MemTable size > 1GB, consider flushing"

  - name: qaexchange_system
    rules:
      - alert: HighMemoryUsage
        expr: qaexchange_memory_usage_bytes / (1024*1024*1024) > 50
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Memory usage > 50GB"

      - alert: DiskSpaceLow
        expr: (qaexchange_disk_total_bytes - qaexchange_disk_usage_bytes) / qaexchange_disk_total_bytes < 0.1
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Disk space < 10%"
```

### Alertmanager 配置

```yaml
# /etc/alertmanager/alertmanager.yml
global:
  resolve_timeout: 5m

route:
  receiver: 'default'
  group_by: ['alertname', 'severity']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
    - match:
        severity: warning
      receiver: 'slack'

receivers:
  - name: 'default'
    email_configs:
      - to: 'ops@quantaxis.io'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: '<PAGERDUTY_KEY>'

  - name: 'slack'
    slack_configs:
      - api_url: '<SLACK_WEBHOOK>'
        channel: '#alerts'
```

---

## 最佳实践

### 1. 采样策略

```rust
// 生产环境：按比例采样
let config = TracingConfig {
    sampling_rate: 0.1,  // 10% 采样
    ..Default::default()
};

// 关键路径：100% 采样
// 在代码中显式标记
#[tracing::instrument(fields(sampling = "always"))]
async fn critical_operation() {
    // ...
}
```

### 2. 指标命名规范

```
# 格式: <namespace>_<subsystem>_<name>_<unit>

# Good
qaexchange_matching_orders_total
qaexchange_storage_wal_bytes_written
qaexchange_http_request_duration_seconds

# Bad
order_count              # 缺少 namespace
qaexchange_latency       # 缺少 unit
total_orders             # 缺少 subsystem
```

### 3. 标签使用

```rust
// Good: 有限的标签值
ORDERS_TOTAL.with_label_values(&["IF2501", "BUY", "filled"]).inc();

// Bad: 高基数标签
ORDERS_TOTAL.with_label_values(&[&order_id]).inc();  // 每个订单一个标签！
```

### 4. 上下文关联

```rust
// 在日志中包含 trace_id
info!(
    trace_id = %current_trace_id(),
    order_id = %order.id,
    "Order processed"
);
```

### 5. 性能影响最小化

```rust
// 延迟采集耗时指标
let timer = ORDER_LATENCY.with_label_values(&["match"]).start_timer();
// ... 执行操作
timer.observe_duration();  // 自动计算耗时

// 避免在热路径中分配
// 预分配标签值
static LABELS: &[&str] = &["match", "validate", "persist"];
```

---

## 附录

### A. 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `RUST_LOG` | 日志级别 | `info,qaexchange=debug` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP 端点 | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | 服务名 | `qaexchange` |
| `OTEL_TRACES_SAMPLER_ARG` | 采样率 | `1.0` |

### B. Docker Compose 示例

```yaml
version: '3.8'
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP gRPC

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    volumes:
      - ./dashboards:/var/lib/grafana/dashboards

  qaexchange:
    build: .
    environment:
      - RUST_LOG=info,qaexchange=debug
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
```

### C. 参考资料

- [OpenTelemetry Rust](https://opentelemetry.io/docs/instrumentation/rust/)
- [Prometheus Rust Client](https://github.com/prometheus/client_rust)
- [tracing crate](https://docs.rs/tracing/)
- [Grafana Dashboard JSON](https://grafana.com/docs/grafana/latest/dashboards/json-model/)

---

**文档版本**: v1.0.0
**最后更新**: 2025-12-18
**维护者**: @yutiansut @quantaxis
