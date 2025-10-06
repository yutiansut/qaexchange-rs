//! 撮合引擎模块
//!
//! 复用 qars::qamarket::matchengine::Orderbook 并提供交易所特定扩展

// 重导出 qars 撮合引擎核心类型
pub use qars::qamarket::matchengine::{
    domain::{Order as MatchOrder, OrderDirection, OrderType, OrderTrait},
    orderbook::{Orderbook, Success, Failed, TradingState, OrderProcessingResult},
    orders::{self, OrderRequest},
};

/// 交易所撮合引擎封装
pub mod engine;

/// 集合竞价增强
pub mod auction;

/// 成交记录器
pub mod trade_recorder;

/// 撮合引擎核心（独立进程版本）
pub mod core;
