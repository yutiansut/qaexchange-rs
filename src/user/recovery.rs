//! 用户数据恢复模块
//!
//! 从WAL恢复用户注册和账户绑定数据

use crate::storage::hybrid::OltpHybridStorage;
use crate::storage::wal::record::WalRecord;
use crate::user::{User, UserManager, UserStatus};
use crate::ExchangeError;
use std::collections::HashMap;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// 用户恢复统计
#[derive(Debug, Clone, Default)]
pub struct UserRecoveryStats {
    pub total_records: usize,
    pub user_register_records: usize,
    pub account_bind_records: usize,
    pub users_recovered: usize,
    pub recovery_time_ms: u128,
}

/// 用户数据恢复器
pub struct UserRecovery {
    storage: Arc<OltpHybridStorage>,
    user_manager: Arc<UserManager>,
}

impl UserRecovery {
    /// 创建恢复器
    pub fn new(storage: Arc<OltpHybridStorage>, user_manager: Arc<UserManager>) -> Self {
        Self {
            storage,
            user_manager,
        }
    }

    /// 从WAL恢复用户数据
    pub fn recover_users(&self, start_ts: i64, end_ts: i64) -> Result<UserRecoveryStats> {
        let start_time = std::time::Instant::now();

        let mut stats = UserRecoveryStats::default();
        let mut users_map: HashMap<String, User> = HashMap::new();
        let mut bindings: Vec<(String, String)> = Vec::new(); // (user_id, account_id)

        // 从WAL读取记录
        let records = self
            .storage
            .range_query(start_ts, end_ts)
            .map_err(|e| ExchangeError::StorageError(format!("Failed to query WAL: {}", e)))?;

        log::info!(
            "Recovering user data from {} records (ts range: {} - {})",
            records.len(),
            start_ts,
            end_ts
        );

        for (_timestamp, _sequence, record) in records {
            stats.total_records += 1;

            match record {
                WalRecord::UserRegister {
                    user_id,
                    username,
                    password_hash,
                    phone,
                    email,
                    created_at,
                    roles_bitmask,
                } => {
                    stats.user_register_records += 1;

                    let user_id_str = WalRecord::from_fixed_array(&user_id);
                    let username_str = WalRecord::from_fixed_array(&username);
                    let password_hash_str = WalRecord::from_fixed_array(&password_hash);

                    let phone_str = WalRecord::from_fixed_array(&phone);
                    let email_str = WalRecord::from_fixed_array(&email);

                    let mut user = User::new(username_str, password_hash_str);
                    user.user_id = user_id_str.clone();
                    user.created_at = created_at;
                    user.updated_at = created_at;

                    // 恢复用户角色 @yutiansut @quantaxis
                    user.roles = crate::user::UserRole::from_bitmask(roles_bitmask);

                    // 只有非空字符串才设置
                    if !phone_str.is_empty() {
                        user.phone = Some(phone_str);
                    }
                    if !email_str.is_empty() {
                        user.email = Some(email_str);
                    }

                    // 保留最新的用户记录（按时间戳）
                    users_map
                        .entry(user_id_str)
                        .and_modify(|existing| {
                            if user.created_at > existing.created_at {
                                *existing = user.clone();
                            }
                        })
                        .or_insert(user);
                }

                WalRecord::AccountBind {
                    user_id,
                    account_id,
                    ..
                } => {
                    stats.account_bind_records += 1;

                    let user_id_str = WalRecord::from_fixed_array(&user_id);
                    let account_id_str = WalRecord::from_fixed_array(&account_id);

                    bindings.push((user_id_str, account_id_str));
                }

                // 处理角色更新 @yutiansut @quantaxis
                WalRecord::UserRoleUpdate {
                    user_id,
                    roles_bitmask,
                    timestamp,
                } => {
                    let user_id_str = WalRecord::from_fixed_array(&user_id);
                    // 应用最新的角色更新到用户
                    if let Some(user) = users_map.get_mut(&user_id_str) {
                        // 只有更新时间戳更新的情况下才更新角色
                        if timestamp >= user.updated_at {
                            user.roles = crate::user::UserRole::from_bitmask(roles_bitmask);
                            user.updated_at = timestamp;
                        }
                    }
                }

                _ => {
                    // 忽略其他类型的记录
                }
            }
        }

        // 恢复用户到 UserManager
        for (user_id, user) in users_map {
            // 直接插入到 UserManager（绕过注册逻辑）
            self.user_manager.users.insert(
                user_id.clone(),
                Arc::new(parking_lot::RwLock::new(user.clone())),
            );

            // 更新索引
            self.user_manager
                .username_index
                .insert(user.username.clone(), user_id.clone());
            if let Some(ref phone) = user.phone {
                self.user_manager
                    .phone_index
                    .insert(phone.clone(), user_id.clone());
            }
            if let Some(ref email) = user.email {
                self.user_manager
                    .email_index
                    .insert(email.clone(), user_id.clone());
            }

            stats.users_recovered += 1;
        }

        // 恢复账户绑定关系
        for (user_id, account_id) in bindings {
            if let Some(user_arc) = self.user_manager.users.get(&user_id) {
                let mut user = user_arc.write();
                if !user.account_ids.contains(&account_id) {
                    user.account_ids.push(account_id);
                }
            }
        }

        stats.recovery_time_ms = start_time.elapsed().as_millis();

        log::info!(
            "User data recovery completed: {} users, {} bindings in {}ms",
            stats.users_recovered,
            stats.account_bind_records,
            stats.recovery_time_ms
        );

        Ok(stats)
    }

    /// 恢复最近N小时的用户数据
    pub fn recover_recent_hours(&self, hours: i64) -> Result<UserRecoveryStats> {
        let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let start_ts = end_ts - (hours * 3600 * 1_000_000_000);

        self.recover_users(start_ts, end_ts)
    }

    /// 恢复所有用户数据（从时间0开始）
    pub fn recover_all_users(&self) -> Result<UserRecoveryStats> {
        let end_ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let start_ts = 0;

        self.recover_users(start_ts, end_ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::hybrid::oltp::OltpHybridConfig;
    use crate::user::UserRegisterRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_user_recovery() {
        // 创建临时目录
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().to_str().unwrap();

        // 创建存储
        let config = OltpHybridConfig {
            base_path: storage_path.to_string(),
            memtable_size_bytes: 1024 * 1024,
            estimated_entry_size: 256,
            enable_olap_conversion: false, // 测试中禁用 OLAP 转换
            ..Default::default()
        };

        let storage = Arc::new(OltpHybridStorage::create("test_user", config).unwrap());

        // 创建用户管理器
        let mut user_manager = UserManager::new();
        user_manager.set_storage(storage.clone());
        let user_manager = Arc::new(user_manager);

        // 注册几个用户
        let req1 = UserRegisterRequest {
            username: "user1".to_string(),
            password: "password1".to_string(),
            phone: Some("13800138001".to_string()),
            email: None,
            real_name: None,
            id_card: None,
        };

        let req2 = UserRegisterRequest {
            username: "user2".to_string(),
            password: "password2".to_string(),
            phone: None,
            email: Some("user2@example.com".to_string()),
            real_name: None,
            id_card: None,
        };

        let user1 = user_manager.register(req1).unwrap();
        let user2 = user_manager.register(req2).unwrap();

        // 绑定账户
        user_manager
            .bind_account(&user1.user_id, "account1".to_string())
            .unwrap();
        user_manager
            .bind_account(&user1.user_id, "account2".to_string())
            .unwrap();
        user_manager
            .bind_account(&user2.user_id, "account3".to_string())
            .unwrap();

        // 等待WAL刷盘
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 创建新的用户管理器（模拟重启）
        let new_user_manager = Arc::new(UserManager::new());

        // 创建恢复器并恢复
        let recovery = UserRecovery::new(storage.clone(), new_user_manager.clone());
        let stats = recovery.recover_all_users().unwrap();

        // 验证恢复结果
        assert_eq!(stats.users_recovered, 2);
        assert_eq!(stats.user_register_records, 2);
        assert_eq!(stats.account_bind_records, 3);

        // 验证用户数据 - 使用用户名查询而不是user_id
        let recovered_user1 = new_user_manager.get_user_by_username("user1").unwrap();
        assert_eq!(recovered_user1.username, "user1");
        assert_eq!(recovered_user1.phone, Some("13800138001".to_string()));
        assert_eq!(recovered_user1.account_ids.len(), 2);
        // 验证恢复的user_id与原始user_id一致
        assert_eq!(recovered_user1.user_id, user1.user_id);

        let recovered_user2 = new_user_manager.get_user_by_username("user2").unwrap();
        assert_eq!(recovered_user2.username, "user2");
        assert_eq!(recovered_user2.email, Some("user2@example.com".to_string()));
        assert_eq!(recovered_user2.account_ids.len(), 1);
        assert_eq!(recovered_user2.user_id, user2.user_id);

        // 验证索引 - 通过username查询后再验证user_id
        let user_by_username = new_user_manager.get_user_by_username("user1").unwrap();
        assert_eq!(user_by_username.user_id, recovered_user1.user_id);
    }
}
