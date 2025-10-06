//! HTTP API 路由配置

use actix_web::web;
use super::handlers;
use super::monitoring;
use super::market;
use super::admin;
use super::management;
use super::auth;

/// 配置所有路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // 健康检查
        .route("/health", web::get().to(handlers::health_check))

        // 用户认证
        .service(
            web::scope("/api/auth")
                .route("/register", web::post().to(auth::register))
                .route("/login", web::post().to(auth::login))
                .route("/user/{user_id}", web::get().to(auth::get_current_user))
                .route("/users", web::get().to(auth::list_users))  // 获取所有用户列表（管理员）
        )

        // 用户账户管理 (Phase 10)
        .service(
            web::scope("/api/user")
                .route("/{user_id}/account/create", web::post().to(handlers::create_user_account))
                .route("/{user_id}/accounts", web::get().to(handlers::get_user_accounts))
        )

        // 账户管理
        .service(
            web::scope("/api/account")
                .route("/open", web::post().to(handlers::open_account))
                .route("/{account_id}", web::get().to(handlers::query_account))  // 修复: 明确是 account_id
                .route("/deposit", web::post().to(handlers::deposit))
                .route("/withdraw", web::post().to(handlers::withdraw))
        )

        // 订单管理
        .service(
            web::scope("/api/order")
                .route("/submit", web::post().to(handlers::submit_order))
                .route("/cancel", web::post().to(handlers::cancel_order))
                .route("/{order_id}", web::get().to(handlers::query_order))
                .route("/user/{user_id}", web::get().to(handlers::query_user_orders))
        )

        // 持仓查询
        .service(
            web::scope("/api/position")
                .route("/account/{account_id}", web::get().to(handlers::query_position))  // 按account_id查询
                .route("/user/{user_id}", web::get().to(handlers::query_positions_by_user))  // 按user_id查询所有
        )

        // 成交记录查询
        .service(
            web::scope("/api/trades")
                .route("/user/{user_id}", web::get().to(handlers::query_user_trades))
        )

        // 市场数据（行情）
        .service(
            web::scope("/api/market")
                .route("/instruments", web::get().to(market::get_instruments))
                .route("/orderbook/{instrument_id}", web::get().to(market::get_orderbook))
                .route("/tick/{instrument_id}", web::get().to(market::get_tick))
                .route("/trades/{instrument_id}", web::get().to(market::get_recent_trades))
        )

        // 监控和统计
        .service(
            web::scope("/api/monitoring")
                .route("/system", web::get().to(monitoring::get_system_monitoring))
                .route("/accounts", web::get().to(monitoring::get_accounts_monitoring))
                .route("/orders", web::get().to(monitoring::get_orders_monitoring))
                .route("/trades", web::get().to(monitoring::get_trades_monitoring))
                .route("/storage", web::get().to(monitoring::get_storage_monitoring))
                .route("/report", web::get().to(monitoring::generate_report))
        )

        // 管理员功能 - 市场统计
        .service(
            web::scope("/api/admin/market")
                .route("/order-stats", web::get().to(market::get_market_order_stats))
        )

        // 管理端路由 - 合约管理和结算管理
        .service(
            web::scope("/api/admin")
                // 合约管理
                .route("/instruments", web::get().to(admin::get_all_instruments))
                .route("/instrument/create", web::post().to(admin::create_instrument))
                .route("/instrument/{id}/update", web::put().to(admin::update_instrument))
                .route("/instrument/{id}/suspend", web::put().to(admin::suspend_instrument))
                .route("/instrument/{id}/resume", web::put().to(admin::resume_instrument))
                .route("/instrument/{id}/delist", web::delete().to(admin::delist_instrument))
                // 结算管理
                .route("/settlement/set-price", web::post().to(admin::set_settlement_price))
                .route("/settlement/batch-set-prices", web::post().to(admin::batch_set_settlement_prices))
                .route("/settlement/execute", web::post().to(admin::execute_settlement))
                .route("/settlement/history", web::get().to(admin::get_settlement_history))
                .route("/settlement/detail/{date}", web::get().to(admin::get_settlement_detail))
        )

        // 管理端路由 - 账户管理、资金管理、风控监控
        .service(
            web::scope("/api/management")
                // 账户管理
                .route("/accounts", web::get().to(management::list_all_accounts))
                .route("/account/{user_id}/detail", web::get().to(management::get_account_detail))
                // 资金管理
                .route("/deposit", web::post().to(management::deposit))
                .route("/withdraw", web::post().to(management::withdraw))
                .route("/transactions/{user_id}", web::get().to(management::get_transactions))
                // 风控监控
                .route("/risk/accounts", web::get().to(management::get_risk_accounts))
                .route("/risk/margin-summary", web::get().to(management::get_margin_summary))
                .route("/risk/liquidations", web::get().to(management::get_liquidation_records))
        );
}
