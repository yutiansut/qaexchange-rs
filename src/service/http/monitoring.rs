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
    // ✨ 非有限/超量级防护(与 management.rs:237 同一 MONEY_MAX=1e15 口径)。
    // 单个账户 balance/available=1e300(测试账户 T1e300)会把全局合计吸成 1e300,
    // 令 dashboard 资金饼图「数据不可用」。management.rs 的分页求和早已跳过,
    // 这里 consolidate 同一 guard。@yutiansut @quantaxis
    const MONEY_MAX: f64 = 1e15;
    let mut excluded_nonfinite = 0usize;

    for account in accounts {
        let acc = account.read();

        // 检查是否有持仓
        if !acc.hold.is_empty() {
            active_count += 1;
        }

        // QA_Account 字段：money（可用资金）, accounts.balance（总权益）
        // ✨ 三个资金字段任一非有限/超界,整户从资金合计剔除(与 management.rs 一致)。
        let money_ok = acc.money.is_finite()
            && acc.money.abs() < MONEY_MAX
            && acc.accounts.balance.is_finite()
            && acc.accounts.balance.abs() < MONEY_MAX
            && acc.accounts.margin.is_finite()
            && acc.accounts.margin.abs() < MONEY_MAX;
        if money_ok {
            total_available += acc.money;
        }

        // 汇总资金流向 @yutiansut @quantaxis
        //
        // ⚠️ 原实现是 `total_withdraw += acc.accounts.withdraw`,而这个指标
        // **结构性恒为 0** —— qars 的 `withdraw()`(qars2 account.rs:541-548)
        // 做的是 `self.accounts.deposit -= amount`,**从不写 `accounts.withdraw`**
        // (该字段只在反序列化/恢复时被赋值)。实测面板显示
        // `total_deposit 9,910,601 / total_withdraw 0`,而系统确实发生过多笔出金。
        // 于是「总出金」永远是 0,「总入金」其实是**净额**,两个数都在误导读者。
        //
        // 拿不到真正的「总额」:资金变动分散在三条路径且没有统一账本 ——
        //   · `/api/management/deposit|withdraw` → CapitalManager 的 transactions
        //   · `/api/account/deposit|withdraw`    → **绕过 CapitalManager**(handlers.rs:808 注释)
        //   · 银期转账                            → TransferStore
        // 在数据模型允许的范围内做诚实呈现:按账户净额的正负拆分。
        // `total_deposit - total_withdraw` 仍等于真实净流入。
        if acc.accounts.deposit >= 0.0 {
            total_deposit += acc.accounts.deposit;
        } else {
            total_withdraw += -acc.accounts.deposit;
        }
        // qars 若将来真的写 withdraw 字段,这里也要计入,避免重复统计需先判零
        if acc.accounts.withdraw != 0.0 {
            total_withdraw += acc.accounts.withdraw;
        }

        // 总权益与保证金 —— 直接读账户自己的值,不要自己重算 @yutiansut @quantaxis
        //
        // ⚠️ 原实现自己遍历持仓重算,踩了两个坑,叠加后误差 381 倍
        //   (实测:本接口报总保证金 1,035 万,而逐账户求和是 39.48 亿):
        //     ① `volume * lastest_price` **漏了合约乘数**(IF 是 300)
        //        —— 与 order_router / pre_trade_check 同一个病(见 L21)
        //     ② `position_value * 0.15`「假设15%保证金」写死,
        //        既不是合约真实保证金率,也**完全不计冻结保证金**
        //
        // `acc.accounts.balance / margin` 由 `risk::risk_monitor` 每秒回写
        // (monitor_interval_ms 默认 1000),所以读锁路径就能拿到 ≤1s 新鲜的真值,
        // 不必为 get_margin() 升级写锁。口径与 management.rs:110 一致。
        if money_ok {
            total_balance += acc.accounts.balance;
            total_margin_used += acc.accounts.margin;
        } else {
            excluded_nonfinite += 1;
        }
    }
    if excluded_nonfinite > 0 {
        log::warn!(
            "[monitoring] {} 个账户因 balance/available/margin 非有限或超 {:.0e} 被剔除出资金合计",
            excluded_nonfinite, MONEY_MAX
        );
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

    // ⚠️ 读全局计数器,不读 AppState 的那个。
    // AppState.ws_connection_count 在 HTTP App(8094)里,而 WS 会话跑在
    // 独立的 WS App(8095)中,永远碰不到它 —— 那个字段恒为 0。
    // @yutiansut @quantaxis
    let ws_connections = crate::service::websocket::ws_connection_count();
    let _ = app_state.ws_connection_count.load(Ordering::Relaxed); // 保留字段以免破坏既有结构

    let status = SystemStatus {
        start_time: start_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        uptime_seconds: total_seconds,
        uptime_display,
        ws_connections,
        status: "running".to_string(),
        // ⚠️ 这个值**没有任何测量支撑**,是硬编码字面量 @yutiansut @quantaxis
        //
        // 前端 `dashboard/index.vue:211` 把它当「撮合延迟」显示给用户,
        // 而它从头到尾就是这一行字符串 —— 前后端各编一次。
        //
        // 真实的撮合延迟数据其实**存在**:`matching/engine.rs` 的
        // `test_latency_single_order` 实测 P50 1.6μs / P99 520μs(隔离下),
        // 但那是测试里的采样,生产没有在线统计。
        //
        // 改为明确的「未采集」,而不是编一个看起来很好的数 ——
        // 用户看到 "P99 < 100μs" 会以为系统在测量它。
        // 要真给这个数,需要在撮合热路径上加直方图统计(有性能代价,是独立决定)。
        matching_latency_p99: "未采集".to_string(),
    };

    HttpResponse::Ok().json(status)
}
