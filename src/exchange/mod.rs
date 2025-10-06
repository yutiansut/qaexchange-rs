//! 交易所核心业务模块
//!
//! 实现交易所的核心业务逻辑

/// 账户管理中心
pub mod account_mgr;

/// 资金管理
pub mod capital_mgr;

/// 订单路由
pub mod order_router;

/// 成交回报网关
pub mod trade_gateway;

/// 结算系统
pub mod settlement;

/// 合约注册表
pub mod instrument_registry;

/// 用户管理
pub mod user_mgr;

/// 交易所类型定义（内部记录 + 回报）
pub mod exchange_types;

/// 交易所ID生成器
pub mod id_generator;

// 重导出核心类型
pub use account_mgr::AccountManager;
pub use capital_mgr::{CapitalManager, FundTransaction, TransactionType, TransactionStatus};
pub use order_router::OrderRouter;
pub use trade_gateway::{TradeGateway, Notification};
pub use settlement::SettlementEngine;
pub use instrument_registry::InstrumentRegistry;
pub use user_mgr::{UserManager, RegisterRequest, LoginRequest, LoginResponse};
pub use exchange_types::{ExchangeResponse, ExchangeOrderRecord, ExchangeTradeRecord};
pub use id_generator::ExchangeIdGenerator;
