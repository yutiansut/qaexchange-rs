//! DIFF 协议数据类型定义
//!
//! 实现 DIFF 协议的完整数据类型，包括：
//! - **QIFI 复用**: 直接使用 QIFI 的 Account, Position, Order 类型
//! - **DIFF 扩展**: 新增 Quote, Kline, Tick, Notify 类型
//!
//! # 设计原则
//!
//! DIFF 协议 100% 向后兼容 QIFI/TIFI：
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      DIFF 协议                               │
//! │  ┌────────────────────┬─────────────────────────────────┐   │
//! │  │  QIFI 数据层       │   DIFF 扩展层                    │   │
//! │  │  (零成本复用)      │   (新增数据类型)                 │   │
//! │  ├────────────────────┼─────────────────────────────────┤   │
//! │  │  Account           │   Quote  (行情数据)              │   │
//! │  │  Position          │   Kline  (K线数据)               │   │
//! │  │  Order             │   Tick   (逐笔成交)              │   │
//! │  │  Trade             │   Notify (通知消息)              │   │
//! │  └────────────────────┴─────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 使用示例
//!
//! ```rust
//! use qaexchange::protocol::diff::types::{DiffAccount, Quote, Kline, Notify};
//! use qaexchange::protocol::qifi::Account;
//!
//! // DiffAccount 就是 QIFI Account（零成本）
//! let account = DiffAccount {
//!     user_id: "user123".to_string(),
//!     currency: "CNY".to_string(),
//!     balance: 100000.0,
//!     available: 95000.0,
//!     // ... 其他 QIFI 字段
//!     ..Default::default()
//! };
//!
//! // DIFF 扩展：行情数据
//! let quote = Quote {
//!     instrument_id: "SHFE.cu2512".to_string(),
//!     last_price: 75230.0,
//!     bid_price1: 75220.0,
//!     ask_price1: 75240.0,
//!     volume: 123456,
//!     // ... 其他行情字段
//!     ..Default::default()
//! };
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// QIFI 类型复用（零成本抽象）
// ============================================================================

/// DIFF 账户类型（直接复用 QIFI Account）
///
/// 100% 兼容 QIFI 的 Account 结构，包含所有字段：
/// - user_id, currency
/// - pre_balance, deposit, withdraw, static_balance
/// - close_profit, commission, premium
/// - position_profit, float_profit
/// - balance, margin, frozen_margin, available, risk_ratio
pub use qars::qaprotocol::qifi::account::Account as DiffAccount;

/// DIFF 持仓类型（直接复用 QIFI Position）
///
/// 100% 兼容 QIFI 的 Position 结构，包含所有字段：
/// - user_id, exchange_id, instrument_id
/// - volume_long, volume_short, open_price_long, open_price_short
/// - float_profit, position_profit, margin
pub use qars::qaprotocol::qifi::account::Position as DiffPosition;

/// DIFF 委托单类型（直接复用 QIFI Order）
///
/// 100% 兼容 QIFI 的 Order 结构，包含所有字段：
/// - user_id, order_id, exchange_id, instrument_id
/// - direction, offset, volume_orign, price_type, limit_price
/// - status, volume_left, frozen_margin, insert_date_time
pub use qars::qaprotocol::qifi::account::Order as DiffOrder;

/// DIFF 成交记录类型（直接复用 QIFI Trade）
///
/// 100% 兼容 QIFI 的 Trade 结构，包含所有字段：
/// - user_id, order_id, trade_id, exchange_id, instrument_id
/// - direction, offset, volume, price, trade_date_time, commission
pub use qars::qaprotocol::qifi::account::Trade as DiffTrade;

// ============================================================================
// DIFF 扩展类型（新增数据类型）
// ============================================================================

/// 行情数据
///
/// 实时行情信息，包含最新价、盘口、成交量等。
///
/// # 字段说明
///
/// - **基础信息**: instrument_id, volume_multiple, price_tick
/// - **盘口数据**: bid_price1/volume1, ask_price1/volume1
/// - **价格信息**: last_price, open, high, low, close
/// - **成交信息**: volume, amount, open_interest
/// - **涨跌停**: upper_limit, lower_limit
/// - **结算价**: pre_settlement, settlement
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Quote {
    /// 合约代码（如 "SHFE.cu2512"）
    pub instrument_id: String,

    /// 合约基础信息
    pub volume_multiple: i32, // 合约乘数
    pub price_tick: f64, // 最小变动价位
    pub price_decs: i32, // 价格小数位数

    /// 交易限制
    pub max_market_order_volume: i32, // 市价单最大下单量
    pub min_market_order_volume: i32, // 市价单最小下单量
    pub max_limit_order_volume: i32,  // 限价单最大下单量
    pub min_limit_order_volume: i32,  // 限价单最小下单量

    /// 保证金和手续费
    pub margin: f64, // 保证金率
    pub commission: f64, // 手续费率

    /// 行情时间
    pub datetime: String, // 行情时间（ISO 8601 格式）

    /// 盘口数据（一档）
    pub ask_price1: f64, // 卖一价
    pub ask_volume1: i64, // 卖一量
    pub bid_price1: f64,  // 买一价
    pub bid_volume1: i64, // 买一量

    /// 价格信息
    pub last_price: f64, // 最新价
    pub highest: f64,        // 最高价
    pub lowest: f64,         // 最低价
    pub average: f64,        // 均价
    pub pre_close: f64,      // 昨收价
    pub pre_settlement: f64, // 昨结算价
    pub open: f64,           // 开盘价
    pub close: f64,          // 收盘价
    pub settlement: f64,     // 结算价

    /// 涨跌停
    pub lower_limit: f64, // 跌停价
    pub upper_limit: f64, // 涨停价

    /// 成交信息
    pub amount: f64, // 成交额
    pub volume: i64,            // 成交量
    pub open_interest: i64,     // 持仓量
    pub pre_open_interest: i64, // 昨持仓量
}

/// K线柱
///
/// 单根K线数据，包含 OHLCV 和持仓量信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineBar {
    /// K线时间戳（纳秒）
    pub datetime: i64,

    /// OHLC 价格
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,

    /// 成交量
    pub volume: i64,

    /// 持仓量
    pub open_oi: i64, // 开盘持仓
    pub close_oi: i64, // 收盘持仓
}

/// K线数据
///
/// 多根K线的集合，用于图表展示。
///
/// # 字段说明
///
/// - `last_id`: 最后一根K线的ID（用于增量更新）
/// - `data`: K线柱集合（key = K线ID，value = K线柱）
/// - `binding`: K线绑定关系（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    /// 最后一根K线的ID
    pub last_id: i64,

    /// K线柱数据（key = K线ID，value = K线柱）
    pub data: HashMap<String, KlineBar>,

    /// K线绑定关系（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<HashMap<String, HashMap<String, i64>>>,
}

/// Tick数据
///
/// 逐笔成交数据（Level1 行情）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickBar {
    /// Tick 时间戳（纳秒）
    pub datetime: i64,

    /// 价格信息
    pub last_price: f64, // 最新价
    pub average: f64, // 均价
    pub highest: f64, // 最高价
    pub lowest: f64,  // 最低价

    /// 盘口
    pub bid_price1: f64,
    pub ask_price1: f64,
    pub bid_volume1: i64,
    pub ask_volume1: i64,

    /// 成交信息
    pub volume: i64,
    pub amount: f64,
    pub open_interest: i64,
}

/// Tick序列数据
///
/// 多个 Tick 的集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickSeries {
    /// 最后一个 Tick 的ID
    pub last_id: i64,

    /// Tick 数据（key = Tick ID，value = Tick柱）
    pub data: HashMap<String, TickBar>,
}

/// 因子数据
///
/// 实时计算的技术因子值（MA, EMA, RSI, MACD 等）。
///
/// # 字段说明
///
/// - `period`: K线周期（0=日线, 4=1min, 5=5min, 6=15min, 7=30min, 8=60min）
/// - `timestamp`: 计算时间戳（毫秒）
/// - `values`: 因子值映射（key = 因子ID，value = 因子值）
///
/// # 因子ID 命名规范
///
/// - MA系列: "ma5", "ma10", "ma20", "ma60"
/// - EMA系列: "ema12", "ema26"
/// - RSI: "rsi14"
/// - MACD: "macd_dif", "macd_dea", "macd_hist"
///
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactorData {
    /// K线周期
    pub period: i32,

    /// 计算时间戳（毫秒）
    pub timestamp: i64,

    /// 因子值（key = 因子ID，value = 因子值）
    pub values: HashMap<String, f64>,
}

/// 通知数据
///
/// 系统通知、错误消息、警告等。
///
/// # 字段说明
///
/// - `type`: 消息类型（MESSAGE, TEXT, HTML）
/// - `level`: 消息级别（INFO, WARNING, ERROR）
/// - `code`: 错误码或消息码
/// - `content`: 消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notify {
    /// 消息类型
    pub r#type: String, // MESSAGE / TEXT / HTML

    /// 消息级别
    pub level: String, // INFO / WARNING / ERROR

    /// 消息码
    pub code: i32,

    /// 消息内容
    pub content: String,
}

// ============================================================================
// 业务快照（完整数据结构）
// ============================================================================

/// 用户交易数据
///
/// 包含用户的所有交易相关数据（账户、持仓、委托、成交等）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserTradeData {
    /// 用户ID
    pub user_id: String,

    /// 资金账户
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts: Option<HashMap<String, DiffAccount>>,

    /// 持仓
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<HashMap<String, DiffPosition>>,

    /// 委托单
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<HashMap<String, DiffOrder>>,

    /// 成交记录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trades: Option<HashMap<String, DiffTrade>>,

    /// 签约银行
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banks: Option<HashMap<String, BankData>>,

    /// 转账记录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfers: Option<HashMap<String, TransferData>>,
}

/// 银行数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankData {
    pub id: String,
    pub name: String,
}

/// 转账记录数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferData {
    pub datetime: i64,
    pub currency: String,
    pub amount: f64,
    pub error_id: i32,
    pub error_msg: String,
}

/// 业务快照
///
/// 完整的业务数据快照，包含交易、行情、K线、Tick、通知等所有数据。
///
/// # 使用示例
///
/// ```rust
/// use qaexchange::protocol::diff::types::BusinessSnapshot;
/// use serde_json::json;
///
/// let snapshot = BusinessSnapshot {
///     trade: Some(json!({
///         "user123": {
///             "accounts": {
///                 "ACC001": {
///                     "balance": 100000.0
///                 }
///             }
///         }
///     }).as_object().unwrap().iter().map(|(k, v)| {
///         (k.clone(), serde_json::from_value(v.clone()).unwrap())
///     }).collect()),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessSnapshot {
    /// 交易数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade: Option<HashMap<String, UserTradeData>>,

    /// 行情数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quotes: Option<HashMap<String, Quote>>,

    /// K线数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub klines: Option<HashMap<String, HashMap<String, Kline>>>,

    /// Tick数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<HashMap<String, TickSeries>>,

    /// 因子数据（实时计算的技术指标）
    /// key = instrument_id, value = FactorData
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factors: Option<HashMap<String, FactorData>>,

    /// 通知数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<HashMap<String, Notify>>,
}

// ============================================================================
// 常量定义
// ============================================================================

/// 消息类型常量
pub mod message_type {
    pub const MESSAGE: &str = "MESSAGE";
    pub const TEXT: &str = "TEXT";
    pub const HTML: &str = "HTML";
}

/// 消息级别常量
pub mod message_level {
    pub const INFO: &str = "INFO";
    pub const WARNING: &str = "WARNING";
    pub const ERROR: &str = "ERROR";
}

/// 委托单状态常量
pub mod order_status {
    pub const ALIVE: &str = "ALIVE"; // 未完成
    pub const FINISHED: &str = "FINISHED"; // 已完成
}

/// 买卖方向常量
pub mod direction {
    pub const BUY: &str = "BUY";
    pub const SELL: &str = "SELL";
}

/// 开平仓常量
pub mod offset {
    pub const OPEN: &str = "OPEN";
    pub const CLOSE: &str = "CLOSE";
    pub const CLOSE_TODAY: &str = "CLOSE_TODAY";
    pub const CLOSE_YESTERDAY: &str = "CLOSE_YESTERDAY";
}

/// 委托价格类型常量
pub mod price_type {
    pub const LIMIT: &str = "LIMIT"; // 限价单
    pub const MARKET: &str = "MARKET"; // 市价单
    pub const ANY: &str = "ANY"; // 任意价
}

// ============================================================================
// 辅助函数
// ============================================================================

impl Quote {
    /// 创建空的行情数据
    pub fn empty(instrument_id: &str) -> Self {
        Self {
            instrument_id: instrument_id.to_string(),
            ..Default::default()
        }
    }

    /// 检查是否为空行情
    pub fn is_empty(&self) -> bool {
        self.last_price == 0.0 && self.volume == 0
    }
}

impl Notify {
    /// 创建信息通知
    pub fn info(content: &str) -> Self {
        Self {
            r#type: message_type::MESSAGE.to_string(),
            level: message_level::INFO.to_string(),
            code: 0,
            content: content.to_string(),
        }
    }

    /// 创建警告通知
    pub fn warning(code: i32, content: &str) -> Self {
        Self {
            r#type: message_type::MESSAGE.to_string(),
            level: message_level::WARNING.to_string(),
            code,
            content: content.to_string(),
        }
    }

    /// 创建错误通知
    pub fn error(code: i32, content: &str) -> Self {
        Self {
            r#type: message_type::MESSAGE.to_string(),
            level: message_level::ERROR.to_string(),
            code,
            content: content.to_string(),
        }
    }
}

impl BusinessSnapshot {
    /// 创建空的业务快照
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查快照是否为空
    pub fn is_empty(&self) -> bool {
        self.trade.is_none()
            && self.quotes.is_none()
            && self.klines.is_none()
            && self.ticks.is_none()
            && self.factors.is_none()
            && self.notify.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qifi_type_alias() {
        // 验证 DiffAccount 就是 QIFI Account
        let account = DiffAccount {
            user_id: "user123".to_string(),
            currency: "CNY".to_string(),
            balance: 100000.0,
            available: 95000.0,
            ..Default::default()
        };

        assert_eq!(account.user_id, "user123");
        assert_eq!(account.balance, 100000.0);
    }

    #[test]
    fn test_quote_creation() {
        let quote = Quote {
            instrument_id: "SHFE.cu2512".to_string(),
            last_price: 75230.0,
            bid_price1: 75220.0,
            ask_price1: 75240.0,
            volume: 123456,
            ..Default::default()
        };

        assert_eq!(quote.instrument_id, "SHFE.cu2512");
        assert_eq!(quote.last_price, 75230.0);
        assert!(!quote.is_empty());
    }

    #[test]
    fn test_quote_empty() {
        let quote = Quote::empty("SHFE.cu2512");
        assert!(quote.is_empty());
    }

    #[test]
    fn test_notify_helpers() {
        let info = Notify::info("操作成功");
        assert_eq!(info.level, message_level::INFO);
        assert_eq!(info.code, 0);

        let warning = Notify::warning(1001, "余额不足");
        assert_eq!(warning.level, message_level::WARNING);
        assert_eq!(warning.code, 1001);

        let error = Notify::error(5000, "系统错误");
        assert_eq!(error.level, message_level::ERROR);
        assert_eq!(error.code, 5000);
    }

    #[test]
    fn test_business_snapshot_empty() {
        let snapshot = BusinessSnapshot::new();
        assert!(snapshot.is_empty());

        let mut snapshot2 = BusinessSnapshot::default();
        snapshot2.quotes = Some(HashMap::new());
        assert!(!snapshot2.is_empty());
    }

    #[test]
    fn test_kline_bar() {
        let bar = KlineBar {
            datetime: 1704067200000000000, // 2024-01-01 00:00:00
            open: 75000.0,
            high: 75500.0,
            low: 74800.0,
            close: 75200.0,
            volume: 10000,
            open_oi: 50000,
            close_oi: 51000,
        };

        assert_eq!(bar.open, 75000.0);
        assert_eq!(bar.close, 75200.0);
    }

    #[test]
    fn test_tick_bar() {
        let tick = TickBar {
            datetime: 1704067200000000000,
            last_price: 75230.0,
            average: 75200.0,
            highest: 75500.0,
            lowest: 74800.0,
            bid_price1: 75220.0,
            ask_price1: 75240.0,
            bid_volume1: 100,
            ask_volume1: 150,
            volume: 5000,
            amount: 376000000.0,
            open_interest: 50000,
        };

        assert_eq!(tick.last_price, 75230.0);
        assert_eq!(tick.volume, 5000);
    }

    #[test]
    fn test_user_trade_data() {
        let mut accounts = HashMap::new();
        accounts.insert(
            "ACC001".to_string(),
            DiffAccount {
                user_id: "user123".to_string(),
                currency: "CNY".to_string(),
                balance: 100000.0,
                ..Default::default()
            },
        );

        let user_data = UserTradeData {
            user_id: "user123".to_string(),
            accounts: Some(accounts),
            ..Default::default()
        };

        assert_eq!(user_data.user_id, "user123");
        assert!(user_data.accounts.is_some());
        assert_eq!(user_data.accounts.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_serialization() {
        // 测试序列化和反序列化
        let quote = Quote {
            instrument_id: "SHFE.cu2512".to_string(),
            last_price: 75230.0,
            ..Default::default()
        };

        let json = serde_json::to_string(&quote).unwrap();
        let deserialized: Quote = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.instrument_id, "SHFE.cu2512");
        assert_eq!(deserialized.last_price, 75230.0);
    }
}
