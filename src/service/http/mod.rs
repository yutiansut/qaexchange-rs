//! HTTP API 服务模块
//!
//! 提供 RESTful API 接口用于账户管理、订单操作、查询等功能

pub mod admin;
pub mod account_admin;  // Phase 12-13: 密码/手续费/保证金/冻结/审计/公告 @yutiansut @quantaxis
pub mod auth;
pub mod data_query;  // 数据查询和导出 @yutiansut @quantaxis
pub mod handlers;
pub mod kline;
pub mod management;
pub mod market;
pub mod models;
pub mod monitoring;
pub mod routes;
pub mod transfer;  // 银期转账 @yutiansut @quantaxis

use actix_web::{middleware, web, App, HttpServer as ActixHttpServer};
use std::io;
use std::sync::Arc;

use crate::exchange::{AccountManager, OrderRouter, SettlementEngine};
use crate::market::MarketDataService;
use crate::matching::engine::ExchangeMatchingEngine;
use crate::user::UserManager;
use handlers::AppState;

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
        settlement_engine: Arc<SettlementEngine>,
        matching_engine: Arc<ExchangeMatchingEngine>,
        bind_address: String,
    ) -> Self {
        let trade_recorder = matching_engine.get_trade_recorder();

        let app_state = Arc::new(AppState {
            order_router,
            account_mgr,
            settlement_engine,
            trade_recorder,
            user_mgr,
            storage_stats: None,
            conversion_mgr: None,
            // 数据查询存储组件（在HttpServer::new中初始化为None，由main.rs完整初始化）@yutiansut @quantaxis
            market_data_storage: None,
            kline_wal_manager: None,
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
                        .max_age(3600),
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
