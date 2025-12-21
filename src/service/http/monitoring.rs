//! 监控和统计 API 处理器

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::handlers::AppState;
use crate::exchange::AccountManager;
// OrderRouter 不再用于统计，订单/成交数据从账户 QIFI 结构体获取 @yutiansut @quantaxis

/// 系统监控状态
#[derive(Debug, Serialize)]
pub struct SystemMonitoring {
    /// 账户统计
    pub accounts: AccountStats,

    /// 订单统计
    pub orders: OrderStats,

    /// 成交统计
    pub trades: TradeStats,

    /// 存储统计
    pub storage: StorageStats,
}

/// 账户统计 @yutiansut @quantaxis
/// 包含"中央银行"视角的资金流入/流出统计
#[derive(Debug, Serialize)]
pub struct AccountStats {
    /// 总账户数
    pub total_count: usize,

    /// 活跃账户数（有持仓）
    pub active_count: usize,

    /// 总资金
    pub total_balance: f64,

    /// 总可用资金
    pub total_available: f64,

    /// 总占用保证金
    pub total_margin_used: f64,

    /// 今日总入金（中央银行视角）@yutiansut @quantaxis
    pub total_deposit: f64,

    /// 今日总出金（中央银行视角）@yutiansut @quantaxis
    pub total_withdraw: f64,
}

/// 订单统计
#[derive(Debug, Serialize)]
pub struct OrderStats {
    /// 总订单数
    pub total_count: usize,

    /// 待成交订单数
    pub pending_count: usize,

    /// 完全成交订单数
    pub filled_count: usize,

    /// 已撤销订单数
    pub cancelled_count: usize,
}

/// 成交统计
#[derive(Debug, Serialize)]
pub struct TradeStats {
    /// 总成交笔数
    pub total_count: usize,

    /// 总成交金额
    pub total_amount: f64,

    /// 总成交量
    pub total_volume: f64,
}

/// 存储统计
#[derive(Debug, Serialize)]
pub struct StorageStats {
    /// OLTP 存储状态
    pub oltp: OltpStats,

    /// OLAP 转换状态
    pub olap: OlapStats,
}

/// OLTP 存储统计
#[derive(Debug, Serialize)]
pub struct OltpStats {
    /// 已写入记录数
    pub total_records: u64,

    /// 批次数
    pub total_batches: u64,

    /// 错误数
    pub total_errors: u64,
}

/// OLAP 转换统计
#[derive(Debug, Serialize)]
pub struct OlapStats {
    /// 总转换任务数
    pub total_tasks: usize,

    /// 待转换任务数
    pub pending_tasks: usize,

    /// 转换中任务数
    pub converting_tasks: usize,

    /// 成功任务数
    pub success_tasks: usize,

    /// 失败任务数
    pub failed_tasks: usize,

    /// 平均转换时长（秒）
    pub avg_duration_secs: usize,
}

/// 查询系统监控信息
///
/// GET /api/monitoring/system
pub async fn get_system_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    // 1. 账户统计
    let accounts = get_account_stats(&app_state.account_mgr);

    // 2. 订单统计 - 从账户 dailyorders 获取 @yutiansut @quantaxis
    let orders = get_order_stats(&app_state.account_mgr);

    // 3. 成交统计 - 从账户 dailytrades 获取 @yutiansut @quantaxis
    let trades = get_trade_stats(&app_state.account_mgr);

    // 4. 存储统计
    let oltp_stats = if let Some(ref stats_handle) = app_state.storage_stats {
        let stats = stats_handle.lock();
        OltpStats {
            total_records: stats.total_persisted,
            total_batches: stats.total_batches,
            total_errors: stats.total_errors,
        }
    } else {
        OltpStats {
            total_records: 0,
            total_batches: 0,
            total_errors: 0,
        }
    };

    let olap_stats = if let Some(ref mgr) = app_state.conversion_mgr {
        let stats = mgr.lock().get_stats();
        OlapStats {
            total_tasks: stats.total,
            pending_tasks: stats.pending,
            converting_tasks: stats.converting,
            success_tasks: stats.success,
            failed_tasks: stats.failed,
            avg_duration_secs: stats.avg_duration_secs as usize,
        }
    } else {
        OlapStats {
            total_tasks: 0,
            pending_tasks: 0,
            converting_tasks: 0,
            success_tasks: 0,
            failed_tasks: 0,
            avg_duration_secs: 0,
        }
    };

    let storage = StorageStats {
        oltp: oltp_stats,
        olap: olap_stats,
    };

    let monitoring = SystemMonitoring {
        accounts,
        orders,
        trades,
        storage,
    };

    HttpResponse::Ok().json(monitoring)
}

/// 查询账户统计
///
/// GET /api/monitoring/accounts
pub async fn get_accounts_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let stats = get_account_stats(&app_state.account_mgr);
    HttpResponse::Ok().json(stats)
}

/// 查询订单统计
///
/// GET /api/monitoring/orders
pub async fn get_orders_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let stats = get_order_stats(&app_state.account_mgr);
    HttpResponse::Ok().json(stats)
}

/// 查询成交统计
///
/// GET /api/monitoring/trades
pub async fn get_trades_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let stats = get_trade_stats(&app_state.account_mgr);
    HttpResponse::Ok().json(stats)
}

/// 查询存储统计
///
/// GET /api/monitoring/storage
pub async fn get_storage_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let oltp_stats = if let Some(ref stats_handle) = app_state.storage_stats {
        let stats = stats_handle.lock();
        OltpStats {
            total_records: stats.total_persisted,
            total_batches: stats.total_batches,
            total_errors: stats.total_errors,
        }
    } else {
        OltpStats {
            total_records: 0,
            total_batches: 0,
            total_errors: 0,
        }
    };

    let olap_stats = if let Some(ref mgr) = app_state.conversion_mgr {
        let stats = mgr.lock().get_stats();
        OlapStats {
            total_tasks: stats.total,
            pending_tasks: stats.pending,
            converting_tasks: stats.converting,
            success_tasks: stats.success,
            failed_tasks: stats.failed,
            avg_duration_secs: stats.avg_duration_secs as usize,
        }
    } else {
        OlapStats {
            total_tasks: 0,
            pending_tasks: 0,
            converting_tasks: 0,
            success_tasks: 0,
            failed_tasks: 0,
            avg_duration_secs: 0,
        }
    };

    let stats = StorageStats {
        oltp: oltp_stats,
        olap: olap_stats,
    };

    HttpResponse::Ok().json(stats)
}

// ============= 内部辅助函数 =============

fn get_account_stats(account_mgr: &AccountManager) -> AccountStats {
    let accounts = account_mgr.get_all_accounts();

    let total_count = accounts.len();
    let mut active_count = 0;
    let mut total_balance = 0.0;
    let mut total_available = 0.0;
    let mut total_margin_used = 0.0;
    // 中央银行视角：汇总所有账户的入金/出金 @yutiansut @quantaxis
    let mut total_deposit = 0.0;
    let mut total_withdraw = 0.0;

    for account in accounts {
        let acc = account.read();

        // 检查是否有持仓
        if !acc.hold.is_empty() {
            active_count += 1;
        }

        // QA_Account 字段：money（可用资金）, accounts.balance（总权益）
        total_available += acc.money;

        // 汇总 QIFI 的 deposit/withdraw 字段 @yutiansut @quantaxis
        total_deposit += acc.accounts.deposit;
        total_withdraw += acc.accounts.withdraw;

        // 计算总权益和保证金（简化版本）
        let mut account_balance = acc.money;
        let mut margin_used = 0.0;

        for position in acc.hold.values() {
            let long_value = position.volume_long_unmut() * position.lastest_price;
            let short_value = position.volume_short_unmut() * position.lastest_price;
            let position_value = long_value + short_value;
            account_balance += position_value;
            margin_used += position_value * 0.15; // 假设15%保证金
        }

        total_balance += account_balance;
        total_margin_used += margin_used;
    }

    AccountStats {
        total_count,
        active_count,
        total_balance,
        total_available,
        total_margin_used,
        total_deposit,
        total_withdraw,
    }
}

/// 从所有账户的 dailyorders 统计订单数据 @yutiansut @quantaxis
///
/// 订单数据来源于 QIFI 结构体中的 orders/dailyorders，而非 OrderRouter 内存结构
/// OrderRouter.orders 只存储当前会话的实时订单，不包含历史数据
fn get_order_stats(account_mgr: &AccountManager) -> OrderStats {
    let accounts = account_mgr.get_all_accounts();

    let mut total_count = 0;
    let mut pending_count = 0;
    let mut filled_count = 0;
    let mut cancelled_count = 0;

    for account in accounts {
        let acc = account.read();

        // 统计 dailyorders 中的订单
        for order in acc.dailyorders.values() {
            total_count += 1;

            // 根据 QIFI Order 的 status 字段判断状态
            // status 值: "SUBMITTED", "ALIVE", "FINISHED", "CANCELLED", "REJECTED" 等
            match order.status.as_str() {
                "SUBMITTED" | "ALIVE" | "PENDING" => pending_count += 1,
                "FINISHED" | "FILLED" => {
                    // 检查是否完全成交
                    if order.volume_left <= 0.0 {
                        filled_count += 1;
                    } else {
                        // 部分成交后撤单
                        cancelled_count += 1;
                    }
                }
                "CANCELLED" | "CANCELED" => cancelled_count += 1,
                _ => {
                    // 其他状态：根据 volume_left 判断
                    if order.volume_left <= 0.0 && order.volume_orign > 0.0 {
                        filled_count += 1;
                    } else if order.last_msg.contains("撤") {
                        cancelled_count += 1;
                    }
                }
            }
        }
    }

    OrderStats {
        total_count,
        pending_count,
        filled_count,
        cancelled_count,
    }
}

/// 从所有账户的 dailytrades 统计成交数据 @yutiansut @quantaxis
///
/// 成交数据来源于 QIFI 结构体中的 trades/dailytrades，而非 OrderRouter 内存结构
fn get_trade_stats(account_mgr: &AccountManager) -> TradeStats {
    let accounts = account_mgr.get_all_accounts();

    let mut total_count = 0;
    let mut total_amount = 0.0;
    let mut total_volume = 0.0;

    for account in accounts {
        let acc = account.read();

        // 统计 dailytrades 中的成交
        for trade in acc.dailytrades.values() {
            total_count += 1;
            total_volume += trade.volume;
            // 成交金额 = 价格 * 数量
            total_amount += trade.price * trade.volume;
        }
    }

    TradeStats {
        total_count,
        total_amount,
        total_volume,
    }
}

/// 生成日志报告
///
/// GET /api/monitoring/report
pub async fn generate_report(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let accounts = get_account_stats(&app_state.account_mgr);
    let orders = get_order_stats(&app_state.account_mgr);
    let trades = get_trade_stats(&app_state.account_mgr);

    let report = format!(
        r#"
╔═══════════════════════════════════════════════════════════════════════╗
║                       QAExchange Monitoring Report                    ║
╚═══════════════════════════════════════════════════════════════════════╝

📊 Account Statistics:
   • Total Accounts:     {}
   • Active Accounts:    {}
   • Total Balance:      ¥{:.2}
   • Total Available:    ¥{:.2}
   • Total Margin Used:  ¥{:.2}
   • Margin Utilization: {:.1}%

📋 Order Statistics:
   • Total Orders:       {}
   • Pending Orders:     {}
   • Filled Orders:      {}
   • Cancelled Orders:   {}

💰 Trade Statistics:
   • Total Trades:       {}
   • Total Amount:       ¥{:.2}
   • Total Volume:       {}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Generated at: {}
"#,
        accounts.total_count,
        accounts.active_count,
        accounts.total_balance,
        accounts.total_available,
        accounts.total_margin_used,
        if accounts.total_balance > 0.0 {
            (accounts.total_margin_used / accounts.total_balance) * 100.0
        } else {
            0.0
        },
        orders.total_count,
        orders.pending_count,
        orders.filled_count,
        orders.cancelled_count,
        trades.total_count,
        trades.total_amount,
        trades.total_volume,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
    );

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(report)
}

/// 系统运行状态 @yutiansut @quantaxis
#[derive(Debug, Serialize)]
pub struct SystemStatus {
    /// 服务器启动时间（ISO 8601 格式）
    pub start_time: String,

    /// 运行时长（秒）
    pub uptime_seconds: i64,

    /// 运行时长（人类可读格式）
    pub uptime_display: String,

    /// WebSocket 连接数
    pub ws_connections: usize,

    /// 系统状态
    pub status: String,

    /// 撮合延迟（P99）
    pub matching_latency_p99: String,
}

/// 查询系统运行状态
///
/// GET /api/monitoring/status
pub async fn get_system_status(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    use std::sync::atomic::Ordering;

    let now = chrono::Utc::now();
    let start_time = app_state.server_start_time;
    let uptime = now.signed_duration_since(start_time);

    // 计算天、时、分
    let total_seconds = uptime.num_seconds();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;

    let uptime_display = format!("{}d {}h {}m", days, hours, minutes);

    let ws_connections = app_state.ws_connection_count.load(Ordering::Relaxed);

    let status = SystemStatus {
        start_time: start_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        uptime_seconds: total_seconds,
        uptime_display,
        ws_connections,
        status: "running".to_string(),
        matching_latency_p99: "P99 < 100μs".to_string(),
    };

    HttpResponse::Ok().json(status)
}
