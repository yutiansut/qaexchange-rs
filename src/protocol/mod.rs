//! 协议层 - 重导出 qars 协议 + IPC 消息 + DIFF 协议

pub use qars::qaprotocol::qifi;
pub use qars::qaprotocol::tifi;
pub use qars::qaprotocol::mifi;

/// IPC 进程间通信消息协议
pub mod ipc_messages;

/// DIFF 协议 (Differential Information Flow for Finance)
/// 基于 QIFI+TIFI 的差分推送协议
pub mod diff;
