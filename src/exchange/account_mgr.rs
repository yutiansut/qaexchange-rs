//! 账户管理中心
//!
//! 负责账户的开户、销户、查询等管理功能

use crate::core::{QA_Account, Account, QIFI};
use crate::core::account_ext::{OpenAccountRequest, AccountType};
use crate::ExchangeError;
use crate::notification::NotificationBroker;
use crate::notification::message::{Notification, NotificationType, NotificationPayload, AccountOpenNotify};
use crate::user::UserManager;
use dashmap::DashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::Local;
use std::path::Path;

/// 账户元数据
#[derive(Debug, Clone)]
struct AccountMetadata {
    /// 所属用户ID
    user_id: String,

    /// 账户名称
    account_name: String,

    /// 账户类型
    account_type: AccountType,

    /// 创建时间
    created_at: i64,
}

/// 账户管理器
pub struct AccountManager {
    /// 账户映射 (account_id -> QA_Account)
    accounts: DashMap<String, Arc<RwLock<QA_Account>>>,

    /// 账户元数据映射 (account_id -> AccountMetadata)
    metadata: DashMap<String, AccountMetadata>,

    /// 用户账户索引 (user_id -> [account_ids])
    user_accounts: DashMap<String, Vec<String>>,

    /// 通知中心（用于WAL恢复）
    notification_broker: Option<Arc<NotificationBroker>>,

    /// 用户管理器（用于验证用户和自动绑定）
    user_manager: Option<Arc<UserManager>>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            metadata: DashMap::new(),
            user_accounts: DashMap::new(),
            notification_broker: None,
            user_manager: None,
        }
    }

    /// 创建带通知功能的账户管理器（用于WAL恢复）
    pub fn with_notification_broker(broker: Arc<NotificationBroker>) -> Self {
        Self {
            accounts: DashMap::new(),
            metadata: DashMap::new(),
            user_accounts: DashMap::new(),
            notification_broker: Some(broker),
            user_manager: None,
        }
    }

    /// 设置通知中心
    pub fn set_notification_broker(&mut self, broker: Arc<NotificationBroker>) {
        self.notification_broker = Some(broker);
    }

    /// 获取通知中心
    pub fn notification_broker(&self) -> Option<&Arc<NotificationBroker>> {
        self.notification_broker.as_ref()
    }

    /// 设置用户管理器
    pub fn set_user_manager(&mut self, user_manager: Arc<UserManager>) {
        self.user_manager = Some(user_manager);
    }

    /// 开户
    ///
    /// 为指定用户创建一个新的交易账户。
    ///
    /// # 参数
    /// - `req`: 开户请求，包含用户ID、账户名称、初始资金等信息
    ///
    /// # 返回
    /// - `Ok(account_id)`: 成功创建的账户ID
    /// - `Err(...)`: 创建失败的错误信息
    pub fn open_account(&self, req: OpenAccountRequest) -> Result<String, ExchangeError> {
        // 验证用户是否存在（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            user_mgr.get_user(&req.user_id)?;
        }

        // 生成或使用提供的账户ID
        let account_id = req.account_id.unwrap_or_else(|| {
            format!("ACC_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
        });

        // 检查账户是否已存在
        if self.accounts.contains_key(&account_id) {
            return Err(ExchangeError::AccountError(
                format!("Account already exists: {}", account_id)
            ));
        }

        // 创建账户 (复用 QA_Account)
        // portfolio_cookie 使用 user_id，建立 User -> Account 关系
        // user_cookie 使用 account_name，对应 QIFI 的 investor_name
        // environment 设置为 "real" 以启用完整的订单管理功能
        let account = QA_Account::new(
            &account_id,        // account_cookie (账户唯一标识)
            &req.user_id,       // portfolio_cookie (用户ID - 建立User关联)
            &req.account_name,  // user_cookie (账户名称 -> QIFI investor_name)
            req.init_cash,      // init_cash
            false,              // auto_reload
            "real",             // environment (必须是 "real" 才能使用 dailyorders)
        );

        // 存储账户
        self.accounts.insert(account_id.clone(), Arc::new(RwLock::new(account)));

        // 存储元数据
        let metadata = AccountMetadata {
            user_id: req.user_id.clone(),
            account_name: req.account_name.clone(),
            account_type: req.account_type,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.metadata.insert(account_id.clone(), metadata);

        // 更新用户账户索引
        self.user_accounts
            .entry(req.user_id.clone())
            .or_insert_with(Vec::new)
            .push(account_id.clone());

        log::info!(
            "Account opened: {} for user {} (type: {:?}, name: {})",
            account_id, req.user_id, req.account_type, req.account_name
        );

        // 绑定账户到用户（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            if let Err(e) = user_mgr.bind_account(&req.user_id, account_id.clone()) {
                log::warn!("Failed to bind account to user: {}", e);
                // 不返回错误，因为账户已成功创建
            }
        }

        // 发送AccountOpen通知（用于WAL恢复）
        if let Some(broker) = &self.notification_broker {
            let notification = Notification::new(
                NotificationType::AccountOpen,
                Arc::from(account_id.clone()),
                NotificationPayload::AccountOpen(AccountOpenNotify {
                    account_id: account_id.clone(),
                    user_id: req.user_id.clone(),
                    account_name: req.account_name.clone(),
                    init_cash: req.init_cash,
                    account_type: req.account_type as u8,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }),
                "AccountManager",
            );

            if let Err(e) = broker.publish(notification) {
                log::error!("Failed to publish AccountOpen notification: {}", e);
                // 不返回错误，因为账户已成功创建
            }
        }

        Ok(account_id)
    }

    /// 销户
    pub fn close_account(&self, account_id: &str) -> Result<(), ExchangeError> {
        // 获取元数据（用于更新用户账户索引）
        let metadata = self.metadata.get(account_id).map(|m| m.clone());

        if let Some((_, account)) = self.accounts.remove(account_id) {
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

            // 从用户账户索引中移除
            if let Some(meta) = metadata {
                if let Some(mut accounts) = self.user_accounts.get_mut(&meta.user_id) {
                    accounts.retain(|id| id != account_id);
                }

                // 从用户解绑账户
                if let Some(user_mgr) = &self.user_manager {
                    if let Err(e) = user_mgr.unbind_account(&meta.user_id, account_id) {
                        log::warn!("Failed to unbind account from user: {}", e);
                    }
                }
            }

            self.metadata.remove(account_id);

            log::info!("Account closed: {}", account_id);
            Ok(())
        } else {
            Err(ExchangeError::AccountError(
                format!("Account not found: {}", account_id)
            ))
        }
    }

    /// 查询账户（通过账户ID）
    pub fn get_account(&self, account_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
        self.accounts
            .get(account_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| ExchangeError::AccountError(
                format!("Account not found: {}", account_id)
            ))
    }

    /// 查询用户的所有账户
    pub fn get_accounts_by_user(&self, user_id: &str) -> Vec<Arc<RwLock<QA_Account>>> {
        self.user_accounts
            .get(user_id)
            .map(|account_ids| {
                account_ids
                    .iter()
                    .filter_map(|id| self.accounts.get(id).map(|r| r.value().clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取用户的默认账户（第一个账户）
    ///
    /// 这是一个便捷方法，用于兼容旧代码（当用户只有一个账户时）
    /// 如果用户有多个账户，返回第一个账户
    pub fn get_default_account(&self, user_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
        self.user_accounts
            .get(user_id)
            .and_then(|account_ids| {
                account_ids.first().and_then(|id| {
                    self.accounts.get(id).map(|r| r.value().clone())
                })
            })
            .ok_or_else(|| ExchangeError::AccountError(
                format!("No account found for user: {}", user_id)
            ))
    }

    /// 查询用户的账户数量
    pub fn get_user_account_count(&self, user_id: &str) -> usize {
        self.user_accounts
            .get(user_id)
            .map(|ids| ids.len())
            .unwrap_or(0)
    }

    /// 验证账户所有权
    ///
    /// 验证指定的 account_id 是否属于指定的 user_id
    ///
    /// # 参数
    /// - `account_id`: 账户ID（如 "ACC_xxx"）
    /// - `user_id`: 用户ID（UUID）
    ///
    /// # 返回
    /// - `Ok(())` - 验证通过，账户属于该用户
    /// - `Err(ExchangeError::AccountError)` - 账户不存在
    /// - `Err(ExchangeError::PermissionDenied)` - 账户不属于该用户
    ///
    /// # 示例
    /// ```ignore
    /// account_mgr.verify_account_ownership("ACC_xxx", "user123")?;
    /// ```
    pub fn verify_account_ownership(
        &self,
        account_id: &str,
        user_id: &str
    ) -> Result<(), ExchangeError> {
        // 1. 检查账户是否存在
        let metadata = self.metadata.get(account_id)
            .ok_or_else(|| ExchangeError::AccountError(
                format!("Account not found: {}", account_id)
            ))?;

        // 2. 检查账户所有权
        if metadata.user_id != user_id {
            return Err(ExchangeError::PermissionDenied(
                format!(
                    "Account {} does not belong to user {} (owner: {})",
                    account_id, user_id, metadata.user_id
                )
            ));
        }

        Ok(())
    }

    /// 查询账户 QIFI 格式（实时 - 仅账户信息）
    /// 直接使用 qars 的 get_accountmessage() 方法获取实时账户数据
    pub fn get_account_qifi(&self, account_id: &str) -> Result<Account, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();

        // 直接调用 qars 的方法，它会自动计算所有实时数据
        // balance = get_balance() (实时总权益)
        // available = money (实时现金)
        // margin = get_margin() (实时保证金)
        // position_profit = get_positionprofit() (实时持仓盈亏)
        // risk_ratio = get_riskratio() (实时风险度)
        Ok(acc.get_accountmessage())
    }

    /// 获取完整 QIFI 切片（包含账户+持仓+订单+成交）
    pub fn get_qifi_slice(&self, account_id: &str) -> Result<crate::QIFI, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();
        Ok(acc.get_qifi_slice())
    }

    /// 获取 MOM 资金切片（轻量级实时资金快照）
    pub fn get_mom_slice(&self, account_id: &str) -> Result<crate::qars::qaaccount::account::QAMOMSlice, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();
        Ok(acc.get_mom_slice())
    }

    /// 获取所有账户
    pub fn get_all_accounts(&self) -> Vec<Arc<RwLock<QA_Account>>> {
        self.accounts.iter().map(|r| r.value().clone()).collect()
    }

    /// 获取账户数量
    pub fn get_account_count(&self) -> usize {
        self.accounts.len()
    }


    /// 同步所有账户时间
    pub fn sync_time(&self) {
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for account in self.accounts.iter() {
            account.value().write().change_datetime(current_time.clone());
        }
    }

    /// 获取账户元数据
    pub fn get_account_metadata(&self, account_id: &str) -> Option<(String, String, AccountType, i64)> {
        self.metadata.get(account_id).map(|m| {
            (m.user_id.clone(), m.account_name.clone(), m.account_type, m.created_at)
        })
    }

    /// 获取账户类型
    pub fn get_account_type(&self, account_id: &str) -> Option<AccountType> {
        self.metadata.get(account_id).map(|m| m.account_type)
    }

    /// 获取账户所属用户
    pub fn get_account_owner(&self, account_id: &str) -> Option<String> {
        self.metadata.get(account_id).map(|m| m.user_id.clone())
    }

    // ========== 方案A: QIFI快照保存与恢复 ==========

    /// 保存所有账户快照到QIFI文件
    pub fn save_snapshots(&self, snapshot_dir: &str) -> Result<usize, ExchangeError> {
        std::fs::create_dir_all(snapshot_dir)
            .map_err(|e| ExchangeError::IOError(format!("Create snapshot dir failed: {}", e)))?;

        let mut saved_count = 0;

        for entry in self.accounts.iter() {
            let account_id = entry.key();
            let account = entry.value();

            // 获取QIFI快照
            let mut acc = account.write();
            let qifi = acc.get_qifi_slice();

            // 序列化为JSON
            let json = serde_json::to_string_pretty(&qifi)
                .map_err(|e| ExchangeError::SerializationError(format!("QIFI serialization failed: {}", e)))?;

            // 写入文件（使用账户ID作为文件名）
            let file_path = format!("{}/{}.json", snapshot_dir, account_id);
            std::fs::write(&file_path, json)
                .map_err(|e| ExchangeError::IOError(format!("Write snapshot failed: {}", e)))?;

            saved_count += 1;
        }

        log::info!("Saved {} account snapshots to {}", saved_count, snapshot_dir);
        Ok(saved_count)
    }

    /// 从QIFI快照恢复所有账户
    pub fn restore_from_snapshots(&self, snapshot_dir: &str) -> Result<usize, ExchangeError> {
        let snapshot_path = Path::new(snapshot_dir);

        if !snapshot_path.exists() {
            log::info!("No snapshot directory found at {}, skipping recovery", snapshot_dir);
            return Ok(0);
        }

        let mut restored_count = 0;

        for entry in std::fs::read_dir(snapshot_path)
            .map_err(|e| ExchangeError::IOError(format!("Read snapshot dir failed: {}", e)))?
        {
            let entry = entry
                .map_err(|e| ExchangeError::IOError(format!("Read dir entry failed: {}", e)))?;

            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // 读取QIFI文件
            let json = std::fs::read_to_string(&path)
                .map_err(|e| ExchangeError::IOError(format!("Read snapshot file failed: {}", e)))?;

            let qifi: QIFI = serde_json::from_str(&json)
                .map_err(|e| ExchangeError::SerializationError(format!("QIFI deserialization failed: {}", e)))?;

            // 恢复账户
            self.restore_account_from_qifi(qifi)?;

            restored_count += 1;
        }

        log::info!("Restored {} accounts from snapshots in {}", restored_count, snapshot_dir);
        Ok(restored_count)
    }

    /// 从单个QIFI恢复账户
    pub fn restore_account_from_qifi(&self, qifi: QIFI) -> Result<(), ExchangeError> {
        let account_id = qifi.account_cookie.clone();
        let user_id = qifi.portfolio.clone();
        let account_name = qifi.investor_name.clone(); // 从 QIFI investor_name 恢复账户名称

        // 检查账户是否已存在
        if self.accounts.contains_key(&account_id) {
            log::warn!("Account {} already exists, skipping restoration", account_id);
            return Ok(());
        }

        // 从QIFI创建QA_Account
        let account = QA_Account::new_from_qifi(qifi);

        // 存储账户
        self.accounts.insert(account_id.clone(), Arc::new(RwLock::new(account)));

        // 恢复元数据（从QIFI恢复）
        // 注意：account_type 和 created_at 在使用 update_metadata_for_recovery() 后会被正确恢复
        let metadata = AccountMetadata {
            user_id: user_id.clone(),
            account_name: if account_name.is_empty() { account_id.clone() } else { account_name }, // 从 QIFI investor_name 恢复
            account_type: AccountType::Individual, // 默认值，恢复时会被 update_metadata_for_recovery() 覆盖
            created_at: chrono::Utc::now().timestamp(), // 默认值，恢复时会被 update_metadata_for_recovery() 覆盖
        };
        self.metadata.insert(account_id.clone(), metadata);

        // 更新用户账户索引
        self.user_accounts
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(account_id.clone());

        // 绑定账户到用户（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            if let Err(e) = user_mgr.bind_account(&user_id, account_id.clone()) {
                log::warn!("Failed to bind restored account to user: {}", e);
            }
        }

        log::info!("Restored account {} (user: {}) from QIFI snapshot", account_id, user_id);
        Ok(())
    }

    /// 更新账户余额（仅用于恢复）
    ///
    /// 这是一个特殊方法，仅在从WAL恢复账户时使用。
    /// 正常交易过程中不应使用此方法，而应通过交易回报更新余额。
    ///
    /// # 参数
    /// - `account_id`: 账户ID
    /// - `balance`: 新的账户余额
    /// - `available`: 可用资金
    /// - `deposit`: 累计入金
    /// - `withdraw`: 累计出金
    ///
    /// # 安全性
    /// 此方法直接修改账户余额，绕过了正常的交易流程。
    /// 仅在恢复流程中使用，确保数据一致性由调用者负责。
    pub fn update_balance_for_recovery(
        &self,
        account_id: &str,
        balance: f64,
        available: f64,
        deposit: f64,
        withdraw: f64,
    ) -> Result<(), ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();

        // 直接设置字段值（通过 accounts QIFI 结构）
        acc.accounts.balance = balance;
        acc.accounts.available = available;
        acc.accounts.deposit = deposit;
        acc.accounts.withdraw = withdraw;

        // 重新计算 static_balance
        acc.accounts.static_balance = acc.accounts.pre_balance + deposit - withdraw;

        log::debug!(
            "Updated balance for account {} during recovery: balance={}, available={}, deposit={}, withdraw={}",
            account_id, balance, available, deposit, withdraw
        );

        Ok(())
    }

    /// 更新账户元数据（仅用于恢复）
    ///
    /// 这是一个特殊方法，仅在从WAL恢复账户时使用。
    /// 用于恢复 account_type 和 created_at 字段。
    ///
    /// # 参数
    /// - `account_id`: 账户ID
    /// - `account_type`: 账户类型
    /// - `created_at`: 创建时间戳
    pub fn update_metadata_for_recovery(
        &self,
        account_id: &str,
        account_type: AccountType,
        created_at: i64,
    ) -> Result<(), ExchangeError> {
        let mut metadata = self.metadata.get_mut(account_id)
            .ok_or_else(|| ExchangeError::AccountError(format!("Account not found: {}", account_id)))?;

        metadata.account_type = account_type;
        metadata.created_at = created_at;

        log::debug!(
            "Updated metadata for account {} during recovery: account_type={:?}, created_at={}",
            account_id, account_type, created_at
        );

        Ok(())
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
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "Test Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let result = mgr.open_account(req);
        assert!(result.is_ok());

        let account_id = result.unwrap();
        assert_eq!(account_id, "test_account");

        let account = mgr.get_account(&account_id);
        assert!(account.is_ok());
    }

    #[test]
    fn test_duplicate_account() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "Test Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req.clone()).unwrap();
        let result = mgr.open_account(req);
        assert!(result.is_err());
    }

    #[test]
    fn test_user_account_mapping() {
        let mgr = AccountManager::new();

        let user_id = "user_001";

        // 为同一用户创建多个账户
        let req1 = OpenAccountRequest {
            user_id: user_id.to_string(),
            account_id: Some("account_1".to_string()),
            account_name: "Account 1".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let req2 = OpenAccountRequest {
            user_id: user_id.to_string(),
            account_id: Some("account_2".to_string()),
            account_name: "Account 2".to_string(),
            init_cash: 50000.0,
            account_type: AccountType::Institutional,
        };

        mgr.open_account(req1).unwrap();
        mgr.open_account(req2).unwrap();

        // 验证用户账户映射
        let accounts = mgr.get_accounts_by_user(user_id);
        assert_eq!(accounts.len(), 2);

        let count = mgr.get_user_account_count(user_id);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_account_metadata() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "My Trading Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::MarketMaker,
        };

        let account_id = mgr.open_account(req).unwrap();

        let metadata = mgr.get_account_metadata(&account_id);
        assert!(metadata.is_some());

        let (user_id, account_name, account_type, _created_at) = metadata.unwrap();
        assert_eq!(user_id, "user_001");
        assert_eq!(account_name, "My Trading Account");
        assert_eq!(account_type, AccountType::MarketMaker);
    }

    #[test]
    fn test_verify_account_ownership() {
        let mgr = AccountManager::new();

        // 创建账户
        let req = OpenAccountRequest {
            user_id: "user_alice".to_string(),
            account_id: Some("ACC_alice_001".to_string()),
            account_name: "Alice's Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let account_id = mgr.open_account(req).unwrap();
        assert_eq!(account_id, "ACC_alice_001");

        // 测试1: 正确的用户验证账户所有权 - 应该成功
        let result = mgr.verify_account_ownership(&account_id, "user_alice");
        assert!(result.is_ok(), "Alice should own her account");

        // 测试2: 错误的用户验证账户所有权 - 应该失败
        let result = mgr.verify_account_ownership(&account_id, "user_bob");
        assert!(result.is_err(), "Bob should not own Alice's account");

        match result {
            Err(ExchangeError::PermissionDenied(msg)) => {
                assert!(msg.contains("does not belong to"), "Error message should indicate ownership mismatch");
                assert!(msg.contains("user_bob"), "Error message should mention the requesting user");
            },
            _ => panic!("Expected PermissionDenied error"),
        }

        // 测试3: 不存在的账户 - 应该失败
        let result = mgr.verify_account_ownership("ACC_nonexistent", "user_alice");
        assert!(result.is_err(), "Nonexistent account should fail");

        match result {
            Err(ExchangeError::AccountError(msg)) => {
                assert!(msg.contains("not found"), "Error message should indicate account not found");
            },
            _ => panic!("Expected AccountError for nonexistent account"),
        }
    }
}
