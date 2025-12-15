//! 风控系统模块
//!
//! ## 功能概述
//! - **盘前风控**: PreTradeCheck - 订单提交前的资金、持仓、风险检查
//! - **盘中风控**: RiskMonitor - 实时监控账户风险，自动预警和强平触发
//!
//! @yutiansut @quantaxis

pub mod pre_trade_check;
pub mod risk_monitor;

pub use pre_trade_check::PreTradeCheck;
pub use risk_monitor::{
    LiquidationCallback,
    LiquidationRecord,
    MarginSummary,
    MonitorStats,
    RiskAccount,
    RiskAlert,
    RiskAlertType,
    RiskLevel,
    RiskMonitor,
    RiskMonitorConfig,
};
