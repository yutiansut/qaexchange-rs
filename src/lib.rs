//! # QAEXCHANGE-RS
//!
//! 高性能量化交易所系统 - 基于 QARS 核心架构
//!
//! ## 核心能力
//!
//! - **账户管理**: 开户/入金/出金/查询 (复用 qars::qaaccount)
//! - **订单系统**: 下单/撤单/订单路由 (复用 qars::qaaccount::QAOrder)
//! - **撮合引擎**: 价格时间优先/连续交易 (复用 qars::qamarket::matchengine)
//! - **成交回报**: 实时成交推送/账户更新
//! - **行情推送**: Level1/Level2/逐笔成交 (复用 qars::qadata::broadcast_hub)
//! - **结算系统**: 盯市盈亏/结算价管理
//! - **风控系统**: 盘前风控/持仓限额/自成交防范/盘中风险监控
//!
//! ## ⚠️ 已实现但**默认不启用**的能力 @yutiansut @quantaxis
//!
//! 下面这些代码是完整的、有单测的,但**当前不会自己运行** ——
//! 之前这份清单把它们与上面的能力并列,读者会以为它们在工作。
//! 2026-09-05 逐条核过:
//!
//! - **集合竞价**:`matching/auction.rs`(304 行,7 个测试)—— **生产零引用**。
//!   `AuctionCalculator` / `AuctionResult` 全仓无外部调用点。
//! - **交易时段门禁**:`exchange/trading_session.rs:461 validate_order` 本身完整
//!   (查交易日历时段的 `allow_order` + 合约 `TradingState`),
//!   但需 `QAEX_ENFORCE_TRADING_SESSION=1` 才生效 ——
//!   **默认 7×24 都能下单**,没有午休/收盘之分。
//! - **日终结算**:`daily_settlement` 只能手动调
//!   (`POST /api/admin/settlement/execute`),`main.rs` **无任何时间调度**,
//!   `trading_session` 也不触发它。**当前从未执行过**。
//!   连带后果:`dailyorders`/`dailytrades` 永不清空(装着全部历史)、
//!   `position_price` 永不滚动、零手持仓永不回收。
//! - **自动强平**:`risk_monitor` 的 `set_liquidation_callback` **生产零调用**,
//!   回调恒为 `None` —— 盘中风控**只告警不强平**。
//! - **高性能撮合**:`matching/high_perf.rs`(643 行)—— 生产零引用,
//!   与 qars 的撮合核心重复。
//!
//! ## ⚠️ 完全未接线的模块(2026-09-05 核查)@yutiansut @quantaxis
//!
//! 下面四个模块**编译进 crate 但生产代码零引用**,合计 **4,780 行 / 44 个测试**。
//! 它们各自都有测试、写得也认真,只是最后一根线没接:
//!
//! | 模块 | 行数 | 测试 | 模块外引用 |
//! |---|---|---|---|
//! | [`dsl`] —— 因子表达式解析 | 1,641 | 7 | **0** |
//! | [`perf`] —— 对象池 / 性能原语 | 1,844 | 27 | 仅被同样死的 `matching::high_perf` 用 |
//! | [`observability`] —— 指标导出 | 800 | 6 | **0** |
//! | [`cluster`] —— 一致性哈希 | 495 | 4 | **0** |
//!
//! 核查方法:逐个代表符号(`FactorParser` / `AstBuilder` / `PhysicalNode` /
//! `ConsistentHash` / `init_metrics` / `export_metrics`)查模块外引用,全部 0;
//! 阳性对照:同法查 `risk` / `market` / `exchange` 得 3 / 32 / 42 次。
//!
//! **读到这里的人请注意**:引用它们之前先确认那部分功能是否真的接通,
//! 不要因为"代码在仓里、还有测试"就假定它在工作。
//! 处置(删除 / 接线 / 移出主编译单元)见 `tasks/DECISIONS-PENDING.md`。
//!
//! 详见 `tasks/DECISIONS-PENDING.md`。
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
pub use futures;
pub use tokio;

// 并发工具
pub use crossbeam;
pub use dashmap;
pub use parking_lot;
pub use rayon;

// 序列化
pub use serde;
pub use serde_json;

// 时间
pub use chrono;

// 日志
pub use log;

// 错误处理
pub use anyhow;
pub use thiserror;

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

/// 账户系统（高性能独立版本）
pub mod account;

/// 风控系统
pub mod risk;

/// 行情推送系统
pub mod market;

/// 用户管理系统
pub mod user;

/// 对外服务层 (WebSocket + HTTP)
pub mod service;

/// 持久化存储
pub mod storage;

/// 协议层 (QIFI/TIFI/MIFI)
pub mod protocol;

/// 工具模块
pub mod utils;

/// 通知消息系统
pub mod notification;

// iceoryx2 零拷贝 IPC
pub mod ipc;

// 主从复制系统
/// ⚠️ **当前未接线** —— 4529 行,生产运行时完全不参与。@yutiansut @quantaxis
///
/// **完全孤立** —— main.rs 0 引用,非自身模块 0 引用
///
/// 实测(生产运行 1 小时、92k 订单):日志里 `factor` / `dsl` / `replication` /
/// `Raft` / `leader` / `follower` 全部 **0 次命中**;`nm -C` 在 release 二进制里
/// 找不到这四块的任何符号 —— Rust 已作为死代码优化掉,**运行时零成本**,
/// 代价只在编译时间与维护负担。
///
/// 保留 `pub mod` 而不删除:这些是有价值的未完成工作(主从复制、因子计算、
/// 查询引擎),删掉会丢失设计。但**不要以为它们在工作** ——
/// 接线前先读 sim/patches/APPLY.md 的「已定位但未修」一节。
pub mod replication;

// 查询引擎系统 (Phase 8)
/// ⚠️ **当前未接线** —— 2908 行,生产运行时完全不参与。@yutiansut @quantaxis
///
/// 1 处外部引用(storage/hybrid/batch_source.rs:18),
/// 而那条分支**永不进入** —— `olap_cutoff_timestamp` 恒为 0、
/// `refresh_olap_files` 零调用
///
/// 实测(生产运行 1 小时、92k 订单):日志里 `factor` / `dsl` / `replication` /
/// `Raft` / `leader` / `follower` 全部 **0 次命中**;`nm -C` 在 release 二进制里
/// 找不到这四块的任何符号 —— Rust 已作为死代码优化掉,**运行时零成本**,
/// 代价只在编译时间与维护负担。
///
/// 保留 `pub mod` 而不删除:这些是有价值的未完成工作(主从复制、因子计算、
/// 查询引擎),删掉会丢失设计。但**不要以为它们在工作** ——
/// 接线前先读 sim/patches/APPLY.md 的「已定位但未修」一节。
pub mod query;

// 因子计算系统 (流批一体化)
/// ⚠️ **当前未接线** —— 6258 行,生产运行时完全不参与。@yutiansut @quantaxis
///
/// 5 处外部引用,但**引用者本身都是死的**:
///   · dsl —— 自身孤立
///   · kline_actor —— `enable_factor_compute` 默认 false(:72),
///     且 set 它的 setter(:92) 生产零调用
///   · unified_recovery —— 生产零调用(审查已确认)
///
/// 实测(生产运行 1 小时、92k 订单):日志里 `factor` / `dsl` / `replication` /
/// `Raft` / `leader` / `follower` 全部 **0 次命中**;`nm -C` 在 release 二进制里
/// 找不到这四块的任何符号 —— Rust 已作为死代码优化掉,**运行时零成本**,
/// 代价只在编译时间与维护负担。
///
/// 保留 `pub mod` 而不删除:这些是有价值的未完成工作(主从复制、因子计算、
/// 查询引擎),删掉会丢失设计。但**不要以为它们在工作** ——
/// 接线前先读 sim/patches/APPLY.md 的「已定位但未修」一节。
pub mod factor;

// 集群管理系统 (一致性哈希)
pub mod cluster;

// DSL 解析系统
/// ⚠️ **当前未接线** —— 1641 行,生产运行时完全不参与。@yutiansut @quantaxis
///
/// **完全孤立** —— 唯一的外部引用是它自己引用 factor
///
/// 实测(生产运行 1 小时、92k 订单):日志里 `factor` / `dsl` / `replication` /
/// `Raft` / `leader` / `follower` 全部 **0 次命中**;`nm -C` 在 release 二进制里
/// 找不到这四块的任何符号 —— Rust 已作为死代码优化掉,**运行时零成本**,
/// 代价只在编译时间与维护负担。
///
/// 保留 `pub mod` 而不删除:这些是有价值的未完成工作(主从复制、因子计算、
/// 查询引擎),删掉会丢失设计。但**不要以为它们在工作** ——
/// 接线前先读 sim/patches/APPLY.md 的「已定位但未修」一节。
pub mod dsl;

// 性能优化模块 (Phase 5.2)
pub mod perf;

// 可观测性模块 (Prometheus)
pub mod observability;

// ============================================================================
// 重导出常用类型
// ============================================================================

// 从 qars 重导出核心类型
pub use qars::qaaccount::account::QA_Account;
pub use qars::qaaccount::order::{QAOrder, QAOrderExt};
pub use qars::qaaccount::position::QA_Position;

// 从 qars 重导出协议类型
pub use qars::qaprotocol::mifi;
pub use qars::qaprotocol::qifi::account::{Account, Order, Position, Trade, QIFI};
pub use qars::qaprotocol::tifi::{ReqCancel, ReqLogin, ReqOrder};

// 从 qars 重导出撮合引擎
pub use qars::qamarket::matchengine::{
    domain::{OrderDirection, OrderType},
    orderbook::{Failed, Orderbook, Success, TradingState},
};

// 从 qars 重导出数据广播
pub use qars::qadata::broadcast_hub::{BroadcastConfig, DataBroadcaster, MarketDataType};

// ============================================================================
// 全局错误类型
// ============================================================================

/// 交易所错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("User error: {0}")]
    UserError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

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

    #[error("Instrument error: {0}")]
    InstrumentError(String),

    /// 资源不存在 —— 映射到 HTTP 404 @yutiansut @quantaxis
    ///
    /// 加这个变体是为了**收敛三处各自表达「不存在」的写法**:
    ///   market/mod.rs:327  MatchingError("Instrument not found: {id}")   → HTTP 500
    ///   market/mod.rs:472  MatchingError("Instrument not found: {id}")   → HTTP 500
    ///   instrument_registry.rs:177  InstrumentError("Instrument {id} not found")
    /// 连措辞都不一致,更关键的是**语义丢失**:HTTP 层只能看到 MatchingError,
    /// 只好一律 500。实测 `GET /api/market/tick/{不存在的合约}` 返回
    /// `500 Failed to get tick: Matching error: Instrument not found`,
    /// 而正确语义是 404。
    ///
    /// 为什么不复用 `InstrumentError`:它同时承担「已存在」(instrument_registry.rs:132,
    /// 重复上市,语义是 409/400)和「不存在」(:177),**变体本身区分不了**,
    /// 一律映射 404 会把「重复上市」也说成「找不到」。
    // Display **不加变体前缀** —— 与其他变体不同,这里的消息体本身就是完整句子
    // ("Instrument not found: X" / "Account not found: X")。加上 "Not found: "
    // 会得到 `Not found: Instrument not found: X`,再被 handler 的 context 一裹
    // 就是三重复述。实测旧行为:
    //   `Account not found: AccountError("Account not found: ZZNOACC9")`
    // @yutiansut @quantaxis
    #[error("{0}")]
    NotFound(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Service error: {0}")]
    ServiceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("IO error: {0}")]
    IOError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),
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
