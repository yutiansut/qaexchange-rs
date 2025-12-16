//! 监控和统计 API 处理器

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::handlers::AppState;
use crate::exchange::{AccountManager, OrderRouter};
use crate::storage::conversion::ConversionManager;

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

    // 2. 订单统计
    let orders = get_order_stats(&app_state.order_router);

    // 3. 成交统计
    let trades = get_trade_stats(&app_state.order_router);

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
    let stats = get_order_stats(&app_state.order_router);
    HttpResponse::Ok().json(stats)
}

/// 查询成交统计
///
/// GET /api/monitoring/trades
pub async fn get_trades_monitoring(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let stats = get_trade_stats(&app_state.order_router);
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

fn get_order_stats(order_router: &OrderRouter) -> OrderStats {
    let stats = order_router.get_order_statistics();
    OrderStats {
        total_count: stats.total_count,
        pending_count: stats.pending_count,
        filled_count: stats.filled_count,
        cancelled_count: stats.cancelled_count,
    }
}

fn get_trade_stats(order_router: &OrderRouter) -> TradeStats {
    let stats = order_router.get_trade_statistics();
    TradeStats {
        total_count: stats.total_count as usize,
        total_amount: stats.total_amount,
        total_volume: stats.total_volume,
    }
}

/// 生成日志报告
///
/// GET /api/monitoring/report
pub async fn generate_report(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let accounts = get_account_stats(&app_state.account_mgr);
    let orders = get_order_stats(&app_state.order_router);
    let trades = get_trade_stats(&app_state.order_router);

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
