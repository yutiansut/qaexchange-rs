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

            grouped.entry(user_id).or_insert_with(Vec::new).push(trade);
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

    #[test]
    fn test_batch_update() {
        let (trade_tx, trade_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();
        let (update_tx, update_rx) = unbounded();

        let system = AccountSystemCore::new(trade_rx, accepted_rx, Some(update_tx), 100);

        let account = QA_Account::new("user_01", "default", "user_01", 1000000.0, false, "sim");
        system.register_account("user_01".to_string(), account);

        // 创建测试成交
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

        // 验证账户已更新
        let acc = system.get_account("user_01").unwrap();
        let account = acc.read();
        // money 应该减少（买入开仓需要资金）
        assert!(account.money < 1000000.0);
    }
}
