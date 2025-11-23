//! 用户管理模块
//!
//! 负责用户注册、登录、认证等功能

use crate::core::account_ext::{AccountType, OpenAccountRequest};
use crate::exchange::AccountManager;
use crate::ExchangeError;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub phone: String,
    pub password_hash: String, // 实际应该使用bcrypt等加密
    pub is_admin: bool,
    pub created_at: String,
}

/// 用户注册请求
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub phone: String,
    pub init_cash: Option<f64>,
}

/// 用户登录请求
#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 登录响应
#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
    pub token: String,
}

/// 用户管理器
pub struct UserManager {
    /// 用户信息映射 (user_id -> UserInfo)
    users: DashMap<String, UserInfo>,
    /// 用户名到用户ID的映射 (username -> user_id)
    username_map: DashMap<String, String>,
    /// 账户管理器引用
    account_mgr: Arc<AccountManager>,
    /// 用户序列号
    user_seq: std::sync::atomic::AtomicU64,
}

impl UserManager {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            users: DashMap::new(),
            username_map: DashMap::new(),
            account_mgr,
            user_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 生成用户ID
    fn generate_user_id(&self) -> String {
        let seq = self
            .user_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("user{:06}", seq)
    }

    /// 用户注册
    pub fn register(&self, req: RegisterRequest) -> Result<String, ExchangeError> {
        // 1. 验证用户名唯一性
        if self.username_map.contains_key(&req.username) {
            return Err(ExchangeError::AccountError(format!(
                "用户名已存在: {}",
                req.username
            )));
        }

        // 2. 生成用户ID
        let user_id = self.generate_user_id();

        // 3. 判断是否为第一个用户（自动设为管理员）
        let is_first_user = self.users.is_empty();

        // 4. 创建用户信息，使用bcrypt加密密码
        let password_hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
            .map_err(|e| ExchangeError::InternalError(format!("Password hashing failed: {}", e)))?;

        let user_info = UserInfo {
            user_id: user_id.clone(),
            username: req.username.clone(),
            email: req.email.clone(),
            phone: req.phone.clone(),
            password_hash,
            is_admin: is_first_user, // 第一个用户自动成为管理员
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        // 4. 自动创建关联的交易账户
        let account_req = OpenAccountRequest {
            user_id: user_id.clone(),
            account_id: None, // Auto-generate
            account_name: req.username.clone(),
            init_cash: req.init_cash.unwrap_or(100000.0), // 默认10万初始资金
            account_type: AccountType::Individual,
        };

        self.account_mgr.open_account(account_req)?;

        // 5. 保存用户信息
        self.users.insert(user_id.clone(), user_info);
        self.username_map
            .insert(req.username.clone(), user_id.clone());

        log::info!(
            "用户注册成功: user_id={}, username={}",
            user_id,
            req.username
        );

        Ok(user_id)
    }

    /// 用户登录
    pub fn login(&self, req: LoginRequest) -> Result<LoginResponse, ExchangeError> {
        // 1. 根据用户名查找用户ID
        let user_id = self
            .username_map
            .get(&req.username)
            .map(|r| r.value().clone())
            .ok_or_else(|| ExchangeError::AccountError(format!("用户不存在: {}", req.username)))?;

        // 2. 获取用户信息
        let user = self
            .users
            .get(&user_id)
            .ok_or_else(|| ExchangeError::AccountError(format!("用户信息不存在: {}", user_id)))?;

        // 3. 验证密码，使用bcrypt verify
        let password_valid = bcrypt::verify(&req.password, &user.password_hash).map_err(|e| {
            ExchangeError::InternalError(format!("Password verification failed: {}", e))
        })?;

        if !password_valid {
            return Err(ExchangeError::AuthError("密码错误".to_string()));
        }

        // 4. 生成JWT token
        let token = crate::utils::jwt::generate_token(&user_id, &user.username).map_err(|e| {
            ExchangeError::InternalError(format!("Failed to generate JWT token: {}", e))
        })?;

        log::info!(
            "用户登录成功: user_id={}, username={}",
            user_id,
            req.username
        );

        Ok(LoginResponse {
            user_id: user_id.clone(),
            username: user.username.clone(),
            is_admin: user.is_admin,
            token,
        })
    }

    /// 获取用户信息
    pub fn get_user(&self, user_id: &str) -> Result<UserInfo, ExchangeError> {
        self.users
            .get(user_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| ExchangeError::AccountError(format!("用户不存在: {}", user_id)))
    }

    /// 验证JWT token并返回用户ID
    pub fn verify_token(&self, token: &str) -> Result<String, ExchangeError> {
        let claims = crate::utils::jwt::verify_token(token)
            .map_err(|e| ExchangeError::AuthError(format!("Invalid token: {}", e)))?;

        // 检查用户是否存在
        if !self.users.contains_key(&claims.sub) {
            return Err(ExchangeError::AuthError("User not found".to_string()));
        }

        Ok(claims.sub)
    }

    /// 创建管理员账户
    pub fn create_admin(
        &self,
        username: String,
        password: String,
    ) -> Result<String, ExchangeError> {
        let user_id = self.generate_user_id();

        // 使用bcrypt加密密码
        let password_hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
            .map_err(|e| ExchangeError::InternalError(format!("Password hashing failed: {}", e)))?;

        let user_info = UserInfo {
            user_id: user_id.clone(),
            username: username.clone(),
            email: format!("{}@admin.com", username),
            phone: "".to_string(),
            password_hash,
            is_admin: true,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        // 创建管理员账户
        let account_req = OpenAccountRequest {
            user_id: user_id.clone(),
            account_id: None, // Auto-generate
            account_name: username.clone(),
            init_cash: 1000000.0, // 管理员100万初始资金
            account_type: AccountType::Individual,
        };

        self.account_mgr.open_account(account_req)?;

        self.users.insert(user_id.clone(), user_info);
        self.username_map.insert(username.clone(), user_id.clone());

        log::info!("管理员创建成功: user_id={}, username={}", user_id, username);

        Ok(user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_register_and_login() {
        let account_mgr = Arc::new(AccountManager::new());
        let user_mgr = UserManager::new(account_mgr.clone());

        // 测试注册
        let register_req = RegisterRequest {
            username: "test_user".to_string(),
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            phone: "13800138000".to_string(),
            init_cash: Some(100000.0),
        };

        let user_id = user_mgr.register(register_req).unwrap();
        assert!(!user_id.is_empty());

        // 测试登录成功
        let login_req = LoginRequest {
            username: "test_user".to_string(),
            password: "password123".to_string(),
        };

        let login_resp = user_mgr.login(login_req).unwrap();
        assert_eq!(login_resp.username, "test_user");
        assert!(!login_resp.token.is_empty());

        // 测试登录失败（密码错误）
        let wrong_login = LoginRequest {
            username: "test_user".to_string(),
            password: "wrong_password".to_string(),
        };

        assert!(user_mgr.login(wrong_login).is_err());

        // 测试重复注册
        let dup_register = RegisterRequest {
            username: "test_user".to_string(),
            email: "test2@example.com".to_string(),
            password: "password456".to_string(),
            phone: "13900139000".to_string(),
            init_cash: None,
        };

        assert!(user_mgr.register(dup_register).is_err());
    }
}
