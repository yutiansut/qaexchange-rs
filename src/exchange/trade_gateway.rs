//! 成交回报网关
//!
//! 负责处理撮合引擎的成交结果，更新账户，并推送成交回报到客户端

use crate::core::{Order, QA_Account, Trade};
use crate::exchange::{
    AccountManager, ExchangeIdGenerator, ExchangeOrderRecord, ExchangeTradeRecord,
};
use crate::matching::{Failed, Success};
use crate::notification::broker::NotificationBroker;
use crate::notification::message::{
    AccountUpdateNotify, Notification as NewNotification, NotificationPayload, NotificationType,
    OrderAcceptedNotify, OrderCanceledNotify, OrderFilledNotify, OrderPartiallyFilledNotify,
    OrderRejectedNotify, TradeExecutedNotify,
};
use crate::protocol::diff::snapshot::SnapshotManager;
use crate::protocol::diff::types::{DiffAccount, DiffTrade};
use crate::storage::wal::manager::WalManager;
use crate::storage::wal::record::{WalEntry, WalRecord};
use crate::ExchangeError;
use chrono::Utc;
use crossbeam::channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 成交回报消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeNotification {
    pub trade_id: String,
    pub user_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub direction: String, // BUY/SELL
    pub offset: String,    // OPEN/CLOSE
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub commission: f64,
}

/// 账户更新通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdateNotification {
    pub user_id: String,
    pub balance: f64,
    pub available: f64,
    pub margin: f64,
    pub position_profit: f64,
    pub risk_ratio: f64,
    pub timestamp: i64,
}

/// 订单状态更新通知（交易所回报）
///
/// 这是交易所层面的回报消息，只包含交易所需要的核心字段。
/// 流程：交易所回报(exchange_order_id) → 映射查找(order_id, user_id) → 账户更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatusNotification {
    // 交易所回报字段
    pub exchange_id: String,       // 交易所ID
    pub instrument_id: String,     // 合约ID
    pub exchange_order_id: String, // 交易所订单号（关键标识）
    pub direction: String,         // BUY/SELL
    pub offset: String,            // OPEN/CLOSE (towards: 1=买开, -1=卖开, 3=买平, -3=卖平)
    pub price_type: String,        // LIMIT/MARKET
    pub volume: f64,               // 本次成交量（对于ACCEPTED是委托量）
    pub price: f64,                // 价格（对于ACCEPTED是委托价）
    pub status: String,            // ACCEPTED/FILLED/PARTIAL_FILLED/CANCELLED
    pub timestamp: i64,            // 回报时间

    // 内部映射字段（通过 exchange_order_id 查找得到）
    pub order_id: String, // 内部订单ID
    pub user_id: String,  // 用户ID

    /// 附加原因（用于拒绝/撤单失败等场景）
    pub reason: Option<String>,
}

/// 通知类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Notification {
    Trade(TradeNotification),
    AccountUpdate(AccountUpdateNotification),
    OrderStatus(OrderStatusNotification),
}

/// 成交回报网关
pub struct TradeGateway {
    /// 账户管理器
    account_mgr: Arc<AccountManager>,

    /// 成交通知发送器
    trade_sender: Sender<Notification>,

    /// 成交通知接收器
    trade_receiver: Receiver<Notification>,

    /// 订阅者映射 (user_id -> Vec<Sender<Notification>>)
    subscribers: DashMap<String, Arc<RwLock<Vec<Sender<Notification>>>>>,

    /// 全局订阅者 (接收所有通知) - crossbeam channel
    global_subscribers: Arc<RwLock<Vec<Sender<Notification>>>>,

    /// 全局订阅者 (tokio mpsc) - 用于异步任务
    global_tokio_subscribers: Arc<RwLock<Vec<tokio::sync::mpsc::Sender<Notification>>>>,

    /// 成交序号生成器 (旧版 - 待废弃)
    trade_seq: Arc<std::sync::atomic::AtomicU64>,

    /// 交易所统一事件序列生成器 (Phase 2)
    id_generator: Arc<ExchangeIdGenerator>,

    /// 新的通知系统（用于集成存储和WAL）
    notification_broker: Option<Arc<NotificationBroker>>,

    /// DIFF 协议业务快照管理器（零拷贝共享）
    snapshot_mgr: Option<Arc<SnapshotManager>>,

    /// WAL 管理器映射（per-instrument: {instrument_id} -> WalManager for orders/trades）
    /// Phase 5: 存储分离 - 交易所内部数据
    instrument_wal_managers: DashMap<String, Arc<WalManager>>,

    /// WAL 管理器映射（per-account: {user_id} -> WalManager for responses）
    /// Phase 5: 存储分离 - 账户回报数据
    account_wal_managers: DashMap<String, Arc<WalManager>>,

    /// 成交记录器（可选，用于查询历史成交）
    trade_recorder: Option<Arc<crate::matching::trade_recorder::TradeRecorder>>,

    /// WAL 根目录
    wal_root: String,

    /// 市场数据服务（用于更新快照统计）
    market_data_service: Option<Arc<crate::market::MarketDataService>>,
}

impl TradeGateway {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        let (trade_sender, trade_receiver) = unbounded();

        Self {
            account_mgr,
            trade_sender,
            trade_receiver,
            subscribers: DashMap::new(),
            global_subscribers: Arc::new(RwLock::new(Vec::new())),
            global_tokio_subscribers: Arc::new(RwLock::new(Vec::new())),
            trade_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            id_generator: Arc::new(ExchangeIdGenerator::new()),
            notification_broker: None,
            snapshot_mgr: None,
            instrument_wal_managers: DashMap::new(),
            account_wal_managers: DashMap::new(),
            wal_root: "./data/wal".to_string(), // 默认 WAL 根目录
            trade_recorder: None,
            market_data_service: None,
        }
    }

    /// 设置成交记录器
    pub fn set_trade_recorder(
        mut self,
        trade_recorder: Arc<crate::matching::trade_recorder::TradeRecorder>,
    ) -> Self {
        self.trade_recorder = Some(trade_recorder);
        self
    }

    /// 设置市场数据服务（用于更新快照统计）
    pub fn set_market_data_service(
        &mut self,
        market_data_service: Arc<crate::market::MarketDataService>,
    ) {
        self.market_data_service = Some(market_data_service);
    }

    /// 设置 WAL 根目录 (Phase 5)
    pub fn with_wal_root(mut self, wal_root: impl Into<String>) -> Self {
        self.wal_root = wal_root.into();
        self
    }

    /// 设置通知代理（用于集成新的notification系统）
    pub fn with_notification_broker(mut self, broker: Arc<NotificationBroker>) -> Self {
        self.notification_broker = Some(broker);
        self
    }

    /// 设置通知代理（已初始化后设置）
    pub fn set_notification_broker(&mut self, broker: Arc<NotificationBroker>) {
        self.notification_broker = Some(broker);
    }

    /// 设置 DIFF 快照管理器（用于 DIFF 协议实时推送）
    pub fn set_snapshot_manager(&mut self, snapshot_mgr: Arc<SnapshotManager>) {
        self.snapshot_mgr = Some(snapshot_mgr);
    }

    /// 获取 DIFF 快照管理器
    pub fn snapshot_manager(&self) -> Option<&Arc<SnapshotManager>> {
        self.snapshot_mgr.as_ref()
    }

    /// 处理撮合结果 (已废弃 - OrderRouter 直接调用 handle_filled/handle_partially_filled)
    ///
    /// ⚠️ 此方法已废弃，因为缺少交易所回报必需的字段（exchange_id, exchange_order_id, price_type）
    /// 请直接从 OrderRouter 调用 handle_filled/handle_partially_filled/handle_accepted/handle_cancelled
    #[deprecated(note = "Use handle_filled/handle_partially_filled directly from OrderRouter")]
    #[allow(dead_code)]
    pub fn process_match_result(
        &self,
        _order_id: &str,
        _user_id: &str,
        _instrument_id: &str,
        _direction: &str,
        _offset: &str,
        _result: Result<Success, Failed>,
        _qa_order_id: &str,
    ) -> Result<(), ExchangeError> {
        Err(ExchangeError::OrderError(
            "process_match_result is deprecated, use handle_filled/handle_partially_filled directly".to_string()
        ))
    }

    /// 处理全部成交
    pub fn handle_filled(
        &self,
        order_id: &str,
        user_id: &str,
        exchange_id: &str,
        instrument_id: &str,
        exchange_order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        volume: f64,
        qa_order_id: &str, // qars 内部订单ID
    ) -> Result<(), ExchangeError> {
        // 1. 更新账户 (Phase 4: 调用 order.trade() 更新 volume_left，并获取订单状态)
        let (order_status, volume_left, volume_orign) = self.update_account(
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
            qa_order_id,
        )?;

        // 2. 生成成交回报
        let trade_notification = self.create_trade_notification(
            order_id,
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
        );

        // 3. 推送成交回报（Trade 事件）
        self.send_notification(Notification::Trade(trade_notification.clone()))?;

        // 4. 推送订单状态更新（包含 volume_left 和 status，用户根据这些字段自己判断）
        let order_status_notification = OrderStatusNotification {
            exchange_id: exchange_id.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume: volume_left, // 剩余未成交量
            price,
            status: order_status.clone(), // 实际状态：ALIVE 或 FINISHED
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };
        self.send_notification(Notification::OrderStatus(order_status_notification.clone()))?;

        // 5. 推送账户更新
        self.push_account_update(user_id)?;

        // 6. DIFF 协议：推送成交和订单状态 patch（如果设置了 SnapshotManager）
        if let Some(snapshot_mgr) = &self.snapshot_mgr {
            let trade_patch = serde_json::json!({
                "trades": {
                    trade_notification.trade_id.clone(): {
                        "trade_id": trade_notification.trade_id,
                        "user_id": trade_notification.user_id,
                        "order_id": trade_notification.order_id,
                        "instrument_id": trade_notification.instrument_id,
                        "direction": trade_notification.direction,
                        "offset": trade_notification.offset,
                        "price": trade_notification.price,
                        "volume": trade_notification.volume,
                        "commission": trade_notification.commission,
                        "timestamp": trade_notification.timestamp,
                    }
                }
            });

            let order_patch = serde_json::json!({
                "orders": {
                    order_id: {
                        "status": order_status,
                        "volume_left": volume_left,
                        "volume_orign": volume_orign,
                        "update_time": order_status_notification.timestamp,
                    }
                }
            });

            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = user_id.to_string();
            tokio::spawn(async move {
                snapshot_mgr.push_patch(&user_id, trade_patch).await;
                snapshot_mgr.push_patch(&user_id, order_patch).await;
            });
        }

        log::info!(
            "Trade executed for order {}: {} @ {} x {} | status={}, volume_left={}/{}",
            order_id,
            instrument_id,
            price,
            volume,
            order_status,
            volume_left,
            volume_orign
        );

        Ok(())
    }

    /// 处理部分成交
    pub fn handle_partially_filled(
        &self,
        order_id: &str,
        user_id: &str,
        exchange_id: &str,
        instrument_id: &str,
        exchange_order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        volume: f64,
        qa_order_id: &str, // qars 内部订单ID
    ) -> Result<(), ExchangeError> {
        // 1. 更新账户 (Phase 4: 调用 order.trade() 更新 volume_left，并获取订单状态)
        let (order_status, volume_left, volume_orign) = self.update_account(
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
            qa_order_id,
        )?;

        // 2. 生成成交回报
        let trade_notification = self.create_trade_notification(
            order_id,
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
        );

        // 3. 推送成交回报（Trade 事件）
        self.send_notification(Notification::Trade(trade_notification.clone()))?;

        // 4. 推送订单状态更新（包含 volume_left 和 status，用户根据这些字段自己判断）
        let order_status_notification = OrderStatusNotification {
            exchange_id: exchange_id.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume: volume_left, // 剩余未成交量
            price,
            status: order_status.clone(), // 实际状态：ALIVE 或 FINISHED
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };
        self.send_notification(Notification::OrderStatus(order_status_notification.clone()))?;

        // 5. 推送账户更新
        self.push_account_update(user_id)?;

        // 6. DIFF 协议：推送成交和订单状态 patch（如果设置了 SnapshotManager）
        if let Some(snapshot_mgr) = &self.snapshot_mgr {
            let trade_patch = serde_json::json!({
                "trades": {
                    trade_notification.trade_id.clone(): {
                        "trade_id": trade_notification.trade_id,
                        "user_id": trade_notification.user_id,
                        "order_id": trade_notification.order_id,
                        "instrument_id": trade_notification.instrument_id,
                        "direction": trade_notification.direction,
                        "offset": trade_notification.offset,
                        "price": trade_notification.price,
                        "volume": trade_notification.volume,
                        "commission": trade_notification.commission,
                        "timestamp": trade_notification.timestamp,
                    }
                }
            });

            let order_patch = serde_json::json!({
                "orders": {
                    order_id: {
                        "status": order_status,
                        "volume_left": volume_left,
                        "volume_orign": volume_orign,
                        "update_time": order_status_notification.timestamp,
                    }
                }
            });

            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = user_id.to_string();
            tokio::spawn(async move {
                snapshot_mgr.push_patch(&user_id, trade_patch).await;
                snapshot_mgr.push_patch(&user_id, order_patch).await;
            });
        }

        log::info!(
            "Trade executed for order {}: {} @ {} x {} | status={}, volume_left={}/{}",
            order_id,
            instrument_id,
            price,
            volume,
            order_status,
            volume_left,
            volume_orign
        );

        Ok(())
    }

    /// 处理订单接受（原有方法保留）
    /// 处理订单已接受（旧版本 - 已废弃）
    ///
    /// ⚠️ 此方法已废弃，因为缺少交易所回报必需的字段（exchange_id, exchange_order_id, price_type）
    /// 请使用新版本的 handle_accepted 方法
    #[deprecated(note = "Use handle_accepted with exchange fields")]
    #[allow(dead_code)]
    pub fn handle_accepted_original(
        &self,
        _order_id: &str,
        _user_id: &str,
        _instrument_id: &str,
        _direction: &str,
        _offset: &str,
        _price: f64,
        _volume: f64,
        _qa_order_id: &str,
    ) -> Result<(), ExchangeError> {
        Err(ExchangeError::OrderError(
            "handle_accepted_original is deprecated, use handle_accepted with exchange fields"
                .to_string(),
        ))
    }

    /// 处理订单已接受
    pub fn handle_accepted(
        &self,
        order_id: &str,
        user_id: &str,
        exchange_id: &str,
        instrument_id: &str,
        exchange_order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        volume: f64,
    ) -> Result<(), ExchangeError> {
        let order_status = OrderStatusNotification {
            // 交易所回报字段
            exchange_id: exchange_id.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume, // 委托量
            price,  // 委托价格
            status: "ACCEPTED".to_string(),
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            // 内部映射字段
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };

        self.send_notification(Notification::OrderStatus(order_status))?;

        log::info!("Order {} accepted", order_id);
        Ok(())
    }

    /// 处理订单已撤销
    pub fn handle_cancelled(
        &self,
        order_id: &str,
        user_id: &str,
        exchange_id: &str,
        instrument_id: &str,
        exchange_order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        volume: f64, // 撤单时的剩余量
    ) -> Result<(), ExchangeError> {
        let order_status = OrderStatusNotification {
            // 交易所回报字段
            exchange_id: exchange_id.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume, // 撤单时的剩余量
            price,  // 委托价格
            status: "CANCELLED".to_string(),
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            // 内部映射字段
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };

        self.send_notification(Notification::OrderStatus(order_status))?;

        log::info!("Order {} cancelled", order_id);
        Ok(())
    }

    // ==================== Phase 3: 新的交易所回报方法 ====================
    //
    // 交易所只推送5种回报：
    // 1. OrderAccepted - 订单接受
    // 2. OrderRejected - 订单拒绝
    // 3. Trade - 成交（不判断FILLED/PARTIAL_FILLED）
    // 4. CancelAccepted - 撤单成功
    // 5. CancelRejected - 撤单拒绝
    //
    // 账户端收到TRADE回报后自己判断FILLED/PARTIAL_FILLED

    /// 处理订单接受回报 (Phase 3 + Phase 5)
    ///
    /// 交易所接受订单，推送OrderAccepted回报给账户
    pub fn handle_order_accepted_new(
        &self,
        exchange: &str, // 交易所代码
        instrument_id: &str,
        user_id: &str,    // 用于映射
        order_id: &str,   // 内部订单ID
        direction: &str,  // BUY/SELL
        offset: &str,     // OPEN/CLOSE/CLOSETODAY
        price_type: &str, // LIMIT/MARKET
        price: f64,
        volume: f64,
    ) -> Result<i64, ExchangeError> {
        // 生成交易所订单号（统一事件序列）
        let exchange_order_id = self.id_generator.next_sequence(instrument_id);
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Phase 5: 存储 ExchangeOrderRecord 到 {instrument_id}/orders/
        let order_record = WalRecord::ExchangeOrderRecord {
            exchange: WalRecord::to_fixed_array_16(exchange),
            instrument: WalRecord::to_fixed_array_16(instrument_id),
            exchange_order_id,
            direction: match direction {
                "BUY" => 0,
                "SELL" => 1,
                _ => 0,
            },
            offset: match offset {
                "OPEN" => 0,
                "CLOSE" => 1,
                "CLOSETODAY" => 2,
                _ => 0,
            },
            price_type: match price_type {
                "LIMIT" => 0,
                "MARKET" => 1,
                _ => 0,
            },
            price,
            volume,
            time: timestamp,
            internal_order_id: WalRecord::to_fixed_array_32(order_id),
            user_id: WalRecord::to_fixed_array_32(user_id),
        };

        // 获取或创建 instrument WAL manager
        let wal_mgr = self.get_or_create_instrument_wal(instrument_id)?;

        // 持久化 WAL record（WalManager 内部会创建 entry）
        wal_mgr.append(order_record).map_err(|e| {
            ExchangeError::StorageError(format!("Failed to append ExchangeOrderRecord: {}", e))
        })?;

        // Phase 5: 存储 ExchangeResponseRecord 到 __ACCOUNT__/{user_id}/
        let response_record = WalRecord::ExchangeResponseRecord {
            response_type: 0, // 0=OrderAccepted
            exchange_order_id,
            instrument: WalRecord::to_fixed_array_16(instrument_id),
            user_id: WalRecord::to_fixed_array_32(user_id),
            timestamp,
            trade_id: 0,        // N/A for OrderAccepted
            volume: 0.0,        // N/A
            price: 0.0,         // N/A
            reason: [0u8; 128], // N/A
        };

        // 获取或创建 account WAL manager
        let account_wal_mgr = self.get_or_create_account_wal(user_id)?;

        // 持久化 WAL record（WalManager 内部会创建 entry）
        account_wal_mgr.append(response_record).map_err(|e| {
            ExchangeError::StorageError(format!("Failed to append ExchangeResponseRecord: {}", e))
        })?;

        let order_status = OrderStatusNotification {
            exchange_id: exchange.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume,
            price,
            status: "ACCEPTED".to_string(),
            timestamp,
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };

        self.emit_order_status(order_status)?;

        log::info!(
            "Order accepted: exchange_order_id={}, instrument={}, user={}, order_id={}",
            exchange_order_id,
            instrument_id,
            user_id,
            order_id
        );

        Ok(exchange_order_id)
    }

    /// 处理订单拒绝回报 (Phase 3)
    ///
    /// 交易所拒绝订单，推送OrderRejected回报给账户
    pub fn handle_order_rejected_new(
        &self,
        exchange: &str,
        instrument_id: &str,
        user_id: &str,
        order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        volume: f64,
        reason: &str,
    ) -> Result<i64, ExchangeError> {
        // 生成交易所订单号（统一事件序列）
        let exchange_order_id = self.id_generator.next_sequence(instrument_id);
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let order_status = OrderStatusNotification {
            exchange_id: exchange.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume,
            price,
            status: "REJECTED".to_string(),
            timestamp,
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: Some(reason.to_string()),
        };

        self.emit_order_status(order_status)?;

        log::warn!(
            "Order rejected: exchange_order_id={}, instrument={}, user={}, order_id={}, reason={}",
            exchange_order_id,
            instrument_id,
            user_id,
            order_id,
            reason
        );

        Ok(exchange_order_id)
    }

    /// 处理成交回报 (Phase 3 + Phase 5)
    ///
    /// 交易所成交，推送Trade回报给账户（不判断FILLED/PARTIAL_FILLED）
    /// 账户端收到TRADE后自己计算 volume_left 判断状态
    pub fn handle_trade_new(
        &self,
        exchange: &str, // 交易所代码
        instrument_id: &str,
        exchange_order_id: i64, // 订单的交易所订单号
        user_id: &str,
        order_id: &str,
        direction: &str, // BUY/SELL (用于确定买卖方)
        offset: &str,
        volume: f64,
        price: f64,
        opposite_order_id: Option<i64>, // 对手方订单号（如果可用）
    ) -> Result<i64, ExchangeError> {
        // 生成成交ID（统一事件序列）
        let trade_id = self.id_generator.next_sequence(instrument_id);
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Phase 5: 存储 ExchangeTradeRecord 到 {instrument_id}/trades/
        // 根据 direction 确定买卖方订单号
        let (buy_exchange_order_id, sell_exchange_order_id) = match direction {
            "BUY" => (exchange_order_id, opposite_order_id.unwrap_or(0)),
            "SELL" => (opposite_order_id.unwrap_or(0), exchange_order_id),
            _ => (exchange_order_id, 0), // fallback
        };

        let trade_record = WalRecord::ExchangeTradeRecord {
            exchange: WalRecord::to_fixed_array_16(exchange),
            instrument: WalRecord::to_fixed_array_16(instrument_id),
            buy_exchange_order_id,
            sell_exchange_order_id,
            deal_price: price,
            deal_volume: volume,
            time: timestamp,
            trade_id,
        };

        // 获取或创建 instrument WAL manager
        let wal_mgr = self.get_or_create_instrument_wal(instrument_id)?;

        // 持久化 WAL record（WalManager 内部会创建 entry）
        wal_mgr.append(trade_record).map_err(|e| {
            ExchangeError::StorageError(format!("Failed to append ExchangeTradeRecord: {}", e))
        })?;

        // Phase 5: 存储 ExchangeResponseRecord 到 __ACCOUNT__/{user_id}/
        let response_record = WalRecord::ExchangeResponseRecord {
            response_type: 2, // 2=Trade
            exchange_order_id,
            instrument: WalRecord::to_fixed_array_16(instrument_id),
            user_id: WalRecord::to_fixed_array_32(user_id),
            timestamp,
            trade_id,
            volume,
            price,
            reason: [0u8; 128], // N/A for Trade
        };

        // 获取或创建 account WAL manager
        let account_wal_mgr = self.get_or_create_account_wal(user_id)?;

        // 持久化 WAL record（WalManager 内部会创建 entry）
        account_wal_mgr.append(response_record).map_err(|e| {
            ExchangeError::StorageError(format!(
                "Failed to append ExchangeResponseRecord (Trade): {}",
                e
            ))
        })?;

        // 记录成交到 TradeRecorder（用于查询）
        if let Some(recorder) = &self.trade_recorder {
            // 注意：这里的 user_id 实际上是 account_id
            // 由于没有对手方信息，暂时两边都用同一个 account_id
            // 在完整实现中，应该从 opposite_order_id 查找对手方 account_id
            let trading_day = chrono::Utc::now().format("%Y-%m-%d").to_string();

            recorder.record_trade(
                instrument_id.to_string(),
                user_id.to_string(), // buy_account_id (如果是BUY方)
                user_id.to_string(), // sell_account_id (如果是SELL方，应该从对手方获取)
                order_id.to_string(),
                format!("opposite_{}", opposite_order_id.unwrap_or(0)),
                price,
                volume,
                trading_day,
            );
        }

        // 更新快照生成器的成交统计
        if let Some(mds) = &self.market_data_service {
            let turnover = price * volume;
            mds.update_trade_stats(instrument_id, volume as i64, turnover);
            mds.on_trade(instrument_id, price, volume as i64);
            log::trace!(
                "Updated snapshot stats: {} volume={}, turnover={:.2}",
                instrument_id,
                volume,
                turnover
            );
        }

        let trade_notification = self.create_trade_notification(
            order_id,
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
        );

        self.emit_trade_notification(trade_notification)?;
        self.push_account_update(user_id)?;

        log::info!(
            "Trade executed: trade_id={}, exchange_order_id={}, instrument={}, volume={}, price={}",
            trade_id,
            exchange_order_id,
            instrument_id,
            volume,
            price
        );

        Ok(trade_id)
    }

    /// 处理撤单成功回报 (Phase 3)
    ///
    /// 交易所撤单成功，推送CancelAccepted回报给账户
    pub fn handle_cancel_accepted_new(
        &self,
        exchange: &str,
        instrument_id: &str,
        exchange_order_id: i64,
        user_id: &str,
        order_id: &str,
        direction: &str,
        offset: &str,
        price_type: &str,
        price: f64,
        remaining_volume: f64,
    ) -> Result<(), ExchangeError> {
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let order_status = OrderStatusNotification {
            exchange_id: exchange.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price_type: price_type.to_string(),
            volume: remaining_volume,
            price,
            status: "CANCELLED".to_string(),
            timestamp,
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: None,
        };

        self.emit_order_status(order_status)?;

        log::info!(
            "Cancel accepted: exchange_order_id={}, instrument={}, user={}, order_id={}",
            exchange_order_id,
            instrument_id,
            user_id,
            order_id
        );

        Ok(())
    }

    /// 处理撤单拒绝回报 (Phase 3)
    ///
    /// 交易所撤单失败，推送CancelRejected回报给账户
    pub fn handle_cancel_rejected_new(
        &self,
        exchange: &str,
        instrument_id: &str,
        exchange_order_id: i64,
        user_id: &str,
        order_id: &str,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let order_status = OrderStatusNotification {
            exchange_id: exchange.to_string(),
            instrument_id: instrument_id.to_string(),
            exchange_order_id: exchange_order_id.to_string(),
            direction: "".to_string(),
            offset: "".to_string(),
            price_type: String::new(),
            volume: 0.0,
            price: 0.0,
            status: "CANCEL_REJECTED".to_string(),
            timestamp,
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            reason: Some(reason.to_string()),
        };

        self.emit_order_status(order_status)?;

        log::warn!(
            "Cancel rejected: exchange_order_id={}, instrument={}, user={}, order_id={}, reason={}",
            exchange_order_id,
            instrument_id,
            user_id,
            order_id,
            reason
        );

        Ok(())
    }

    /// 更新账户资金和持仓（方案B：成交时只调用 receive_deal_sim）
    ///
    /// 注意：send_order 已在订单提交时调用，此处只需要处理成交
    ///
    /// Phase 4: 调用 order.trade() 更新 volume_left
    /// 返回: (status, volume_left, volume_orign) - 订单的当前状态
    fn update_account(
        &self,
        user_id: &str,
        instrument_id: &str,
        direction: &str,
        offset: &str,
        price: f64,
        volume: f64,
        qa_order_id: &str, // qars 内部订单ID（非交易所订单ID）
    ) -> Result<(String, f64, f64), ExchangeError> {
        log::debug!("🔧 update_account called: user={}, instrument={}, {}  {}, price={}, volume={}, qa_order_id={}",
            user_id, instrument_id, direction, offset, price, volume, qa_order_id);

        let account = self.account_mgr.get_default_account(user_id)?;
        let mut acc = account.write();

        // 检查成交前的持仓（详细）
        if let Some(pos) = acc.get_position(instrument_id) {
            log::debug!(
                "🔧   BEFORE receive_deal_sim: {} position details:",
                user_id
            );
            log::debug!("🔧     volume_short_today={}, volume_short_his={}, volume_short_frozen_today={}, volume_short_frozen_his={}",
                pos.volume_short_today, pos.volume_short_his, pos.volume_short_frozen_today, pos.volume_short_frozen_his);
            log::debug!("🔧     volume_short_unmut()={}", pos.volume_short_unmut());
        } else {
            log::debug!(
                "🔧   BEFORE receive_deal_sim: {} no position for {}",
                user_id,
                instrument_id
            );
        }

        // 生成时间戳字符串
        let datetime = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 计算 towards (遵循 qars 的定义)
        let towards = match (direction, offset) {
            ("BUY", "OPEN") => 2,    // 买开 = 2 (qars 标准)
            ("SELL", "OPEN") => -2,  // 卖开 = -2
            ("BUY", "CLOSE") => 3,   // 买平 (平空) = 3
            ("SELL", "CLOSE") => -3, // 卖平 (平多) = -3 ✅
            ("BUY", "CLOSETODAY") => 4,
            ("SELL", "CLOSETODAY") => -4,
            _ => {
                return Err(ExchangeError::OrderError(format!(
                    "Invalid direction/offset: {}/{}",
                    direction, offset
                )))
            }
        };

        // 处理成交 (释放冻结资金，更新持仓和余额)
        // 注意：send_order 已在订单提交时调用，此处不需要再次调用
        let trade_id = format!("T{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));

        log::debug!(
            "🔧   Calling receive_deal_sim with qa_order_id={}",
            qa_order_id
        );
        acc.receive_deal_sim(
            instrument_id.to_string(),
            volume,
            price,
            datetime.clone(),
            qa_order_id.to_string(), // ✅ 使用 qars 内部订单ID (关键修复！)
            trade_id.clone(),
            qa_order_id.to_string(), // realorder_id 与 qa_order_id 相同
            towards,
        );

        // 检查成交后的持仓
        let pos_after = acc
            .get_position(instrument_id)
            .map(|p| (p.volume_long_unmut(), p.volume_short_unmut()));
        log::debug!(
            "🔧   AFTER receive_deal_sim: {} position={:?}",
            user_id,
            pos_after
        );

        // 注意：不要在这里调用 settle()！
        // settle() 是日终结算，会重新计算持仓盈亏，只能在日终时调用一次

        // Phase 4: 更新订单的 volume_left（用户自己根据 volume_left 判断订单状态）
        let (status, volume_left, volume_orign) = if let Some(order) =
            acc.dailyorders.get_mut(qa_order_id)
        {
            log::debug!("🔧   BEFORE order.trade(): order_id={}, volume_left={}, volume_orign={}, status={}",
                qa_order_id, order.volume_left, order.volume_orign, order.status);

            // 调用订单的 trade() 方法，自动更新 volume_left
            // qars 的 trade() 方法会：
            // 1. volume_left -= amount
            // 2. if volume_left == 0.0 { status = "FINISHED" }
            order.trade(volume);

            log::debug!(
                "🔧   AFTER order.trade(): order_id={}, volume_left={}, status={}",
                qa_order_id,
                order.volume_left,
                order.status
            );

            // 返回订单的当前状态
            (order.status.clone(), order.volume_left, order.volume_orign)
        } else {
            log::warn!(
                "⚠️  Order {} not found in dailyorders, cannot update volume_left",
                qa_order_id
            );
            // 订单未找到时返回默认值（ALIVE 状态，假设全部未成交）
            ("ALIVE".to_string(), volume, volume)
        };

        log::debug!(
            "Account updated: {} {} {} {} @ {} x {} | qa_order_id: {} | trade_id: {} | money: {:.2} | order_status={}, volume_left={}/{}",
            user_id, direction, offset, instrument_id, price, volume, qa_order_id, trade_id, acc.money, status, volume_left, volume_orign
        );

        Ok((status, volume_left, volume_orign))
    }

    // ==================== Phase 5: WAL Manager 辅助方法 ====================

    /// 获取或创建 instrument 的 WAL 管理器
    /// 路径: {wal_root}/{instrument_id}/
    fn get_or_create_instrument_wal(
        &self,
        instrument_id: &str,
    ) -> Result<Arc<WalManager>, ExchangeError> {
        if let Some(wal_mgr) = self.instrument_wal_managers.get(instrument_id) {
            return Ok(wal_mgr.value().clone());
        }

        // 创建新的 WAL manager
        let wal_dir = format!("{}/{}", self.wal_root, instrument_id);
        let wal_mgr = Arc::new(WalManager::new(&wal_dir));

        // 存储到映射表
        self.instrument_wal_managers
            .insert(instrument_id.to_string(), wal_mgr.clone());

        log::debug!(
            "Created instrument WAL manager for {}: {}",
            instrument_id,
            wal_dir
        );

        Ok(wal_mgr)
    }

    /// 获取或创建 account 的 WAL 管理器
    /// 路径: {wal_root}/__ACCOUNT__/{user_id}/
    fn get_or_create_account_wal(&self, user_id: &str) -> Result<Arc<WalManager>, ExchangeError> {
        if let Some(wal_mgr) = self.account_wal_managers.get(user_id) {
            return Ok(wal_mgr.value().clone());
        }

        // 创建新的 WAL manager
        let wal_dir = format!("{}/__ACCOUNT__/{}", self.wal_root, user_id);
        let wal_mgr = Arc::new(WalManager::new(&wal_dir));

        // 存储到映射表
        self.account_wal_managers
            .insert(user_id.to_string(), wal_mgr.clone());

        log::debug!("Created account WAL manager for {}: {}", user_id, wal_dir);

        Ok(wal_mgr)
    }

    /// 创建成交通知
    fn create_trade_notification(
        &self,
        order_id: &str,
        user_id: &str,
        instrument_id: &str,
        direction: &str,
        offset: &str,
        price: f64,
        volume: f64,
    ) -> TradeNotification {
        let trade_id = self.generate_trade_id();
        let commission = price * volume * 0.0003;

        TradeNotification {
            trade_id,
            user_id: user_id.to_string(),
            order_id: order_id.to_string(),
            instrument_id: instrument_id.to_string(),
            direction: direction.to_string(),
            offset: offset.to_string(),
            price,
            volume,
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            commission,
        }
    }

    /// 推送账户更新
    fn push_account_update(&self, user_id: &str) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_default_account(user_id)?;
        let acc = account.read();

        let notification = AccountUpdateNotification {
            user_id: user_id.to_string(),
            balance: acc.accounts.balance,
            available: acc.money,
            margin: acc.accounts.margin,
            position_profit: acc.accounts.position_profit,
            risk_ratio: acc.accounts.risk_ratio,
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        self.send_notification(Notification::AccountUpdate(notification))?;

        // DIFF 协议：推送账户更新 patch（如果设置了 SnapshotManager）
        if let Some(snapshot_mgr) = &self.snapshot_mgr {
            let patch = serde_json::json!({
                "accounts": {
                    user_id: {
                        "balance": acc.accounts.balance,
                        "available": acc.money,
                        "margin": acc.accounts.margin,
                        "position_profit": acc.accounts.position_profit,
                        "risk_ratio": acc.accounts.risk_ratio,
                    }
                }
            });

            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = user_id.to_string();
            tokio::spawn(async move {
                snapshot_mgr.push_patch(&user_id, patch).await;
            });
        }

        Ok(())
    }

    /// 推送订单状态通知并同步 DIFF 快照
    fn emit_order_status(&self, status: OrderStatusNotification) -> Result<(), ExchangeError> {
        self.send_notification(Notification::OrderStatus(status.clone()))?;

        if let Some(snapshot_mgr) = &self.snapshot_mgr {
            let patch = serde_json::json!({
                "orders": {
                    status.order_id.clone(): {
                        "status": status.status,
                        "exchange_id": status.exchange_id,
                        "exchange_order_id": status.exchange_order_id,
                        "instrument_id": status.instrument_id,
                        "direction": status.direction,
                        "offset": status.offset,
                        "price_type": status.price_type,
                        "price": status.price,
                        "volume": status.volume,
                        "reason": status.reason,
                        "update_time": status.timestamp,
                    }
                }
            });

            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = status.user_id.clone();
            tokio::spawn(async move {
                snapshot_mgr.push_patch(&user_id, patch).await;
            });
        }

        Ok(())
    }

    /// 推发送成交通知并同步 DIFF 快照
    fn emit_trade_notification(&self, trade: TradeNotification) -> Result<(), ExchangeError> {
        self.send_notification(Notification::Trade(trade.clone()))?;

        if let Some(snapshot_mgr) = &self.snapshot_mgr {
            let patch = serde_json::json!({
                "trades": {
                    trade.trade_id.clone(): {
                        "trade_id": trade.trade_id,
                        "user_id": trade.user_id,
                        "order_id": trade.order_id,
                        "instrument_id": trade.instrument_id,
                        "direction": trade.direction,
                        "offset": trade.offset,
                        "price": trade.price,
                        "volume": trade.volume,
                        "commission": trade.commission,
                        "timestamp": trade.timestamp,
                    }
                }
            });

            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = trade.user_id.clone();
            tokio::spawn(async move {
                snapshot_mgr.push_patch(&user_id, patch).await;
            });
        }

        Ok(())
    }

    /// 发送通知
    fn send_notification(&self, notification: Notification) -> Result<(), ExchangeError> {
        // 发送到全局通道
        self.trade_sender.send(notification.clone()).map_err(|e| {
            ExchangeError::InternalError(format!("Failed to send notification: {}", e))
        })?;

        // 发送到用户特定的订阅者
        let user_id = match &notification {
            Notification::Trade(t) => &t.user_id,
            Notification::AccountUpdate(a) => &a.user_id,
            Notification::OrderStatus(o) => &o.user_id,
        };

        if let Some(subs) = self.subscribers.get(user_id) {
            for sender in subs.read().iter() {
                let _ = sender.send(notification.clone()); // 忽略发送失败
            }
        }

        // 发送到全局订阅者 (crossbeam)
        for sender in self.global_subscribers.read().iter() {
            let _ = sender.send(notification.clone());
        }

        // 发送到全局订阅者 (tokio mpsc) - 异步非阻塞
        for sender in self.global_tokio_subscribers.read().iter() {
            let _ = sender.try_send(notification.clone()); // try_send 不阻塞
        }

        // 发送到新的 notification 系统（用于 WAL/Storage）
        if let Some(broker) = &self.notification_broker {
            if let Some(new_notification) = self.convert_to_new_notification(&notification) {
                if let Err(e) = broker.publish(new_notification) {
                    log::warn!("Failed to publish to notification broker: {}", e);
                }
            }
        }

        Ok(())
    }

    /// 转换旧的 Notification 到新的 Notification 系统
    fn convert_to_new_notification(&self, old: &Notification) -> Option<NewNotification> {
        match old {
            Notification::Trade(trade) => {
                Some(NewNotification::new(
                    NotificationType::TradeExecuted,
                    Arc::from(trade.user_id.clone()),
                    NotificationPayload::TradeExecuted(TradeExecutedNotify {
                        trade_id: trade.trade_id.clone(),
                        order_id: trade.order_id.clone(),
                        exchange_order_id: trade.order_id.clone(), // 使用 order_id 作为 exchange_order_id
                        instrument_id: trade.instrument_id.clone(),
                        direction: trade.direction.clone(),
                        offset: trade.offset.clone(),
                        price: trade.price,
                        volume: trade.volume,
                        commission: trade.commission,
                        fill_type: "FULL".to_string(), // TradeNotification 没有区分全部/部分成交
                        timestamp: trade.timestamp,
                    }),
                    "TradeGateway",
                ))
            }
            Notification::AccountUpdate(account) => {
                Some(NewNotification::new(
                    NotificationType::AccountUpdate,
                    Arc::from(account.user_id.clone()),
                    NotificationPayload::AccountUpdate(AccountUpdateNotify {
                        user_id: account.user_id.clone(),
                        balance: account.balance,
                        available: account.available,
                        frozen: 0.0, // 旧的 AccountUpdateNotification 没有 frozen 字段
                        margin: account.margin,
                        position_profit: account.position_profit,
                        close_profit: 0.0, // 旧的 AccountUpdateNotification 没有 close_profit 字段
                        risk_ratio: account.risk_ratio,
                        timestamp: account.timestamp,
                    }),
                    "TradeGateway",
                ))
            }
            Notification::OrderStatus(order) => {
                let (msg_type, payload) = match order.status.as_str() {
                    "ACCEPTED" => (
                        NotificationType::OrderAccepted,
                        NotificationPayload::OrderAccepted(OrderAcceptedNotify {
                            order_id: order.order_id.clone(),
                            exchange_order_id: order.exchange_order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            direction: order.direction.clone(),
                            offset: order.offset.clone(),
                            price: order.price,
                            volume: order.volume, // 委托量
                            order_type: order.price_type.clone(),
                            frozen_margin: 0.0, // 交易所回报没有 frozen_margin，需账户管理器计算
                            timestamp: order.timestamp,
                        }),
                    ),
                    "FILLED" => (
                        NotificationType::OrderFilled,
                        NotificationPayload::OrderFilled(OrderFilledNotify {
                            order_id: order.order_id.clone(),
                            exchange_order_id: order.exchange_order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            filled_volume: order.volume, // 本次成交量
                            average_price: order.price,  // 成交价格
                            timestamp: order.timestamp,
                        }),
                    ),
                    "PARTIAL_FILLED" => (
                        NotificationType::OrderPartiallyFilled,
                        NotificationPayload::OrderPartiallyFilled(OrderPartiallyFilledNotify {
                            order_id: order.order_id.clone(),
                            exchange_order_id: order.exchange_order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            filled_volume: order.volume, // 本次成交量
                            remaining_volume: 0.0,       // 交易所回报没有剩余量，需账户管理器计算
                            average_price: order.price,
                            timestamp: order.timestamp,
                        }),
                    ),
                    "CANCELLED" => (
                        NotificationType::OrderCanceled,
                        NotificationPayload::OrderCanceled(OrderCanceledNotify {
                            order_id: order.order_id.clone(),
                            exchange_order_id: order.exchange_order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            reason: order
                                .reason
                                .clone()
                                .unwrap_or_else(|| "User cancelled".to_string()),
                            timestamp: order.timestamp,
                        }),
                    ),
                    "REJECTED" => (
                        NotificationType::OrderRejected,
                        NotificationPayload::OrderRejected(OrderRejectedNotify {
                            order_id: order.order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            reason: order
                                .reason
                                .clone()
                                .unwrap_or_else(|| "Order rejected".to_string()),
                            error_code: 0,
                            timestamp: order.timestamp,
                        }),
                    ),
                    "CANCEL_REJECTED" => (
                        NotificationType::OrderRejected,
                        NotificationPayload::OrderRejected(OrderRejectedNotify {
                            order_id: order.order_id.clone(),
                            instrument_id: order.instrument_id.clone(),
                            reason: order
                                .reason
                                .clone()
                                .unwrap_or_else(|| "Cancel rejected".to_string()),
                            error_code: 0,
                            timestamp: order.timestamp,
                        }),
                    ),
                    _ => return None, // 未知状态
                };

                Some(NewNotification::new(
                    msg_type,
                    Arc::from(order.user_id.clone()),
                    payload,
                    "TradeGateway",
                ))
            }
        }
    }

    /// 订阅用户通知
    pub fn subscribe_user(&self, user_id: String) -> Receiver<Notification> {
        let (sender, receiver) = unbounded();

        self.subscribers
            .entry(user_id)
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(sender);

        receiver
    }

    /// 订阅全局通知 (crossbeam channel)
    pub fn subscribe_global(&self) -> Receiver<Notification> {
        let (sender, receiver) = unbounded();
        self.global_subscribers.write().push(sender);
        receiver
    }

    /// 订阅全局通知 (tokio mpsc) - 用于异步任务
    pub fn subscribe_global_tokio(&self, sender: tokio::sync::mpsc::Sender<Notification>) {
        self.global_tokio_subscribers.write().push(sender);
    }

    /// 获取通知接收器 (主通道)
    pub fn get_receiver(&self) -> &Receiver<Notification> {
        &self.trade_receiver
    }

    /// 生成成交ID
    fn generate_trade_id(&self) -> String {
        let seq = self
            .trade_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = Utc::now().timestamp_millis();
        format!("T{}{:010}", timestamp, seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{AccountType, OpenAccountRequest};

    fn create_test_gateway() -> (TradeGateway, Arc<AccountManager>, String) {
        let account_mgr = Arc::new(AccountManager::new());

        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: None,
            account_name: "Test User".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        let account_id = account_mgr.open_account(req).unwrap();

        // 使用项目内测试目录存储WAL数据 (output/testexchange/)
        let gateway =
            TradeGateway::new(account_mgr.clone()).with_wal_root("./output/testexchange/wal");

        (gateway, account_mgr, account_id)
    }

    #[test]
    fn test_generate_trade_id() {
        let (gateway, _, _) = create_test_gateway();

        let id1 = gateway.generate_trade_id();
        let id2 = gateway.generate_trade_id();

        assert_ne!(id1, id2);
        assert!(id1.starts_with('T'));
        assert!(id2.starts_with('T'));
    }

    #[test]
    fn test_subscribe_user() {
        let (gateway, _, account_id) = create_test_gateway();

        let receiver = gateway.subscribe_user(account_id.clone());

        // 创建测试通知
        let notification = Notification::Trade(TradeNotification {
            trade_id: "T001".to_string(),
            user_id: account_id.clone(),
            order_id: "O001".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 120.0,
            volume: 10.0,
            timestamp: 0,
            commission: 0.36,
        });

        gateway.send_notification(notification).unwrap();

        // 接收通知
        let received = receiver.try_recv().unwrap();
        if let Notification::Trade(t) = received {
            assert_eq!(t.trade_id, "T001");
            assert_eq!(t.user_id, account_id);
        } else {
            panic!("Expected Trade notification");
        }
    }

    #[test]
    #[ignore] // TODO: Phase 3 重构后更新此测试
    fn test_handle_accepted() {
        let (_gateway, _, _account_id) = create_test_gateway();

        // let receiver = gateway.subscribe_user(account_id.clone());
        // gateway.handle_accepted(...).unwrap();
        // let received = receiver.try_recv().unwrap();
        // ...
    }

    #[tokio::test]
    async fn test_diff_snapshot_manager_integration() {
        use crate::protocol::diff::snapshot::SnapshotManager;

        let (mut gateway, _, account_id) = create_test_gateway();

        // 创建并设置 SnapshotManager
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        gateway.set_snapshot_manager(snapshot_mgr.clone());

        // 验证 SnapshotManager 已设置
        assert!(gateway.snapshot_manager().is_some());

        // 初始化用户快照（使用 user_id，不是 account_id）
        let user_id = "test_user";
        snapshot_mgr.initialize_user(user_id).await;

        // 启动 peek 任务
        let peek_task = tokio::spawn({
            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = user_id.to_string();
            async move { snapshot_mgr.peek(&user_id).await }
        });

        // 等待一小段时间确保 peek 任务开始等待
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 推送账户更新（这会触发 DIFF patch）
        // 注意：push_account_update 需要 user_id，不是 account_id
        gateway.push_account_update("test_user").unwrap();

        // 等待 peek 返回
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(2), peek_task).await;

        assert!(result.is_ok(), "peek() should return within timeout");
        let patches = result.unwrap().unwrap();
        assert!(patches.is_some(), "Should receive patches");

        let patches = patches.unwrap();
        assert!(!patches.is_empty(), "Should have at least one patch");

        // 验证 patch 包含账户数据
        let patch_str = serde_json::to_string(&patches[0]).unwrap();
        assert!(
            patch_str.contains("accounts") || patch_str.contains("balance"),
            "Patch should contain account data"
        );
    }

    #[tokio::test]
    async fn test_diff_multiple_patches() {
        use crate::protocol::diff::snapshot::SnapshotManager;

        let (mut gateway, _, account_id) = create_test_gateway();

        // 创建并设置 SnapshotManager
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        gateway.set_snapshot_manager(snapshot_mgr.clone());

        // 初始化用户快照（使用 user_id，不是 account_id）
        let user_id = "test_user";
        snapshot_mgr.initialize_user(user_id).await;

        // 启动 peek 任务
        let peek_task = tokio::spawn({
            let snapshot_mgr = snapshot_mgr.clone();
            let user_id = user_id.to_string();
            async move { snapshot_mgr.peek(&user_id).await }
        });

        // 等待 peek 开始
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 推送多个账户更新（模拟多次成交）
        // 注意：push_account_update 需要 user_id，不是 account_id
        gateway.push_account_update("test_user").unwrap();

        // 等待一小段时间确保异步任务完成
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 等待 peek 返回
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(2), peek_task).await;

        assert!(result.is_ok(), "peek() should return within timeout");
        let patches = result.unwrap().unwrap();
        assert!(patches.is_some(), "Should receive patches");

        let patches = patches.unwrap();
        assert!(!patches.is_empty(), "Should have at least one patch");

        // 验证 patch 内容包含账户数据
        let all_patches_str = serde_json::to_string(&patches).unwrap();
        assert!(
            all_patches_str.contains("accounts") || all_patches_str.contains("balance"),
            "Patches should contain account data"
        );
    }

    #[test]
    fn test_snapshot_manager_getter() {
        use crate::protocol::diff::snapshot::SnapshotManager;

        let (mut gateway, _, _) = create_test_gateway();

        // 初始状态应该是 None
        assert!(gateway.snapshot_manager().is_none());

        // 设置 SnapshotManager
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        gateway.set_snapshot_manager(snapshot_mgr.clone());

        // 验证已设置
        assert!(gateway.snapshot_manager().is_some());

        // 验证是同一个实例
        let retrieved = gateway.snapshot_manager().unwrap();
        assert!(Arc::ptr_eq(retrieved, &snapshot_mgr));
    }

    // ==================== Phase 3: 新方法测试 ====================

    #[test]
    fn test_handle_order_accepted_new() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let order_id = "O001";

        // 第一次调用
        let exchange_order_id_1 = gateway
            .handle_order_accepted_new(
                "SHFE",
                instrument_id,
                &account_id,
                order_id,
                "BUY",
                "OPEN",
                "LIMIT",
                50000.0,
                10.0,
            )
            .unwrap();

        // 验证 exchange_order_id 是递增的
        assert_eq!(exchange_order_id_1, 1);

        // 第二次调用
        let exchange_order_id_2 = gateway
            .handle_order_accepted_new(
                "SHFE",
                instrument_id,
                &account_id,
                "O002",
                "SELL",
                "OPEN",
                "LIMIT",
                51000.0,
                5.0,
            )
            .unwrap();

        assert_eq!(exchange_order_id_2, 2);
        assert!(exchange_order_id_2 > exchange_order_id_1);
    }

    #[test]
    fn test_handle_order_rejected_new() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let order_id = "O001";
        let reason = "Insufficient margin";

        let exchange_order_id = gateway
            .handle_order_rejected_new(
                "SHFE",
                instrument_id,
                &account_id,
                order_id,
                "BUY",
                "OPEN",
                "LIMIT",
                50000.0,
                10.0,
                reason,
            )
            .unwrap();

        // 验证 exchange_order_id 是递增的
        assert_eq!(exchange_order_id, 1);
    }

    #[test]
    fn test_handle_trade_new() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let exchange_order_id = 1i64;
        let order_id = "O001";
        let volume = 10.0;
        let price = 50000.0;

        let trade_id = gateway
            .handle_trade_new(
                "SHFE",
                instrument_id,
                exchange_order_id,
                &account_id,
                order_id,
                "BUY",
                "OPEN",
                volume,
                price,
                Some(2i64),
            )
            .unwrap();

        // 验证 trade_id 是递增的
        assert_eq!(trade_id, 1);

        // 第二次成交
        let trade_id_2 = gateway
            .handle_trade_new(
                "SHFE",
                instrument_id,
                exchange_order_id,
                &account_id,
                order_id,
                "BUY",
                "OPEN",
                5.0,
                50100.0,
                Some(3i64),
            )
            .unwrap();

        assert_eq!(trade_id_2, 2);
        assert!(trade_id_2 > trade_id);
    }

    #[test]
    fn test_handle_cancel_accepted_new() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let exchange_order_id = 1i64;
        let order_id = "O001";

        let result = gateway.handle_cancel_accepted_new(
            "SHFE",
            instrument_id,
            exchange_order_id,
            &account_id,
            order_id,
            "BUY",
            "OPEN",
            "LIMIT",
            50000.0,
            5.0,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_cancel_rejected_new() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let exchange_order_id = 1i64;
        let order_id = "O001";
        let reason = "Order already filled";

        let result = gateway.handle_cancel_rejected_new(
            "SHFE",
            instrument_id,
            exchange_order_id,
            &account_id,
            order_id,
            reason,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_unified_sequence_across_events() {
        let (gateway, _, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";

        // 下单事件 (sequence = 1)
        let exchange_order_id = gateway
            .handle_order_accepted_new(
                "SHFE",
                instrument_id,
                &account_id,
                "O001",
                "BUY",
                "OPEN",
                "LIMIT",
                50000.0,
                10.0,
            )
            .unwrap();
        assert_eq!(exchange_order_id, 1);

        // 成交事件 (sequence = 2)
        let trade_id = gateway
            .handle_trade_new(
                "SHFE",
                instrument_id,
                exchange_order_id,
                &account_id,
                "O001",
                "BUY",
                "OPEN",
                10.0,
                50000.0,
                Some(exchange_order_id + 1),
            )
            .unwrap();
        assert_eq!(trade_id, 2);

        // 下单事件 (sequence = 3)
        let exchange_order_id_2 = gateway
            .handle_order_accepted_new(
                "SHFE",
                instrument_id,
                &account_id,
                "O002",
                "SELL",
                "OPEN",
                "LIMIT",
                51000.0,
                5.0,
            )
            .unwrap();
        assert_eq!(exchange_order_id_2, 3);

        // 验证序列严格递增
        assert!(trade_id > exchange_order_id);
        assert!(exchange_order_id_2 > trade_id);
    }

    #[test]
    fn test_different_instruments_independent_sequences() {
        let (gateway, _, account_id) = create_test_gateway();

        let cu_instrument = "SHFE.cu2501";
        let ag_instrument = "SHFE.ag2501";

        // cu2501 的序列
        let cu_order_1 = gateway
            .handle_order_accepted_new(
                "SHFE",
                cu_instrument,
                &account_id,
                "O001",
                "BUY",
                "OPEN",
                "LIMIT",
                50000.0,
                10.0,
            )
            .unwrap();
        assert_eq!(cu_order_1, 1);

        // ag2501 的序列（独立计数）
        let ag_order_1 = gateway
            .handle_order_accepted_new(
                "SHFE",
                ag_instrument,
                &account_id,
                "O002",
                "BUY",
                "OPEN",
                "LIMIT",
                4000.0,
                20.0,
            )
            .unwrap();
        assert_eq!(ag_order_1, 1);

        // cu2501 继续递增
        let cu_order_2 = gateway
            .handle_order_accepted_new(
                "SHFE",
                cu_instrument,
                &account_id,
                "O003",
                "SELL",
                "OPEN",
                "LIMIT",
                51000.0,
                5.0,
            )
            .unwrap();
        assert_eq!(cu_order_2, 2);

        // ag2501 继续递增
        let ag_order_2 = gateway
            .handle_order_accepted_new(
                "SHFE",
                ag_instrument,
                &account_id,
                "O004",
                "SELL",
                "OPEN",
                "LIMIT",
                4100.0,
                15.0,
            )
            .unwrap();
        assert_eq!(ag_order_2, 2);
    }
}
