//! HTTP API 请求处理器

use actix_web::{web, HttpResponse, Result};
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
use chrono::{DateTime, Utc};
use log;
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::models::{
    ApiResponse, AccountInfo, OpenAccountRequest, SubmitOrderRequest, SubmitOrderResponse,
    CancelOrderRequest, OrderInfo, PositionInfo, DepositRequest, WithdrawRequest, CreateAccountRequest,
    // Phase 11: 批量下单/条件单/订单修改 @yutiansut @quantaxis
    BatchOrderRequest, BatchOrderResponse, SingleOrderResult,
    BatchCancelRequest, BatchCancelResponse,
    ModifyOrderRequest, CreateConditionalOrderRequest,
    // Phase 14: 入金流水记录 @yutiansut @quantaxis
    TransferRecord,
};
use crate::core::account_ext::{AccountType, OpenAccountRequest as CoreOpenAccountRequest};
use crate::exchange::order_router::{
    CancelOrderRequest as CoreCancelOrderRequest, SubmitOrderRequest as CoreSubmitOrderRequest,
};
use crate::exchange::settlement::AccountSettlement;
use crate::exchange::{AccountManager, OrderRouter, SettlementEngine};
use crate::matching::trade_recorder::{TradeRecord, TradeRecorder};
use crate::protocol::diff::snapshot::SnapshotManager;
use crate::storage::conversion::ConversionManager;
use crate::storage::subscriber::SubscriberStats;
use crate::user::UserManager;

/// 应用状态
pub struct AppState {
    pub order_router: Arc<OrderRouter>,
    pub account_mgr: Arc<AccountManager>,
    pub settlement_engine: Arc<SettlementEngine>,
    pub trade_recorder: Arc<TradeRecorder>,
    pub user_mgr: Arc<UserManager>,
    pub storage_stats: Option<Arc<parking_lot::Mutex<SubscriberStats>>>,
    pub conversion_mgr: Option<Arc<parking_lot::Mutex<ConversionManager>>>,
    /// 市场数据存储（WAL+MemTable+SSTable）用于历史Tick查询 @yutiansut @quantaxis
    pub market_data_storage: Option<Arc<crate::storage::hybrid::OltpHybridStorage>>,
    /// K线WAL管理器 用于历史K线查询 @yutiansut @quantaxis
    pub kline_wal_manager: Option<Arc<crate::storage::wal::WalManager>>,
    /// 服务器启动时间 用于计算运行时间 @yutiansut @quantaxis
    pub server_start_time: DateTime<Utc>,
    /// WebSocket 连接数计数器 @yutiansut @quantaxis
    pub ws_connection_count: Arc<AtomicUsize>,
    /// SnapshotManager 用于广播公告到所有连接的用户 @yutiansut @quantaxis
    pub snapshot_mgr: Option<Arc<SnapshotManager>>,
}

/// 用户成交视图 - 包含用户方向信息
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize)]
pub struct UserTradeView {
    pub trade_id: String,
    pub instrument_id: String,
    /// 用户在这笔成交中的方向（BUY/SELL）
    pub user_direction: String,
    /// 用户的订单ID
    pub user_order_id: String,
    /// 对手方订单ID
    pub opposite_order_id: String,
    /// 是否为主动方（taker）。
    ///
    /// ⚠️ `Option` 而**不是** `bool` —— 从账户 QIFI 的 `dailytrades` 构造时
    /// 这个信息**不存在**(QIFI Trade 规范里没有 taker 字段,它是撮合实现细节,
    /// 不该塞进规范结构)。用 `false` 填会让前端把每一笔都显示成「被动成交」,
    /// 那是**编造**,与本轮删掉的绿色对勾、假趋势徽章同类。
    /// 前端须按三态渲染:null → `--`。@yutiansut @quantaxis
    pub is_taker: Option<bool>,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub trading_day: String,
    /// 本笔成交手续费(来自 qars preset.calc_commission)
    pub commission: f64,
}

impl UserTradeView {
    /// 从 TradeRecord 创建用户视图
    /// account_id: 查询的账户ID，用于判断用户方向
    pub fn from_trade_record(record: &TradeRecord, account_id: &str) -> Self {
        // 判断用户是买方还是卖方
        let is_buyer = record.buy_user_id == account_id;
        let user_direction = if is_buyer { "BUY" } else { "SELL" }.to_string();

        let (user_order_id, opposite_order_id) = if is_buyer {
            (record.buy_order_id.clone(), record.sell_order_id.clone())
        } else {
            (record.sell_order_id.clone(), record.buy_order_id.clone())
        };

        // 判断是否为主动方：taker_order_id 等于用户的订单ID则为主动方
        let is_taker = Some(record.taker_order_id == user_order_id);

        Self {
            trade_id: record.trade_id.clone(),
            instrument_id: record.instrument_id.clone(),
            user_direction,
            user_order_id,
            opposite_order_id,
            is_taker,
            price: record.price,
            volume: record.volume,
            timestamp: record.timestamp,
            trading_day: record.trading_day.clone(),
            commission: record.commission,
        }
    }

    /// 从账户 QIFI 的 `dailytrades` 条目创建用户视图。@yutiansut @quantaxis
    ///
    /// 为什么需要这一个:`TradeRecorder` 是**执行态**组件,`new()` 建空表、
    /// 全仓无 restore 入口,**每次重启归零**。而账户 QIFI 里有完整历史且
    /// 快照恢复已验证可用 —— 实测 78 个账户 `/order/user/` 合计 668 条,
    /// 而 `/management/…/detail` 的 orders_total 合计 410,371(可见 0.16%),
    /// 成交更是 73/78 个账户返回 0。
    ///
    /// QIFI Trade 反而比 TradeRecord **更适合**做用户视图:
    ///   · `direction` 就是**该账户自己**的方向,不需要比对 buy/sell_user_id 推导
    ///     (实测 151,695 个 trade_id **各自唯一、无一出现在两个账户上**,
    ///      即买卖双方各记一条独立记录);
    ///   · `commission` 是真值且**全量非零**(151,695/151,695,来自 qars
    ///     `preset.calc_commission`);TradeRecord 本身没有这个字段。
    /// 唯一缺的是 `is_taker` —— 见该字段上的说明,如实给 `None`。
    /// 从账户 QIFI 的 `dailytrades` 条目创建用户视图。
    ///
    /// `trading_day` 由调用方用 [`TradingDayResolver`] 按**这笔成交自己的时间戳**算出,
    /// 不能传账户当前的 `qifi.trading_day` —— 见该 struct 的说明。
    pub fn from_qifi_trade(
        t: &qars::qaprotocol::qifi::account::Trade,
        trading_day: &str,
    ) -> Self {
        Self {
            trade_id: t.trade_id.clone(),
            instrument_id: t.instrument_id.clone(),
            user_direction: t.direction.clone(),
            user_order_id: t.order_id.clone(),
            // QIFI 记录的是本方视角,不含对手方订单号
            opposite_order_id: String::new(),
            is_taker: None,
            price: t.price,
            volume: t.volume,
            timestamp: t.trade_date_time,
            trading_day: trading_day.to_string(),
            commission: t.commission,
        }
    }
}

/// 按**每笔成交自己的时间戳**解析交易日。@yutiansut @quantaxis
///
/// ⚠️ 原实现把账户当前的 `qifi.trading_day` 原样贴到**每一笔**成交上。
/// 本系统从不结算,`dailytrades` 装的是全量历史,于是:
/// 实测(2026-09-06 快照)151,699 笔成交**实际发生在 2026-09-04 / 09-05**,
/// 却无一例外被标成 `20260907`(账户当前交易日)——
/// **22 个有成交的账户全部标签与实际不符**。
/// 成交自己的时间在 `trade_date_time` 里且是对的,坏的只是这个派生字段。
///
/// 复用 qars 的交易日历 `QATradeDate::get_trade_day`,它同时处理:
///   · 非交易日顺延到下一个交易日(周末、节假日)
///   · **夜盘规则**:16:00 之后的成交计入下一交易日
/// 自己写日期加减会漏掉这两条(见 tasks/lessons.md 关于「自算交易日」的记录)。
///
/// **按 (日期, 是否夜盘) 缓存** —— 一次请求里通常只有个位数个不同取值,
/// 而 `QATradeDate::new()` 要建一个 6544 元素的表,不能每笔成交都建。
///
/// **`catch_unwind` 是必需的**:`get_next_day` 有两处 `.unwrap()`,
/// 日期越出硬编码日历表(实测覆盖到 `20261231`)即 panic。这里 panic 不该
/// 打掉整个查询 —— 退回「该笔成交自己的自然日」,那仍然远好于「账户当前交易日」。
/// (根因与剩余天数见 `main.rs` 的 `check_trading_calendar_horizon`。)
pub struct TradingDayResolver {
    cache: std::collections::HashMap<(String, bool), String>,
}

impl TradingDayResolver {
    pub fn new() -> Self {
        Self { cache: std::collections::HashMap::new() }
    }

    /// `ts` 是 `trade_date_time`。兼容秒/毫秒/微秒/纳秒
    /// —— 历史数据里同一字段出现过多种单位(与 `OrderPageQuery::match_date` 同一套判据)。
    pub fn resolve(&mut self, ts: i64) -> Option<String> {
        use chrono::TimeZone;
        let secs = if ts.abs() > 100_000_000_000_000_000 { ts / 1_000_000_000 }
            else if ts.abs() > 100_000_000_000_000 { ts / 1_000_000 }
            else if ts.abs() > 100_000_000_000 { ts / 1_000 }
            else { ts };
        let dt = chrono::Local.timestamp_opt(secs, 0).single()?;
        let date = dt.format("%Y-%m-%d").to_string();
        let night = dt.format("%H").to_string().parse::<u32>().ok()? >= 16;
        let key = (date.clone(), night);
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let stamp = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let resolved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut cal = qars::qautil::tradedate::QATradeDate::new();
            cal.get_trade_day(stamp)
        }))
        .ok()
        .map(|d| d.chars().filter(|c| *c != '-').collect::<String>())
        // 日历越界时退回成交自己的自然日 —— 仍远好于「账户当前交易日」
        .unwrap_or_else(|| date.chars().filter(|c| *c != '-').collect::<String>());
        self.cache.insert(key, resolved.clone());
        Some(resolved)
    }
}

impl Default for TradingDayResolver {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod trading_day_resolver_tests {
    use super::*;
    use chrono::TimeZone;

    /// 把本地时间转成纳秒时间戳(与线上 `trade_date_time` 同单位)
    fn ns(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
        chrono::Local
            .with_ymd_and_hms(y, m, d, h, mi, 0)
            .single()
            .unwrap()
            .timestamp()
            * 1_000_000_000
    }

    /// ✨ 用**线上实测值**做基准:2026-09-04 是周五,日盘成交应当就是当天。
    #[test]
    fn day_session_stays_on_its_own_date() {
        let mut r = TradingDayResolver::new();
        assert_eq!(r.resolve(ns(2026, 9, 4, 14, 30)).unwrap(), "20260904");
    }

    /// 夜盘规则:16:00 之后计入**下一个**交易日。周五夜盘 → 下周一。
    /// (这一条正是自己写日期加减会漏掉的。)
    #[test]
    fn night_session_rolls_to_next_trading_day() {
        let mut r = TradingDayResolver::new();
        assert_eq!(r.resolve(ns(2026, 9, 4, 21, 0)).unwrap(), "20260907");
    }

    /// 周末顺延到下一个交易日 —— 与线上 79 份快照里的 `20260907` 一致。
    #[test]
    fn weekend_rolls_forward() {
        let mut r = TradingDayResolver::new();
        assert_eq!(r.resolve(ns(2026, 9, 6, 9, 12)).unwrap(), "20260907");
    }

    /// ✨ 这才是本次修复的核心:**同一批成交跨两天,必须解析出两个不同的交易日**。
    /// 原实现给每一笔贴账户当前交易日,这条会失败。
    #[test]
    fn distinct_timestamps_yield_distinct_days() {
        let mut r = TradingDayResolver::new();
        let fri = r.resolve(ns(2026, 9, 4, 10, 0)).unwrap();
        let thu = r.resolve(ns(2026, 9, 3, 10, 0)).unwrap();
        assert_ne!(fri, thu, "不同日期的成交必须得到不同的交易日");
        assert_eq!((fri.as_str(), thu.as_str()), ("20260904", "20260903"));
    }

    /// 缓存不能改变结果 —— 键是 (日期, 是否夜盘),同日日盘/夜盘必须仍然分开。
    #[test]
    fn cache_keeps_day_and_night_separate() {
        let mut r = TradingDayResolver::new();
        let day = r.resolve(ns(2026, 9, 4, 10, 0)).unwrap();
        let night = r.resolve(ns(2026, 9, 4, 21, 0)).unwrap();
        // 再取一遍,走缓存
        assert_eq!(r.resolve(ns(2026, 9, 4, 11, 0)).unwrap(), day);
        assert_eq!(r.resolve(ns(2026, 9, 4, 22, 0)).unwrap(), night);
        assert_ne!(day, night);
    }

    /// 单位自适应:秒 / 毫秒 / 微秒 / 纳秒 应当得到同一个交易日。
    #[test]
    fn accepts_multiple_timestamp_units() {
        let mut r = TradingDayResolver::new();
        let n = ns(2026, 9, 4, 14, 30);
        let expect = "20260904";
        assert_eq!(r.resolve(n).unwrap(), expect, "纳秒");
        assert_eq!(r.resolve(n / 1_000).unwrap(), expect, "微秒");
        assert_eq!(r.resolve(n / 1_000_000).unwrap(), expect, "毫秒");
        assert_eq!(r.resolve(n / 1_000_000_000).unwrap(), expect, "秒");
    }

    /// 日历越界不能打掉查询 —— 退回成交自己的自然日。
    /// 2027 年已超出 qars 硬编码表(覆盖到 20261231)。
    #[test]
    fn out_of_calendar_falls_back_to_own_date() {
        let mut r = TradingDayResolver::new();
        let got = r.resolve(ns(2027, 3, 1, 10, 0));
        assert_eq!(got.unwrap(), "20270301", "越界应退回自然日而不是 panic");
    }
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

    // ⚠️ 开户资金必须校验 —— 与入金走同一套判据。@yutiansut @quantaxis
    //
    // 实测(2026-09-05,回验代理发现):`init_cash` **完全不校验**,
    // `1e300` 与**负数**都返回 HTTP 200 开户成功。
    // 而入金/出金/转账早已统一走 `validate_money_amount`
    // (`models.rs:120`,挡 NaN/Inf、非正数、超单笔上限)——
    // **同一个坑修了那边没修这边**,开户恰恰是资金进入系统的第一道门。
    //
    // 与入金的唯一差异:**允许 0** —— 注册时自动开的默认账户就是
    // `init_cash: 0.0`(auth.rs:29),那是合法的"空账户"。
    if req.init_cash != 0.0 {
        if let Err(e) = super::models::validate_money_amount(req.init_cash) {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error(400, format!("初始资金不合法:{}", e))));
        }
    } else if !req.init_cash.is_finite() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(400, "初始资金必须是有限数值".to_string())));
    }

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
            // ✨ 把开户密码写进校验表 —— 否则用户拿自己设的密码改密码会恒定 400,
            //    银期转账的资金密码也只认硬编码的 "123456"。
            //    见 account_admin::seed_account_password 的说明。@yutiansut @quantaxis
            super::account_admin::seed_account_password(&account_id, &req.password);
            Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({ "account_id": account_id }),
            )))
        }
        Err(e) => {
            log::error!("Failed to open account: {:?}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    500,
                    format!("Failed to open account: {}", e),
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
            // ✨ 使用 write() 获取可变引用，以便调用 get_margin() 动态计算 @yutiansut @quantaxis
            let mut acc = account.write();
            let frozen = acc.get_balance() - acc.money;   // 同上:不可读缓存字段

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

            // ✨ 动态计算保证金 = 持仓保证金 + 挂单冻结保证金 @yutiansut @quantaxis
            let position_margin = acc.get_margin();
            let frozen_margin = acc.get_frozen_margin();
            let margin = position_margin + frozen_margin;

            // ✨ 降为 debug + 内联(原热路径 info! 每次查询刷屏,frozen_keys 每次克隆全量)。@yutiansut @quantaxis
            log::debug!(
                "query_account: account_id={}, position_margin={:.2}, frozen_margin={:.2}, frozen_count={}, frozen_keys={:?}, money={:.2}",
                account_id.as_str(),
                position_margin,
                frozen_margin,
                acc.frozen.len(),
                acc.frozen.keys().cloned().collect::<Vec<String>>(),
                acc.money
            );

            let info = AccountInfo {
                user_id: acc.account_cookie.clone(),
                user_name: account_name,
                // ⚠️ 必须用**动态 getter**,不能读 `acc.accounts.*` 的缓存字段。
                // QIFI Account 里的 balance / risk_ratio / close_profit 是派生量,
                // 只有调用对应 getter 时才重算;直接读字段拿到的是初始值 ——
                // 实测 NOISE_SELL 交易 6.8 万笔后本接口仍返回
                // balance=1,500,000,000(= init_cash)、risk_ratio=0.0,
                // 而同一账户在 /api/management/accounts 是 1,683,618,586 / 0.5113。
                // 本函数上面已经为 margin 特意取了 write 锁做动态计算
                // (见 :163 注释),balance/risk_ratio/profit 当时漏了。
                // 口径与 management.rs:110 保持一致。@yutiansut @quantaxis
                balance: acc.get_balance(),
                available: acc.money,
                frozen,
                margin,
                profit: acc.get_closeprofit(),
                risk_ratio: acc.safe_risk_ratio(),
                account_type: format!("{:?}", account_type).to_lowercase(),
                created_at,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(info)))
        }
        Err(e) => {
            log::warn!("Failed to query account: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("{}", e),   // context 不复述错误本身 @yutiansut @quantaxis
            )))
        }
    }
}

/// 提交订单
pub async fn submit_order(
    req: web::Json<SubmitOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // ✨ Debug: 打印接收到的请求 @yutiansut @quantaxis
    log::debug!(
        "📥 HTTP submit_order: user_id={}, account_id={:?}, instrument={}",
        req.user_id,
        req.account_id,
        req.instrument_id
    );

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
                // ⚠️ 默认账户余额为 0 时,必须**直说是没传 account_id**,
                //    不能让它继续走下去然后报「资金不足」。@yutiansut @quantaxis
                //
                // 注册时会自动建一个 `init_cash = 0` 的默认账户。省略 account_id 时
                // fallback 到它 ⇒ 风控报 `Insufficient funds: available=0.00,
                // required=84971.71` —— 用户看到的是「资金不足」,真实原因却是
                // 「没指定账户」,极具误导性。实测这正是「接口报 400」里最难懂的一条。
                // (同一个坑在压测脚本 `sim/qaex.py:81` 也用大写警告标注过。)
                if acc.money <= 0.0 && acc.accounts.balance <= 0.0 {
                    let cookie = acc.account_cookie.clone();
                    drop(acc);
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                        4006,
                        format!(
                            "请求未指定 account_id，已回退到默认账户 {} 而它的余额为 0。请显式传入要交易的 account_id（可从 GET /api/user/{}/accounts 获取）。",
                            cookie, req.user_id
                        ),
                    )));
                }
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
        time_condition: None,
        volume_condition: None,
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
                // ⚠️ 默认账户余额为 0 时,必须**直说是没传 account_id**,
                //    不能让它继续走下去然后报「资金不足」。@yutiansut @quantaxis
                //
                // 注册时会自动建一个 `init_cash = 0` 的默认账户。省略 account_id 时
                // fallback 到它 ⇒ 风控报 `Insufficient funds: available=0.00,
                // required=84971.71` —— 用户看到的是「资金不足」,真实原因却是
                // 「没指定账户」,极具误导性。实测这正是「接口报 400」里最难懂的一条。
                // (同一个坑在压测脚本 `sim/qaex.py:81` 也用大写警告标注过。)
                if acc.money <= 0.0 && acc.accounts.balance <= 0.0 {
                    let cookie = acc.account_cookie.clone();
                    drop(acc);
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                        4006,
                        format!(
                            "请求未指定 account_id，已回退到默认账户 {} 而它的余额为 0。请显式传入要交易的 account_id（可从 GET /api/user/{}/accounts 获取）。",
                            cookie, req.user_id
                        ),
                    )));
                }
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
            log::warn!("Failed to cancel order: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!("Cancel order failed: {}", e),
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
            log::warn!("Order not found: {}", order_id);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("Order not found: {}", order_id),
            )))
        }
    }
}

/// 查询用户订单列表
/// 分页参数(可选) —— 不传则保持原有的「全量返回」行为,向后兼容 @yutiansut @quantaxis
#[derive(Debug, serde::Deserialize)]
pub struct OrderPageQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    /// 按合约筛选(空串或缺省=不筛)
    ///
    /// ⚠️ 订单页(`orders/index.vue:23`)与成交页(`trades/index.vue:13`)都有
    /// 「合约」下拉,但前端从不发、后端也没有这个字段 —— 用户选了没反应。
    /// @yutiansut @quantaxis
    pub instrument_id: Option<String>,
    /// 起始日期 `YYYY-MM-DD`(含),按订单/成交时间过滤
    pub start_date: Option<String>,
    /// 结束日期 `YYYY-MM-DD`(含)
    pub end_date: Option<String>,
}

impl OrderPageQuery {
    /// 合约是否匹配(空筛选一律通过)
    pub fn match_instrument(&self, instrument_id: &str) -> bool {
        match self.instrument_id.as_deref().map(str::trim) {
            Some(f) if !f.is_empty() => f.eq_ignore_ascii_case(instrument_id),
            _ => true,
        }
    }

    /// 纳秒时间戳是否落在 [start_date, end_date] 内(闭区间,按本地日历日)
    ///
    /// 交易所内部时间戳是纳秒。这里只做日期级比较,不引入交易日历 ——
    /// 交易日口径统一是另一件事(见 tasks/lessons.md 与 memory)。
    pub fn match_date(&self, ts_nanos: i64) -> bool {
        let has_start = self.start_date.as_deref().map_or(false, |s| !s.is_empty());
        let has_end = self.end_date.as_deref().map_or(false, |s| !s.is_empty());
        if !has_start && !has_end {
            return true;
        }
        // 按数量级识别单位 —— 历史数据里同一字段出现过秒与纳秒两种
        // (根因已修:order_router 恢复路径曾 `/1_000_000_000`;这里保持容错,
        //  以免任何遗留或第三方来源的时间戳把筛选算成 1970 年)。
        // 当前纪元:秒 ~1.8e9 / 毫秒 ~1.8e12 / 微秒 ~1.8e15 / 纳秒 ~1.8e18
        let secs = if ts_nanos.abs() > 100_000_000_000_000_000 {
            ts_nanos / 1_000_000_000 // 纳秒
        } else if ts_nanos.abs() > 100_000_000_000_000 {
            ts_nanos / 1_000_000 // 微秒
        } else if ts_nanos.abs() > 100_000_000_000 {
            ts_nanos / 1_000 // 毫秒
        } else {
            ts_nanos // 秒
        };
        let day = match chrono::DateTime::from_timestamp(secs, 0) {
            Some(dt) => dt
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string(),
            None => return true, // 时间戳异常时不因筛选而丢数据
        };
        if let Some(start) = self.start_date.as_deref().filter(|s| !s.is_empty()) {
            if day.as_str() < start {
                return false;
            }
        }
        if let Some(end) = self.end_date.as_deref().filter(|s| !s.is_empty()) {
            if day.as_str() > end {
                return false;
            }
        }
        true
    }
}

pub async fn query_user_orders(
    user_id: web::Path<String>,
    query: web::Query<OrderPageQuery>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let order_details = state.order_router.get_user_order_details(&user_id);

    // 「用户不存在」必须与「该用户没有记录」可区分 @yutiansut @quantaxis
    //
    // 实测(2026-09-05)对同一个不存在的 user_id:
    //   GET /api/position/user/{x} → 404 (query_positions_by_user:868 有检查)
    //   GET /api/trades/user/{x}   → 200 空表   
    //   GET /api/order/user/{x}    → 200 空表   ← 本端点
    // 相邻端点行为相反,前端无从区分,表格一律显示「暂无数据」。
    //
    // ⚠️ 我先前只修了 `/api/trades/account/{id}` —— 而 `trades/index.vue:220`
    //    调的是 `queryUserTrades` → **本端点**;`queryAccountTrades` 全仓零调用。
    //    修了没人用的那个。改端点语义前先确认前端调的是哪一个。
    //
    // 沿用 query_positions_by_user 的判据,但**多一个条件**:仅当同时无任何记录时
    // 才 404,避免「账户已移除、历史记录仍在」时把数据藏起来。
    if order_details.is_empty()
        && state
            .account_mgr
            .get_accounts_by_user(&user_id)
            .is_empty()
    {
        return Ok(super::models::exchange_error_response(
            "查询用户数据",
            &crate::ExchangeError::NotFound(format!("用户不存在或名下没有账户:{}", user_id)),
        ));
    }

    // ✨ 支持分页 —— 原先无条件全量返回 @yutiansut @quantaxis
    //
    // 2026-09-04 实测:单账户 5,951 单时前端需 **18.4 秒**出表、
    // 产生 321,731 个 DOM 节点、905MB heap;两个活跃账户合计 11,902 行
    // → 39.9 秒 / 1.7GB heap。而压测中该账户 5 分钟就从 5,903 涨到 5,951 单,
    // 全所 389,772 单仍在增长 —— 不分页迟早把浏览器打死。
    //
    // 兼容性:`page`/`page_size` 都不传时行为完全不变(全量),
    // 所以现有调用方无需同步修改。
    //
    // ⚠️ `total` 是**筛选后、分页前**的总数(见下方 retain 之后的赋值)——
    // 分页器要按它算页数。加了 instrument_id/日期筛选之后,
    // 若仍用筛选前的总数,分页器会显示出根本不存在的页。

    // 保留每条执行态订单的 qa_order_id —— 它是 QIFI `orders` 的 key,
    // 跨源去重必须用它,否则同一笔单会显示成两条。见 UserOrderDetail 的说明。
    // @yutiansut @quantaxis
    let qa_ids: Vec<String> = order_details.iter().map(|d| d.qa_order_id.clone()).collect();

    let order_infos: Vec<OrderInfo> = order_details
        .into_iter()
        .map(|d| OrderInfo {
            order_id: d.order_id,
            user_id: d.order.user_id,
            instrument_id: d.order.instrument_id,
            direction: d.order.direction,
            offset: d.order.offset,
            volume: d.order.volume_orign,
            price: d.order.limit_price,
            filled_volume: d.filled_volume,
            status: format!("{:?}", d.status),
            submit_time: d.submit_time,
            update_time: d.update_time,
        })
        .collect();

    // ✨ 补上账户 QIFI 里的历史订单。@yutiansut @quantaxis
    //
    // `OrderRouter` 是**执行态**组件:`restore_orders_from_accounts`
    // (order_router.rs:2559,已接线)按设计**只恢复 SUBMITTED/ALIVE 活单** ——
    // 这对撮合是正确的,它要的就是活单。错的是**拿它当历史库查**。
    // 实测(2026-09-05):78 个账户经本端点合计 **668** 单,
    // 而 `/management/…/detail` 的 orders_total 合计 **410,371** —— 可见 **0.16%**。
    //
    // 账户 QIFI 的 `dailyorders` 有完整历史(且因为本系统**从不结算**,
    // 这个「日」字辈容器里装的其实是全部历史 —— 见 DECISIONS-PENDING「从不结算」一节),
    // 快照恢复已验证可用。`management.rs:192` 早就是这么读的,此处**复用同一读法**。
    //
    // 合并而非替换:OrderRouter 里有本次运行期的**最新状态**(含 filled_volume
    // 与实时 status),QIFI 里有全部历史。按 order_id 去重,**执行态优先**。
    let mut order_infos = order_infos;
    {
        // ⚠️ 种子必须同时放**两个 ID** —— 同一笔订单在 OrderRouter 与 QIFI 里
        // ID 不同(`O1788…` vs UUID),只放 order_id 去重会漏,实测下 1 单显示 2 行。
        // @yutiansut @quantaxis
        let mut seen: std::collections::HashSet<String> = order_infos
            .iter()
            .map(|o| o.order_id.clone())
            .chain(qa_ids.iter().filter(|s| !s.is_empty()).cloned())
            .collect();
        for account in state.account_mgr.get_accounts_by_user(&user_id) {
            let account_id = {
                let acc = account.read();
                acc.account_cookie.clone()
            }; // ← 读锁尽早释放:get_qifi_slice 会自己取锁
            if let Ok(qifi) = state.account_mgr.get_qifi_slice(&account_id) {
                for (oid, o) in qifi.orders.iter() {
                    let key = if o.order_id.is_empty() { oid.clone() } else { o.order_id.clone() };
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                    order_infos.push(OrderInfo {
                        order_id: key,
                        user_id: o.user_id.clone(),
                        instrument_id: o.instrument_id.clone(),
                        direction: o.direction.clone(),
                        offset: o.offset.clone(),
                        volume: o.volume_orign,
                        price: o.limit_price,
                        // QIFI 只给剩余量,已成交 = 原始 - 剩余
                        filled_volume: (o.volume_orign - o.volume_left).max(0.0),
                        status: o.status.clone(),
                        submit_time: o.insert_date_time,
                        update_time: o.insert_date_time,
                    });
                }
            }
        }
        // 时间倒序 —— HashMap 迭代顺序不定,不排序翻页会见到重复条目
        order_infos.sort_by(|a, b| b.submit_time.cmp(&a.submit_time));
    }

    // 分页切片(两个参数都给了才切,否则维持全量)
    // ✨ 先按合约/日期筛选,再分页 —— 顺序不能反 @yutiansut @quantaxis
    order_infos.retain(|o| query.match_instrument(&o.instrument_id) && query.match_date(o.submit_time));
    let total = order_infos.len(); // 筛选后的总数才是分页器该显示的

    let paged: Vec<OrderInfo> = match (query.page, query.page_size) {
        (Some(page), Some(size)) if size > 0 => {
            let start = page.saturating_sub(1).saturating_mul(size);
            order_infos.into_iter().skip(start).take(size).collect()
        }
        _ => order_infos,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "orders": paged,
            "total": total          // 过滤前总数,供前端算页数
        }))),
    )
}

/// 获取账户权益曲线
pub async fn get_equity_curve(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let user_id = user_id.into_inner();
    if user_id.is_empty() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(400, "Missing user_id".to_string())));
    }

    let accounts = state.account_mgr.get_accounts_by_user(&user_id);

    let mut account_responses = Vec::new();
    for account in accounts {
        // ✨ 获取账户真实数据 @yutiansut @quantaxis
        let (account_id, account_name, balance, available, margin) = {
            let mut acc = account.write();  // 需要 write() 因为 get_margin() 需要可变引用
            // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
            let position_margin = acc.get_margin();
            let frozen_margin = acc.get_frozen_margin();
            let total_margin = position_margin + frozen_margin;
            (
                acc.account_cookie.clone(),
                acc.user_cookie.clone(),
                // 同上:本处已持写锁(为 get_margin 取的),读缓存字段拿到的是
                // 开户初始值。@yutiansut @quantaxis
                acc.get_balance(),
                acc.money,
                total_margin,
            )
        };

        let settlements = state.settlement_engine.get_account_settlements(&account_id);
        let mut points = convert_settlements(settlements);

        // ✨ 无结算记录时使用当前账户真实数据（不再生成模拟数据）@yutiansut @quantaxis
        if points.is_empty() {
            log::info!(
                "📈 [Equity Curve] No settlements for account {}, using current real balance: {}",
                account_id,
                balance
            );
            // 使用当前真实数据作为今天的数据点
            // 权益曲线的「今天」要与用户所见的日期一致(交易所本地日历)。
            // @yutiansut @quantaxis
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            points.push(EquityCurvePoint {
                date: today,
                balance,
                available,
                margin,
                daily_profit: 0.0,      // 无历史数据，无法计算日盈亏
                daily_profit_rate: 0.0,
                trade_count: 0,
                commission: 0.0,
            });
        }

        let stats = compute_statistics(&points);

        account_responses.push(EquityCurveAccountResponse {
            account_id,
            account_name,
            points,
            statistics: stats,
        });
    }

    let response = EquityCurveResponse {
        user_id,
        accounts: account_responses,
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
}

#[derive(Debug, Clone, Serialize)]
struct EquityCurvePoint {
    date: String,
    balance: f64,
    available: f64,
    margin: f64,
    daily_profit: f64,
    daily_profit_rate: f64,
    trade_count: i32,
    commission: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct EquityStatistics {
    start_balance: f64,
    end_balance: f64,
    total_profit: f64,
    total_profit_rate: f64,
    max_drawdown: f64,
    max_drawdown_rate: f64,
    profit_days: usize,
    loss_days: usize,
    win_rate: f64,
    avg_daily_profit: f64,
    sharpe_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EquityCurveAccountResponse {
    account_id: String,
    account_name: String,
    points: Vec<EquityCurvePoint>,
    statistics: EquityStatistics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EquityCurveResponse {
    user_id: String,
    accounts: Vec<EquityCurveAccountResponse>,
}

fn convert_settlements(mut settlements: Vec<AccountSettlement>) -> Vec<EquityCurvePoint> {
    if settlements.is_empty() {
        return Vec::new();
    }

    settlements.sort_by(|a, b| a.date.cmp(&b.date));

    let mut points = Vec::with_capacity(settlements.len());
    let mut prev_balance: Option<f64> = None;

    for settlement in settlements {
        let previous = prev_balance.unwrap_or(settlement.balance - settlement.close_profit);
        let daily_profit = settlement.balance - previous;
        let daily_profit_rate = if previous.abs() > f64::EPSILON {
            daily_profit / previous
        } else {
            0.0
        };

        points.push(EquityCurvePoint {
            date: settlement.date,
            balance: settlement.balance,
            available: settlement.available,
            margin: settlement.margin,
            daily_profit,
            daily_profit_rate,
            trade_count: 0,
            commission: settlement.commission,
        });

        prev_balance = Some(settlement.balance);
    }

    points
}

fn compute_statistics(points: &[EquityCurvePoint]) -> EquityStatistics {
    if points.is_empty() {
        return EquityStatistics::default();
    }

    let start_balance = points.first().map(|p| p.balance).unwrap_or(0.0);
    let end_balance = points.last().map(|p| p.balance).unwrap_or(start_balance);
    let total_profit = end_balance - start_balance;
    let total_profit_rate = if start_balance.abs() > f64::EPSILON {
        total_profit / start_balance
    } else {
        0.0
    };

    let mut peak = start_balance;
    let mut max_drawdown: f64 = 0.0;
    let mut max_drawdown_rate: f64 = 0.0;
    let mut profit_days = 0;
    let mut loss_days = 0;
    let mut returns = Vec::new();

    for window in points.windows(2) {
        let prev = window[0].balance;
        let curr = window[1].balance;
        peak = peak.max(curr);
        let dd = peak - curr;
        max_drawdown = max_drawdown.max(dd);
        if peak > 0.0 {
            max_drawdown_rate = max_drawdown_rate.max(dd / peak);
        }

        let daily_profit = curr - prev;
        if daily_profit >= 0.0 {
            profit_days += 1;
        } else {
            loss_days += 1;
        }

        if prev.abs() > f64::EPSILON {
            returns.push(daily_profit / prev);
        }
    }

    let total_days = (points.len().saturating_sub(1)) as f64;
    let avg_daily_profit = if total_days > 0.0 {
        total_profit / total_days
    } else {
        0.0
    };

    let win_rate = if profit_days + loss_days > 0 {
        profit_days as f64 / (profit_days + loss_days) as f64
    } else {
        0.0
    };

    let sharpe_ratio = if !returns.is_empty() {
        let mean = returns.iter().copied().sum::<f64>() / returns.len() as f64;
        let variance = returns
            .iter()
            .map(|r| {
                let diff = r - mean;
                diff * diff
            })
            .sum::<f64>()
            / returns.len() as f64;
        let std_dev = variance.sqrt();
        if std_dev > 0.0 {
            mean / std_dev * (returns.len() as f64).sqrt()
        } else {
            0.0
        }
    } else {
        0.0
    };

    EquityStatistics {
        start_balance,
        end_balance,
        total_profit,
        total_profit_rate,
        max_drawdown,
        max_drawdown_rate,
        profit_days,
        loss_days,
        win_rate,
        avg_daily_profit,
        sharpe_ratio,
    }
}

/// 查询持仓（按account_id查询单个账户）
pub async fn query_position(
    account_id: web::Path<String>, // 修复: 改为account_id
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_account(&account_id) {
        Ok(account) => {
            // @yutiansut @quantaxis: 直接用 qars 的 volume_long()/volume_short() 包含冻结量
            let mut acc = account.write();
            let mut positions = Vec::new();
            for (code, pos) in acc.hold.iter_mut() {
                                // ⚠️ 跳过全零持仓 —— 否则持仓页会出现一行行"空壳"。@yutiansut @quantaxis
                //
                // qars 清理零持仓的 `retain`(qars2 account.rs:562)在 **`settle()` 里**,
                // 而本系统**从不结算**(daily_settlement 只能手动调,main.rs 无调度)——
                // 平光的持仓就永远留在 `hold` 里。
                // 实测(2026-09-05,78 个账户):本接口返回 51 行,其中 **11 行四个量全为 0**。
                //
                // 在查询层过滤而非改 qars:① 不动别人的库;② 不依赖结算是否运行;
                // ③ `hold` 保留记录本身是对的(带着 open_cost 等历史),
                //    只是不该出现在"当前持仓"列表里。
                if pos.volume_long() == 0.0
                    && pos.volume_short() == 0.0
                    && pos.volume_long_frozen() == 0.0
                    && pos.volume_short_frozen() == 0.0
                {
                    continue;
                }
                positions.push(PositionInfo {
                    account_id: account_id.to_string(),
                    instrument_id: code.clone(),
                    volume_long: pos.volume_long(),
                    volume_short: pos.volume_short(),
                    volume_long_frozen: pos.volume_long_frozen(),   // @yutiansut @quantaxis
                    volume_short_frozen: pos.volume_short_frozen(), // @yutiansut @quantaxis
                    cost_long: pos.open_price_long,
                    cost_short: pos.open_price_short,
                    profit_long: pos.float_profit_long(),
                    profit_short: pos.float_profit_short(),
                    // ✨ 真实保证金/市价,替代前端的 ×300 与 15% 硬编码 @yutiansut @quantaxis
                    margin_long: pos.margin_long,
                    margin_short: pos.margin_short,
                    last_price: pos.lastest_price,
                    position_cost_long: pos.position_cost_long,
                    position_cost_short: pos.position_cost_short,
                });
            }

            Ok(HttpResponse::Ok().json(ApiResponse::success(positions)))
        }
        Err(e) => {
            log::warn!("Failed to query position by account_id: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("{}", e),   // context 不复述错误本身 @yutiansut @quantaxis
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

    // @yutiansut @quantaxis: 直接用 qars 的 volume_long()/volume_short()
    let mut all_positions = Vec::new();
    for account in accounts {
        let mut acc = account.write();
        let acc_id = acc.account_cookie.clone();
        for (code, pos) in acc.hold.iter_mut() {
            // 同上:跳过全零持仓(qars 的清理 `retain` 在 settle 里,
            // 而本系统从不结算 —— 见另一处的完整说明)。@yutiansut @quantaxis
            if pos.volume_long() == 0.0
                && pos.volume_short() == 0.0
                && pos.volume_long_frozen() == 0.0
                && pos.volume_short_frozen() == 0.0
            {
                continue;
            }
            all_positions.push(PositionInfo {
                account_id: acc_id.clone(),
                instrument_id: code.clone(),
                volume_long: pos.volume_long(),
                volume_short: pos.volume_short(),
                volume_long_frozen: pos.volume_long_frozen(),   // @yutiansut @quantaxis
                volume_short_frozen: pos.volume_short_frozen(), // @yutiansut @quantaxis
                cost_long: pos.open_price_long,
                cost_short: pos.open_price_short,
                profit_long: pos.float_profit_long(),
                profit_short: pos.float_profit_short(),
                // ✨ 同上
                margin_long: pos.margin_long,
                margin_short: pos.margin_short,
                last_price: pos.lastest_price,
                position_cost_long: pos.position_cost_long,
                position_cost_short: pos.position_cost_short,
            });
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(all_positions)))
}

/// 入金
///
/// 支持两种方式 @yutiansut @quantaxis：
/// 1. 传入 account_id (ACC_xxx 格式) - 直接指定账户（推荐）
/// 2. 传入 user_id (UUID 格式) - 仅当用户只有一个账户时有效
///
/// 注意：如果用户有多个账户，必须使用 account_id 指定具体账户
pub async fn deposit(
    req: web::Json<DepositRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::service::http::transfer::TRANSFER_STORE;
    use chrono::Utc;

    // ✨ 金额必须为正 @yutiansut @quantaxis
    //
    // 这两个用户端接口**绕过了 CapitalManager**,直接调 acc.deposit()/acc.withdraw(),
    // 因此也绕过了 capital_mgr.rs:110/:188 已有的 `amount <= 0.0` 校验。
    //
    // 实测(隔离实例,干净账户 100 万):
    //   POST /api/account/withdraw  {"amount": -9000000}
    //     → 200, available 100 万 → **1000 万**(凭空造出 900 万)
    //   POST /api/account/deposit   {"amount": -200000000}
    //     → 200, available → **-1 亿**(负数入金 = 不受余额校验的出金)
    //   POST /api/management/deposit {"amount": -123456}
    //     → 400 "存款金额必须大于0"     ← 管理端走 CapitalManager,是对的
    //
    // 即出金的 `acc.money < req.amount` 检查对负数完全无效:
    // 正余额账户 `1000000 < -9000000` 为假,直接放行。
    if let Err(e) = super::models::validate_money_amount(req.amount) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e)));
    }


    // 尝试直接获取账户，如果失败且ID不是ACC_开头，则尝试按user_id查找
    let account = if req.account_id.starts_with("ACC_") {
        state.account_mgr.get_account(&req.account_id)
    } else {
        // 尝试按 user_id 查找用户的账户
        let accounts = state.account_mgr.get_accounts_by_user(&req.account_id);
        if accounts.is_empty() {
            Err(crate::ExchangeError::AccountError(format!(
                "No accounts found for user: {}",
                req.account_id
            )))
        } else if accounts.len() > 1 {
            // 用户有多个账户，必须指定 account_id
            let account_ids: Vec<String> = accounts
                .iter()
                .map(|a| a.read().account_cookie.clone())
                .collect();
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!(
                    "User has {} accounts, please specify account_id: {:?}",
                    accounts.len(),
                    account_ids
                ),
            )));
        } else {
            Ok(accounts[0].clone())
        }
    };

    match account {
        Ok(account) => {
            let mut acc = account.write();
            let account_id = acc.account_cookie.clone();
            // 变动**前**取值 —— 不能事后用 after-amount 倒推,那样浮点误差会累积
            // 且一旦 deposit 语义变化(比如手续费)就不成立。@yutiansut @quantaxis
            let balance_before = acc.get_balance();
            // 使用 QA_Account 的标准 deposit 方法
            acc.deposit(req.amount);
            let balance_after = acc.get_balance();

            // 记录转账流水到 TransferStore
            let record = TransferRecord {
                id: uuid::Uuid::new_v4().to_string(),
                datetime: Utc::now().timestamp_millis(),
                currency: "CNY".to_string(),
                amount: req.amount,
                balance_before,
                balance_after,
                error_id: 0,
                error_msg: "入金成功".to_string(),
                bank_id: "API".to_string(),
                bank_name: "API入金".to_string(),
            };
            TRANSFER_STORE.add_record(&account_id, record);

            log::info!("Deposit {} to account {}", req.amount, account_id);

            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "account_id": account_id,
                    "balance": acc.get_balance(),
                    "available": acc.money
                }))),
            )
        }
        Err(e) => {
            log::warn!("Failed to deposit: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("{}", e),   // context 不复述错误本身 @yutiansut @quantaxis
            )))
        }
    }
}

/// 出金
///
/// 支持两种方式 @yutiansut @quantaxis：
/// 1. 传入 account_id (ACC_xxx 格式) - 直接指定账户（推荐）
/// 2. 传入 user_id (UUID 格式) - 仅当用户只有一个账户时有效
///
/// 注意：如果用户有多个账户，必须使用 account_id 指定具体账户
pub async fn withdraw(
    req: web::Json<WithdrawRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // ✨ 金额必须为正 @yutiansut @quantaxis
    //
    // 这两个用户端接口**绕过了 CapitalManager**,直接调 acc.deposit()/acc.withdraw(),
    // 因此也绕过了 capital_mgr.rs:110/:188 已有的 `amount <= 0.0` 校验。
    //
    // 实测(隔离实例,干净账户 100 万):
    //   POST /api/account/withdraw  {"amount": -9000000}
    //     → 200, available 100 万 → **1000 万**(凭空造出 900 万)
    //   POST /api/account/deposit   {"amount": -200000000}
    //     → 200, available → **-1 亿**(负数入金 = 不受余额校验的出金)
    //   POST /api/management/deposit {"amount": -123456}
    //     → 400 "存款金额必须大于0"     ← 管理端走 CapitalManager,是对的
    //
    // 即出金的 `acc.money < req.amount` 检查对负数完全无效:
    // 正余额账户 `1000000 < -9000000` 为假,直接放行。
    if let Err(e) = super::models::validate_money_amount(req.amount) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e)));
    }


    use crate::service::http::transfer::TRANSFER_STORE;
    use chrono::Utc;

    // 尝试直接获取账户，如果失败且ID不是ACC_开头，则尝试按user_id查找
    let account = if req.account_id.starts_with("ACC_") {
        state.account_mgr.get_account(&req.account_id)
    } else {
        // 尝试按 user_id 查找用户的账户
        let accounts = state.account_mgr.get_accounts_by_user(&req.account_id);
        if accounts.is_empty() {
            Err(crate::ExchangeError::AccountError(format!(
                "No accounts found for user: {}",
                req.account_id
            )))
        } else if accounts.len() > 1 {
            // 用户有多个账户，必须指定 account_id
            let account_ids: Vec<String> = accounts
                .iter()
                .map(|a| a.read().account_cookie.clone())
                .collect();
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!(
                    "User has {} accounts, please specify account_id: {:?}",
                    accounts.len(),
                    account_ids
                ),
            )));
        } else {
            Ok(accounts[0].clone())
        }
    };

    match account {
        Ok(account) => {
            let mut acc = account.write();
            let account_id = acc.account_cookie.clone();
            // 变动前取值,与入金路径保持一致(不用 after+amount 倒推)
            let balance_before = acc.get_balance();

            // 检查可用余额（acc.money 才是真正的可用资金）
            if acc.money < req.amount {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    400,
                    "Insufficient available balance".to_string(),
                )));
            }

            // 使用带核对的出金 —— 见 account_mgr::withdraw_checked 的说明:
            // qars 的 withdraw 对「出金 == 全部可用资金」是静默空转且无返回值。
            if let Err(e) =
                crate::exchange::account_mgr::withdraw_checked(&mut acc, req.amount)
            {
                log::error!("Withdraw {} from {} 未生效: {}", req.amount, account_id, e);
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error(400, format!("出金未生效: {}", e))));
            }

            // 记录转账流水到 TransferStore（出金用负数表示）
            let balance_after = acc.get_balance();
            let record = TransferRecord {
                id: uuid::Uuid::new_v4().to_string(),
                datetime: Utc::now().timestamp_millis(),
                currency: "CNY".to_string(),
                amount: -req.amount, // 出金用负数
                balance_before,
                balance_after,
                error_id: 0,
                error_msg: "出金成功".to_string(),
                bank_id: "API".to_string(),
                bank_name: "API出金".to_string(),
            };
            TRANSFER_STORE.add_record(&account_id, record);

            log::info!("Withdraw {} from account {}", req.amount, account_id);

            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "account_id": account_id,
                    "balance": acc.get_balance(),
                    "available": acc.money
                }))),
            )
        }
        Err(e) => {
            log::warn!("Failed to withdraw: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("{}", e),   // context 不复述错误本身 @yutiansut @quantaxis
            )))
        }
    }
}

/// 查询用户成交记录
/// @yutiansut @quantaxis
/// 返回 UserTradeView 列表，包含用户方向信息（BUY/SELL）和是否为主动方（is_taker）
pub async fn query_user_trades(
    user_id: web::Path<String>,
    query: web::Query<OrderPageQuery>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 按user_id查询：聚合该用户所有账户的成交记录
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);
    let accounts_empty = accounts.is_empty();

    // ✨ 数据源:账户 QIFI 的 dailytrades **为主**,TradeRecorder 为补充。
    // @yutiansut @quantaxis
    //
    // `trade_recorder` 是**执行态**组件:`new()` 建空表、全仓无 restore 入口
    // (已做阳性对照),**每次重启归零**。实测(进程已运行 12 小时):
    // 21 个账户在快照里共 151,695 笔成交,而本端点返回 **6 笔**、73/78 个账户为 0;
    // 同时仪表盘显示「成交 151,695」(它读 account_mgr)—— **同屏自相矛盾**。
    //
    // 账户 QIFI 有完整历史且快照恢复已验证(`✅ Recovered 70 accounts`)。
    // `management.rs:192 get_account_detail` 早就是这么读的 —— **复用既有读法**。
    //
    // 两者合并而非二选一:TradeRecorder 有本次运行期的最新成交(可能还没进
    // 60 秒快照),QIFI 有全部历史。按 trade_id 去重。
    let mut all_trades = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for account in accounts {
        let account_id = {
            let acc = account.read();
            acc.account_cookie.clone()
        }; // ← 读锁尽早释放:下面 get_qifi_slice 会自己取锁
        if let Ok(qifi) = state.account_mgr.get_qifi_slice(&account_id) {
            // 账户当前交易日只作**兜底** —— 每笔成交优先用自己的时间戳解析
            let td = qifi.trading_day.clone();
            let mut resolver = TradingDayResolver::new();
            for t in qifi.trades.values() {
                if seen.insert(t.trade_id.clone()) {
                    let day = resolver.resolve(t.trade_date_time).unwrap_or_else(|| td.clone());
                    all_trades.push(UserTradeView::from_qifi_trade(t, &day));
                }
            }
        }
        for t in state.trade_recorder.get_trades_by_user(&account_id).iter() {
            if seen.insert(t.trade_id.clone()) {
                all_trades.push(UserTradeView::from_trade_record(t, &account_id));
            }
        }
    }
    // 按时间倒序 —— HashMap 迭代顺序不定,不排序会导致翻页看到重复条目
    // (management.rs:241 踩过同一个坑)。
    all_trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 「用户不存在」必须与「该用户没有记录」可区分 @yutiansut @quantaxis
    //
    // 实测(2026-09-05)对同一个不存在的 user_id:
    //   GET /api/position/user/{x} → 404 (query_positions_by_user:868 有检查)
    //   GET /api/trades/user/{x}   → 200 空表   ← 本端点
    //   GET /api/order/user/{x}    → 200 空表
    // 相邻端点行为相反,前端无从区分,表格一律显示「暂无数据」。
    //
    // ⚠️ 我先前只修了 `/api/trades/account/{id}` —— 而 `trades/index.vue:220`
    //    调的是 `queryUserTrades` → **本端点**;`queryAccountTrades` 全仓零调用。
    //    修了没人用的那个。改端点语义前先确认前端调的是哪一个。
    //
    // 沿用 query_positions_by_user 的判据,但**多一个条件**:仅当同时无任何记录时
    // 才 404,避免「账户已移除、历史记录仍在」时把数据藏起来。
    if accounts_empty && all_trades.is_empty() {
        return Ok(super::models::exchange_error_response(
            "查询用户数据",
            &crate::ExchangeError::NotFound(format!("用户不存在或名下没有账户:{}", user_id)),
        ));
    }

    log::info!(
        "Querying trades for user: {}, found {} trades",
        user_id,
        all_trades.len()
    );

    // ✨ 分页 —— 与 query_user_orders 同一套参数,不传则全量(向后兼容)
    //    @yutiansut @quantaxis
    //
    // 单账户实测 18,609 条成交 / **5.4MB** 响应。这个体积还引发过另一个坑:
    // 后端对大响应用 brotli 压缩,而 requests 的 brotli 解码器在这个尺寸上会炸
    // (见 tasks/lessons.md L40),压测端因此把 18,609 条统计成 `trades: 0`。
    // ✨ 先按合约/日期筛选,再分页 @yutiansut @quantaxis
    all_trades.retain(|t| query.match_instrument(&t.instrument_id) && query.match_date(t.timestamp));
    let total = all_trades.len();
    let paged = match (query.page, query.page_size) {
        (Some(page), Some(size)) if size > 0 => {
            let start = page.saturating_sub(1).saturating_mul(size);
            all_trades.into_iter().skip(start).take(size).collect::<Vec<_>>()
        }
        _ => all_trades,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "trades": paged,
            "total": total
        }))),
    )
}

/// 查询账户成交记录（按account_id）
/// @yutiansut @quantaxis
/// 返回 UserTradeView 列表，包含用户方向信息（BUY/SELL）和是否为主动方（is_taker）
pub async fn query_account_trades(
    account_id: web::Path<String>,
    query: web::Query<OrderPageQuery>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 注意：TradeRecorder.by_user 实际上索引的是 account_id
    let trades = state.trade_recorder.get_trades_by_user(&account_id);

    // 「账户不存在」必须与「账户没有成交」可区分 @yutiansut @quantaxis
    //
    // 实测(2026-09-05)对同一个不存在的账户:
    //   GET /api/position/account/{x}  → 404  (query_position:823 走 get_account,有检查)
    //   GET /api/trades/account/{x}    → 200 {"trades":[],"total":0}   ← 这里,无检查
    // 同一资源、相邻端点,行为相反。后果是**前端无从区分**「账户不存在」与
    // 「该账户没成交」—— 表格一律显示「暂无数据」,而 `loadError` 为 null,
    // 前端那套「加载失败 vs 真没数据」的区分在这里根本无从下手(信号在源头就丢了)。
    //
    // 只在「账户不存在 **且** 没有任何成交」时才 404 —— 这样即便存在
    // 「账户已从 account_mgr 移除、历史成交仍在 trade_recorder」的情形,
    // 数据也不会因为这个检查而变得不可访问。宁可少报一次 404,不可藏数据。
    if trades.is_empty() && state.account_mgr.get_account(&account_id).is_err() {
        return Ok(super::models::exchange_error_response(
            "查询账户成交",
            &crate::ExchangeError::NotFound(format!("账户不存在:{}", account_id)),
        ));
    }

    // ✨ 数据源:账户 QIFI 的 dailytrades **为主**,TradeRecorder 为补充。
    // @yutiansut @quantaxis
    //
    // 原实现只读 `trade_recorder`。它是**执行态**组件:`new()` 建空表、
    // 全仓无 restore/load 入口(已做阳性对照),**每次重启归零**。
    // 实测(2026-09-05,进程已运行 11 小时):21 个账户在快照里共有 151,695 笔成交,
    // 而本接口对其中 8 个抽样账户返回 0/0/0/0/0/0/2/0 —— 页面一片空白,
    // 同时仪表盘却显示「成交 151,695」(它读的是 account_mgr)。**同屏自相矛盾**。
    //
    // 账户 QIFI 里有完整历史且快照恢复已验证可用(启动日志
    // `✅ Recovered 70 accounts from snapshots`)。`management.rs:192
    // get_account_detail` 早就是这么读的 —— 所以这里是**复用既有读法**,
    // 不是新实现;那个接口在同一账户上能正确显示 45,753 条。
    //
    // 两者合并而非二选一:TradeRecorder 里有**本次运行期**的最新成交
    // (可能还没进 60 秒快照),QIFI 里有全部历史。按 trade_id 去重。
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut user_trades: Vec<UserTradeView> = Vec::new();

    if let Ok(qifi) = state.account_mgr.get_qifi_slice(&account_id) {
        // 同上:账户当前交易日只作兜底
        let td = qifi.trading_day.clone();
        let mut resolver = TradingDayResolver::new();
        for t in qifi.trades.values() {
            if seen.insert(t.trade_id.clone()) {
                let day = resolver.resolve(t.trade_date_time).unwrap_or_else(|| td.clone());
                user_trades.push(UserTradeView::from_qifi_trade(t, &day));
            }
        }
    }
    for t in trades.iter() {
        if seen.insert(t.trade_id.clone()) {
            user_trades.push(UserTradeView::from_trade_record(t, &account_id));
        }
    }
    // 按时间倒序 —— HashMap 迭代顺序不定,不排序会导致翻页看到重复条目
    // (management.rs:241 踩过同一个坑,注释在那里)。
    user_trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    log::info!(
        "Querying trades for account: {}, found {} trades",
        account_id,
        user_trades.len()
    );

    // ✨ 分页 —— 与 query_user_trades 同一套参数 @yutiansut @quantaxis
    // ✨ 同上:先筛选后分页
    let mut user_trades = user_trades;
    user_trades.retain(|t| query.match_instrument(&t.instrument_id) && query.match_date(t.timestamp));
    let total = user_trades.len();
    let paged = match (query.page, query.page_size) {
        (Some(page), Some(size)) if size > 0 => {
            let start = page.saturating_sub(1).saturating_mul(size);
            user_trades.into_iter().skip(start).take(size).collect::<Vec<_>>()
        }
        _ => user_trades,
    };


    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "trades": paged,
            "total": total
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

    // ⚠️ 开户资金必须校验 —— 与入金走同一套判据。@yutiansut @quantaxis
    //
    // 实测(2026-09-05,回验代理发现):`init_cash` **完全不校验**,
    // `1e300` 与**负数**都返回 HTTP 200 开户成功。
    // 而入金/出金/转账早已统一走 `validate_money_amount`
    // (`models.rs:120`,挡 NaN/Inf、非正数、超单笔上限)——
    // **同一个坑修了那边没修这边**,开户恰恰是资金进入系统的第一道门。
    //
    // 与入金的唯一差异:**允许 0** —— 注册时自动开的默认账户就是
    // `init_cash: 0.0`(auth.rs:29),那是合法的"空账户"。
    if req.init_cash != 0.0 {
        if let Err(e) = super::models::validate_money_amount(req.init_cash) {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error(400, format!("初始资金不合法:{}", e))));
        }
    } else if !req.init_cash.is_finite() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(400, "初始资金必须是有限数值".to_string())));
    }

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
                    format!("Failed to create account: {}", e),
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

            // margin = 持仓保证金 + 冻结保证金（待处理订单）@yutiansut @quantaxis
            let position_margin = acc.get_margin();
            let frozen_margin = acc.get_frozen_margin();
            let margin = position_margin + frozen_margin;

            serde_json::json!({
                "account_id": acc.account_cookie.clone(),
                "account_name": account_name,
                "account_type": format!("{:?}", account_type),
                "balance": acc.get_balance(),
                "available": acc.money,
                "margin": margin,
                "frozen_margin": frozen_margin,
                "risk_ratio": acc.safe_risk_ratio(),
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

// ==================== Phase 11: 批量下单/条件单/订单修改 API ====================
// @yutiansut @quantaxis

/// 批量下单
/// POST /api/order/batch
pub async fn batch_submit_orders(
    req: web::Json<BatchOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::order_router::SubmitOrderRequest as CoreSubmitOrderRequest;

    let account_id = &req.account_id;
    let orders = &req.orders;

    // ✨ 校验账户归属 @yutiansut @quantaxis
    //
    // 归属校验在其余 6 个下单/撤单入口都有:handlers.rs:241/:307、
    // websocket/handler.rs:112/:179、websocket/diff_handler.rs:619/:801。
    // **只有批量这两个没有** —— 它们的请求体连 user_id 都没有,
    // 等于没有调用者身份的概念,无从校验。
    //
    // 实测(隔离实例,两个不同用户):
    //   单笔:攻击者用受害者 account_id → 4003 "does not belong to user"  ✓
    //   批量:同样越权 → **200 success**,受害者账户上真的多了一笔
    //         O17884446970280000000006 IF2501 BUY 2 手                    ✗
    if let Err(e) = state
        .account_mgr
        .verify_account_ownership(account_id, &req.user_id)
    {
        return Ok(HttpResponse::Forbidden().json(ApiResponse::<()>::error(
            4003,
            format!("Account verification failed: {}", e),
        )));
    }


    log::info!(
        "📦 批量下单: account_id={}, 订单数={}",
        account_id,
        orders.len()
    );

    // 验证账户存在
    if state.account_mgr.get_account(account_id).is_err() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在: {}", account_id),
        )));
    }

    let mut results = Vec::with_capacity(orders.len());
    let mut success_count = 0;
    let mut failed_count = 0;

    for (index, order) in orders.iter().enumerate() {
        let core_req = CoreSubmitOrderRequest {
            account_id: account_id.clone(),
            instrument_id: order.instrument_id.clone(),
            direction: order.direction.clone(),
            offset: order.offset.clone(),
            volume: order.volume,
            price: order.price,
            order_type: order.order_type.clone(),
            time_condition: None,
            volume_condition: None,
        };

        let response = state.order_router.submit_order(core_req);

        if response.success {
            success_count += 1;
            results.push(SingleOrderResult {
                index,
                success: true,
                order_id: response.order_id,
                error: None,
            });
        } else {
            failed_count += 1;
            results.push(SingleOrderResult {
                index,
                success: false,
                order_id: None,
                error: response.error_message,
            });
        }
    }

    log::info!(
        "📦 批量下单完成: 成功={}, 失败={}",
        success_count,
        failed_count
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(BatchOrderResponse {
        total: orders.len(),
        success_count,
        failed_count,
        results,
    })))
}

/// 批量撤单
/// POST /api/order/batch-cancel
pub async fn batch_cancel_orders(
    req: web::Json<BatchCancelRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::order_router::CancelOrderRequest as CoreCancelOrderRequest;

    let account_id = &req.account_id;
    let order_ids = &req.order_ids;

    // ✨ 校验账户归属 @yutiansut @quantaxis
    //
    // 归属校验在其余 6 个下单/撤单入口都有:handlers.rs:241/:307、
    // websocket/handler.rs:112/:179、websocket/diff_handler.rs:619/:801。
    // **只有批量这两个没有** —— 它们的请求体连 user_id 都没有,
    // 等于没有调用者身份的概念,无从校验。
    //
    // 实测(隔离实例,两个不同用户):
    //   单笔:攻击者用受害者 account_id → 4003 "does not belong to user"  ✓
    //   批量:同样越权 → **200 success**,受害者账户上真的多了一笔
    //         O17884446970280000000006 IF2501 BUY 2 手                    ✗
    if let Err(e) = state
        .account_mgr
        .verify_account_ownership(account_id, &req.user_id)
    {
        return Ok(HttpResponse::Forbidden().json(ApiResponse::<()>::error(
            4003,
            format!("Account verification failed: {}", e),
        )));
    }


    log::info!(
        "📦 批量撤单: account_id={}, 订单数={}",
        account_id,
        order_ids.len()
    );

    let mut results = Vec::with_capacity(order_ids.len());
    let mut success_count = 0;
    let mut failed_count = 0;

    for (index, order_id) in order_ids.iter().enumerate() {
        let core_req = CoreCancelOrderRequest {
            account_id: account_id.clone(),
            order_id: order_id.clone(),
        };

        match state.order_router.cancel_order(core_req) {
            Ok(_) => {
                success_count += 1;
                results.push(SingleOrderResult {
                    index,
                    success: true,
                    order_id: Some(order_id.clone()),
                    error: None,
                });
            }
            Err(e) => {
                failed_count += 1;
                results.push(SingleOrderResult {
                    index,
                    success: false,
                    order_id: Some(order_id.clone()),
                    error: Some(format!("{}", e)),
                });
            }
        }
    }

    log::info!(
        "📦 批量撤单完成: 成功={}, 失败={}",
        success_count,
        failed_count
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(BatchCancelResponse {
        total: order_ids.len(),
        success_count,
        failed_count,
        results,
    })))
}

/// 修改订单（撤单 + 重新下单）
/// PUT /api/order/{order_id}
pub async fn modify_order(
    order_id: web::Path<String>,
    req: web::Json<ModifyOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::order_router::{
        CancelOrderRequest as CoreCancelOrderRequest, SubmitOrderRequest as CoreSubmitOrderRequest,
    };

    let order_id = order_id.into_inner();
    log::info!(
        "📝 修改订单: order_id={}, account_id={}",
        order_id,
        req.account_id
    );

    // 1. 获取原订单信息
    let original = match state.order_router.get_order_detail(&order_id) {
        Some((order, status, _, _, filled)) => {
            // ✨ 用模式匹配判定可改状态,不要比较 Debug 字符串 @yutiansut @quantaxis
            //
            // 原实现是:
            //   if format!("{:?}", status) != "ALIVE" && format!("{:?}", status) != "Alive"
            //
            // 但 `get_order_detail` 返回的是 `order_router::OrderStatus`,它的变体是
            //   PendingRisk / PendingRoute / Submitted / PartiallyFilled /
            //   Filled / Cancelled / Rejected            (order_router.rs:156-172)
            // 另一个 `core::order_ext::OrderStatus` 的变体是
            //   Pending / Accepted / PartiallyFilled / Filled / Cancelled / Rejected
            //                                            (core/order_ext.rs:10-23)
            // **两个枚举都没有 `Alive`**,所以该条件恒为真 ——
            // 这个接口对任何状态的任何订单都返回 4005,从来不可能成功。
            //
            // 实测(隔离实例,一张 status=Submitted 的活单):
            //   改价 / 改量 / 改不存在的单 / 改负价 / 改 0 手
            //   → 全部 `4005 订单状态不允许修改: Submitted`
            // 生产 92,023 笔订单的状态分布里也没有 Alive:
            //   {Cancelled: 75053, Submitted: 173, Filled: 16749, PartiallyFilled: 48}
            //
            // 可改 = 已在簿上且未终结:Submitted / PartiallyFilled。
            // 排除 PendingRisk / PendingRoute(还没进簿,改了也没意义)
            // 与 Filled / Cancelled / Rejected(已终结)。
            //
            // 顺带:比较 `format!("{:?}", ..)` 的字面量是编译器抓不到的坑 ——
            // 改个变体名就静默失效。matches! 既正确又能被编译器检查。
            if !matches!(
                status,
                crate::exchange::order_router::OrderStatus::Submitted
                    | crate::exchange::order_router::OrderStatus::PartiallyFilled
            ) {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    4005,
                    format!("订单状态不允许修改: {:?}", status),
                )));
            }
            if filled > 0.0 {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                    4006,
                    "已部分成交的订单不能修改".to_string(),
                )));
            }
            order
        }
        None => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("订单不存在: {}", order_id),
            )));
        }
    };

    // 2. 撤销原订单
    let cancel_req = CoreCancelOrderRequest {
        account_id: req.account_id.clone(),
        order_id: order_id.clone(),
    };

    if let Err(e) = state.order_router.cancel_order(cancel_req) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4007,
            format!("撤单失败: {}", e),
        )));
    }

    // 3. 重新下单（使用新价格/数量）
    let new_price = req.new_price.unwrap_or(original.limit_price);
    let new_volume = req.new_volume.unwrap_or(original.volume_orign);

    let submit_req = CoreSubmitOrderRequest {
        account_id: req.account_id.clone(),
        instrument_id: original.instrument_id.clone(),
        direction: original.direction.clone(),
        offset: original.offset.clone(),
        volume: new_volume,
        price: new_price,
        order_type: original.price_type.clone(),
        time_condition: None,
        volume_condition: None,
    };

    let response = state.order_router.submit_order(submit_req);

    if response.success {
        log::info!(
            "📝 订单修改成功: {} -> {:?}",
            order_id,
            response.order_id
        );
        Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "old_order_id": order_id,
            "new_order_id": response.order_id,
            "new_price": new_price,
            "new_volume": new_volume,
            "message": "订单修改成功"
        }))))
    } else {
        log::warn!(
            "📝 订单修改失败（重新下单失败）: {} - {}",
            order_id,
            response.error_message.as_deref().unwrap_or("未知错误")
        );
        Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4008,
            format!(
                "订单修改失败（原订单已撤销，新订单提交失败）: {}",
                response.error_message.unwrap_or_default()
            ),
        )))
    }
}

/// 创建条件单
/// POST /api/order/conditional
pub async fn create_conditional_order(
    req: web::Json<CreateConditionalOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::conditional_order::CONDITIONAL_ORDER_ENGINE;

    log::info!(
        "📋 创建条件单: account_id={}, instrument={}, trigger={}",
        req.account_id,
        req.instrument_id,
        req.trigger_price
    );

    // 验证账户存在
    if state.account_mgr.get_account(&req.account_id).is_err() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在: {}", req.account_id),
        )));
    }

    let engine = CONDITIONAL_ORDER_ENGINE.read();
    match engine.create_order(req.into_inner()) {
        Ok(order_info) => {
            log::info!("📋 条件单创建成功: {}", order_info.conditional_order_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(order_info)))
        }
        Err(e) => {
            log::warn!("📋 条件单创建失败: {}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                4009,
                e,
            )))
        }
    }
}

/// 查询条件单列表
/// GET /api/order/conditional/list?account_id=xxx
pub async fn get_conditional_orders(
    query: web::Query<std::collections::HashMap<String, String>>,
    _state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::conditional_order::CONDITIONAL_ORDER_ENGINE;

    let account_id = match query.get("account_id") {
        Some(id) => id,
        None => {
            // 带上**实际收到的参数名** @yutiansut @quantaxis
            //
            // 排查「接口报 400」时,「缺少 account_id」只说了缺什么,没说收到了什么。
            // 而这类 400 的常见真因是**传成了 user_id**(本仓同一份数据常有
            // by-account / by-user 两套端点,很容易拿错)。把收到的键列出来,
            // 一眼就能看出是「没传」还是「传错名字」。
            let got: Vec<&str> = query.keys().map(|k| k.as_str()).collect();
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!(
                    "缺少 account_id 参数(收到的参数: {})",
                    if got.is_empty() { "<无>".to_string() } else { got.join(", ") }
                ),
            )));
        }
    };

    let engine = CONDITIONAL_ORDER_ENGINE.read();
    let orders = engine.get_orders_by_account(account_id);

    log::info!(
        "📋 查询条件单: account_id={}, 数量={}",
        account_id,
        orders.len()
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "orders": orders,
        "total": orders.len()
    }))))
}

/// 取消条件单
/// DELETE /api/order/conditional/{conditional_order_id}
pub async fn cancel_conditional_order(
    conditional_order_id: web::Path<String>,
    _state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::conditional_order::CONDITIONAL_ORDER_ENGINE;

    let order_id = conditional_order_id.into_inner();
    log::info!("📋 取消条件单: {}", order_id);

    let engine = CONDITIONAL_ORDER_ENGINE.read();
    match engine.cancel_order(&order_id) {
        Ok(_) => {
            log::info!("📋 条件单取消成功: {}", order_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "conditional_order_id": order_id,
                "message": "条件单取消成功"
            }))))
        }
        Err(e) => {
            log::warn!("📋 条件单取消失败: {} - {}", order_id, e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                4010,
                e,
            )))
        }
    }
}

/// 获取条件单统计
/// GET /api/order/conditional/statistics
pub async fn get_conditional_order_statistics(
    _state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    use crate::exchange::conditional_order::CONDITIONAL_ORDER_ENGINE;

    let engine = CONDITIONAL_ORDER_ENGINE.read();
    let stats = engine.get_statistics();

    Ok(HttpResponse::Ok().json(ApiResponse::success(stats)))
}
