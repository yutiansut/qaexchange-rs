//! 订单扩展功能
//!
//! 在 qars::QAOrder 基础上提供交易所特定的扩展功能

use crate::core::Order;
use serde::{Deserialize, Serialize};

/// 订单状态枚举 (扩展)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    /// 未提交
    Pending = 100,
    /// 已接受
    Accepted = 200,
    /// 部分成交
    PartiallyFilled = 300,
    /// 全部成交
    Filled = 400,
    /// 已撤单
    Cancelled = 500,
    /// 已拒绝
    Rejected = 600,
}

impl OrderStatus {
    pub fn from_string(s: &str) -> Self {
        match s {
            "PENDING" => OrderStatus::Pending,
            "ACCEPTED" => OrderStatus::Accepted,
            "ALIVE" => OrderStatus::PartiallyFilled,
            "FINISHED" => OrderStatus::Filled,
            "CANCELLED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            _ => OrderStatus::Pending,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            OrderStatus::Pending => "PENDING".to_string(),
            OrderStatus::Accepted => "ACCEPTED".to_string(),
            OrderStatus::PartiallyFilled => "ALIVE".to_string(),
            OrderStatus::Filled => "FINISHED".to_string(),
            OrderStatus::Cancelled => "CANCELLED".to_string(),
            OrderStatus::Rejected => "REJECTED".to_string(),
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::Accepted | OrderStatus::PartiallyFilled)
    }
}

/// 订单扩展 trait
pub trait OrderExtension {
    /// 获取订单状态
    fn get_status(&self) -> OrderStatus;

    /// 是否为活动订单
    fn is_active(&self) -> bool;

    /// 是否已完成（终态）
    fn is_final(&self) -> bool;

    /// 获取已成交数量
    fn filled_volume(&self) -> f64;

    /// 获取未成交数量
    fn unfilled_volume(&self) -> f64;
}

impl OrderExtension for Order {
    fn get_status(&self) -> OrderStatus {
        OrderStatus::from_string(&self.status)
    }

    fn is_active(&self) -> bool {
        self.get_status().is_active()
    }

    fn is_final(&self) -> bool {
        self.get_status().is_final()
    }

    fn filled_volume(&self) -> f64 {
        self.volume_orign - self.volume_left
    }

    fn unfilled_volume(&self) -> f64 {
        self.volume_left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_status_conversion() {
        let status = OrderStatus::Accepted;
        let s = status.to_string();
        assert_eq!(s, "ACCEPTED");

        let status2 = OrderStatus::from_string(&s);
        assert_eq!(status, status2);
    }

    #[test]
    fn test_order_status_checks() {
        assert!(OrderStatus::Filled.is_final());
        assert!(!OrderStatus::Accepted.is_final());

        assert!(OrderStatus::Accepted.is_active());
        assert!(!OrderStatus::Filled.is_active());
    }
}
