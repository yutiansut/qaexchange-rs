//! 用户认证 HTTP API
//!
//! 提供用户注册、登录等认证功能

use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;

use crate::user::{UserRegisterRequest, UserLoginRequest};
use super::models::ApiResponse;
use super::handlers::AppState;

/// 用户注册
pub async fn register(
    req: web::Json<UserRegisterRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    match state.user_mgr.register(req.into_inner()) {
        Ok(user) => {
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "user_id": user.user_id,
                "username": user.username,
                "message": "注册成功"
            }))))
        }
        Err(e) => {
            log::error!("User registration failed: {:?}", e);
            Ok(HttpResponse::BadRequest().json(
                ApiResponse::<()>::error(400, e.to_string())
            ))
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
                log::info!("User {} logged in successfully", login_resp.username.as_ref().unwrap_or(&"unknown".to_string()));
                Ok(HttpResponse::Ok().json(ApiResponse::success(login_resp)))
            } else {
                Ok(HttpResponse::Unauthorized().json(ApiResponse::success(login_resp)))
            }
        }
        Err(e) => {
            log::error!("User login failed: {:?}", e);
            Ok(HttpResponse::BadRequest().json(
                ApiResponse::<()>::error(400, e.to_string())
            ))
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
            Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                "user_id": user.user_id,
                "username": user.username,
                "phone": user.phone,
                "email": user.email,
                "real_name": user.real_name,
                "account_ids": user.account_ids,
                "created_at": user.created_at,
                "status": user.status,
            }))))
        }
        Err(e) => {
            log::error!("Get user failed: {:?}", e);
            Ok(HttpResponse::NotFound().json(
                ApiResponse::<()>::error(404, e.to_string())
            ))
        }
    }
}

/// 获取所有用户列表（管理员功能）
pub async fn list_users(
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let users = state.user_mgr.list_users();

    // 过滤掉敏感信息（密码哈希）
    let user_list: Vec<serde_json::Value> = users.iter().map(|user| {
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
    }).collect();

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "users": user_list,
        "total": users.len(),
    }))))
}
