//! IPC 消息协议
//!
//! 用于进程间通信的零拷贝消息定义
//! 所有结构体必须：
//! 1. #[repr(C)] - 保证内存布局稳定
//! 2. Clone + Copy - 可直接拷贝到共享内存
//! 3. 固定大小 - 避免动态分配

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// 订单请求（从网关发送到撮合引擎）
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderRequest {
    /// 订单ID（UUID格式需要40字节：36字符+终止符+对齐）
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],

    /// 用户ID
    pub user_id: [u8; 32],

    /// 合约代码
    pub instrument_id: [u8; 16],

    /// 方向：0=BUY, 1=SELL
    pub direction: u8,

    /// 开平：0=OPEN, 1=CLOSE, 2=CLOSETODAY
    pub offset: u8,

    /// 价格类型：0=LIMIT, 1=MARKET
    pub order_type: u8,

    /// 预留字段
    pub _reserved: u8,

    /// 价格
    pub price: f64,

    /// 数量
    pub volume: f64,

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 网关ID（用于回报路由）
    pub gateway_id: u32,

    /// 会话ID（用于回报路由）
    pub session_id: u32,
}

impl OrderRequest {
    pub fn new(
        order_id: &str,
        user_id: &str,
        instrument_id: &str,
        direction: OrderDirection,
        offset: OrderOffset,
        price: f64,
        volume: f64,
    ) -> Self {
        let mut req = Self {
            order_id: [0; 40],
            user_id: [0; 32],
            instrument_id: [0; 16],
            direction: direction as u8,
            offset: offset as u8,
            order_type: 0, // LIMIT
            _reserved: 0,
            price,
            volume,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            gateway_id: 0,
            session_id: 0,
        };

        // 复制字符串到固定数组
        copy_str_to_array(order_id, &mut req.order_id);
        copy_str_to_array(user_id, &mut req.user_id);
        copy_str_to_array(instrument_id, &mut req.instrument_id);

        req
    }
}

/// 成交回报（从撮合引擎发送到账户系统和网关）
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeReport {
    /// 成交ID（交易所生成，全局唯一）
    pub trade_id: [u8; 32],

    /// 账户订单ID（用于账户系统匹配 dailyorders，UUID需要40字节）
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],

    /// 交易所订单ID（交易所生成，全局唯一，用于行情推送）
    pub exchange_order_id: [u8; 32],

    /// 用户ID
    pub user_id: [u8; 32],

    /// 合约代码
    pub instrument_id: [u8; 16],

    /// 方向：0=BUY, 1=SELL
    pub direction: u8,

    /// 开平：0=OPEN, 1=CLOSE
    pub offset: u8,

    /// 成交类型：0=完全成交, 1=部分成交
    pub fill_type: u8,

    /// 预留
    pub _reserved: u8,

    /// 成交价格
    pub price: f64,

    /// 成交数量
    pub volume: f64,

    /// 手续费
    pub commission: f64,

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 对手订单ID（用于调试）
    pub opposite_order_id: [u8; 32],

    /// 网关ID（回报路由）
    pub gateway_id: u32,

    /// 会话ID（回报路由）
    pub session_id: u32,
}

/// 订单簿快照（从撮合引擎发送到行情系统）
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    /// 合约代码
    pub instrument_id: [u8; 16],

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 最新价
    pub last_price: f64,

    /// 买档（最多10档）
    pub bids: [PriceLevel; 10],

    /// 卖档（最多10档）
    pub asks: [PriceLevel; 10],

    /// 有效买档数量
    pub bid_count: u8,

    /// 有效卖档数量
    pub ask_count: u8,

    /// 预留
    pub _reserved: [u8; 6],
}

/// 价格档位
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PriceLevel {
    /// 价格
    pub price: f64,

    /// 数量
    pub volume: f64,

    /// 订单数
    pub order_count: u32,

    /// 预留
    pub _reserved: u32,
}

/// 订单方向
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OrderDirection {
    BUY = 0,
    SELL = 1,
}

/// 开平标志
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OrderOffset {
    OPEN = 0,
    CLOSE = 1,
    CLOSETODAY = 2,
}

/// 订单确认消息（从撮合引擎发送到账户系统，用于 sim 模式的 on_order_confirm）
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderAccepted {
    /// 账户订单ID（用于匹配 dailyorders，UUID需要40字节）
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],

    /// 交易所订单ID（交易所生成，全局唯一）
    pub exchange_order_id: [u8; 32],

    /// 用户ID
    pub user_id: [u8; 32],

    /// 合约代码
    pub instrument_id: [u8; 16],

    /// 时间戳
    pub timestamp: i64,

    /// 网关ID
    pub gateway_id: u32,

    /// 会话ID
    pub session_id: u32,
}

/// 订单状态通知（从撮合引擎发送到网关）
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderStatusNotify {
    /// 订单ID（UUID需要40字节）
    #[serde(with = "BigArray")]
    pub order_id: [u8; 40],

    /// 用户ID
    pub user_id: [u8; 32],

    /// 状态：0=已接受, 1=部分成交, 2=全部成交, 3=已撤销, 4=已拒绝
    pub status: u8,

    /// 预留
    pub _reserved: [u8; 7],

    /// 已成交数量
    pub filled_volume: f64,

    /// 剩余数量
    pub remaining_volume: f64,

    /// 时间戳
    pub timestamp: i64,

    /// 网关ID
    pub gateway_id: u32,

    /// 会话ID
    pub session_id: u32,
}

/// 辅助函数：复制字符串到固定数组
fn copy_str_to_array(src: &str, dst: &mut [u8]) {
    let bytes = src.as_bytes();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_request_size() {
        // 验证消息大小合理（用于共享内存分配）
        assert_eq!(std::mem::size_of::<OrderRequest>(), 128);
    }

    #[test]
    fn test_trade_report_size() {
        assert!(std::mem::size_of::<TradeReport>() <= 256);
    }

    #[test]
    fn test_orderbook_snapshot_size() {
        // Level2 快照不应超过 1KB
        assert!(std::mem::size_of::<OrderbookSnapshot>() <= 1024);
    }
}
