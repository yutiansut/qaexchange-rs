//! 市场数据 HTTP API 处理器
//!
//! 网络层：仅负责 HTTP 请求/响应处理，调用 MarketDataService 的业务逻辑

use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};

use super::models::ApiResponse;
use crate::market::MarketDataService;

/// 订单簿查询请求
#[derive(Debug, Deserialize)]
pub struct OrderBookQuery {
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_depth() -> usize {
    5  // 默认五档
}

/// 获取订单簿（买卖盘）
///
/// GET /api/market/orderbook/{instrument_id}?depth=5
pub async fn get_orderbook(
    instrument_id: web::Path<String>,
    query: web::Query<OrderBookQuery>,
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    match market_service.get_orderbook_snapshot(&instrument_id, query.depth) {
        Ok(snapshot) => Ok(HttpResponse::Ok().json(ApiResponse::success(snapshot))),
        Err(e) => {
            log::error!("Failed to get orderbook for {}: {}", instrument_id, e);
            Ok(HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(500, format!("Failed to get orderbook: {}", e))
            ))
        }
    }
}

/// 获取合约列表
///
/// GET /api/market/instruments
pub async fn get_instruments(
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    match market_service.get_instruments() {
        Ok(instruments) => Ok(HttpResponse::Ok().json(ApiResponse::success(instruments))),
        Err(e) => {
            log::error!("Failed to get instruments: {}", e);
            Ok(HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(500, format!("Failed to get instruments: {}", e))
            ))
        }
    }
}

/// 获取 Tick 数据（实时行情）
///
/// GET /api/market/tick/{instrument_id}
pub async fn get_tick(
    instrument_id: web::Path<String>,
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    match market_service.get_tick_data(&instrument_id) {
        Ok(tick) => Ok(HttpResponse::Ok().json(ApiResponse::success(tick))),
        Err(e) => {
            log::error!("Failed to get tick for {}: {}", instrument_id, e);
            Ok(HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(500, format!("Failed to get tick: {}", e))
            ))
        }
    }
}

/// 获取最近成交记录
///
/// GET /api/market/trades/{instrument_id}?limit=20
#[derive(Debug, Deserialize)]
pub struct TradesQuery {
    #[serde(default = "default_trades_limit")]
    pub limit: usize,
}

fn default_trades_limit() -> usize {
    20
}

pub async fn get_recent_trades(
    instrument_id: web::Path<String>,
    query: web::Query<TradesQuery>,
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    match market_service.get_recent_trades(&instrument_id, query.limit) {
        Ok(trades) => Ok(HttpResponse::Ok().json(ApiResponse::success(trades))),
        Err(e) => {
            log::error!("Failed to get trades for {}: {}", instrument_id, e);
            Ok(HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(500, format!("Failed to get trades: {}", e))
            ))
        }
    }
}

/// 管理员功能：获取市场订单统计
///
/// GET /api/admin/market/order-stats
pub async fn get_market_order_stats(
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    match market_service.get_market_order_stats() {
        Ok(stats) => Ok(HttpResponse::Ok().json(ApiResponse::success(stats))),
        Err(e) => {
            log::error!("Failed to get market order stats: {}", e);
            Ok(HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(500, format!("Failed to get stats: {}", e))
            ))
        }
    }
}
