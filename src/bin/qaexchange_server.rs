//! QAExchange 完整交易所服务
//!
//! @yutiansut @quantaxis
//!
//! 集成功能：
//! 1. 交易所核心引擎（撮合、风控、账户管理）
//! 2. HTTP API（REST 接口）
//! 3. WebSocket API（实时推送）
//! 4. 解耦存储层（异步持久化）
//! 5. 通知系统（NotificationBroker + Gateway）
//!
//! 运行: cargo run --bin qaexchange-server

use actix_web::{middleware, web, App, HttpServer as ActixHttpServer};
use qaexchange::exchange::instrument_registry::InstrumentInfo;
use qaexchange::exchange::{AccountManager, InstrumentRegistry, OrderRouter, SettlementEngine, TradeGateway};
use qaexchange::market::MarketDataBroadcaster;
use qaexchange::matching::engine::ExchangeMatchingEngine;
use qaexchange::notification::{NotificationBroker, NotificationGateway};
use qaexchange::service::websocket::WebSocketServer;
use qaexchange::storage::hybrid::oltp::OltpHybridConfig;
use qaexchange::storage::subscriber::{StorageSubscriber, StorageSubscriberConfig};
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;

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

/// 通知系统句柄
struct NotificationHandles {
    /// Broker 后台任务
    broker_handle: tokio::task::JoinHandle<()>,
    /// Gateway 推送任务
    gateway_pusher_handle: tokio::task::JoinHandle<()>,
    /// Gateway 心跳任务
    gateway_heartbeat_handle: tokio::task::JoinHandle<()>,
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

    /// 行情广播器
    market_broadcaster: Arc<MarketDataBroadcaster>,

    /// 结算引擎
    settlement_engine: Arc<SettlementEngine>,

    /// 通知路由中心
    notification_broker: Arc<NotificationBroker>,

    /// 通知推送网关
    notification_gateway: Arc<NotificationGateway>,
}

impl ExchangeServer {
    /// 创建交易所服务
    fn new(config: ExchangeConfig) -> Self {
        log::info!("Initializing Exchange Server...");

        // ═══════════════════════════════════════════════════════════════════
        // 1. 创建通知系统 (首先创建，因为其他组件依赖它)
        // ═══════════════════════════════════════════════════════════════════
        let notification_broker = Arc::new(NotificationBroker::new());

        // 创建 Gateway 接收通道
        let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
        let notification_gateway = Arc::new(NotificationGateway::new("main_gateway", gateway_rx));

        // 注册 Gateway 到 Broker
        notification_broker.register_gateway("main_gateway", gateway_tx);

        log::info!("✅ Notification system initialized");

        // ═══════════════════════════════════════════════════════════════════
        // 2. 创建核心交易组件
        // ═══════════════════════════════════════════════════════════════════
        let account_mgr = Arc::new(AccountManager::new());
        let matching_engine = Arc::new(ExchangeMatchingEngine::new());
        let instrument_registry = Arc::new(InstrumentRegistry::new());

        // 创建 TradeGateway 并注入 NotificationBroker
        let mut trade_gateway = TradeGateway::new(account_mgr.clone());
        trade_gateway.set_notification_broker(notification_broker.clone());
        let trade_gateway = Arc::new(trade_gateway);

        let market_broadcaster = Arc::new(MarketDataBroadcaster::new());
        let settlement_engine = Arc::new(SettlementEngine::new(account_mgr.clone()));

        // 3. 创建订单路由器
        let order_router = Arc::new(OrderRouter::new(
            account_mgr.clone(),
            matching_engine.clone(),
            instrument_registry.clone(),
            trade_gateway.clone(),
        ));

        log::info!("✅ Core components initialized");

        Self {
            config,
            account_mgr,
            matching_engine,
            instrument_registry,
            trade_gateway,
            order_router,
            market_broadcaster,
            settlement_engine,
            notification_broker,
            notification_gateway,
        }
    }

    /// 初始化合约
    fn init_instruments(&self) {
        log::info!("Initializing instruments...");

        use qaexchange::exchange::instrument_registry::{InstrumentStatus, InstrumentType};
        let instruments = vec![
            {
                let mut info = InstrumentInfo::new(
                    "IF2501".to_string(),
                    "沪深300股指期货2501".to_string(),
                    InstrumentType::IndexFuture,
                    "CFFEX".to_string(),
                );
                info.status = InstrumentStatus::Active;
                info
            },
            {
                let mut info = InstrumentInfo::new(
                    "IF2502".to_string(),
                    "沪深300股指期货2502".to_string(),
                    InstrumentType::IndexFuture,
                    "CFFEX".to_string(),
                );
                info.status = InstrumentStatus::Active;
                info
            },
            {
                let mut info = InstrumentInfo::new(
                    "IC2501".to_string(),
                    "中证500股指期货2501".to_string(),
                    InstrumentType::IndexFuture,
                    "CFFEX".to_string(),
                );
                info.status = InstrumentStatus::Active;
                info
            },
            {
                let mut info = InstrumentInfo::new(
                    "IH2501".to_string(),
                    "上证50股指期货2501".to_string(),
                    InstrumentType::IndexFuture,
                    "CFFEX".to_string(),
                );
                info.status = InstrumentStatus::Active;
                info
            },
        ];

        for inst in instruments {
            let _ = self.instrument_registry.register(inst.clone());

            let init_price = match inst.instrument_id.as_str() {
                "IF2501" => 3800.0,
                "IF2502" => 3820.0,
                "IC2501" => 5600.0,
                "IH2501" => 2800.0,
                _ => 100.0,
            };

            self.matching_engine
                .register_instrument(inst.instrument_id.clone(), init_price)
                .expect("Failed to register instrument");

            log::info!("  ✓ {} @ {}", inst.instrument_id, init_price);
        }

        log::info!(
            "✅ {} instruments initialized",
            self.instrument_registry.list_all().len()
        );
    }

    /// 启动通知系统后台任务
    fn start_notification_system(&self) -> NotificationHandles {
        log::info!("Starting notification system...");

        // 1. 启动 Broker 优先级处理器
        let broker_handle = self.notification_broker.clone().start_priority_processor();

        // 2. 启动 Gateway 推送任务
        let gateway_pusher_handle = self.notification_gateway.clone().start_notification_pusher();

        // 3. 启动 Gateway 心跳检测
        let gateway_heartbeat_handle = self.notification_gateway.clone().start_heartbeat_checker();

        log::info!("✅ Notification system started");
        log::info!("   Broker: Priority processor (P0-P3 queues)");
        log::info!("   Gateway: Pusher + Heartbeat checker (300s timeout)");

        NotificationHandles {
            broker_handle,
            gateway_pusher_handle,
            gateway_heartbeat_handle,
        }
    }

    /// 启动存储订阅器（连接到通知系统）
    fn start_storage_subscriber(&self) -> Option<tokio::task::JoinHandle<()>> {
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
                enable_olap_conversion: true,           // 启用 OLAP 转换
                olap_conversion_threshold: 10,          // 10 个 SSTable 触发转换
                olap_conversion_age_seconds: 86400,     // 24 小时后数据转为 OLAP
            },
            batch_size: 100,
            batch_timeout_ms: 10,
            buffer_size: 10000,
        };

        let (subscriber, storage_sender, stats) = StorageSubscriber::new(storage_config);

        // ═══════════════════════════════════════════════════════════════════
        // 关键：将 StorageSubscriber 连接到 NotificationBroker
        // ═══════════════════════════════════════════════════════════════════

        // 创建一个桥接通道：从 NotificationBroker 转发到 StorageSubscriber
        let broker = self.notification_broker.clone();
        let (bridge_tx, mut bridge_rx) = mpsc::unbounded_channel();

        // 注册为全局订阅者
        broker.subscribe_global("storage_subscriber", bridge_tx);

        // 启动桥接任务：从 Broker 读取通知并转发到 Storage
        let storage_sender_clone = storage_sender.clone();
        tokio::spawn(async move {
            log::info!("Storage bridge started: NotificationBroker -> StorageSubscriber");
            while let Some(notification) = bridge_rx.recv().await {
                if let Err(e) = storage_sender_clone.send(notification) {
                    log::error!("Failed to forward notification to storage: {}", e);
                }
            }
            log::warn!("Storage bridge stopped");
        });

        // 启动订阅器
        let handle = tokio::spawn(async move {
            subscriber.run().await;
        });

        // 定期打印统计信息
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let s = stats_clone.lock();
                log::info!(
                    "📊 Storage stats: received={}, persisted={}, batches={}, errors={}",
                    s.total_received,
                    s.total_persisted,
                    s.total_batches,
                    s.total_errors
                );
            }
        });

        log::info!("✅ Storage subscriber started and connected to NotificationBroker");
        log::info!("   Path: {}", self.config.storage_path);
        log::info!("   Batch: 100 records / 10ms timeout");

        Some(handle)
    }

    /// 启动 HTTP 服务器
    async fn start_http_server(self: Arc<Self>) -> io::Result<actix_web::dev::Server> {
        log::info!("Starting HTTP server at {}...", self.config.http_address);

        use qaexchange::matching::trade_recorder::TradeRecorder;
        use qaexchange::service::http::handlers::AppState;
        use qaexchange::user::UserManager;

        let app_state = Arc::new(AppState {
            order_router: self.order_router.clone(),
            account_mgr: self.account_mgr.clone(),
            settlement_engine: self.settlement_engine.clone(),
            trade_recorder: Arc::new(TradeRecorder::new()),
            user_mgr: Arc::new(UserManager::new()),
            storage_stats: None,
            conversion_mgr: None,
        });

        let bind_address = self.config.http_address.clone();

        let server = ActixHttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(app_state.clone()))
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                .configure(qaexchange::service::http::routes::configure)
        })
        .bind(&bind_address)?
        .run();

        log::info!("✅ HTTP server started at http://{}", bind_address);
        log::info!("   Health: http://{}/health", bind_address);
        log::info!("   API Docs: http://{}/api", bind_address);

        Ok(server)
    }

    /// 启动 WebSocket 服务器
    async fn start_websocket_server(self: Arc<Self>) -> io::Result<actix_web::dev::Server> {
        log::info!("Starting WebSocket server at {}...", self.config.ws_address);

        use actix::Actor;
        use qaexchange::market::KLineActor;
        use qaexchange::storage::wal::manager::WalManager;
        use qaexchange::user::UserManager;

        // 创建 KLineActor
        let wal_path = format!("{}/kline_wal", self.config.storage_path);
        let wal_manager = Arc::new(WalManager::new(&wal_path));
        let kline_actor = KLineActor::new(self.market_broadcaster.clone(), wal_manager).start();

        let ws_server = Arc::new(WebSocketServer::new(
            self.order_router.clone(),
            self.account_mgr.clone(),
            Arc::new(UserManager::new()),
            self.trade_gateway.clone(),
            self.market_broadcaster.clone(),
            kline_actor,
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
                .route("/health", web::get().to(|| async { "OK" }))
        })
        .bind(&bind_address)?
        .run();

        log::info!("✅ WebSocket server started at ws://{}/ws", bind_address);
        log::info!("   Connection: ws://{}/ws?user_id=<USER_ID>", bind_address);

        Ok(server)
    }

    /// 运行服务器
    async fn run(self) -> io::Result<()> {
        let server = Arc::new(self);

        // 1. 初始化合约
        server.init_instruments();

        // 2. 启动通知系统
        let _notification_handles = server.start_notification_system();

        // 3. 启动存储订阅器（连接到通知系统）
        let _storage_handle = server.start_storage_subscriber();

        // 4. 启动 HTTP 服务器
        let http_server = server.clone().start_http_server().await?;

        // 5. 启动 WebSocket 服务器
        let ws_server = server.clone().start_websocket_server().await?;

        // 6. 打印启动信息
        print_startup_banner(&server.config, &server.notification_broker);

        // 7. 等待服务器
        tokio::try_join!(async { http_server.await }, async { ws_server.await })?;

        Ok(())
    }
}

/// 打印启动横幅
fn print_startup_banner(config: &ExchangeConfig, broker: &Arc<NotificationBroker>) {
    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║                    🚀 QAExchange Server Started                       ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    println!("📡 Service Endpoints:");
    println!("   • HTTP API:    http://{}", config.http_address);
    println!("   • WebSocket:   ws://{}/ws", config.ws_address);
    println!("   • Health:      http://{}/health", config.http_address);

    println!("\n🔔 Notification System:");
    let stats = broker.get_stats();
    println!("   • Broker:      Active (P0-P3 priority queues)");
    println!("   • Gateways:    {} registered", stats.active_gateways);
    println!("   • Users:       {} subscribed", stats.active_users);

    println!("\n💾 Storage:");
    if config.enable_storage {
        println!("   • Status:      Enabled ✓ (Connected to NotificationBroker)");
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

    // 解析命令行参数
    let mut config = ExchangeConfig::default();

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
    let server = ExchangeServer::new(config);
    server.run().await
}
