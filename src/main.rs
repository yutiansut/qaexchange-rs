//! QAExchange 完整交易所服务
//!
//! 集成功能：
//! 1. 交易所核心引擎（撮合、风控、账户管理）
//! 2. HTTP API（REST 接口）
//! 3. WebSocket API（实时推送）
//! 4. 解耦存储层（异步持久化）
//!
//! 运行: cargo run --bin qaexchange-server

use qaexchange::storage::subscriber::{StorageSubscriber, StorageSubscriberConfig};
use qaexchange::storage::hybrid::oltp::OltpHybridConfig;
use qaexchange::storage::conversion::{ConversionManager, SchedulerConfig, WorkerConfig};
use qaexchange::exchange::{AccountManager, InstrumentRegistry, TradeGateway, OrderRouter, SettlementEngine, CapitalManager};
use qaexchange::user::UserManager;
use qaexchange::exchange::instrument_registry::{InstrumentInfo, InstrumentType, InstrumentStatus};
use qaexchange::matching::engine::ExchangeMatchingEngine;
use qaexchange::notification::broker::NotificationBroker;
use qaexchange::market::{MarketDataBroadcaster, SnapshotBroadcastService};
// use qaexchange::service::http::HttpServer;  // 未使用
use qaexchange::service::http::admin::AdminAppState;
use qaexchange::service::http::management::ManagementAppState;
use qaexchange::service::websocket::WebSocketServer;
use qaexchange::risk::RiskMonitor;
use qaexchange::utils::config::ExchangeConfig as TomlConfig;
use actix_web::{App, HttpServer as ActixHttpServer, middleware, web};
use std::sync::Arc;
use std::io;
use std::path::PathBuf;
use chrono;

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
}

impl ExchangeConfig {
    /// 从 TOML 配置文件加载
    fn from_toml(toml_config: TomlConfig) -> Self {
        Self {
            http_address: toml_config.http.bind_address(),
            ws_address: toml_config.websocket.bind_address(),
            storage_path: toml_config.storage.base_path,
            enable_storage: toml_config.storage.enabled,
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
    storage_stats: Option<Arc<parking_lot::Mutex<qaexchange::storage::subscriber::SubscriberStats>>>,

    /// OLAP 转换管理器
    conversion_mgr: Option<Arc<parking_lot::Mutex<ConversionManager>>>,

    /// iceoryx2 管理器（零拷贝 IPC）
    iceoryx_manager: Option<Arc<parking_lot::RwLock<qaexchange::ipc::IceoryxManager>>>,

    /// 快照生成器线程句柄
    snapshot_generator_handle: Option<std::thread::JoinHandle<()>>,
}

impl ExchangeServer {
    /// 创建交易所服务
    fn new(config: ExchangeConfig, perf_config: qaexchange::utils::config::PerformanceConfig) -> Self {
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
                    memtable_size_bytes: 16 * 1024 * 1024,  // 16MB（用户数据量小）
                    estimated_entry_size: 512,
                },
            ).expect("Failed to create user storage")
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
        let mut trade_gateway_inner = TradeGateway::new(account_mgr.clone());
        trade_gateway_inner.set_notification_broker(notification_broker.clone());

        // 从 matching_engine 获取 trade_recorder 并设置到 trade_gateway
        let trade_recorder = matching_engine.get_trade_recorder();
        trade_gateway_inner = trade_gateway_inner.set_trade_recorder(trade_recorder.clone());

        // 先创建 trade_gateway Arc（后续会设置 market_data_service）
        let trade_gateway = Arc::new(trade_gateway_inner);

        let market_broadcaster = Arc::new(MarketDataBroadcaster::new());

        // 1.4 创建 iceoryx2 管理器（如果启用）
        let iceoryx_manager: Option<Arc<parking_lot::RwLock<qaexchange::ipc::IceoryxManager>>> = if perf_config.iceoryx.enabled {
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
            log::info!("iceoryx2 disabled (set enabled=true in config/performance.toml to enable)");
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
                    memtable_size_bytes: 64 * 1024 * 1024,  // 64MB（市场数据量大）
                    estimated_entry_size: 256,  // TickData + OrderBookSnapshot 平均大小
                },
            ).expect("Failed to create market data storage")
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
                log::info!("✅ Added {} VIP users to priority queue",
                    perf_config.priority_queue.vip_users.len());
            }
        } else {
            log::info!("Priority queue disabled (set enabled=true in config/performance.toml to enable)");
        }

        let order_router = Arc::new(order_router);

        // 4. 创建结算引擎
        let settlement_engine = Arc::new(SettlementEngine::new(account_mgr.clone()));

        // 5. 创建资金管理器
        let capital_mgr = Arc::new(CapitalManager::new(account_mgr.clone()));

        // 6. 创建风险监控器
        let risk_monitor = Arc::new(RiskMonitor::new(account_mgr.clone()));

        // 7. 创建市场数据服务（包含快照生成器）
        let market_data_service = {
            let mut service = qaexchange::market::MarketDataService::new(matching_engine.clone());

            // 设置存储（用于市场数据恢复）
            service = service.with_storage(market_data_storage.clone());

            // 设置 iceoryx2（如果启用）
            if let Some(ref iceoryx_mgr) = iceoryx_manager {
                service = service.with_iceoryx(iceoryx_mgr.clone());
            }

            // 配置快照生成器：订阅所有合约，每秒生成一次快照
            let instruments = vec![
                "IF2501".to_string(),
                "IF2502".to_string(),
                "IC2501".to_string(),
                "IH2501".to_string(),
            ];
            service = service.with_snapshot_generator(instruments, 1000);

            Arc::new(service)
        };
        log::info!("✅ Market data service with snapshot generator initialized");

        // 7.1 设置 market_data_service 到 trade_gateway（用于更新快照统计）
        // 由于 trade_gateway 已经是 Arc，需要使用 unsafe 获取可变引用
        // 安全性：此时 trade_gateway 只有一个引用（刚创建），可以安全修改
        unsafe {
            let trade_gateway_ptr = Arc::as_ptr(&trade_gateway) as *mut TradeGateway;
            (*trade_gateway_ptr).set_market_data_service(market_data_service.clone());
        }
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
            snapshot_generator_handle: None,
        }
    }

    /// 初始化合约
    fn init_instruments(&self) {
        log::info!("Initializing instruments...");

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 注册合约：沪深300股指期货
        let instruments = vec![
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
                _ => 100.0,
            };

            self.matching_engine
                .register_instrument(inst.instrument_id.clone(), init_price)
                .expect("Failed to register instrument to matching engine");

            // 设置初始结算价
            self.settlement_engine.set_settlement_price(inst.instrument_id.clone(), init_price);

            // 设置快照生成器的昨收盘价（用于涨跌幅计算）
            self.market_data_service.set_pre_close(&inst.instrument_id, init_price);

            log::info!("  ✓ {} @ {} (margin: {}%, commission: {}%)",
                inst.instrument_id,
                init_price,
                inst.margin_rate * 100.0,
                inst.commission_rate * 100.0
            );
        }

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
                memtable_size_bytes: 256 * 1024 * 1024, // 256 MB
                estimated_entry_size: 256,
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
            scan_interval_secs: 300,        // 5 分钟扫描一次
            min_sstables_per_batch: 3,      // 至少 3 个 SSTable
            max_sstables_per_batch: 20,     // 最多 20 个 SSTable
            min_sstable_age_secs: 60,       // 文件至少 1 分钟未修改
            max_retries: 5,
            zombie_timeout_secs: 3600,       // 1 小时超时
        };

        let worker_config = WorkerConfig {
            worker_count: 2,                // 2 个 worker
            batch_read_size: 10000,
            delete_source_after_success: true,
            source_retention_secs: 3600,    // 保留 1 小时
        };

        match ConversionManager::new(
            storage_base,
            metadata_path,
            scheduler_config,
            worker_config,
        ) {
            Ok(mut manager) => {
                manager.start();
                log::info!("✅ OLAP conversion system started");
                log::info!("   Workers: 2");
                log::info!("   Scan interval: 5 minutes");
                log::info!("   Batch size: 3-20 SSTables");

                // 保存到 Arc<Mutex> 以便共享
                self.conversion_mgr = Some(Arc::new(parking_lot::Mutex::new(manager)));
            }
            Err(e) => {
                log::error!("Failed to start OLAP conversion: {}", e);
            }
        }
    }

    /// 启动 HTTP 服务器
    async fn start_http_server(self: Arc<Self>) -> io::Result<actix_web::dev::Server> {
        log::info!("Starting HTTP server at {}...", self.config.http_address);

        let app_state = Arc::new(qaexchange::service::http::handlers::AppState {
            order_router: self.order_router.clone(),
            account_mgr: self.account_mgr.clone(),
            trade_recorder: self.matching_engine.get_trade_recorder(),
            user_mgr: self.user_mgr.clone(),
            storage_stats: self.storage_stats.clone(),
            conversion_mgr: self.conversion_mgr.clone(),
        });

        // 创建市场数据服务（解耦：业务逻辑与网络层分离）
        // 传递 market_data_storage 以支持从 WAL 恢复历史行情
        let mut market_service = qaexchange::market::MarketDataService::new(self.matching_engine.clone())
            .with_storage(self.market_data_storage.clone());

        // 如果启用了 iceoryx2，将 manager 传递给 MarketDataService
        if let Some(ref manager) = self.iceoryx_manager {
            market_service = market_service.with_iceoryx(manager.clone());
        }

        // 创建管理端状态（合约管理、结算管理）
        let admin_state = AdminAppState {
            instrument_registry: self.instrument_registry.clone(),
            settlement_engine: self.settlement_engine.clone(),
            account_mgr: self.account_mgr.clone(),
        };
        let admin_data = web::Data::new(admin_state);

        // 创建管理端状态（账户管理、资金管理、风控监控）
        let management_state = ManagementAppState {
            account_mgr: self.account_mgr.clone(),
            capital_mgr: self.capital_mgr.clone(),
            risk_monitor: self.risk_monitor.clone(),
        };
        let management_data = web::Data::new(management_state);

        let bind_address = self.config.http_address.clone();

        let server = ActixHttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(app_state.clone()))
                .app_data(web::Data::new(market_service.clone()))  // MarketDataService 实现了 Clone
                .app_data(admin_data.clone())
                .app_data(management_data.clone())
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                .wrap(
                    actix_cors::Cors::default()
                        .allow_any_origin()
                        .allow_any_method()
                        .allow_any_header()
                        .max_age(3600)
                )
                .configure(qaexchange::service::http::routes::configure)
        })
        .bind(&bind_address)?
        .run();

        log::info!("✅ HTTP server started at http://{}", bind_address);
        log::info!("   Health: http://{}/health", bind_address);
        log::info!("   Market API: http://{}/api/market/instruments", bind_address);
        log::info!("   Admin API: http://{}/api/admin/market/order-stats", bind_address);

        Ok(server)
    }

    /// 启动订单簿快照广播服务
    fn start_snapshot_broadcaster(&self) {
        log::info!("Starting orderbook snapshot broadcaster...");

        // 启动快照广播服务（500ms 间隔）
        SnapshotBroadcastService::spawn(
            self.matching_engine.clone(),
            self.market_broadcaster.clone(),
            500, // 500ms 间隔
        );

        log::info!("✅ Orderbook snapshot broadcaster started (500ms interval)");
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
        ));

        let bind_address = self.config.ws_address.clone();

        let server = ActixHttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(ws_server.clone()))
                .wrap(middleware::Logger::default())
                .route("/ws", web::get().to(qaexchange::service::websocket::ws_route))
                .route("/ws/diff", web::get().to(qaexchange::service::websocket::ws_diff_route))
                .route("/health", web::get().to(|| async { "OK" }))
        })
        .bind(&bind_address)?
        .run();

        log::info!("✅ WebSocket server started at ws://{}", bind_address);
        log::info!("   Legacy Protocol: ws://{}/ws?user_id=<USER_ID>", bind_address);
        log::info!("   DIFF Protocol:   ws://{}/ws/diff?user_id=<USER_ID> (Recommended)", bind_address);
        log::info!("   Market Data: Subscribe to channels [orderbook, tick, last_price]");

        Ok(server)
    }

    /// 启动定期日志报告
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

                    // 计算总权益和保证金
                    let mut account_balance = acc.money;
                    let mut margin_used = 0.0;

                    for position in acc.hold.values() {
                        let long_value = position.volume_long_unmut() * position.lastest_price;
                        let short_value = position.volume_short_unmut() * position.lastest_price;
                        let position_value = long_value + short_value;
                        account_balance += position_value;
                        margin_used += position_value * 0.15;
                    }

                    total_balance += account_balance;
                    total_margin += margin_used;
                }

                log::info!("━━━━━━━━━━━━━━━━━━━ Periodic Report ━━━━━━━━━━━━━━━━━━━");
                log::info!("📊 Accounts: {} total, {} active", total_count, active_count);
                log::info!("💰 Balance: ¥{:.2} total, ¥{:.2} available, ¥{:.2} margin",
                    total_balance, total_available, total_margin);
                if total_balance > 0.0 {
                    log::info!("📈 Margin Utilization: {:.1}%",
                        (total_margin / total_balance) * 100.0);
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

            loop {
                interval.tick().await;

                match account_mgr.save_snapshots(&snapshot_dir) {
                    Ok(count) if count > 0 => {
                        log::debug!("Saved {} account snapshots", count);
                    }
                    Ok(_) => {} // 没有账户，不记录日志
                    Err(e) => {
                        log::error!("Failed to save snapshots: {}", e);
                    }
                }
            }
        })
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

        let wal_dir = format!("{}/wal", self.config.storage_path);
        let recovery_mgr = RecoveryManager::new(wal_dir);

        match recovery_mgr.recover(&self.account_mgr) {
            Ok(count) if count > 0 => {
                log::info!("✅ [WAL Recovery] Recovered {} accounts from WAL", count);
            }
            Ok(_) => {
                log::debug!("[WAL Recovery] No WAL records found (first time startup or after snapshot)");
            }
            Err(e) => {
                log::error!("[WAL Recovery] Failed to recover from WAL: {}", e);
            }
        }
    }

    /// 从WAL恢复用户数据
    fn recover_from_user_wal(&self) {
        use qaexchange::user::recovery::UserRecovery;

        let user_recovery = UserRecovery::new(
            self.user_storage.clone(),
            self.user_mgr.clone()
        );

        match user_recovery.recover_all_users() {
            Ok(stats) if stats.users_recovered > 0 => {
                log::info!("✅ [User Recovery] Recovered {} users ({} registrations, {} bindings) in {}ms",
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

        // 4. 启动存储订阅器
        let _storage_handle = self.start_storage_subscriber();

        // 4. 启动 OLAP 转换系统
        self.start_olap_conversion();

        // 5. 将 server 包装到 Arc 以便在异步任务中共享
        let server = Arc::new(self);

        // 5. 启动QIFI快照定期保存 (方案A)
        let _snapshot_handle = server.start_snapshot_scheduler();

        // 6. 启动快照广播服务
        server.start_snapshot_broadcaster();

        // 7. 启动定期报告
        let _report_handle = server.start_periodic_reporting();

        // 6. 启动 HTTP 服务器
        let http_server = server.clone().start_http_server().await?;

        // 7. 启动 WebSocket 服务器
        let ws_server = server.clone().start_websocket_server().await?;

        // 8. 打印启动信息
        print_startup_banner(&server.config);

        // 8. 等待服务器
        tokio::try_join!(
            async { http_server.await },
            async { ws_server.await }
        )?;

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
    println!("   1. 开户:     curl -X POST http://{}/api/account/open \\", config.http_address);
    println!("                  -H 'Content-Type: application/json' \\");
    println!("                  -d '{{\"user_id\":\"demo\",\"user_name\":\"Demo User\",\"init_cash\":1000000,\"account_type\":\"individual\",\"password\":\"demo123\"}}'");
    println!("\n   2. 提交订单: curl -X POST http://{}/api/order/submit \\", config.http_address);
    println!("                  -H 'Content-Type: application/json' \\");
    println!("                  -d '{{\"user_id\":\"demo\",\"instrument_id\":\"IF2501\",\"direction\":\"BUY\",\"offset\":\"OPEN\",\"volume\":1,\"price\":3800,\"order_type\":\"LIMIT\"}}'");
    println!("\n   3. 查询账户: curl http://{}/api/account/demo", config.http_address);

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
                },
                instruments: vec![],
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

    // 2. 转换为运行时配置
    let mut config = ExchangeConfig::from_toml(toml_config);

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
