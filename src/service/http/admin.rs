//! 管理端 HTTP API 处理器
//!
//! 提供合约管理、风控监控、结算管理等管理员功能的 HTTP API

use actix_web::{web, HttpResponse};
use log;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::exchange::instrument_registry::{InstrumentInfo, InstrumentStatus, InstrumentType};
use crate::exchange::{AccountManager, InstrumentRegistry, SettlementEngine};
use crate::market::MarketDataService;
use crate::matching::engine::ExchangeMatchingEngine;
use crate::ExchangeError;

// ============================================================================
// 管理端应用状态
// ============================================================================

pub struct AdminAppState {
    pub instrument_registry: Arc<InstrumentRegistry>,
    pub settlement_engine: Arc<SettlementEngine>,
    pub account_mgr: Arc<AccountManager>,
    /// 撮合引擎 —— 新合约必须在这里建订单簿，否则不可交易 @yutiansut @quantaxis
    pub matching_engine: Arc<ExchangeMatchingEngine>,
    /// 行情服务 —— 新合约必须设置昨收盘价，否则涨跌幅算不出来 @yutiansut @quantaxis
    pub market_data_service: Arc<MarketDataService>,
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
    /// 上市初始价（= 订单簿基准价 / 初始结算价 / 昨收盘价）@yutiansut @quantaxis
    /// 省略时取 100.0，与 main.rs 启动路径的兜底值一致。
    pub init_price: Option<f64>,
}

// 合约更新请求
#[derive(Debug, Deserialize)]
pub struct UpdateInstrumentRequest {
    // ⚠️ 本结构此前只有 7 个字段,而 `CreateInstrumentRequest` 有 12 个 ——
    // 少了 instrument_type / exchange / list_date / expire_date。
    // 前端 `admin/instruments.vue:361` 是 `updateInstrument(id, this.form)`
    // **整表提交**,而后端没有 `deny_unknown_fields`,于是这 4 个字段
    // **静默丢弃**:管理员改完上市/到期日期点保存,弹「合约更新成功」,值没变。
    // 4 个字段在 `InstrumentInfo` 里都存在(列表页也在显示),只是 update 不设它们。
    // 又一处「修了一半」—— create 想到了,update 忘了。@yutiansut @quantaxis
    pub instrument_name: Option<String>,
    pub instrument_type: Option<InstrumentType>,
    pub exchange: Option<String>,
    pub list_date: Option<String>,
    pub expire_date: Option<String>,
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

    // 4 步注册，缺一不可 @yutiansut @quantaxis
    //
    // 原实现只做了第 1 步（写注册表），结果新合约是「幽灵合约」：
    //   /api/admin/instruments        → 看得见
    //   /api/market/instruments       → 看不见（该接口读的是撮合引擎的 orderbooks）
    //   /api/market/orderbook/{id}    → 500 Instrument not found
    //   /api/order/submit             → 失败
    // main.rs 启动路径（instruments 初始化循环）做了全部 4 步，此处对齐它。
    let init_price = req.init_price.unwrap_or(100.0);

    // 1. 合约注册表（同时负责重复上市的校验）
    if let Err(e) = state.instrument_registry.register(instrument) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e.to_string())));
    }

    // 2. 撮合引擎建订单簿 —— 没有这步合约就不可交易
    if let Err(e) = state
        .matching_engine
        .register_instrument(req.instrument_id.clone(), init_price)
    {
        // 回滚注册表，避免留下一个不可交易的幽灵合约
        let _ = state.instrument_registry.delist(&req.instrument_id);
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
            "Registered in registry but failed to create orderbook (rolled back): {}",
            e
        ))));
    }

    // 3. 初始结算价
    state
        .settlement_engine
        .set_settlement_price(req.instrument_id.clone(), init_price);

    // 4. 昨收盘价（快照生成器算涨跌幅用）
    state
        .market_data_service
        .set_pre_close(&req.instrument_id, init_price);

    log::info!(
        "Instrument {} listed @ {} (orderbook + settlement price + pre_close wired)",
        req.instrument_id,
        init_price
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(())))
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
        // 下面 4 项是本次补齐的(见 UpdateInstrumentRequest 注释)
        if let Some(t) = req.instrument_type {
            info.instrument_type = t;
        }
        if let Some(ex) = &req.exchange {
            info.exchange = ex.clone();
        }
        if let Some(d) = &req.list_date {
            info.list_date = Some(d.clone());
        }
        if let Some(d) = &req.expire_date {
            info.expire_date = Some(d.clone());
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
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(error_msg)));
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
    log::info!(
        "POST /api/admin/settlement/batch-set-prices: {} prices",
        req.prices.len()
    );

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

    // ⚠️ 必须走完整日终(闭市 → 撤活单 → 归档 → 结算 → 开市),不能直接 `daily_settlement()`:
    // 后者清掉账户当日委托而挂单仍在撮合簿,之后一撤单/成交 qars 就 panic,panic=abort
    // 整个进程退出(2026-11-08 隔离实例实测)。日终是阻塞调用 → web::block。
    // 手动触发结算的是今天(北京时间)。响应仍以 SettlementResult 字段为顶层(flatten),
    // 另带 sweep / archived_snapshots 等日终明细。@yutiansut @quantaxis
    let engine = state.settlement_engine.clone();
    let bj = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 offset");
    let trading_day = chrono::Utc::now().with_timezone(&bj).format("%Y-%m-%d").to_string();
    match web::block(move || engine.run_end_of_day(&trading_day)).await {
        Ok(Ok(report)) => Ok(HttpResponse::Ok().json(ApiResponse::success(report))),
        Ok(Err(e)) => {
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string())))
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("日终任务异常: {}", e)))),
    }
}

/// 获取结算历史
/// 结算历史的日期区间参数
///
/// ⚠️ 前端 `admin/settlement.vue:387-389` **一直在发** `start_date`/`end_date`,
/// 而此前这个 handler 连 `web::Query` 提取器都没有 —— 参数被静默丢弃,
/// 无论选什么区间都返回全量。@yutiansut @quantaxis
#[derive(Debug, serde::Deserialize)]
pub struct SettlementHistoryQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

pub async fn get_settlement_history(
    query: web::Query<SettlementHistoryQuery>,
    state: web::Data<AdminAppState>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!(
        "GET /api/admin/settlement/history start={:?} end={:?}",
        query.start_date,
        query.end_date
    );

    let mut history = state.settlement_engine.get_settlement_history();

    // settlement_date 是 `YYYY-MM-DD` 定长字符串,字典序即时间序,可直接比较
    if let Some(start) = query.start_date.as_deref().filter(|s| !s.is_empty()) {
        history.retain(|r| r.settlement_date.as_str() >= start);
    }
    if let Some(end) = query.end_date.as_deref().filter(|s| !s.is_empty()) {
        history.retain(|r| r.settlement_date.as_str() <= end);
    }

    // ⚠️ 来源是 DashMap 迭代,顺序不定 —— 历史视图必须自己排序,否则每次刷新顺序都变
    history.sort_by(|a, b| b.settlement_date.cmp(&a.settlement_date));

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
        None => Ok(HttpResponse::NotFound()
            .json(ApiResponse::<()>::error("Settlement not found".to_string()))),
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
