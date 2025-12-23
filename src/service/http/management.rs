//! 账户、资金和风控管理 HTTP API
//!
//! 提供账户列表查询、出入金、资金流水、风险监控、全市场订单/成交查询等管理功能
//! @yutiansut @quantaxis

use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::models::ApiResponse;
use crate::exchange::{AccountManager, CapitalManager, FundTransaction, OrderRouter, SettlementEngine};
use crate::matching::trade_recorder::TradeRecorder;
use crate::risk::{LiquidationRecord, MarginSummary, RiskAccount, RiskLevel, RiskMonitor};

/// 管理端应用状态
/// @yutiansut @quantaxis
pub struct ManagementAppState {
    pub account_mgr: Arc<AccountManager>,
    pub capital_mgr: Arc<CapitalManager>,
    pub risk_monitor: Arc<RiskMonitor>,
    pub settlement_engine: Arc<SettlementEngine>,
    /// 订单路由器 (用于全市场订单查询)
    pub order_router: Arc<OrderRouter>,
    /// 成交记录器 (用于全市场成交查询)
    pub trade_recorder: Arc<TradeRecorder>,
}

// ============================================================================
// 账户管理 API
// ============================================================================

/// 账户列表响应
#[derive(Debug, Clone, Serialize)]
pub struct AccountListItem {
    pub user_id: String,
    pub user_name: String,
    pub account_type: String,
    pub balance: f64,
    pub available: f64,
    pub margin_used: f64,
    pub risk_ratio: f64,
    pub created_at: i64,
}

/// 账户详情响应
#[derive(Debug, Serialize)]
pub struct AccountDetailResponse {
    pub account_info: serde_json::Value,
    pub positions: Vec<serde_json::Value>,
    pub orders: Vec<serde_json::Value>,
}

/// 查询参数
#[derive(Debug, Deserialize)]
pub struct AccountListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub status: Option<String>,
}

/// 获取所有账户列表 (管理端)
pub async fn list_all_accounts(
    query: web::Query<AccountListQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let accounts = state.account_mgr.get_all_accounts();

    let mut account_list: Vec<AccountListItem> = accounts
        .iter()
        .filter_map(|account| {
            let mut acc = account.write();

            // 获取元数据
            let (_owner_user_id, account_name, account_type, created_at) = state
                .account_mgr
                .get_account_metadata(&acc.account_cookie)?;

            // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
            let position_margin = acc.get_margin();
            let frozen_margin = acc.get_frozen_margin();
            let total_margin = position_margin + frozen_margin;

            Some(AccountListItem {
                user_id: acc.account_cookie.clone(),
                user_name: account_name,
                account_type: format!("{:?}", account_type),
                balance: acc.get_balance(),
                available: acc.money,
                margin_used: total_margin,
                risk_ratio: acc.get_riskratio(),
                created_at,
            })
        })
        .collect();

    // 分页处理
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    let total = account_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    if start < total {
        account_list = account_list[start..end].to_vec();
    } else {
        account_list = vec![];
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "accounts": account_list,
        }))),
    )
}

/// 获取账户详情
pub async fn get_account_detail(
    user_id: web::Path<String>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_qifi_slice(&user_id) {
        Ok(qifi) => {
            let detail = AccountDetailResponse {
                account_info: serde_json::to_value(&qifi.accounts).unwrap(),
                positions: qifi
                    .positions
                    .iter()
                    .map(|p| serde_json::to_value(p).unwrap())
                    .collect(),
                orders: qifi
                    .orders
                    .iter()
                    .map(|o| serde_json::to_value(o).unwrap())
                    .collect(),
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(detail)))
        }
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

// ============================================================================
// 资金管理 API
// ============================================================================

/// 入金请求
#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub user_id: String,
    pub amount: f64,
    pub method: Option<String>,
    pub remark: Option<String>,
}

/// 出金请求
#[derive(Debug, Deserialize)]
pub struct WithdrawRequest {
    pub user_id: String,
    pub amount: f64,
    pub method: Option<String>,
    pub bank_account: Option<String>,
}

/// 资金流水查询参数
#[derive(Debug, Deserialize)]
pub struct TransactionQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<usize>,
}

/// 入金
pub async fn deposit(
    req: web::Json<DepositRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state.capital_mgr.deposit_with_record(
        req.user_id.clone(),
        req.amount,
        req.method.clone(),
        req.remark.clone(),
    ) {
        Ok(transaction) => Ok(HttpResponse::Ok().json(ApiResponse::success(transaction))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

/// 出金
pub async fn withdraw(
    req: web::Json<WithdrawRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let remark = req
        .bank_account
        .as_ref()
        .map(|acc| format!("提现至银行账户: {}", acc));

    match state.capital_mgr.withdraw_with_record(
        req.user_id.clone(),
        req.amount,
        req.method.clone(),
        remark,
    ) {
        Ok(transaction) => Ok(HttpResponse::Ok().json(ApiResponse::success(transaction))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

/// 查询资金流水
///
/// 合并查询 CapitalManager 和 TransferStore 的记录 @yutiansut @quantaxis
pub async fn get_transactions(
    user_id: web::Path<String>,
    query: web::Query<TransactionQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    use crate::exchange::{TransactionStatus, TransactionType};
    use crate::service::http::transfer::TRANSFER_STORE;

    // 1. 从 CapitalManager 获取记录
    let mut transactions = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        state
            .capital_mgr
            .get_transactions_by_date_range(&user_id, start, end)
    } else if let Some(limit) = query.limit {
        state.capital_mgr.get_recent_transactions(&user_id, limit)
    } else {
        state.capital_mgr.get_transactions(&user_id)
    };

    // 2. 从 TransferStore 获取记录（通过 user 的所有账户）
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);
    for account in accounts {
        let account_id = account.read().account_cookie.clone();
        let (transfer_records, _) = TRANSFER_STORE.get_records(&account_id, None, None, None, None);

        // 转换 TransferRecord 为 FundTransaction
        for record in transfer_records {
            let transaction = FundTransaction {
                transaction_id: record.id,
                user_id: user_id.to_string(),
                transaction_type: if record.amount >= 0.0 {
                    TransactionType::Deposit
                } else {
                    TransactionType::Withdrawal
                },
                amount: record.amount.abs(),
                balance_before: 0.0,  // TransferStore 不记录这个
                balance_after: 0.0,   // TransferStore 不记录这个
                status: if record.error_id == 0 {
                    TransactionStatus::Completed
                } else {
                    TransactionStatus::Failed
                },
                method: Some(record.bank_name),
                remark: Some(record.error_msg),
                created_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default(),
                updated_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default(),
            };
            transactions.push(transaction);
        }
    }

    // 3. 按时间排序（最新的在前）
    transactions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 4. 如果有 limit，截取
    if let Some(limit) = query.limit {
        transactions.truncate(limit);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(transactions)))
}

/// 查询所有账户的资金流水（不需要指定用户）
///
/// @yutiansut @quantaxis
/// 管理端默认加载全部流水，支持分页和筛选
pub async fn get_all_transactions(
    query: web::Query<AllTransactionsQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    use crate::exchange::{TransactionStatus, TransactionType};
    use crate::service::http::transfer::TRANSFER_STORE;
    use rayon::prelude::*;

    // 1. 并行遍历所有账户，从 TransferStore 获取流水
    let all_accounts = state.account_mgr.get_all_accounts();

    // 先收集账户信息（需要释放锁）
    let account_infos: Vec<(String, String)> = all_accounts
        .iter()
        .map(|account| {
            let acc = account.read();
            (acc.account_cookie.clone(), acc.portfolio_cookie.clone())
        })
        .collect();

    // 使用 rayon 并行处理每个账户的 TransferStore 记录
    let transfer_transactions: Vec<FundTransaction> = account_infos
        .par_iter()
        .flat_map(|(account_id, user_id)| {
            let (transfer_records, _) = TRANSFER_STORE.get_records(account_id, None, None, None, None);

            transfer_records.into_iter().map(|record| {
                FundTransaction {
                    transaction_id: record.id,
                    user_id: user_id.clone(),
                    transaction_type: if record.amount >= 0.0 {
                        TransactionType::Deposit
                    } else {
                        TransactionType::Withdrawal
                    },
                    amount: record.amount.abs(),
                    balance_before: 0.0,
                    balance_after: 0.0,
                    status: if record.error_id == 0 {
                        TransactionStatus::Completed
                    } else {
                        TransactionStatus::Failed
                    },
                    method: Some(record.bank_name),
                    remark: Some(format!("{} (账户: {})", record.error_msg, account_id)),
                    created_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                    updated_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                }
            }).collect::<Vec<_>>()
        })
        .collect();

    // 2. 从 CapitalManager 获取所有用户的流水（已经使用rayon）
    let capital_transactions = state.capital_mgr.get_all_transactions();

    // 3. 合并流水
    let mut all_transactions = transfer_transactions;
    all_transactions.extend(capital_transactions);

    // 4. 使用 rayon 并行排序（最新的在前）
    all_transactions.par_sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 5. 筛选类型
    if let Some(ref tx_type) = query.transaction_type {
        let tx_type_clone = tx_type.clone();
        all_transactions = all_transactions
            .into_par_iter()
            .filter(|t| {
                match tx_type_clone.as_str() {
                    "deposit" => matches!(t.transaction_type, TransactionType::Deposit),
                    "withdrawal" => matches!(t.transaction_type, TransactionType::Withdrawal),
                    _ => true,
                }
            })
            .collect();
    }

    // 6. 分页
    let page = query.page.unwrap_or(1).max(1) as usize;
    let page_size = query.page_size.unwrap_or(20).min(100) as usize;
    let total = all_transactions.len();
    let start = (page - 1) * page_size;

    let paged: Vec<_> = all_transactions
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect();

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "records": paged,
        "total": total,
        "page": page,
        "page_size": page_size
    }))))
}

/// 全部流水查询参数
#[derive(Debug, Deserialize)]
pub struct AllTransactionsQuery {
    pub transaction_type: Option<String>,  // deposit / withdrawal
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

// ============================================================================
// 风控监控 API
// ============================================================================

/// 风险监控查询参数
#[derive(Debug, Deserialize)]
pub struct RiskQuery {
    pub risk_level: Option<String>,
}

/// 强平记录查询参数
#[derive(Debug, Deserialize)]
pub struct LiquidationQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// 强平请求
#[derive(Debug, Deserialize)]
pub struct ForceLiquidateRequest {
    pub account_id: String,
    pub reason: Option<String>,
}

/// 获取风险账户列表
pub async fn get_risk_accounts(
    query: web::Query<RiskQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let risk_level_filter = query
        .risk_level
        .as_ref()
        .and_then(|level| match level.as_str() {
            "low" => Some(RiskLevel::Low),
            "medium" => Some(RiskLevel::Medium),
            "high" => Some(RiskLevel::High),
            "critical" => Some(RiskLevel::Critical),
            _ => None,
        });

    let accounts = state.risk_monitor.get_risk_accounts(risk_level_filter);
    Ok(HttpResponse::Ok().json(ApiResponse::success(accounts)))
}

/// 获取保证金监控汇总
pub async fn get_margin_summary(state: web::Data<ManagementAppState>) -> Result<HttpResponse> {
    let summary = state.risk_monitor.get_margin_summary();
    Ok(HttpResponse::Ok().json(ApiResponse::success(summary)))
}

/// 获取强平记录
pub async fn get_liquidation_records(
    query: web::Query<LiquidationQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let records = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        state
            .risk_monitor
            .get_liquidation_records_by_date_range(start, end)
    } else {
        state.risk_monitor.get_all_liquidation_records()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(records)))
}

/// 触发强平
pub async fn force_liquidate_account(
    req: web::Json<ForceLiquidateRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state
        .settlement_engine
        .force_liquidate_account(&req.account_id, req.reason.clone())
    {
        Ok(result) => Ok(HttpResponse::Ok().json(ApiResponse::success(result))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            400,
            format!("Force liquidation failed: {}", e),
        ))),
    }
}

// ============================================================================
// 全市场订单/成交查询 API (管理端)
// ============================================================================

/// 全市场订单查询参数
/// @yutiansut @quantaxis
#[derive(Debug, Deserialize)]
pub struct AllOrdersQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub status: Option<String>,
    pub instrument_id: Option<String>,
}

/// 全市场成交查询参数
/// @yutiansut @quantaxis
#[derive(Debug, Deserialize)]
pub struct AllTradesQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub instrument_id: Option<String>,
}

/// 订单列表项 (管理端)
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize)]
pub struct OrderListItem {
    pub order_id: String,
    pub user_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub filled_volume: f64,
    pub status: String,
    pub submit_time: i64,
    pub update_time: i64,
}

/// 成交列表项 (管理端)
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize)]
pub struct TradeListItem {
    pub trade_id: String,
    pub instrument_id: String,
    pub buy_user_id: String,
    pub sell_user_id: String,
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub trading_day: String,
}

/// 获取全市场所有订单 (管理端)
/// @yutiansut @quantaxis
pub async fn list_all_orders(
    query: web::Query<AllOrdersQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let all_orders = state.order_router.get_all_orders();

    // 转换为列表项
    // QIFI Order 字段: user_id = account_cookie, limit_price, volume_orign
    // @yutiansut @quantaxis
    let mut order_list: Vec<OrderListItem> = all_orders
        .into_iter()
        .map(|(order_id, order, status, submit_time, update_time, filled_volume)| {
            OrderListItem {
                order_id,
                user_id: order.user_id.clone(),     // QIFI: user_id 即账户ID
                account_id: order.user_id.clone(),  // QIFI: 同 user_id
                instrument_id: order.instrument_id.clone(),
                direction: order.direction.clone(),  // QIFI: 已是 String
                offset: order.offset.clone(),        // QIFI: 已是 String
                price: order.limit_price,            // QIFI: limit_price
                volume: order.volume_orign,          // QIFI: volume_orign
                filled_volume,
                status: format!("{:?}", status),
                submit_time,
                update_time,
            }
        })
        .collect();

    // 过滤: 状态
    if let Some(ref status_filter) = query.status {
        order_list.retain(|o| o.status.to_lowercase().contains(&status_filter.to_lowercase()));
    }

    // 过滤: 合约
    if let Some(ref inst_filter) = query.instrument_id {
        order_list.retain(|o| o.instrument_id.contains(inst_filter));
    }

    // 按更新时间降序排序
    order_list.sort_by(|a, b| b.update_time.cmp(&a.update_time));

    // 分页处理
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(50);
    let total = order_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    let paged_list = if start < total {
        order_list[start..end].to_vec()
    } else {
        vec![]
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "orders": paged_list,
        }))),
    )
}

/// 获取全市场所有成交 (管理端)
/// @yutiansut @quantaxis
pub async fn list_all_trades(
    query: web::Query<AllTradesQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let all_trades = state.trade_recorder.get_all_trades();

    // 转换为列表项
    let mut trade_list: Vec<TradeListItem> = all_trades
        .into_iter()
        .map(|t| TradeListItem {
            trade_id: t.trade_id,
            instrument_id: t.instrument_id,
            buy_user_id: t.buy_user_id,
            sell_user_id: t.sell_user_id,
            buy_order_id: t.buy_order_id,
            sell_order_id: t.sell_order_id,
            price: t.price,
            volume: t.volume,
            timestamp: t.timestamp,
            trading_day: t.trading_day,
        })
        .collect();

    // 过滤: 合约
    if let Some(ref inst_filter) = query.instrument_id {
        trade_list.retain(|t| t.instrument_id.contains(inst_filter));
    }

    // 按时间降序排序
    trade_list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 分页处理
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(50);
    let total = trade_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    let paged_list = if start < total {
        trade_list[start..end].to_vec()
    } else {
        vec![]
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "trades": paged_list,
        }))),
    )
}
