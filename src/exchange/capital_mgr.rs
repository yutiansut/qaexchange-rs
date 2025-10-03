//! 资金管理模块

use crate::exchange::AccountManager;
use crate::ExchangeError;
use std::sync::Arc;

pub struct CapitalManager {
    account_mgr: Arc<AccountManager>,
}

impl CapitalManager {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self { account_mgr }
    }

    pub fn deposit(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_account(user_id)?;
        account.write().deposit(amount);
        log::info!("Deposit: user={}, amount={}", user_id, amount);
        Ok(())
    }

    pub fn withdraw(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_account(user_id)?;
        let mut acc = account.write();

        if acc.accounts.available < amount {
            return Err(ExchangeError::AccountError("Insufficient funds".to_string()));
        }

        acc.withdraw(amount);
        log::info!("Withdraw: user={}, amount={}", user_id, amount);
        Ok(())
    }
}
