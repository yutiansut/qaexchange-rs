//! 用户认证 HTTP API @yutiansut @quantaxis
//!
//! 提供用户注册、登录、角色管理等认证功能

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::sync::Arc;

use super::handlers::AppState;
use super::models::ApiResponse;
use crate::core::account_ext::{AccountType, OpenAccountRequest as CoreOpenAccountRequest};
use crate::user::{UserLoginRequest, UserRegisterRequest, UserRole};

/// 用户注册 @yutiansut @quantaxis
/// 注册成功后自动创建一个默认交易账户
pub async fn register(
    req: web::Json<UserRegisterRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let username = req.username.clone();

    match state.user_mgr.register(req.into_inner()) {
        Ok(user) => {
            // ✨ 自动为新用户创建默认交易账户 @yutiansut @quantaxis
            let account_req = CoreOpenAccountRequest {
                user_id: user.user_id.clone(),
                account_id: None,  // 自动生成
                account_name: format!("{} 的默认账户", username),
                init_cash: 0.0,  // 初始资金为0，需要入金
                account_type: AccountType::Individual,
            };

            let account_id = match state.account_mgr.open_account(account_req) {
                Ok(id) => {
                    log::info!(
                        "Auto-created default account {} for user {}",
                        id,
                        user.user_id
                    );
                    Some(id)
                }
                Err(e) => {
                    log::warn!(
                        "Failed to auto-create account for user {}: {:?}",
                        user.user_id,
                        e
                    );
                    None
                }
            };

            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "user_id": user.user_id,
                    "username": user.username,
                    "account_id": account_id,
                    "message": "注册成功"
                }))),
            )
        },
        Err(e) => {
            log::warn!("User registration failed: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}

/// 用户登录
pub async fn login(
    req: web::Json<UserLoginRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.user_mgr.login(req.into_inner()) {
        Ok(login_resp) => {
            if login_resp.success {
                log::info!(
                    "User {} logged in successfully",
                    login_resp
                        .username
                        .as_ref()
                        .unwrap_or(&"unknown".to_string())
                );
                Ok(HttpResponse::Ok().json(ApiResponse::success(login_resp)))
            } else {
                Ok(HttpResponse::Unauthorized().json(ApiResponse::success(login_resp)))
            }
        }
        Err(e) => {
            log::warn!("User login failed: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}

/// 获取当前用户信息
pub async fn get_current_user(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.user_mgr.get_user(&user_id) {
        Ok(user) => {
            // 不返回密码哈希
            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "user_id": user.user_id,
                    "username": user.username,
                    "phone": user.phone,
                    "email": user.email,
                    "real_name": user.real_name,
                    "account_ids": user.account_ids,
                    "created_at": user.created_at,
                    "status": user.status,
                    // ✨ 补 is_admin / roles —— 与登录响应同形状。@yutiansut @quantaxis
                    //
                    // 此前这里**两个都不返回**,而登录返回的是
                    // `{token, user_id, username, is_admin, roles, permissions}`,
                    // 前端据此构造 `userInfo = {user_id, username, is_admin, roles}`。
                    // 于是 `store/index.js` 的 `fetchUserInfo`(唯一调本接口的地方)
                    // 成了**地雷**:它 `commit('SET_USER_INFO', data)` 是**整体替换**,
                    // 一旦有人把它接上,`userInfo.is_admin` 就变 undefined ⇒
                    // getter `isAdmin`(:318)变 false ⇒ 路由守卫
                    // (`router/index.js:246,271`)把管理员挡在管理页外,
                    // `views/accounts/index.vue` 的「选择用户」也会消失。
                    // 现在它零调用,所以没炸 —— 但那是运气,不是设计。
                    "is_admin": user.is_admin(),
                    "roles": user.roles.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>(),
                }))),
            )
        }
        Err(e) => {
            log::warn!("Get user failed: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(404, e.to_string())))
        }
    }
}

/// 获取所有用户列表（管理员功能）
pub async fn list_users(state: web::Data<Arc<AppState>>) -> Result<HttpResponse> {
    let users = state.user_mgr.list_users();

    // 过滤掉敏感信息（密码哈希），但包含角色信息
    let user_list: Vec<serde_json::Value> = users
        .iter()
        .map(|user| {
            serde_json::json!({
                "user_id": user.user_id,
                "username": user.username,
                "phone": user.phone,
                "email": user.email,
                "real_name": user.real_name,
                "account_ids": user.account_ids,
                "roles": user.roles.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>(),
                "created_at": user.created_at,
                "status": user.status,
            })
        })
        .collect();

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "users": user_list,
            "total": users.len(),
        }))),
    )
}

// ==================== 角色管理 API @yutiansut @quantaxis ====================

/// 设置用户角色请求
#[derive(Debug, Deserialize)]
pub struct SetUserRoleRequest {
    /// 用户 ID (UUID)
    pub user_id: String,
    /// 角色列表
    pub roles: Vec<String>,
}

/// 设置用户角色（管理员功能）
/// POST /api/auth/user/roles
pub async fn set_user_roles(
    req: web::Json<SetUserRoleRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 将字符串角色转换为 UserRole 枚举
    let roles: Vec<UserRole> = req
        .roles
        .iter()
        .filter_map(|r| match r.as_str() {
            "Admin" => Some(UserRole::Admin),
            "Trader" => Some(UserRole::Trader),
            "Analyst" => Some(UserRole::Analyst),
            "ReadOnly" => Some(UserRole::ReadOnly),
            "RiskManager" => Some(UserRole::RiskManager),
            "Settlement" => Some(UserRole::Settlement),
            _ => None,
        })
        .collect();

    if roles.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            400,
            "无效的角色列表".to_string(),
        )));
    }

    match state.user_mgr.set_user_roles(&req.user_id, roles.clone()) {
        Ok(_) => {
            log::info!("User {} roles updated to {:?}", req.user_id, roles);
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "user_id": req.user_id,
                "roles": req.roles,
                "message": "角色更新成功"
            }))))
        }
        Err(e) => {
            log::warn!("Failed to set user roles: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}

/// 添加用户角色请求
#[derive(Debug, Deserialize)]
pub struct AddUserRoleRequest {
    /// 用户 ID (UUID)
    pub user_id: String,
    /// 角色
    pub role: String,
}

/// 添加用户角色（管理员功能）
/// POST /api/auth/user/role/add
pub async fn add_user_role(
    req: web::Json<AddUserRoleRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let role = match req.role.as_str() {
        "Admin" => UserRole::Admin,
        "Trader" => UserRole::Trader,
        "Analyst" => UserRole::Analyst,
        "ReadOnly" => UserRole::ReadOnly,
        "RiskManager" => UserRole::RiskManager,
        "Settlement" => UserRole::Settlement,
        _ => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                400,
                format!("无效的角色: {}", req.role),
            )));
        }
    };

    match state.user_mgr.add_user_role(&req.user_id, role) {
        Ok(_) => {
            log::info!("Role {:?} added to user {}", role, req.user_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "user_id": req.user_id,
                "role": req.role,
                "message": "角色添加成功"
            }))))
        }
        Err(e) => {
            log::warn!("Failed to add user role: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}

/// 升级用户为管理员（便捷接口）
/// POST /api/auth/user/{user_id}/make-admin
pub async fn make_admin(
    user_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let user_id = user_id.into_inner();

    match state.user_mgr.add_user_role(&user_id, UserRole::Admin) {
        Ok(_) => {
            log::info!("User {} upgraded to Admin", user_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "user_id": user_id,
                "message": "用户已升级为管理员"
            }))))
        }
        Err(e) => {
            log::warn!("Failed to make user admin: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}

// ============================================================================
// 管理端鉴权中间件 @yutiansut @quantaxis
//
// ⚠️ 背景:权限模型完整存在但从未接入 HTTP 层。
//    UserManager::user_has_permission / is_user_admin / user_has_role
//    全部只有 #[cfg(test)] 调用(user_manager.rs 的 #[cfg(test)] 在 :517,
//    这些函数的全部引用都在 1125 行之后)。routes.rs 也没有任何鉴权 wrap。
//    实测:普通用户(roles=['Trader'], is_admin=false),甚至**不带 token**,
//    直接 POST /api/admin/instrument/create 返回 200。
//
// 本中间件把已有的 verify_token + is_user_admin 接到 /api/admin 与 /api/management。
// ============================================================================

use actix_web::body::{BoxBody, EitherBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error as ActixError;
use futures::future::{ok, LocalBoxFuture, Ready};

/// 管理端鉴权:要求 `Authorization: Bearer <token>` 且该用户是管理员
#[derive(Clone)]
pub struct AdminAuth {
    user_mgr: Arc<crate::user::UserManager>,
    /// 关闭时完全放行(保持既有行为),便于灰度上线
    enabled: bool,
}

impl AdminAuth {
    pub fn new(user_mgr: Arc<crate::user::UserManager>, enabled: bool) -> Self {
        Self { user_mgr, enabled }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AdminAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, BoxBody>>;
    type Error = ActixError;
    type Transform = AdminAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<std::result::Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AdminAuthMiddleware {
            service: Arc::new(service),
            user_mgr: self.user_mgr.clone(),
            enabled: self.enabled,
        })
    }
}

pub struct AdminAuthMiddleware<S> {
    service: Arc<S>,
    user_mgr: Arc<crate::user::UserManager>,
    enabled: bool,
}

impl<S, B> Service<ServiceRequest> for AdminAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, BoxBody>>;
    type Error = ActixError;
    type Future = LocalBoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();
        let user_mgr = self.user_mgr.clone();
        let enabled = self.enabled;

        Box::pin(async move {
            if !enabled {
                return Ok(svc.call(req).await?.map_into_left_body());
            }

            let deny = |req: ServiceRequest, code: u16, msg: &str| {
                let body = serde_json::json!({
                    "success": false, "data": null,
                    "error": { "code": code, "message": msg }
                });
                let resp = if code == 401 {
                    HttpResponse::Unauthorized().json(body)
                } else {
                    HttpResponse::Forbidden().json(body)
                };
                Ok(req.into_response(resp).map_into_right_body())
            };

            let token = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(|s| s.to_string());

            let token = match token {
                Some(t) if !t.is_empty() => t,
                _ => return deny(req, 401, "Missing or malformed Authorization header"),
            };

            let user_id = match user_mgr.verify_token(&token) {
                Ok(uid) => uid,
                Err(e) => {
                    let m = format!("Invalid token: {}", e);
                    return deny(req, 401, &m);
                }
            };

            match user_mgr.is_user_admin(&user_id) {
                Ok(true) => Ok(svc.call(req).await?.map_into_left_body()),
                Ok(false) => {
                    log::warn!(
                        "Admin endpoint denied for non-admin user {}: {} {}",
                        user_id,
                        req.method(),
                        req.path()
                    );
                    deny(req, 403, "Admin role required")
                }
                Err(e) => {
                    let m = format!("Authorization check failed: {}", e);
                    deny(req, 403, &m)
                }
            }
        })
    }
}
