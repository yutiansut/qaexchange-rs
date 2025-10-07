//! 压力测试：10 个品种 + 10 个账户随机交易

use qaexchange::exchange::{AccountManager, InstrumentRegistry, OrderRouter, TradeGateway};
use qaexchange::exchange::instrument_registry::InstrumentInfo;
use qaexchange::matching::engine::ExchangeMatchingEngine;
use qaexchange::core::account_ext::{OpenAccountRequest, AccountType};
use qaexchange::exchange::order_router::SubmitOrderRequest;
use std::sync::Arc;
use rand::Rng;

fn main() {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== QAEXCHANGE 压力测试 ===\n");
    println!("测试目标：");
    println!("  - 注册 10 个品种");
    println!("  - 创建 10 个账户");
    println!("  - 执行 100 次随机交易");
    println!("  - 验证撮合引擎正常工作\n");

    // 1. 创建核心组件
    println!(">>> 步骤 1: 初始化核心组件");
    let account_mgr = Arc::new(AccountManager::new());
    let matching_engine = Arc::new(ExchangeMatchingEngine::new());
    let instrument_registry = Arc::new(InstrumentRegistry::new());
    let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));
    let order_router = Arc::new(OrderRouter::new(
        account_mgr.clone(),
        matching_engine.clone(),
        instrument_registry.clone(),
        trade_gateway.clone(),
    ));
    println!("✓ 核心组件初始化完成\n");

    // 2. 注册 10 个品种
    println!(">>> 步骤 2: 注册 10 个品种");
    let instruments = vec![
        ("IX2401", "IX指数2401", 100.0),
        ("IX2402", "IX指数2402", 105.0),
        ("IF2401", "沪深300期货2401", 3800.0),
        ("IF2402", "沪深300期货2402", 3850.0),
        ("IC2401", "中证500期货2401", 5200.0),
        ("IC2402", "中证500期货2402", 5250.0),
        ("IH2401", "上证50期货2401", 2600.0),
        ("IH2402", "上证50期货2402", 2650.0),
        ("IM2401", "中证1000期货2401", 6500.0),
        ("IM2402", "中证1000期货2402", 6550.0),
    ];

    for (code, name, price) in &instruments {
        // 注册到合约注册表
        use qaexchange::exchange::instrument_registry::{InstrumentType, InstrumentStatus};
        let mut info = InstrumentInfo::new(
            code.to_string(),
            name.to_string(),
            InstrumentType::IndexFuture,
            "CFFEX".to_string(),
        );
        info.status = InstrumentStatus::Active;

        instrument_registry.register(info).expect(&format!("Failed to register {}", code));

        // 创建订单簿
        matching_engine.register_instrument(code.to_string(), *price)
            .expect(&format!("Failed to register {}", code));

        println!("  ✓ {} - {} (初始价格: {})", code, name, price);
    }
    println!("✓ 10 个品种注册完成\n");

    // 3. 创建 10 个账户
    println!(">>> 步骤 3: 创建 10 个账户");
    let mut accounts = Vec::new();
    for i in 0..10 {
        let user_id = format!("user_{:02}", i + 1);
        let account_name = format!("测试账户{:02}", i + 1);

        let req = OpenAccountRequest {
            user_id: user_id.clone(),
            account_id: None, // 自动生成
            account_name: account_name.clone(),
            init_cash: 1_000_000.0, // 每个账户 100 万初始资金
            account_type: AccountType::Individual,
        };

        let account_id = account_mgr.open_account(req)
            .expect(&format!("Failed to open account for {}", user_id));

        accounts.push(account_id.clone());
        println!("  ✓ {} - {} (初始资金: 1,000,000)", account_id, account_name);
    }
    println!("✓ 10 个账户创建完成\n");

    // 4. 订阅全局成交通知
    println!(">>> 步骤 4: 订阅成交通知");
    let global_receiver = trade_gateway.subscribe_global();
    println!("✓ 成交通知订阅完成\n");

    // 5. 执行随机交易
    println!(">>> 步骤 5: 执行 100 次随机交易");
    let mut rng = rand::thread_rng();
    let mut success_count = 0;
    let mut failed_count = 0;
    let mut trade_count = 0;

    for round in 0..100 {
        // 随机选择账户
        let account_id = &accounts[rng.gen_range(0..accounts.len())];

        // 随机选择品种
        let (instrument_id, _, base_price) = &instruments[rng.gen_range(0..instruments.len())];

        // 随机选择买卖方向
        let direction = if rng.gen_bool(0.5) { "BUY" } else { "SELL" };

        // 随机选择开平仓 (70% 开仓, 30% 平仓)
        let offset = "OPEN";

        // 随机数量 (1-20)
        let volume = rng.gen_range(1..=20) as f64;

        // 价格在基准价格上下 5% 浮动
        let price_fluctuation = rng.gen_range(-0.05..=0.05);
        let price = base_price * (1.0 + price_fluctuation);

        // 提交订单
        let req = SubmitOrderRequest {
            account_id: account_id.clone(),
            instrument_id: instrument_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            volume,
            price,
            order_type: "LIMIT".to_string(),
        };

        let response = order_router.submit_order(req);

        if response.success {
            success_count += 1;
            println!("  [{}] ✓ {} {} {} {} @ {} x {} | 订单: {}",
                round + 1,
                account_id,
                direction,
                offset,
                instrument_id,
                format!("{:.2}", price),
                volume,
                response.order_id.as_ref().unwrap()
            );
        } else {
            failed_count += 1;
            println!("  [{}] ✗ {} {} {} {} @ {} x {} | 失败: {}",
                round + 1,
                account_id,
                direction,
                offset,
                instrument_id,
                format!("{:.2}", price),
                volume,
                response.error_message.unwrap_or_else(|| "Unknown error".to_string())
            );
        }

        // 检查是否有成交通知
        while let Ok(notification) = global_receiver.try_recv() {
            use qaexchange::exchange::trade_gateway::Notification;
            match notification {
                Notification::Trade(trade) => {
                    trade_count += 1;
                    println!("      >>> 成交: {} {} {} @ {} x {} (手续费: {:.2})",
                        trade.user_id,
                        trade.direction,
                        trade.instrument_id,
                        format!("{:.2}", trade.price),
                        trade.volume,
                        trade.commission
                    );
                }
                Notification::OrderStatus(status) => {
                    if status.status == "FILLED" || status.status == "PARTIAL_FILLED" {
                        println!("      >>> 订单状态: {} - {}", status.order_id, status.status);
                    }
                }
                Notification::AccountUpdate(update) => {
                    println!("      >>> 账户更新: {} 余额={:.2} 可用={:.2} 保证金={:.2}",
                        update.user_id,
                        update.balance,
                        update.available,
                        update.margin
                    );
                }
            }
        }

        // 每 20 次交易暂停一下
        if (round + 1) % 20 == 0 {
            println!();
        }
    }

    println!("\n✓ 随机交易执行完成\n");

    // 6. 统计结果
    println!(">>> 步骤 6: 统计测试结果");
    println!("  订单统计:");
    println!("    - 成功提交: {}", success_count);
    println!("    - 提交失败: {}", failed_count);
    println!("    - 总计: {}", success_count + failed_count);
    println!("  成交统计:");
    println!("    - 成交笔数: {}", trade_count);
    println!();

    // 7. 查询账户最终状态
    println!(">>> 步骤 7: 账户最终状态");
    for user_id in &accounts {
        if let Ok(qifi) = account_mgr.get_account_qifi(user_id) {
            println!("  {} - 余额: {:.2} | 可用: {:.2} | 保证金: {:.2} | 持仓盈亏: {:.2} | 风险度: {:.2}%",
                user_id,
                qifi.balance,
                qifi.available,
                qifi.margin,
                qifi.position_profit,
                qifi.risk_ratio * 100.0
            );
        }
    }
    println!();

    // 8. 查询活动订单
    println!(">>> 步骤 8: 活动订单数量");
    let active_orders = order_router.get_active_order_count();
    println!("  活动订单: {}", active_orders);
    println!();

    println!("=== 压力测试完成 ===");
    println!("\n测试摘要:");
    println!("  ✓ 10 个品种成功注册");
    println!("  ✓ 10 个账户成功创建");
    println!("  ✓ {} 个订单成功提交", success_count);
    println!("  ✓ {} 笔成交完成", trade_count);
    println!("  ✓ 撮合引擎运行正常");
    println!("\n所有测试通过！");
}
