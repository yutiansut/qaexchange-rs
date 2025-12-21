//! 用户管理模块
//!
//! 提供用户注册、登录、账户绑定等功能
//! 用户(User) 1对多 账户(QA_Account) 的关系管理
//! RBAC 权限体系 @yutiansut @quantaxis

pub mod recovery;
pub mod user_manager;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

// ==================== RBAC 权限体系 @yutiansut @quantaxis ====================

/// 用户角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum UserRole {
    /// 系统管理员 - 最高权限
    Admin,
    /// 交易员 - 可以交易和查看
    #[default]
    Trader,
    /// 分析师 - 只读权限 + 数据分析
    Analyst,
    /// 只读用户 - 最小权限
    ReadOnly,
    /// 风控员 - 风控相关操作
    RiskManager,
    /// 结算员 - 结算相关操作
    Settlement,
}


impl UserRole {
    /// 角色显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            UserRole::Admin => "系统管理员",
            UserRole::Trader => "交易员",
            UserRole::Analyst => "分析师",
            UserRole::ReadOnly => "只读用户",
            UserRole::RiskManager => "风控员",
            UserRole::Settlement => "结算员",
        }
    }

    /// 角色优先级 (用于权限比较)
    pub fn priority(&self) -> u8 {
        match self {
            UserRole::Admin => 100,
            UserRole::RiskManager => 80,
            UserRole::Settlement => 70,
            UserRole::Trader => 50,
            UserRole::Analyst => 30,
            UserRole::ReadOnly => 10,
        }
    }

    /// 获取角色拥有的所有权限
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            UserRole::Admin => Permission::all(),
            UserRole::Trader => vec![
                // 交易权限
                Permission::Trade,
                Permission::CancelOrder,
                Permission::ModifyOrder,
                Permission::BatchOrder,
                Permission::ConditionalOrder,
                Permission::Transfer,
                // 查看权限
                Permission::ViewOwnAccount,
                Permission::ViewOwnOrders,
                Permission::ViewOwnPositions,
                Permission::ViewOwnTrades,
                Permission::ViewMarketData,
                Permission::ViewKline,
            ],
            UserRole::Analyst => vec![
                // 只读 + 分析权限
                Permission::ViewOwnAccount,
                Permission::ViewOwnOrders,
                Permission::ViewOwnPositions,
                Permission::ViewOwnTrades,
                Permission::ViewMarketData,
                Permission::ViewKline,
                Permission::ViewStatistics,
                Permission::ExportData,
            ],
            UserRole::ReadOnly => vec![
                Permission::ViewOwnAccount,
                Permission::ViewOwnOrders,
                Permission::ViewOwnPositions,
                Permission::ViewOwnTrades,
                Permission::ViewMarketData,
            ],
            UserRole::RiskManager => vec![
                // 风控权限
                Permission::ViewAllAccounts,
                Permission::ViewAllOrders,
                Permission::ViewAllPositions,
                Permission::ViewRisk,
                Permission::ForceLiquidate,
                Permission::FreezeAccount,
                Permission::ViewAuditLogs,
                // 基本查看
                Permission::ViewMarketData,
                Permission::ViewKline,
                Permission::ViewStatistics,
            ],
            UserRole::Settlement => vec![
                // 结算权限
                Permission::ExecuteSettlement,
                Permission::SetSettlementPrice,
                Permission::ViewSettlementHistory,
                // 基本查看
                Permission::ViewAllAccounts,
                Permission::ViewAllPositions,
                Permission::ViewMarketData,
            ],
        }
    }
}

/// 系统权限
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // ==================== 交易权限 ====================
    /// 下单交易
    Trade,
    /// 撤销订单
    CancelOrder,
    /// 修改订单
    ModifyOrder,
    /// 批量下单
    BatchOrder,
    /// 条件单
    ConditionalOrder,
    /// 银期转账
    Transfer,

    // ==================== 账户权限 ====================
    /// 查看自己的账户
    ViewOwnAccount,
    /// 查看所有账户 (管理员)
    ViewAllAccounts,
    /// 开户
    OpenAccount,
    /// 销户
    CloseAccount,
    /// 冻结账户
    FreezeAccount,
    /// 出入金
    DepositWithdraw,

    // ==================== 订单/持仓/成交权限 ====================
    /// 查看自己的订单
    ViewOwnOrders,
    /// 查看所有订单 (管理员)
    ViewAllOrders,
    /// 查看自己的持仓
    ViewOwnPositions,
    /// 查看所有持仓 (管理员)
    ViewAllPositions,
    /// 查看自己的成交
    ViewOwnTrades,
    /// 查看所有成交 (管理员)
    ViewAllTrades,

    // ==================== 市场数据权限 ====================
    /// 查看行情数据
    ViewMarketData,
    /// 查看K线数据
    ViewKline,
    /// 查看深度数据
    ViewOrderbook,

    // ==================== 风控权限 ====================
    /// 查看风控信息
    ViewRisk,
    /// 强制平仓
    ForceLiquidate,

    // ==================== 结算权限 ====================
    /// 执行结算
    ExecuteSettlement,
    /// 设置结算价
    SetSettlementPrice,
    /// 查看结算历史
    ViewSettlementHistory,

    // ==================== 合约管理权限 ====================
    /// 查看合约列表
    ViewInstruments,
    /// 创建合约
    CreateInstrument,
    /// 修改合约
    ModifyInstrument,
    /// 暂停/恢复合约交易
    SuspendResumeInstrument,

    // ==================== 用户管理权限 ====================
    /// 查看用户列表
    ViewUsers,
    /// 创建用户
    CreateUser,
    /// 修改用户角色
    ModifyUserRole,
    /// 冻结/解冻用户
    FreezeUser,

    // ==================== 系统管理权限 ====================
    /// 查看系统监控
    ViewMonitoring,
    /// 查看审计日志
    ViewAuditLogs,
    /// 发布公告
    ManageAnnouncements,
    /// 查看统计数据
    ViewStatistics,
    /// 导出数据
    ExportData,
}

impl Permission {
    /// 获取所有权限
    pub fn all() -> Vec<Permission> {
        vec![
            // 交易
            Permission::Trade,
            Permission::CancelOrder,
            Permission::ModifyOrder,
            Permission::BatchOrder,
            Permission::ConditionalOrder,
            Permission::Transfer,
            // 账户
            Permission::ViewOwnAccount,
            Permission::ViewAllAccounts,
            Permission::OpenAccount,
            Permission::CloseAccount,
            Permission::FreezeAccount,
            Permission::DepositWithdraw,
            // 订单/持仓/成交
            Permission::ViewOwnOrders,
            Permission::ViewAllOrders,
            Permission::ViewOwnPositions,
            Permission::ViewAllPositions,
            Permission::ViewOwnTrades,
            Permission::ViewAllTrades,
            // 市场数据
            Permission::ViewMarketData,
            Permission::ViewKline,
            Permission::ViewOrderbook,
            // 风控
            Permission::ViewRisk,
            Permission::ForceLiquidate,
            // 结算
            Permission::ExecuteSettlement,
            Permission::SetSettlementPrice,
            Permission::ViewSettlementHistory,
            // 合约管理
            Permission::ViewInstruments,
            Permission::CreateInstrument,
            Permission::ModifyInstrument,
            Permission::SuspendResumeInstrument,
            // 用户管理
            Permission::ViewUsers,
            Permission::CreateUser,
            Permission::ModifyUserRole,
            Permission::FreezeUser,
            // 系统管理
            Permission::ViewMonitoring,
            Permission::ViewAuditLogs,
            Permission::ManageAnnouncements,
            Permission::ViewStatistics,
            Permission::ExportData,
        ]
    }

    /// 权限显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Permission::Trade => "下单交易",
            Permission::CancelOrder => "撤销订单",
            Permission::ModifyOrder => "修改订单",
            Permission::BatchOrder => "批量下单",
            Permission::ConditionalOrder => "条件单",
            Permission::Transfer => "银期转账",
            Permission::ViewOwnAccount => "查看自己账户",
            Permission::ViewAllAccounts => "查看所有账户",
            Permission::OpenAccount => "开户",
            Permission::CloseAccount => "销户",
            Permission::FreezeAccount => "冻结账户",
            Permission::DepositWithdraw => "出入金",
            Permission::ViewOwnOrders => "查看自己订单",
            Permission::ViewAllOrders => "查看所有订单",
            Permission::ViewOwnPositions => "查看自己持仓",
            Permission::ViewAllPositions => "查看所有持仓",
            Permission::ViewOwnTrades => "查看自己成交",
            Permission::ViewAllTrades => "查看所有成交",
            Permission::ViewMarketData => "查看行情",
            Permission::ViewKline => "查看K线",
            Permission::ViewOrderbook => "查看深度",
            Permission::ViewRisk => "查看风控",
            Permission::ForceLiquidate => "强制平仓",
            Permission::ExecuteSettlement => "执行结算",
            Permission::SetSettlementPrice => "设置结算价",
            Permission::ViewSettlementHistory => "查看结算历史",
            Permission::ViewInstruments => "查看合约",
            Permission::CreateInstrument => "创建合约",
            Permission::ModifyInstrument => "修改合约",
            Permission::SuspendResumeInstrument => "暂停/恢复合约",
            Permission::ViewUsers => "查看用户",
            Permission::CreateUser => "创建用户",
            Permission::ModifyUserRole => "修改用户角色",
            Permission::FreezeUser => "冻结用户",
            Permission::ViewMonitoring => "查看监控",
            Permission::ViewAuditLogs => "查看审计日志",
            Permission::ManageAnnouncements => "管理公告",
            Permission::ViewStatistics => "查看统计",
            Permission::ExportData => "导出数据",
        }
    }
}

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

    /// 用户角色列表 (RBAC) @yutiansut @quantaxis
    pub roles: Vec<UserRole>,

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
    /// 创建新用户 (默认 Trader 角色)
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
            roles: vec![UserRole::Trader], // 默认 Trader 角色
            created_at: now,
            updated_at: now,
            status: UserStatus::Active,
        }
    }

    /// 创建管理员用户 @yutiansut @quantaxis
    pub fn new_admin(username: String, password_hash: String) -> Self {
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
            roles: vec![UserRole::Admin],
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

    // ==================== RBAC 方法 @yutiansut @quantaxis ====================

    /// 检查用户是否拥有指定角色
    pub fn has_role(&self, role: UserRole) -> bool {
        self.roles.contains(&role)
    }

    /// 检查用户是否是管理员
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&UserRole::Admin)
    }

    /// 获取用户的所有权限 (根据角色汇总)
    pub fn get_permissions(&self) -> HashSet<Permission> {
        let mut permissions = HashSet::new();
        for role in &self.roles {
            for perm in role.permissions() {
                permissions.insert(perm);
            }
        }
        permissions
    }

    /// 检查用户是否拥有指定权限
    pub fn has_permission(&self, permission: Permission) -> bool {
        // Admin 拥有所有权限
        if self.is_admin() {
            return true;
        }
        self.get_permissions().contains(&permission)
    }

    /// 检查用户是否拥有所有指定权限
    pub fn has_all_permissions(&self, permissions: &[Permission]) -> bool {
        if self.is_admin() {
            return true;
        }
        let user_perms = self.get_permissions();
        permissions.iter().all(|p| user_perms.contains(p))
    }

    /// 检查用户是否拥有任一指定权限
    pub fn has_any_permission(&self, permissions: &[Permission]) -> bool {
        if self.is_admin() {
            return true;
        }
        let user_perms = self.get_permissions();
        permissions.iter().any(|p| user_perms.contains(p))
    }

    /// 添加角色
    pub fn add_role(&mut self, role: UserRole) {
        if !self.roles.contains(&role) {
            self.roles.push(role);
            self.updated_at = Utc::now().timestamp();
        }
    }

    /// 移除角色
    pub fn remove_role(&mut self, role: UserRole) {
        self.roles.retain(|r| r != &role);
        self.updated_at = Utc::now().timestamp();
    }

    /// 设置角色列表 (替换所有角色)
    pub fn set_roles(&mut self, roles: Vec<UserRole>) {
        self.roles = roles;
        self.updated_at = Utc::now().timestamp();
    }

    /// 获取用户最高优先级的角色
    pub fn primary_role(&self) -> UserRole {
        self.roles
            .iter()
            .max_by_key(|r| r.priority())
            .copied()
            .unwrap_or(UserRole::ReadOnly)
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

/// 用户登录响应 @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLoginResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub token: Option<String>,
    pub message: String,
    /// 用户角色列表 (RBAC)
    pub roles: Option<Vec<UserRole>>,
    /// 是否是管理员 (向后兼容)
    pub is_admin: Option<bool>,
    /// 用户权限列表
    pub permissions: Option<Vec<String>>,
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
