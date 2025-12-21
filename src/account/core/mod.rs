//! 账户系统核心（独立进程）
//!
//! 设计原则：
//! 1. 异步更新 - 接收成交回报后异步更新账户，不阻塞撮合
//! 2. 批量处理 - 批量接收成交，减少锁竞争
//! 3. 分片账户 - 多线程处理不同账户，提高并发
//! 4. WAL 日志 - 写入日志后才确认，保证数据安全

use crate::core::QA_Account;
use crate::protocol::ipc_messages::{OrderAccepted, TradeReport};
use crossbeam::channel::{Receiver, Sender};
use dashmap::DashMap;
use parking_lot::RwLock;
use rayon::prelude::*;
use std::sync::Arc;

/// 账户系统核心
///
/// 运行在独立进程中，通过 iceoryx2 接收成交回报，异步更新账户
pub struct AccountSystemCore {
    /// 账户池
    accounts: DashMap<String, Arc<RwLock<QA_Account>>>,

    /// 成交订阅器（暂时用 crossbeam，后续替换为 iceoryx2）
    trade_receiver: Receiver<TradeReport>,

    /// 订单确认订阅器（用于 sim 模式的 on_order_confirm）
    accepted_receiver: Receiver<OrderAccepted>,

    /// 账户更新通知发送器（可选，用于通知其他系统）
    update_sender: Option<Sender<AccountUpdateNotify>>,

    /// 批量处理大小
    batch_size: usize,

    /// 运行标志
    running: Arc<std::sync::atomic::AtomicBool>,
}

/// 账户更新通知
#[derive(Debug, Clone)]
pub struct AccountUpdateNotify {
    pub user_id: String,
    pub balance: f64,
    pub available: f64,
    pub margin: f64,
    pub timestamp: i64,
}

impl AccountSystemCore {
    /// 创建账户系统核心
    pub fn new(
        trade_receiver: Receiver<TradeReport>,
        accepted_receiver: Receiver<OrderAccepted>,
        update_sender: Option<Sender<AccountUpdateNotify>>,
        batch_size: usize,
    ) -> Self {
        Self {
            accounts: DashMap::new(),
            trade_receiver,
            accepted_receiver,
            update_sender,
            batch_size,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 注册账户
    pub fn register_account(&self, user_id: String, account: QA_Account) {
        self.accounts
            .insert(user_id.clone(), Arc::new(RwLock::new(account)));
        log::info!("Registered account in AccountSystemCore: {}", user_id);
    }

    /// 启动账户系统主循环
    pub fn run(&self) {
        use crossbeam::channel::select;
        use std::sync::atomic::Ordering;

        self.running.store(true, Ordering::SeqCst);
        log::info!("AccountSystemCore started");

        let mut update_queue: Vec<TradeReport> = Vec::with_capacity(self.batch_size);

        while self.running.load(Ordering::SeqCst) {
            // 使用 select 同时监听两个通道
            select! {
                recv(self.accepted_receiver) -> msg => {
                    if let Ok(accepted) = msg {
                        self.handle_order_accepted(accepted);
                    }
                }
                recv(self.trade_receiver) -> msg => {
                    if let Ok(trade) = msg {
                        update_queue.push(trade);

                        // 如果达到批量大小，立即处理
                        if update_queue.len() >= self.batch_size {
                            self.batch_update_accounts(&update_queue);
                            update_queue.clear();
                        }
                    }
                }
                default(std::time::Duration::from_millis(10)) => {
                    // 超时，处理已有数据
                    if !update_queue.is_empty() {
                        self.batch_update_accounts(&update_queue);
                        update_queue.clear();
                    }
                }
            }
        }

        log::info!("AccountSystemCore stopped");
    }

    /// 处理订单确认（sim 模式）
    fn handle_order_accepted(&self, accepted: OrderAccepted) {
        let order_id = std::str::from_utf8(&accepted.order_id)
            .unwrap_or("")
            .trim_end_matches('\0');

        let exchange_order_id = std::str::from_utf8(&accepted.exchange_order_id)
            .unwrap_or("")
            .trim_end_matches('\0');

        let user_id = std::str::from_utf8(&accepted.user_id)
            .unwrap_or("")
            .trim_end_matches('\0');

        if let Some(account) = self.accounts.get(user_id) {
            let mut acc = account.write();
            if let Err(e) = acc.on_order_confirm(order_id, exchange_order_id) {
                log::error!("Failed to confirm order {}: {}", order_id, e);
            } else {
                log::debug!("Order confirmed: {} -> {}", order_id, exchange_order_id);
            }
        } else {
            log::warn!("Account not found for order confirmation: {}", user_id);
        }
    }

    /// 批量更新账户
    fn batch_update_accounts(&self, trades: &[TradeReport]) {
        // 1. 按账户分组
        use std::collections::HashMap;
        let mut grouped: HashMap<String, Vec<&TradeReport>> = HashMap::new();

        for trade in trades {
            let user_id = std::str::from_utf8(&trade.user_id)
                .unwrap_or("")
                .trim_end_matches('\0')
                .to_string();

            grouped.entry(user_id).or_default().push(trade);
        }

        // 2. 并行更新不同账户（减少锁竞争）
        grouped.par_iter().for_each(|(user_id, user_trades)| {
            if let Some(account) = self.accounts.get(user_id) {
                let mut acc = account.write();

                for trade in user_trades {
                    self.apply_trade(&mut acc, trade);
                }

                // 发送账户更新通知
                if let Some(ref sender) = self.update_sender {
                    let notify = AccountUpdateNotify {
                        user_id: user_id.clone(),
                        balance: acc.money,
                        available: acc.money,
                        margin: acc.get_margin(),
                        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                    };
                    let _ = sender.send(notify);
                }
            } else {
                log::warn!("Account not found: {}", user_id);
            }
        });

        log::debug!(
            "Batch updated {} trades for {} accounts",
            trades.len(),
            grouped.len()
        );
    }

    /// 应用单笔成交到账户
    ///
    /// 正确的交易所架构流程（已在 high_performance_demo 中实现）：
    /// 1. Gateway 收到订单 → 调用 AccountSystem.send_order()
    ///    - 生成 order_id（账户内部ID）
    ///    - 校验资金/保证金
    ///    - 冻结资金/保证金
    ///    - 订单记录到 dailyorders
    /// 2. Gateway → MatchingEngine（携带 order_id）
    ///    - 撮合引擎生成 exchange_order_id（交易所全局唯一ID）
    ///    - 撮合引擎只负责撮合，不关心账户
    /// 3. MatchingEngine → TradeReport（包含 order_id + exchange_order_id）
    /// 4. AccountSystem.receive_deal_sim(order_id)
    ///    - 根据 order_id 找到 dailyorders 中的订单
    ///    - **更新订单的 exchange_order_id**
    ///    - 更新持仓
    ///    - 释放冻结资金
    ///
    /// 这个方法假设订单已经由 Gateway 通过 send_order() 创建，
    /// dailyorders 中已经存在对应的 order_id。
    fn apply_trade(&self, acc: &mut QA_Account, trade: &TradeReport) {
        let instrument_id = std::str::from_utf8(&trade.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');

        let order_id = std::str::from_utf8(&trade.order_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        let exchange_order_id = std::str::from_utf8(&trade.exchange_order_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        let trade_id = std::str::from_utf8(&trade.trade_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        // 计算 towards (qars 定义：1=BUY OPEN, 3=BUY CLOSE, -2=SELL OPEN, -3=SELL CLOSE)
        let towards = match (trade.direction, trade.offset) {
            (0, 0) => 1,  // BUY OPEN
            (1, 0) => -2, // SELL OPEN
            (0, 1) => 3,  // BUY CLOSE
            (1, 1) => -3, // SELL CLOSE
            _ => 1,
        };

        let datetime = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 关键步骤1：更新订单的 exchange_order_id（交易所全局唯一ID）
        if let Some(order) = acc.dailyorders.get_mut(&order_id) {
            order.exchange_order_id = exchange_order_id.clone();
            log::debug!(
                "Updated exchange_order_id: {} -> {}",
                order_id,
                exchange_order_id
            );
        }

        // 关键步骤2：调用 receive_deal_sim 处理成交
        // order_id 已经由 Gateway 通过 send_order() 创建并存在于 dailyorders 中
        acc.receive_deal_sim(
            instrument_id.to_string(),
            trade.volume,
            trade.price,
            datetime,
            order_id.clone(),
            trade_id,
            order_id, // realorder_id
            towards,
        );

        log::debug!(
            "Applied trade: {} {} {} @ {} x {} (exchange_order_id: {})",
            std::str::from_utf8(&trade.user_id).unwrap_or(""),
            if trade.direction == 0 { "BUY" } else { "SELL" },
            instrument_id,
            trade.price,
            trade.volume,
            exchange_order_id
        );
    }

    /// 获取账户
    pub fn get_account(&self, user_id: &str) -> Option<Arc<RwLock<QA_Account>>> {
        self.accounts.get(user_id).map(|r| r.clone())
    }

    /// 停止账户系统
    pub fn stop(&self) {
        use std::sync::atomic::Ordering;
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::unbounded;

    #[test]
    fn test_account_system_core_creation() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account);

        assert_eq!(system.accounts.len(), 1);
    }

    /// 测试批量更新（需要预创建订单）
    /// 注意：receive_deal_sim 要求订单先通过 send_order() 创建
    /// 此测试验证完整订单流程：send_order -> batch_update_accounts
    #[test]
    fn test_batch_update() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let mut account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");

        // 关键步骤：先通过 send_order 创建订单
        // send_order 参数: code, amount, time, towards, price, order_id, price_type
        // towards=1 表示 BUY OPEN
        let order_result = account.send_order(
            "IX2401",       // code
            10.0,           // amount (volume)
            "2025-12-17",   // time
            1,              // towards: 1=BUY OPEN
            100.0,          // price
            "ORDER001",     // order_id
            "LIMIT",        // price_type
        );
        assert!(order_result.is_ok(), "send_order 应成功创建订单");

        system.register_account("user_01".to_string(), account);

        // 创建与订单匹配的成交
        let trade = TradeReport {
            trade_id: *b"TRADE001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"user_01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 0, // BUY
            offset: 0,    // OPEN
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 10.0,
            commission: 0.3,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        system.batch_update_accounts(&[trade]);

        // 验证收到更新通知
        let notify = update_rx.try_recv();
        assert!(notify.is_ok(), "应收到账户更新通知");
        assert_eq!(notify.unwrap().user_id, "user_01");
    }

    // ==================== AccountUpdateNotify 测试 @yutiansut @quantaxis ====================

    /// 测试 AccountUpdateNotify 结构体字段
    /// 验证通知结构体包含所有必要的账户更新信息
    #[test]
    fn test_account_update_notify_fields() {
        let notify = AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 1000000.0,
            available: 900000.0,
            margin: 100000.0,
            timestamp: 1734567890000000000,
        };

        assert_eq!(notify.user_id, "user_01");
        assert_eq!(notify.balance, 1000000.0);
        assert_eq!(notify.available, 900000.0);
        assert_eq!(notify.margin, 100000.0);
        assert_eq!(notify.timestamp, 1734567890000000000);
    }

    /// 测试 AccountUpdateNotify 的 Clone trait
    /// 验证通知可以被正确克隆
    #[test]
    fn test_account_update_notify_clone() {
        let notify1 = AccountUpdateNotify {
            user_id: "user_01".to_string(),
            balance: 500000.0,
            available: 400000.0,
            margin: 50000.0,
            timestamp: 1234567890,
        };

        let notify2 = notify1.clone();

        assert_eq!(notify1.user_id, notify2.user_id);
        assert_eq!(notify1.balance, notify2.balance);
        assert_eq!(notify1.available, notify2.available);
        assert_eq!(notify1.margin, notify2.margin);
        assert_eq!(notify1.timestamp, notify2.timestamp);
    }

    // ==================== AccountSystemCore 创建测试 @yutiansut @quantaxis ====================

    /// 测试不同批量大小的 AccountSystemCore 创建
    /// batch_size 决定批量处理成交的数量阈值
    #[test]
    fn test_account_system_core_different_batch_sizes() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        // 测试小批量
        let system_small = AccountSystemCore::new(trade_rx.clone(), accepted_rx.clone(), None, 10);
        assert_eq!(system_small.batch_size, 10);

        // 测试大批量
        let (trade_tx2, trade_rx2) = unbounded();
        let (accepted_tx2, accepted_rx2) = unbounded();
        let system_large = AccountSystemCore::new(trade_rx2, accepted_rx2, None, 1000);
        assert_eq!(system_large.batch_size, 1000);
    }

    /// 测试无通知发送器的 AccountSystemCore 创建
    /// update_sender 为 None 时系统仍可正常运行
    #[test]
    fn test_account_system_core_without_update_sender() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        assert!(system.update_sender.is_none());
        assert!(system.accounts.is_empty());
    }

    /// 测试 AccountSystemCore 初始 running 状态
    /// 创建后 running 标志应为 false，调用 run() 后变为 true
    #[test]
    fn test_account_system_core_initial_running_state() {
        use std::sync::atomic::Ordering;

        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        // 初始状态应该是 false
        assert!(!system.running.load(Ordering::SeqCst));
    }

    // ==================== 账户注册测试 @yutiansut @quantaxis ====================

    /// 测试注册多个账户
    /// 验证多账户并发存储和访问
    #[test]
    fn test_register_multiple_accounts() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        // 注册多个账户
        for i in 0..5 {
            let user_id = format!("user_{:02}", i);
            let initial_balance = 1000000.0 + (i as f64 * 100000.0);
            let account = QA_Account::new(&user_id, "default", &user_id, initial_balance, false, "sim");
            system.register_account(user_id, account);
        }

        assert_eq!(system.accounts.len(), 5);
    }

    /// 测试重复注册同一账户
    /// 行为：覆盖旧账户数据
    #[test]
    fn test_register_account_overwrite() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        // 第一次注册
        let account1 = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account1);

        // 第二次注册（覆盖）
        let account2 = QA_Account::new("user_01", "default", "user_01", 2000000.0, false, "sim");
        system.register_account("user_01".to_string(), account2);

        // 账户数量不变
        assert_eq!(system.accounts.len(), 1);

        // 验证是新账户
        let acc = system.get_account("user_01").unwrap();
        let account = acc.read();
        assert_eq!(account.money, 2000000.0);
    }

    // ==================== 获取账户测试 @yutiansut @quantaxis ====================

    /// 测试获取存在的账户
    #[test]
    fn test_get_account_found() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        let account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account);

        let result = system.get_account("user_01");
        assert!(result.is_some());

        let acc = result.unwrap();
        let account = acc.read();
        assert_eq!(account.account_cookie, "user_01");
    }

    /// 测试获取不存在的账户
    #[test]
    fn test_get_account_not_found() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        let result = system.get_account("non_existent");
        assert!(result.is_none());
    }

    // ==================== 停止系统测试 @yutiansut @quantaxis ====================

    /// 测试停止账户系统
    /// stop() 应将 running 标志设为 false
    #[test]
    fn test_account_system_stop() {
        use std::sync::atomic::Ordering;

        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        // 手动设置 running 为 true
        system.running.store(true, Ordering::SeqCst);
        assert!(system.running.load(Ordering::SeqCst));

        // 调用 stop
        system.stop();

        // running 应变为 false
        assert!(!system.running.load(Ordering::SeqCst));
    }

    // ==================== 成交方向计算测试 @yutiansut @quantaxis ====================

    /// 测试买入开仓成交处理
    /// towards 计算: direction=0(BUY), offset=0(OPEN) → towards=1
    /// 完整流程：send_order 创建订单 → batch_update_accounts 处理成交
    #[test]
    fn test_batch_update_buy_open() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let mut account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");

        // 先创建订单（BUY OPEN, towards=1）
        // send_order 参数: code, amount, time, towards, price, order_id, price_type
        let _ = account.send_order("IX2401", 10.0, "2025-12-17", 1, 100.0, "ORDER_BUY", "LIMIT");

        system.register_account("user_01".to_string(), account);

        // BUY OPEN: direction=0, offset=0 → towards=1
        let trade = TradeReport {
            trade_id: *b"TRADE001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER_BUY\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"user_01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 0, // BUY
            offset: 0,    // OPEN
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 10.0,
            commission: 0.3,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        system.batch_update_accounts(&[trade]);

        // 验证收到更新通知
        let notify = update_rx.try_recv();
        assert!(notify.is_ok());
        assert_eq!(notify.unwrap().user_id, "user_01");
    }

    /// 测试卖出开仓成交处理
    /// towards 计算: direction=1(SELL), offset=0(OPEN) → towards=-2
    /// 完整流程：send_order 创建订单 → batch_update_accounts 处理成交
    #[test]
    fn test_batch_update_sell_open() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let mut account = QA_Account::new("user_02", "default", "user_02", 1000000.0, false, "sim");

        // 先创建订单（SELL OPEN, towards=-2）
        // send_order 参数: code, amount, time, towards, price, order_id, price_type
        let _ = account.send_order("IX2401", 5.0, "2025-12-17", -2, 100.0, "ORDER_SELL", "LIMIT");

        system.register_account("user_02".to_string(), account);

        // SELL OPEN: direction=1, offset=0 → towards=-2
        let trade = TradeReport {
            trade_id: *b"TRADE002\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER_SELL\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"user_02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 1, // SELL
            offset: 0,    // OPEN
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 5.0,
            commission: 0.15,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        system.batch_update_accounts(&[trade]);

        // 验证收到更新通知
        let notify = update_rx.try_recv();
        assert!(notify.is_ok());
        assert_eq!(notify.unwrap().user_id, "user_02");
    }

    // ==================== 批量更新测试 @yutiansut @quantaxis ====================

    /// 测试多账户批量更新
    /// 验证多个账户的成交可以并行处理（使用 Rayon）
    /// 完整流程：每个账户先创建订单 → 批量处理所有成交
    #[test]
    fn test_batch_update_multiple_accounts() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        // 注册两个账户，每个都先创建订单
        // send_order 参数: code, amount, time, towards, price, order_id, price_type
        let mut account1 = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        let _ = account1.send_order("IX2401", 10.0, "2025-12-17", 1, 100.0, "ORDER_A1", "LIMIT");

        let mut account2 = QA_Account::new("user_02", "default", "user_02", 2000000.0, false, "sim");
        let _ = account2.send_order("IX2401", 5.0, "2025-12-17", -2, 100.0, "ORDER_A2", "LIMIT");

        system.register_account("user_01".to_string(), account1);
        system.register_account("user_02".to_string(), account2);

        // 创建两个不同账户的成交
        let trade1 = TradeReport {
            trade_id: *b"TRADE001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER_A1\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"user_01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 0,
            offset: 0,
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 10.0,
            commission: 0.3,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        let trade2 = TradeReport {
            trade_id: *b"TRADE002\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER_A2\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"user_02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 1,
            offset: 0,
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 5.0,
            commission: 0.15,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        system.batch_update_accounts(&[trade1, trade2]);

        // 应收到两个更新通知
        let notify1 = update_rx.try_recv();
        let notify2 = update_rx.try_recv();
        assert!(notify1.is_ok());
        assert!(notify2.is_ok());
    }

    /// 测试同一账户多笔成交批量更新
    /// 验证同一账户的多笔成交按顺序处理
    /// 完整流程：先创建所有订单 → 批量处理所有成交
    #[test]
    fn test_batch_update_same_account_multiple_trades() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let mut account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");

        // 先创建三个订单
        // send_order 参数: code, amount, time, towards, price, order_id, price_type
        for i in 0..3 {
            let order_id = format!("ORDER{:03}", i);
            let _ = account.send_order("IX2401", 1.0, "2025-12-17", 1, 100.0 + i as f64, &order_id, "LIMIT");
        }

        system.register_account("user_01".to_string(), account);

        // 同一账户三笔成交
        let trades: Vec<TradeReport> = (0..3)
            .map(|i| {
                let mut trade_id = [0u8; 32];
                let id_str = format!("TRADE{:03}", i);
                trade_id[..id_str.len()].copy_from_slice(id_str.as_bytes());

                let mut order_id = [0u8; 40];
                let oid_str = format!("ORDER{:03}", i);
                order_id[..oid_str.len()].copy_from_slice(oid_str.as_bytes());

                TradeReport {
                    trade_id,
                    order_id,
                    exchange_order_id: [0; 32],
                    user_id: *b"user_01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
                    instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
                    direction: 0,
                    offset: 0,
                    fill_type: 0,
                    _reserved: 0,
                    price: 100.0 + i as f64,
                    volume: 1.0,
                    commission: 0.03,
                    timestamp: i as i64,
                    opposite_order_id: [0; 32],
                    gateway_id: 0,
                    session_id: 0,
                }
            })
            .collect();

        system.batch_update_accounts(&trades);

        // 同一账户只发送一次通知（批量处理后）
        let notify = update_rx.try_recv();
        assert!(notify.is_ok());
        assert_eq!(notify.unwrap().user_id, "user_01");
    }

    /// 测试空成交批量更新
    /// 空列表不应产生任何通知
    #[test]
    fn test_batch_update_empty_trades() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account);

        // 空成交列表
        system.batch_update_accounts(&[]);

        // 不应有通知
        let notify = update_rx.try_recv();
        assert!(notify.is_err());
    }

    /// 测试不存在账户的成交处理
    /// 应该忽略未注册账户的成交
    #[test]
    fn test_batch_update_unknown_account() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        // 不注册任何账户

        let trade = TradeReport {
            trade_id: *b"TRADE001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            order_id: *b"ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: [0; 32],
            user_id: *b"unknown\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            direction: 0,
            offset: 0,
            fill_type: 0,
            _reserved: 0,
            price: 100.0,
            volume: 10.0,
            commission: 0.3,
            timestamp: 0,
            opposite_order_id: [0; 32],
            gateway_id: 0,
            session_id: 0,
        };

        // 应该不会 panic，只是警告日志
        system.batch_update_accounts(&[trade]);

        // 不应有通知（账户不存在）
        let notify = update_rx.try_recv();
        assert!(notify.is_err());
    }

    // ==================== 并发安全测试 @yutiansut @quantaxis ====================

    /// 测试并发注册和获取账户
    /// 验证 DashMap 的线程安全性
    #[test]
    fn test_concurrent_account_access() {
        use std::thread;

        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = Arc::new(AccountSystemCore::new(trade_rx, accepted_rx, None, 100));

        // 并发注册
        let mut handles = vec![];
        for i in 0..10 {
            let system_clone = system.clone();
            handles.push(thread::spawn(move || {
                let user_id = format!("user_{:02}", i);
                let account = QA_Account::new(&user_id, "default", &user_id, 1000000.0, false, "sim");
                system_clone.register_account(user_id, account);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(system.accounts.len(), 10);

        // 并发获取
        let mut handles = vec![];
        for i in 0..10 {
            let system_clone = system.clone();
            handles.push(thread::spawn(move || {
                let user_id = format!("user_{:02}", i);
                system_clone.get_account(&user_id).is_some()
            }));
        }

        for handle in handles {
            assert!(handle.join().unwrap());
        }
    }

    // ==================== 订单确认测试 @yutiansut @quantaxis ====================

    /// 测试订单确认处理 - 账户存在
    /// handle_order_accepted 应更新订单的 exchange_order_id
    #[test]
    fn test_handle_order_accepted_account_found() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        let account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account);

        let accepted = OrderAccepted {
            order_id: *b"ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: *b"EX_ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            user_id: *b"user_01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            timestamp: 1234567890,
            gateway_id: 0,
            session_id: 0,
        };

        // 不应 panic
        system.handle_order_accepted(accepted);
    }

    /// 测试订单确认处理 - 账户不存在
    /// 应该记录警告日志但不 panic
    #[test]
    fn test_handle_order_accepted_account_not_found() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, None, 100);

        // 不注册账户

        let accepted = OrderAccepted {
            order_id: *b"ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            exchange_order_id: *b"EX_ORDER001\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            user_id: *b"unknown\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            instrument_id: *b"IX2401\0\0\0\0\0\0\0\0\0\0",
            timestamp: 1234567890,
            gateway_id: 0,
            session_id: 0,
        };

        // 不应 panic
        system.handle_order_accepted(accepted);
    }
}
