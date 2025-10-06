//! 账户系统模块
//!
//! 提供两种账户管理方式：
//! 1. 集成模式 - AccountManager（当前使用，与撮合引擎耦合）
//! 2. 独立模式 - AccountSystemCore（高性能，独立进程）

/// 账户系统核心（独立进程版本）
pub mod core;
