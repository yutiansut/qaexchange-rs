//! QAExchange 完整交易所服务
//!
//! @yutiansut @quantaxis
//!
//! 集成功能：
//! 1. 交易所核心引擎（撮合、风控、账户管理）
//! 2. HTTP API（REST 接口）
//! 3. WebSocket API（实时推送）
//! 4. 解耦存储层（异步持久化）
//!
//! 性能优化：
//! - mimalloc 高性能内存分配器
//! - 零拷贝序列化 (rkyv)
//! - 无锁并发数据结构 (DashMap)
//!
//! 运行: cargo run --bin qaexchange-server

// ═══════════════════════════════════════════════════════════════════════════
// 全局内存分配器说明
// ═══════════════════════════════════════════════════════════════════════════
//
// 注意：qars_core3 已经定义了全局分配器，这里不再重复定义
// mimalloc 优势（由 qars_core3 提供）：
// - 多线程场景下 2-3 倍性能提升
// - 更低的内存碎片
// - 更好的缓存局部性
// - 适合高频交易场景的低延迟分配

use qaexchange::exchange::instrument_registry::{InstrumentInfo, InstrumentStatus, InstrumentType};
use qaexchange::exchange::{
    AccountManager, CapitalManager, InstrumentRegistry, OrderRouter, SettlementEngine, TradeGateway,
};
use qaexchange::market::{MarketDataBroadcaster, SnapshotBroadcastService};
use qaexchange::matching::engine::ExchangeMatchingEngine;
use qaexchange::notification::broker::NotificationBroker;
use qaexchange::storage::conversion::{ConversionManager, SchedulerConfig, WorkerConfig};
use qaexchange::storage::hybrid::oltp::OltpHybridConfig;
use qaexchange::storage::subscriber::{StorageSubscriber, StorageSubscriberConfig};
use qaexchange::user::UserManager;
// use qaexchange::service::http::HttpServer;  // 未使用
use actix::Actor;
use actix_web::{middleware, web, App, HttpServer as ActixHttpServer};
use chrono;
use qaexchange::risk::RiskMonitor;
use qaexchange::service::http::admin::AdminAppState;
use qaexchange::service::http::management::ManagementAppState;
use qaexchange::service::websocket::WebSocketServer;
use qaexchange::utils::config::ExchangeConfig as TomlConfig;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// HTTP / WS 优雅停机等待秒数(`QAEX_SHUTDOWN_TIMEOUT_SECS`,默认 5)。@yutiansut @quantaxis
fn shutdown_timeout_secs() -> u64 {
    std::env::var("QAEX_SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5)
}

/// 把 actix 提取器（Json / Query / Path）产生的 400 统一包成
/// `{ success, data, error }` 信封。
///
/// 默认情况下 actix 直接返回 `text/plain` 裸文本（例如
/// `Json deserialize error: missing field ...`），前端 `web/src/api/request.js`
/// 每个 HttpServer 的 worker 线程数。
///
/// ⚠️ actix-web 默认 = 逻辑核数。本机 96 核 × 2 个 HttpServer(HTTP 8094 + WS 8095)
/// = 288 个 worker 线程。实测危害在**内存**:全局分配器是 mimalloc(由 qars_core3
/// 提供),它按线程持有本地 segment(v2 的 MI_SEGMENT_SIZE = 32 MiB)且不主动归还
/// —— 288 × 32 MiB ≈ 9.2 GB 只是地板。实测 RSS 12.9 GB 中,活数据仅约 2.4 GB,
/// smaps 显示 LazyFree = 0、Private_Dirty ≈ 全部 RSS(即并非可回收的 MADV_FREE 页)。
///
/// 交易所的请求路径是**锁受限**而非 CPU 受限(账户锁 / 合约撮合锁),
/// worker 多于必要值只会加剧 convoy,不会提高吞吐。
///
/// 可用 `QAEX_HTTP_WORKERS` / `QAEX_WS_WORKERS` 覆盖(0 或非法值 = 用默认)。
/// @yutiansut @quantaxis
fn worker_threads(env_key: &str, default_cap: usize) -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let n = std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or_else(|| cores.min(default_cap));
    log::info!(
        "{} = {} (逻辑核 {}, 默认上限 {})",
        env_key,
        n,
        cores,
        default_cap
    );
    n
}

/// 的响应拦截器读不到 `error.message`，用户只会看到
/// "Request failed with status code 400"。@yutiansut @quantaxis
fn extractor_error<E>(err: E, _req: &actix_web::HttpRequest) -> actix_web::Error
where
    E: std::fmt::Debug + std::fmt::Display + 'static,
{
    let msg = err.to_string();
    log::warn!("Request rejected by extractor: {}", msg);
    actix_web::error::InternalError::from_response(
        err,
        actix_web::HttpResponse::BadRequest().json(
            qaexchange::service::http::models::ApiResponse::<()>::error(400, msg),
        ),
    )
    .into()
}

/// 交易所服务配置
#[derive(Debug, Clone)]
struct ExchangeConfig {
    /// HTTP 监听地址
    http_address: String,

    /// WebSocket 监听地址
    ws_address: String,

    /// 存储路径
    storage_path: String,

    /// 是否启用持久化
    enable_storage: bool,

    /// `[[instruments]]` 配置段。@yutiansut @quantaxis
    ///
    /// ⚠️ 此前这一段**整段是死配置**:TOML 解析了,但运行时配置里没有这个字段,
    /// 上市清单与行情合约配置都写死在本文件里(IF2501 / IF2502 / IC2501 / IH2501)——
    /// 2026-11-08 在配置里加 FG / SA 后网页、订单簿、撮合全都看不见。
    /// 现在:写死的四个照旧,配置里**新增**的合约一并上市(见 `init_instruments`、行情配置)。
    instruments: Vec<qaexchange::utils::config::InstrumentConfig>,

    /// `[settlement]` 日终结算调度(缺省每自然日北京 00:00)@yutiansut @quantaxis
    settlement: qaexchange::utils::config::SettlementSettings,
}

impl ExchangeConfig {
    /// 从 TOML 配置文件加载
    fn from_toml(toml_config: TomlConfig) -> Self {
        Self {
            http_address: toml_config.http.bind_address(),
            ws_address: toml_config.websocket.bind_address(),
            storage_path: toml_config.storage.base_path,
            enable_storage: toml_config.storage.enabled,
            instruments: toml_config.instruments,
            settlement: toml_config.settlement,
        }
    }
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            http_address: "127.0.0.1:8080".to_string(),
            ws_address: "127.0.0.1:8081".to_string(),
            storage_path: "/tmp/qaexchange/storage".to_string(),
            enable_storage: true,
            instruments: Vec::new(),
            settlement: Default::default(),
        }
    }
}

/// 完整的交易所服务
struct ExchangeServer {
    /// 配置
    config: ExchangeConfig,

    /// 账户管理器
    account_mgr: Arc<AccountManager>,

    /// 撮合引擎
    matching_engine: Arc<ExchangeMatchingEngine>,

    /// 合约注册表
    instrument_registry: Arc<InstrumentRegistry>,

    /// 成交回报网关
    trade_gateway: Arc<TradeGateway>,

    /// 订单路由器
    order_router: Arc<OrderRouter>,

    /// 市场数据广播器
    market_broadcaster: Arc<MarketDataBroadcaster>,

    /// 市场数据服务（包含快照生成器）
    market_data_service: Arc<qaexchange::market::MarketDataService>,

    /// 结算引擎
    settlement_engine: Arc<SettlementEngine>,

    /// 资金管理器
    capital_mgr: Arc<CapitalManager>,

    /// 风险监控器
    risk_monitor: Arc<RiskMonitor>,

    /// 用户管理器
    user_mgr: Arc<UserManager>,

    /// 用户存储（用于恢复）
    user_storage: Arc<qaexchange::storage::hybrid::OltpHybridStorage>,

    /// 市场数据存储（用于持久化 TickData 和 OrderBookSnapshot）
    market_data_storage: Arc<qaexchange::storage::hybrid::OltpHybridStorage>,

    /// 存储订阅器统计信息
    storage_stats:
        Option<Arc<parking_lot::Mutex<qaexchange::storage::subscriber::SubscriberStats>>>,

    /// OLAP 转换管理器
    conversion_mgr: Option<Arc<parking_lot::Mutex<ConversionManager>>>,

    /// iceoryx2 管理器（零拷贝 IPC）
    iceoryx_manager: Option<Arc<parking_lot::RwLock<qaexchange::ipc::IceoryxManager>>>,

    /// K线Actor地址（独立Actor处理K线聚合）
    kline_actor: actix::Addr<qaexchange::market::KLineActor>,

    /// K线WAL管理器（用于历史K线查询）@yutiansut @quantaxis
    kline_wal_manager: Arc<qaexchange::storage::wal::WalManager>,

    /// 快照生成器线程句柄
    snapshot_generator_handle: Option<std::thread::JoinHandle<()>>,
}

impl ExchangeServer {
    /// 创建交易所服务
    fn new(
        config: ExchangeConfig,
        perf_config: qaexchange::utils::config::PerformanceConfig,
    ) -> Self {
        log::info!("Initializing Exchange Server...");

        // 1. 创建核心组件
        // 1.1 创建通知系统
        let notification_broker = Arc::new(NotificationBroker::new());

        // 启动通知优先级处理器（必须启动，否则通知不会被路由）
        let _priority_processor_handle = notification_broker.clone().start_priority_processor();
        log::info!("✅ Notification priority processor started");

        // 1.2 创建用户管理器并设置持久化存储
        let mut user_mgr_inner = UserManager::new();

        // 为用户管理器创建专用存储（用户数据量小，独立存储）
        let user_storage = Arc::new(
            qaexchange::storage::hybrid::OltpHybridStorage::create(
                "users",
                qaexchange::storage::hybrid::oltp::OltpHybridConfig {
                    base_path: config.storage_path.clone(),
                    memtable_size_bytes: 1 * 1024 * 1024, // 1MB（降低阈值以便测试）
                    estimated_entry_size: 512,
                    enable_olap_conversion: false,        // 用户数据不需要 OLAP 转换
                    olap_conversion_threshold: 10,
                    olap_conversion_age_seconds: 3600 * 24,
                },
            )
            .expect("Failed to create user storage"),
        );

        user_mgr_inner.set_storage(user_storage.clone());
        let user_mgr = Arc::new(user_mgr_inner);
        log::info!("✅ User manager with persistent storage initialized");

        // 1.3 创建账户管理器
        let mut account_mgr_inner = AccountManager::new();
        account_mgr_inner.set_notification_broker(notification_broker.clone());

        // 设置 UserManager 与 AccountManager 的双向关联
        // 这样开户时可以自动绑定到用户
        account_mgr_inner.set_user_manager(user_mgr.clone());

        // 现在可以安全地包装成 Arc
        let account_mgr = Arc::new(account_mgr_inner);

        let matching_engine = Arc::new(ExchangeMatchingEngine::new());
        let instrument_registry = Arc::new(InstrumentRegistry::new());

        // 1.3 创建交易网关并设置通知系统和成交记录器
        // ✨ WAL 根目录必须显式给,不能用默认值 @yutiansut @quantaxis
        //
        // TradeGateway 的默认 wal_root 是 `"./data/wal"`（trade_gateway.rs:156）——
        // **相对当前工作目录**。`with_wal_root`（:180）此前只有 #[cfg(test)] 调用，
        // 生产从不设置，于是这 554MB 数据的落点取决于你从哪个目录启动进程。
        // 挂到 storage_path 下，与其余存储同处一棵树。
        let mut trade_gateway_inner = TradeGateway::new(account_mgr.clone())
            .with_wal_root(format!("{}/gateway_wal", config.storage_path));
        trade_gateway_inner.set_notification_broker(notification_broker.clone());

        // 从 matching_engine 获取 trade_recorder 并设置到 trade_gateway
        let trade_recorder = matching_engine.get_trade_recorder();
        trade_gateway_inner = trade_gateway_inner.set_trade_recorder(trade_recorder.clone());

        // 先创建 trade_gateway Arc（后续会设置 market_data_service）
        let trade_gateway = Arc::new(trade_gateway_inner);

        let market_broadcaster = Arc::new(MarketDataBroadcaster::new());

        // 1.3.1 创建K线WAL管理器（统一到配置路径）
        let kline_wal_dir = format!("{}/klines/wal", config.storage_path);
        std::fs::create_dir_all(&kline_wal_dir).unwrap_or_else(|e| {
            log::warn!("Failed to create K-line WAL directory: {}", e);
        });
        let kline_wal_manager = Arc::new(qaexchange::storage::wal::WalManager::new(&kline_wal_dir));
        log::info!("✅ K-line WAL Manager initialized at {}", kline_wal_dir);

        // 1.3.2 启动K线Actor（订阅tick事件，独立处理K线聚合）
        log::info!("Starting KLine Actor...");
        // ⚠️ KLineActor 必须跑在**独立 Arbiter** 上,不能用 `.start()` 挂在主运行时 @yutiansut @quantaxis
        //
        // `.start()` 把 actor 放到当前 Arbiter —— 即 `#[actix_web::main]` 的**单线程**主运行时;
        // 它的 tick 处理循环(`ctx.spawn`)每收完一根 K 线就同步 `wal_manager.append()`,
        // 而 `append` 每条都 `flush` + `sync_all`(fsync)。2026-11-08 实测:把 60s 快照落盘
        // 移出主运行时之后,主线程**仍有 96% 时间处于 D 态**,wchan 以
        // `jbd2_log_wait_commit`(fsync 等日志提交)为主,同运行时上的盘口快照广播
        // 8 秒只发出 1 张。fsync 在这块盘上一次可达数百毫秒(WAL 单测 P99 446ms)。
        // 放进独立 Arbiter 后,fsync 只会拖慢 K 线自己,不再饿死主运行时上的其他任务。
        // actix-rt 2.11 的 `Arbiter` 没有 Drop 实现 —— 离开作用域不会停线程,随 System 一起退出。
        let kline_arbiter = actix::Arbiter::new();
        let kline_broadcaster = market_broadcaster.clone();
        let kline_wal = kline_wal_manager.clone();
        let kline_actor = qaexchange::market::KLineActor::start_in_arbiter(
            &kline_arbiter.handle(),
            move |_ctx| {
        qaexchange::market::KLineActor::new(
            kline_broadcaster,
            kline_wal,
        )
        // .with_instruments(vec![])  // 空列表表示订阅所有合约
        // ✨ 开启因子计算 —— 否则 /factors 页面结构性地拿不到数据
        // @yutiansut @quantaxis
        //
        // `enable_factor_compute` 默认 false(kline_actor.rs:72「默认关闭,需要显式启用」),
        // 而这里从来没有显式启用过。链条:不算因子 → 不广播 MarketDataEvent::FactorUpdate
        // → DIFF 快照里永远没有 `factors` 键 → 前端 `/factors` 读 `snapshot.factors`
        // 永远是空。导航里挂着一个结构性无法显示的页面。
        // (实测:启动日志中「Factor computation enabled」出现 0 次;
        //  订阅 WS 12 秒收到的补丁顶层键里没有 factors。)
        //
        // 开销可忽略:DEFAULT_FACTORS 7 个(ma5/ma10/ma20/ema12/ema26/rsi14/macd),
        // 只在 K 线收盘时对已收盘的那根算一次,不在 tick 热路径上。
        .with_factor_compute(true)
            },
        );
        log::info!("✅ KLine Actor started on dedicated arbiter (subscribed to tick events)");

        // 1.4 创建 iceoryx2 管理器（如果启用）
        let iceoryx_manager: Option<Arc<parking_lot::RwLock<qaexchange::ipc::IceoryxManager>>> =
            if perf_config.iceoryx.enabled {
                log::info!("Initializing iceoryx2 manager...");
                let ipc_config = qaexchange::ipc::IpcConfig {
                    service_prefix: perf_config.iceoryx.service_prefix.clone(),
                    max_subscribers: perf_config.iceoryx.max_subscribers,
                    queue_capacity: perf_config.iceoryx.queue_capacity,
                    max_message_size: perf_config.iceoryx.max_message_size,
                };

                #[cfg(feature = "iceoryx2")]
                let manager = {
                    let mut manager = qaexchange::ipc::IceoryxManager::new(ipc_config);

                    // 启动市场数据发布者（需要 iceoryx2 特性）
                    if let Err(e) = manager.start_market_data_publisher() {
                        log::error!("Failed to start iceoryx2 market data publisher: {}", e);
                    } else {
                        log::info!("✅ iceoryx2 market data publisher started");
                    }

                    if let Err(e) = manager.start_notification_publisher() {
                        log::error!("Failed to start iceoryx2 notification publisher: {}", e);
                    } else {
                        log::info!("✅ iceoryx2 notification publisher started");
                    }

                    manager
                };

                #[cfg(not(feature = "iceoryx2"))]
                let manager = {
                    log::warn!("⚠️  iceoryx2 enabled in config but feature not compiled (use --features iceoryx2)");
                    qaexchange::ipc::IceoryxManager::new(ipc_config)
                };

                Some(Arc::new(parking_lot::RwLock::new(manager)))
            } else {
                log::info!(
                    "iceoryx2 disabled (set enabled=true in config/performance.toml to enable)"
                );
                None
            };

        // 2. 创建订单路由器
        let mut order_router = OrderRouter::new(
            account_mgr.clone(),
            matching_engine.clone(),
            instrument_registry.clone(),
            trade_gateway.clone(),
        );

        // 2.1 为订单路由器创建市场数据存储（用于持久化 TickData 和 OrderBookSnapshot）
        let market_data_storage = Arc::new(
            qaexchange::storage::hybrid::OltpHybridStorage::create(
                "market_data",
                qaexchange::storage::hybrid::oltp::OltpHybridConfig {
                    base_path: config.storage_path.clone(),
                    memtable_size_bytes: 2 * 1024 * 1024, // 2MB（降低阈值以便测试，生产环境建议 64MB）
                    estimated_entry_size: 256,            // TickData + OrderBookSnapshot 平均大小
                    enable_olap_conversion: true,         // 市场数据启用 OLAP 转换（历史分析用）
                    olap_conversion_threshold: 10,
                    olap_conversion_age_seconds: 3600 * 24, // 1 天前的数据转换为 OLAP
                },
            )
            .expect("Failed to create market data storage"),
        );

        // 3. 设置市场数据广播器和存储到订单路由器
        order_router.set_market_broadcaster(market_broadcaster.clone());
        order_router.set_storage(market_data_storage.clone());
        log::info!("✅ OrderRouter market data storage initialized");

        // 启动批量刷新线程（性能优化：tick数据批量写入）
        order_router.start_batch_flush_worker();
        log::info!("✅ Batch flush worker started (10ms interval, max 1000 records/batch)");

        // 配置优先级队列（如果启用）
        if perf_config.priority_queue.enabled {
            order_router.enable_priority_queue(
                perf_config.priority_queue.low_queue_limit,
                perf_config.priority_queue.critical_amount_threshold,
            );
            // 添加VIP用户
            if !perf_config.priority_queue.vip_users.is_empty() {
                order_router.add_vip_users(perf_config.priority_queue.vip_users.clone());
                log::info!(
                    "✅ Added {} VIP users to priority queue",
                    perf_config.priority_queue.vip_users.len()
                );
            }
        } else {
            log::info!(
                "Priority queue disabled (set enabled=true in config/performance.toml to enable)"
            );
        }

        // 3.9 交易时段管制 @yutiansut @quantaxis
        //
        // OrderRouter::trading_state_machine 默认是 None(:310,:411)。
        //
        // 【历史】此处原注释写「set_trading_state_machine **全库零调用**」——
        // 那是**接线之前**的状态,下面 :430 已经调了,注释当时没同步更新。
        // 2026-09-05 复核:`grep -rn set_trading_state_machine src/` = 3 处
        // (定义 order_router.rs:331 + 本处调用 + 本注释)。
        //
        // 现状:默认仍**不启用**(见下方 env 开关),所以门禁默认进不去 ——
        // **7×24 都能下单,没有集合竞价/午休/收盘之分**。这一半仍然成立。
        //
        // validate_order(trading_session.rs:461)本身是完整的:
        // 先查交易日历当前时段的 allow_order,再查合约的 TradingState。
        //
        // ⚠️ 默认**关闭**:打开后非交易时段的下单会被拒(错误码 4100),
        // 现有的 7×24 压测脚本会立刻大面积失败。
        // 设 QAEX_ENFORCE_TRADING_SESSION=1 打开。
        if std::env::var("QAEX_ENFORCE_TRADING_SESSION")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            order_router.set_trading_state_machine(Arc::new(
                qaexchange::exchange::TradingStateMachine::new(),
            ));
            log::info!("🕐 Trading session enforcement ENABLED (orders rejected outside session hours)");
        } else {
            log::info!("Trading session enforcement disabled (set QAEX_ENFORCE_TRADING_SESSION=1 to enable)");
        }

        let order_router = Arc::new(order_router);

        // 4. 创建结算引擎
        let settlement_engine = Arc::new(SettlementEngine::new(account_mgr.clone()));
        settlement_engine.set_order_router(order_router.clone());
        // 日终归档当日快照 / 落结算后快照用(与 start_snapshot_scheduler 同一目录)@yutiansut @quantaxis
        settlement_engine.set_snapshot_dir(format!("{}/snapshots", config.storage_path));

        // ✨ 条件单引擎也需要 order_router @yutiansut @quantaxis
        //
        // ConditionalOrderEngine::set_order_router(conditional_order.rs:121)
        // 此前**从未在生产调用**,`self.order_router` 恒为 None,
        // 于是 check_triggers 里的 `if let Some(router)` 分支永远进不去 ——
        // 就算触发了也不会真的下单。
        qaexchange::exchange::CONDITIONAL_ORDER_ENGINE
            .write()
            .set_order_router(order_router.clone());

        // 5. 创建资金管理器
        let capital_mgr = Arc::new(CapitalManager::new(account_mgr.clone()));

        // 6. 创建风险监控器
        let risk_monitor = Arc::new(RiskMonitor::new(account_mgr.clone()));
        settlement_engine.set_risk_monitor(risk_monitor.clone());
        // ✨ 接入行情源,启用盯市刷价 @yutiansut @quantaxis
        //
        // 此前行情侧与账户侧之间**没有任何连线**:
        //   · market/{mod,snapshot_generator,kline_actor} 涉及账户 0 处
        //   · MarketDataBroadcaster 的生产订阅者只有 kline_actor(画K线)
        //   · qars `on_price_change` 生产调用只在 receive_deal_sim(用成交价)
        // 结果是持仓价停在各账户最后一次成交时,全系统
        // Σposition_profit = +2.53 亿(封闭系统应为 0)。
        risk_monitor.set_price_source(matching_engine.clone());

        // 7. 创建市场数据服务（包含快照生成器）
        let market_data_service = {
            let mut service = qaexchange::market::MarketDataService::new(matching_engine.clone());

            // 设置存储（用于市场数据恢复）
            service = service.with_storage(market_data_storage.clone());

            // 注入账户管理器和广播器
            service = service.with_account_manager(account_mgr.clone());
            service = service.with_broadcaster(market_broadcaster.clone());

            // 设置 iceoryx2（如果启用）
            if let Some(ref iceoryx_mgr) = iceoryx_manager {
                service = service.with_iceoryx(iceoryx_mgr.clone());
            }

            // ✨ 合约配置(含**真实乘数**)必须注入 —— 否则接口只会吐兜底值 @yutiansut @quantaxis
            //
            // `MarketDataService::instrument_configs` 此前**从未被填充**
            // (`set_instrument_configs` 唯一调用点在 `market/mod.rs:267` 的
            //  `new_with_configs`,生产路径 main.rs 从没走过),于是
            // `get_instruments()` 永远落到 `market/mod.rs:413` 的兜底
            //     (format!("{} 期货", id), 300.0, 0.2)
            // 对**所有**合约都报乘数 300。
            //
            // 而 IC2501 的真实乘数是 **200**(见 init_instruments,main.rs:542) ——
            // 前端 `positions/index.vue` 拿不到真值只能自己硬编码 ×300,
            // 对 IC 系合约的持仓市值/盈亏比**偏差 50%**。
            //
            // ⚠️ 这份表必须与 `init_instruments()` 的注册值保持一致。
            //    两处都硬编码是隐患,但把合约表提到 `new()` 之前需要较大重构,
            //    先用一致性断言兜住:启动时若两边对不上会 panic。
            let inst_specs: Vec<(&str, &str, f64, f64)> = vec![
                // (合约, 名称, 乘数, 最小变动价位)
                ("IF2501", "沪深300股指期货2501", 300.0, 0.2),
                ("IF2502", "沪深300股指期货2502", 300.0, 0.2),
                ("IC2501", "中证500股指期货2501", 200.0, 0.2),
                ("IH2501", "上证50股指期货2501",  300.0, 0.2),
            ];
            let mut configs: Vec<qaexchange::utils::config::InstrumentConfig> = inst_specs
                .iter()
                .map(|(id, name, mult, tick)| qaexchange::utils::config::InstrumentConfig {
                    instrument_id: id.to_string(),
                    name: name.to_string(),
                    exchange_id: "CFFEX".to_string(),
                    product_type: "index_future".to_string(),
                    init_price: 0.0,
                    is_trading: true,
                    multiplier: *mult,
                    tick_size: *tick,
                    commission_per_lot: None,
                    commission_rate: None,
                    margin_rate: None,
                })
                .collect();
            // 配置里新增的合约(与 `init_instruments` 用同一份 `config.instruments`)@yutiansut @quantaxis
            for c in &config.instruments {
                if !configs.iter().any(|x| x.instrument_id == c.instrument_id) {
                    configs.push(c.clone());
                }
            }
            let instruments: Vec<String> =
                configs.iter().map(|c| c.instrument_id.clone()).collect();
            service.set_instrument_configs(configs);

            // 配置快照生成器：订阅所有合约，每秒生成一次快照
            service = service.with_snapshot_generator(instruments, 1000);

            Arc::new(service)
        };
        settlement_engine.set_market_data_service(market_data_service.clone());
        log::info!("✅ Market data service with snapshot generator initialized");

        // 7.1 设置 market_data_service 到 trade_gateway（用于更新快照统计）
        // 由于 trade_gateway 已经是 Arc，需要使用 unsafe 获取可变引用
        // 安全性：此时 trade_gateway 只有一个引用（刚创建），可以安全修改
        // ✨ 不再需要 unsafe。原注释写着「此时 trade_gateway 只有一个引用(刚创建)」——
        // **那是错的**::364 已经把 `trade_gateway.clone()` 交给了 `OrderRouter`,
        // 早于此处 178 行。字段已改为 `OnceLock`,`set` 只要 `&self`。
        // @yutiansut @quantaxis
        trade_gateway.set_market_data_service(market_data_service.clone());
        log::info!("✅ Market data service connected to trade gateway");

        log::info!("✅ Core components initialized");
        log::info!("✅ Market data broadcaster initialized");
        log::info!("✅ Settlement engine initialized");
        log::info!("✅ Capital manager initialized");
        log::info!("✅ Risk monitor initialized");
        log::info!("✅ User manager initialized");

        Self {
            config,
            account_mgr,
            matching_engine,
            instrument_registry,
            trade_gateway,
            order_router,
            market_broadcaster,
            market_data_service,
            settlement_engine,
            capital_mgr,
            risk_monitor,
            user_mgr,
            user_storage,
            market_data_storage,
            storage_stats: None,
            conversion_mgr: None,
            iceoryx_manager,
            kline_actor,
            kline_wal_manager,
            snapshot_generator_handle: None,
        }
    }

    /// 初始化合约
    fn init_instruments(&self) {
        log::info!("Initializing instruments...");

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 注册合约：沪深300股指期货
        let mut instruments = vec![
            InstrumentInfo {
                instrument_id: "IF2501".to_string(),
                instrument_name: "沪深300股指期货2501".to_string(),
                instrument_type: InstrumentType::IndexFuture,
                exchange: "CFFEX".to_string(),
                contract_multiplier: 300,
                price_tick: 0.2,
                margin_rate: 0.12,
                commission_rate: 0.0001,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: InstrumentStatus::Active,
                list_date: Some("2024-09-16".to_string()),
                expire_date: Some("2025-01-17".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            InstrumentInfo {
                instrument_id: "IF2502".to_string(),
                instrument_name: "沪深300股指期货2502".to_string(),
                instrument_type: InstrumentType::IndexFuture,
                exchange: "CFFEX".to_string(),
                contract_multiplier: 300,
                price_tick: 0.2,
                margin_rate: 0.12,
                commission_rate: 0.0001,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: InstrumentStatus::Active,
                list_date: Some("2024-10-21".to_string()),
                expire_date: Some("2025-02-21".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            InstrumentInfo {
                instrument_id: "IC2501".to_string(),
                instrument_name: "中证500股指期货2501".to_string(),
                instrument_type: InstrumentType::IndexFuture,
                exchange: "CFFEX".to_string(),
                contract_multiplier: 200,
                price_tick: 0.2,
                margin_rate: 0.12,
                commission_rate: 0.0001,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: InstrumentStatus::Active,
                list_date: Some("2024-09-16".to_string()),
                expire_date: Some("2025-01-17".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            InstrumentInfo {
                instrument_id: "IH2501".to_string(),
                instrument_name: "上证50股指期货2501".to_string(),
                instrument_type: InstrumentType::IndexFuture,
                exchange: "CFFEX".to_string(),
                contract_multiplier: 300,
                price_tick: 0.2,
                margin_rate: 0.12,
                commission_rate: 0.0001,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: InstrumentStatus::Active,
                list_date: Some("2024-09-16".to_string()),
                expire_date: Some("2025-01-17".to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        ];

        // ✨ 配置 `[[instruments]]` 里新增的合约一并上市(写死的四个保持原样)@yutiansut @quantaxis
        //    保证金率取 qars MarketPreset(真正冻结与扣费用的就是它,见下方一致性校验);
        //    按成交额计费的品种手续费率同样取 preset,按手计费的(如 FG)保留默认值仅供预检。
        let mut config_init_price: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for c in &self.config.instruments {
            if instruments.iter().any(|i| i.instrument_id == c.instrument_id) {
                continue;
            }
            let preset = qaexchange::core::account_ext::effective_preset(&c.instrument_id);
            let instrument_type = if c.product_type.contains("index") || c.exchange_id == "CFFEX" {
                InstrumentType::IndexFuture
            } else {
                InstrumentType::CommodityFuture
            };
            instruments.push(InstrumentInfo {
                instrument_id: c.instrument_id.clone(),
                instrument_name: c.name.clone(),
                instrument_type,
                exchange: c.exchange_id.clone(),
                contract_multiplier: c.multiplier.round() as i32,
                price_tick: c.tick_size,
                margin_rate: if preset.buy_frozen_coeff > 0.0 { preset.buy_frozen_coeff } else { 0.12 },
                commission_rate: if preset.commission_coeff_peramount > 0.0 {
                    preset.commission_coeff_peramount
                } else {
                    0.0001
                },
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: if c.is_trading { InstrumentStatus::Active } else { InstrumentStatus::Suspended },
                list_date: None,
                expire_date: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            config_init_price.insert(c.instrument_id.clone(), c.init_price);
        }

        // 手续费率不一致按**汇总**打印,不逐合约刷屏 @yutiansut @quantaxis
        //
        // 实测:4 个合约的 `commission_rate` 全是 0.0001、qars preset 全是 2.301e-05
        // ⇒ 4 条告警**内容完全一致**,纯重复。而同批的保证金率告警只有 3 条,
        // 其中指向 2.49 亿担保缺口的那条会被这 4 条重复淹没
        // (先前正是这么丢掉 IH 那条的)。
        let mut comm_mismatch: Vec<(String, f64, f64, f64)> = Vec::new();

        for inst in &instruments {
            // 注册合约到合约注册表
            if let Err(e) = self.instrument_registry.register(inst.clone()) {
                log::error!("Failed to register {}: {}", inst.instrument_id, e);
                continue;
            }

            // 注册到撮合引擎（初始价格）
            let init_price = match inst.instrument_id.as_str() {
                "IF2501" => 3800.0,
                "IF2502" => 3820.0,
                "IC2501" => 5600.0,
                "IH2501" => 2800.0,
                other => config_init_price
                    .get(other)
                    .copied()
                    .filter(|p| *p > 0.0)
                    .unwrap_or(100.0),
            };

            self.matching_engine
                .register_instrument(inst.instrument_id.clone(), init_price)
                .expect("Failed to register instrument to matching engine");

            // 设置初始结算价
            self.settlement_engine
                .set_settlement_price(inst.instrument_id.clone(), init_price);

            // 设置快照生成器的昨收盘价（用于涨跌幅计算）
            self.market_data_service
                .set_pre_close(&inst.instrument_id, init_price);

            // ✨ 费率一致性校验 @yutiansut @quantaxis
            //
            // 本系统有**四套**费率数据源,分工其实是合理的:
            //   · instrument_registry     → 下单前资金预检(pre_trade_check / order_router)
            //   · qars MarketPreset       → **真正冻结与扣费**(calc_coeff / calc_commission)
            //   · account-admin 费率接口  → 管理页展示(品种级 CTP 格式)
            // 问题在于三者之间**没有任何一致性约束**:
            //   实测 registry.margin_rate = 0.12 而 qars buy_frozen_coeff = 0.1,
            //   预检偏严 20% —— 方向恰好安全,但那是巧合。
            //   若有人把 registry 调到 0.08,预检就会放行实际开不出的仓。
            //   (2026-09-04 我自己写验证判据时也拿错了源,预期值算成 142,481
            //    而实际是 114,000 —— 连排查者都会踩,说明必须有显式约束。)
            //
            // 这里只告警不阻断:preset 是品种级预设,registry 是合约级配置,
            // 允许按合约微调,但偏离过大几乎总是配错了。
            {
                // 生效费率(qars 预设 + 按合约覆盖),与实际冻结 / 扣费一致
                let preset = qaexchange::core::account_ext::effective_preset(&inst.instrument_id);
                let preset_margin = preset.buy_frozen_coeff;
                if preset_margin > 0.0 {
                    let dev = (inst.margin_rate - preset_margin).abs() / preset_margin;
                    // ⚠️ 阈值从 0.25 收紧到 0.05 @yutiansut @quantaxis
                    //
                    // 2026-09-05 全账户实测(按 margin_long+short / position_cost_long+short):
                    //   IC  0.1200 vs 0.12  dev=0.00  —— 一致
                    //   IF  0.1000 vs 0.12  dev=0.20  —— **旧阈值 0.25 下不报**,而缺口约 1.00 亿
                    //   IH  0.0500 vs 0.12  dev=1.40  —— 报了,但淹没在同批手续费告警里
                    // 即旧阈值恰好把 IF 的一亿缺口漏掉。保证金率是资金安全的一等参数,
                    // 5% 的偏离就该说话 —— 它不像手续费那样允许「按合约微调」。
                    if dev > 0.05 {
                        log::warn!(
                            "⚠️ 保证金率不一致 {}: registry={:.4} vs qars preset buy_frozen_coeff={:.4} \
                             (偏离 {:.0}%) —— 预检用前者、**实际扣费用后者**;\
                             每 1 亿名义敞口的担保差额约 {:.0} 万",
                            inst.instrument_id, inst.margin_rate, preset_margin, dev * 100.0,
                            (inst.margin_rate - preset_margin).abs() * 1e8 / 1e4
                        );
                    }
                }
                let preset_comm = preset.commission_coeff_peramount;
                if preset_comm > 0.0 {
                    let dev = (inst.commission_rate - preset_comm).abs() / preset_comm;
                    if dev > 1.0 {
                        // 收集而非逐条打印 —— 见循环外的汇总。@yutiansut @quantaxis
                        comm_mismatch.push((
                            inst.instrument_id.clone(),
                            inst.commission_rate,
                            preset_comm,
                            dev,
                        ));
                    }
                }
            }

            log::info!(
                "  ✓ {} @ {} (margin: {}%, commission: {}%)",
                inst.instrument_id,
                init_price,
                inst.margin_rate * 100.0,
                inst.commission_rate * 100.0
            );
        }

        // 手续费率不一致的汇总告警 @yutiansut @quantaxis
        if !comm_mismatch.is_empty() {
            let all_same = comm_mismatch
                .windows(2)
                .all(|w| (w[0].1 - w[1].1).abs() < 1e-12 && (w[0].2 - w[1].2).abs() < 1e-12);
            let (_, reg, pre, dev) = &comm_mismatch[0];
            let ids: Vec<&str> = comm_mismatch.iter().map(|(i, ..)| i.as_str()).collect();
            if all_same {
                log::warn!(
                    "⚠️ 手续费率不一致({} 个合约,参数相同): registry={:.6} vs qars preset={:.6} \
                     (偏离 {:.0}%,即实际只收 1/{:.1});预检按前者冻结、**实际按后者扣费** \
                     ⇒ 每 1 亿名义敞口少收约 {:.0} 元。合约: {}",
                    comm_mismatch.len(), reg, pre, dev * 100.0, reg / pre,
                    (reg - pre) * 1e8, ids.join(", ")
                );
            } else {
                log::warn!("⚠️ 手续费率不一致({} 个合约,参数不同):", comm_mismatch.len());
                for (id, r, p, d) in &comm_mismatch {
                    log::warn!("    {}: registry={:.6} vs preset={:.6} (偏离 {:.0}%)", id, r, p, d * 100.0);
                }
            }
        }

        check_trading_calendar_horizon();

        log::info!("✅ {} instruments initialized", instruments.len());
    }

    /// 启动快照生成器
    fn start_snapshot_generator(&mut self) {
        if let Some(handle) = self.market_data_service.start_snapshot_generator() {
            self.snapshot_generator_handle = Some(handle);
            log::info!("✅ Snapshot generator started (1s interval)");
        } else {
            log::warn!("⚠️  Snapshot generator not started (not configured)");
        }
    }

    /// 启动存储订阅器
    fn start_storage_subscriber(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.enable_storage {
            log::info!("Storage disabled");
            return None;
        }

        log::info!("Starting storage subscriber...");

        let storage_config = StorageSubscriberConfig {
            storage_config: OltpHybridConfig {
                base_path: self.config.storage_path.clone(),
                memtable_size_bytes: 2 * 1024 * 1024, // 2 MB（降低阈值以便测试）
                estimated_entry_size: 256,
                enable_olap_conversion: true,         // 订阅存储启用 OLAP 转换
                olap_conversion_threshold: 10,
                olap_conversion_age_seconds: 3600 * 24, // 1 天前的数据转换为 OLAP
            },
            batch_size: 100,
            batch_timeout_ms: 10,
            buffer_size: 10000,
        };

        let (subscriber, storage_sender, stats_handle) = StorageSubscriber::new(storage_config);

        // 保存统计信息句柄
        self.storage_stats = Some(stats_handle);

        // 订阅 NotificationBroker（AccountManager 和 TradeGateway 的通知都会发布到这里）
        if let Some(broker) = self.account_mgr.notification_broker() {
            broker.subscribe_global("storage", storage_sender);
            log::info!("✅ Storage subscriber connected to notification broker");
        } else {
            log::warn!("⚠️  Notification broker not available, storage subscriber will not receive notifications");
        }

        // 启动订阅器
        let handle = tokio::spawn(async move {
            subscriber.run().await;
        });

        log::info!("✅ Storage subscriber started");
        log::info!("   Path: {}", self.config.storage_path);
        log::info!("   Batch: 100 records / 10ms timeout");

        Some(handle)
    }

    /// 启动 OLAP 转换系统
    fn start_olap_conversion(&mut self) {
        if !self.config.enable_storage {
            log::info!("OLAP conversion disabled (storage disabled)");
            return;
        }

        log::info!("Starting OLAP conversion system...");

        let storage_base = PathBuf::from(&self.config.storage_path);
        let metadata_path = storage_base.join("conversion_metadata.json");

        let scheduler_config = SchedulerConfig {
            scan_interval_secs: 300,    // 5 分钟扫描一次
            min_sstables_per_batch: 3,  // 至少 3 个 SSTable
            max_sstables_per_batch: 20, // 最多 20 个 SSTable
            min_sstable_age_secs: 60,   // 文件至少 1 分钟未修改
            max_retries: 5,
            zombie_timeout_secs: 3600, // 1 小时超时
        };

        let worker_config = WorkerConfig {
            worker_count: 2, // 2 个 worker
            batch_read_size: 10000,
            delete_source_after_success: true,
            source_retention_secs: 3600, // 保留 1 小时
        };

        // ✨ 走进程级单例 @yutiansut @quantaxis
        //
        // 原实现 `self.conversion_mgr = Some(..)` 需要 `&mut self`,
        // 而 `start_http_server` 用 `Arc<Self>` 读 —— AppState 拿到的永远是 None,
        // 监控接口 `olap.total_tasks` 恒为 0,**即使转换实际在工作**
        // (实测已产出 6 个 Parquet,接口仍报 0)。
        // 改成单例后,AppState 直接从单例取,与赋值时序无关。
        self.conversion_mgr = qaexchange::storage::conversion::global_conversion_manager(
            storage_base,
            metadata_path,
            scheduler_config,
            worker_config,
        );
        if self.conversion_mgr.is_some() {
            log::info!("✅ OLAP conversion system started (进程级单例)");
        }
    }

    /// 启动 HTTP 服务器
    async fn start_http_server(self: Arc<Self>) -> io::Result<actix_web::dev::Server> {
        log::info!("Starting HTTP server at {}...", self.config.http_address);

        let app_state = Arc::new(qaexchange::service::http::handlers::AppState {
            order_router: self.order_router.clone(),
            account_mgr: self.account_mgr.clone(),
            settlement_engine: self.settlement_engine.clone(),
            trade_recorder: self.matching_engine.get_trade_recorder(),
            user_mgr: self.user_mgr.clone(),
            storage_stats: self.storage_stats.clone(),
            // 优先用自身持有的;为 None 时回退到全局单例 ——
            // 覆盖 `&mut self` 赋值发生在 Arc 化之后的时序问题。@yutiansut @quantaxis
            conversion_mgr: self
                .conversion_mgr
                .clone()
                .or_else(qaexchange::storage::conversion::get_global_conversion_manager),
            // Phase 14: 数据查询存储组件 @yutiansut @quantaxis
            market_data_storage: Some(self.market_data_storage.clone()),
            kline_wal_manager: Some(self.kline_wal_manager.clone()),
            // 服务器启动时间 @yutiansut @quantaxis
            server_start_time: chrono::Utc::now(),
            // WebSocket 连接计数器 @yutiansut @quantaxis
            ws_connection_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            // 公告广播用 SnapshotManager;None 时 account_admin 回退到全局静态
            // (WebSocketServer::new 会设置它) @yutiansut @quantaxis
            snapshot_mgr: None,
        });

        // ✨ 复用 self.market_data_service,不要在这里另造一个 @yutiansut @quantaxis
        //
        // 原实现在此处 `MarketDataService::new(...)` **新建了第二个实例**,
        // 与 `new()` 里(main.rs:426)构造并注入了配置/快照生成器/账户管理器/
        // 广播器的那一个完全独立。HTTP handler 的
        // `web::Data<MarketDataService>` 拿到的是这个新的空壳,后果:
        //   · `/api/market/instruments` 的 `instrument_configs` 恒空 →
        //     落到 `market/mod.rs:413` 的兜底,**对所有合约都报乘数 300**,
        //     而 IC2501 真实是 200 → 持仓页市值/盈亏比偏差 50%;
        //   · 我在 :426 注入合约配置后重启,接口**仍然返回 300** ——
        //     就是被这个分叉吃掉了。
        //
        // 这与本会话早先「7 个 ConversionManager 实例」是同一类问题:
        // **同一个服务在两处各建一次,配置只进了其中一个。**
        let mut market_service = (*self.market_data_service).clone();

        // 如果启用了 iceoryx2，将 manager 传递给 MarketDataService
        if let Some(ref manager) = self.iceoryx_manager {
            market_service = market_service.with_iceoryx(manager.clone());
        }

        // 创建管理端状态（合约管理、结算管理）
        let admin_state = AdminAppState {
            instrument_registry: self.instrument_registry.clone(),
            settlement_engine: self.settlement_engine.clone(),
            account_mgr: self.account_mgr.clone(),
            // 合约上市要落到撮合引擎和行情服务，否则是幽灵合约 @yutiansut @quantaxis
            matching_engine: self.matching_engine.clone(),
            market_data_service: self.market_data_service.clone(),
        };
        let admin_data = web::Data::new(admin_state);

        // 创建管理端状态（账户管理、资金管理、风控监控、全市场订单/成交查询）
        // @yutiansut @quantaxis
        let management_state = ManagementAppState {
            account_mgr: self.account_mgr.clone(),
            capital_mgr: self.capital_mgr.clone(),
            risk_monitor: self.risk_monitor.clone(),
            settlement_engine: self.settlement_engine.clone(),
            order_router: self.order_router.clone(),
            trade_recorder: self.matching_engine.get_trade_recorder(),
        };
        let management_data = web::Data::new(management_state);

        // account_admin 的三个 handler（change_password / get_commission_statistics /
        // get_margin_summary）直接提取 web::Data<Arc<AccountManager>>，
        // 未注册时 actix 返回 500 "Requested application data is not configured correctly"
        // @yutiansut @quantaxis
        let account_mgr_data = web::Data::new(self.account_mgr.clone());

        let bind_address = self.config.http_address.clone();
        let kline_actor_addr = self.kline_actor.clone();
        // 供管理端鉴权中间件使用 @yutiansut @quantaxis
        let admin_user_mgr = self.user_mgr.clone();
        // 只解析、只打印一次 —— App::new 闭包会被每个 worker 执行
        let require_admin_auth = std::env::var("QAEX_REQUIRE_ADMIN_AUTH")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if require_admin_auth {
            log::info!("🔒 Admin auth enabled for /api/admin, /api/management, /api/account-admin");
        } else {
            log::warn!("⚠️  Admin auth DISABLED — /api/admin/* accepts unauthenticated requests. Set QAEX_REQUIRE_ADMIN_AUTH=1 to enforce.");
        }

        let server = ActixHttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(app_state.clone()))
                .app_data(web::Data::new(market_service.clone())) // MarketDataService 实现了 Clone
                .app_data(web::Data::new(kline_actor_addr.clone())) // KLineActor 地址
                .app_data(admin_data.clone())
                .app_data(management_data.clone())
                .app_data(account_mgr_data.clone()) // account_admin handlers @yutiansut @quantaxis
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
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
                // 提取器错误统一走 JSON 信封 @yutiansut @quantaxis
                .app_data(web::JsonConfig::default().error_handler(extractor_error))
                .app_data(web::QueryConfig::default().error_handler(extractor_error))
                .app_data(web::PathConfig::default().error_handler(extractor_error))
                .configure({
                    // ✨ 管理端鉴权 @yutiansut @quantaxis
                    // 默认**关闭**以保持既有行为(现有压测/运维脚本都不带 token);
                    // 设 QAEX_REQUIRE_ADMIN_AUTH=1 打开。生产环境应当打开。
                    //
                    // ⚠️ 本闭包每个 actix worker 执行一次(生产 96 个),
                    // 所以状态日志**不能**放这里 —— 已上移到闭包之前只打一次。
                    let guard = qaexchange::service::http::auth::AdminAuth::new(
                        admin_user_mgr.clone(),
                        require_admin_auth,
                    );
                    move |cfg: &mut web::ServiceConfig| {
                        qaexchange::service::http::routes::configure(cfg, guard.clone())
                    }
                })
        })
        .workers(worker_threads("QAEX_HTTP_WORKERS", 16))
        // 优雅停机只等 5 秒(actix 默认 30 秒):keep-alive 连接在这段时间里还会继续下单,
        // 2026-11-08 06:38 实测停机共 74 秒(HTTP 30s + WS 30s + 落盘)。@yutiansut @quantaxis
        .shutdown_timeout(shutdown_timeout_secs())
        .bind(&bind_address)?
        // ⚠️ 关掉 actix 自带的信号处理 —— 见 run() 末尾的落盘说明。@yutiansut @quantaxis
        // actix 默认收到 SIGTERM 会 stop 整个 System,运行时当场终止,
        // `try_join!` **之后的代码永远不会执行**(2026-09-05 实测:
        // SIGTERM 后进程 1 秒退出、快照文件数 0→0、日志无落盘记录)。
        .disable_signals()
        .run();

        log::info!("✅ HTTP server started at http://{}", bind_address);
        log::info!("   Health: http://{}/health", bind_address);
        log::info!(
            "   Market API: http://{}/api/market/instruments",
            bind_address
        );
        log::info!(
            "   Admin API: http://{}/api/admin/market/order-stats",
            bind_address
        );

        Ok(server)
    }

    /// 启动订单簿快照广播服务
    fn start_snapshot_broadcaster(&self) {
        log::info!("Starting orderbook snapshot broadcaster...");

        // 启动实时行情快照发布(事件驱动,10ms 合并窗口;500ms 只是空闲心跳)
        // 独立 OS 线程 —— 见 snapshot_broadcaster.rs 模块文档 @yutiansut @quantaxis
        let _quote_publisher = SnapshotBroadcastService::spawn(
            self.matching_engine.clone(),
            self.market_broadcaster.clone(),
            self.market_data_service.clone(), // ✨ 共用成交写入的那个实例(累计量/额来自它的日 K)
            500, // 空闲心跳间隔
        );

        log::info!("✅ Realtime quote publisher started (event-driven, 10ms coalesce, 500ms heartbeat)");
    }

    /// 启动 WebSocket 服务器
    async fn start_websocket_server(self: Arc<Self>) -> io::Result<actix_web::dev::Server> {
        log::info!("Starting WebSocket server at {}...", self.config.ws_address);

        let ws_server = Arc::new(WebSocketServer::new(
            self.order_router.clone(),
            self.account_mgr.clone(),
            self.user_mgr.clone(),
            self.trade_gateway.clone(),
            self.market_broadcaster.clone(),
            self.kline_actor.clone(),
            self.market_data_service.clone(),
        ));

        let bind_address = self.config.ws_address.clone();

        let server = ActixHttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(ws_server.clone()))
                .wrap(middleware::Logger::default())
                .route(
                    "/ws",
                    web::get().to(qaexchange::service::websocket::ws_route),
                )
                .route(
                    "/ws/diff",
                    web::get().to(qaexchange::service::websocket::ws_diff_route),
                )
                .route("/health", web::get().to(|| async { "OK" }))
        })
        .workers(worker_threads("QAEX_WS_WORKERS", 8))
        .shutdown_timeout(shutdown_timeout_secs())
        .bind(&bind_address)?
        .disable_signals()   // 同 HTTP,见 run() 末尾 @yutiansut @quantaxis
        .run();

        log::info!("✅ WebSocket server started at ws://{}", bind_address);
        log::info!(
            "   Legacy Protocol: ws://{}/ws?user_id=<USER_ID>",
            bind_address
        );
        log::info!(
            "   DIFF Protocol:   ws://{}/ws/diff?user_id=<USER_ID> (Recommended)",
            bind_address
        );
        log::info!("   Market Data: Subscribe to channels [orderbook, tick, last_price]");

        Ok(server)
    }

    /// 启动定期日志报告
    /// 启动条件单触发扫描 @yutiansut @quantaxis
    ///
    /// `ConditionalOrderEngine::check_triggers`(conditional_order.rs:200)
    /// 此前**从未在生产被调用** —— 全部引用都在该文件 `#[cfg(test)]`(:322) 之后。
    /// 实测:建一个触发条件已满足的条件单(现价 3800 >= 触发价 1900),
    /// 20 秒后 status 仍为 PENDING,且没有产生任何真实订单。
    ///
    /// check_triggers 本身是完整的(查过期 → 判触发 → 经 order_router 下单 →
    /// 置 Triggered 并记 result_order_id),缺的只是「谁来调它」。
    ///
    /// 用独立的周期任务而不是挂在成交/行情热路径上:check_triggers 内部会
    /// 调用 `router.submit_order(...)`,如果从 handle_success_result 里调,
    /// 等于在处理成交的过程中重入下单路径,有死锁与递归风险。
    fn start_conditional_order_scanner(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let matching_engine = self.matching_engine.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            log::info!("✅ Conditional order scanner started (500ms)");

            loop {
                interval.tick().await;

                for instrument_id in matching_engine.get_instruments() {
                    let last_price = match matching_engine.get_last_price(&instrument_id) {
                        Some(p) if p > 0.0 => p,
                        _ => continue,
                    };
                    let triggered = qaexchange::exchange::CONDITIONAL_ORDER_ENGINE
                        .read()
                        .check_triggers(&instrument_id, last_price);
                    if !triggered.is_empty() {
                        log::info!(
                            "Conditional orders triggered for {}: {} order(s) @ {}",
                            instrument_id,
                            triggered.len(),
                            last_price
                        );
                    }
                }
            }
        })
    }

    /// 周期性 WAL checkpoint —— 只对**有冷存储备份**的存储执行 @yutiansut @quantaxis
    ///
    /// `create_checkpoint()`(storage/hybrid/oltp.rs:560)此前**零生产调用**,
    /// WAL 只增不减。安全性已逐项核实:
    ///   ① `force_flush()` 先把 memtable 落成 SSTable;
    ///   ② SSTable 写入走 `file.sync_all()`(sstable/oltp_rkyv.rs:260),**真 fsync**;
    ///   ③ `WalManager::checkpoint`(wal/manager.rs:621)只删
    ///      「后继文件起始序号 ≤ checkpoint 序号」的**完整**文件,
    ///      且循环上界是 `len-1`,**从不删当前正在写的文件**。
    ///
    /// ⚠️ **只对 `market_data_storage` 执行**。实测各目录的冷存储覆盖:
    ///   · market_data  WAL 1.6G / 157 个 parquet  → 有备份,可回收
    ///   · 四个合约目录 WAL ~250M / 29~34 parquet   → 有备份(由 OrderRouter 持有,此处够不到)
    ///   · `gateway_wal` **WAL 3.4G / parquet 0 个** → **没有任何备份**
    ///   · `klines` 57M、`users` 692K              → 同样无备份
    /// 对无备份的 WAL 做 checkpoint 等于**直接删数据**,所以一律不碰。
    ///
    /// 尤其注意 `gateway_wal`:它是全系统最大的 WAL,存的是订单/成交/持仓事件
    /// (`ExchangeOrderRecord`/`ExchangeTradeRecord`/`PositionSnapshot`),
    /// 与合约目录的行情数据(`TickData`/`OrderBookSnapshot`)**完全不重叠**,
    /// 而且**只写不读**(TradeGateway 无 replay/recover,data_query 读的是
    /// market_data_storage 与 kline_wal_manager)。
    /// 即:最重要的审计数据是唯一没有冷存储层的 —— 需要先给它加 OLAP 转换,
    /// 或者明确一个可接受丢失的保留策略,才谈得上回收。
    fn start_periodic_checkpoint(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let storage = self.market_data_storage.clone();

        tokio::spawn(async move {
            // 首次延迟 10 分钟,避开启动期的恢复与预热
            tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));

            loop {
                interval.tick().await;
                let storage = storage.clone();
                // checkpoint 会做磁盘 I/O 且持 memtable 写锁,放到阻塞线程池
                let r = tokio::task::spawn_blocking(move || storage.create_checkpoint()).await;
                match r {
                    // ✨ 报「回收了几个文件」,不报笼统的「完成」。@yutiansut @quantaxis
                    //
                    // 实测:连续 136 次 ✅「完成」,而 `Removed old WAL` 0 次、
                    // `Rotated to new WAL` 0 次 —— WAL 一个字节都没少。
                    // 这不是 bug:`WalManager::checkpoint` 的循环是
                    // `0..files.len().saturating_sub(1)`,而每个 WAL 目录只有 1 个文件
                    // (滚动阈值 1GB,market_data 现 601MB = 63%),唯一那个段正在写入、
                    // 右端开放,无法证明其中没有 >= checkpoint 的记录,因此不能删
                    // (wal/manager.rs 的测试已把该语义锁死)。越过滚动阈值前,
                    // checkpoint 本来就无事可做。
                    //
                    // 危害全在日志:运维看到 136 个 ✅ 会以为 WAL 正在回收。
                    // 无事可做时降到 debug —— 既不撒谎,也不再产生每 10 分钟一条的噪音。
                    Ok(Ok(0)) => log::debug!("market_data WAL checkpoint:无可回收的旧 WAL 段"),
                    Ok(Ok(n)) => {
                        log::info!("✅ market_data WAL checkpoint:回收 {} 个旧 WAL 文件", n)
                    }
                    Ok(Err(e)) => log::error!("❌ market_data WAL checkpoint 失败: {}", e),
                    Err(e) => log::error!("❌ checkpoint 任务 panic: {}", e),
                }
            }
        })
    }

    fn start_periodic_reporting(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let server = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 分钟

            loop {
                interval.tick().await;

                // 生成报告
                let accounts = server.account_mgr.get_all_accounts();
                let total_count = accounts.len();
                let mut active_count = 0;
                let mut total_balance = 0.0;
                let mut total_available = 0.0;
                let mut total_margin = 0.0;

                for account in &accounts {
                    let acc = account.read();
                    if !acc.hold.is_empty() {
                        active_count += 1;
                    }

                    // QA_Account 字段：money（可用资金）
                    total_available += acc.money;

                    // ✨ 用 qars 的权威字段,不要自己重算 @yutiansut @quantaxis
                    //
                    // 原实现犯了本会话的主线错误(两处):
                    //   let long_value = volume_long * lastest_price;   // **漏乘合约乘数**
                    //   margin_used += position_value * 0.15;           // **硬编码 15%**
                    // 实测真实保证金率:IF=10% / IH=5% / IC 乘数还是 200 而非 300,
                    // 这样算出来的「总保证金」偏差 +50% ~ +200%,
                    // 而它每 5 分钟就写进一次日志,是错误数据的稳定来源。
                    //
                    // `QA_Position` 本来就带 `margin_long` / `margin_short`
                    // (由 qars 按 MarketPreset 的真实费率与乘数算出),
                    // `QA_Account` 也有 `get_balance()`。直接取即可。
                    // ⚠️ `get_balance()` 需要 `&mut self`(qars 的签名),这里只有读锁。
                    //    用 QIFI 缓存字段 —— 它由 RiskMonitor 每秒回写
                    //    (risk/risk_monitor.rs 的派生量回写段),对 5 分钟一次的
                    //    报告来说足够新。
                    let account_balance = acc.accounts.balance;
                    let margin_used: f64 = acc
                        .hold
                        .values()
                        .map(|p| p.margin_long + p.margin_short)
                        .sum();

                    total_balance += account_balance;
                    total_margin += margin_used;
                }

                log::info!("━━━━━━━━━━━━━━━━━━━ Periodic Report ━━━━━━━━━━━━━━━━━━━");
                log::info!(
                    "📊 Accounts: {} total, {} active",
                    total_count,
                    active_count
                );
                log::info!(
                    "💰 Balance: ¥{:.2} total, ¥{:.2} available, ¥{:.2} margin",
                    total_balance,
                    total_available,
                    total_margin
                );
                if total_balance > 0.0 {
                    log::info!(
                        "📈 Margin Utilization: {:.1}%",
                        (total_margin / total_balance) * 100.0
                    );
                }
                log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
        })
    }

    /// 启动QIFI快照定期保存
    fn start_snapshot_scheduler(&self) -> tokio::task::JoinHandle<()> {
        let account_mgr = self.account_mgr.clone();
        let snapshot_dir = format!("{}/snapshots", self.config.storage_path);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // 每60秒保存一次
            // 一轮落盘超过 60s 时,下一轮从本轮结束起再等 60s,而不是立刻背靠背补跑
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;

                // ⚠️ 必须 spawn_blocking —— 不能在这里直接同步调用 @yutiansut @quantaxis
                //
                // `save_snapshots` 是同步的:逐账户序列化整份 QIFI + 写 tmp + fsync + rename。
                // 本任务跑在 `#[actix_web::main]` 的**单线程**运行时上,与 500ms 盘口快照广播
                // (`start_snapshot_broadcaster`)、条件单扫描器、周期报告共用这一个线程。
                // 2026-11-08 实测:快照目录涨到 2.0GB(每轮重写 ~112 个账户,单个 60~92MB),
                // 一轮约 60 秒 → interval 每次都已到期、任务从不让出 → 同线程任务全部饿死:
                //   · 主线程 60 秒采样:D 态(磁盘等待)66%、R 19%、空闲(ep_poll)仅 15%;
                //   · 旧 `/ws` 原样抓广播器 10 秒:order_book_update 541 条、
                //     order_book_snapshot **0 条** —— DIFF 客户端从此收不到任何盘口档位。
                // 同文件 `start_periodic_checkpoint` 早就是 spawn_blocking 的正确写法,这里照搬。
                let mgr = account_mgr.clone();
                let dir = snapshot_dir.clone();
                match tokio::task::spawn_blocking(move || mgr.save_snapshots(&dir)).await {
                    Ok(Ok(count)) if count > 0 => {
                        log::debug!("Saved {} account snapshots", count);
                    }
                    Ok(Ok(_)) => {} // 没有账户，不记录日志
                    Ok(Err(e)) => {
                        log::error!("Failed to save snapshots: {}", e);
                    }
                    Err(e) => {
                        log::error!("Snapshot save task panicked/cancelled: {}", e);
                    }
                }
            }
        })
    }

    /// 每自然日日终结算定时任务(用户拍板)@yutiansut @quantaxis
    ///
    /// 北京时间 `[settlement] time`(缺省 00:00)触发 `SettlementEngine::run_end_of_day`,
    /// 结算的是触发时刻之前的那个自然日。`run_end_of_day` 是阻塞调用(撤单 + 两轮快照落盘
    /// + 并行结算),放 spawn_blocking —— 本任务所在的是 actix 单线程运行时(见快照任务的说明)。
    /// 进程在触发时刻宕机则这一天不补结算:第二天的日终会把两天一起结掉(结算只依赖当前状态)。
    fn start_daily_settlement_scheduler(&self) -> Option<tokio::task::JoinHandle<()>> {
        let cfg = self.config.settlement.clone();
        if !cfg.auto_daily {
            log::info!("日终结算定时任务未启用([settlement] auto_daily = false);手动 POST /api/admin/settlement/execute");
            return None;
        }
        let at = match chrono::NaiveTime::parse_from_str(&cfg.time, "%H:%M") {
            Ok(t) => t,
            Err(e) => {
                log::error!("[settlement] time = {:?} 不是 HH:MM({}),日终定时任务未启动", cfg.time, e);
                return None;
            }
        };
        let engine = self.settlement_engine.clone();
        Some(tokio::spawn(async move {
            let bj = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 offset");
            loop {
                let now = chrono::Utc::now().with_timezone(&bj);
                let mut fire = now.date_naive().and_time(at);
                if fire <= now.naive_local() {
                    fire += chrono::Duration::days(1);
                }
                // 结算的是触发时刻之前的那个自然日:00:00 → 前一天;15:30 → 当天
                let trading_day = (fire - chrono::Duration::seconds(1))
                    .date()
                    .format("%Y-%m-%d")
                    .to_string();
                let wait = (fire - now.naive_local()).to_std().unwrap_or_default();
                log::info!(
                    "📅 下次日终结算:北京时间 {} 结算 {}(约 {:.1} 小时后)",
                    fire.format("%Y-%m-%d %H:%M"),
                    trading_day,
                    wait.as_secs_f64() / 3600.0
                );
                tokio::time::sleep(wait).await;

                let e = engine.clone();
                let day = trading_day.clone();
                match tokio::task::spawn_blocking(move || e.run_end_of_day(&day)).await {
                    Ok(Ok(_)) => {} // run_end_of_day 自己打完成日志
                    Ok(Err(err)) => log::error!("[EOD] {} 日终结算失败: {}", trading_day, err),
                    Err(err) => log::error!("[EOD] {} 日终任务异常退出: {}", trading_day, err),
                }
                // 同一分钟内不重复触发
                tokio::time::sleep(std::time::Duration::from_secs(61)).await;
            }
        }))
    }

    /// 从快照恢复账户
    fn recover_from_snapshots(&self) {
        let snapshot_dir = format!("{}/snapshots", self.config.storage_path);

        match self.account_mgr.restore_from_snapshots(&snapshot_dir) {
            Ok(count) if count > 0 => {
                log::info!("✅ Recovered {} accounts from snapshots", count);
            }
            Ok(_) => {
                log::info!("No existing snapshots found (first time startup)");
            }
            Err(e) => {
                log::error!("Failed to restore snapshots: {}", e);
            }
        }
    }

    /// 从WAL恢复账户 (方案B)
    fn recover_from_wal(&self) {
        use qaexchange::storage::recovery::RecoveryManager;

        // 传存储根目录，由 RecoveryManager 自己拼 `__ACCOUNT__/wal`
        // —— 与 OltpHybridStorage::create 的写出路径保持一致 @yutiansut @quantaxis
        let recovery_mgr = RecoveryManager::new(self.config.storage_path.clone());

        match recovery_mgr.recover(&self.account_mgr) {
            Ok(count) if count > 0 => {
                log::info!("✅ [WAL Recovery] Recovered {} accounts from WAL", count);
            }
            Ok(_) => {
                log::debug!(
                    "[WAL Recovery] No WAL records found (first time startup or after snapshot)"
                );
            }
            Err(e) => {
                log::error!("[WAL Recovery] Failed to recover from WAL: {}", e);
            }
        }
    }

    /// 从WAL恢复用户数据
    fn recover_from_user_wal(&self) {
        use qaexchange::user::recovery::UserRecovery;

        let user_recovery = UserRecovery::new(self.user_storage.clone(), self.user_mgr.clone());

        match user_recovery.recover_all_users() {
            Ok(stats) if stats.users_recovered > 0 => {
                log::info!(
                    "✅ [User Recovery] Recovered {} users ({} registrations, {} bindings) in {}ms",
                    stats.users_recovered,
                    stats.user_register_records,
                    stats.account_bind_records,
                    stats.recovery_time_ms
                );
            }
            Ok(_) => {
                log::debug!("[User Recovery] No user records found (first time startup)");
            }
            Err(e) => {
                log::error!("[User Recovery] Failed to recover users from WAL: {}", e);
            }
        }
    }

    /// 运行服务器
    async fn run(mut self) -> io::Result<()> {
        // 1. 初始化合约
        self.init_instruments();

        // 1.5. 启动快照生成器
        self.start_snapshot_generator();

        // 2. 从WAL恢复用户数据（必须在账户恢复之前，因为账户需要绑定到用户）
        self.recover_from_user_wal();

        // 3. 从快照恢复账户 (方案A)
        self.recover_from_snapshots();

        // 3.5. 从WAL恢复账户 (方案B - 补充快照遗漏的数据)
        self.recover_from_wal();

        // 3.6. 从账户的 dailyorders 恢复订单索引到 order_router @yutiansut @quantaxis
        // 客户端编号映射文件:恢复挂单(按快照里的交易所单号)之后再读回 @yutiansut @quantaxis
        self.order_router
            .set_client_order_id_file(format!("{}/client_order_ids.jsonl", self.config.storage_path));
        self.order_router.restore_orders_from_accounts();
        let n = self.order_router.load_client_order_ids();
        if n > 0 {
            log::info!("✅ 读回客户端编号映射 {} 条", n);
        }

        // 3.7. ✨ 恢复总结 —— 一行看清"这次启动到底恢复了什么"。@yutiansut @quantaxis
        //
        // 为什么需要:上面四步各自的日志散落在几百行启动输出里,
        // 且 `recover_from_wal` 的"无数据"分支是 `log::debug!`(生产级别看不到)。
        // 运维在部署后最需要回答的问题是「数据都回来了吗」,
        // 而这个问题此前要翻日志、数 `Restored account` 的行数才能答。
        //
        // 本会话的部署评估里,「快照恢复是否可用」正是关键一环
        // (见 tasks/DECISIONS-PENDING.md 的重启安全性五项实测)。
        {
            let accounts = self.account_mgr.get_all_accounts();
            let n_acc = accounts.len();
            let (mut n_ord, mut n_trd, mut n_pos, mut total_bal) = (0usize, 0usize, 0usize, 0f64);
            for a in &accounts {
                let acc = a.read();
                n_ord += acc.dailyorders.len();
                n_trd += acc.dailytrades.len();
                n_pos += acc
                    .hold
                    .values()
                    .filter(|p| p.volume_long_unmut() != 0.0 || p.volume_short_unmut() != 0.0)
                    .count();
                total_bal += acc.accounts.balance;
            }
            log::info!(
                "📦 [恢复总结] 账户 {} 个 | 订单 {} 条 | 成交 {} 笔 | 有量持仓 {} 个 | 权益合计 ¥{:.2}",
                n_acc, n_ord, n_trd, n_pos, total_bal
            );
        }

        // 4. 启动存储订阅器
        let _storage_handle = self.start_storage_subscriber();

        // 4. 启动 OLAP 转换系统
        self.start_olap_conversion();

        // 4.5 启动盘中风控监控 @yutiansut @quantaxis
        //
        // RiskMonitor::start_monitoring 此前**从未在生产被调用** ——
        // 全部引用都在 risk_monitor.rs 的 #[cfg(test)] 模块内(该模块从 :678 开始,
        // 引用全在 784 之后)。实测:1 小时压测 92,023 笔订单,
        // `grep -c RiskMonitor` 全日志 = 0,盘中风控一次都没跑过。
        //
        // do_risk_check(:305) 负责:风险等级升级预警 / 可用资金为负告警 /
        // 临近强平线(95%~100%)告警 / 触发强平。默认 1 秒一轮(:169)。
        //
        // ⚠️ 只告警,不会自动强平:强平走 liquidation_callback(:221)。
        // 2026-09-05 复核 `grep -rn set_liquidation_callback src/` = 3 处:
        //   risk_monitor.rs:295 定义 / :1001 测试 / :1011 测试内调用
        // —— **生产零调用**(测试里有,原注释「没有任何调用点」措辞不准),
        // 回调恒为 None。要启用自动强平需另行接回调 —— 那是独立决定。
        self.risk_monitor.start_monitoring();
        log::info!("✅ Intra-day risk monitor started (alerts only; auto-liquidation callback not wired)");

        // 5. 将 server 包装到 Arc 以便在异步任务中共享
        let server = Arc::new(self);

        // 5. 启动QIFI快照定期保存 (方案A)
        let _snapshot_handle = server.start_snapshot_scheduler();

        // 5.1 每自然日日终结算(`[settlement]`,缺省北京 00:00)@yutiansut @quantaxis
        let _eod_handle = server.start_daily_settlement_scheduler();

        // 6. 启动快照广播服务
        server.start_snapshot_broadcaster();

        // 6.5 启动条件单触发扫描 @yutiansut @quantaxis
        let _cond_handle = server.start_conditional_order_scanner();

        // 7. 启动定期报告
        let _report_handle = server.start_periodic_reporting();
        let _checkpoint_handle = server.start_periodic_checkpoint();
        log::info!("✅ WAL checkpoint 任务已启动(market_data,10 分钟一次)");

        // 6. 启动 HTTP 服务器
        let http_server = server.clone().start_http_server().await?;

        // 7. 启动 WebSocket 服务器
        let ws_server = server.clone().start_websocket_server().await?;

        // 8. 打印启动信息
        print_startup_banner(&server.config);

        // 8. 自己接管 SIGTERM/SIGINT @yutiansut @quantaxis
        //
        // 两个 server 都 `.disable_signals()` 了,所以必须自己收信号并调
        // `handle.stop(true)`(true = 优雅:等在途请求处理完)。这样 `try_join!`
        // 才会**正常返回**,下面的最终落盘才有机会执行。
        {
            let http_handle = http_server.handle();
            let ws_handle = ws_server.handle();
            tokio::spawn(async move {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut term = match signal(SignalKind::terminate()) {
                        Ok(x) => x,
                        Err(e) => { log::error!("SIGTERM 注册失败: {}", e); return; }
                    };
                    let mut intr = match signal(SignalKind::interrupt()) {
                        Ok(x) => x,
                        Err(e) => { log::error!("SIGINT 注册失败: {}", e); return; }
                    };
                    tokio::select! {
                        _ = term.recv() => log::info!("收到 SIGTERM,开始优雅停机"),
                        _ = intr.recv() => log::info!("收到 SIGINT,开始优雅停机"),
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = tokio::signal::ctrl_c().await;
                    log::info!("收到 Ctrl-C,开始优雅停机");
                }
                http_handle.stop(true).await;
                ws_handle.stop(true).await;
            });
        }

        // 9. 等待服务器
        tokio::try_join!(async { http_server.await }, async { ws_server.await })?;

        // 9. 停服前补一次落盘 —— 消除固有的 60 秒丢失窗口 @yutiansut @quantaxis
        //
        // 此前**没有任何 shutdown 钩子**:全仓 `ctrl_c` / `signal::unix` / `SignalKind`
        // 零命中。actix 收到 SIGTERM 会优雅关闭 HTTP 连接并让上面的 try_join! 返回,
        // 但快照调度器(`start_snapshot_scheduler`,60 秒一次)只是随进程被丢弃,
        // **不会补写最后一次**。于是每次停服都必然丢失「距上次落盘以来」的状态,
        // 最坏 60 秒 —— 包括计划内的每一次部署。
        //
        // 放在这里而不是注册信号处理器:try_join! 返回就是「两个 server 都已停止」,
        // 此时没有新请求进来,内存态是静止的,是落盘的最佳时刻;
        // 而自己注册 SIGTERM 会与 actix 自带的处理竞争。
        //
        // 仍**不能**替代 kill -9 的防护:SIGKILL 不给任何执行机会。
        // (2026-09-05 事故正是 kill -9 撞上当时非原子的写入,毁掉 172,035 条订单。
        //  `save_snapshots` 现已是 tmp+fsync+rename 原子写。)
        // 用 `server`(:1346 的 `Arc::new(self)`)而非 `self` —— self 已被 move 进 Arc
        // ✨ 先把 WAL 组提交缓冲刷完(async 模式下可能还有最后一个刷盘间隔的记录)@yutiansut @quantaxis
        match qaexchange::storage::wal::flush_all_pending(std::time::Duration::from_secs(30)) {
            Some(n) => log::info!("✅ WAL 缓冲已刷盘: {} 条", n),
            None => log::error!("❌ WAL 缓冲刷盘超时(30s)—— 最后一个刷盘间隔的记录可能未落盘"),
        }
        let snapshot_dir = format!("{}/snapshots", server.config.storage_path);
        log::info!("🛑 服务已停止,执行最后一次快照落盘...");
        match server.account_mgr.save_snapshots(&snapshot_dir) {
            Ok(n) => log::info!("✅ 最终快照已保存: {} 个账户", n),
            Err(e) => log::error!("❌ 最终快照保存失败: {} —— 本次停服可能丢失最多 60 秒状态", e),
        }

        Ok(())
    }
}

/// 打印启动横幅
fn print_startup_banner(config: &ExchangeConfig) {
    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║                    🚀 QAExchange Server Started                       ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    println!("📡 Service Endpoints:");
    println!("   • HTTP API:    http://{}", config.http_address);
    println!("   • WebSocket:   ws://{}/ws", config.ws_address);
    println!("   • Health:      http://{}/health", config.http_address);

    println!("\n💾 Storage:");
    if config.enable_storage {
        println!("   • Status:      Enabled ✓");
        println!("   • Path:        {}", config.storage_path);
        println!("   • Mode:        Async batch write (100 records / 10ms)");
    } else {
        println!("   • Status:      Disabled");
    }

    println!("\n📋 Available APIs:");
    println!("\n   HTTP REST API:");
    println!("   ┌─────────────────────────────────────────────────────────────────┐");
    println!("   │ POST   /api/account/open          - 开户                        │");
    println!("   │ GET    /api/account/:user_id      - 查询账户                    │");
    println!("   │ POST   /api/order/submit          - 提交订单                    │");
    println!("   │ POST   /api/order/cancel          - 撤单                        │");
    println!("   │ GET    /api/order/:order_id       - 查询订单                    │");
    println!("   │ GET    /api/order/user/:user_id   - 查询用户订单                │");
    println!("   │ GET    /api/position/:user_id     - 查询持仓                    │");
    println!("   └─────────────────────────────────────────────────────────────────┘");

    println!("\n   Monitoring API:");
    println!("   ┌─────────────────────────────────────────────────────────────────┐");
    println!("   │ GET    /api/monitoring/system     - 系统监控（全部统计）        │");
    println!("   │ GET    /api/monitoring/accounts   - 账户统计                    │");
    println!("   │ GET    /api/monitoring/orders     - 订单统计                    │");
    println!("   │ GET    /api/monitoring/trades     - 成交统计                    │");
    println!("   │ GET    /api/monitoring/storage    - 存储统计                    │");
    println!("   │ GET    /api/monitoring/report     - 生成文本报告                │");
    println!("   └─────────────────────────────────────────────────────────────────┘");

    println!("\n   WebSocket API:");
    println!("   ┌─────────────────────────────────────────────────────────────────┐");
    println!("   │ auth             - 认证                                         │");
    println!("   │ subscribe        - 订阅行情                                     │");
    println!("   │ submit_order     - 提交订单                                     │");
    println!("   │ cancel_order     - 撤单                                         │");
    println!("   │ query_account    - 查询账户                                     │");
    println!("   │ ping             - 心跳                                         │");
    println!("   └─────────────────────────────────────────────────────────────────┘");

    println!("\n📊 Trading Instruments:");
    println!("   • IF2501 - 沪深300股指期货2501 @ 3800.0");
    println!("   • IF2502 - 沪深300股指期货2502 @ 3820.0");
    println!("   • IC2501 - 中证500股指期货2501 @ 5600.0");
    println!("   • IH2501 - 上证50股指期货2501  @ 2800.0");

    println!("\n💡 Quick Start:");
    println!(
        "   1. 开户:     curl -X POST http://{}/api/account/open \\",
        config.http_address
    );
    println!("                  -H 'Content-Type: application/json' \\");
    println!("                  -d '{{\"user_id\":\"demo\",\"user_name\":\"Demo User\",\"init_cash\":1000000,\"account_type\":\"individual\",\"password\":\"demo123\"}}'");
    println!(
        "\n   2. 提交订单: curl -X POST http://{}/api/order/submit \\",
        config.http_address
    );
    println!("                  -H 'Content-Type: application/json' \\");
    println!("                  -d '{{\"user_id\":\"demo\",\"instrument_id\":\"IF2501\",\"direction\":\"BUY\",\"offset\":\"OPEN\",\"volume\":1,\"price\":3800,\"order_type\":\"LIMIT\"}}'");
    println!(
        "\n   3. 查询账户: curl http://{}/api/account/demo",
        config.http_address
    );

    println!("\n🔗 Documentation:");
    println!("   • Architecture:  docs/DECOUPLED_STORAGE_ARCHITECTURE.md");
    println!("   • Performance:   docs/PERFORMANCE.md");

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🟢 Server is running. Press Ctrl+C to stop.\n");
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // ✨ 给 rayon 全局线程池设上限 —— 必须在任何并行操作**之前** @yutiansut @quantaxis
    //
    // rayon 的全局池默认 = 逻辑核数(本机 96),**首次用到时懒创建、此后永不销毁**。
    // 实测:全新实例 24 个 actix-rt 线程稳定 119 秒不变;一调
    // `/api/management/transactions`(该 handler 用了 `par_iter`)立刻跳到 **120**
    // (24 + 96),20 秒后仍是 120;RSS 同步从 2,350 MB 涨到 3,051 MB(约 7.3 MB/线程)。
    //
    // ⚠️ 这些线程**很难被发现**:rayon 用无名 `thread::Builder`,而 Linux 对无名线程
    // 的 `comm` 继承父线程 —— 父线程是 actix worker,所以 96 个 rayon 线程全都顶着
    // `actix-rt|system` 的名字,和真正的 worker 混在一起。我一度以为是
    // `.workers()` 上限失效了。
    //
    // 全仓 rayon 使用点:qaexchange 19 处 + qars account.rs 7 处
    // (`hold.par_iter_mut()` 等)。这里的并行度需求很小(43 个账户的聚合、
    // 持仓结算),8 个线程绰绰有余。可用 `QAEX_RAYON_THREADS` 覆盖。
    {
        let n = std::env::var("QAEX_RAYON_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(8);
        match rayon::ThreadPoolBuilder::new().num_threads(n).build_global() {
            Ok(()) => log::info!("QAEX_RAYON_THREADS = {} (rayon 全局池上限)", n),
            // 只可能因为已经初始化过而失败;不致命
            Err(e) => log::warn!("rayon 全局池已被初始化,跳过设置: {}", e),
        }
    }

    // 1. 加载配置文件
    let toml_config = match TomlConfig::load_default() {
        Ok(cfg) => cfg,
        Err(e) => {
            log::warn!("Failed to load config file: {}, using defaults", e);
            // 手动构建默认配置
            TomlConfig {
                server: qaexchange::utils::config::ServerConfig {
                    name: "QAExchange".to_string(),
                    environment: "development".to_string(),
                    log_level: "info".to_string(),
                },
                http: qaexchange::utils::config::HttpConfig {
                    host: "127.0.0.1".to_string(),
                    port: 8080,
                },
                websocket: qaexchange::utils::config::WebSocketConfig {
                    host: "127.0.0.1".to_string(),
                    port: 8081,
                },
                storage: qaexchange::utils::config::StorageConfig {
                    enabled: true,
                    base_path: "/tmp/qaexchange/storage".to_string(),
                    subscriber: qaexchange::utils::config::SubscriberConfig {
                        batch_size: 100,
                        batch_timeout_ms: 10,
                        buffer_size: 10000,
                    },
                    wal: Default::default(), // 无配置文件时保持同步落盘
                },
                instruments: vec![],
                settlement: Default::default(), // 每自然日北京 00:00 日终
            }
        }
    };

    // 1.1 加载性能优化配置
    let perf_config = match qaexchange::utils::config::PerformanceConfig::load_default() {
        Ok(cfg) => {
            log::info!("✅ Performance config loaded from config/performance.toml");
            cfg
        }
        Err(e) => {
            log::warn!("Failed to load performance config: {}, using defaults", e);
            qaexchange::utils::config::PerformanceConfig::default()
        }
    };

    log::info!("Configuration loaded");
    log::info!("  Storage path: {}", toml_config.storage.base_path);
    log::info!("  Storage enabled: {}", toml_config.storage.enabled);

    // ✨ WAL 落盘模式 —— 必须在任何 WalManager 打开之前设置 @yutiansut @quantaxis
    // `[storage.wal] sync_mode` 此前是死配置(见 storage/wal/manager.rs 顶部说明)。
    {
        let wal = &toml_config.storage.wal;
        if !wal.is_async() && !wal.sync_mode.eq_ignore_ascii_case("sync") {
            log::warn!("⚠️ [storage.wal] sync_mode = {:?} 不认识,按 \"sync\" 处理", wal.sync_mode);
        }
        qaexchange::storage::wal::configure_wal_sync_mode(wal.is_async(), wal.sync_interval_ms);
        if wal.is_async() {
            log::info!(
                "  WAL: async 组提交,后台每 {}ms 刷盘(崩溃最多丢该时间窗内的记录)",
                wal.sync_interval_ms
            );
        } else {
            log::info!("  WAL: sync(每条记录 fsync)");
        }
    }

    // 2. 转换为运行时配置
    let mut config = ExchangeConfig::from_toml(toml_config);

    // ✨ 按合约覆盖 qars 费率 / 保证金(须早于账户恢复:恢复时据此改写持仓副本)@yutiansut @quantaxis
    let n_override = qaexchange::core::account_ext::set_preset_overrides(&config.instruments);
    if n_override > 0 {
        for c in config.instruments.iter().filter(|c| {
            c.commission_per_lot.is_some() || c.commission_rate.is_some() || c.margin_rate.is_some()
        }) {
            log::info!(
                "  费率覆盖 {}: 每手 {:?} / 费率 {:?} / 保证金 {:?}",
                c.instrument_id, c.commission_per_lot, c.commission_rate, c.margin_rate
            );
        }
    }

    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        match args[i].as_str() {
            "--http" | "-h" => {
                if i + 1 < args.len() {
                    config.http_address = args[i + 1].clone();
                }
            }
            "--ws" | "-w" => {
                if i + 1 < args.len() {
                    config.ws_address = args[i + 1].clone();
                }
            }
            "--storage" | "-s" => {
                if i + 1 < args.len() {
                    config.storage_path = args[i + 1].clone();
                }
            }
            "--no-storage" => {
                config.enable_storage = false;
            }
            _ => {}
        }
    }

    // 创建并运行服务器
    let server = ExchangeServer::new(config, perf_config);
    server.run().await
}

/// ✨ 交易日历剩余可用天数校验 —— 把一颗静默的定时炸弹变成启动告警。
/// @yutiansut @quantaxis
///
/// qars `QATradeDate` 的交易日表是**硬编码 Vec<i32>**,实测覆盖 `20000104 ~ 20261231`。
/// 而 `get_next_day` 有两处 `.unwrap()`,越界即 **panic**:
///   · `trade_date.iter().position(|x| x >= &ur).unwrap()`   → 日期超出表尾
///   · `trade_date.get_mut(u + 1).unwrap()`                  → 日期正好是表中最后一天
///
/// 危害面很大:`get_qifi_tradeday()` 被 `get_qifi_slice()` 调用,而后者是
/// **每次 QIFI 投影的必经之路** —— 60 秒一次的快照写出、账户详情接口、
/// 成交查询接口、WebSocket 推送全都走它。表一到头,这些路径同时开始 panic。
///
/// 实测(2026-09-06,用真实表模拟并以线上观测值做正对照:
/// `2026-09-06 → 20260907`,与 79 份快照一致):
///   `2026-12-31 09:00` → 20261231 ✅   `2026-12-31 20:00` → 💥
///   `2027-01-04`(任意时刻)          → 💥
///
/// 本函数不修根因(要改 qars2 的表或让 `get_next_day` 不 panic,属共享库决策),
/// 只在启动时**探测还剩多少天**并按紧迫程度分级告警。
///
/// ⚠️ 探测**绝不能靠触发 panic**(2026-11-07 修)@yutiansut @quantaxis
///
/// 原实现二分查找「调用 `get_trade_day` 不 panic 的最后一天」,用 `catch_unwind`
/// 包住探测、并临时把 panic hook 换成空函数。但 release / verify profile 是
/// **`panic = "abort"`**(Cargo.toml)—— `catch_unwind` 在 abort 模式下**什么也接不住**,
/// 第一个越界探测(今天 + 900 天)直接 `abort()`。又因为 hook 被换成了空函数,
/// **进程连一行 panic 信息都不留**就以 SIGABRT 退出(apport 抓到 core 才定位到
/// `tradedate.rs:1166 get_next_day` ← 本函数)。
/// `cargo test` 走 unwind,单测照样通过 —— 所以只有起 release 二进制才会暴露。
///
/// 改为只用**不会 panic** 的 `if_trade_date`(纯 `Vec::contains`)向前扫,
/// 找出表里最后一个交易日。与原二分**结果完全等价**:对 `YYYY-MM-DD` 形式的输入,
/// `get_trade_day(d)` panic 当且仅当 `d` 晚于表中最后一个交易日
/// (`get_next_day` 的 `position(|x| x >= d).unwrap()` 落空)。
///
/// 探测交易日历还能处理多少天。返回 `None` 表示**今天就已越界**。
/// 抽成返回值函数是为了能单测 —— 只打日志的检查无法证明它算得对。
fn trading_calendar_days_left() -> Option<i64> {
    use chrono::{Duration, Local};
    let today = Local::now().date_naive();
    const HORIZON: i64 = 900; // 探测上界:约 2.5 年,够覆盖任何合理的表
    // 表若延伸到上界之后,上界后半个月内必有交易日(最长假期 ~10 天)
    const TAIL: i64 = 15;
    let mut cal = qars::qautil::tradedate::QATradeDate::new();
    let mut last: Option<i64> = None;
    for k in 0..=HORIZON + TAIL {
        let s = (today + Duration::days(k)).format("%Y-%m-%d").to_string();
        if cal.if_trade_date(&s) {
            last = Some(k);
        }
    }
    // 与原二分语义一致:上界那天仍可处理 ⇒ 报 HORIZON(「余量充足」)
    last.map(|k| k.min(HORIZON))
}

/// 上面那个探测结果的分级告警。
fn check_trading_calendar_horizon() {
    use chrono::{Duration, Local};
    let today = Local::now().date_naive();
    let left = trading_calendar_days_left();
    let lo = left.unwrap_or(0);
    if left.is_none() {
        log::error!(
            "❌ 交易日历已越界:qars QATradeDate 处理不了今天({})—— \
             `get_qifi_slice()` 路径(快照写出/账户详情/成交查询/WS 推送)会 panic。\
             需扩表或让 `get_next_day` 不 panic(改 qars2)。",
            today
        );
        return;
    }
    let last_ok = today + Duration::days(lo);
    if lo >= 900 {
        log::info!("✅ 交易日历余量充足(探测上界 900 天内均可用)");
    } else if lo <= 30 {
        log::error!(
            "❌ 交易日历只剩 {} 天(最后可用日 {})—— 到期后 `get_qifi_slice()` 会 panic,\
             快照写出/账户详情/成交查询/WS 推送同时失效。请扩 qars2 的 trade_date 表。",
            lo, last_ok
        );
    } else {
        log::warn!(
            "⚠️ 交易日历剩余 {} 天(最后可用日 {})—— qars `QATradeDate` 的表是硬编码的,\
             越界后 `get_next_day` 的 `.unwrap()` 会 panic,而 `get_qifi_tradeday()` \
             在每次 QIFI 投影时都会调用它。请在到期前扩表。",
            lo, last_ok
        );
    }
}

#[cfg(test)]
mod trading_calendar_tests {
    use super::*;

    /// 日历必须至少还能用「今天」—— 否则 `get_qifi_slice()` 全线 panic。
    #[test]
    fn calendar_covers_today() {
        assert!(
            trading_calendar_days_left().is_some(),
            "qars 交易日历已无法处理今天,QIFI 投影路径会 panic"
        );
    }

    /// ✨ 正对照:探测器必须真的能**测出边界**,而不是恒返回上界。
    /// qars 的表实测覆盖到 20261231,所以剩余天数必须 < 900(探测上界)。
    /// 若哪天扩了表,这条会失败 —— 那是**好事**,提醒把上界调大。
    #[test]
    fn horizon_is_finite_and_detected() {
        let left = trading_calendar_days_left().expect("今天就越界了");
        // 打出来:`cargo test -- --nocapture` 能直接看到还剩多少天
        eprintln!("  交易日历剩余可用天数 = {}", left);
        assert!(left >= 0, "剩余天数不能为负: {}", left);
        assert!(
            left < 900,
            "探测到剩余 {} 天 = 探测上界,说明没测出真实边界(或表已扩过,请调大上界)",
            left
        );
    }

    /// 探测器不能把 panic 泄漏出来 —— 它自己就是用来检测 panic 的。
    #[test]
    fn probe_does_not_propagate_panic() {
        // 连调两次:第二次能正常返回,说明第一次的 panic hook 已还原
        let a = trading_calendar_days_left();
        let b = trading_calendar_days_left();
        assert_eq!(a, b, "两次探测结果应一致(panic hook 未被破坏)");
    }

    /// ✨ 回归(2026-11-07):探测**全程不得触发任何 panic**。@yutiansut @quantaxis
    ///
    /// release 是 `panic = "abort"`,探测里只要 panic 一次进程就 SIGABRT ——
    /// 而测试走 unwind,旧实现(catch_unwind 二分)在这里照样「通过」。
    /// 所以这条用 panic hook 计数,把「探测期间发生过 panic」本身判为失败。
    #[test]
    fn probe_never_panics_even_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        // hook 是进程全局的,测试并行跑 —— 只数**本线程**的 panic,
        // 否则别的测试(含下面那条故意 panic 的正对照)会让这条假失败
        let me = std::thread::current().id();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            if std::thread::current().id() == me { h.fetch_add(1, Ordering::SeqCst); }
        }));
        let _ = trading_calendar_days_left();
        std::panic::set_hook(prev);
        assert_eq!(hits.load(Ordering::SeqCst), 0,
                   "探测期间发生了 panic —— 在 panic=abort 的 release 里这等于进程崩溃");
    }

    /// ✨ 结果必须与「get_trade_day 会不会 panic」的边界一致:
    /// 今天+lo 可处理、今天+lo+1 起在表外。用 unwind 模式下的真实调用做正对照。
    #[test]
    fn horizon_matches_real_panic_boundary() {
        use chrono::{Duration, Local};
        let lo = match trading_calendar_days_left() { Some(v) => v, None => return };
        if lo >= 900 { return; }
        let today = Local::now().date_naive();
        let call = |k: i64| {
            let s = (today + Duration::days(k)).format("%Y-%m-%d").to_string();
            std::panic::catch_unwind(|| {
                qars::qautil::tradedate::QATradeDate::new().get_trade_day(s)
            }).is_ok()
        };
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let (ok_at_lo, ok_after) = (call(lo), call(lo + 1));
        std::panic::set_hook(prev);
        assert!(ok_at_lo, "第 {} 天应可处理", lo);
        assert!(!ok_after, "第 {} 天应已越界", lo + 1);
    }
}
