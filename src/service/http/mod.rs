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
use std::sync::atomic::AtomicUsize;
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
            // 服务器启动时间 @yutiansut @quantaxis
            server_start_time: chrono::Utc::now(),
            // WebSocket 连接计数器 @yutiansut @quantaxis
            ws_connection_count: Arc::new(AtomicUsize::new(0)),
            // SnapshotManager（由WebSocketServer设置）@yutiansut @quantaxis
            snapshot_mgr: None,
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
                // account_admin 的 change_password / get_commission_statistics /
                // get_margin_summary 直接提取 web::Data<Arc<AccountManager>>
                // @yutiansut @quantaxis
                .app_data(web::Data::new(app_state.account_mgr.clone()))
                // 中间件
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                // CORS 支持
                .wrap(
                    actix_cors::Cors::default()
                        .allow_any_origin()
                        .allow_any_method()
                        .allow_any_header()
                        // ⚠️ 自定义响应头必须显式 expose,否则浏览器**不给 JS 读**
                        //    @yutiansut @quantaxis
                        //
                        // CORS 默认只暴露 6 个 simple response headers
                        // (Cache-Control / Content-Language / Content-Type /
                        //  Expires / Last-Modified / Pragma)。
                        // `X-Total-Count`(management.rs 按用户查流水时带的总数)
                        // 不在其中 —— 不 expose 的话 axios/fetch 读到的是 undefined,
                        // 而且**没有任何报错**,又是一个静默失效。
                        .expose_headers(vec!["X-Total-Count"])
                        .max_age(3600),
                )
                // 配置路由
                .configure({
                    let require = std::env::var("QAEX_REQUIRE_ADMIN_AUTH")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    let guard = auth::AdminAuth::new(app_state.user_mgr.clone(), require);
                    move |cfg: &mut web::ServiceConfig| routes::configure(cfg, guard.clone())
                })
        })
        .bind(&bind_address)?
        .run()
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ✨ 原来这里是**空函数体**,注释写「完整的 HTTP 测试需要 actix 运行时」——
    //    但构造 `HttpServer::new` 根本不需要运行时(它只组装 AppState,
    //    起服务是 `start()` 的事),这个理由不成立。
    //    空测试「通过但零覆盖」,比失败更坏:它在测试计数里占一格,
    //    让人以为这条路径有覆盖。@yutiansut @quantaxis
    #[test]
    fn test_http_server_creation() {
        use crate::exchange::{InstrumentRegistry, TradeGateway};

        let account_mgr = Arc::new(AccountManager::new());
        let matching_engine = Arc::new(ExchangeMatchingEngine::new());
        let instrument_registry = Arc::new(InstrumentRegistry::new());
        // ⚠️ 必须给 wal_root —— 默认是 `"./data/wal"`(相对 CWD = 仓库根)
        //    @yutiansut @quantaxis
        //
        // 这个测试是本会话新加的(替换原来的空函数体),我第一版忘了给,
        // 于是每跑一次就在仓库里写 `data/wal/__ACCOUNT__/test_user/` 等真实文件。
        // 同族写法见 `trade_gateway.rs:1995` 与 `order_router.rs:2801`。
        let trade_gateway = Arc::new(
            TradeGateway::new(account_mgr.clone())
                .with_wal_root("./output/testexchange/wal_http_mod"),
        );
        let order_router = Arc::new(OrderRouter::new(
            account_mgr.clone(),
            matching_engine.clone(),
            instrument_registry,
            trade_gateway,
        ));
        let settlement_engine = Arc::new(SettlementEngine::new(account_mgr.clone()));
        let user_mgr = Arc::new(UserManager::new());

        let server = HttpServer::new(
            order_router,
            account_mgr,
            user_mgr,
            settlement_engine,
            matching_engine,
            "127.0.0.1:0".to_string(),
        );

        // 实质断言:AppState 已组装,且 main.rs 之外的可选组件确为 None
        // (`mod.rs:60-61` 注释说明它们由 main.rs 完整初始化)
        assert_eq!(server.bind_address, "127.0.0.1:0");
        assert!(server.app_state.storage_stats.is_none());
        assert!(server.app_state.conversion_mgr.is_none());
    }
}
