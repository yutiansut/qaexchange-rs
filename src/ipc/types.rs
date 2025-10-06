//! IPC 消息类型定义
//!
//! 这些类型通过共享内存传递，必须满足：
//! 1. Plain Old Data (POD) - 可以直接内存映射
//! 2. 固定大小 - 便于共享内存分配
//! 3. 无堆分配 - 避免跨进程指针问题

use std::mem::MaybeUninit;

/// 交易通知（共享内存版本）
///
/// 注意：所有字符串字段使用固定大小数组，避免堆分配
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IpcNotification {
    /// 通知类型：0=成交, 1=订单状态, 2=账户更新
    pub notification_type: u8,

    /// 填充字节（对齐）
    _padding: [u8; 7],

    /// 通知数据（根据类型解释）
    pub data: IpcNotificationData,
}

impl std::fmt::Debug for IpcNotification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcNotification")
            .field("notification_type", &self.notification_type)
            .finish()
    }
}

/// 通知数据联合体
#[repr(C)]
#[derive(Clone, Copy)]
pub union IpcNotificationData {
    pub trade: IpcTrade,
    pub order_status: IpcOrderStatus,
    pub account: IpcAccountUpdate,
}

/// 成交通知
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcTrade {
    /// 成交ID
    pub trade_id: [u8; 64],

    /// 订单ID
    pub order_id: [u8; 64],

    /// 合约ID
    pub instrument_id: [u8; 32],

    /// 方向：0=买, 1=卖
    pub direction: u8,

    /// 开平：0=开仓, 1=平仓
    pub offset: u8,

    /// 填充
    _padding: [u8; 6],

    /// 成交价格
    pub price: f64,

    /// 成交数量
    pub volume: i64,

    /// 时间戳（纳秒）
    pub timestamp: i64,
}

/// 订单状态通知
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcOrderStatus {
    /// 订单ID
    pub order_id: [u8; 64],

    /// 状态：0=待成交, 1=部分成交, 2=全部成交, 3=已撤销
    pub status: u8,

    /// 填充
    _padding: [u8; 7],

    /// 已成交数量
    pub filled_volume: i64,

    /// 剩余数量
    pub remaining_volume: i64,

    /// 时间戳（纳秒）
    pub timestamp: i64,
}

/// 账户更新通知
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcAccountUpdate {
    /// 用户ID
    pub user_id: [u8; 64],

    /// 余额
    pub balance: f64,

    /// 可用资金
    pub available: f64,

    /// 保证金占用
    pub margin: f64,

    /// 持仓盈亏
    pub profit: f64,

    /// 风险度
    pub risk_ratio: f64,

    /// 时间戳（纳秒）
    pub timestamp: i64,
}

/// 市场数据（共享内存版本）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IpcMarketData {
    /// 数据类型：0=订单簿快照, 1=Tick成交, 2=最新价
    pub data_type: u8,

    /// 填充
    _padding: [u8; 7],

    /// 合约ID
    pub instrument_id: [u8; 32],

    /// 时间戳（纳秒）
    pub timestamp: i64,

    /// 数据内容
    pub data: IpcMarketDataContent,
}

impl std::fmt::Debug for IpcMarketData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcMarketData")
            .field("data_type", &self.data_type)
            .field("timestamp", &self.timestamp)
            .finish()
    }
}

/// 市场数据内容联合体
#[repr(C)]
#[derive(Clone, Copy)]
pub union IpcMarketDataContent {
    pub orderbook: IpcOrderBook,
    pub tick: IpcTick,
    pub last_price: IpcLastPrice,
}

/// 订单簿快照（5档）
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcOrderBook {
    /// 买盘价格
    pub bid_prices: [f64; 5],

    /// 买盘数量
    pub bid_volumes: [i64; 5],

    /// 卖盘价格
    pub ask_prices: [f64; 5],

    /// 卖盘数量
    pub ask_volumes: [i64; 5],
}

/// Tick成交数据
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcTick {
    /// 成交价
    pub price: f64,

    /// 成交量
    pub volume: i64,

    /// 方向：0=买, 1=卖
    pub direction: u8,

    /// 填充
    _padding: [u8; 7],
}

/// 最新价
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IpcLastPrice {
    /// 最新价
    pub price: f64,
}

// 辅助函数：字符串转固定数组
pub fn str_to_fixed_array<const N: usize>(s: &str) -> [u8; N] {
    let mut arr = [0u8; N];
    let bytes = s.as_bytes();
    let len = bytes.len().min(N - 1); // 保留一个字节作为终止符
    arr[..len].copy_from_slice(&bytes[..len]);
    arr
}

// 辅助函数：固定数组转字符串
pub fn fixed_array_to_str(arr: &[u8]) -> String {
    // 找到第一个 0 字节
    let end = arr.iter().position(|&b| b == 0).unwrap_or(arr.len());
    String::from_utf8_lossy(&arr[..end]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_notification_size() {
        // 确保 POD 类型
        assert_eq!(std::mem::size_of::<IpcNotification>(), 256);

        // 确保是 Copy 类型
        fn assert_copy<T: Copy>() {}
        assert_copy::<IpcNotification>();
    }

    #[test]
    fn test_string_conversion() {
        let s = "test_order_123";
        let arr = str_to_fixed_array::<64>(s);
        let result = fixed_array_to_str(&arr);
        assert_eq!(result, s);
    }
}
