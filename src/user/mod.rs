//! 用户管理模块
//!
//! 提供用户注册、登录、账户绑定等功能
//! 用户(User) 1对多 账户(QA_Account) 的关系管理

pub mod recovery;
pub mod user_manager;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 用户实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// 用户ID (UUID)
    pub user_id: String,

    /// 用户名 (唯一)
    pub username: String,

    /// 密码哈希 (bcrypt)
    pub password_hash: String,

    /// 手机号
    pub phone: Option<String>,

    /// 邮箱
    pub email: Option<String>,

    /// 真实姓名
    pub real_name: Option<String>,

    /// 身份证号
    pub id_card: Option<String>,

    /// 关联的账户ID列表
    pub account_ids: Vec<String>,

    /// 创建时间 (Unix timestamp)
    pub created_at: i64,

    /// 更新时间
    pub updated_at: i64,

    /// 账户状态
    pub status: UserStatus,
}

/// 用户状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UserStatus {
    /// 正常
    Active,
    /// 已冻结
    Frozen,
    /// 已注销
    Deleted,
}

impl User {
    /// 创建新用户
    pub fn new(username: String, password_hash: String) -> Self {
        let now = Utc::now().timestamp();
        Self {
            user_id: Uuid::new_v4().to_string(),
            username,
            password_hash,
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
            account_ids: Vec::new(),
            created_at: now,
            updated_at: now,
            status: UserStatus::Active,
        }
    }

    /// 添加账户ID
    pub fn add_account(&mut self, account_id: String) {
        if !self.account_ids.contains(&account_id) {
            self.account_ids.push(account_id);
            self.updated_at = Utc::now().timestamp();
        }
    }

    /// 移除账户ID
    pub fn remove_account(&mut self, account_id: &str) {
        self.account_ids.retain(|id| id != account_id);
        self.updated_at = Utc::now().timestamp();
    }

    /// 验证密码
    pub fn verify_password(&self, password: &str) -> bool {
        bcrypt::verify(password, &self.password_hash).unwrap_or(false)
    }

    /// 更新密码
    pub fn update_password(&mut self, new_password: &str) -> Result<(), bcrypt::BcryptError> {
        self.password_hash = bcrypt::hash(new_password, bcrypt::DEFAULT_COST)?;
        self.updated_at = Utc::now().timestamp();
        Ok(())
    }

    /// 冻结用户
    pub fn freeze(&mut self) {
        self.status = UserStatus::Frozen;
        self.updated_at = Utc::now().timestamp();
    }

    /// 解冻用户
    pub fn unfreeze(&mut self) {
        self.status = UserStatus::Active;
        self.updated_at = Utc::now().timestamp();
    }

    /// 是否激活
    pub fn is_active(&self) -> bool {
        self.status == UserStatus::Active
    }
}

/// 用户注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRegisterRequest {
    pub username: String,
    pub password: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub real_name: Option<String>,
    pub id_card: Option<String>,
}

/// 用户登录请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLoginRequest {
    pub username: String,
    pub password: String,
}

/// 用户登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLoginResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub token: Option<String>,
    pub message: String,
}

/// 账户绑定请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBindRequest {
    pub user_id: String,
    pub account_name: String,
    pub init_cash: f64,
}

// 重新导出
pub use recovery::{UserRecovery, UserRecoveryStats};
pub use user_manager::UserManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let password = "test123";
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
        let user = User::new("testuser".to_string(), hash);

        assert_eq!(user.username, "testuser");
        assert!(user.verify_password("test123"));
        assert!(!user.verify_password("wrong"));
        assert!(user.is_active());
    }

    #[test]
    fn test_account_management() {
        let hash = bcrypt::hash("test", bcrypt::DEFAULT_COST).unwrap();
        let mut user = User::new("testuser".to_string(), hash);

        assert_eq!(user.account_ids.len(), 0);

        user.add_account("acc1".to_string());
        assert_eq!(user.account_ids.len(), 1);

        user.add_account("acc2".to_string());
        assert_eq!(user.account_ids.len(), 2);

        user.remove_account("acc1");
        assert_eq!(user.account_ids.len(), 1);
        assert!(user.account_ids.contains(&"acc2".to_string()));
    }

    #[test]
    fn test_user_status() {
        let hash = bcrypt::hash("test", bcrypt::DEFAULT_COST).unwrap();
        let mut user = User::new("testuser".to_string(), hash);

        assert!(user.is_active());

        user.freeze();
        assert!(!user.is_active());
        assert_eq!(user.status, UserStatus::Frozen);

        user.unfreeze();
        assert!(user.is_active());
        assert_eq!(user.status, UserStatus::Active);
    }
}
