//! 快照生成器集成测试示例
//!
//! 测试快照生成器的完整功能：
//! 1. 订单簿数据 → 快照生成
//! 2. 成交事件 → 统计更新
//! 3. 多订阅者并发消费
//!
//! 运行: cargo run --example test_snapshot_generator

use qaexchange::market::snapshot_generator::{MarketSnapshotGenerator, SnapshotGeneratorConfig};
use qaexchange::matching::engine::ExchangeMatchingEngine;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    env_logger::init();

    println!("\n=== 快照生成器测试 ===\n");

    // 1. 创建撮合引擎并注册合约
    println!("1️⃣  初始化撮合引擎...");
    let matching_engine = Arc::new(ExchangeMatchingEngine::new());

    let instrument_id = "IF2501";
    let init_price = 3800.0;
    matching_engine
        .register_instrument(instrument_id.to_string(), init_price)
        .expect("Failed to register instrument");

    println!("   ✅ 注册合约: {} @ {}", instrument_id, init_price);

    // 2. 创建快照生成器
    println!("\n2️⃣  创建快照生成器...");
    let config = SnapshotGeneratorConfig {
        interval_ms: 1000, // 1秒
        enable_persistence: false,
        instruments: vec![instrument_id.to_string()],
    };

    let generator = Arc::new(MarketSnapshotGenerator::new(
        matching_engine.clone(),
        config,
    ));

    // 设置昨收盘价（用于涨跌幅计算）
    generator.set_pre_close(instrument_id, init_price);
    println!("   ✅ 快照生成器已创建 (间隔: 1s)");

    // 3. 订阅快照（创建3个消费者）
    println!("\n3️⃣  创建订阅者...");
    let subscriber1 = generator.subscribe();
    let subscriber2 = generator.subscribe();
    let subscriber3 = generator.subscribe();
    println!("   ✅ 创建了 3 个订阅者");

    // 4. 启动快照生成器
    println!("\n4️⃣  启动快照生成器...");
    let _generator_handle = generator.clone().start();
    println!("   ✅ 后台线程已启动");

    // 5. 提交订单到订单簿（使用低级 API 直接插入）
    println!("\n5️⃣  提交测试订单...");
    let orderbook = matching_engine.get_orderbook(instrument_id).unwrap();

    // 由于直接访问 qars orderbook 的限制，我们改为通过 OrderRouter 提交订单
    // 这里简化演示：直接更新生成器的统计数据，模拟订单簿有数据
    println!("   ⚠️  简化版测试：跳过订单提交，直接测试快照生成");
    println!("   （完整测试请使用集成环境，通过 OrderRouter 提交订单）");

    // 6. 模拟成交并更新统计
    println!("\n6️⃣  模拟成交事件...");
    std::thread::sleep(Duration::from_millis(100));

    generator.update_trade_stats(instrument_id, 100, 380000.0);
    println!("   ✅ 第1笔成交: volume=100, turnover=380,000");

    std::thread::sleep(Duration::from_millis(500));

    generator.update_trade_stats(instrument_id, 50, 190000.0);
    println!("   ✅ 第2笔成交: volume=50, turnover=190,000");

    // 7. 订阅者消费快照
    println!("\n7️⃣  订阅者开始消费快照...");
    println!("   (等待 5 秒，每秒接收一次快照)\n");

    let start_time = std::time::Instant::now();

    // 启动3个消费者线程
    let consumer1 = std::thread::spawn(move || {
        let mut count = 0;
        while count < 5 {
            if let Ok(snapshot) = subscriber1.recv_timeout(Duration::from_secs(2)) {
                count += 1;
                println!(
                    "   [订阅者1] 收到快照 #{}: {} @ {:.2} (涨跌: {:.2}%, 成交量: {})",
                    count,
                    snapshot.instrument_id,
                    snapshot.last_price,
                    snapshot.change_percent,
                    snapshot.volume,
                );
            }
        }
    });

    let consumer2 = std::thread::spawn(move || {
        let mut count = 0;
        while count < 5 {
            if let Ok(snapshot) = subscriber2.recv_timeout(Duration::from_secs(2)) {
                count += 1;
                println!(
                    "   [订阅者2] 买一: {:.2} x {}, 卖一: {:.2} x {}",
                    snapshot.bid_price1,
                    snapshot.bid_volume1,
                    snapshot.ask_price1,
                    snapshot.ask_volume1,
                );
            }
        }
    });

    let consumer3 = std::thread::spawn(move || {
        let mut count = 0;
        while count < 5 {
            if let Ok(snapshot) = subscriber3.recv_timeout(Duration::from_secs(2)) {
                count += 1;
                println!(
                    "   [订阅者3] OHLC: O={:.2} H={:.2} L={:.2} (成交额: {:.2})",
                    snapshot.open, snapshot.high, snapshot.low, snapshot.turnover,
                );
            }
        }
    });

    consumer1.join().unwrap();
    consumer2.join().unwrap();
    consumer3.join().unwrap();

    let elapsed = start_time.elapsed();

    // 8. 统计信息
    println!("\n8️⃣  测试统计:");
    println!("   总快照数: {}", generator.get_snapshot_count());
    println!("   运行时长: {:.2}s", elapsed.as_secs_f64());
    println!(
        "   快照频率: ~{:.1}/s",
        generator.get_snapshot_count() as f64 / elapsed.as_secs_f64()
    );

    println!("\n✅ 测试完成！\n");
}
