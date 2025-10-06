//! 管理端 HTTP API 处理器
//!
//! 提供合约管理、风控监控、结算管理等管理员功能的 HTTP API

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use log;

use crate::ExchangeError;
use crate::exchange::{InstrumentRegistry, SettlementEngine, AccountManager};
use crate::exchange::instrument_registry::{InstrumentInfo, InstrumentType, InstrumentStatus};

// ============================================================================
// 管理端应用状态
// ============================================================================

pub struct AdminAppState {
    pub instrument_registry: Arc<InstrumentRegistry>,
    pub settlement_engine: Arc<SettlementEngine>,
    pub account_mgr: Arc<AccountManager>,
}

// ============================================================================
// API 请求/响应模型
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorDetail>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ErrorDetail { message }),
        }
    }
}

// 合约创建请求
#[derive(Debug, Deserialize)]
pub struct CreateInstrumentRequest {
    pub instrument_id: String,
    pub instrument_name: String,
    pub instrument_type: InstrumentType,
    pub exchange: String,
    pub contract_multiplier: i32,
    pub price_tick: f64,
    pub margin_rate: f64,
    pub commission_rate: f64,
    pub limit_up_rate: f64,
    pub limit_down_rate: f64,
    pub list_date: Option<String>,
    pub expire_date: Option<String>,
}

// 合约更新请求
#[derive(Debug, Deserialize)]
pub struct UpdateInstrumentRequest {
    pub instrument_name: Option<String>,
    pub contract_multiplier: Option<i32>,
    pub price_tick: Option<f64>,
    pub margin_rate: Option<f64>,
    pub commission_rate: Option<f64>,
    pub limit_up_rate: Option<f64>,
    pub limit_down_rate: Option<f64>,
}

// ============================================================================
// 合约管理 API
// ============================================================================

/// 获取所有合约列表
pub async fn get_all_instruments(
    state: web::Data<AdminAppState>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("GET /api/admin/instruments");

    let instruments = state.instrument_registry.list_all();

    Ok(HttpResponse::Ok().json(ApiResponse::success(instruments)))
}

/// 创建/上市新合约
pub async fn create_instrument(
    state: web::Data<AdminAppState>,
    req: web::Json<CreateInstrumentRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("POST /api/admin/instrument/create: {:?}", req.instrument_id);

    let mut instrument = InstrumentInfo::new(
        req.instrument_id.clone(),
        req.instrument_name.clone(),
        req.instrument_type,
        req.exchange.clone(),
    );

    // 设置参数
    instrument.contract_multiplier = req.contract_multiplier;
    instrument.price_tick = req.price_tick;
    instrument.margin_rate = req.margin_rate;
    instrument.commission_rate = req.commission_rate;
    instrument.limit_up_rate = req.limit_up_rate;
    instrument.limit_down_rate = req.limit_down_rate;
    instrument.list_date = req.list_date.clone();
    instrument.expire_date = req.expire_date.clone();

    match state.instrument_registry.register(instrument) {
        Ok(_) => Ok(HttpResponse::Ok().json(ApiResponse::success(()))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

/// 更新合约信息
pub async fn update_instrument(
    state: web::Data<AdminAppState>,
    path: web::Path<String>,
    req: web::Json<UpdateInstrumentRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let instrument_id = path.into_inner();
    log::info!("PUT /api/admin/instrument/{}/update", instrument_id);

    let result = state.instrument_registry.update(&instrument_id, |info| {
        if let Some(name) = &req.instrument_name {
            info.instrument_name = name.clone();
        }
        if let Some(mult) = req.contract_multiplier {
            info.contract_multiplier = mult;
        }
        if let Some(tick) = req.price_tick {
            info.price_tick = tick;
        }
        if let Some(margin) = req.margin_rate {
            info.margin_rate = margin;
        }
        if let Some(commission) = req.commission_rate {
            info.commission_rate = commission;
        }
        if let Some(limit_up) = req.limit_up_rate {
            info.limit_up_rate = limit_up;
        }
        if let Some(limit_down) = req.limit_down_rate {
            info.limit_down_rate = limit_down;
        }
    });

    match result {
        Ok(_) => Ok(HttpResponse::Ok().json(ApiResponse::success(()))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

/// 暂停合约交易
pub async fn suspend_instrument(
    state: web::Data<AdminAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let instrument_id = path.into_inner();
    log::info!("PUT /api/admin/instrument/{}/suspend", instrument_id);

    match state.instrument_registry.suspend(&instrument_id) {
        Ok(_) => Ok(HttpResponse::Ok().json(ApiResponse::success(()))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

/// 恢复合约交易
pub async fn resume_instrument(
    state: web::Data<AdminAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let instrument_id = path.into_inner();
    log::info!("PUT /api/admin/instrument/{}/resume", instrument_id);

    match state.instrument_registry.resume(&instrument_id) {
        Ok(_) => Ok(HttpResponse::Ok().json(ApiResponse::success(()))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

/// 下市合约
pub async fn delist_instrument(
    state: web::Data<AdminAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let instrument_id = path.into_inner();
    log::info!("DELETE /api/admin/instrument/{}/delist", instrument_id);

    // 检查是否有未平仓持仓
    let accounts = state.account_mgr.get_all_accounts();
    let mut open_positions_count = 0;
    let mut accounts_with_positions = Vec::new();

    for account in accounts.iter() {
        let acc = account.read();
        if let Some(pos) = acc.get_position_unmut(&instrument_id) {
            let total_long = pos.volume_long_unmut();
            let total_short = pos.volume_short_unmut();
            if total_long > 0.0 || total_short > 0.0 {
                open_positions_count += 1;
                accounts_with_positions.push(acc.account_cookie.clone());
                log::warn!(
                    "Account {} has open positions for {}: long={}, short={}",
                    acc.account_cookie,
                    instrument_id,
                    total_long,
                    total_short
                );
            }
        }
    }

    if open_positions_count > 0 {
        let error_msg = format!(
            "Cannot delist {}: {} account(s) have open positions. Accounts: {}",
            instrument_id,
            open_positions_count,
            accounts_with_positions.join(", ")
        );
        log::error!("{}", error_msg);
        return Ok(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(error_msg)
        ));
    }

    match state.instrument_registry.delist(&instrument_id) {
        Ok(_) => {
            log::info!("Instrument {} delisted successfully", instrument_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(())))
        }
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

// ============================================================================
// 结算管理 API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SetSettlementPriceRequest {
    pub instrument_id: String,
    pub settlement_price: f64,
}

#[derive(Debug, Deserialize)]
pub struct BatchSetSettlementPricesRequest {
    pub prices: Vec<SetSettlementPriceRequest>,
}

/// 设置结算价
pub async fn set_settlement_price(
    state: web::Data<AdminAppState>,
    req: web::Json<SetSettlementPriceRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!(
        "POST /api/admin/settlement/set-price: {} = {}",
        req.instrument_id,
        req.settlement_price
    );

    state
        .settlement_engine
        .set_settlement_price(req.instrument_id.clone(), req.settlement_price);

    Ok(HttpResponse::Ok().json(ApiResponse::success(())))
}

/// 批量设置结算价
pub async fn batch_set_settlement_prices(
    state: web::Data<AdminAppState>,
    req: web::Json<BatchSetSettlementPricesRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("POST /api/admin/settlement/batch-set-prices: {} prices", req.prices.len());

    for price in &req.prices {
        state
            .settlement_engine
            .set_settlement_price(price.instrument_id.clone(), price.settlement_price);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(())))
}

/// 执行日终结算
pub async fn execute_settlement(
    state: web::Data<AdminAppState>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("POST /api/admin/settlement/execute");

    match state.settlement_engine.daily_settlement() {
        Ok(result) => Ok(HttpResponse::Ok().json(ApiResponse::success(result))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))),
    }
}

/// 获取结算历史
pub async fn get_settlement_history(
    state: web::Data<AdminAppState>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("GET /api/admin/settlement/history");

    let history = state.settlement_engine.get_settlement_history();

    Ok(HttpResponse::Ok().json(ApiResponse::success(history)))
}

/// 获取结算详情
pub async fn get_settlement_detail(
    state: web::Data<AdminAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let date = path.into_inner();
    log::debug!("GET /api/admin/settlement/detail/{}", date);

    match state.settlement_engine.get_settlement_detail(&date) {
        Some(detail) => Ok(HttpResponse::Ok().json(ApiResponse::success(detail))),
        None => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("Settlement not found".to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response() {
        let success_response = ApiResponse::success("test data");
        assert!(success_response.success);
        assert!(success_response.data.is_some());

        let error_response = ApiResponse::<String>::error("error message".to_string());
        assert!(!error_response.success);
        assert!(error_response.error.is_some());
    }
}
