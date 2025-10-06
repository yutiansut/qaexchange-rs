//! DIFF 协议实现
//!
//! Differential Information Flow for Finance (DIFF) 协议是在 QIFI+TIFI 基础上扩展的
//! 实时差分推送协议，用于高效同步交易所业务数据。
//!
//! # 协议层级
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      DIFF 协议                               │
//! │  (差分推送 + 行情数据 + K线 + 通知)                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │                      TIFI 协议                               │
//! │  (peek_message + rtn_data 传输机制)                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │                      QIFI 协议                               │
//! │  (Account, Position, Order 数据结构)                         │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 核心模块
//!
//! - `merge`: JSON Merge Patch (RFC 7386) 实现
//! - `snapshot`: 业务快照管理器
//! - `types`: DIFF 协议数据类型定义
//!
//! # 兼容性
//!
//! DIFF 协议 100% 向后兼容 QIFI 和 TIFI 协议：
//!
//! - **数据层**: 直接复用 QIFI 的 `Account`, `Position`, `Order` 类型
//! - **传输层**: 复用 TIFI 的 `peek_message` + `rtn_data` 机制
//! - **扩展层**: 新增 `Quote`, `Kline`, `Notify` 数据类型
//!
//! # 使用示例
//!
//! ```rust
//! use qaexchange::protocol::diff::merge::merge_patch;
//! use serde_json::json;
//!
//! // 业务快照
//! let mut snapshot = json!({
//!     "trade": {
//!         "user123": {
//!             "accounts": {
//!                 "ACC001": {
//!                     "balance": 100000.0,
//!                     "available": 95000.0
//!                 }
//!             }
//!         }
//!     }
//! });
//!
//! // 差分更新
//! let patch = json!({
//!     "trade": {
//!         "user123": {
//!             "accounts": {
//!                 "ACC001": {
//!                     "balance": 105000.0,
//!                     "available": 100000.0
//!                 }
//!             }
//!         }
//!     }
//! });
//!
//! merge_patch(&mut snapshot, &patch);
//!
//! assert_eq!(snapshot["trade"]["user123"]["accounts"]["ACC001"]["balance"], 105000.0);
//! ```

pub mod merge;
pub mod snapshot;
pub mod types;
