//! WebSocket 消息协议定义

use serde::{Deserialize, Serialize};

/// 客户端发送的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// 认证
    Auth { user_id: String, token: String },

    /// 订阅行情
    Subscribe {
        channels: Vec<String>,    // ["trade", "orderbook", "ticker"]
        instruments: Vec<String>, // ["IX2301", "IF2301"]
    },

    /// 取消订阅
    Unsubscribe {
        channels: Vec<String>,
        instruments: Vec<String>,
    },

    /// 提交订单
    SubmitOrder {
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>, // 交易账户（推荐明确传递）✨
        instrument_id: String,
        direction: String, // BUY/SELL
        offset: String,    // OPEN/CLOSE
        volume: f64,
        price: f64,
        order_type: String, // LIMIT/MARKET
    },

    /// 撤单
    CancelOrder {
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>, // 交易账户（推荐明确传递）✨
        order_id: String,
    },

    /// 查询订单
    QueryOrder { order_id: String },

    /// 查询账户
    QueryAccount,

    /// 查询持仓
    QueryPosition { instrument_id: Option<String> },

    /// Ping（心跳）
    Ping,
}

/// 服务端发送的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// 认证响应
    AuthResponse {
        success: bool,
        user_id: String,
        message: String,
    },

    /// 订阅响应
    SubscribeResponse {
        success: bool,
        channels: Vec<String>,
        instruments: Vec<String>,
        message: String,
    },

    /// 订单提交响应
    OrderResponse {
        success: bool,
        order_id: Option<String>,
        error_code: Option<u32>,
        error_message: Option<String>,
    },

    /// 成交推送
    Trade {
        trade_id: String,
        order_id: String,
        instrument_id: String,
        direction: String,
        offset: String,
        price: f64,
        volume: f64,
        timestamp: i64,
    },

    /// 订单状态推送（交易所回报格式）
    OrderStatus {
        order_id: String,
        exchange_id: String,
        instrument_id: String,
        exchange_order_id: String,
        direction: String,
        offset: String,
        price_type: String,
        volume: f64, // 本次成交量或委托量
        price: f64,  // 价格
        status: String,
        timestamp: i64,
    },

    /// 账户更新推送
    AccountUpdate {
        balance: f64,
        available: f64,
        frozen: f64,
        margin: f64,
        profit: f64,
        risk_ratio: f64,
        timestamp: i64,
    },

    /// 订单簿推送（Level2）
    OrderBook {
        instrument_id: String,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        timestamp: i64,
    },

    /// 逐笔成交推送
    Ticker {
        instrument_id: String,
        last_price: f64,
        volume: f64,
        timestamp: i64,
    },

    /// 查询响应
    QueryResponse {
        request_type: String,
        data: serde_json::Value,
    },

    /// 错误消息
    Error { code: u32, message: String },

    /// Pong（心跳响应）
    Pong,
}

/// 价格档位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub volume: f64,
    pub order_count: u32,
}

/// WebSocket 帧类型
#[derive(Debug)]
pub enum WsFrame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<String>),
}
