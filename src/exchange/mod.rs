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

// 重导出核心类型
pub use account_mgr::AccountManager;
pub use capital_mgr::CapitalManager;
pub use order_router::OrderRouter;
pub use trade_gateway::TradeGateway;
pub use settlement::SettlementEngine;
pub use instrument_registry::InstrumentRegistry;
