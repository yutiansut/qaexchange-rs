//! 性能监控指标聚合器
//!
//! 统一收集和暴露系统各组件的性能指标，支持：
//! - 实时查询
//! - JSON 导出
//! - Prometheus 格式输出
//!
//! @author @yutiansut @quantaxis

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 系统指标聚合
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// 撮合引擎指标
    pub matching: MatchingMetrics,
    /// 订单路由指标
    pub order_router: OrderRouterMetrics,
    /// 市场数据指标
    pub market_data: MarketDataMetrics,
    /// WAL 指标
    pub wal: WalMetrics,
    /// 结算指标
    pub settlement: SettlementMetrics,
    /// 因子计算指标
    pub factor: FactorMetrics,
    /// 系统资源指标
    pub system: SystemResourceMetrics,
}

/// 撮合引擎指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MatchingMetrics {
    /// 订单总数
    pub total_orders: u64,
    /// 成交总数
    pub total_trades: u64,
    /// 撮合延迟 P50 (微秒)
    pub latency_p50_us: u64,
    /// 撮合延迟 P99 (微秒)
    pub latency_p99_us: u64,
    /// 订单簿数量
    pub orderbook_count: u64,
}

/// 订单路由指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OrderRouterMetrics {
    /// 接收订单数
    pub orders_received: u64,
    /// 风控拒绝数
    pub orders_rejected_risk: u64,
    /// 资金不足拒绝数
    pub orders_rejected_funds: u64,
    /// 成功提交数
    pub orders_submitted: u64,
    /// 平均处理延迟 (微秒)
    pub avg_latency_us: u64,
}

/// 市场数据指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MarketDataMetrics {
    /// 订阅者数量
    pub subscriber_count: u64,
    /// 广播总数
    pub total_broadcasts: u64,
    /// 发送成功数
    pub total_sent: u64,
    /// 丢弃数量
    pub total_dropped: u64,
    /// 丢弃率
    pub drop_rate: f64,
    /// 平均广播延迟 (微秒)
    pub avg_broadcast_latency_us: f64,
    /// 慢订阅者断开数
    pub slow_disconnects: u64,
}

/// WAL 指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WalMetrics {
    /// 写入计数
    pub write_count: u64,
    /// 写入字节数
    pub write_bytes: u64,
    /// fsync 次数
    pub sync_count: u64,
    /// 组提交次数
    pub group_commit_count: u64,
    /// 平均组提交大小
    pub avg_group_commit_size: f64,
    /// 平均写入延迟 (微秒)
    pub avg_write_latency_us: f64,
}

/// 结算指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SettlementMetrics {
    /// 已结算账户数
    pub settled_accounts: u64,
    /// 强平账户数
    pub force_closed_accounts: u64,
    /// 待强平队列长度
    pub pending_force_close: u64,
    /// 结算总耗时 (微秒)
    pub total_time_us: u64,
}

/// 因子计算指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FactorMetrics {
    /// 计算次数
    pub compute_count: u64,
    /// 计算总耗时 (微秒)
    pub total_time_us: u64,
    /// 因子数量
    pub factor_count: u64,
    /// DAG 层数
    pub dag_levels: u64,
}

/// 系统资源指标
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SystemResourceMetrics {
    /// 启动时间 (Unix 时间戳)
    pub start_time: u64,
    /// 运行时间 (秒)
    pub uptime_seconds: u64,
    /// 线程数
    pub thread_count: u64,
}

/// 指标收集器 (原子计数器版本)
#[derive(Debug)]
pub struct MetricsCollector {
    // 撮合引擎
    pub matching_total_orders: AtomicU64,
    pub matching_total_trades: AtomicU64,
    pub matching_latency_sum_us: AtomicU64,
    pub matching_latency_count: AtomicU64,

    // 订单路由
    pub router_orders_received: AtomicU64,
    pub router_orders_rejected_risk: AtomicU64,
    pub router_orders_rejected_funds: AtomicU64,
    pub router_orders_submitted: AtomicU64,
    pub router_latency_sum_us: AtomicU64,

    // 系统
    start_time: Instant,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            matching_total_orders: AtomicU64::new(0),
            matching_total_trades: AtomicU64::new(0),
            matching_latency_sum_us: AtomicU64::new(0),
            matching_latency_count: AtomicU64::new(0),
            router_orders_received: AtomicU64::new(0),
            router_orders_rejected_risk: AtomicU64::new(0),
            router_orders_rejected_funds: AtomicU64::new(0),
            router_orders_submitted: AtomicU64::new(0),
            router_latency_sum_us: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// 记录撮合订单
    pub fn record_matching_order(&self, latency_us: u64) {
        self.matching_total_orders.fetch_add(1, Ordering::Relaxed);
        self.matching_latency_sum_us.fetch_add(latency_us, Ordering::Relaxed);
        self.matching_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录成交
    pub fn record_trade(&self) {
        self.matching_total_trades.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录订单接收
    pub fn record_order_received(&self) {
        self.router_orders_received.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录风控拒绝
    pub fn record_order_rejected_risk(&self) {
        self.router_orders_rejected_risk.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录资金拒绝
    pub fn record_order_rejected_funds(&self) {
        self.router_orders_rejected_funds.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录订单提交
    pub fn record_order_submitted(&self, latency_us: u64) {
        self.router_orders_submitted.fetch_add(1, Ordering::Relaxed);
        self.router_latency_sum_us.fetch_add(latency_us, Ordering::Relaxed);
    }

    /// 获取运行时间
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// 构建撮合指标
    pub fn get_matching_metrics(&self) -> MatchingMetrics {
        let count = self.matching_latency_count.load(Ordering::Relaxed);
        let avg_latency = if count > 0 {
            self.matching_latency_sum_us.load(Ordering::Relaxed) / count
        } else {
            0
        };

        MatchingMetrics {
            total_orders: self.matching_total_orders.load(Ordering::Relaxed),
            total_trades: self.matching_total_trades.load(Ordering::Relaxed),
            latency_p50_us: avg_latency,
            latency_p99_us: avg_latency, // 简化：使用平均值代替
            orderbook_count: 0,          // 需要从 MatchingEngine 获取
        }
    }

    /// 构建订单路由指标
    pub fn get_order_router_metrics(&self) -> OrderRouterMetrics {
        let submitted = self.router_orders_submitted.load(Ordering::Relaxed);
        let avg_latency = if submitted > 0 {
            self.router_latency_sum_us.load(Ordering::Relaxed) / submitted
        } else {
            0
        };

        OrderRouterMetrics {
            orders_received: self.router_orders_received.load(Ordering::Relaxed),
            orders_rejected_risk: self.router_orders_rejected_risk.load(Ordering::Relaxed),
            orders_rejected_funds: self.router_orders_rejected_funds.load(Ordering::Relaxed),
            orders_submitted: submitted,
            avg_latency_us: avg_latency,
        }
    }

    /// 构建系统资源指标
    pub fn get_system_metrics(&self) -> SystemResourceMetrics {
        SystemResourceMetrics {
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - self.start_time.elapsed().as_secs(),
            uptime_seconds: self.uptime_seconds(),
            thread_count: std::thread::available_parallelism()
                .map(|p| p.get() as u64)
                .unwrap_or(1),
        }
    }
}

/// 全局指标收集器
static GLOBAL_COLLECTOR: std::sync::OnceLock<Arc<MetricsCollector>> = std::sync::OnceLock::new();

/// 获取全局指标收集器
pub fn get_collector() -> Arc<MetricsCollector> {
    GLOBAL_COLLECTOR
        .get_or_init(|| Arc::new(MetricsCollector::new()))
        .clone()
}

/// 导出 Prometheus 格式指标
pub fn export_prometheus(metrics: &SystemMetrics) -> String {
    let mut output = String::new();

    // Matching metrics
    output.push_str(&format!(
        "# HELP qaexchange_matching_orders_total Total orders processed\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_matching_orders_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_matching_orders_total {}\n",
        metrics.matching.total_orders
    ));

    output.push_str(&format!(
        "# HELP qaexchange_matching_trades_total Total trades executed\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_matching_trades_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_matching_trades_total {}\n",
        metrics.matching.total_trades
    ));

    output.push_str(&format!(
        "# HELP qaexchange_matching_latency_us Matching latency in microseconds\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_matching_latency_us gauge\n"
    ));
    output.push_str(&format!(
        "qaexchange_matching_latency_us{{quantile=\"0.50\"}} {}\n",
        metrics.matching.latency_p50_us
    ));
    output.push_str(&format!(
        "qaexchange_matching_latency_us{{quantile=\"0.99\"}} {}\n",
        metrics.matching.latency_p99_us
    ));

    // Order router metrics
    output.push_str(&format!(
        "# HELP qaexchange_router_orders_total Order router statistics\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_router_orders_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_router_orders_total{{status=\"received\"}} {}\n",
        metrics.order_router.orders_received
    ));
    output.push_str(&format!(
        "qaexchange_router_orders_total{{status=\"submitted\"}} {}\n",
        metrics.order_router.orders_submitted
    ));
    output.push_str(&format!(
        "qaexchange_router_orders_total{{status=\"rejected_risk\"}} {}\n",
        metrics.order_router.orders_rejected_risk
    ));
    output.push_str(&format!(
        "qaexchange_router_orders_total{{status=\"rejected_funds\"}} {}\n",
        metrics.order_router.orders_rejected_funds
    ));

    // Market data metrics
    output.push_str(&format!(
        "# HELP qaexchange_market_subscribers Active market data subscribers\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_market_subscribers gauge\n"
    ));
    output.push_str(&format!(
        "qaexchange_market_subscribers {}\n",
        metrics.market_data.subscriber_count
    ));

    output.push_str(&format!(
        "# HELP qaexchange_market_broadcasts_total Market data broadcasts\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_market_broadcasts_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_market_broadcasts_total{{status=\"sent\"}} {}\n",
        metrics.market_data.total_sent
    ));
    output.push_str(&format!(
        "qaexchange_market_broadcasts_total{{status=\"dropped\"}} {}\n",
        metrics.market_data.total_dropped
    ));

    output.push_str(&format!(
        "# HELP qaexchange_market_drop_rate Market data drop rate\n"
    ));
    output.push_str(&format!("# TYPE qaexchange_market_drop_rate gauge\n"));
    output.push_str(&format!(
        "qaexchange_market_drop_rate {}\n",
        metrics.market_data.drop_rate
    ));

    // WAL metrics
    output.push_str(&format!(
        "# HELP qaexchange_wal_writes_total WAL write operations\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_wal_writes_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_wal_writes_total {}\n",
        metrics.wal.write_count
    ));

    output.push_str(&format!(
        "# HELP qaexchange_wal_bytes_total WAL bytes written\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_wal_bytes_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_wal_bytes_total {}\n",
        metrics.wal.write_bytes
    ));

    output.push_str(&format!(
        "# HELP qaexchange_wal_latency_us WAL write latency in microseconds\n"
    ));
    output.push_str(&format!("# TYPE qaexchange_wal_latency_us gauge\n"));
    output.push_str(&format!(
        "qaexchange_wal_latency_us {}\n",
        metrics.wal.avg_write_latency_us
    ));

    // Settlement metrics
    output.push_str(&format!(
        "# HELP qaexchange_settlement_accounts_total Settlement statistics\n"
    ));
    output.push_str(&format!(
        "# TYPE qaexchange_settlement_accounts_total counter\n"
    ));
    output.push_str(&format!(
        "qaexchange_settlement_accounts_total{{status=\"settled\"}} {}\n",
        metrics.settlement.settled_accounts
    ));
    output.push_str(&format!(
        "qaexchange_settlement_accounts_total{{status=\"force_closed\"}} {}\n",
        metrics.settlement.force_closed_accounts
    ));

    // System metrics
    output.push_str(&format!(
        "# HELP qaexchange_uptime_seconds System uptime in seconds\n"
    ));
    output.push_str(&format!("# TYPE qaexchange_uptime_seconds gauge\n"));
    output.push_str(&format!(
        "qaexchange_uptime_seconds {}\n",
        metrics.system.uptime_seconds
    ));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        // 记录一些指标
        collector.record_order_received();
        collector.record_order_received();
        collector.record_order_submitted(100);
        collector.record_order_rejected_risk();

        // 验证
        let router = collector.get_order_router_metrics();
        assert_eq!(router.orders_received, 2);
        assert_eq!(router.orders_submitted, 1);
        assert_eq!(router.orders_rejected_risk, 1);
        assert_eq!(router.avg_latency_us, 100);
    }

    #[test]
    fn test_global_collector() {
        let c1 = get_collector();
        let c2 = get_collector();
        assert!(Arc::ptr_eq(&c1, &c2));
    }

    #[test]
    fn test_prometheus_export() {
        let metrics = SystemMetrics::default();
        let output = export_prometheus(&metrics);
        assert!(output.contains("qaexchange_matching_orders_total"));
        assert!(output.contains("qaexchange_uptime_seconds"));
    }
}
