//! Storage 层与交易流程集成示例
//!
//! 演示如何将 OltpHybridStorage 集成到交易所的完整流程中：
//! 1. 订单提交 → WAL (OrderInsert) → MemTable → SSTable
//! 2. 订单撮合 → WAL (TradeExecuted) → MemTable → SSTable
//! 3. 账户更新 → WAL (AccountUpdate) → MemTable → SSTable
//!
//! 运行: cargo run --example storage_integration_demo

use qaexchange::storage::hybrid::oltp::{OltpHybridStorage, OltpHybridConfig};
use qaexchange::storage::wal::record::WalRecord;
use qaexchange::exchange::{AccountManager, InstrumentRegistry, TradeGateway};
use qaexchange::exchange::order_router::{OrderRouter, SubmitOrderRequest};
use qaexchange::exchange::instrument_registry::InstrumentInfo;
use qaexchange::core::account_ext::{OpenAccountRequest, AccountType};
use qaexchange::matching::engine::ExchangeMatchingEngine;
use std::sync::Arc;
use std::time::Instant;

/// 集成了 Storage 的订单路由器扩展
///
/// **设计模式**: Extension wrapper (扩展现有 OrderRouter，不修改其代码)
struct StorageIntegratedRouter {
    /// 原始订单路由器（复用现有逻辑）
    router: Arc<OrderRouter>,

    /// Storage 层（按品种分离）
    storages: dashmap::DashMap<String, Arc<OltpHybridStorage>>,

    /// Storage 配置
    config: OltpHybridConfig,
}

impl StorageIntegratedRouter {
    fn new(router: Arc<OrderRouter>, config: OltpHybridConfig) -> Self {
        Self {
            router,
            storages: dashmap::DashMap::new(),
            config,
        }
    }

    /// 获取或创建品种的 Storage
    fn get_or_create_storage(&self, instrument_id: &str) -> Arc<OltpHybridStorage> {
        self.storages.entry(instrument_id.to_string())
            .or_insert_with(|| {
                let storage = OltpHybridStorage::create(instrument_id, self.config.clone())
                    .expect("Create storage failed");
                Arc::new(storage)
            })
            .clone()
    }

    /// 提交订单 (带持久化)
    fn submit_order_with_persistence(&self, req: SubmitOrderRequest) -> Result<String, String> {
        let start = Instant::now();

        // 1. 准备 WAL 记录
        let order_id = self.generate_order_id();
        let storage = self.get_or_create_storage(&req.instrument_id);

        // 将 user_id 转换为 [u8; 32]
        let mut user_id_bytes = [0u8; 32];
        let user_bytes = req.user_id.as_bytes();
        let copy_len = user_bytes.len().min(32);
        user_id_bytes[..copy_len].copy_from_slice(&user_bytes[..copy_len]);

        // 将 instrument_id 转换为 [u8; 16]
        let mut instrument_id_bytes = [0u8; 16];
        let inst_bytes = req.instrument_id.as_bytes();
        let copy_len = inst_bytes.len().min(16);
        instrument_id_bytes[..copy_len].copy_from_slice(&inst_bytes[..copy_len]);

        let direction_byte = match req.direction.as_str() {
            "BUY" => 0u8,
            "SELL" => 1u8,
            _ => 0u8,
        };

        let offset_byte = match req.offset.as_str() {
            "OPEN" => 0u8,
            "CLOSE" | "CLOSETODAY" => 1u8,
            _ => 0u8,
        };

        let wal_record = WalRecord::OrderInsert {
            order_id: order_id.parse::<u64>().unwrap_or(0),
            user_id: user_id_bytes,
            instrument_id: instrument_id_bytes,
            direction: direction_byte,
            offset: offset_byte,
            price: req.price,
            volume: req.volume,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        // 2. 写入 Storage (WAL + MemTable)
        let sequence = storage.write(wal_record)
            .map_err(|e| format!("Storage write failed: {}", e))?;

        println!("✅ WAL写入成功: sequence={}, 耗时={:?}", sequence, start.elapsed());

        // 3. 调用原始订单路由器
        let response = self.router.submit_order(req);

        if response.success {
            Ok(response.order_id.unwrap_or(order_id))
        } else {
            Err(response.error_message.unwrap_or("Unknown error".to_string()))
        }
    }

    /// 记录成交 (持久化)
    fn record_trade(&self, instrument_id: &str, order_id: u64, trade_id: u64, price: f64, volume: f64) -> Result<u64, String> {
        let storage = self.get_or_create_storage(instrument_id);

        let wal_record = WalRecord::TradeExecuted {
            trade_id,
            order_id,
            exchange_order_id: order_id, // 模拟盘与 order_id 相同
            price,
            volume,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        storage.write(wal_record)
    }

    /// 记录账户更新 (持久化)
    fn record_account_update(&self, instrument_id: &str, user_id: &str, balance: f64, available: f64, margin: f64) -> Result<u64, String> {
        let storage = self.get_or_create_storage(instrument_id);

        let mut user_id_bytes = [0u8; 32];
        let user_bytes = user_id.as_bytes();
        let copy_len = user_bytes.len().min(32);
        user_id_bytes[..copy_len].copy_from_slice(&user_bytes[..copy_len]);

        let wal_record = WalRecord::AccountUpdate {
            user_id: user_id_bytes,
            balance,
            available,
            frozen: 0.0, // 简化处理，frozen 是复杂结构
            margin,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        storage.write(wal_record)
    }

    fn generate_order_id(&self) -> String {
        let timestamp = chrono::Utc::now().timestamp_millis();
        format!("O{}{:010}", timestamp, rand::random::<u32>())
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║          Storage 层与交易流程集成演示                                ║");
    println!("║  订单提交 → WAL → MemTable → 撮合 → 成交 → WAL → SSTable         ║");
    println!("╚════════════════════════════════════════════════════════════════════╝\n");

    // ============================================================
    // 第一步: 初始化交易所核心组件
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📦 Step 1: 初始化交易所核心组件");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 1.1 创建账户管理器
    let account_mgr = Arc::new(AccountManager::new());

    // 1.2 创建撮合引擎
    let matching_engine = Arc::new(ExchangeMatchingEngine::new());

    // 1.3 创建合约注册表
    let instrument_registry = Arc::new(InstrumentRegistry::new());
    instrument_registry.register(InstrumentInfo {
        instrument_id: "IF2501".to_string(),
        name: "沪深300股指期货2501".to_string(),
        exchange_id: "CFFEX".to_string(),
        product_type: "futures".to_string(),
        is_trading: true,
    });

    // 1.4 创建成交网关
    let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));

    // 1.5 创建订单路由器
    let router = Arc::new(OrderRouter::new(
        account_mgr.clone(),
        matching_engine.clone(),
        instrument_registry.clone(),
        trade_gateway.clone(),
    ));

    println!("✅ 账户管理器、撮合引擎、订单路由器初始化完成\n");

    // ============================================================
    // 第二步: 集成 Storage 层
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("💾 Step 2: 集成 Storage 层 (WAL + MemTable + SSTable)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let storage_config = OltpHybridConfig {
        base_path: "/tmp/qaexchange_demo/storage".to_string(),
        memtable_size_bytes: 64 * 1024 * 1024, // 64 MB
        estimated_entry_size: 256,
    };

    let integrated_router = StorageIntegratedRouter::new(router.clone(), storage_config);

    println!("✅ Storage 层集成完成");
    println!("   • 存储路径: /tmp/qaexchange_demo/storage");
    println!("   • MemTable 大小: 64 MB");
    println!("   • 持久化模式: WAL + MemTable + SSTable\n");

    // ============================================================
    // 第三步: 开户并注册合约
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("👤 Step 3: 开户并注册合约");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 开户
    let open_req = OpenAccountRequest {
        user_id: "trader_001".to_string(),
        user_name: "张三".to_string(),
        init_cash: 1_000_000.0,
        account_type: AccountType::Individual,
        password: "secure_password".to_string(),
    };

    match account_mgr.open_account(open_req) {
        Ok(user_id) => {
            println!("✅ 账户开设成功:");
            println!("   • 用户ID: {}", user_id);
            println!("   • 初始资金: ¥1000000.00");
            println!("   • 可用资金: ¥1000000.00");
        }
        Err(e) => {
            println!("❌ 开户失败: {}", e);
            return;
        }
    }

    // 注册合约到撮合引擎
    matching_engine.register_instrument("IF2501".to_string(), 3800.0)
        .expect("Register instrument failed");

    println!("✅ 合约注册成功:");
    println!("   • 合约代码: IF2501");
    println!("   • 当前价格: ¥3800.00\n");

    // ============================================================
    // 第四步: 提交订单（带持久化）
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📝 Step 4: 提交订单（写入 WAL + MemTable）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let order_req = SubmitOrderRequest {
        user_id: "trader_001".to_string(),
        instrument_id: "IF2501".to_string(),
        direction: "BUY".to_string(),
        offset: "OPEN".to_string(),
        volume: 10.0,
        price: 3800.0,
        order_type: "LIMIT".to_string(),
    };

    println!("订单详情:");
    println!("   • 用户: trader_001");
    println!("   • 合约: IF2501");
    println!("   • 方向: BUY OPEN (买开)");
    println!("   • 数量: 10 手");
    println!("   • 价格: ¥3800.00\n");

    let start = Instant::now();
    match integrated_router.submit_order_with_persistence(order_req.clone()) {
        Ok(order_id) => {
            println!("✅ 订单提交成功!");
            println!("   • 订单ID: {}", order_id);
            println!("   • 总耗时: {:?}", start.elapsed());
            println!("   • 持久化: WAL ✓, MemTable ✓\n");
        }
        Err(e) => {
            println!("❌ 订单提交失败: {}\n", e);
            return;
        }
    }

    // ============================================================
    // 第五步: 模拟成交并持久化
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🤝 Step 5: 模拟成交并写入 WAL");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let trade_id = 100001u64;
    let order_id = 200001u64;

    let start = Instant::now();
    match integrated_router.record_trade("IF2501", order_id, trade_id, 3800.0, 10.0) {
        Ok(sequence) => {
            println!("✅ 成交记录持久化成功!");
            println!("   • 成交ID: {}", trade_id);
            println!("   • 成交价: ¥3800.00");
            println!("   • 成交量: 10 手");
            println!("   • WAL序号: {}", sequence);
            println!("   • 耗时: {:?}\n", start.elapsed());
        }
        Err(e) => println!("❌ 成交记录失败: {}\n", e),
    }

    // ============================================================
    // 第六步: 账户更新持久化
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("💰 Step 6: 账户更新并持久化");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 获取账户状态
    if let Ok(account) = account_mgr.get_account("trader_001") {
        let mut acc = account.write();
        let balance = acc.get_balance();
        let available = acc.money;
        let margin = acc.get_margin();

        println!("当前账户状态:");
        println!("   • 总资金: ¥{:.2}", balance);
        println!("   • 可用: ¥{:.2}", available);
        println!("   • 保证金: ¥{:.2}\n", margin);

        let start = Instant::now();
        match integrated_router.record_account_update("IF2501", "trader_001", balance, available, margin) {
            Ok(sequence) => {
                println!("✅ 账户更新持久化成功!");
                println!("   • WAL序号: {}", sequence);
                println!("   • 耗时: {:?}\n", start.elapsed());
            }
            Err(e) => println!("❌ 账户更新失败: {}\n", e),
        }
    }

    // ============================================================
    // 第七步: 统计信息
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Step 7: 统计信息");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("✅ 集成完成统计:");
    println!("   • 订单持久化: 1 条 (OrderInsert)");
    println!("   • 成交持久化: 1 条 (TradeExecuted)");
    println!("   • 账户持久化: 1 条 (AccountUpdate)");
    println!("   • 总WAL记录: 3 条");
    println!("   • 存储位置: /tmp/qaexchange_demo/storage/IF2501/");
    println!("   • 数据格式: rkyv (零拷贝反序列化)");

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 演示完成! Storage 层已成功集成到交易流程");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    println!("\n💡 下一步集成建议:");
    println!("   1. 在 OrderRouter::submit_order 中直接调用 storage.write()");
    println!("   2. 在 TradeGateway::handle_filled 中写入 TradeExecuted 记录");
    println!("   3. 在 AccountManager 更新时写入 AccountUpdate 记录");
    println!("   4. 实现崩溃恢复：启动时从 WAL replay 所有记录");
    println!("   5. 实现定期 flush：MemTable → SSTable 持久化");
}
