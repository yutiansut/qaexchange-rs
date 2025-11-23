//! 交易所类型定义
//!
//! 区分交易所内部记录和交易所→账户回报

use serde::{Deserialize, Serialize};

/// 交易所回报类型（推送给账户的5种回报）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExchangeResponse {
    /// 订单接受
    OrderAccepted {
        exchange_order_id: i64,
        instrument_id: String,
        timestamp: i64,
    },

    /// 订单拒绝
    OrderRejected {
        exchange_order_id: i64,
        instrument_id: String,
        reason: String,
        timestamp: i64,
    },

    /// 成交回报（不判断全部/部分，由账户自己判断）
    Trade {
        trade_id: i64,
        exchange_order_id: i64,
        instrument_id: String,
        volume: f64,
        price: f64,
        timestamp: i64,
    },

    /// 撤单成功
    CancelAccepted {
        exchange_order_id: i64,
        instrument_id: String,
        timestamp: i64,
    },

    /// 撤单拒绝
    CancelRejected {
        exchange_order_id: i64,
        instrument_id: String,
        reason: String,
        timestamp: i64,
    },
}

/// 交易所内部逐笔委托记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeOrderRecord {
    /// 交易所ID
    pub exchange: String,

    /// 合约ID
    pub instrument: String,

    /// 交易所订单号（自增i64）
    pub exchange_order_id: i64,

    /// 方向（BUY/SELL）
    pub direction: String,

    /// 开平标志（OPEN/CLOSE）
    pub offset: String,

    /// 价格类型（LIMIT/MARKET）
    pub price_type: String,

    /// 委托价格
    pub price: f64,

    /// 委托量
    pub volume: f64,

    /// 委托时间（纳秒）
    pub time: i64,

    /// 内部订单ID（用于映射回用户）
    pub internal_order_id: String,

    /// 用户ID（用于映射回用户）
    pub user_id: String,
}

/// 交易所内部逐笔成交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeTradeRecord {
    /// 交易所ID
    pub exchange: String,

    /// 合约ID
    pub instrument: String,

    /// 买方交易所订单号
    pub buy_exchange_order_id: i64,

    /// 卖方交易所订单号
    pub sell_exchange_order_id: i64,

    /// 成交价格
    pub deal_price: f64,

    /// 成交量
    pub deal_volume: f64,

    /// 成交时间（纳秒）
    pub time: i64,

    /// 成交ID（自增i64）
    pub trade_id: i64,
}

impl ExchangeResponse {
    /// 获取交易所订单号
    pub fn exchange_order_id(&self) -> i64 {
        match self {
            ExchangeResponse::OrderAccepted {
                exchange_order_id, ..
            } => *exchange_order_id,
            ExchangeResponse::OrderRejected {
                exchange_order_id, ..
            } => *exchange_order_id,
            ExchangeResponse::Trade {
                exchange_order_id, ..
            } => *exchange_order_id,
            ExchangeResponse::CancelAccepted {
                exchange_order_id, ..
            } => *exchange_order_id,
            ExchangeResponse::CancelRejected {
                exchange_order_id, ..
            } => *exchange_order_id,
        }
    }

    /// 获取合约ID
    pub fn instrument_id(&self) -> &str {
        match self {
            ExchangeResponse::OrderAccepted { instrument_id, .. } => instrument_id,
            ExchangeResponse::OrderRejected { instrument_id, .. } => instrument_id,
            ExchangeResponse::Trade { instrument_id, .. } => instrument_id,
            ExchangeResponse::CancelAccepted { instrument_id, .. } => instrument_id,
            ExchangeResponse::CancelRejected { instrument_id, .. } => instrument_id,
        }
    }

    /// 获取时间戳
    pub fn timestamp(&self) -> i64 {
        match self {
            ExchangeResponse::OrderAccepted { timestamp, .. } => *timestamp,
            ExchangeResponse::OrderRejected { timestamp, .. } => *timestamp,
            ExchangeResponse::Trade { timestamp, .. } => *timestamp,
            ExchangeResponse::CancelAccepted { timestamp, .. } => *timestamp,
            ExchangeResponse::CancelRejected { timestamp, .. } => *timestamp,
        }
    }
}
