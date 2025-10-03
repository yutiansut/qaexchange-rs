//! 账户管理扩展功能
//!
//! 在 qars::QA_Account 基础上提供交易所特定的扩展功能

use crate::core::QA_Account;
use crate::ExchangeError;
use serde::{Deserialize, Serialize};

/// 开户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAccountRequest {
    pub user_id: String,
    pub user_name: String,
    pub init_cash: f64,
    pub account_type: AccountType,
    pub password: String, // 实际应该加密存储
}

/// 账户类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AccountType {
    /// 个人账户
    Individual,
    /// 机构账户
    Institutional,
    /// 做市商账户
    MarketMaker,
}

/// 入金请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    pub user_id: String,
    pub amount: f64,
    pub reference: String, // 入金流水号
}

/// 出金请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawRequest {
    pub user_id: String,
    pub amount: f64,
    pub reference: String, // 出金流水号
}

/// 账户扩展 trait
pub trait AccountExt {
    /// 验证账户状态
    fn validate(&self) -> Result<(), ExchangeError>;

    /// 检查是否可以交易
    fn can_trade(&self) -> bool;

    /// 检查资金是否充足
    fn has_sufficient_funds(&self, required: f64) -> bool;
}

impl AccountExt for QA_Account {
    fn validate(&self) -> Result<(), ExchangeError> {
        if self.money < 0.0 {
            return Err(ExchangeError::AccountError(
                "Account balance cannot be negative".to_string(),
            ));
        }

        if self.accounts.risk_ratio > 1.0 {
            return Err(ExchangeError::RiskCheckFailed(
                format!("Risk ratio too high: {}", self.accounts.risk_ratio),
            ));
        }

        Ok(())
    }

    fn can_trade(&self) -> bool {
        self.accounts.available > 0.0 && self.accounts.risk_ratio < 1.0
    }

    fn has_sufficient_funds(&self, required: f64) -> bool {
        self.accounts.available >= required
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_type_serialization() {
        let account_type = AccountType::Individual;
        let json = serde_json::to_string(&account_type).unwrap();
        let deserialized: AccountType = serde_json::from_str(&json).unwrap();
        assert_eq!(account_type, deserialized);
    }
}
