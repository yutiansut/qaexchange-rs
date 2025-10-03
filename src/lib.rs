//! # QAEXCHANGE-RS
//!
//! 高性能量化交易所系统 - 基于 QARS 核心架构
//!
//! ## 核心能力
//!
//! - **账户管理**: 开户/入金/出金/查询 (复用 qars::qaaccount)
//! - **订单系统**: 下单/撤单/订单路由 (复用 qars::qaaccount::QAOrder)
//! - **撮合引擎**: 价格时间优先/集合竞价/连续交易 (复用 qars::qamarket::matchengine)
//! - **成交回报**: 实时成交推送/账户更新
//! - **行情推送**: Level1/Level2/逐笔成交 (复用 qars::qadata::broadcast_hub)
//! - **结算系统**: 日终结算/盯市盈亏/强平处理
//! - **风控系统**: 盘前风控/持仓限额/自成交防范
//! - **对外服务**: WebSocket + HTTP API (基于 Actix-web)
//!
//! ## 架构设计
//!
//! ```text
//! 客户端 (WebSocket/HTTP)
//!     ↓
//! Service Layer (service/)
//!     ↓
//! Exchange Core (exchange/)
//!     ↓
//! Matching Engine (matching/) ← 复用 qars::qamarket::matchengine
//!     ↓
//! Account System (core/) ← 复用 qars::qaaccount
//!     ↓
//! Storage (storage/) ← 复用 qars::qaconnector
//! ```
//!
//! ## 性能目标
//!
//! - 订单吞吐量: > 100K orders/sec
//! - 撮合延迟: P99 < 100μs
//! - 行情推送延迟: P99 < 10μs
//! - 并发账户数: > 10,000
//! - 并发订阅者: > 1,000

#![allow(dead_code)]
#![allow(unused_imports)]

// ============================================================================
// 外部依赖
// ============================================================================

// 复用 QARS 核心模块
pub use qars;

// Web 框架
pub use actix;
pub use actix_web;

// 异步运行时
pub use tokio;
pub use futures;

// 并发工具
pub use dashmap;
pub use parking_lot;
pub use crossbeam;
pub use rayon;

// 序列化
pub use serde;
pub use serde_json;

// 时间
pub use chrono;

// 日志
pub use log;

// 错误处理
pub use thiserror;
pub use anyhow;

// UUID
pub use uuid;

// ============================================================================
// 内部模块
// ============================================================================

/// 核心模块 - 复用 qars 账户/订单/持仓系统
pub mod core;

/// 撮合引擎 - 复用 qars 撮合引擎并扩展
pub mod matching;

/// 交易所核心业务逻辑
pub mod exchange;

/// 风控系统
pub mod risk;

/// 行情推送系统
pub mod market;

/// 对外服务层 (WebSocket + HTTP)
pub mod service;

/// 持久化存储
pub mod storage;

/// 协议层 (QIFI/TIFI/MIFI)
pub mod protocol;

/// 工具模块
pub mod utils;

// ============================================================================
// 重导出常用类型
// ============================================================================

// 从 qars 重导出核心类型
pub use qars::qaaccount::account::QA_Account;
pub use qars::qaaccount::order::{QAOrder, QAOrderExt};
pub use qars::qaaccount::position::QA_Position;

// 从 qars 重导出协议类型
pub use qars::qaprotocol::qifi::account::{Account, Order, Position, Trade, QIFI};
pub use qars::qaprotocol::tifi::{ReqOrder, ReqCancel, ReqLogin};
pub use qars::qaprotocol::mifi;

// 从 qars 重导出撮合引擎
pub use qars::qamarket::matchengine::{
    domain::{OrderDirection, OrderType},
    orderbook::{Orderbook, Success, Failed, TradingState},
};

// 从 qars 重导出数据广播
pub use qars::qadata::broadcast_hub::{DataBroadcaster, BroadcastConfig, MarketDataType};

// ============================================================================
// 全局错误类型
// ============================================================================

/// 交易所错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("Account error: {0}")]
    AccountError(String),

    #[error("Order error: {0}")]
    OrderError(String),

    #[error("Matching error: {0}")]
    MatchingError(String),

    #[error("Risk check failed: {0}")]
    RiskCheckFailed(String),

    #[error("Settlement error: {0}")]
    SettlementError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Service error: {0}")]
    ServiceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type Result<T> = std::result::Result<T, ExchangeError>;

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imports() {
        // 验证可以正确导入 qars 模块
        let _account_type = std::any::type_name::<QA_Account>();
        let _order_type = std::any::type_name::<QAOrder>();
    }
}
