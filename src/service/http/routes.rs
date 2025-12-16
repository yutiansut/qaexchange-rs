//! HTTP API 路由配置

use super::admin;
use super::account_admin;  // Phase 12-13: 密码/手续费/保证金/冻结/审计/公告 @yutiansut @quantaxis
use super::auth;
use super::data_query;  // 数据查询和导出 @yutiansut @quantaxis
use super::handlers;
use super::kline;
use super::management;
use super::market;
use super::monitoring;
use super::transfer;  // 银期转账 @yutiansut @quantaxis
use actix_web::web;

/// 配置所有路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // 健康检查
        .route("/health", web::get().to(handlers::health_check))
        // 用户认证 @yutiansut @quantaxis
        .service(
            web::scope("/api/auth")
                .route("/register", web::post().to(auth::register))
                .route("/login", web::post().to(auth::login))
                .route("/user/{user_id}", web::get().to(auth::get_current_user))
                .route("/users", web::get().to(auth::list_users)) // 获取所有用户列表（管理员）
                // 角色管理 API @yutiansut @quantaxis
                .route("/user/roles", web::post().to(auth::set_user_roles)) // 设置用户角色
                .route("/user/role/add", web::post().to(auth::add_user_role)) // 添加用户角色
                .route("/user/{user_id}/make-admin", web::post().to(auth::make_admin)), // 升级为管理员
        )
        // 用户账户管理 (Phase 10)
        .service(
            web::scope("/api/user")
                .route(
                    "/{user_id}/account/create",
                    web::post().to(handlers::create_user_account),
                )
                .route(
                    "/{user_id}/accounts",
                    web::get().to(handlers::get_user_accounts),
                ),
        )
        // 账户管理
        .service(
            web::scope("/api/account")
                .route("/open", web::post().to(handlers::open_account))
                .route("/{account_id}", web::get().to(handlers::query_account)) // 修复: 明确是 account_id
                .route("/deposit", web::post().to(handlers::deposit))
                .route("/withdraw", web::post().to(handlers::withdraw))
                .route(
                    "/{user_id}/equity-curve",
                    web::get().to(handlers::get_equity_curve),
                )
                // Phase 11: 银期转账 @yutiansut @quantaxis
                .route("/transfer", web::post().to(transfer::do_transfer))  // 执行转账
                .route("/{account_id}/banks", web::get().to(transfer::get_banks))  // 签约银行
                .route("/{account_id}/transfers", web::get().to(transfer::get_transfers)),  // 转账记录
        )
        // 订单管理
        .service(
            web::scope("/api/order")
                .route("/submit", web::post().to(handlers::submit_order))
                .route("/cancel", web::post().to(handlers::cancel_order))
                .route("/{order_id}", web::get().to(handlers::query_order))
                .route(
                    "/user/{user_id}",
                    web::get().to(handlers::query_user_orders),
                )
                // Phase 11: 批量下单/撤单 @yutiansut @quantaxis
                .route("/batch", web::post().to(handlers::batch_submit_orders))
                .route("/batch-cancel", web::post().to(handlers::batch_cancel_orders))
                // Phase 11: 订单修改 @yutiansut @quantaxis
                .route("/modify/{order_id}", web::put().to(handlers::modify_order))
                // Phase 11: 条件单 @yutiansut @quantaxis
                .route("/conditional", web::post().to(handlers::create_conditional_order))
                .route("/conditional/list", web::get().to(handlers::get_conditional_orders))
                .route("/conditional/statistics", web::get().to(handlers::get_conditional_order_statistics))
                .route("/conditional/{conditional_order_id}", web::delete().to(handlers::cancel_conditional_order)),
        )
        // 持仓查询
        .service(
            web::scope("/api/position")
                .route(
                    "/account/{account_id}",
                    web::get().to(handlers::query_position),
                ) // 按account_id查询
                .route(
                    "/user/{user_id}",
                    web::get().to(handlers::query_positions_by_user),
                ), // 按user_id查询所有
        )
        // 成交记录查询
        .service(
            web::scope("/api/trades")
                .route(
                    "/account/{account_id}",
                    web::get().to(handlers::query_account_trades),
                ) // 按account_id查询
                .route(
                    "/user/{user_id}",
                    web::get().to(handlers::query_user_trades),
                ), // 按user_id查询所有
        )
        // 市场数据（行情）
        .service(
            web::scope("/api/market")
                .route("/instruments", web::get().to(market::get_instruments))
                .route(
                    "/orderbook/{instrument_id}",
                    web::get().to(market::get_orderbook),
                )
                .route("/tick/{instrument_id}", web::get().to(market::get_tick))
                .route(
                    "/trades/{instrument_id}",
                    web::get().to(market::get_recent_trades),
                )
                .route(
                    "/kline/{instrument_id}",
                    web::get().to(kline::get_kline_data),
                ), // K线数据
        )
        // 监控和统计
        .service(
            web::scope("/api/monitoring")
                .route("/system", web::get().to(monitoring::get_system_monitoring))
                .route(
                    "/accounts",
                    web::get().to(monitoring::get_accounts_monitoring),
                )
                .route("/orders", web::get().to(monitoring::get_orders_monitoring))
                .route("/trades", web::get().to(monitoring::get_trades_monitoring))
                .route(
                    "/storage",
                    web::get().to(monitoring::get_storage_monitoring),
                )
                .route("/report", web::get().to(monitoring::generate_report)),
        )
        // 管理员功能 - 市场统计
        .service(web::scope("/api/admin/market").route(
            "/order-stats",
            web::get().to(market::get_market_order_stats),
        ))
        // 管理端路由 - 合约管理和结算管理
        .service(
            web::scope("/api/admin")
                // 合约管理
                .route("/instruments", web::get().to(admin::get_all_instruments))
                .route(
                    "/instrument/create",
                    web::post().to(admin::create_instrument),
                )
                .route(
                    "/instrument/{id}/update",
                    web::put().to(admin::update_instrument),
                )
                .route(
                    "/instrument/{id}/suspend",
                    web::put().to(admin::suspend_instrument),
                )
                .route(
                    "/instrument/{id}/resume",
                    web::put().to(admin::resume_instrument),
                )
                .route(
                    "/instrument/{id}/delist",
                    web::delete().to(admin::delist_instrument),
                )
                // 结算管理
                .route(
                    "/settlement/set-price",
                    web::post().to(admin::set_settlement_price),
                )
                .route(
                    "/settlement/batch-set-prices",
                    web::post().to(admin::batch_set_settlement_prices),
                )
                .route(
                    "/settlement/execute",
                    web::post().to(admin::execute_settlement),
                )
                .route(
                    "/settlement/history",
                    web::get().to(admin::get_settlement_history),
                )
                .route(
                    "/settlement/detail/{date}",
                    web::get().to(admin::get_settlement_detail),
                ),
        )
        // 管理端路由 - 账户管理、资金管理、风控监控
        .service(
            web::scope("/api/management")
                // 账户管理
                .route("/accounts", web::get().to(management::list_all_accounts))
                .route(
                    "/account/{user_id}/detail",
                    web::get().to(management::get_account_detail),
                )
                // 全市场订单/成交查询 (管理端) @yutiansut @quantaxis
                .route("/orders", web::get().to(management::list_all_orders))
                .route("/trades", web::get().to(management::list_all_trades))
                // 资金管理
                .route("/deposit", web::post().to(management::deposit))
                .route("/withdraw", web::post().to(management::withdraw))
                .route(
                    "/transactions/{user_id}",
                    web::get().to(management::get_transactions),
                )
                // 风控监控
                .route(
                    "/risk/accounts",
                    web::get().to(management::get_risk_accounts),
                )
                .route(
                    "/risk/margin-summary",
                    web::get().to(management::get_margin_summary),
                )
                .route(
                    "/risk/liquidations",
                    web::get().to(management::get_liquidation_records),
                )
                .route(
                    "/risk/force-liquidate",
                    web::post().to(management::force_liquidate_account),
                ),
        )
        // ==================== Phase 12-13: 账户管理扩展功能 ====================
        // @yutiansut @quantaxis
        .service(
            web::scope("/api/account-admin")
                // Phase 12: 密码管理
                .route("/password/change", web::post().to(account_admin::change_password))
                .route("/password/reset", web::post().to(account_admin::reset_password))
                // Phase 12: 手续费查询
                .route("/commission/rates", web::get().to(account_admin::get_commission_rates))
                .route("/commission/statistics/{account_id}", web::get().to(account_admin::get_commission_statistics))
                // Phase 12: 保证金率管理
                .route("/margin/rates", web::get().to(account_admin::get_margin_rates))
                .route("/margin/summary/{account_id}", web::get().to(account_admin::get_margin_summary))
                // Phase 13: 账户冻结
                .route("/status/{account_id}", web::get().to(account_admin::get_account_status))
                .route("/freeze", web::post().to(account_admin::freeze_account))
                .route("/unfreeze", web::post().to(account_admin::unfreeze_account))
        )
        // Phase 13: 审计日志
        .service(
            web::scope("/api/audit")
                .route("/logs", web::get().to(account_admin::query_audit_logs))
                .route("/logs/{log_id}", web::get().to(account_admin::get_audit_log))
        )
        // Phase 13: 系统公告
        .service(
            web::scope("/api/announcements")
                .route("", web::get().to(account_admin::query_announcements))
                .route("", web::post().to(account_admin::create_announcement))
                .route("/{announcement_id}", web::get().to(account_admin::get_announcement))
                .route("/{announcement_id}", web::delete().to(account_admin::delete_announcement))
        )
        // ==================== 数据查询和分析 API ====================
        // @yutiansut @quantaxis
        .service(
            web::scope("/api/data")
                // 历史数据查询
                .route("/history/ticks", web::get().to(data_query::query_history_ticks))
                .route("/history/klines", web::get().to(data_query::query_batch_klines))
                // 交易统计分析
                .route("/statistics/trades", web::get().to(data_query::get_trade_statistics))
                .route("/statistics/pnl", web::get().to(data_query::get_pnl_analysis))
                .route("/statistics/risk", web::get().to(data_query::get_risk_statistics))
                .route("/statistics/rankings", web::get().to(data_query::get_trade_rankings))
                // 持仓分析
                .route("/analysis/position/{account_id}", web::get().to(data_query::get_position_analysis))
                // 日结算单
                .route("/settlement/statement", web::get().to(data_query::get_settlement_statement))
                // 数据导出
                .route("/export", web::get().to(data_query::export_data))
                // 市场概览
                .route("/overview/market", web::get().to(data_query::get_market_overview))
        );
}
