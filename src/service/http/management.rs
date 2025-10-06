//! 账户、资金和风控管理 HTTP API
//!
//! 提供账户列表查询、出入金、资金流水、风险监控等管理功能

use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::exchange::{AccountManager, CapitalManager, FundTransaction};
use crate::risk::{RiskMonitor, RiskAccount, LiquidationRecord, MarginSummary, RiskLevel};
use super::models::ApiResponse;

/// 管理端应用状态
pub struct ManagementAppState {
    pub account_mgr: Arc<AccountManager>,
    pub capital_mgr: Arc<CapitalManager>,
    pub risk_monitor: Arc<RiskMonitor>,
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

    let mut account_list: Vec<AccountListItem> = accounts.iter()
        .filter_map(|account| {
            let mut acc = account.write();

            // 获取元数据
            let (_owner_user_id, account_name, account_type, created_at) =
                state.account_mgr.get_account_metadata(&acc.account_cookie)?;

            Some(AccountListItem {
                user_id: acc.account_cookie.clone(),
                user_name: account_name,
                account_type: format!("{:?}", account_type),
                balance: acc.get_balance(),
                available: acc.money,
                margin_used: acc.get_margin(),
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "accounts": account_list,
    }))))
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
                positions: qifi.positions.iter().map(|p| serde_json::to_value(p).unwrap()).collect(),
                orders: qifi.orders.iter().map(|o| serde_json::to_value(o).unwrap()).collect(),
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(detail)))
        }
        Err(e) => Ok(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(400, e.to_string())
        )),
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
        Err(e) => Ok(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(400, e.to_string())
        )),
    }
}

/// 出金
pub async fn withdraw(
    req: web::Json<WithdrawRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let remark = req.bank_account.as_ref()
        .map(|acc| format!("提现至银行账户: {}", acc));

    match state.capital_mgr.withdraw_with_record(
        req.user_id.clone(),
        req.amount,
        req.method.clone(),
        remark,
    ) {
        Ok(transaction) => Ok(HttpResponse::Ok().json(ApiResponse::success(transaction))),
        Err(e) => Ok(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(400, e.to_string())
        )),
    }
}

/// 查询资金流水
pub async fn get_transactions(
    user_id: web::Path<String>,
    query: web::Query<TransactionQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let transactions = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        // 按日期范围查询
        state.capital_mgr.get_transactions_by_date_range(&user_id, start, end)
    } else if let Some(limit) = query.limit {
        // 查询最近N条
        state.capital_mgr.get_recent_transactions(&user_id, limit)
    } else {
        // 查询全部
        state.capital_mgr.get_transactions(&user_id)
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(transactions)))
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

/// 获取风险账户列表
pub async fn get_risk_accounts(
    query: web::Query<RiskQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let risk_level_filter = query.risk_level.as_ref().and_then(|level| {
        match level.as_str() {
            "low" => Some(RiskLevel::Low),
            "medium" => Some(RiskLevel::Medium),
            "high" => Some(RiskLevel::High),
            "critical" => Some(RiskLevel::Critical),
            _ => None,
        }
    });

    let accounts = state.risk_monitor.get_risk_accounts(risk_level_filter);
    Ok(HttpResponse::Ok().json(ApiResponse::success(accounts)))
}

/// 获取保证金监控汇总
pub async fn get_margin_summary(
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let summary = state.risk_monitor.get_margin_summary();
    Ok(HttpResponse::Ok().json(ApiResponse::success(summary)))
}

/// 获取强平记录
pub async fn get_liquidation_records(
    query: web::Query<LiquidationQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let records = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        state.risk_monitor.get_liquidation_records_by_date_range(start, end)
    } else {
        state.risk_monitor.get_all_liquidation_records()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(records)))
}
