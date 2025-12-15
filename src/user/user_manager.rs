//! 用户管理器
//!
//! 负责用户的注册、登录、查询、账户绑定等管理功能

use super::{User, UserLoginRequest, UserLoginResponse, UserRegisterRequest, UserRole, UserStatus};
use crate::ExchangeError;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// 用户管理器
pub struct UserManager {
    /// 用户映射 (user_id -> User)
    pub(crate) users: DashMap<String, Arc<RwLock<User>>>,

    /// 用户名索引 (username -> user_id)
    pub(crate) username_index: DashMap<String, String>,

    /// 手机号索引 (phone -> user_id)
    pub(crate) phone_index: DashMap<String, String>,

    /// 邮箱索引 (email -> user_id)
    pub(crate) email_index: DashMap<String, String>,

    /// 存储管理器（用于持久化）
    storage: Option<Arc<crate::storage::hybrid::OltpHybridStorage>>,
}

impl UserManager {
    /// 创建用户管理器
    pub fn new() -> Self {
        Self {
            users: DashMap::new(),
            username_index: DashMap::new(),
            phone_index: DashMap::new(),
            email_index: DashMap::new(),
            storage: None,
        }
    }

    /// 设置存储管理器
    pub fn set_storage(&mut self, storage: Arc<crate::storage::hybrid::OltpHybridStorage>) {
        self.storage = Some(storage);
    }

    /// 注册新用户 @yutiansut @quantaxis
    /// 第一个注册的用户自动成为管理员
    pub fn register(&self, req: UserRegisterRequest) -> Result<User> {
        // 检查用户名是否已存在
        if self.username_index.contains_key(&req.username) {
            return Err(ExchangeError::UserError(format!(
                "Username already exists: {}",
                req.username
            )));
        }

        // 检查手机号是否已存在
        if let Some(ref phone) = req.phone {
            if self.phone_index.contains_key(phone) {
                return Err(ExchangeError::UserError(format!(
                    "Phone already registered: {}",
                    phone
                )));
            }
        }

        // 检查邮箱是否已存在
        if let Some(ref email) = req.email {
            if self.email_index.contains_key(email) {
                return Err(ExchangeError::UserError(format!(
                    "Email already registered: {}",
                    email
                )));
            }
        }

        // 密码加密
        let password_hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
            .map_err(|e| ExchangeError::InternalError(format!("Password hashing failed: {}", e)))?;

        // 检查是否是第一个用户（自动成为管理员）@yutiansut @quantaxis
        let is_first_user = self.users.is_empty();

        // 创建用户
        let mut user = if is_first_user {
            log::info!("First user registration, granting Admin role");
            User::new_admin(req.username.clone(), password_hash)
        } else {
            User::new(req.username.clone(), password_hash)
        };
        user.phone = req.phone.clone();
        user.email = req.email.clone();
        user.real_name = req.real_name;
        user.id_card = req.id_card;

        let user_id = user.user_id.clone();

        // 存储用户
        self.users
            .insert(user_id.clone(), Arc::new(RwLock::new(user.clone())));

        // 更新索引
        self.username_index.insert(req.username, user_id.clone());
        if let Some(phone) = req.phone {
            self.phone_index.insert(phone, user_id.clone());
        }
        if let Some(email) = req.email {
            self.email_index.insert(email, user_id.clone());
        }

        // 持久化到WAL
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            let user_record = WalRecord::UserRegister {
                user_id: WalRecord::to_fixed_array_40(&user.user_id),
                username: WalRecord::to_fixed_array_32(&user.username),
                password_hash: WalRecord::to_fixed_array_64(&user.password_hash),
                phone: user
                    .phone
                    .as_ref()
                    .map(|s| WalRecord::to_fixed_array_16(s))
                    .unwrap_or([0u8; 16]),
                email: user
                    .email
                    .as_ref()
                    .map(|s| WalRecord::to_fixed_array_32(s))
                    .unwrap_or([0u8; 32]),
                created_at: user.created_at,
            };

            if let Err(e) = storage.write(user_record) {
                log::warn!("Failed to persist user registration to WAL: {}", e);
            }
        }

        log::info!("User registered: {} ({})", user.username, user.user_id);

        Ok(user)
    }

    /// 验证 JWT token 并返回用户ID
    pub fn verify_token(&self, token: &str) -> Result<String> {
        let claims = crate::utils::jwt::verify_token(token)
            .map_err(|e| ExchangeError::AuthError(format!("Invalid token: {}", e)))?;

        // 检查用户是否存在且未被冻结
        if let Some(user_arc) = self.users.get(&claims.sub) {
            let user = user_arc.read();
            if !user.is_active() {
                return Err(ExchangeError::AuthError(
                    "User is frozen or deleted".to_string(),
                ));
            }
            Ok(claims.sub)
        } else {
            Err(ExchangeError::AuthError("User not found".to_string()))
        }
    }

    /// 用户登录 @yutiansut @quantaxis
    /// 返回用户角色和权限信息
    pub fn login(&self, req: UserLoginRequest) -> Result<UserLoginResponse> {
        // 查找用户
        let user_id = self
            .username_index
            .get(&req.username)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", req.username)))?
            .clone();

        let user_arc = self
            .users
            .get(&user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", req.username)))?;

        let user = user_arc.read();

        // 检查用户状态
        if !user.is_active() {
            return Ok(UserLoginResponse {
                success: false,
                user_id: None,
                username: None,
                token: None,
                message: "User is frozen or deleted".to_string(),
                roles: None,
                is_admin: None,
                permissions: None,
            });
        }

        // 验证密码
        if !user.verify_password(&req.password) {
            return Ok(UserLoginResponse {
                success: false,
                user_id: None,
                username: None,
                token: None,
                message: "Invalid password".to_string(),
                roles: None,
                is_admin: None,
                permissions: None,
            });
        }

        // 生成JWT token
        let token =
            crate::utils::jwt::generate_token(&user.user_id, &user.username).map_err(|e| {
                ExchangeError::InternalError(format!("Failed to generate JWT token: {}", e))
            })?;

        // 获取用户权限列表 (字符串格式用于前端)
        let permissions: Vec<String> = user
            .get_permissions()
            .iter()
            .map(|p| format!("{:?}", p))
            .collect();

        Ok(UserLoginResponse {
            success: true,
            user_id: Some(user.user_id.clone()),
            username: Some(user.username.clone()),
            token: Some(token),
            message: "Login successful".to_string(),
            roles: Some(user.roles.clone()),
            is_admin: Some(user.is_admin()),
            permissions: Some(permissions),
        })
    }

    /// 获取用户
    pub fn get_user(&self, user_id: &str) -> Result<User> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let user = user_arc.read().clone();
        Ok(user)
    }

    /// 通过用户名获取用户
    pub fn get_user_by_username(&self, username: &str) -> Result<User> {
        let user_id = self
            .username_index
            .get(username)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", username)))?
            .clone();

        self.get_user(&user_id)
    }

    /// 获取用户的账户列表
    pub fn get_user_accounts(&self, user_id: &str) -> Result<Vec<String>> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let account_ids = user_arc.read().account_ids.clone();
        Ok(account_ids)
    }

    /// 绑定账户到用户
    pub fn bind_account(&self, user_id: &str, account_id: String) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let mut user = user_arc.write();
        user.add_account(account_id.clone());

        // 持久化到WAL
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            let bind_record = WalRecord::AccountBind {
                user_id: WalRecord::to_fixed_array_40(user_id),
                account_id: WalRecord::to_fixed_array_40(&account_id),
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            };

            if let Err(e) = storage.write(bind_record) {
                log::warn!("Failed to persist account binding to WAL: {}", e);
            }
        }

        log::info!("Account {} bound to user {}", account_id, user_id);

        Ok(())
    }

    /// 解绑账户
    pub fn unbind_account(&self, user_id: &str, account_id: &str) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let mut user = user_arc.write();
        user.remove_account(account_id);

        log::info!("Account {} unbound from user {}", account_id, user_id);

        Ok(())
    }

    /// 冻结用户
    pub fn freeze_user(&self, user_id: &str) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        user_arc.write().freeze();

        log::info!("User frozen: {}", user_id);

        Ok(())
    }

    /// 解冻用户
    pub fn unfreeze_user(&self, user_id: &str) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        user_arc.write().unfreeze();

        log::info!("User unfrozen: {}", user_id);

        Ok(())
    }

    /// 列出所有用户
    pub fn list_users(&self) -> Vec<User> {
        self.users
            .iter()
            .map(|entry| entry.value().read().clone())
            .collect()
    }

    /// 获取用户数量
    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    // ==================== RBAC 角色管理方法 @yutiansut @quantaxis ====================

    /// 为用户添加角色
    pub fn add_user_role(&self, user_id: &str, role: UserRole) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let mut user = user_arc.write();
        user.add_role(role);

        log::info!("Role {:?} added to user {}", role, user_id);
        Ok(())
    }

    /// 移除用户角色
    pub fn remove_user_role(&self, user_id: &str, role: UserRole) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let mut user = user_arc.write();
        user.remove_role(role);

        log::info!("Role {:?} removed from user {}", role, user_id);
        Ok(())
    }

    /// 设置用户角色列表
    pub fn set_user_roles(&self, user_id: &str, roles: Vec<UserRole>) -> Result<()> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let mut user = user_arc.write();
        user.set_roles(roles.clone());

        log::info!("Roles {:?} set for user {}", roles, user_id);
        Ok(())
    }

    /// 获取用户角色
    pub fn get_user_roles(&self, user_id: &str) -> Result<Vec<UserRole>> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let user = user_arc.read();
        Ok(user.roles.clone())
    }

    /// 检查用户是否拥有指定角色
    pub fn user_has_role(&self, user_id: &str, role: UserRole) -> Result<bool> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let user = user_arc.read();
        Ok(user.has_role(role))
    }

    /// 检查用户是否是管理员
    pub fn is_user_admin(&self, user_id: &str) -> Result<bool> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let user = user_arc.read();
        Ok(user.is_admin())
    }

    /// 检查用户是否拥有指定权限
    pub fn user_has_permission(&self, user_id: &str, permission: super::Permission) -> Result<bool> {
        let user_arc = self
            .users
            .get(user_id)
            .ok_or_else(|| ExchangeError::UserError(format!("User not found: {}", user_id)))?;

        let user = user_arc.read();
        Ok(user.has_permission(permission))
    }

    /// 获取所有管理员用户
    pub fn list_admins(&self) -> Vec<User> {
        self.users
            .iter()
            .filter_map(|entry| {
                let user = entry.value().read();
                if user.is_admin() {
                    Some(user.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// 按角色筛选用户
    pub fn list_users_by_role(&self, role: UserRole) -> Vec<User> {
        self.users
            .iter()
            .filter_map(|entry| {
                let user = entry.value().read();
                if user.has_role(role) {
                    Some(user.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_registration() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "testuser".to_string(),
            password: "password123".to_string(),
            phone: Some("13800138000".to_string()),
            email: Some("test@example.com".to_string()),
            real_name: Some("张三".to_string()),
            id_card: None,
        };

        let user = mgr.register(req).unwrap();
        assert_eq!(user.username, "testuser");
        assert!(user.phone.is_some());

        // 重复注册应该失败
        let req2 = UserRegisterRequest {
            username: "testuser".to_string(),
            password: "another".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        assert!(mgr.register(req2).is_err());
    }

    #[test]
    fn test_user_login() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "logintest".to_string(),
            password: "mypassword".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        mgr.register(req).unwrap();

        // 正确密码登录
        let login_req = UserLoginRequest {
            username: "logintest".to_string(),
            password: "mypassword".to_string(),
        };

        let resp = mgr.login(login_req).unwrap();
        assert!(resp.success);
        assert!(resp.user_id.is_some());

        // 错误密码登录
        let wrong_req = UserLoginRequest {
            username: "logintest".to_string(),
            password: "wrongpass".to_string(),
        };

        let resp = mgr.login(wrong_req).unwrap();
        assert!(!resp.success);
    }

    #[test]
    fn test_account_binding() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "bindtest".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let user = mgr.register(req).unwrap();

        // 绑定账户
        mgr.bind_account(&user.user_id, "account1".to_string())
            .unwrap();
        mgr.bind_account(&user.user_id, "account2".to_string())
            .unwrap();

        let accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(accounts.len(), 2);

        // 解绑账户
        mgr.unbind_account(&user.user_id, "account1").unwrap();
        let accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(accounts.len(), 1);
    }

    #[test]
    fn test_duplicate_phone_detection() {
        let mgr = UserManager::new();

        let req1 = UserRegisterRequest {
            username: "user1".to_string(),
            password: "pass1".to_string(),
            phone: Some("13800138000".to_string()),
            email: None,
            real_name: None,
            id_card: None,
        };

        mgr.register(req1).unwrap();

        // 相同手机号注册应该失败
        let req2 = UserRegisterRequest {
            username: "user2".to_string(),
            password: "pass2".to_string(),
            phone: Some("13800138000".to_string()),
            email: None,
            real_name: None,
            id_card: None,
        };

        let result = mgr.register(req2);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Phone already registered"));
    }

    #[test]
    fn test_duplicate_email_detection() {
        let mgr = UserManager::new();

        let req1 = UserRegisterRequest {
            username: "user1".to_string(),
            password: "pass1".to_string(),
            phone: None,
            email: Some("test@example.com".to_string()),
            real_name: None,
            id_card: None,
        };

        mgr.register(req1).unwrap();

        // 相同邮箱注册应该失败
        let req2 = UserRegisterRequest {
            username: "user2".to_string(),
            password: "pass2".to_string(),
            phone: None,
            email: Some("test@example.com".to_string()),
            real_name: None,
            id_card: None,
        };

        let result = mgr.register(req2);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Email already registered"));
    }

    #[test]
    fn test_user_freeze_and_unfreeze() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "freezetest".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let user = mgr.register(req).unwrap();

        // 冻结用户
        mgr.freeze_user(&user.user_id).unwrap();

        // 尝试登录应该失败
        let login_req = UserLoginRequest {
            username: "freezetest".to_string(),
            password: "password".to_string(),
        };

        let resp = mgr.login(login_req).unwrap();
        assert!(!resp.success);
        assert!(resp.message.contains("frozen"));

        // 解冻用户
        mgr.unfreeze_user(&user.user_id).unwrap();

        // 再次登录应该成功
        let login_req2 = UserLoginRequest {
            username: "freezetest".to_string(),
            password: "password".to_string(),
        };

        let resp2 = mgr.login(login_req2).unwrap();
        assert!(resp2.success);
    }

    #[test]
    fn test_get_user_by_username() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "querytest".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let registered_user = mgr.register(req).unwrap();

        // 通过用户名查询
        let found_user = mgr.get_user_by_username("querytest").unwrap();
        assert_eq!(found_user.user_id, registered_user.user_id);
        assert_eq!(found_user.username, "querytest");

        // 查询不存在的用户
        let result = mgr.get_user_by_username("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_user_list_and_count() {
        let mgr = UserManager::new();

        assert_eq!(mgr.user_count(), 0);

        // 注册3个用户
        for i in 1..=3 {
            let req = UserRegisterRequest {
                username: format!("user{}", i),
                password: "password".to_string(),
                phone: None,
                email: None,
                real_name: None,
                id_card: None,
            };
            mgr.register(req).unwrap();
        }

        assert_eq!(mgr.user_count(), 3);

        let users = mgr.list_users();
        assert_eq!(users.len(), 3);
    }

    #[test]
    fn test_bind_account_to_nonexistent_user() {
        let mgr = UserManager::new();

        let result = mgr.bind_account("nonexistent_user_id", "account1".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("User not found"));
    }

    #[test]
    fn test_password_verification() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "passtest".to_string(),
            password: "correct_password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        mgr.register(req).unwrap();

        // 正确密码
        let login_correct = UserLoginRequest {
            username: "passtest".to_string(),
            password: "correct_password".to_string(),
        };
        let resp = mgr.login(login_correct).unwrap();
        assert!(resp.success);

        // 错误密码
        let login_wrong = UserLoginRequest {
            username: "passtest".to_string(),
            password: "wrong_password".to_string(),
        };
        let resp = mgr.login(login_wrong).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.message, "Invalid password");
    }

    #[test]
    fn test_login_nonexistent_user() {
        let mgr = UserManager::new();

        let login_req = UserLoginRequest {
            username: "nonexistent".to_string(),
            password: "password".to_string(),
        };

        let result = mgr.login(login_req);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("User not found"));
    }
}
