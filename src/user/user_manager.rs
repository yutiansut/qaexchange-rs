//! 用户管理器
//!
//! 负责用户的注册、登录、查询、账户绑定等管理功能

use super::{Permission, User, UserLoginRequest, UserLoginResponse, UserRegisterRequest, UserRole, UserStatus};
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
                roles_bitmask: UserRole::roles_to_bitmask(&user.roles), // 保存角色 @yutiansut @quantaxis
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

        // 持久化角色更新到WAL @yutiansut @quantaxis
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;
            let record = WalRecord::UserRoleUpdate {
                user_id: WalRecord::to_fixed_array_40(user_id),
                roles_bitmask: UserRole::roles_to_bitmask(&user.roles),
                timestamp: chrono::Utc::now().timestamp(),
            };
            if let Err(e) = storage.write(record) {
                log::warn!("Failed to persist role update to WAL: {}", e);
            }
        }

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

        // 持久化角色更新到WAL @yutiansut @quantaxis
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;
            let record = WalRecord::UserRoleUpdate {
                user_id: WalRecord::to_fixed_array_40(user_id),
                roles_bitmask: UserRole::roles_to_bitmask(&user.roles),
                timestamp: chrono::Utc::now().timestamp(),
            };
            if let Err(e) = storage.write(record) {
                log::warn!("Failed to persist role update to WAL: {}", e);
            }
        }

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

        // 持久化角色更新到WAL @yutiansut @quantaxis
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;
            let record = WalRecord::UserRoleUpdate {
                user_id: WalRecord::to_fixed_array_40(user_id),
                roles_bitmask: UserRole::roles_to_bitmask(&roles),
                timestamp: chrono::Utc::now().timestamp(),
            };
            if let Err(e) = storage.write(record) {
                log::warn!("Failed to persist role update to WAL: {}", e);
            }
        }

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
    pub fn user_has_permission(&self, user_id: &str, permission: Permission) -> Result<bool> {
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

    // ====================================================================================
    // UserManager 基础测试 @yutiansut @quantaxis
    //
    // 用户管理器是交易系统的核心组件之一，负责：
    // 1. 用户注册与认证（密码使用 bcrypt 加密）
    // 2. 用户-账户关系管理（一个用户可绑定多个交易账户）
    // 3. RBAC（基于角色的访问控制）
    // 4. 用户状态管理（激活/冻结/删除）
    // ====================================================================================

    /// 测试 UserManager::new() 构造函数
    ///
    /// 验证逻辑：
    /// - 新建的管理器应该没有任何用户
    /// - 所有索引（用户名、手机、邮箱）都应为空
    /// - 存储管理器默认为 None
    #[test]
    fn test_user_manager_new() {
        let mgr = UserManager::new();

        // 验证初始状态：所有容器为空
        assert_eq!(mgr.users.len(), 0, "初始用户数应为0");
        assert_eq!(mgr.username_index.len(), 0, "用户名索引应为空");
        assert_eq!(mgr.phone_index.len(), 0, "手机号索引应为空");
        assert_eq!(mgr.email_index.len(), 0, "邮箱索引应为空");
        assert!(mgr.storage.is_none(), "存储管理器默认为None");
    }

    /// 测试 Default trait 实现
    ///
    /// 验证 Default::default() 与 new() 行为一致
    #[test]
    fn test_user_manager_default() {
        let mgr = UserManager::default();

        assert_eq!(mgr.user_count(), 0);
    }

    /// 测试第一个用户自动成为管理员
    ///
    /// 业务规则：
    /// - 系统中第一个注册的用户自动获得 Admin 角色
    /// - 后续注册的用户默认为普通用户（Trader 角色）
    /// - 这是为了确保系统有初始管理员，无需人工干预
    ///
    /// 权限差异：
    /// - Admin: 拥有所有权限（用户管理、系统管理、交易管理等）
    /// - Trader: 仅有交易相关权限
    #[test]
    fn test_first_user_becomes_admin() {
        let mgr = UserManager::new();

        // 注册第一个用户
        let req1 = UserRegisterRequest {
            username: "first_user".to_string(),
            password: "password123".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let first_user = mgr.register(req1).unwrap();

        // 验证：第一个用户应该是管理员
        assert!(
            first_user.is_admin(),
            "第一个注册的用户应自动成为管理员"
        );
        assert!(
            first_user.has_role(UserRole::Admin),
            "第一个用户应拥有 Admin 角色"
        );

        // 注册第二个用户
        let req2 = UserRegisterRequest {
            username: "second_user".to_string(),
            password: "password123".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let second_user = mgr.register(req2).unwrap();

        // 验证：第二个用户不是管理员
        assert!(
            !second_user.is_admin(),
            "后续用户不应自动成为管理员"
        );
        assert!(
            second_user.has_role(UserRole::Trader),
            "普通用户应拥有 Trader 角色"
        );
    }

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

    // ====================================================================================
    // RBAC 角色管理测试 @yutiansut @quantaxis
    //
    // RBAC (Role-Based Access Control) 是一种基于角色的访问控制系统
    // 核心概念:
    // - 角色 (Role): 定义一组权限的集合 (Admin, Trader, Analyst, etc.)
    // - 权限 (Permission): 具体的操作权限 (Trade, ViewAccount, etc.)
    // - 用户角色关系: 一个用户可以拥有多个角色
    // - 权限汇总: 用户的权限 = 所有角色权限的并集
    //
    // 角色层级 (按优先级从高到低):
    // - Admin (100): 系统管理员，拥有所有权限
    // - RiskManager (80): 风控员，可强平、冻结账户
    // - Settlement (70): 结算员，可执行结算
    // - Trader (50): 交易员，可下单、撤单
    // - Analyst (30): 分析师，只读 + 数据分析
    // - ReadOnly (10): 只读用户，最小权限
    // ====================================================================================

    /// 测试添加用户角色
    ///
    /// 业务场景：
    /// - 用户晋升为风控员，需要添加 RiskManager 角色
    /// - 用户兼任多个岗位，需要同时拥有多个角色
    ///
    /// 实现逻辑：
    /// - 通过 `add_user_role` 为用户添加角色
    /// - 角色不重复添加（幂等性）
    /// - 用户权限自动扩展为所有角色权限的并集
    #[test]
    fn test_add_user_role() {
        let mgr = UserManager::new();

        // 先注册第一个用户（会成为Admin）
        let _ = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册第二个用户（普通Trader）
        let user = mgr.register(UserRegisterRequest {
            username: "trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 验证初始角色
        assert!(user.has_role(UserRole::Trader), "新用户默认应是Trader");
        assert!(!user.has_role(UserRole::RiskManager), "新用户不应有RiskManager角色");

        // 添加风控角色
        mgr.add_user_role(&user.user_id, UserRole::RiskManager).unwrap();

        // 验证角色已添加
        let roles = mgr.get_user_roles(&user.user_id).unwrap();
        assert!(roles.contains(&UserRole::Trader), "应保留原有Trader角色");
        assert!(roles.contains(&UserRole::RiskManager), "应添加RiskManager角色");
        assert_eq!(roles.len(), 2, "应只有2个角色");
    }

    /// 测试移除用户角色
    ///
    /// 业务场景：
    /// - 用户离开风控岗位，需要移除 RiskManager 角色
    /// - 降级用户权限
    ///
    /// 安全考虑：
    /// - 移除角色后，相关权限立即失效
    /// - 应避免移除用户的所有角色（至少保留一个）
    #[test]
    fn test_remove_user_role() {
        let mgr = UserManager::new();

        // 先注册第一个用户（会成为Admin）
        let _ = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册第二个用户
        let user = mgr.register(UserRegisterRequest {
            username: "trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 添加多个角色
        mgr.add_user_role(&user.user_id, UserRole::RiskManager).unwrap();
        mgr.add_user_role(&user.user_id, UserRole::Settlement).unwrap();

        let roles_before = mgr.get_user_roles(&user.user_id).unwrap();
        assert_eq!(roles_before.len(), 3, "应有3个角色");

        // 移除风控角色
        mgr.remove_user_role(&user.user_id, UserRole::RiskManager).unwrap();

        let roles_after = mgr.get_user_roles(&user.user_id).unwrap();
        assert_eq!(roles_after.len(), 2, "移除后应剩2个角色");
        assert!(!roles_after.contains(&UserRole::RiskManager), "RiskManager应被移除");
        assert!(roles_after.contains(&UserRole::Trader), "Trader应保留");
        assert!(roles_after.contains(&UserRole::Settlement), "Settlement应保留");
    }

    /// 测试设置用户角色列表
    ///
    /// 业务场景：
    /// - 管理员批量调整用户权限
    /// - 用户岗位变更，需要完全替换角色
    ///
    /// 与 add_role 的区别：
    /// - set_user_roles: 替换所有角色（全量更新）
    /// - add_user_role: 添加单个角色（增量更新）
    #[test]
    fn test_set_user_roles() {
        let mgr = UserManager::new();

        // 先注册第一个用户（会成为Admin）
        let _ = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册第二个用户
        let user = mgr.register(UserRegisterRequest {
            username: "trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 原角色
        let original_roles = mgr.get_user_roles(&user.user_id).unwrap();
        assert!(original_roles.contains(&UserRole::Trader));

        // 设置新角色列表（完全替换）
        let new_roles = vec![UserRole::Analyst, UserRole::ReadOnly];
        mgr.set_user_roles(&user.user_id, new_roles.clone()).unwrap();

        // 验证角色已完全替换
        let current_roles = mgr.get_user_roles(&user.user_id).unwrap();
        assert!(!current_roles.contains(&UserRole::Trader), "原Trader角色应被移除");
        assert!(current_roles.contains(&UserRole::Analyst), "应有Analyst角色");
        assert!(current_roles.contains(&UserRole::ReadOnly), "应有ReadOnly角色");
        assert_eq!(current_roles.len(), 2);
    }

    /// 测试检查用户是否拥有指定角色
    ///
    /// 使用场景：
    /// - 权限检查前先验证用户角色
    /// - 条件性显示UI元素
    #[test]
    fn test_user_has_role() {
        let mgr = UserManager::new();

        // 注册第一个用户（Admin）
        let admin = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 验证Admin角色
        assert!(mgr.user_has_role(&admin.user_id, UserRole::Admin).unwrap());
        assert!(!mgr.user_has_role(&admin.user_id, UserRole::Trader).unwrap());
        assert!(!mgr.user_has_role(&admin.user_id, UserRole::RiskManager).unwrap());
    }

    /// 测试检查用户是否是管理员
    ///
    /// Admin 角色的特殊性：
    /// - 拥有所有权限（Permission::all()）
    /// - 优先级最高 (100)
    /// - 只有第一个注册用户自动获得
    /// - 后续用户需要手动授予
    #[test]
    fn test_is_user_admin() {
        let mgr = UserManager::new();

        // 注册第一个用户（自动成为Admin）
        let first_user = mgr.register(UserRegisterRequest {
            username: "first".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册第二个用户（普通Trader）
        let second_user = mgr.register(UserRegisterRequest {
            username: "second".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 验证管理员状态
        assert!(
            mgr.is_user_admin(&first_user.user_id).unwrap(),
            "第一个用户应是Admin"
        );
        assert!(
            !mgr.is_user_admin(&second_user.user_id).unwrap(),
            "第二个用户不应是Admin"
        );

        // 手动授予Admin角色后再验证
        mgr.add_user_role(&second_user.user_id, UserRole::Admin).unwrap();
        assert!(
            mgr.is_user_admin(&second_user.user_id).unwrap(),
            "手动添加Admin后应是Admin"
        );
    }

    /// 测试检查用户是否拥有指定权限
    ///
    /// 权限检查逻辑：
    /// 1. 如果用户是 Admin，直接返回 true（拥有所有权限）
    /// 2. 否则，汇总用户所有角色的权限，检查是否包含指定权限
    ///
    /// 示例：
    /// - Trader 有 Trade 权限，无 ForceLiquidate 权限
    /// - RiskManager 有 ForceLiquidate 权限，无 Trade 权限
    /// - 同时拥有 Trader + RiskManager 的用户，两个权限都有
    #[test]
    fn test_user_has_permission() {
        let mgr = UserManager::new();

        // 注册Admin
        let admin = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册Trader
        let trader = mgr.register(UserRegisterRequest {
            username: "trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // Admin 拥有所有权限
        assert!(
            mgr.user_has_permission(&admin.user_id, Permission::Trade).unwrap(),
            "Admin应有Trade权限"
        );
        assert!(
            mgr.user_has_permission(&admin.user_id, Permission::ForceLiquidate).unwrap(),
            "Admin应有ForceLiquidate权限"
        );
        assert!(
            mgr.user_has_permission(&admin.user_id, Permission::ExecuteSettlement).unwrap(),
            "Admin应有ExecuteSettlement权限"
        );

        // Trader 有交易权限，无风控权限
        assert!(
            mgr.user_has_permission(&trader.user_id, Permission::Trade).unwrap(),
            "Trader应有Trade权限"
        );
        assert!(
            mgr.user_has_permission(&trader.user_id, Permission::CancelOrder).unwrap(),
            "Trader应有CancelOrder权限"
        );
        assert!(
            !mgr.user_has_permission(&trader.user_id, Permission::ForceLiquidate).unwrap(),
            "Trader不应有ForceLiquidate权限"
        );
        assert!(
            !mgr.user_has_permission(&trader.user_id, Permission::ExecuteSettlement).unwrap(),
            "Trader不应有ExecuteSettlement权限"
        );
    }

    /// 测试列出所有管理员
    ///
    /// 使用场景：
    /// - 系统监控：查看当前有哪些管理员
    /// - 审计日志：记录管理员操作
    /// - 通知系统：向所有管理员发送重要通知
    #[test]
    fn test_list_admins() {
        let mgr = UserManager::new();

        // 注册第一个用户（自动Admin）
        let admin1 = mgr.register(UserRegisterRequest {
            username: "admin1".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册普通用户
        let trader = mgr.register(UserRegisterRequest {
            username: "trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 手动提升为Admin
        mgr.add_user_role(&trader.user_id, UserRole::Admin).unwrap();

        // 验证管理员列表
        let admins = mgr.list_admins();
        assert_eq!(admins.len(), 2, "应有2个管理员");

        let admin_usernames: Vec<String> = admins.iter().map(|u| u.username.clone()).collect();
        assert!(admin_usernames.contains(&"admin1".to_string()));
        // 注意：trader已被提升为Admin
        let admin_ids: Vec<String> = admins.iter().map(|u| u.user_id.clone()).collect();
        assert!(admin_ids.contains(&admin1.user_id));
        assert!(admin_ids.contains(&trader.user_id));
    }

    /// 测试按角色筛选用户
    ///
    /// 使用场景：
    /// - 批量通知特定角色的用户
    /// - 生成按角色分组的报表
    /// - 管理特定岗位的人员
    #[test]
    fn test_list_users_by_role() {
        let mgr = UserManager::new();

        // 注册Admin
        let _ = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册3个Trader
        for i in 1..=3 {
            mgr.register(UserRegisterRequest {
                username: format!("trader{}", i),
                password: "password".to_string(),
                phone: None,
                email: None,
                real_name: None,
                id_card: None,
            }).unwrap();
        }

        // 注册2个用户并设为RiskManager
        for i in 1..=2 {
            let user = mgr.register(UserRegisterRequest {
                username: format!("risk{}", i),
                password: "password".to_string(),
                phone: None,
                email: None,
                real_name: None,
                id_card: None,
            }).unwrap();
            mgr.set_user_roles(&user.user_id, vec![UserRole::RiskManager]).unwrap();
        }

        // 验证按角色筛选
        let traders = mgr.list_users_by_role(UserRole::Trader);
        assert_eq!(traders.len(), 3, "应有3个Trader");

        let risk_managers = mgr.list_users_by_role(UserRole::RiskManager);
        assert_eq!(risk_managers.len(), 2, "应有2个RiskManager");

        let admins = mgr.list_users_by_role(UserRole::Admin);
        assert_eq!(admins.len(), 1, "应有1个Admin");

        let analysts = mgr.list_users_by_role(UserRole::Analyst);
        assert_eq!(analysts.len(), 0, "不应有Analyst");
    }

    // ====================================================================================
    // 权限继承与组合测试 @yutiansut @quantaxis
    //
    // 多角色用户的权限计算：
    // - 权限是所有角色权限的并集 (Union)
    // - 不是交集 (Intersection)
    // - 角色之间不冲突，权限只增不减
    //
    // 示例：
    // - Trader权限: {Trade, CancelOrder, ViewOwnAccount, ...}
    // - Analyst权限: {ViewOwnAccount, ViewStatistics, ExportData, ...}
    // - Trader + Analyst = {Trade, CancelOrder, ViewStatistics, ExportData, ...}
    // ====================================================================================

    /// 测试多角色用户的权限汇总
    ///
    /// 验证：权限是所有角色权限的并集
    #[test]
    fn test_multi_role_permission_union() {
        let mgr = UserManager::new();

        // 注册Admin
        let _ = mgr.register(UserRegisterRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册用户并赋予多角色
        let user = mgr.register(UserRegisterRequest {
            username: "multi_role_user".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 初始只有 Trader 角色
        assert!(
            mgr.user_has_permission(&user.user_id, Permission::Trade).unwrap(),
            "Trader应有Trade权限"
        );
        assert!(
            !mgr.user_has_permission(&user.user_id, Permission::ExportData).unwrap(),
            "Trader不应有ExportData权限（Analyst专属）"
        );

        // 添加 Analyst 角色
        mgr.add_user_role(&user.user_id, UserRole::Analyst).unwrap();

        // 现在应该同时拥有 Trader 和 Analyst 的权限
        assert!(
            mgr.user_has_permission(&user.user_id, Permission::Trade).unwrap(),
            "添加Analyst后仍应有Trade权限"
        );
        assert!(
            mgr.user_has_permission(&user.user_id, Permission::ExportData).unwrap(),
            "添加Analyst后应有ExportData权限"
        );
        assert!(
            mgr.user_has_permission(&user.user_id, Permission::ViewStatistics).unwrap(),
            "添加Analyst后应有ViewStatistics权限"
        );
    }

    /// 测试角色优先级
    ///
    /// 角色优先级用于：
    /// - 确定用户的主要角色（primary_role）
    /// - UI显示时的角色排序
    /// - 权限冲突时的决策依据（目前未实现冲突处理）
    #[test]
    fn test_role_priority() {
        // 验证角色优先级值
        assert_eq!(UserRole::Admin.priority(), 100);
        assert_eq!(UserRole::RiskManager.priority(), 80);
        assert_eq!(UserRole::Settlement.priority(), 70);
        assert_eq!(UserRole::Trader.priority(), 50);
        assert_eq!(UserRole::Analyst.priority(), 30);
        assert_eq!(UserRole::ReadOnly.priority(), 10);

        // 验证优先级顺序：Admin > RiskManager > Settlement > Trader > Analyst > ReadOnly
        assert!(UserRole::Admin.priority() > UserRole::RiskManager.priority());
        assert!(UserRole::RiskManager.priority() > UserRole::Settlement.priority());
        assert!(UserRole::Settlement.priority() > UserRole::Trader.priority());
        assert!(UserRole::Trader.priority() > UserRole::Analyst.priority());
        assert!(UserRole::Analyst.priority() > UserRole::ReadOnly.priority());
    }

    // ====================================================================================
    // 用户-账户绑定测试 @yutiansut @quantaxis
    //
    // 用户-账户关系模型：
    // - 一个用户可以绑定多个交易账户 (1:N 关系)
    // - 账户ID存储在 User.account_ids 列表中
    // - 绑定关系通过 UserManager.bind_account() 建立
    // - 解绑关系通过 UserManager.unbind_account() 删除
    //
    // QIFI 数据结构对应：
    // - User.user_id 对应 QIFI 中的某个唯一标识
    // - 绑定的账户 account_id 对应 QIFI 中的 account_cookie
    // - 账户的 portfolio_cookie 字段存储 user_id 以建立反向关联
    // ====================================================================================

    /// 测试重复绑定同一账户（幂等性）
    ///
    /// 业务规则：
    /// - 同一账户只能被绑定一次
    /// - 重复绑定不会报错，但不会重复添加
    /// - 这保证了接口的幂等性，避免因重试导致数据不一致
    #[test]
    fn test_bind_account_idempotent() {
        let mgr = UserManager::new();

        let user = mgr.register(UserRegisterRequest {
            username: "bind_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 绑定同一账户多次
        mgr.bind_account(&user.user_id, "account1".to_string()).unwrap();
        mgr.bind_account(&user.user_id, "account1".to_string()).unwrap();
        mgr.bind_account(&user.user_id, "account1".to_string()).unwrap();

        // 账户列表应只有一个
        let accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(accounts.len(), 1, "重复绑定不应导致重复记录");
        assert_eq!(accounts[0], "account1");
    }

    /// 测试解绑不存在的账户
    ///
    /// 业务规则：
    /// - 解绑不存在的账户不会报错
    /// - 这保证了接口的幂等性
    #[test]
    fn test_unbind_nonexistent_account() {
        let mgr = UserManager::new();

        let user = mgr.register(UserRegisterRequest {
            username: "unbind_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 解绑不存在的账户应该不报错
        let result = mgr.unbind_account(&user.user_id, "nonexistent_account");
        assert!(result.is_ok(), "解绑不存在的账户不应报错");
    }

    /// 测试用户账户绑定的完整生命周期
    ///
    /// 场景：用户从开户到销户的完整流程
    /// 1. 用户注册
    /// 2. 绑定账户A
    /// 3. 绑定账户B
    /// 4. 查询账户列表
    /// 5. 解绑账户A
    /// 6. 验证只剩账户B
    #[test]
    fn test_account_binding_lifecycle() {
        let mgr = UserManager::new();

        // 1. 用户注册
        let user = mgr.register(UserRegisterRequest {
            username: "lifecycle_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 初始无账户
        let initial_accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(initial_accounts.len(), 0, "初始应无绑定账户");

        // 2. 绑定账户A
        mgr.bind_account(&user.user_id, "ACC_A".to_string()).unwrap();

        // 3. 绑定账户B
        mgr.bind_account(&user.user_id, "ACC_B".to_string()).unwrap();

        // 4. 查询账户列表
        let accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(accounts.len(), 2, "应有2个绑定账户");
        assert!(accounts.contains(&"ACC_A".to_string()));
        assert!(accounts.contains(&"ACC_B".to_string()));

        // 5. 解绑账户A
        mgr.unbind_account(&user.user_id, "ACC_A").unwrap();

        // 6. 验证只剩账户B
        let final_accounts = mgr.get_user_accounts(&user.user_id).unwrap();
        assert_eq!(final_accounts.len(), 1, "应只剩1个账户");
        assert_eq!(final_accounts[0], "ACC_B");
    }

    // ====================================================================================
    // 用户状态管理测试 @yutiansut @quantaxis
    //
    // 用户状态枚举：
    // - Active: 正常状态，可以登录和操作
    // - Frozen: 冻结状态，不能登录，账户操作受限
    // - Deleted: 已删除，不能登录也不能恢复（软删除）
    //
    // 状态转换规则：
    // - Active → Frozen: freeze_user()
    // - Frozen → Active: unfreeze_user()
    // - * → Deleted: delete_user() (目前未实现)
    //
    // 状态对操作的影响：
    // - Frozen 用户：登录失败，返回 "frozen" 错误
    // - Deleted 用户：登录失败，返回 "deleted" 错误
    // ====================================================================================

    /// 测试冻结用户后尝试登录
    ///
    /// 业务场景：
    /// - 用户违规，管理员冻结其账户
    /// - 风控触发，自动冻结高风险用户
    ///
    /// 预期行为：
    /// - 冻结后用户无法登录
    /// - 登录返回 success=false，message 包含 "frozen"
    #[test]
    fn test_frozen_user_cannot_login() {
        let mgr = UserManager::new();

        let user = mgr.register(UserRegisterRequest {
            username: "freeze_login_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 正常登录应成功
        let login_req = UserLoginRequest {
            username: "freeze_login_test".to_string(),
            password: "password".to_string(),
        };
        let resp = mgr.login(login_req.clone()).unwrap();
        assert!(resp.success, "冻结前登录应成功");

        // 冻结用户
        mgr.freeze_user(&user.user_id).unwrap();

        // 冻结后登录应失败
        let resp2 = mgr.login(login_req).unwrap();
        assert!(!resp2.success, "冻结后登录应失败");
        assert!(
            resp2.message.contains("frozen") || resp2.message.contains("deleted"),
            "错误消息应包含frozen或deleted"
        );
    }

    /// 测试冻结后解冻恢复
    ///
    /// 业务场景：
    /// - 用户申诉成功，管理员解除冻结
    /// - 风控误判，恢复用户权限
    #[test]
    fn test_unfreeze_restores_login() {
        let mgr = UserManager::new();

        let user = mgr.register(UserRegisterRequest {
            username: "unfreeze_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        let login_req = UserLoginRequest {
            username: "unfreeze_test".to_string(),
            password: "password".to_string(),
        };

        // 冻结
        mgr.freeze_user(&user.user_id).unwrap();
        let resp1 = mgr.login(login_req.clone()).unwrap();
        assert!(!resp1.success, "冻结后不能登录");

        // 解冻
        mgr.unfreeze_user(&user.user_id).unwrap();
        let resp2 = mgr.login(login_req).unwrap();
        assert!(resp2.success, "解冻后应能登录");
    }

    // ====================================================================================
    // 错误处理测试 @yutiansut @quantaxis
    //
    // 错误类型：
    // - ExchangeError::UserError: 用户相关错误（不存在、重复等）
    // - ExchangeError::AuthError: 认证错误（密码错误、token无效）
    // - ExchangeError::InternalError: 内部错误（加密失败等）
    // ====================================================================================

    /// 测试对不存在用户的角色操作
    #[test]
    fn test_role_operations_on_nonexistent_user() {
        let mgr = UserManager::new();

        // 添加角色应失败
        let result = mgr.add_user_role("nonexistent", UserRole::Admin);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("User not found"));

        // 移除角色应失败
        let result = mgr.remove_user_role("nonexistent", UserRole::Admin);
        assert!(result.is_err());

        // 设置角色应失败
        let result = mgr.set_user_roles("nonexistent", vec![UserRole::Trader]);
        assert!(result.is_err());

        // 获取角色应失败
        let result = mgr.get_user_roles("nonexistent");
        assert!(result.is_err());

        // 检查角色应失败
        let result = mgr.user_has_role("nonexistent", UserRole::Admin);
        assert!(result.is_err());

        // 检查管理员应失败
        let result = mgr.is_user_admin("nonexistent");
        assert!(result.is_err());

        // 检查权限应失败
        let result = mgr.user_has_permission("nonexistent", Permission::Trade);
        assert!(result.is_err());
    }

    /// 测试冻结/解冻不存在的用户
    #[test]
    fn test_freeze_nonexistent_user() {
        let mgr = UserManager::new();

        let result = mgr.freeze_user("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("User not found"));

        let result = mgr.unfreeze_user("nonexistent");
        assert!(result.is_err());
    }

    /// 测试获取不存在用户的账户列表
    #[test]
    fn test_get_accounts_nonexistent_user() {
        let mgr = UserManager::new();

        let result = mgr.get_user_accounts("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("User not found"));
    }

    // ====================================================================================
    // 并发测试 @yutiansut @quantaxis
    //
    // 并发安全保证：
    // - DashMap: 无锁并发HashMap，支持高并发读写
    // - parking_lot::RwLock: 高性能读写锁，保护User对象
    // - Arc: 原子引用计数，确保线程安全的共享
    //
    // 测试目标：
    // - 验证多线程同时注册用户不会数据竞争
    // - 验证多线程同时修改同一用户的角色不会出错
    // - 验证多线程同时绑定账户不会重复
    // ====================================================================================

    /// 测试并发注册用户
    ///
    /// 验证：多线程同时注册不会导致数据不一致
    #[test]
    fn test_concurrent_user_registration() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(UserManager::new());
        let mut handles = vec![];

        // 10个线程同时注册用户
        for i in 0..10 {
            let mgr_clone = mgr.clone();
            handles.push(thread::spawn(move || {
                let req = UserRegisterRequest {
                    username: format!("concurrent_user_{}", i),
                    password: "password".to_string(),
                    phone: Some(format!("1380013800{}", i)),
                    email: Some(format!("user{}@test.com", i)),
                    real_name: None,
                    id_card: None,
                };
                mgr_clone.register(req)
            }));
        }

        // 等待所有线程完成
        let mut success_count = 0;
        for handle in handles {
            if handle.join().unwrap().is_ok() {
                success_count += 1;
            }
        }

        // 所有注册都应成功
        assert_eq!(success_count, 10, "所有并发注册都应成功");
        assert_eq!(mgr.user_count(), 10, "应有10个用户");
    }

    /// 测试并发修改用户角色
    ///
    /// 验证：多线程同时修改同一用户的角色不会导致数据损坏
    #[test]
    fn test_concurrent_role_modification() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(UserManager::new());

        // 注册一个用户
        let user = mgr.register(UserRegisterRequest {
            username: "concurrent_role_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        let user_id = user.user_id.clone();
        let mut handles = vec![];

        // 多个线程同时添加不同角色
        let roles = vec![
            UserRole::Analyst,
            UserRole::RiskManager,
            UserRole::Settlement,
            UserRole::ReadOnly,
        ];

        for role in roles {
            let mgr_clone = mgr.clone();
            let user_id_clone = user_id.clone();
            handles.push(thread::spawn(move || {
                mgr_clone.add_user_role(&user_id_clone, role)
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }

        // 验证所有角色都被添加（第一个用户是Admin，所以不会有Trader默认角色）
        let final_roles = mgr.get_user_roles(&user_id).unwrap();
        // 第一个用户是Admin，所以初始角色是Admin
        assert!(final_roles.contains(&UserRole::Admin), "应有Admin角色");
        assert!(final_roles.contains(&UserRole::Analyst), "应有Analyst角色");
        assert!(final_roles.contains(&UserRole::RiskManager), "应有RiskManager角色");
        assert!(final_roles.contains(&UserRole::Settlement), "应有Settlement角色");
        assert!(final_roles.contains(&UserRole::ReadOnly), "应有ReadOnly角色");
    }

    /// 测试并发绑定账户
    ///
    /// 验证：多线程同时绑定账户不会导致重复记录
    #[test]
    fn test_concurrent_account_binding() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(UserManager::new());

        let user = mgr.register(UserRegisterRequest {
            username: "concurrent_bind_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        let user_id = user.user_id.clone();
        let mut handles = vec![];

        // 多个线程同时绑定相同账户（测试幂等性）
        for _ in 0..5 {
            let mgr_clone = mgr.clone();
            let user_id_clone = user_id.clone();
            handles.push(thread::spawn(move || {
                mgr_clone.bind_account(&user_id_clone, "shared_account".to_string())
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }

        // 验证账户只被绑定一次
        let accounts = mgr.get_user_accounts(&user_id).unwrap();
        assert_eq!(accounts.len(), 1, "并发绑定相同账户应只有一条记录");
        assert_eq!(accounts[0], "shared_account");
    }

    // ====================================================================================
    // 索引一致性测试 @yutiansut @quantaxis
    //
    // UserManager 维护三个索引：
    // - username_index: 用户名 → user_id
    // - phone_index: 手机号 → user_id
    // - email_index: 邮箱 → user_id
    //
    // 这些索引用于：
    // - 快速查找：通过用户名/手机/邮箱查找用户
    // - 唯一性检查：注册时检查是否重复
    // ====================================================================================

    /// 测试索引在用户注册时正确更新
    #[test]
    fn test_index_consistency_on_registration() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "index_test".to_string(),
            password: "password".to_string(),
            phone: Some("13900139000".to_string()),
            email: Some("index@test.com".to_string()),
            real_name: None,
            id_card: None,
        };

        let user = mgr.register(req).unwrap();

        // 验证用户名索引
        assert!(mgr.username_index.contains_key("index_test"));
        assert_eq!(
            mgr.username_index.get("index_test").unwrap().clone(),
            user.user_id
        );

        // 验证手机索引
        assert!(mgr.phone_index.contains_key("13900139000"));
        assert_eq!(
            mgr.phone_index.get("13900139000").unwrap().clone(),
            user.user_id
        );

        // 验证邮箱索引
        assert!(mgr.email_index.contains_key("index@test.com"));
        assert_eq!(
            mgr.email_index.get("index@test.com").unwrap().clone(),
            user.user_id
        );
    }

    /// 测试无手机/邮箱用户的索引
    #[test]
    fn test_index_with_optional_fields() {
        let mgr = UserManager::new();

        let req = UserRegisterRequest {
            username: "minimal_user".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        };

        let user = mgr.register(req).unwrap();

        // 用户名索引应存在
        assert!(mgr.username_index.contains_key("minimal_user"));

        // 手机和邮箱索引应为空（因为没有提供）
        assert_eq!(mgr.phone_index.len(), 0);
        assert_eq!(mgr.email_index.len(), 0);

        // 但用户数据完整
        assert!(mgr.users.contains_key(&user.user_id));
    }

    // ====================================================================================
    // 登录响应字段验证 @yutiansut @quantaxis
    //
    // UserLoginResponse 结构说明：
    // - success: 登录是否成功
    // - user_id: 用户ID（成功时有值）
    // - username: 用户名（成功时有值）
    // - token: JWT token（成功时有值）
    // - message: 结果消息
    // - roles: 用户角色列表（RBAC）
    // - is_admin: 是否是管理员（向后兼容）
    // - permissions: 用户权限列表（字符串格式）
    // ====================================================================================

    /// 测试登录成功时的响应字段
    #[test]
    fn test_login_response_fields_on_success() {
        let mgr = UserManager::new();

        // 注册Admin用户
        let _ = mgr.register(UserRegisterRequest {
            username: "resp_admin".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        // 注册普通用户
        let user = mgr.register(UserRegisterRequest {
            username: "resp_trader".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        let login_req = UserLoginRequest {
            username: "resp_trader".to_string(),
            password: "password".to_string(),
        };

        let resp = mgr.login(login_req).unwrap();

        // 验证基本字段
        assert!(resp.success, "登录应成功");
        assert_eq!(resp.user_id, Some(user.user_id.clone()));
        assert_eq!(resp.username, Some("resp_trader".to_string()));
        assert!(resp.token.is_some(), "应返回JWT token");
        assert_eq!(resp.message, "Login successful");

        // 验证RBAC字段
        assert!(resp.roles.is_some(), "应返回角色列表");
        let roles = resp.roles.unwrap();
        assert!(roles.contains(&UserRole::Trader), "应包含Trader角色");

        assert_eq!(resp.is_admin, Some(false), "普通用户is_admin应为false");

        // 验证权限列表
        assert!(resp.permissions.is_some(), "应返回权限列表");
        let permissions = resp.permissions.unwrap();
        assert!(!permissions.is_empty(), "Trader应有权限");
    }

    /// 测试Admin登录时的权限字段
    #[test]
    fn test_admin_login_has_all_permissions() {
        let mgr = UserManager::new();

        // 注册Admin（第一个用户）
        mgr.register(UserRegisterRequest {
            username: "admin_perm_test".to_string(),
            password: "password".to_string(),
            phone: None,
            email: None,
            real_name: None,
            id_card: None,
        }).unwrap();

        let login_req = UserLoginRequest {
            username: "admin_perm_test".to_string(),
            password: "password".to_string(),
        };

        let resp = mgr.login(login_req).unwrap();

        assert!(resp.success);
        assert_eq!(resp.is_admin, Some(true), "Admin用户is_admin应为true");

        let permissions = resp.permissions.unwrap();
        // Admin应有所有权限
        let all_permission_count = Permission::all().len();
        assert_eq!(
            permissions.len(),
            all_permission_count,
            "Admin应有全部{}个权限",
            all_permission_count
        );
    }
}
