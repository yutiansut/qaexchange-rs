//! HTTP API 请求/响应模型

use serde::{Deserialize, Serialize};

/// `ExchangeError` → HTTP 响应的**统一映射** @yutiansut @quantaxis
///
/// 加这个函数是**收敛**,不是新增能力:在此之前全仓**没有**任何
/// `ExchangeError` → HTTP 状态码的统一策略,每个 handler 各自硬编码。
/// 仅 `market.rs` 一个文件就有 5 段一模一样的
/// `HttpResponse::InternalServerError().json(ApiResponse::<()>::error(500, ...))`,
/// 于是「合约不存在」这种客户端错误也返回 500。
///
/// 实测(2026-09-05,后端真实访问日志 2251 次浏览器请求):
///   4xx = 0,5xx = 2,两条 5xx 全是 `GET /api/market/tick/{不存在的合约}`。
///   即当前**唯一**的 5xx 来源就是这个错误映射,而它本该是 404。
///
/// 为什么这不只是洁癖:
/// ① 前端 `api/request.js` 按「4xx+结构化消息 ⇒ 业务判定(黄色 warning)/
///    5xx ⇒ 系统故障(红色 error)」分流。合约打错字会弹红色「系统故障」。
/// ② 5xx 率是运维告警的常用指标,用户输入错误不该触发。
/// ③ `log::error!` 会把客户端错误写进错误日志,污染排查基线。
///
/// `InstrumentError` **故意不映射 400**:收敛后它只剩「重复上市」一种含义
/// (`instrument_registry.rs:132`),按名字看却是个泛化变体,未来可能装服务端问题。
/// 而该路径已被 `admin.rs:157` 本地按 `BadRequest` 处理,走不到这里,
/// 保守留在默认分支不产生实际代价。
///
/// 未覆盖的变体一律 500 —— 这是**保守**方向:宁可把客户端错误报成服务端错误
/// (吵闹但安全),不可把服务端故障伪装成客户端错误(静默掉真问题)。
pub fn exchange_error_response(context: &str, e: &crate::ExchangeError) -> actix_web::HttpResponse {
    use crate::ExchangeError as E;
    let (resp, code) = match e {
        E::NotFound(_) => (actix_web::HttpResponse::NotFound(), 404u32),
        E::AuthError(_) => (actix_web::HttpResponse::Unauthorized(), 401),
        E::PermissionDenied(_) => (actix_web::HttpResponse::Forbidden(), 403),
        // 客户端输入/状态问题 —— 后端**主动做出的判定**,不是故障
        E::UserError(_)
        | E::OrderError(_)
        | E::AccountError(_)
        | E::RiskCheckFailed(_)
        | E::InvalidParameter(_)
        | E::InsufficientBalance(_) => (actix_web::HttpResponse::BadRequest(), 400),
        _ => (actix_web::HttpResponse::InternalServerError(), 500),
    };
    let msg = format!("{}: {}", context, e);
    // 客户端错误用 warn,只有真正的服务端故障才进 error 日志。
    //
    // ⚠️ **绕过本函数手写 HttpResponse 的分支必须守同一条规矩** ——
    // 2026-09-06 扫出 15 处「返回 4xx 却 log::error!」(auth.rs 6 处、
    // handlers.rs 9 处),已统一改为 warn。密码错、用户不存在、订单找不到
    // 都是**后端主动做出的判定**,不是故障;记成 error 会让
    // 「grep ERROR 找问题」这条运维路径失效。@yutiansut @quantaxis
    if code >= 500 {
        log::error!("❌ [HTTP] {}", msg);
    } else {
        log::warn!("⚠️ [HTTP {}] {}", code, msg);
    }
    let mut resp = resp;
    resp.json(ApiResponse::<()>::error(code, msg))
}

/// 通用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

/// API 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: u32,
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

    pub fn error(code: u32, message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError { code, message }),
        }
    }
}

/// 开户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAccountRequest {
    pub user_id: String,
    pub user_name: String,
    pub init_cash: f64,
    pub account_type: String, // "individual" | "institutional"
    pub password: String,
}

/// 单笔资金变动的上限(元)
///
/// 期货交易所单笔出入金 1 万亿已经远超任何真实场景;设这个上限是为了挡住
/// 明显异常的输入,而不是业务限额。
pub const MAX_MONEY_AMOUNT: f64 = 1_000_000_000_000.0;

/// 校验一笔资金金额 @yutiansut @quantaxis
///
/// 此前各入口只检查 `!(amount > 0.0)`(这个写法是**故意**的 —— 它同时挡住
/// NaN,因为 `NaN > 0.0` 为假;换成 `amount <= 0.0` 反而会放过 NaN)。
/// 但**没有上界**:2026-09-04 实测 `{"amount": 1e308}` 入金返回 200,
/// 余额真的变成 `1e+308`。
///
/// 危害不止于"数字很大":f64 在 1e308 量级的精度约 1e292,
/// 该账户**原有余额直接消失在浮点噪声里**,而 deposit/withdraw 是绑定
/// `money` 与 `accounts.deposit` 同步增减的 —— 一旦精度丢失,
/// **无法再通过任何 API 组合把账户恢复到一致状态**。
/// 后续任何加法还可能溢出成 inf,再参与除法就得到 NaN,污染风险率等派生量。
///
/// (`1e400` 会被 serde 在 JSON 解析阶段挡掉,所以 inf 进不来;
///  真正的缺口是"有限但荒谬"的数值。)
pub fn validate_money_amount(amount: f64) -> Result<(), String> {
    if !amount.is_finite() {
        return Err("金额必须是有限数值".to_string());
    }
    if !(amount > 0.0) {
        return Err("金额必须大于 0".to_string());
    }
    if amount > MAX_MONEY_AMOUNT {
        return Err(format!(
            "金额 {:.2} 超出单笔上限 {:.0}",
            amount, MAX_MONEY_AMOUNT
        ));
    }
    Ok(())
}

/// 账户查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub user_id: String,
    pub user_name: String,
    pub balance: f64,
    pub available: f64,
    pub frozen: f64,
    pub margin: f64,
    pub profit: f64,
    pub risk_ratio: f64,
    pub account_type: String,
    pub created_at: i64,
}

/// 订单提交请求（外部 HTTP API）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub user_id: String, // 用户身份（用于验证）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>, // 交易账户（推荐明确传递）✨
    pub instrument_id: String,
    pub direction: String, // BUY/SELL
    pub offset: String,    // OPEN/CLOSE/CLOSETODAY
    pub volume: f64,
    pub price: f64,
    pub order_type: String, // LIMIT/MARKET
}

/// 订单提交响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub order_id: String,
    pub status: String,
}

/// 撤单请求（外部 HTTP API）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub user_id: String, // 用户身份（用于验证）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>, // 交易账户（推荐明确传递）✨
    pub order_id: String,
}

/// 订单查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub order_id: String,
    pub user_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub filled_volume: f64,
    pub status: String,
    pub submit_time: i64,
    pub update_time: i64,
}

/// 持仓查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub account_id: String, // 账户ID（用于平仓时指定账户）
    pub instrument_id: String,
    pub volume_long: f64,
    pub volume_short: f64,
    pub volume_long_frozen: f64,   // 多头冻结量 @yutiansut @quantaxis
    pub volume_short_frozen: f64,  // 空头冻结量 @yutiansut @quantaxis
    pub cost_long: f64,
    pub cost_short: f64,
    pub profit_long: f64,
    pub profit_short: f64,
    // ✨ 补齐真实保证金与市价 —— 别让前端自己编公式 @yutiansut @quantaxis
    //
    // 原先本结构只有上面 10 个字段,没有 margin、没有 last_price、没有合约乘数,
    // 于是 web/src/views/positions/index.vue:249/264/270/285 只能硬编码
    //     positionValue = volume * lastPrice * 300
    //     margin        = volume * cost * 300 * 0.15   // 「假设保证金率15%」
    // ×300 只对 IF/IC/IH 成立(IH 也是 300,但其他品种不是),
    // 15% 更是凭空假设 —— 真实保证金率来自 MarketPreset,每个品种不同。
    // qars 的 QA_Position 本来就带 margin_long/margin_short/last_price,
    // 直接透出即可。同项目 user/margin.vue 已经在用真实值,证明数据拿得到。
    pub margin_long: f64,
    pub margin_short: f64,
    pub last_price: f64,
    /// 持仓成本(已含合约乘数),前端算市值/收益率时用它,不要再自乘乘数
    pub position_cost_long: f64,
    pub position_cost_short: f64,
}

/// 成交查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeInfo {
    pub trade_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
}

/// 入金请求
///
/// 支持两种方式 @yutiansut @quantaxis：
/// 1. 传入 account_id (ACC_xxx 格式) - 直接指定账户
/// 2. 传入 user_id (UUID 格式) - 自动查找用户的第一个账户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    /// 账户ID (优先使用) 或 用户ID (向后兼容)
    #[serde(alias = "user_id")]
    pub account_id: String,
    pub amount: f64,
}

/// 出金请求
///
/// 支持两种方式：
/// 1. 传入 account_id (ACC_xxx 格式) - 直接指定账户
/// 2. 传入 user_id (UUID 格式) - 自动查找用户的第一个账户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawRequest {
    /// 账户ID (优先使用) 或 用户ID (向后兼容)
    #[serde(alias = "user_id")]
    pub account_id: String,
    pub amount: f64,
}

/// 资金流水响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashFlowInfo {
    pub timestamp: i64,
    pub flow_type: String, // "deposit" | "withdraw" | "commission" | "profit"
    pub amount: f64,
    pub balance_after: f64,
    pub remark: String,
}

// ==================== Phase 10: User-Account API Models ====================

/// 为用户创建新账户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    pub account_name: String,
    pub init_cash: f64,
    pub account_type: String, // "individual" | "institutional" | "market_maker"
}

// ==================== Phase 11: 银期转账 API Models ====================
// @yutiansut @quantaxis

/// 银行信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankInfo {
    pub id: String,
    pub name: String,
}

/// 银期转账请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    pub account_id: String,
    pub bank_id: String,
    pub amount: f64,         // > 0 转入期货账户, < 0 转出期货账户
    pub bank_password: String,
    pub future_password: String,
}

/// 银期转账记录查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferQueryRequest {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 银期转账记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub datetime: i64,
    pub currency: String,
    pub amount: f64,
    /// 变动前余额 @yutiansut @quantaxis
    ///
    /// ⚠️ 此前 `TransferRecord` **没有余额字段**,而 `management.rs:410/481` 把它转成
    /// `FundTransaction` 时只能硬写 `0.0`(注释直言「TransferStore 不记录这个」)。
    /// 于是资金流水页(`admin/transactions.vue:102/107`)的「交易前/后余额」两列
    /// **永远显示 0.00**,毫无信息量 —— 而这两列各占 140px。
    /// 写入点本来就持有账户,取值零成本。
    #[serde(default)]
    pub balance_before: f64,
    /// 变动后余额
    #[serde(default)]
    pub balance_after: f64,
    pub error_id: i32,
    pub error_msg: String,
    pub bank_id: String,
    pub bank_name: String,
}

// ==================== Phase 11: 条件单 API Models ====================
// @yutiansut @quantaxis

/// 条件单类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConditionType {
    StopLoss,      // 止损
    TakeProfit,    // 止盈
    PriceTouch,    // 触价
}

/// 触发条件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TriggerCondition {
    #[serde(rename = "GE")]
    GreaterOrEqual,  // >=
    #[serde(rename = "LE")]
    LessOrEqual,     // <=
}

/// 条件单状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConditionalOrderStatus {
    Pending,     // 等待触发
    Triggered,   // 已触发
    Cancelled,   // 已取消
    Expired,     // 已过期
    Failed,      // 触发失败
}

/// 创建条件单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConditionalOrderRequest {
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,           // BUY/SELL
    pub offset: String,              // OPEN/CLOSE
    pub volume: f64,
    pub order_type: String,          // LIMIT/MARKET
    pub limit_price: Option<f64>,
    pub condition_type: ConditionType,
    pub trigger_price: f64,          // 触发价格
    pub trigger_condition: TriggerCondition, // GE (>=) / LE (<=)
    pub valid_until: Option<i64>,    // 有效期（时间戳，毫秒）
}

/// 条件单信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalOrderInfo {
    pub conditional_order_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub condition_type: ConditionType,
    pub trigger_price: f64,
    pub trigger_condition: TriggerCondition,
    pub valid_until: Option<i64>,
    pub status: ConditionalOrderStatus,
    pub created_at: i64,
    pub triggered_at: Option<i64>,
    pub result_order_id: Option<String>,  // 触发后生成的订单ID
}

// ==================== Phase 11: 批量下单 API Models ====================
// @yutiansut @quantaxis

/// 单个订单请求（用于批量下单）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleOrderRequest {
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub order_type: String,
}

/// 批量下单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrderRequest {
    /// 下单发起者 —— 用于校验 account_id 归属 @yutiansut @quantaxis
    pub user_id: String,
    pub account_id: String,
    pub orders: Vec<SingleOrderRequest>,
}

/// 单个订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleOrderResult {
    pub index: usize,
    pub success: bool,
    pub order_id: Option<String>,
    pub error: Option<String>,
}

/// 批量下单响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrderResponse {
    pub total: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub results: Vec<SingleOrderResult>,
}

/// 批量撤单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCancelRequest {
    /// 撤单发起者 —— 用于校验 account_id 归属 @yutiansut @quantaxis
    pub user_id: String,
    pub account_id: String,
    pub order_ids: Vec<String>,
}

/// 批量撤单响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCancelResponse {
    pub total: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub results: Vec<SingleOrderResult>,
}

// ==================== Phase 11: 订单修改 API Models ====================
// @yutiansut @quantaxis

/// 订单修改请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOrderRequest {
    pub account_id: String,
    pub order_id: String,
    pub new_price: Option<f64>,
    pub new_volume: Option<f64>,
}

// ==================== Phase 12: 密码管理 API Models ====================
// @yutiansut @quantaxis

/// 修改交易密码请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub account_id: String,
    pub old_password: String,
    pub new_password: String,
    pub password_type: PasswordType,  // TRADING / FUND
}

/// 密码类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PasswordType {
    Trading,  // 交易密码
    Fund,     // 资金密码
}

/// 重置密码请求（管理员操作）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub account_id: String,
    pub new_password: String,
    pub password_type: PasswordType,
    pub admin_token: String,  // 管理员认证令牌
}

// ==================== Phase 12: 手续费查询 API Models ====================
// @yutiansut @quantaxis

/// 手续费率信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommissionRate {
    pub instrument_id: String,
    pub exchange_id: String,
    pub product_id: String,              // 品种代码
    pub open_ratio_by_money: f64,        // 开仓按金额比例
    pub open_ratio_by_volume: f64,       // 开仓按手数
    pub close_ratio_by_money: f64,       // 平仓按金额比例
    pub close_ratio_by_volume: f64,      // 平仓按手数
    pub close_today_ratio_by_money: f64, // 平今按金额比例
    pub close_today_ratio_by_volume: f64,// 平今按手数
}

/// 手续费查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommissionQueryRequest {
    /// 可选：handler 目前不使用该字段，费率是全局表；
    /// 声明为必填会让「不带参数查全部费率」直接 400 @yutiansut @quantaxis
    #[serde(default)]
    pub account_id: Option<String>,
    pub instrument_id: Option<String>,   // 为空则查询全部
}

/// 手续费统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommissionStatistics {
    pub account_id: String,
    pub total_commission: f64,           // 总手续费
    pub today_commission: f64,           // 今日手续费
    pub commission_by_instrument: Vec<InstrumentCommission>,
}

/// 按合约统计的手续费
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentCommission {
    pub instrument_id: String,
    pub open_commission: f64,
    pub close_commission: f64,
    pub close_today_commission: f64,
    pub total: f64,
}

// ==================== Phase 12: 保证金率管理 API Models ====================
// @yutiansut @quantaxis

/// 保证金率信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginRate {
    pub instrument_id: String,
    pub exchange_id: String,
    pub product_id: String,
    pub long_margin_ratio_by_money: f64,  // 多头按金额比例
    pub long_margin_ratio_by_volume: f64, // 多头按手数
    pub short_margin_ratio_by_money: f64, // 空头按金额比例
    pub short_margin_ratio_by_volume: f64,// 空头按手数
}

/// 保证金率查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginRateQueryRequest {
    /// 可选：同 CommissionQueryRequest，handler 不使用该字段
    /// @yutiansut @quantaxis
    #[serde(default)]
    pub account_id: Option<String>,
    pub instrument_id: Option<String>,
}

/// 持仓保证金详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionMarginDetail {
    pub instrument_id: String,
    pub volume_long: f64,
    pub volume_short: f64,
    pub margin_long: f64,
    pub margin_short: f64,
    pub margin_rate_long: f64,   // 实际使用的保证金率
    pub margin_rate_short: f64,
    pub last_price: f64,         // 最新价
    pub multiplier: f64,         // 合约乘数
}

/// 保证金汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginSummary {
    pub account_id: String,
    pub total_margin: f64,           // 总保证金
    pub frozen_margin: f64,          // 冻结保证金
    pub available_margin: f64,       // 可用保证金
    pub margin_ratio: f64,           // 保证金占用比例
    pub risk_degree: f64,            // 风险度 = margin / balance
    pub positions: Vec<PositionMarginDetail>,
}

// ==================== Phase 13: 账户冻结 API Models ====================
// @yutiansut @quantaxis

/// 账户状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountStatus {
    Active,      // 正常
    Frozen,      // 冻结（禁止交易）
    Suspended,   // 暂停（限制部分功能）
    Closed,      // 已注销
}

/// 冻结账户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeAccountRequest {
    pub account_id: String,
    pub reason: String,
    pub freeze_type: FreezeType,
    pub admin_token: String,
}

/// 冻结类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FreezeType {
    TradingOnly,    // 仅禁止交易
    WithdrawOnly,   // 仅禁止出金
    Full,           // 完全冻结
}

/// 解冻账户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfreezeAccountRequest {
    pub account_id: String,
    pub reason: String,
    pub admin_token: String,
}

/// 账户状态信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatusInfo {
    pub account_id: String,
    pub status: AccountStatus,
    pub freeze_type: Option<FreezeType>,
    pub freeze_reason: Option<String>,
    pub frozen_at: Option<i64>,
    pub frozen_by: Option<String>,
    /// 是否允许交易 @yutiansut @quantaxis
    #[serde(default)]
    pub can_trade: bool,
    /// 是否允许出金
    #[serde(default)]
    pub can_withdraw: bool,
}

// ==================== Phase 13: 审计日志 API Models ====================
// @yutiansut @quantaxis

/// 审计日志类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditLogType {
    Login,           // 登录
    Logout,          // 登出
    OrderSubmit,     // 下单
    OrderCancel,     // 撤单
    Deposit,         // 入金
    Withdraw,        // 出金
    Transfer,        // 银期转账
    PasswordChange,  // 密码修改
    AccountFreeze,   // 账户冻结
    AccountUnfreeze, // 账户解冻
    SettingsChange,  // 设置修改
    RiskAlert,       // 风险警报
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: i64,
    pub account_id: String,
    pub user_id: String,
    pub log_type: AuditLogType,
    pub action: String,
    pub details: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub result: AuditResult,
}

/// 审计结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditResult {
    Success,
    Failed,
    Blocked,  // 被风控阻止
}

/// 审计日志查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogQueryRequest {
    pub account_id: Option<String>,
    pub user_id: Option<String>,
    pub log_type: Option<AuditLogType>,
    // ⚠️ 时间字段用容错反序列化 @yutiansut @quantaxis
    //
    // `Option<i64>` 只容忍「字段缺失」,**不容忍「格式错误」** ——
    // `?start_time=2026-09-01 00:00:00` 会让 actix 的 Query 提取器
    // 直接 400 并抛原始 serde 消息:
    //     "Query deserialize error: invalid digit found in string"
    // 它既不说是哪个字段,也不说该传什么。而**整个查询都失败了** ——
    // 一个筛选条件写错,其余条件全部作废(fail-closed)。
    //
    // 当前不可达:前端 `audit-logs.vue:46` 用 `value-format="timestamp"`(毫秒),
    // 与本字段的 i64 匹配。但这是「靠上游恰好传对」的正确性(见 tasks/lessons.md L82),
    // 上游一次改动就会失效。改为格式错时降级 None(= 不按时间筛),
    // 而不是让整个请求失败。
    #[serde(default, deserialize_with = "lenient_opt_i64")]
    pub start_time: Option<i64>,
    #[serde(default, deserialize_with = "lenient_opt_i64")]
    pub end_time: Option<i64>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 宽容地反序列化 `Option<i64>`:接受数字、数字字符串;**其余一律当作 None**。
/// 用于查询参数 —— 一个筛选条件格式错不该让整个查询失败。@yutiansut @quantaxis
fn lenient_opt_i64<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    // 先按 Value 收下,避免类型不匹配直接报错
    let v = serde_json::Value::deserialize(d).unwrap_or(serde_json::Value::Null);
    Ok(match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    })
}

/// 审计日志查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub logs: Vec<AuditLogEntry>,
}

// ==================== Phase 13: 系统公告 API Models ====================
// @yutiansut @quantaxis

/// 公告类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnnouncementType {
    System,      // 系统公告
    Maintenance, // 维护通知
    Trading,     // 交易相关
    Risk,        // 风险提示
    Promotion,   // 活动推广
}

/// 公告优先级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnnouncementPriority {
    Low,
    Normal,
    High,
    Urgent,
}

/// 系统公告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub content: String,
    pub announcement_type: AnnouncementType,
    pub priority: AnnouncementPriority,
    pub publish_time: i64,
    pub expire_time: Option<i64>,
    pub is_active: bool,
    pub author: String,
    pub attachments: Vec<String>,  // 附件URL列表
}

/// 创建公告请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAnnouncementRequest {
    pub title: String,
    pub content: String,
    pub announcement_type: AnnouncementType,
    pub priority: AnnouncementPriority,
    /// 失效时间(**毫秒**时间戳,与 account_admin.rs:104 `current_timestamp()` 同口径)
    ///
    /// ⚠️ 前端历来发的是 `effective_until`(admin/announcements.vue:285),
    /// 后端没有 `deny_unknown_fields` → 静默丢弃 → **公告永不过期**。
    /// 加 alias 兜底;前端也已改为直接用 `expire_time`。@yutiansut @quantaxis
    #[serde(alias = "effective_until")]
    pub expire_time: Option<i64>,
    /// 生效时间(毫秒时间戳,同口径)。不传则用当前时间。
    ///
    /// ⚠️ 此前**结构体里根本没有这个字段**,而前端
    /// `admin/announcements.vue:283` 一直在发 `publish_time`(用户选的生效时间)——
    /// 没有 `deny_unknown_fields`,于是被静默丢弃,后端一律用 `current_timestamp()`。
    /// 用户设的生效时间从来没生效过。@yutiansut @quantaxis
    pub publish_time: Option<i64>,
    pub admin_token: String,
}

/// 公告查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncementQueryRequest {
    pub announcement_type: Option<AnnouncementType>,
    /// 只看生效中的公告
    ///
    /// ⚠️ 前端 4 处调用发的是 `active_only`(login.vue:258、announcements/index.vue:128、
    /// admin/announcements.vue:25 与 :191),与本字段名相反。后端没有
    /// `deny_unknown_fields`,所以该参数**永远落空** → 走
    /// `unwrap_or(true)`(account_admin.rs:804)→ 管理页的「全部状态」筛选
    /// 和 `active_only: false` 都无效,永远只显示生效中的公告。
    /// 用 serde alias 同时接受两种写法,不必改 4 处前端。@yutiansut @quantaxis
    #[serde(alias = "active_only")]
    pub only_active: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 公告列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncementListResponse {
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub announcements: Vec<Announcement>,
}

#[cfg(test)]
mod error_status_tests {
    use super::exchange_error_response;
    use crate::ExchangeError as E;

    fn status(e: E) -> u16 {
        exchange_error_response("ctx", &e).status().as_u16()
    }

    /// 锁住 `ExchangeError` → HTTP 状态码的映射。@yutiansut @quantaxis
    ///
    /// 背景:`GET /api/market/tick/{不存在的合约}` 曾返回 **500**
    /// (`market/mod.rs` 两处用 `MatchingError("Instrument not found")` 表达「不存在」,
    /// 语义在 HTTP 层丢失,只能一律 500)。改用 `NotFound` 变体后才是 404。
    /// 这个映射没有任何测试守着 —— 任何人把某个变体挪进/挪出 `_ => 500`
    /// 那条兜底臂都不会被发现。
    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(status(E::NotFound("Instrument not found: X".into())), 404);
    }

    /// 「后端主动做出的判定」必须是 4xx,不能落进 500 兜底臂。
    #[test]
    fn client_side_errors_map_to_4xx() {
        for (e, want) in [
            (E::AuthError("bad password".into()), 401),
            (E::PermissionDenied("no admin".into()), 403),
            (E::UserError("x".into()), 400),
            (E::OrderError("x".into()), 400),
            (E::AccountError("x".into()), 400),
            (E::RiskCheckFailed("x".into()), 400),
            (E::InvalidParameter("x".into()), 400),
            (E::InsufficientBalance("x".into()), 400),
        ] {
            let got = status(e);
            assert_eq!(got, want, "期望 {}, 实得 {}", want, got);
        }
    }

    /// **负对照**:真正的服务端故障仍须是 500 —— 否则一个「全部返回 404」
    /// 的实现也能让上面两个测试通过。
    #[test]
    fn server_faults_still_map_to_500() {
        for e in [
            E::StorageError("disk gone".into()),
            E::ServiceError("actor dead".into()),
            E::ConfigError("bad toml".into()),
        ] {
            assert_eq!(status(e), 500);
        }
    }

    /// 消息体不重复措辞:`NotFound` 的 Display 不加变体前缀,
    /// 否则会得到 `Not found: Instrument not found: X` 这样的三重复述。
    #[test]
    fn not_found_message_is_not_doubled() {
        let e = E::NotFound("Instrument not found: IF9999".into());
        let s = format!("{}", e);
        assert_eq!(s, "Instrument not found: IF9999", "Display 不应加变体前缀");
        assert_eq!(s.matches("not found").count(), 1, "措辞不应重复");
    }
}
