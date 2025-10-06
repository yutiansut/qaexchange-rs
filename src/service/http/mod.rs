//! HTTP API 服务模块
//!
//! 提供 RESTful API 接口用于账户管理、订单操作、查询等功能

pub mod models;
pub mod handlers;
pub mod routes;
pub mod monitoring;
pub mod market;
pub mod admin;
pub mod management;
pub mod auth;

use actix_web::{App, HttpServer as ActixHttpServer, middleware, web};
use std::sync::Arc;
use std::io;

use handlers::AppState;
use crate::exchange::{OrderRouter, AccountManager};
use crate::matching::engine::ExchangeMatchingEngine;
use crate::market::MarketDataService;
use crate::user::UserManager;

/// HTTP 服务器
pub struct HttpServer {
    /// 应用状态
    app_state: Arc<AppState>,

    /// 市场数据服务
    market_service: Arc<MarketDataService>,

    /// 监听地址
    bind_address: String,
}

impl HttpServer {
    /// 创建新的 HTTP 服务器
    pub fn new(
        order_router: Arc<OrderRouter>,
        account_mgr: Arc<AccountManager>,
        user_mgr: Arc<UserManager>,
        matching_engine: Arc<ExchangeMatchingEngine>,
        bind_address: String,
    ) -> Self {
        let trade_recorder = matching_engine.get_trade_recorder();

        let app_state = Arc::new(AppState {
            order_router,
            account_mgr,
            trade_recorder,
            user_mgr,
            storage_stats: None,
            conversion_mgr: None,
        });

        let market_service = Arc::new(MarketDataService::new(matching_engine));

        Self {
            app_state,
            market_service,
            bind_address,
        }
    }

    /// 启动 HTTP 服务器
    pub async fn run(self) -> io::Result<()> {
        log::info!("Starting HTTP server at {}", self.bind_address);

        let app_state = self.app_state.clone();
        let market_service = self.market_service.clone();
        let bind_address = self.bind_address.clone();

        ActixHttpServer::new(move || {
            App::new()
                // 应用状态
                .app_data(web::Data::new(app_state.clone()))
                .app_data(web::Data::new(market_service.clone()))

                // 中间件
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())

                // CORS 支持
                .wrap(
                    actix_cors::Cors::default()
                        .allow_any_origin()
                        .allow_any_method()
                        .allow_any_header()
                        .max_age(3600)
                )

                // 配置路由
                .configure(routes::configure)
        })
        .bind(&bind_address)?
        .run()
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_server_creation() {
        // 基本的创建测试
        // 完整的 HTTP 测试需要 actix 运行时
    }
}
