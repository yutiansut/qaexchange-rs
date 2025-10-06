//! 资金管理模块
//!
//! 负责管理账户资金的出入金、流水记录等功能

use crate::exchange::AccountManager;
use crate::ExchangeError;
use std::sync::Arc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// 交易类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    /// 入金
    Deposit,
    /// 出金
    Withdrawal,
    /// 手续费
    Commission,
    /// 盈亏
    PnL,
    /// 结算
    Settlement,
}

/// 交易状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    /// 待处理
    Pending,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 资金流水记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundTransaction {
    /// 交易ID
    pub transaction_id: String,
    /// 用户ID
    pub user_id: String,
    /// 交易类型
    pub transaction_type: TransactionType,
    /// 交易金额
    pub amount: f64,
    /// 交易前余额
    pub balance_before: f64,
    /// 交易后余额
    pub balance_after: f64,
    /// 交易状态
    pub status: TransactionStatus,
    /// 支付方式
    pub method: Option<String>,
    /// 备注
    pub remark: Option<String>,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
}

pub struct CapitalManager {
    account_mgr: Arc<AccountManager>,
    /// 资金流水记录 (user_id -> Vec<FundTransaction>)
    transactions: DashMap<String, Vec<FundTransaction>>,
    /// 交易序列号
    transaction_seq: std::sync::atomic::AtomicU64,
}

impl CapitalManager {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            transactions: DashMap::new(),
            transaction_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 生成交易ID
    fn generate_transaction_id(&self) -> String {
        let seq = self.transaction_seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let now = chrono::Local::now();
        format!("TXN{}{:08}", now.format("%Y%m%d"), seq)
    }

    /// 入金 (旧接口,保持兼容)
    pub fn deposit(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_default_account(user_id)?;
        account.write().deposit(amount);
        log::info!("Deposit: user={}, amount={}", user_id, amount);
        Ok(())
    }

    /// 入金 (新接口,带流水记录)
    pub fn deposit_with_record(
        &self,
        user_id: String,
        amount: f64,
        method: Option<String>,
        remark: Option<String>,
    ) -> Result<FundTransaction, ExchangeError> {
        if amount <= 0.0 {
            return Err(ExchangeError::InvalidParameter("存款金额必须大于0".to_string()));
        }

        // 获取账户当前余额
        let balance_before = {
            let account = self.account_mgr.get_default_account(&user_id)?;
            let acc = account.read();
            acc.accounts.balance
        };

        // 执行入金
        self.deposit(&user_id, amount)?;

        // 获取交易后余额
        let balance_after = balance_before + amount;

        // 创建交易记录
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let transaction = FundTransaction {
            transaction_id: self.generate_transaction_id(),
            user_id: user_id.clone(),
            transaction_type: TransactionType::Deposit,
            amount,
            balance_before,
            balance_after,
            status: TransactionStatus::Completed,
            method,
            remark,
            created_at: now.clone(),
            updated_at: now,
        };

        // 保存交易记录
        self.transactions
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(transaction.clone());

        log::info!("Deposit completed: user={}, amount={}, transaction_id={}",
            user_id, amount, transaction.transaction_id);

        Ok(transaction)
    }

    /// 出金 (旧接口,保持兼容)
    pub fn withdraw(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_default_account(user_id)?;
        let mut acc = account.write();

        if acc.money < amount {
            return Err(ExchangeError::AccountError("Insufficient funds".to_string()));
        }

        acc.withdraw(amount);
        log::info!("Withdraw: user={}, amount={}", user_id, amount);
        Ok(())
    }

    /// 出金 (新接口,带流水记录)
    pub fn withdraw_with_record(
        &self,
        user_id: String,
        amount: f64,
        method: Option<String>,
        remark: Option<String>,
    ) -> Result<FundTransaction, ExchangeError> {
        if amount <= 0.0 {
            return Err(ExchangeError::InvalidParameter("取款金额必须大于0".to_string()));
        }

        // 获取账户信息
        let (balance_before, available) = {
            let account = self.account_mgr.get_default_account(&user_id)?;
            let acc = account.read();
            (acc.accounts.balance, acc.money)
        };

        // 检查可用资金
        if available < amount {
            return Err(ExchangeError::InsufficientBalance(
                format!("可用资金不足: 需要={}, 可用={}", amount, available)
            ));
        }

        // 执行出金
        self.withdraw(&user_id, amount)?;

        // 获取交易后余额
        let balance_after = balance_before - amount;

        // 创建交易记录
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let transaction = FundTransaction {
            transaction_id: self.generate_transaction_id(),
            user_id: user_id.clone(),
            transaction_type: TransactionType::Withdrawal,
            amount,
            balance_before,
            balance_after,
            status: TransactionStatus::Completed,
            method,
            remark,
            created_at: now.clone(),
            updated_at: now,
        };

        // 保存交易记录
        self.transactions
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(transaction.clone());

        log::info!("Withdrawal completed: user={}, amount={}, transaction_id={}",
            user_id, amount, transaction.transaction_id);

        Ok(transaction)
    }

    /// 获取用户的资金流水
    pub fn get_transactions(&self, user_id: &str) -> Vec<FundTransaction> {
        self.transactions
            .get(user_id)
            .map(|txns| txns.value().clone())
            .unwrap_or_default()
    }

    /// 获取用户的最近N条资金流水
    pub fn get_recent_transactions(&self, user_id: &str, limit: usize) -> Vec<FundTransaction> {
        let mut txns = self.get_transactions(user_id);
        txns.reverse(); // 最新的在前
        txns.truncate(limit);
        txns
    }

    /// 根据日期范围获取资金流水
    pub fn get_transactions_by_date_range(
        &self,
        user_id: &str,
        start_date: &str,
        end_date: &str,
    ) -> Vec<FundTransaction> {
        self.get_transactions(user_id)
            .into_iter()
            .filter(|txn| txn.created_at.as_str() >= start_date && txn.created_at.as_str() <= end_date)
            .collect()
    }

    /// 获取所有用户的资金流水数量
    pub fn get_total_transaction_count(&self) -> usize {
        self.transactions
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_and_withdraw_with_record() {
        use crate::core::account_ext::{OpenAccountRequest, AccountType};

        // 创建账户管理器和资金管理器
        let account_mgr = Arc::new(AccountManager::new());
        let capital_mgr = CapitalManager::new(account_mgr.clone());

        // 开户
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: None,
            account_name: "Test User".to_string(),
            init_cash: 10000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 测试入金
        let deposit = capital_mgr.deposit_with_record(
            "test_user".to_string(),
            5000.0,
            Some("bank_transfer".to_string()),
            Some("初始入金".to_string()),
        ).unwrap();

        assert_eq!(deposit.amount, 5000.0);
        assert_eq!(deposit.balance_before, 10000.0);
        assert_eq!(deposit.balance_after, 15000.0);
        assert_eq!(deposit.status, TransactionStatus::Completed);

        // 测试出金
        let withdrawal = capital_mgr.withdraw_with_record(
            "test_user".to_string(),
            3000.0,
            Some("bank_transfer".to_string()),
            None,
        ).unwrap();

        assert_eq!(withdrawal.amount, 3000.0);
        assert_eq!(withdrawal.balance_before, 15000.0);
        assert_eq!(withdrawal.balance_after, 12000.0);

        // 查询流水
        let txns = capital_mgr.get_transactions("test_user");
        assert_eq!(txns.len(), 2);
    }
}
