//! 核心模块 - 复用 QARS 账户/订单/持仓系统
//!
//! 本模块作为对 qars 核心功能的封装和扩展

// 直接重导出 qars 的核心类型
pub use qars::qaaccount::account::QA_Account;
pub use qars::qaaccount::order::{QAOrder, QAOrderExt};
pub use qars::qaaccount::position::QA_Position;

// 重导出 QIFI 协议
pub use qars::qaprotocol::qifi::account::{Account, Frozen, Order, Position, Trade, QIFI};

/// 账户管理器的扩展功能
pub mod account_ext;

/// 订单管理器的扩展功能
pub mod order_ext;
