//! HTTP API 请求/响应模型

use serde::{Deserialize, Serialize};

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
    pub cost_long: f64,
    pub cost_short: f64,
    pub profit_long: f64,
    pub profit_short: f64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    pub user_id: String,
    pub amount: f64,
}

/// 出金请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawRequest {
    pub user_id: String,
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
    pub account_id: String,
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
    pub account_id: String,
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
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
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
    pub expire_time: Option<i64>,
    pub admin_token: String,
}

/// 公告查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncementQueryRequest {
    pub announcement_type: Option<AnnouncementType>,
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
