//! 高性能交易所架构演示
//!
//! 演示撮合引擎、账户系统、行情系统的独立运行和通信
//!
//! 架构：
//! ```
//! Gateway Thread
//!     ↓ (channel)
//! MatchingEngineCore Thread
//!     ↓ (channel)
//! ┌───────────────┬──────────────┐
//! │               │              │
//! AccountCore   MarketData   TradeGateway
//! Thread        Thread       Thread
//! ```

use crossbeam::channel::{unbounded, Receiver, Sender};
use qaexchange::account::core::AccountSystemCore;
use qaexchange::core::QA_Account;
use qaexchange::matching::core::MatchingEngineCore;
use qaexchange::protocol::ipc_messages::{
    OrderAccepted, OrderDirection, OrderOffset, OrderRequest, OrderbookSnapshot, TradeReport,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== 高性能交易所架构演示 ===\n");
    println!("架构特点：");
    println!("  ✓ 撮合引擎独立线程");
    println!("  ✓ 账户系统独立线程");
    println!("  ✓ 零拷贝消息传递");
    println!("  ✓ 批量账户更新\n");

    // 1. 创建通信通道
    //
    // 正确的交易所架构流程：
    // Client → Gateway → AccountSystem.send_order() (生成order_id, 校验资金, 冻结资金)
    //                 → MatchingEngine.process(order_id) (撮合)
    //                 → AccountSystem.receive_deal_sim(order_id) (更新持仓)
    //
    // 关键点：
    // 1. 订单先经过账户系统，生成 order_id 并记录到 dailyorders
    // 2. 撮合引擎只负责撮合，不关心账户状态
    // 3. 成交回报通过 order_id 匹配回原始订单
    let (client_tx, client_rx) = unbounded::<OrderRequest>(); // Client → Gateway
    let (order_tx, order_rx) = unbounded::<OrderRequest>(); // Gateway → MatchingEngine (已通过风控)
    let (trade_tx, trade_rx) = unbounded::<TradeReport>(); // MatchingEngine → AccountSystem
    let (accepted_tx, accepted_rx) = unbounded::<OrderAccepted>(); // MatchingEngine → AccountSystem (订单确认)
    let (market_tx, market_rx) = unbounded::<OrderbookSnapshot>(); // MatchingEngine → MarketData
    let (account_tx, account_rx) = unbounded(); // AccountSystem → Client

    // 2. 启动撮合引擎线程
    println!(">>> 启动撮合引擎线程");
    let matching_engine = MatchingEngineCore::new(
        order_rx.clone(),
        trade_tx.clone(),
        market_tx.clone(),
        accepted_tx.clone(),
    );

    // 注册品种
    matching_engine.register_instrument("IX2401".to_string(), 100.0);
    matching_engine.register_instrument("IF2401".to_string(), 3800.0);
    println!("  ✓ 注册 2 个品种");

    let matching_handle = {
        let engine = matching_engine;
        thread::Builder::new()
            .name("MatchingEngine".to_string())
            .spawn(move || {
                engine.run();
            })
            .unwrap()
    };

    // 3. 启动账户系统线程
    println!(">>> 启动账户系统线程");
    let account_system = Arc::new(AccountSystemCore::new(
        trade_rx.clone(),
        accepted_rx.clone(),
        Some(account_tx.clone()),
        10, // batch_size
    ));

    // 注册账户（sim 模式）
    for i in 0..5 {
        let user_id = format!("user_{:02}", i + 1);
        let account = QA_Account::new(
            &user_id,
            "default",
            &user_id,
            1_000_000.0,
            false,
            "sim", // sim 模式
        );
        account_system.register_account(user_id.clone(), account);
    }
    println!("  ✓ 注册 5 个账户（sim 模式）");

    let account_handle = {
        let system = account_system.clone();
        thread::Builder::new()
            .name("AccountSystem".to_string())
            .spawn(move || {
                system.run();
            })
            .unwrap()
    };

    // 4. 启动 Gateway 线程（订单路由）
    println!(">>> 启动 Gateway 线程（订单路由）");
    let gateway_handle = {
        let account_sys = account_system.clone();
        let order_sender = order_tx.clone();

        thread::Builder::new()
            .name("Gateway".to_string())
            .spawn(move || {
                while let Ok(mut order_req) = client_rx.recv() {
                    // 提取用户信息
                    let user_id = std::str::from_utf8(&order_req.user_id)
                        .unwrap_or("")
                        .trim_end_matches('\0')
                        .to_string();

                    let instrument_id = std::str::from_utf8(&order_req.instrument_id)
                        .unwrap_or("")
                        .trim_end_matches('\0');

                    // 关键：先通过账户系统 send_order，生成 order_id 并冻结资金
                    if let Some(account) = account_sys.get_account(&user_id) {
                        let mut acc = account.write();

                        let towards = if order_req.direction == 0 {
                            if order_req.offset == 0 {
                                1
                            } else {
                                3
                            } // BUY OPEN=1, BUY CLOSE=3
                        } else {
                            if order_req.offset == 0 {
                                -2
                            } else {
                                -3
                            } // SELL OPEN=-2, SELL CLOSE=-3
                        };

                        let datetime = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

                        // send_order: 校验资金 + 冻结 + 生成 order_id
                        match acc.send_order(
                            instrument_id,
                            order_req.volume,
                            &datetime,
                            towards,
                            order_req.price,
                            "",
                            "LIMIT",
                        ) {
                            Ok(qars_order) => {
                                // 获取账户生成的 order_id
                                let account_order_id = qars_order.order_id.clone();

                                // 将 order_id 写入 OrderRequest（用于撮合引擎和回报匹配）
                                let order_id_bytes = account_order_id.as_bytes();
                                let len = order_id_bytes.len().min(40);
                                order_req.order_id[..len].copy_from_slice(&order_id_bytes[..len]);

                                println!(
                                    "  [Gateway] {} 订单已创建: {} (冻结资金完成)",
                                    user_id, account_order_id
                                );

                                // 发送到撮合引擎
                                let _ = order_sender.send(order_req);
                            }
                            Err(e) => {
                                println!("  [Gateway] {} 订单被拒绝: {:?}", user_id, e);
                            }
                        }
                    }
                }
            })
            .unwrap()
    };

    // 4. 启动行情推送线程（简化版）
    println!(">>> 启动行情推送线程");
    let market_handle = thread::Builder::new()
        .name("MarketData".to_string())
        .spawn(move || {
            while let Ok(snapshot) = market_rx.recv() {
                let instrument_id = std::str::from_utf8(&snapshot.instrument_id)
                    .unwrap_or("")
                    .trim_end_matches('\0');

                println!(
                    "      [行情] {} - 买1: {:.2}@{:.0} | 卖1: {:.2}@{:.0}",
                    instrument_id,
                    snapshot.bids[0].price,
                    snapshot.bids[0].volume,
                    snapshot.asks[0].price,
                    snapshot.asks[0].volume
                );
            }
        })
        .unwrap();

    // 5. 监听账户更新通知
    println!(">>> 启动账户更新监听线程");
    let notify_handle = thread::spawn(move || {
        while let Ok(notify) = account_rx.recv() {
            println!(
                "      [账户] {} - 余额: {:.2} | 保证金: {:.2}",
                notify.user_id, notify.balance, notify.margin
            );
        }
    });

    // 等待所有组件启动
    thread::sleep(Duration::from_millis(100));
    println!("\n=== 所有组件已启动 ===\n");

    // 6. 模拟发送订单（发送到 Gateway，而不是直接到撮合引擎）
    println!("\n>>> 发送测试订单");
    println!("策略：完整的开平仓流程演示\n");

    // 阶段1: user_01/02/03 买入开多仓
    println!("【阶段1: 开多仓】");
    for i in 0..3 {
        let order = OrderRequest::new(
            &format!("CLIENT_ORDER_BUY_{:02}", i + 1),
            &format!("user_{:02}", i + 1),
            "IX2401",
            OrderDirection::BUY,
            OrderOffset::OPEN, // ← 开多仓
            100.0 - i as f64 * 0.1,
            10.0,
        );
        client_tx.send(order).unwrap();
        println!(
            "  ✓ [Client] user_{:02} BUY OPEN IX2401 @ {:.2} x 10 (建立多头持仓)",
            i + 1,
            100.0 - i as f64 * 0.1
        );
    }

    // 阶段2: user_04/05 卖出开空仓（会触发撮合）
    println!("\n【阶段2: 开空仓 + 撮合】");
    for i in 3..5 {
        let order = OrderRequest::new(
            &format!("CLIENT_ORDER_SELL_{:02}", i + 1),
            &format!("user_{:02}", i + 1),
            "IX2401",
            OrderDirection::SELL,
            OrderOffset::OPEN, // ← 开空仓
            100.0 - (i - 3) as f64 * 0.1,
            10.0,
        );
        client_tx.send(order).unwrap();
        println!(
            "  ✓ [Client] user_{:02} SELL OPEN IX2401 @ {:.2} x 10 (建立空头持仓)",
            i + 1,
            100.0 - (i - 3) as f64 * 0.1
        );
    }

    println!("\n等待开仓成交...");
    thread::sleep(Duration::from_millis(500));

    // 阶段3: user_01 卖出平多（平掉之前的多头持仓）
    println!("\n【阶段3: 平多仓】");
    let order = OrderRequest::new(
        "CLIENT_ORDER_CLOSE_01",
        "user_01",
        "IX2401",
        OrderDirection::SELL,
        OrderOffset::CLOSE, // ← 平多仓（需要先有多头持仓）
        100.5,              // 高于开仓价，盈利平仓
        10.0,
    );
    client_tx.send(order).unwrap();
    println!("  ✓ [Client] user_01 SELL CLOSE IX2401 @ 100.50 x 10 (平掉多头持仓，盈利!)");

    // 阶段4: user_04 买入平空（平掉之前的空头持仓）
    println!("\n【阶段4: 平空仓】");
    let order = OrderRequest::new(
        "CLIENT_ORDER_CLOSE_04",
        "user_04",
        "IX2401",
        OrderDirection::BUY,
        OrderOffset::CLOSE, // ← 平空仓（需要先有空头持仓）
        99.5,               // 低于开仓价，盈利平仓
        10.0,
    );
    client_tx.send(order).unwrap();
    println!("  ✓ [Client] user_04 BUY CLOSE IX2401 @ 99.50 x 10 (平掉空头持仓，盈利!)");

    println!("\n=== 等待成交处理 ===\n");

    // 等待成交处理
    thread::sleep(Duration::from_secs(2));

    println!("\n=== 演示完成 ===\n");
    println!("架构优势：");
    println!("  ✓ 撮合引擎不阻塞账户更新");
    println!("  ✓ 账户系统异步批量处理");
    println!("  ✓ 行情推送实时无延迟");
    println!("  ✓ 各组件独立可扩展\n");

    println!("下一步优化：");
    println!("  → 替换 crossbeam channel 为 iceoryx2 共享内存");
    println!("  → 添加 CPU 亲和性绑定");
    println!("  → 实现账户分片（减少锁竞争）");
    println!("  → 添加 WAL 日志（保证数据安全）");
}
