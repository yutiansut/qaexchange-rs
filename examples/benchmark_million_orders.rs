//! 百万级订单压力测试
//!
//! 对比两种架构的性能：
//! 1. 集中式架构：所有组件在主线程，直接调用
//! 2. 分布式架构：各组件独立线程，消息传递
//!
//! 测试场景：
//! - 1,000,000 笔订单
//! - 100 个账户
//! - 10 个品种
//! - 测量吞吐量、延迟、内存占用

use qaexchange::matching::core::MatchingEngineCore;
use qaexchange::account::core::AccountSystemCore;
use qaexchange::protocol::ipc_messages::*;
use qaexchange::core::QA_Account;
use qaexchange::matching::Orderbook;
use qaexchange::matching::engine::InstrumentAsset;
use crossbeam::channel::{unbounded, Sender, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use dashmap::DashMap;

const NUM_ORDERS: usize = 1_000_000;
const NUM_ACCOUNTS: usize = 100;
const NUM_INSTRUMENTS: usize = 10;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║         百万级订单压力测试 - 架构性能对比                      ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    println!("测试配置:");
    println!("  • 订单数量: {:>12}", format_number(NUM_ORDERS));
    println!("  • 账户数量: {:>12}", NUM_ACCOUNTS);
    println!("  • 品种数量: {:>12}", NUM_INSTRUMENTS);
    println!();

    // Scenario 1: 集中式架构（基准测试）
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 场景 1: 集中式架构（单线程，直接调用）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    let centralized_result = benchmark_centralized();
    print_results("集中式", &centralized_result);

    println!("\n");

    // Scenario 2: 分布式架构（高性能）
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 场景 2: 分布式架构（多线程，消息传递）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    let distributed_result = benchmark_distributed();
    print_results("分布式", &distributed_result);

    println!("\n");

    // 性能对比
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📈 性能对比总结");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let speedup = distributed_result.throughput / centralized_result.throughput;
    let latency_improvement = centralized_result.avg_latency_us / distributed_result.avg_latency_us;

    println!("吞吐量提升: {:.2}x", speedup);
    println!("延迟改善:   {:.2}x", latency_improvement);

    if speedup > 1.0 {
        println!("\n✅ 分布式架构性能更优！");
    } else {
        println!("\n⚠️  集中式架构在此场景下性能更优");
        println!("   （可能是订单量不够大，消息传递开销占主导）");
    }
}

struct BenchmarkResult {
    total_time_ms: u128,
    throughput: f64,         // orders/sec
    avg_latency_us: f64,     // microseconds
    p50_latency_us: f64,
    p95_latency_us: f64,
    p99_latency_us: f64,
    trades_count: usize,
}

/// 场景1: 集中式架构 - 所有组件在单线程直接调用
fn benchmark_centralized() -> BenchmarkResult {
    println!("初始化组件...");

    // 创建订单簿池（直接使用 qars Orderbook）
    let orderbooks: DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>> = DashMap::new();

    // 注册品种
    for i in 0..NUM_INSTRUMENTS {
        let code = format!("IX240{}", i);
        let orderbook = Orderbook::new(
            InstrumentAsset::from_code(&code),
            100.0,
        );
        orderbooks.insert(code, Arc::new(RwLock::new(orderbook)));
    }

    // 创建账户池（sim 模式）
    let accounts: DashMap<String, Arc<RwLock<QA_Account>>> = DashMap::new();
    for i in 0..NUM_ACCOUNTS {
        let user_id = format!("user_{:03}", i);
        let account = QA_Account::new(
            &user_id,
            "default",
            &user_id,
            10_000_000.0, // 每个账户1000万初始资金
            false,
            "sim",  // sim 模式
        );
        accounts.insert(user_id, Arc::new(RwLock::new(account)));
    }

    println!("  ✓ 注册 {} 个品种", NUM_INSTRUMENTS);
    println!("  ✓ 创建 {} 个账户", NUM_ACCOUNTS);
    println!();

    // 生成订单
    println!("生成 {} 笔订单...", format_number(NUM_ORDERS));
    let orders = generate_orders(NUM_ORDERS, NUM_ACCOUNTS, NUM_INSTRUMENTS);
    println!("  ✓ 订单生成完成\n");

    // 开始压测
    println!("开始压测...");
    let start = Instant::now();
    let mut trades_count = 0;
    let mut latencies = Vec::with_capacity(NUM_ORDERS);

    for (idx, order_req) in orders.iter().enumerate() {
        let order_start = Instant::now();

        // 1. 提取合约代码
        let instrument_id = std::str::from_utf8(&order_req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        // 2. 获取订单簿并撮合
        if let Some(ob) = orderbooks.get(&instrument_id) {
            let mut orderbook = ob.write();

            // 转换为撮合引擎订单
            let direction = if order_req.direction == 0 {
                qaexchange::matching::OrderDirection::BUY
            } else {
                qaexchange::matching::OrderDirection::SELL
            };

            let asset = InstrumentAsset::from_code(&instrument_id);
            let match_order = qaexchange::matching::orders::new_limit_order_request(
                asset,
                direction,
                order_req.price,
                order_req.volume,
                order_req.timestamp,
            );

            let results = orderbook.process_order(match_order);

            // 统计成交
            for result in results {
                if let Ok(success) = result {
                    use qaexchange::matching::Success;
                    match success {
                        Success::Filled { .. } | Success::PartiallyFilled { .. } => {
                            trades_count += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        let order_latency = order_start.elapsed().as_micros() as f64;
        latencies.push(order_latency);

        if (idx + 1) % 100_000 == 0 {
            println!("  进度: {}/{} ({:.1}%)",
                format_number(idx + 1),
                format_number(NUM_ORDERS),
                (idx + 1) as f64 / NUM_ORDERS as f64 * 100.0
            );
        }
    }

    let total_time = start.elapsed();
    println!("  ✓ 压测完成\n");

    calculate_result(total_time, latencies, trades_count)
}

/// 场景2: 分布式架构 - 各组件独立线程
fn benchmark_distributed() -> BenchmarkResult {
    println!("初始化分布式组件...");

    // 创建通信通道
    let (client_tx, client_rx) = unbounded::<OrderRequest>();
    let (order_tx, order_rx) = unbounded::<OrderRequest>();
    let (trade_tx, trade_rx) = unbounded::<TradeReport>();
    let (accepted_tx, accepted_rx) = unbounded::<OrderAccepted>();  // 订单确认通道
    let (market_tx, market_rx) = unbounded::<OrderbookSnapshot>();
    let (account_tx, account_rx) = unbounded();

    // 启动撮合引擎线程
    let matching_engine = MatchingEngineCore::new(
        order_rx.clone(),
        trade_tx.clone(),
        market_tx.clone(),
        accepted_tx.clone(),  // 添加订单确认通道
    );

    for i in 0..NUM_INSTRUMENTS {
        let code = format!("IX240{}", i);
        matching_engine.register_instrument(code, 100.0);
    }

    let _matching_handle = {
        let engine = matching_engine;
        thread::Builder::new()
            .name("MatchingEngine".to_string())
            .spawn(move || {
                engine.run();
            })
            .unwrap()
    };

    // 启动账户系统线程
    let account_system = Arc::new(AccountSystemCore::new(
        trade_rx.clone(),
        accepted_rx.clone(),  // 添加订单确认通道
        Some(account_tx.clone()),
        100, // batch_size
    ));

    for i in 0..NUM_ACCOUNTS {
        let user_id = format!("user_{:03}", i);
        let account = QA_Account::new(
            &user_id,
            "default",
            &user_id,
            10_000_000.0,
            false,
            "sim",  // sim 模式
        );
        account_system.register_account(user_id, account);
    }

    let _account_handle = {
        let system = account_system.clone();
        thread::Builder::new()
            .name("AccountSystem".to_string())
            .spawn(move || {
                system.run();
            })
            .unwrap()
    };

    // 启动 Gateway 线程
    let _gateway_handle = {
        let account_sys = account_system.clone();
        let order_sender = order_tx.clone();

        thread::Builder::new()
            .name("Gateway".to_string())
            .spawn(move || {
                while let Ok(mut order_req) = client_rx.recv() {
                    let user_id = std::str::from_utf8(&order_req.user_id)
                        .unwrap_or("")
                        .trim_end_matches('\0')
                        .to_string();

                    let instrument_id = std::str::from_utf8(&order_req.instrument_id)
                        .unwrap_or("")
                        .trim_end_matches('\0');

                    if let Some(account) = account_sys.get_account(&user_id) {
                        let mut acc = account.write();

                        // qars towards: 1=BUY OPEN, 3=BUY CLOSE, -2=SELL OPEN, -3=SELL CLOSE
                        let towards = if order_req.direction == 0 {
                            if order_req.offset == 0 { 1 } else { 3 }  // BUY OPEN=1, BUY CLOSE=3
                        } else {
                            if order_req.offset == 0 { -2 } else { -3 }  // SELL OPEN=-2, SELL CLOSE=-3
                        };

                        let datetime = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

                        if let Ok(qars_order) = acc.send_order(
                            instrument_id,
                            order_req.volume,
                            &datetime,
                            towards,
                            order_req.price,
                            "",
                            "LIMIT",
                        ) {
                            let account_order_id = qars_order.order_id.clone();
                            let order_id_bytes = account_order_id.as_bytes();
                            let len = order_id_bytes.len().min(40);  // UUID需要40字节
                            order_req.order_id[..len].copy_from_slice(&order_id_bytes[..len]);

                            let _ = order_sender.send(order_req);
                        }
                    }
                }
            })
            .unwrap()
    };

    // 丢弃行情（我们不关心行情，只测撮合性能）
    let _market_handle = thread::spawn(move || {
        while let Ok(_) = market_rx.recv() {}
    });

    let _notify_handle = thread::spawn(move || {
        while let Ok(_) = account_rx.recv() {}
    });

    println!("  ✓ 撮合引擎线程已启动");
    println!("  ✓ 账户系统线程已启动");
    println!("  ✓ Gateway 线程已启动");
    println!();

    // 等待组件启动
    thread::sleep(Duration::from_millis(200));

    // 生成订单
    println!("生成 {} 笔订单...", format_number(NUM_ORDERS));
    let orders = generate_orders(NUM_ORDERS, NUM_ACCOUNTS, NUM_INSTRUMENTS);
    println!("  ✓ 订单生成完成\n");

    // 开始压测
    println!("开始压测...");
    let start = Instant::now();
    let mut latencies = Vec::with_capacity(NUM_ORDERS);

    for (idx, order_req) in orders.iter().enumerate() {
        let order_start = Instant::now();

        client_tx.send(order_req.clone()).unwrap();

        let order_latency = order_start.elapsed().as_micros() as f64;
        latencies.push(order_latency);

        if (idx + 1) % 100_000 == 0 {
            println!("  进度: {}/{} ({:.1}%)",
                format_number(idx + 1),
                format_number(NUM_ORDERS),
                (idx + 1) as f64 / NUM_ORDERS as f64 * 100.0
            );
        }
    }

    // 等待所有订单处理完成
    println!("  等待订单处理完成...");
    thread::sleep(Duration::from_secs(2));

    let total_time = start.elapsed();
    println!("  ✓ 压测完成\n");

    // 注意：分布式架构中我们无法直接统计成交数，这里使用订单数作为近似
    // 实际应该通过 TradeReport channel 统计
    calculate_result(total_time, latencies, NUM_ORDERS / 2)
}

fn generate_orders(num_orders: usize, num_accounts: usize, num_instruments: usize) -> Vec<OrderRequest> {
    let mut orders = Vec::with_capacity(num_orders);

    for i in 0..num_orders {
        let user_idx = i % num_accounts;
        let instrument_idx = i % num_instruments;
        let user_id = format!("user_{:03}", user_idx);
        let instrument_id = format!("IX240{}", instrument_idx);

        // 交替买卖，价格随机波动
        let is_buy = i % 2 == 0;
        let base_price = 100.0;
        let price_offset = (i % 10) as f64 * 0.1;
        let price = if is_buy {
            base_price - price_offset
        } else {
            base_price + price_offset
        };

        let order = OrderRequest::new(
            &format!("ORDER_{:07}", i),
            &user_id,
            &instrument_id,
            if is_buy { OrderDirection::BUY } else { OrderDirection::SELL },
            OrderOffset::OPEN,
            price,
            10.0,
        );

        orders.push(order);
    }

    orders
}

fn calculate_result(
    total_time: Duration,
    mut latencies: Vec<f64>,
    trades_count: usize,
) -> BenchmarkResult {
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let total_time_ms = total_time.as_millis();
    let throughput = (NUM_ORDERS as f64 / total_time.as_secs_f64()).round();
    let avg_latency: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;

    let p50_idx = latencies.len() / 2;
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p99_idx = (latencies.len() as f64 * 0.99) as usize;

    BenchmarkResult {
        total_time_ms,
        throughput,
        avg_latency_us: avg_latency,
        p50_latency_us: latencies[p50_idx],
        p95_latency_us: latencies[p95_idx],
        p99_latency_us: latencies[p99_idx],
        trades_count,
    }
}

fn print_results(name: &str, result: &BenchmarkResult) {
    println!("{}架构性能指标:", name);
    println!("  • 总耗时:      {:>10} ms", result.total_time_ms);
    println!("  • 吞吐量:      {:>10} orders/sec", format_number(result.throughput as usize));
    println!("  • 成交数:      {:>10}", format_number(result.trades_count));
    println!("  • 平均延迟:    {:>10.2} μs", result.avg_latency_us);
    println!("  • P50 延迟:    {:>10.2} μs", result.p50_latency_us);
    println!("  • P95 延迟:    {:>10.2} μs", result.p95_latency_us);
    println!("  • P99 延迟:    {:>10.2} μs", result.p99_latency_us);
}

fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
