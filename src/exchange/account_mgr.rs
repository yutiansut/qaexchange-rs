//! 账户管理中心
//!
//! 负责账户的开户、销户、查询等管理功能

use crate::core::{QA_Account, Account};
use crate::core::account_ext::{OpenAccountRequest, AccountType};
use crate::ExchangeError;
use dashmap::DashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::Local;

/// 账户管理器
pub struct AccountManager {
    /// 账户映射 (user_id -> QA_Account)
    accounts: DashMap<String, Arc<RwLock<QA_Account>>>,

    /// 账户类型映射
    account_types: DashMap<String, AccountType>,

    /// 账户密码（实际应该加密存储）
    passwords: DashMap<String, String>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            account_types: DashMap::new(),
            passwords: DashMap::new(),
        }
    }

    /// 开户
    pub fn open_account(&self, req: OpenAccountRequest) -> Result<String, ExchangeError> {
        // 检查账户是否已存在
        if self.accounts.contains_key(&req.user_id) {
            return Err(ExchangeError::AccountError(
                format!("Account already exists: {}", req.user_id)
            ));
        }

        // 创建账户 (复用 QA_Account)
        let account = QA_Account::new(
            &req.user_id,       // account_cookie
            "default",          // portfolio_cookie
            &req.user_id,       // user_cookie
            req.init_cash,      // init_cash
            false,              // auto_reload
            "exchange",         // environment
        );

        // 存储账户
        self.accounts.insert(req.user_id.clone(), Arc::new(RwLock::new(account)));
        self.account_types.insert(req.user_id.clone(), req.account_type);
        self.passwords.insert(req.user_id.clone(), req.password);

        log::info!("Account opened: {} (type: {:?})", req.user_id, req.account_type);

        Ok(req.user_id)
    }

    /// 销户
    pub fn close_account(&self, user_id: &str) -> Result<(), ExchangeError> {
        if let Some((_, account)) = self.accounts.remove(user_id) {
            let acc = account.read();

            // 检查账户是否可以销户
            if !acc.hold.is_empty() {
                return Err(ExchangeError::AccountError(
                    "Cannot close account with open positions".to_string()
                ));
            }

            if acc.money > 0.0 {
                return Err(ExchangeError::AccountError(
                    "Cannot close account with remaining balance".to_string()
                ));
            }

            self.account_types.remove(user_id);
            self.passwords.remove(user_id);

            log::info!("Account closed: {}", user_id);
            Ok(())
        } else {
            Err(ExchangeError::AccountError(
                format!("Account not found: {}", user_id)
            ))
        }
    }

    /// 查询账户
    pub fn get_account(&self, user_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
        self.accounts
            .get(user_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| ExchangeError::AccountError(
                format!("Account not found: {}", user_id)
            ))
    }

    /// 查询账户 QIFI 格式
    pub fn get_account_qifi(&self, user_id: &str) -> Result<Account, ExchangeError> {
        let account = self.get_account(user_id)?;
        let acc_data = account.read().accounts.clone();
        Ok(acc_data)
    }

    /// 获取所有账户
    pub fn get_all_accounts(&self) -> Vec<Arc<RwLock<QA_Account>>> {
        self.accounts.iter().map(|r| r.value().clone()).collect()
    }

    /// 获取账户数量
    pub fn get_account_count(&self) -> usize {
        self.accounts.len()
    }

    /// 验证密码
    pub fn verify_password(&self, user_id: &str, password: &str) -> bool {
        self.passwords
            .get(user_id)
            .map(|p| p.value() == password)
            .unwrap_or(false)
    }

    /// 修改密码
    pub fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), ExchangeError> {
        if !self.verify_password(user_id, old_password) {
            return Err(ExchangeError::AccountError(
                "Invalid password".to_string()
            ));
        }

        if let Some(mut password) = self.passwords.get_mut(user_id) {
            *password = new_password.to_string();
            log::info!("Password changed for user: {}", user_id);
            Ok(())
        } else {
            Err(ExchangeError::AccountError(
                format!("Account not found: {}", user_id)
            ))
        }
    }

    /// 同步所有账户时间
    pub fn sync_time(&self) {
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for account in self.accounts.iter() {
            account.value().write().change_datetime(current_time.clone());
        }
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_account() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            user_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
            password: "password123".to_string(),
        };

        let result = mgr.open_account(req);
        assert!(result.is_ok());

        let account = mgr.get_account("test_user");
        assert!(account.is_ok());
    }

    #[test]
    fn test_duplicate_account() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            user_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
            password: "password123".to_string(),
        };

        mgr.open_account(req.clone()).unwrap();
        let result = mgr.open_account(req);
        assert!(result.is_err());
    }

    #[test]
    fn test_password_verification() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            user_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
            password: "password123".to_string(),
        };

        mgr.open_account(req).unwrap();

        assert!(mgr.verify_password("test_user", "password123"));
        assert!(!mgr.verify_password("test_user", "wrong_password"));
    }
}
