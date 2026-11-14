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
///
/// `admin_auth` 会 wrap 到 /api/admin、/api/management、/api/account-admin 三个 scope 上。
/// 未启用时中间件直接放行,行为与加它之前完全一致。@yutiansut @quantaxis
pub fn configure(cfg: &mut web::ServiceConfig, admin_auth: auth::AdminAuth) {
    cfg
        // 健康检查
        .route("/health", web::get().to(handlers::health_check))
        // 用户认证 @yutiansut @quantaxis
        .service(
            web::scope("/api/auth")
                .route("/register", web::post().to(auth::register))
                .route("/login", web::post().to(auth::login))
                .route("/user/{user_id}", web::get().to(auth::get_current_user))
                // ⚠️ 列出**全部用户**(user_id / 用户名 / 手机 / 邮箱 / 角色 / 账户列表),
                // 注释本来就写着「管理员」,却没有任何鉴权。实测(2026-09-06)开着
                // `QAEX_REQUIRE_ADMIN_AUTH=1` 也照样 200 —— 因为 `/api/auth` 整个 scope
                // 不能包(注册/登录必须开放),而这条没有单独包。
                // 用与下面提权三条相同的手法:`web::resource` 逐条包。@yutiansut @quantaxis
                .service(
                    web::resource("/users")
                        .wrap(admin_auth.clone())
                        .route(web::get().to(auth::list_users)),
                )
                // 角色管理 API —— **逐 resource 包鉴权** @yutiansut @quantaxis
                //
                // ⚠️ `/api/auth` 这个 scope 本身不能整体包 admin_auth(注册/登录必须开放),
                //    但下面三条是**权限变更**接口。`auth::make_admin`(auth.rs:256)
                //    内部**没有任何调用者身份校验** —— 从路径取 user_id 直接
                //    `add_user_role(user_id, Admin)`。实测不带任何 token POST
                //    `/api/auth/user/<不存在的id>/make-admin` 返回 400「User not found」,
                //    说明 handler 已执行、只是用户不存在;换成真实 user_id 即可提权。
                //    而且 scope 无 wrap ⇒ **即使 `QAEX_REQUIRE_ADMIN_AUTH=1` 也拦不住**。
                //    前端对这三条零调用(仅 `/auth/users` 在用,故保持原样)。
                //    actix 支持在单个 resource 上 wrap,不必重构路由。
                .service(
                    web::resource("/user/roles")
                        .wrap(admin_auth.clone())
                        .route(web::post().to(auth::set_user_roles)),
                )
                .service(
                    web::resource("/user/role/add")
                        .wrap(admin_auth.clone())
                        .route(web::post().to(auth::add_user_role)),
                )
                .service(
                    web::resource("/user/{user_id}/make-admin")
                        .wrap(admin_auth.clone())
                        .route(web::post().to(auth::make_admin)),
                ),
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
                // ✨ 合约规格(只读)—— 与 /api/admin/instruments 同一个 handler、同一个
                // InstrumentRegistry 权威源,但不需要管理员身份。@yutiansut @quantaxis
                //
                // 起因:下单面板(OrderForm.vue)和成交记录页(trades/index.vue)需要
                // contract_multiplier / margin_rate 来算保证金和成交额,而
                // /api/market/instruments 只有 multiplier、没有 margin_rate,
                // 于是两个**交易员**页面绕道调了 /api/admin/instruments ——
                // 一旦 QAEX_REQUIRE_ADMIN_AUTH=1 就 403,且两处都有 try/catch
                // 静默回退到写死的 300,IC2501 真实乘数是 200 → 成交额高报 50%
                // (「自己重算权威量」的第 7 次复发,见 lessons.md L45)。
                //
                // 合约乘数/保证金率是**公开市场信息**,交易所本就对外发布,
                // 只读暴露不构成信息泄露;写操作(create/update/delist)仍在 admin scope 内。
                .route(
                    "/instrument-specs",
                    web::get().to(admin::get_all_instruments),
                )
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
                .route("/status", web::get().to(monitoring::get_system_status))  // 系统运行状态 @yutiansut @quantaxis
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
                .route("/report", web::get().to(monitoring::generate_report))
                // ✨ 监控端点必须与 admin/management 同级保护 @yutiansut @quantaxis
                //
                // 这 7 个端点暴露的是**全所**数据:总权益、全部账户统计、
                // 全部订单/成交、存储状态。原先本 scope **没有 wrap**,
                // 于是即使打开 `QAEX_REQUIRE_ADMIN_AUTH=1`
                // (main.rs:877,生产环境应当打开),它们仍然完全开放 ——
                // 任何人不带 Authorization 就能读到 ¥159 亿总权益与 39 万笔订单。
                //
                // 与 /api/admin、/api/management、/api/account-admin 三个 scope
                // 保持一致:开关关闭时放行(开发态),打开时要求 admin。
                .wrap(admin_auth.clone()),
        )
        // 管理员功能 - 市场统计
        .service(web::scope("/api/admin/market").route(
            "/order-stats",
            web::get().to(market::get_market_order_stats),
        ))
        // 管理端路由 - 合约管理和结算管理
        .service(
            web::scope("/api/admin")
                .wrap(admin_auth.clone())
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
                .wrap(admin_auth.clone())
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
                // 所有资金流水 (默认加载全部，然后筛选) @yutiansut @quantaxis
                .route("/transactions", web::get().to(management::get_all_transactions))
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
                .wrap(admin_auth.clone())
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
            // ⚠️ 审计日志整个 scope 补鉴权 —— 它此前**完全没有** `.wrap(admin_auth)`,
            // 实测开着 `QAEX_REQUIRE_ADMIN_AUTH=1` 仍匿名 200。
            // 审计日志按定义就是「谁在什么时候对哪个账户做了什么」,
            // 匿名可读等于把管理操作记录对外公开。@yutiansut @quantaxis
            web::scope("/api/audit")
                .wrap(admin_auth.clone())
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
                // ⚠️ 补齐鉴权 —— 本 scope 此前**完全没有** `.wrap(admin_auth)`,
                //    而同级的 `/api/management`(:227)与 `/api/admin/*` 都有。
                //    实测:不带任何 header 打
                //    `GET /api/data/export?data_type=orders&account_id=<任意账户>`
                //    → HTTP 200,返回 27,819,515 字节的完整订单历史。
                //    这不只是当前部署关了 admin auth 的问题:该 scope 结构上就没有 wrap,
                //    即使 `QAEX_REQUIRE_ADMIN_AUTH=1` 这 10 个接口(含全量导出、日结算单、
                //    持仓分析)仍然敞开。前端对 `/api/data/*` 零调用(三处 exportData 都是
                //    纯客户端 CSV 生成),补 wrap 不影响任何现有调用方。
                //    注:若将来要开放自助查询(用户查自己的账户),应在此基础上做
                //    「本人或管理员」的细分鉴权,而不是退回无鉴权。@yutiansut @quantaxis
                .wrap(admin_auth.clone())
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
