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
    pub user_id: String,                    // 用户身份（用于验证）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,         // 交易账户（推荐明确传递）✨
    pub instrument_id: String,
    pub direction: String,                  // BUY/SELL
    pub offset: String,                     // OPEN/CLOSE/CLOSETODAY
    pub volume: f64,
    pub price: f64,
    pub order_type: String,                 // LIMIT/MARKET
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
    pub user_id: String,                    // 用户身份（用于验证）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,         // 交易账户（推荐明确传递）✨
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
    pub account_id: String,         // 账户ID（用于平仓时指定账户）
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
