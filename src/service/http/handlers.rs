//! HTTP API 请求处理器

use actix_web::{web, HttpResponse, Result};
use log;
use std::sync::Arc;

use super::models::*;
use crate::core::account_ext::{AccountType, OpenAccountRequest as CoreOpenAccountRequest};
use crate::exchange::order_router::{
    CancelOrderRequest as CoreCancelOrderRequest, SubmitOrderRequest as CoreSubmitOrderRequest,
};
use crate::exchange::{AccountManager, OrderRouter};
use crate::matching::trade_recorder::TradeRecorder;
use crate::storage::conversion::ConversionManager;
use crate::storage::subscriber::SubscriberStats;
use crate::user::UserManager;

/// 应用状态
pub struct AppState {
    pub order_router: Arc<OrderRouter>,
    pub account_mgr: Arc<AccountManager>,
    pub trade_recorder: Arc<TradeRecorder>,
    pub user_mgr: Arc<UserManager>,
    pub storage_stats: Option<Arc<parking_lot::Mutex<SubscriberStats>>>,
    pub conversion_mgr: Option<Arc<parking_lot::Mutex<ConversionManager>>>,
}

/// 健康检查
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "qaexchange"
    }))
}

/// 开户
pub async fn open_account(
    req: web::Json<OpenAccountRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let account_type = match req.account_type.as_str() {
        "individual" => AccountType::Individual,
        "institutional" => AccountType::Institutional,
        _ => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                "Invalid account type".to_string(),
            )))
        }
    };

    let core_req = CoreOpenAccountRequest {
        user_id: req.user_id.clone(),
        account_id: None,                    // Auto-generate
        account_name: req.user_name.clone(), // Use user_name as account_name
        init_cash: req.init_cash,
        account_type,
    };

    match state.account_mgr.open_account(core_req) {
        Ok(account_id) => {
            log::info!("Account opened: {}", account_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({ "account_id": account_id }),
            )))
        }
        Err(e) => {
            log::error!("Failed to open account: {:?}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    500,
                    format!("Failed to open account: {:?}", e),
                )),
            )
        }
    }
}

/// 查询账户（按 account_id 查询单个账户）
pub async fn query_account(
    account_id: web::Path<String>, // 修复: 改为 account_id
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_account(&account_id) {
        Ok(account) => {
            let acc = account.read();
            let frozen = acc.accounts.balance - acc.money;

            // 获取账户元数据
            let (_owner_user_id, account_name, account_type, created_at) = state
                .account_mgr
                .get_account_metadata(&account_id)
                .unwrap_or_else(|| {
                    (
                        "unknown".to_string(),
                        account_id.to_string(),
                        crate::core::account_ext::AccountType::Individual,
                        0,
                    )
                });

            let info = AccountInfo {
                user_id: acc.account_cookie.clone(),
                user_name: account_name,
                balance: acc.accounts.balance,
                available: acc.money,
                frozen,
                margin: acc.accounts.margin,
                profit: acc.accounts.close_profit,
                risk_ratio: acc.accounts.risk_ratio,
                account_type: format!("{:?}", account_type).to_lowercase(),
                created_at,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(info)))
        }
        Err(e) => {
            log::error!("Failed to query account: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Account not found: {:?}", e),
            )))
        }
    }
}

/// 提交订单
pub async fn submit_order(
    req: web::Json<SubmitOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 服务层：验证账户所有权并获取 account_id
    let account_id = if let Some(ref acc_id) = req.account_id {
        // ✅ 客户端明确传递了 account_id，验证所有权
        if let Err(e) = state
            .account_mgr
            .verify_account_ownership(acc_id, &req.user_id)
        {
            return Ok(HttpResponse::Forbidden().json(ApiResponse::<()>::error(
                4003,
                format!("Account verification failed: {}", e),
            )));
        }
        acc_id.clone()
    } else {
        // ⚠️ 向后兼容：客户端未传递 account_id，使用默认账户
        log::warn!("DEPRECATED: Client did not provide account_id for user {}. This behavior will be removed in future versions.", req.user_id);

        match state.account_mgr.get_default_account(&req.user_id) {
            Ok(account_arc) => {
                let acc = account_arc.read();
                acc.account_cookie.clone()
            }
            Err(e) => {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    4000,
                    format!("Account not found for user {}: {}", req.user_id, e),
                )));
            }
        }
    };

    let core_req = CoreSubmitOrderRequest {
        account_id, // 交易层只关心 account_id
        instrument_id: req.instrument_id.clone(),
        direction: req.direction.clone(),
        offset: req.offset.clone(),
        volume: req.volume,
        price: req.price,
        order_type: req.order_type.clone(),
    };

    let response = state.order_router.submit_order(core_req);

    if response.success {
        let resp = SubmitOrderResponse {
            order_id: response.order_id.unwrap_or_default(),
            status: response.status.unwrap_or_else(|| "submitted".to_string()),
        };
        Ok(HttpResponse::Ok().json(ApiResponse::success(resp)))
    } else {
        Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            response.error_code.unwrap_or(400),
            response
                .error_message
                .unwrap_or_else(|| "Order submission failed".to_string()),
        )))
    }
}

/// 撤单
pub async fn cancel_order(
    req: web::Json<CancelOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 服务层：验证账户所有权并获取 account_id
    let account_id = if let Some(ref acc_id) = req.account_id {
        // ✅ 客户端明确传递了 account_id，验证所有权
        if let Err(e) = state
            .account_mgr
            .verify_account_ownership(acc_id, &req.user_id)
        {
            return Ok(HttpResponse::Forbidden().json(ApiResponse::<()>::error(
                4003,
                format!("Account verification failed: {}", e),
            )));
        }
        acc_id.clone()
    } else {
        // ⚠️ 向后兼容：客户端未传递 account_id，使用默认账户
        log::warn!("DEPRECATED: Client did not provide account_id for user {}. This behavior will be removed in future versions.", req.user_id);

        match state.account_mgr.get_default_account(&req.user_id) {
            Ok(account_arc) => {
                let acc = account_arc.read();
                acc.account_cookie.clone()
            }
            Err(e) => {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    4000,
                    format!("Account not found for user {}: {}", req.user_id, e),
                )));
            }
        }
    };

    let core_req = CoreCancelOrderRequest {
        account_id, // 交易层只关心 account_id
        order_id: req.order_id.clone(),
    };

    match state.order_router.cancel_order(core_req) {
        Ok(_) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({ "order_id": req.order_id }),
        ))),
        Err(e) => {
            log::error!("Failed to cancel order: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!("Cancel order failed: {:?}", e),
            )))
        }
    }
}

/// 查询订单
pub async fn query_order(
    order_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.order_router.get_order_detail(&order_id) {
        Some((order, status, submit_time, update_time, filled_volume)) => {
            let info = OrderInfo {
                order_id: order_id.to_string(),
                user_id: order.user_id,
                instrument_id: order.instrument_id,
                direction: order.direction,
                offset: order.offset,
                volume: order.volume_orign,
                price: order.limit_price,
                filled_volume,
                status: format!("{:?}", status),
                submit_time,
                update_time,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(info)))
        }
        None => {
            log::error!("Order not found: {}", order_id);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Order not found: {}", order_id),
            )))
        }
    }
}

/// 查询用户订单列表
pub async fn query_user_orders(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let order_details = state.order_router.get_user_order_details(&user_id);

    let order_infos: Vec<OrderInfo> = order_details
        .into_iter()
        .map(
            |(order_id, order, status, submit_time, update_time, filled_volume)| OrderInfo {
                order_id,
                user_id: order.user_id,
                instrument_id: order.instrument_id,
                direction: order.direction,
                offset: order.offset,
                volume: order.volume_orign,
                price: order.limit_price,
                filled_volume,
                status: format!("{:?}", status),
                submit_time,
                update_time,
            },
        )
        .collect();

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "orders": order_infos,
            "total": order_infos.len()
        }))),
    )
}

/// 查询持仓（按account_id查询单个账户）
pub async fn query_position(
    account_id: web::Path<String>, // 修复: 改为account_id
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_account(&account_id) {
        Ok(account) => {
            let mut acc = account.write(); // 需要 mut 才能调用 float_profit 方法
            let mut positions = Vec::new();
            for (code, pos) in acc.hold.iter_mut() {
                positions.push(PositionInfo {
                    account_id: account_id.to_string(), // 添加account_id
                    instrument_id: code.clone(),
                    volume_long: pos.volume_long_today + pos.volume_long_his,
                    volume_short: pos.volume_short_today + pos.volume_short_his,
                    cost_long: pos.open_price_long,
                    cost_short: pos.open_price_short,
                    profit_long: pos.float_profit_long(),
                    profit_short: pos.float_profit_short(),
                });
            }

            Ok(HttpResponse::Ok().json(ApiResponse::success(positions)))
        }
        Err(e) => {
            log::error!("Failed to query position by account_id: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Account not found: {:?}", e),
            )))
        }
    }
}

/// 查询持仓（按user_id查询该用户所有账户的持仓）
pub async fn query_positions_by_user(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);

    if accounts.is_empty() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("No accounts found for user: {}", user_id),
        )));
    }

    let mut all_positions = Vec::new();
    for account in accounts {
        let mut acc = account.write();
        let acc_id = acc.account_cookie.clone(); // 获取account_id
        for (code, pos) in acc.hold.iter_mut() {
            all_positions.push(PositionInfo {
                account_id: acc_id.clone(), // 添加account_id
                instrument_id: code.clone(),
                volume_long: pos.volume_long_today + pos.volume_long_his,
                volume_short: pos.volume_short_today + pos.volume_short_his,
                cost_long: pos.open_price_long,
                cost_short: pos.open_price_short,
                profit_long: pos.float_profit_long(),
                profit_short: pos.float_profit_short(),
            });
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(all_positions)))
}

/// 入金
pub async fn deposit(
    req: web::Json<DepositRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_account(&req.user_id) {
        Ok(account) => {
            let mut acc = account.write();
            // 使用 QA_Account 的标准 deposit 方法
            acc.deposit(req.amount);

            log::info!("Deposit {} to account {}", req.amount, req.user_id);

            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "balance": acc.get_balance(),
                    "available": acc.money
                }))),
            )
        }
        Err(e) => {
            log::error!("Failed to deposit: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Account not found: {:?}", e),
            )))
        }
    }
}

/// 出金
pub async fn withdraw(
    req: web::Json<WithdrawRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_account(&req.user_id) {
        Ok(account) => {
            let mut acc = account.write();

            // 检查可用余额（acc.money 才是真正的可用资金）
            if acc.money < req.amount {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    400,
                    "Insufficient available balance".to_string(),
                )));
            }

            // 使用 QA_Account 的标准 withdraw 方法
            acc.withdraw(req.amount);

            log::info!("Withdraw {} from account {}", req.amount, req.user_id);

            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "balance": acc.get_balance(),
                    "available": acc.money
                }))),
            )
        }
        Err(e) => {
            log::error!("Failed to withdraw: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Account not found: {:?}", e),
            )))
        }
    }
}

/// 查询用户成交记录
pub async fn query_user_trades(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 按user_id查询：聚合该用户所有账户的成交记录
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);

    let mut all_trades = Vec::new();
    for account in accounts {
        let acc = account.read();
        let account_id = &acc.account_cookie;
        let trades = state.trade_recorder.get_trades_by_user(account_id); // 注意：这里的by_user实际上是by_account
        all_trades.extend(trades);
    }

    log::info!(
        "Querying trades for user: {}, found {} trades",
        user_id,
        all_trades.len()
    );

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "trades": all_trades,
            "total": all_trades.len()
        }))),
    )
}

/// 查询账户成交记录（按account_id）
pub async fn query_account_trades(
    account_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 注意：TradeRecorder.by_user 实际上索引的是 account_id
    let trades = state.trade_recorder.get_trades_by_user(&account_id);

    log::info!(
        "Querying trades for account: {}, found {} trades",
        account_id,
        trades.len()
    );

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "trades": trades,
            "total": trades.len()
        }))),
    )
}

// ==================== 用户账户管理 API (Phase 10) ====================

/// 为用户创建新的交易账户
pub async fn create_user_account(
    user_id: web::Path<String>,
    req: web::Json<CreateAccountRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let account_type = match req.account_type.as_str() {
        "individual" => AccountType::Individual,
        "institutional" => AccountType::Institutional,
        "market_maker" => AccountType::MarketMaker,
        _ => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                "Invalid account type".to_string(),
            )))
        }
    };

    let core_req = CoreOpenAccountRequest {
        user_id: user_id.to_string(),
        account_id: None, // Auto-generate
        account_name: req.account_name.clone(),
        init_cash: req.init_cash,
        account_type,
    };

    match state.account_mgr.open_account(core_req) {
        Ok(account_id) => {
            log::info!("Account created for user {}: {}", user_id, account_id);
            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "account_id": account_id,
                    "message": "账户创建成功"
                }))),
            )
        }
        Err(e) => {
            log::error!("Failed to create account for user {}: {:?}", user_id, e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    500,
                    format!("Failed to create account: {:?}", e),
                )),
            )
        }
    }
}

/// 查询用户的所有交易账户
pub async fn get_user_accounts(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);

    let account_list: Vec<serde_json::Value> = accounts
        .iter()
        .map(|account| {
            let mut acc = account.write();
            let (_owner_user_id, account_name, account_type, created_at) = state
                .account_mgr
                .get_account_metadata(&acc.account_cookie)
                .unwrap_or_else(|| {
                    (
                        user_id.to_string(),
                        acc.account_cookie.clone(),
                        AccountType::Individual,
                        0,
                    )
                });

            serde_json::json!({
                "account_id": acc.account_cookie.clone(),
                "account_name": account_name,
                "account_type": format!("{:?}", account_type),
                "balance": acc.get_balance(),
                "available": acc.money,
                "margin": acc.get_margin(),
                "risk_ratio": acc.get_riskratio(),
                "created_at": created_at,
            })
        })
        .collect();

    log::info!("Found {} accounts for user {}", account_list.len(), user_id);

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "accounts": account_list,
            "total": account_list.len()
        }))),
    )
}
