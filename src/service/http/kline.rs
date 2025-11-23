//! K线数据 HTTP API
//!
//! 提供K线数据查询接口
//!
//! @yutiansut @quantaxis

use actix::Addr;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::market::{kline, GetKLines, KLineActor};

/// K线查询参数
#[derive(Debug, Deserialize)]
pub struct KLineQuery {
    /// K线周期 (0=日线, 4=1分钟, 5=5分钟, 6=15分钟, 7=30分钟, 8=60分钟)
    pub period: Option<i32>,

    /// 数据条数（默认500）
    pub count: Option<usize>,
}

/// K线响应数据
#[derive(Debug, Serialize)]
pub struct KLineResponse {
    pub code: i32,
    pub message: String,
    pub data: Option<KLineData>,
}

#[derive(Debug, Serialize)]
pub struct KLineData {
    pub symbol: String,
    pub period: i32,
    pub klines: Vec<KLineItem>,
}

#[derive(Debug, Serialize)]
pub struct KLineItem {
    /// 时间戳（毫秒）
    pub datetime: i64,

    /// OHLC
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,

    /// 成交量
    pub volume: i64,

    /// 成交额
    pub amount: f64,
}

/// 获取K线数据
///
/// GET /api/market/kline/{instrument_id}?period=5&count=500
pub async fn get_kline_data(
    path: web::Path<String>,
    query: web::Query<KLineQuery>,
    kline_actor: web::Data<Addr<KLineActor>>,
) -> Result<HttpResponse> {
    let instrument_id = path.into_inner();
    let period_int = query.period.unwrap_or(5); // 默认5分钟
    let count = query.count.unwrap_or(500); // 默认500根

    // 转换周期
    let period = match kline::KLinePeriod::from_int(period_int) {
        Some(p) => p,
        None => {
            return Ok(HttpResponse::BadRequest().json(KLineResponse {
                code: 400,
                message: format!("Invalid period: {}", period_int),
                data: None,
            }));
        }
    };

    // 异步查询K线数据（通过KLineActor）
    let klines = match kline_actor
        .send(GetKLines {
            instrument_id: instrument_id.clone(),
            period,
            count,
        })
        .await
    {
        Ok(klines) => klines,
        Err(e) => {
            log::error!("Failed to query K-line data: {}", e);
            return Ok(HttpResponse::InternalServerError().json(KLineResponse {
                code: 500,
                message: format!("Failed to query K-line data: {}", e),
                data: None,
            }));
        }
    };

    // 转换为响应格式
    let kline_items: Vec<KLineItem> = klines
        .into_iter()
        .map(|k| KLineItem {
            datetime: k.timestamp,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            amount: k.amount,
        })
        .collect();

    log::info!(
        "📊 [KLine API] {} period={:?} count={} -> {} bars",
        instrument_id,
        period,
        count,
        kline_items.len()
    );

    Ok(HttpResponse::Ok().json(KLineResponse {
        code: 0,
        message: "success".to_string(),
        data: Some(KLineData {
            symbol: instrument_id,
            period: period_int,
            klines: kline_items,
        }),
    }))
}

// K线路由将在 routes.rs 中直接集成到 /api/market scope

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kline_query_defaults() {
        let query = KLineQuery {
            period: None,
            count: None,
        };

        assert_eq!(query.period.unwrap_or(5), 5);
        assert_eq!(query.count.unwrap_or(500), 500);
    }
}
