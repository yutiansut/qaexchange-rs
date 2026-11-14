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
    5 // 默认五档
}

/// 获取订单簿（买卖盘）
///
/// GET /api/market/orderbook/{instrument_id}?depth=5
pub async fn get_orderbook(
    instrument_id: web::Path<String>,
    query: web::Query<OrderBookQuery>,
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    log::info!(
        "🔍 [HTTP API] GET /api/market/orderbook/{}?depth={}",
        instrument_id,
        query.depth
    );

    match market_service.get_orderbook_snapshot(&instrument_id, query.depth) {
        Ok(snapshot) => {
            log::info!(
                "✅ [HTTP API] Orderbook found for {}: {} bids, {} asks",
                instrument_id,
                snapshot.bids.len(),
                snapshot.asks.len()
            );
            Ok(HttpResponse::Ok().json(ApiResponse::success(snapshot)))
        }
        // 统一错误映射 —— 「合约不存在」应是 404 而非 500。
        // 映射表与理由见 `models.rs::exchange_error_response`。@yutiansut @quantaxis
        Err(e) => Ok(super::models::exchange_error_response("Failed to get orderbook", &e)),
    }
}

/// 获取合约列表
///
/// GET /api/market/instruments
pub async fn get_instruments(market_service: web::Data<MarketDataService>) -> Result<HttpResponse> {
    match market_service.get_instruments() {
        Ok(instruments) => Ok(HttpResponse::Ok().json(ApiResponse::success(instruments))),
        // 统一错误映射 —— 「合约不存在」应是 404 而非 500。
        // 映射表与理由见 `models.rs::exchange_error_response`。@yutiansut @quantaxis
        Err(e) => Ok(super::models::exchange_error_response("Failed to get instruments", &e)),
    }
}

/// 获取 Tick 数据（实时行情）
///
/// GET /api/market/tick/{instrument_id}
pub async fn get_tick(
    instrument_id: web::Path<String>,
    market_service: web::Data<MarketDataService>,
) -> Result<HttpResponse> {
    log::info!("🔍 [HTTP API] GET /api/market/tick/{}", instrument_id);

    match market_service.get_tick_data(&instrument_id) {
        Ok(tick) => {
            log::info!(
                "✅ [HTTP API] Tick data found for {}: price={}, ts={}",
                instrument_id,
                tick.last_price,
                tick.timestamp
            );
            Ok(HttpResponse::Ok().json(ApiResponse::success(tick)))
        }
        // 统一错误映射 —— 「合约不存在」应是 404 而非 500。
        // 映射表与理由见 `models.rs::exchange_error_response`。@yutiansut @quantaxis
        Err(e) => Ok(super::models::exchange_error_response("Failed to get tick", &e)),
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
        // 统一错误映射 —— 「合约不存在」应是 404 而非 500。
        // 映射表与理由见 `models.rs::exchange_error_response`。@yutiansut @quantaxis
        Err(e) => Ok(super::models::exchange_error_response("Failed to get trades", &e)),
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
        // 统一错误映射 —— 「合约不存在」应是 404 而非 500。
        // 映射表与理由见 `models.rs::exchange_error_response`。@yutiansut @quantaxis
        Err(e) => Ok(super::models::exchange_error_response("Failed to get stats", &e)),
    }
}
