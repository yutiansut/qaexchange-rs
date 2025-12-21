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
            log::error!("User registration failed: {:?}", e);
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
            log::error!("User login failed: {:?}", e);
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
                }))),
            )
        }
        Err(e) => {
            log::error!("Get user failed: {:?}", e);
            Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(404, e.to_string())))
        }
    }
}

/// 获取所有用户列表（管理员功能）
pub async fn list_users(state: web::Data<Arc<AppState>>) -> Result<HttpResponse> {
    let users = state.user_mgr.list_users();

    // 过滤掉敏感信息（密码哈希）
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
            log::error!("Failed to set user roles: {:?}", e);
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
            log::error!("Failed to add user role: {:?}", e);
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
            log::error!("Failed to make user admin: {:?}", e);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string())))
        }
    }
}
