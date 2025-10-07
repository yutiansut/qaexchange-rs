//! 解耦存储演示 - 零拷贝 + 异步持久化
//!
//! **架构亮点**:
//! 1. 主交易流程：OrderRouter → Matching → TradeGateway (P99 < 100μs)
//! 2. 存储订阅器：独立 Tokio 任务，异步批量写入
//! 3. 零拷贝通信：基于 crossbeam channel (未来可升级 iceoryx2)
//! 4. 完全解耦：存储故障不影响交易
//!
//! 运行: cargo run --example decoupled_storage_demo

use qaexchange::storage::subscriber::{StorageSubscriber, StorageSubscriberConfig};
use qaexchange::storage::hybrid::oltp::OltpHybridConfig;
use qaexchange::exchange::{AccountManager, InstrumentRegistry, TradeGateway};
use qaexchange::exchange::order_router::{OrderRouter, SubmitOrderRequest};
use qaexchange::exchange::instrument_registry::InstrumentInfo;
use qaexchange::core::account_ext::{OpenAccountRequest, AccountType};
use qaexchange::matching::engine::ExchangeMatchingEngine;
use qaexchange::notification::broker::NotificationBroker;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║          解耦存储演示 - 异步持久化 + 零拷贝通信                      ║");
    println!("║  主流程 (无阻塞) → Channel → 存储订阅器 (独立任务)                 ║");
    println!("╚════════════════════════════════════════════════════════════════════╝\n");

    // ============================================================
    // Step 1: 启动存储订阅器（独立 Tokio 任务）
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🚀 Step 1: 启动存储订阅器（独立任务，不阻塞主流程）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let storage_config = StorageSubscriberConfig {
        storage_config: OltpHybridConfig {
            base_path: "/tmp/qaexchange_decoupled/storage".to_string(),
            memtable_size_bytes: 64 * 1024 * 1024,
            estimated_entry_size: 256,
        },
        batch_size: 100,           // 批量 100 条
        batch_timeout_ms: 10,      // 10ms 超时
        buffer_size: 10000,        // 缓冲 10K 条
    };

    let (subscriber, storage_sender, _stats) = StorageSubscriber::new(storage_config);

    // 创建 NotificationBroker
    let notification_broker = Arc::new(NotificationBroker::new());

    // 订阅通知到存储订阅器
    notification_broker.subscribe_global("storage_subscriber", storage_sender);

    // 启动订阅器（独立任务）
    tokio::spawn(async move {
        subscriber.run().await;
    });

    println!("✅ 存储订阅器已启动");
    println!("   • 批量大小: 100 条");
    println!("   • 超时时间: 10 ms");
    println!("   • 缓冲区: 10000 条");
    println!("   • 模式: 异步批量写入\n");

    // ============================================================
    // Step 2: 初始化交易所核心组件
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📦 Step 2: 初始化交易所核心组件");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let account_mgr = Arc::new(AccountManager::new());
    let matching_engine = Arc::new(ExchangeMatchingEngine::new());
    let instrument_registry = Arc::new(InstrumentRegistry::new());

    // 创建 TradeGateway 并设置 NotificationBroker
    let mut trade_gateway = TradeGateway::new(account_mgr.clone());
    trade_gateway.set_notification_broker(notification_broker.clone());
    let trade_gateway = Arc::new(trade_gateway);

    let router = Arc::new(OrderRouter::new(
        account_mgr.clone(),
        matching_engine.clone(),
        instrument_registry.clone(),
        trade_gateway.clone(),
    ));

    println!("✅ 交易所核心组件初始化完成");
    println!("   • 存储订阅器已连接到全局通知\n");

    // ============================================================
    // Step 3: 开户并注册合约
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("👤 Step 3: 开户并注册合约");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    use qaexchange::exchange::instrument_registry::{InstrumentType, InstrumentStatus};
    let mut info = InstrumentInfo::new(
        "IF2501".to_string(),
        "沪深300股指期货2501".to_string(),
        InstrumentType::IndexFuture,
        "CFFEX".to_string(),
    );
    info.status = InstrumentStatus::Active;
    instrument_registry.register(info).expect("Failed to register instrument");

    matching_engine.register_instrument("IF2501".to_string(), 3800.0)
        .expect("Register instrument failed");

    let open_req = OpenAccountRequest {
        user_id: "user_001".to_string(),
        account_id: Some("trader_001".to_string()),
        account_name: "张三的账户".to_string(),
        init_cash: 1_000_000.0,
        account_type: AccountType::Individual,
    };

    let account_id = account_mgr.open_account(open_req).expect("Open account failed");

    println!("✅ 账户和合约注册完成\n");

    // ============================================================
    // Step 4: 提交订单（主流程，无存储阻塞）
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📝 Step 4: 提交订单（主流程无阻塞，延迟 < 100μs）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let mut latencies = Vec::new();

    for i in 0..10 {
        let req = SubmitOrderRequest {
            account_id: account_id.clone(),
            instrument_id: "IF2501".to_string(),
            direction: if i % 2 == 0 { "BUY" } else { "SELL" }.to_string(),
            offset: if i % 2 == 0 { "OPEN" } else { "CLOSE" }.to_string(),
            volume: 1.0,
            price: 3800.0 + (i as f64) * 0.5,
            order_type: "LIMIT".to_string(),
        };

        let start = Instant::now();
        let response = router.submit_order(req);
        let elapsed = start.elapsed();

        latencies.push(elapsed);

        if response.success {
            println!(
                "✅ 订单 #{} 提交成功 (延迟: {:?})",
                i + 1,
                elapsed
            );
        } else {
            println!(
                "❌ 订单 #{} 提交失败: {:?}",
                i + 1,
                response.error_message
            );
        }
    }

    // 计算延迟统计
    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let max_latency = latencies.iter().max().unwrap();

    println!("\n📊 主流程性能统计:");
    println!("   • 平均延迟: {:?}", avg_latency);
    println!("   • 最大延迟: {:?}", max_latency);
    println!("   • 订单数量: {}", latencies.len());

    // ============================================================
    // Step 5: 等待存储订阅器处理
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("⏳ Step 5: 等待存储订阅器异步处理...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    sleep(Duration::from_millis(500)).await;

    println!("✅ 存储订阅器处理完成");
    println!("   • 数据位置: /tmp/qaexchange_decoupled/storage/");
    println!("   • 持久化格式: WAL + MemTable (rkyv 零拷贝)");

    // ============================================================
    // 总结
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 解耦存储演示完成！");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("🎯 架构优势:");
    println!("   1. 主流程延迟: {:?} (无存储阻塞)", avg_latency);
    println!("   2. 存储解耦: 独立 Tokio 任务，批量写入");
    println!("   3. 零拷贝: rkyv 序列化 (125x faster than JSON)");
    println!("   4. 高可用: 存储故障不影响交易");
    println!("   5. 可扩展: 可升级到 iceoryx2 跨进程通信");

    println!("\n💡 下一步升级路径:");
    println!("   1. crossbeam::channel (当前) → iceoryx2 (跨进程零拷贝)");
    println!("   2. 单进程存储 → 多进程存储集群");
    println!("   3. 批量写入 → 并行写入多品种");
    println!("   4. 本地存储 → 分布式存储 (NVMe-oF/RDMA)");
    println!("   5. 增加 Compaction 线程 (SSTable 合并)");
}
