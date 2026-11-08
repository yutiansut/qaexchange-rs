//! 成交回报网关
//!
//! 负责处理撮合引擎的成交结果，更新账户，并推送成交回报到客户端

use crate::core::{Order, QA_Account, Trade};
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
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
use crossbeam::channel::{bounded, unbounded, Receiver, Sender};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

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

/// 诊断旁路通道容量 —— 见 `TradeGateway::new` 的说明
const NOTIFICATION_TAP_CAPACITY: usize = 8192;

/// 旁路通道因满而丢弃的通知数(仅诊断用,不影响真实投递)
pub static NOTIFICATION_TAP_DROPPED: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

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
    /// ⚠️ 用 `OnceLock` 而不是 `Option` —— 因为它是在 `Arc<TradeGateway>` **已被 clone
    /// 之后**才设置的,原实现靠 `Arc::as_ptr(..) as *mut TradeGateway` 强改,那是 UB。
    /// @yutiansut @quantaxis
    ///
    /// 原写法(`service/websocket/mod.rs:78`、`main.rs:541`):
    /// ```ignore
    /// unsafe {
    ///     let p = Arc::as_ptr(&trade_gateway) as *mut TradeGateway;
    ///     (*p).set_snapshot_manager(mgr);      // 安全性注释写着「此时只有一个引用」
    /// }
    /// ```
    /// **那句注释是错的**:`main.rs:364` 早在 178 行之前就把 `trade_gateway.clone()`
    /// 交给了 `OrderRouter`。`Arc<T>` 只提供共享(不可变)访问,通过裸指针写入
    /// 违反别名规则;而这两个字段的读取点全在成交/撤单**热路径**上
    /// (:321 / :434 / :940 / :1695 / :1776 / :1809),跨线程读写即数据竞争。
    ///
    /// `OnceLock` 语义正好吻合「启动时设一次、之后只读」:`set(&self)` 不需要 `&mut`,
    /// 读取 `get()` 无锁,热路径零额外开销。
    snapshot_mgr: OnceLock<Arc<SnapshotManager>>,

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
    /// 同 `snapshot_mgr`:启动时设一次、热路径只读。@yutiansut @quantaxis
    market_data_service: OnceLock<Arc<crate::market::MarketDataService>>,

    /// 交易所单号(router order_id)→ 客户端编号(DIFF insert_order 的 order_id)。
    /// DIFF `trade.<account>.orders/trades` 的键用客户端编号(DIFF/TqSdk 惯例,
    /// hft_maker 与 QIFI 对账也按它);没给客户端编号的订单仍用交易所单号。@yutiansut @quantaxis
    client_order_ids: DashMap<String, String>,

    /// 每个账户的回报序号(EIFI §6 序号水位线):`trade.<account>.orders/trades` 每个补丁 +1
    diff_seqno: DashMap<String, u64>,
}

/// 成交后应写入的 `last_msg`;已有非空值时返回 `None`(不覆盖)。@yutiansut @quantaxis
///
/// QIFI 里「已撤单」与「全部成交」的 `status` 同为 `FINISHED`,
/// 区分信息只在 `last_msg`。取值与 `diff_messages.rs:363-365` 的契约一致。
fn fill_last_msg(current: &str, volume_left: f64) -> Option<String> {
    if !current.is_empty() {
        return None;
    }
    Some(if volume_left <= 0.0 {
        "全部成交".to_string()
    } else {
        "部分成交".to_string()
    })
}

impl TradeGateway {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        // ✨ 有界 —— 这条通道在生产里没有消费者 @yutiansut @quantaxis
        //
        // `get_receiver()`(唯一的排空入口)只被 2 个测试调用,生产零调用。
        // 而 `send_notification` 有 9 个生产调用点(每笔成交/每次委托状态变化/
        // 每次账户更新),原先用 unbounded() 就是只进不出的黑洞:压测下
        // ~25 万单/小时 × 数条通知,全部永久驻留内存。
        //
        // 真实投递靠后面 4 条路径(per-user / global / global_tokio / broker)。
        // 这条只是测试与诊断用的旁路 tap,因此改为有界 + 满则丢弃。
        let (trade_sender, trade_receiver) = bounded(NOTIFICATION_TAP_CAPACITY);

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
            snapshot_mgr: OnceLock::new(),
            instrument_wal_managers: DashMap::new(),
            account_wal_managers: DashMap::new(),
            wal_root: "./data/wal".to_string(), // 默认 WAL 根目录
            trade_recorder: None,
            market_data_service: OnceLock::new(),
            client_order_ids: DashMap::new(),
            diff_seqno: DashMap::new(),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ✨ DIFF 交易回报:`trade.<account_id>.{orders,trades,positions,accounts}` = QIFI 切片
    // (2026-11-08,hft_maker 接入;依据 qaultraoms docs/EIFI.md §4/§5/§6/§7.3)@yutiansut @quantaxis
    // ═══════════════════════════════════════════════════════════════════════

    /// 登记客户端编号(DIFF insert_order 的 order_id)
    pub fn register_client_order_id(&self, exchange_order_id: &str, client_order_id: &str) {
        self.client_order_ids
            .insert(exchange_order_id.to_string(), client_order_id.to_string());
    }

    /// DIFF 订单键:有客户端编号用客户端编号,否则用交易所单号
    pub fn diff_order_key(&self, exchange_order_id: &str) -> String {
        self.client_order_ids
            .get(exchange_order_id)
            .map(|e| e.value().clone())
            .unwrap_or_else(|| exchange_order_id.to_string())
    }

    /// 合约所属交易所(取合约配置)。订单/成交路径上的 `exchange_id` 是占位串 "EXCHANGE",
    /// qars 持仓的 exchange_id 是空串 —— QIFI 视图一律以合约配置为准。@yutiansut @quantaxis
    pub fn exchange_of(&self, instrument_id: &str, fallback: &str) -> String {
        self.market_data_service
            .get()
            .and_then(|m| m.get_instrument_config(instrument_id).map(|c| c.exchange_id.clone()))
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| {
                if fallback.is_empty() || fallback == "EXCHANGE" { String::new() } else { fallback.to_string() }
            })
    }

    /// 把 `trade.<account_id>.<section>.<key> = value` **同步**推入截面。
    ///
    /// - 按主题扇出:账户 ID(按账户 ID 连接的会话)+ 属主用户(按真实 user_id 连接的会话),
    ///   绑定了其中任一主题的**每条连接各收一份**(同一用户开行情/交易两条连接都要收到回报;
    ///   同时绑定两个主题的连接只收一份)。同一用户名下多个账户持同一合约时,嵌在账户下就不会互相覆盖。
    /// - `orders` / `trades` 段写入账户内单调的 `seqno`(EIFI §6 序号水位线)。
    /// - **不取任何账户锁**(属主从 metadata 取),可在持账户写锁时调用。
    pub fn publish_trade_section(
        &self,
        account_id: &str,
        section: &str,
        key: &str,
        value: serde_json::Value,
    ) {
        self.publish_trade_section_to(None, account_id, section, key, value);
    }

    /// 同 `publish_trade_section`;`conn = Some(连接)` 时**只推给这条连接**
    /// (登录时给新连接补当前状态,不必让同用户的其他连接再收一遍)。@yutiansut @quantaxis
    pub fn publish_trade_section_to(
        &self,
        conn: Option<&str>,
        account_id: &str,
        section: &str,
        key: &str,
        mut value: serde_json::Value,
    ) {
        let Some(snapshot_mgr) = self.snapshot_mgr.get() else { return };
        if section == "orders" || section == "trades" {
            let seq = {
                let mut e = self.diff_seqno.entry(account_id.to_string()).or_insert(0);
                *e += 1;
                *e
            };
            if let Some(obj) = value.as_object_mut() {
                obj.insert("seqno".to_string(), serde_json::json!(seq));
            }
        }
        // 主题上没有连接就不构造补丁 —— 之后连上的会话由登录初始快照补齐当前状态
        // (`publish_account_views_to` + 活单,见 DiffHandler 登录分支)。@yutiansut @quantaxis
        let owner = self.account_mgr.get_account_owner(account_id);
        let mut topics: Vec<&str> = vec![account_id];
        if let Some(o) = owner.as_deref().filter(|o| *o != account_id) {
            topics.push(o);
        }
        if conn.is_none() && !topics.iter().any(|t| snapshot_mgr.has_subscribers(t)) {
            return;
        }
        let patch = serde_json::json!({
            "trade": { account_id: { section: { key: value } } }
        });
        match conn {
            Some(c) => {
                snapshot_mgr.push_to_connection(c, patch);
            }
            None => {
                snapshot_mgr.push_patch_topics(&topics, patch);
            }
        }
    }

    /// 旧格式(顶层 orders/trades/accounts)补丁只有按账户 ID 连接的会话读得到 ——
    /// 账户主题上没有连接就不推(否则 80 个走 HTTP 的仿真账户每个回报都要白构造一次)。
    fn legacy_snapshot_active(&self, account_id: &str) -> bool {
        self.snapshot_mgr
            .get()
            .map_or(false, |m| m.has_subscribers(account_id))
    }

    /// 账户的 DIFF 标准视图:`accounts.CNY`(QIFI Account,已按规范口径校正)与
    /// `positions.<EXCHANGE.INSTRUMENT>`(QIFI Position)。调用方需持账户写锁
    /// (`get_qifi_position` / `get_accountmessage` 要 &mut)。
    fn account_views(
        &self,
        acc: &mut crate::QA_Account,
    ) -> Vec<(&'static str, String, serde_json::Value)> {
        let exch_of = |inst: &str| -> String { self.exchange_of(inst, "") };
        let mut out: Vec<(&'static str, String, serde_json::Value)> = acc
            .hold
            .values_mut()
            .map(|pos| {
                let mut q = pos.get_qifi_position();
                if q.exchange_id.is_empty() {
                    q.exchange_id = exch_of(&q.instrument_id);
                }
                let key = if q.exchange_id.is_empty() {
                    q.instrument_id.clone()
                } else {
                    format!("{}.{}", q.exchange_id, q.instrument_id)
                };
                ("positions", key, serde_json::to_value(&q).unwrap_or_default())
            })
            .collect();
        let mut qacc = acc.get_accountmessage();
        crate::exchange::account_mgr::conform_qifi_account(&mut qacc);
        out.push(("accounts", "CNY".to_string(), serde_json::to_value(&qacc).unwrap_or_default()));
        out
    }

    /// 登录初始快照用:按 DIFF 标准推一次某账户的 accounts / positions。
    pub fn publish_account_views(&self, account_id: &str) {
        self.publish_account_views_to(None, account_id);
    }

    /// 同 `publish_account_views`;`conn = Some(连接)` 时只推给这条连接(登录初始快照)
    pub fn publish_account_views_to(&self, conn: Option<&str>, account_id: &str) {
        let Ok(account) = self.account_mgr.get_account(account_id) else { return };
        let views = {
            let mut acc = account.write();
            self.account_views(&mut acc)
        };
        for (section, key, val) in views {
            self.publish_trade_section_to(conn, account_id, section, &key, val);
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
    /// 同 `set_snapshot_manager`:`OnceLock` 只要 `&self`,调用方不必再用 UB 写法。
    pub fn set_market_data_service(
        &self,
        market_data_service: Arc<crate::market::MarketDataService>,
    ) {
        if self.market_data_service.set(market_data_service).is_err() {
            log::warn!("MarketDataService 已设置过,本次调用被忽略");
        }
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
    /// 设置 SnapshotManager(启动时一次)。重复设置会被忽略并告警 —— 不再需要 `&mut self`,
    /// 因此调用方**不必**再用 `Arc::as_ptr(..) as *mut` 那套 UB 写法。@yutiansut @quantaxis
    pub fn set_snapshot_manager(&self, snapshot_mgr: Arc<SnapshotManager>) {
        if self.snapshot_mgr.set(snapshot_mgr).is_err() {
            log::warn!("SnapshotManager 已设置过,本次调用被忽略(启动路径应只设一次)");
        }
    }

    /// 获取 DIFF 快照管理器
    pub fn snapshot_manager(&self) -> Option<&Arc<SnapshotManager>> {
        self.snapshot_mgr.get()
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
            "", // 旧路径没有交易所成交号
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
        if let Some(snapshot_mgr) = self.snapshot_mgr.get() {
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

        log::debug!(
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
            "", // 旧路径没有交易所成交号
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
        if let Some(snapshot_mgr) = self.snapshot_mgr.get() {
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

        log::debug!(
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
            direction: WalRecord::encode_direction(direction),
            offset: WalRecord::encode_offset(offset),
            price_type: match price_type {
                "LIMIT" => 0,
                "MARKET" => 1,
                _ => 0,
            },
            price,
            volume,
            time: timestamp,
            internal_order_id: WalRecord::to_fixed_array_32(order_id),
            user_id: WalRecord::to_fixed_array_40(user_id),
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
            user_id: WalRecord::to_fixed_array_40(user_id),
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

        // Phase 14: 写入订单状态更新到 WAL @yutiansut @quantaxis
        // 订单接受，状态=0 (ALIVE)
        let direction_u8 = WalRecord::encode_direction(direction);
        let offset_u8 = WalRecord::encode_offset(offset);

        // 获取当前冻结保证金（从账户中读取）
        let frozen_margin = if let Ok(account) = self.account_mgr.get_account(user_id) {
            let acc = account.read();
            acc.frozen.get(order_id)
                .map(|frozen_order| frozen_order.money)
                .unwrap_or(0.0)
        } else {
            0.0
        };

        if let Err(e) = self.write_order_status_update(
            order_id, // qars 内部订单ID
            user_id,
            instrument_id,
            0, // ALIVE
            volume,    // volume_orign
            volume,    // volume_left (全部未成交)
            0.0,       // volume_filled (尚未成交)
            frozen_margin,
            0.0,       // frozen_amount
            direction_u8,
            offset_u8,
            price,
            0.0,       // avg_price (尚未成交)
            "订单接受",
        ) {
            log::error!("Failed to write OrderStatusUpdate WAL for accepted order: {}", e);
        }

        log::debug!(
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

        // Phase 14: 写入订单状态更新到 WAL @yutiansut @quantaxis
        // 订单拒绝，状态=3 (REJECTED)
        let direction_u8 = WalRecord::encode_direction(direction);
        let offset_u8 = WalRecord::encode_offset(offset);

        if let Err(e) = self.write_order_status_update(
            order_id, // qars 内部订单ID
            user_id,
            instrument_id,
            3, // REJECTED
            volume,    // volume_orign
            volume,    // volume_left (全部未成交)
            0.0,       // volume_filled
            0.0,       // frozen_margin (未冻结)
            0.0,       // frozen_amount
            direction_u8,
            offset_u8,
            price,
            0.0,       // avg_price
            reason,    // 拒绝原因
        ) {
            log::error!("Failed to write OrderStatusUpdate WAL for rejected order: {}", e);
        }

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
    /// @yutiansut @quantaxis
    /// is_taker: 是否为主动方（taker），只有 taker 才记录到 TradeRecorder（避免重复记录）
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
        opposite_user_id: Option<&str>, // ✨ 对手方user_id，用于成交记录正确区分买卖方 @yutiansut @quantaxis
        qa_order_id: &str, // ✨ qars内部订单ID，用于调用receive_deal_sim @yutiansut @quantaxis
        opposite_order_id_str: Option<&str>, // ✨ 对手方订单ID字符串，用于成交记录 @yutiansut @quantaxis
        is_taker: bool, // ✨ 是否为主动方，只有 taker 记录到 TradeRecorder @yutiansut @quantaxis
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
            user_id: WalRecord::to_fixed_array_40(user_id),
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
        // ✨ 修复：只有 taker 才记录成交，避免重复记录 @yutiansut @quantaxis
        // taker 是主动方（新下单的一方），maker 是被动方（挂在订单簿上的一方）
        if is_taker {
            if let Some(recorder) = &self.trade_recorder {
                // 成交的交易日必须随交易所本地日历,见 settlement.rs:615 的说明。
                // @yutiansut @quantaxis
                let trading_day = chrono::Local::now().format("%Y-%m-%d").to_string();

                // 根据 direction 确定买卖方的 user_id
                let (buy_user_id, sell_user_id) = match direction {
                    "BUY" => {
                        // 当前方（taker）是买方，对手方（maker）是卖方
                        let sell_id = opposite_user_id.unwrap_or(user_id).to_string();
                        (user_id.to_string(), sell_id)
                    }
                    "SELL" => {
                        // 当前方（taker）是卖方，对手方（maker）是买方
                        let buy_id = opposite_user_id.unwrap_or(user_id).to_string();
                        (buy_id, user_id.to_string())
                    }
                    _ => (user_id.to_string(), user_id.to_string()), // fallback
                };

                // ✨ 根据 direction 确定 buy_order_id 和 sell_order_id @yutiansut @quantaxis
                let opposite_id = opposite_order_id_str
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("opposite_{}", opposite_order_id.unwrap_or(0)));

                let (buy_order_id, sell_order_id) = match direction {
                    "BUY" => (order_id.to_string(), opposite_id),
                    "SELL" => (opposite_id, order_id.to_string()),
                    _ => (order_id.to_string(), "unknown".to_string()),
                };

                // taker_order_id 就是当前订单ID（主动方）
                let taker_order_id = order_id.to_string();

                recorder.record_trade(
                    instrument_id.to_string(),
                    buy_user_id,    // ✨ 正确的买方user_id
                    sell_user_id,   // ✨ 正确的卖方user_id
                    buy_order_id,   // ✨ 正确的买方order_id
                    sell_order_id,  // ✨ 正确的卖方order_id
                    taker_order_id, // ✨ 主动方订单ID
                    price,
                    volume,
                    trading_day,
                    // 用 qars 的权威公式,不复制费率 —— `preset` 里带 unit_table,
                    // 手写 `price * volume * rate` 必然漏合约乘数。@yutiansut @quantaxis
                    // 生效费率 = qars 预设 + 按合约覆盖(core::account_ext)
                    crate::core::account_ext::effective_preset(instrument_id)
                        .calc_commission(price, volume),
                );
            }
        } else {
            log::debug!("🔍 Skipping trade record for maker (is_taker=false): order_id={}", order_id);
        }

        // 更新快照生成器的成交统计
        // ✨ 只在 taker 侧做:本函数对买卖双方各调用一次,不设门会双计 @yutiansut @quantaxis
        if is_taker {
        if let Some(mds) = self.market_data_service.get() {
            let turnover = price * volume;
            mds.update_trade_stats(instrument_id, volume as i64, turnover);
            let dir_str = if direction.eq_ignore_ascii_case("BUY") { "buy" } else { "sell" };
            mds.on_trade(instrument_id, price, volume as i64, dir_str);
            log::trace!(
                "Updated snapshot stats: {} volume={}, turnover={:.2}",
                instrument_id,
                volume,
                turnover
            );
        }
        }

        // ✨ 关键修复：调用receive_deal_sim更新账户持仓和资金 @yutiansut @quantaxis
        log::debug!("🔧 Updating account for trade: user={}, instrument={}, {} {}, price={}, volume={}, qa_order_id={}",
            user_id, instrument_id, direction, offset, price, volume, qa_order_id);

        let (order_status, volume_left, _volume_orign) = self.update_account(
            user_id,
            instrument_id,
            direction,
            offset,
            price,
            volume,
            qa_order_id,
            &trade_id.to_string(), // 撮合成交号 → QIFI exchange_trade_id
        )?;

        log::debug!("✅ Account updated after trade: order_id={}, status={}, volume_left={}",
            order_id, order_status, volume_left);

        // Phase 14: 写入订单状态更新到 WAL @yutiansut @quantaxis
        // 根据成交后的剩余量判断状态
        let status_u8 = if volume_left <= 0.0 {
            1 // FINISHED (全部成交)
        } else {
            4 // PARTIALLY_FILLED (部分成交)
        };
        let direction_u8 = WalRecord::encode_direction(direction);
        let offset_u8 = WalRecord::encode_offset(offset);

        // 获取当前冻结保证金（从账户中读取）
        let frozen_margin = if let Ok(account) = self.account_mgr.get_account(user_id) {
            let acc = account.read();
            acc.frozen.get(qa_order_id)
                .map(|frozen_order| frozen_order.money)
                .unwrap_or(0.0)
        } else {
            0.0
        };

        if let Err(e) = self.write_order_status_update(
            qa_order_id, // 使用 qars 内部订单ID
            user_id,
            instrument_id,
            status_u8,
            volume + volume_left, // volume_orign (近似原始委托量)
            volume_left,          // volume_left
            volume,               // volume_filled (本次成交量)
            frozen_margin,        // frozen_margin
            0.0,                  // frozen_amount
            direction_u8,
            offset_u8,
            price,
            price, // avg_price (简化处理，用本次成交价)
            if status_u8 == 1 { "全部成交" } else { "部分成交" },
        ) {
            log::error!("Failed to write OrderStatusUpdate WAL after trade: {}", e);
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

        // ✨ DIFF 标准成交:`trade.<account>.trades.<order_key|exchange_trade_id>` = QIFI Trade
        //    键是 EIFI §6 / QIFI 规范的 `order_id|交易所成交号`;成交补丁**先于**使订单 FINISHED
        //    的订单补丁(订单补丁由 order_router 在本函数返回后推送)。@yutiansut @quantaxis
        //    手续费用 qars 权威公式(与 TradeRecorder 同一个),不用 create_trade_notification
        //    里那个漏了合约乘数的占位值。
        {
            let order_key = self.diff_order_key(order_id);
            let exchange_trade_id = trade_id.to_string();
            let key = format!("{}|{}", order_key, exchange_trade_id);
            let commission = crate::core::account_ext::effective_preset(instrument_id)
                .calc_commission(price, volume);
            self.publish_trade_section(
                user_id,
                "trades",
                &key,
                serde_json::json!({
                    "user_id": user_id,
                    "trade_id": key,
                    "exchange_id": self.exchange_of(instrument_id, exchange),
                    "instrument_id": instrument_id,
                    "order_id": order_key,
                    "exchange_order_id": order_id,
                    "exchange_trade_id": exchange_trade_id,
                    "direction": direction,
                    "offset": offset,
                    "volume": volume,
                    "price": price,
                    "trade_date_time": Utc::now().timestamp_nanos_opt().unwrap_or(0),
                    "commission": commission,
                }),
            );
        }
        self.push_account_update(user_id)?;

        log::debug!(
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
    /// 交易所撤单成功，推送CANCELLED状态回报给账户，并释放冻结资金
    /// ✨ 修复：添加 qa_order_id 参数，用于调用 qars cancel_order 释放冻结资金 @yutiansut @quantaxis
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
        qa_order_id: &str, // ✨ 新增：qars 内部订单ID，用于释放冻结资金
    ) -> Result<(), ExchangeError> {
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // ✨ 释放冻结资金：调用 qars cancel_order @yutiansut @quantaxis
        // user_id 在 qaexchange 中实际是 account_id
        //
        // ⚠️ 空 qa_order_id 必须提前挡掉,否则会 **panic 掉整个线程**。
        // @yutiansut @quantaxis
        //
        // `order_router.rs:1586` 取 qa_order_id 时,订单不在 `self.orders` 就回退成
        // `String::new()`;而下面的 `acc.cancel_order()` 是 qars 的
        // `QA_Account::cancel_order`(qars2 `account.rs:1227`),它内部是
        // `self.dailyorders.get_mut(order_id).expect("Order not found for order_id")`
        // —— 空串必然取不到,**直接 panic**。
        // 下面用 `match ... { Ok / Err }` 接,但 `match` 接不住 panic。
        //
        // 实测可达性低(`self.orders` 全仓无 remove 点,`grep -c 'orders.remove'` = 0),
        // 重启路径也安全(`restore_orders_from_accounts` 明确设了
        // `qa_order_id: order_id.clone()`)。但这是纵深防御:撤单回报里
        // 一个空 ID 不该能击穿到 panic。
        if qa_order_id.is_empty() {
            log::warn!(
                "撤单回报的 qa_order_id 为空,跳过 qars 冻结资金释放: account={}, exchange_order_id={}",
                user_id, exchange_order_id
            );
        } else if let Ok(account) = self.account_mgr.get_account(user_id) {
            let mut acc = account.write();

            // 调试日志:级别降为 debug,且 **克隆放进宏内惰性求值**。@yutiansut @quantaxis
            //
            // 原写法有两个问题:① 挂在 `log::info!` 上,而它是每笔撤单都打
            //(快照里有 326,913 笔撤单)—— 正常运行会刷屏;
            // ② `frozen_keys` 在宏**之外**先 collect,即使日志被级别过滤掉,
            // 这次 Vec<String> 分配也照样发生 —— 撤单热路径上的无谓开销。
            // 放进宏内后,debug 未开启时 `frozen` 一次都不会被遍历。
            log::debug!(
                "Attempting to release frozen funds: qa_order_id={}, account={}, frozen_keys={:?}",
                qa_order_id,
                user_id,
                acc.frozen.keys().collect::<Vec<_>>()
            );

            let money_before = acc.money;
            // 使用作用域限制借用范围
            let cancel_success = if !acc.dailyorders.contains_key(qa_order_id) {
                // 同上一段空 ID 的道理:委托已不在当日委托里(日终结算清过),qars
                // `cancel_order` 的 expect 会 panic(2026-11-08 隔离实例「结算 → 撤单」实测进程退出)。
                // 日终流程先撤光活单再结算,这里是纵深防御。@yutiansut @quantaxis
                log::warn!(
                    "撤单回报对应的委托不在账户当日委托里(已日终结算?),跳过 qars 撤单: account={}, qa_order_id={}",
                    user_id, qa_order_id
                );
                None
            } else {
                match acc.cancel_order(qa_order_id) {
                    Ok(cancelled_order) => {
                        // ✨ 填 `last_msg` —— QIFI 靠它区分「已撤单」与「全部成交」。
                        // @yutiansut @quantaxis
                        //
                        // CLAUDE.md 的 QIFI 样本里,两笔订单 `status` 同为 `FINISHED`,
                        // 区别只在 `last_msg`(「已撤单」/「全部成交」)与 `volume_left`。
                        // 而本库实测 **410,380 笔订单的 `last_msg` 全是空串** ——
                        // 其中 326,915 笔是撤单、82,781 笔是全成,
                        // 在 QIFI 结构里**无法区分**,只能靠 `volume_left` 倒推。
                        //
                        // 外部消费者(Mongo / 其它系统 / DIFF 客户端)拿到的就是没有原因的
                        // FINISHED。`diff_messages.rs:364-365` 把这个契约写进了文档和测试,
                        // 但那个测试是**手写 JSON 字面量再断言**,测的是 serde 往返,
                        // 不是系统行为 —— 于是契约「有测试」却从未被生产满足。
                        //
                        // `cancel_order` 返回的是 `&mut Order`,在这里填即可,不必改 qars。
                        cancelled_order.last_msg = "已撤单".to_string();
                        Some(cancelled_order.order_id.clone())
                    }
                    Err(_) => None,
                }
            };
            // 借用已结束，可以安全访问 acc.money
            let money_after = acc.money;

            if let Some(released_order_id) = cancel_success {
                log::debug!(
                    "✅ Frozen funds released for cancelled order: qa_order_id={}, account={}, released_order={}, money_before={}, money_after={}",
                    qa_order_id,
                    user_id,
                    released_order_id,
                    money_before,
                    money_after
                );
            } else {
                // 可能订单已经成交或已被取消，frozen 中不存在
                // 失败分支才需要打出 frozen 的键用于诊断 —— 同样在宏内求值。
                // 此处读的是「撤单尝试之后」的键集,但失败恰恰意味着这个 order_id
                // 本就不在 frozen 里、cancel_order 也没能移除它,所以与尝试前等价。
                // @yutiansut @quantaxis
                log::warn!(
                    "⚠️ Failed to release frozen funds: qa_order_id={} NOT FOUND in frozen HashMap (keys: {:?}), account={}",
                    qa_order_id,
                    acc.frozen.keys().collect::<Vec<_>>(),
                    user_id
                );
            }
        } else {
            log::error!(
                "❌ Account not found when releasing frozen funds: user_id={}",
                user_id
            );
        }

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

        // Phase 14: 写入订单状态更新到 WAL @yutiansut @quantaxis
        // 撤单成功，状态=2 (CANCELLED)
        let direction_u8 = WalRecord::encode_direction(direction);
        let offset_u8 = WalRecord::encode_offset(offset);

        if let Err(e) = self.write_order_status_update(
            qa_order_id, // 使用 qars 内部订单ID
            user_id,
            instrument_id,
            2, // CANCELLED
            remaining_volume, // volume_orign (已撤单量)
            0.0,              // volume_left (全部撤销)
            0.0,              // volume_filled (撤单不涉及成交)
            0.0,              // frozen_margin (已释放)
            0.0,              // frozen_amount (已释放)
            direction_u8,
            offset_u8,
            price,
            0.0,              // avg_price (撤单不涉及成交)
            "用户撤单",       // last_msg
        ) {
            log::error!("Failed to write OrderStatusUpdate WAL for cancelled order: {}", e);
        }

        log::debug!(
            "Cancel accepted: exchange_order_id={}, instrument={}, user={}, order_id={}, qa_order_id={}",
            exchange_order_id,
            instrument_id,
            user_id,
            order_id,
            qa_order_id
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
        account_id: &str, // ✨ 实际是 account_id，不是 user_id @yutiansut @quantaxis
        instrument_id: &str,
        direction: &str,
        offset: &str,
        price: f64,
        volume: f64,
        qa_order_id: &str, // qars 内部订单ID（非交易所订单ID）
        exchange_trade_id: &str, // 交易所成交号(写进 QIFI trades;旧路径无成交号传 "")
    ) -> Result<(String, f64, f64), ExchangeError> {
        log::debug!("🔧 update_account called: account={}, instrument={}, {}  {}, price={}, volume={}, qa_order_id={}",
            account_id, instrument_id, direction, offset, price, volume, qa_order_id);

        // 合约所属交易所的兜底值:锁外取(只读合约配置);优先用委托上已回填的值(见下方)
        let exchange_id_cfg = self.exchange_of(instrument_id, "");

        // ✨ 修复：使用 get_account 而非 get_default_account，因为传入的是 account_id
        let account = self.account_mgr.get_account(account_id)?;
        let mut acc = account.write();

        // 检查成交前的持仓（详细）
        if let Some(pos) = acc.get_position(instrument_id) {
            log::debug!(
                "🔧   BEFORE receive_deal_sim: {} position details:",
                account_id
            );
            log::debug!("🔧     volume_short_today={}, volume_short_his={}, volume_short_frozen_today={}, volume_short_frozen_his={}",
                pos.volume_short_today, pos.volume_short_his, pos.volume_short_frozen_today, pos.volume_short_frozen_his);
            log::debug!("🔧     volume_short_unmut()={}", pos.volume_short_unmut());
        } else {
            log::debug!(
                "🔧   BEFORE receive_deal_sim: {} no position for {}",
                account_id,
                instrument_id
            );
        }

        // 生成时间戳字符串 —— 必须是**北京墙钟**:qars `parse_dt_ns` 按 UTC+8 解析再减 8h
        // (qars2 account.rs:455)。原来给 UTC 墙钟 ⇒ QIFI 成交 trade_date_time 恒早 8 小时。@yutiansut @quantaxis
        let datetime = crate::core::account_ext::beijing_now_str();

        // 计算 towards (遵循 qars 的定义)
        let towards = match (direction, offset) {
            ("BUY", "OPEN") => 2,    // 买开 = 2 (qars 标准)
            ("SELL", "OPEN") => -2,  // 卖开 = -2
            ("BUY", "CLOSE") => 3,   // 买平 (平空) = 3
            ("SELL", "CLOSE") => -3, // 卖平 (平多) = -3 ✅
            ("BUY", "CLOSETODAY") => 4,
            ("SELL", "CLOSETODAY") => -4,
            // 平昨与下单侧 `calculate_towards` 同口径(order_router.rs:2545-2546)。缺这两支时,
            // CLOSEYESTERDAY 单在撮合簿里已成交、这里却报错返回 —— 账户不入账,同批其余成交也被跳过。@yutiansut @quantaxis
            ("BUY", "CLOSEYESTERDAY") => 3,
            ("SELL", "CLOSEYESTERDAY") => -3,
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
        // ⚠️ 委托不在账户当日委托里就不能交给 qars:`receive_deal_sim` 内部
        // `dailyorders.get_mut(..).unwrap()`(qars2 account.rs:1681),panic=abort 下整个进程退出。
        // 正常路径不会走到这里 —— 日终先撤光活单再结算(`SettlementEngine::run_end_of_day`);
        // 这是纵深防御:一笔对不上的成交只记错误、不入账,不能拖垮整个交易所。@yutiansut @quantaxis
        if !acc.dailyorders.contains_key(qa_order_id) {
            log::error!(
                "成交对应的委托不在账户当日委托里(已日终结算?),不入账: account={}, qa_order_id={}, {} {} {}@{}",
                account_id, qa_order_id, instrument_id, direction, volume, price
            );
            return Err(ExchangeError::OrderError(format!(
                "Order {} not in account {} daily orders",
                qa_order_id, account_id
            )));
        }
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

        // ✨ QIFI 补全(qars sim 分支成交的 exchange_id / exchange_trade_id、持仓的 exchange_id 恒为空串)
        //    + 持仓均价按成本校正(见 account_ext「QIFI 字段校正」)@yutiansut @quantaxis
        //    交易所:继承所属委托(下单时按合约注册表回填,order_router `send_order` 之后),
        //    旧快照恢复的活单没有这个值时退回合约配置。
        let exchange_id = acc
            .dailyorders
            .get(qa_order_id)
            .map(|o| o.exchange_id.clone())
            .filter(|e| !e.is_empty())
            .unwrap_or(exchange_id_cfg);
        if let Some(t) = acc.dailytrades.get_mut(&trade_id) {
            if t.exchange_id.is_empty() {
                t.exchange_id = exchange_id.clone();
            }
            if t.exchange_trade_id.is_empty() {
                t.exchange_trade_id = exchange_trade_id.to_string();
            }
        }
        if let Some(pos) = acc.get_position(instrument_id) {
            if pos.exchange_id.is_empty() {
                pos.exchange_id = exchange_id;
            }
            crate::core::account_ext::conform_position_price(pos);
        }

        // 检查成交后的持仓
        let pos_after = acc
            .get_position(instrument_id)
            .map(|p| (p.volume_long_unmut(), p.volume_short_unmut()));
        log::debug!(
            "🔧   AFTER receive_deal_sim: {} position={:?}",
            account_id,
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

            // ✨ 这里**不能**再调 order.trade() @yutiansut @quantaxis
            //
            // `receive_deal_sim`(上方几行)内部已经调过了 ——
            // qars2/src/qaaccount/account.rs:1656-1657:
            //     let order = self.dailyorders.get_mut(&order_id).unwrap();
            //     order.trade(amount);
            //
            // 在这里再调一次 → volume_left 被扣两次:
            //     receive_deal_sim  volume_left: 2 → 0
            //     这里              volume_left: 0 → -2
            //
            // 后果:`volume_left` 变成 `-volume_orign`,而恢复路径按
            //     filled_volume = volume_orign - volume_left
            // 计算(order_router.rs:2359),得到 2 - (-2) = 4 ——
            // **已成交量恰好翻倍**,前端「当前委托」里就显示成
            // 「委托 3 手 / 成交 6 手」。
            //
            // 实测(生产 19,785 笔订单):成交量 > 委托量 的 880 笔,
            // 全部是重启恢复的历史单,且成交量精确等于委托量 ×2;
            // 抽查某账户 QIFI:3849 笔里 3847 笔 `volume_left == -volume_orign`。
            //
            // 原注释「qars 的 trade() 方法会…」说明当时并不知道
            // receive_deal_sim 已经做过这件事 —— 同一件事写了两遍,
            // 而这次两遍分别在两个代码库里(参见 tasks/lessons.md L13)。
            //
            // 状态与 volume_left 直接读 receive_deal_sim 处理后的结果。

            log::debug!(
                "🔧   AFTER order.trade(): order_id={}, volume_left={}, status={}",
                qa_order_id,
                order.volume_left,
                order.status
            );

            // ✨ 成交侧也要填 `last_msg` —— 撤单侧已在 handle_cancel_accepted_new 填过。
            // @yutiansut @quantaxis
            //
            // QIFI 里「已撤单」和「全部成交」的 `status` **同为 `FINISHED`**,
            // 区分信息只在 `last_msg`(见 CLAUDE.md 的 QIFI 样本)。
            // 实测本库 410,380 笔订单 `last_msg` 全为空串,其中
            // 326,915 笔撤单 / 82,781 笔全成 **在 QIFI 结构里无法区分**。
            //
            // 取值与 `diff_messages.rs:363-365` 记的契约一致:
            //   全部成交 → "全部成交"(volume_left == 0)
            //   部分成交 → "部分成交"(volume_left > 0 且已成交过)
            // 注意**不要**覆盖已有的非空 last_msg(例如撤单已写过「已撤单」)。
            if let Some(m) = fill_last_msg(&order.last_msg, order.volume_left) {
                order.last_msg = m;
            }

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
            account_id, direction, offset, instrument_id, price, volume, qa_order_id, trade_id, acc.money, status, volume_left, volume_orign
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 14: 订单状态 WAL 持久化
    // @yutiansut @quantaxis
    // ═══════════════════════════════════════════════════════════════════════════

    /// 写入订单状态更新到 WAL
    ///
    /// 用于记录订单生命周期中的状态变化，恢复时重建 dailyorders 和 frozen
    ///
    /// # 参数
    /// - `order_id`: qars 内部订单ID
    /// - `user_id`: 账户ID
    /// - `instrument_id`: 合约ID（格式：EXCHANGE.SYMBOL）
    /// - `status`: 订单状态 (0=ALIVE, 1=FINISHED, 2=CANCELLED, 3=REJECTED, 4=PARTIALLY_FILLED)
    /// - `volume_orign`: 原始委托量
    /// - `volume_left`: 剩余未成交量
    /// - `volume_filled`: 已成交量
    /// - `frozen_margin`: 当前冻结保证金
    /// - `frozen_amount`: 当前冻结资金
    /// - `direction`: 方向 (0=BUY, 1=SELL)
    /// - `offset`: 开平 (0=OPEN, 1=CLOSE, 2=CLOSETODAY)
    /// - `limit_price`: 委托价格
    /// - `avg_price`: 成交均价
    /// - `last_msg`: 最后消息（撤单原因等）
    pub fn write_order_status_update(
        &self,
        order_id: &str,
        user_id: &str,
        instrument_id: &str,
        status: u8,
        volume_orign: f64,
        volume_left: f64,
        volume_filled: f64,
        frozen_margin: f64,
        frozen_amount: f64,
        direction: u8,
        offset: u8,
        limit_price: f64,
        avg_price: f64,
        last_msg: &str,
    ) -> Result<(), ExchangeError> {
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let record = WalRecord::OrderStatusUpdate {
            order_id: WalRecord::to_fixed_array_64(order_id),
            user_id: WalRecord::to_fixed_array_40(user_id),
            instrument_id: WalRecord::to_fixed_array_16(instrument_id),
            status,
            volume_orign,
            volume_left,
            volume_filled,
            frozen_margin,
            frozen_amount,
            direction,
            offset,
            limit_price,
            avg_price,
            last_msg: WalRecord::to_fixed_array_128(last_msg),
            timestamp,
        };

        // 获取或创建 account WAL manager
        let account_wal_mgr = self.get_or_create_account_wal(user_id)?;

        // 持久化 WAL record
        account_wal_mgr.append(record).map_err(|e| {
            ExchangeError::StorageError(format!("Failed to append OrderStatusUpdate: {}", e))
        })?;

        log::debug!(
            "📝 OrderStatusUpdate WAL: order={}, status={}, volume_left={}, frozen={}",
            order_id,
            status,
            volume_left,
            frozen_margin
        );

        Ok(())
    }

    /// 写入持仓快照到 WAL
    ///
    /// 用于定期保存持仓状态，恢复时重建 hold HashMap
    ///
    /// @yutiansut @quantaxis
    #[allow(clippy::too_many_arguments)]
    pub fn write_position_snapshot(
        &self,
        user_id: &str,
        instrument_id: &str,
        exchange_id: &str,
        volume_long_today: f64,
        volume_long_his: f64,
        volume_long_frozen_today: f64,
        volume_long_frozen_his: f64,
        open_price_long: f64,
        open_cost_long: f64,
        position_price_long: f64,
        position_cost_long: f64,
        margin_long: f64,
        volume_short_today: f64,
        volume_short_his: f64,
        volume_short_frozen_today: f64,
        volume_short_frozen_his: f64,
        open_price_short: f64,
        open_cost_short: f64,
        position_price_short: f64,
        position_cost_short: f64,
        margin_short: f64,
        float_profit_long: f64,
        float_profit_short: f64,
        position_profit_long: f64,
        position_profit_short: f64,
        last_price: f64,
    ) -> Result<(), ExchangeError> {
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let record = WalRecord::PositionSnapshot {
            user_id: WalRecord::to_fixed_array_40(user_id),
            instrument_id: WalRecord::to_fixed_array_16(instrument_id),
            exchange_id: WalRecord::to_fixed_array_16(exchange_id),
            volume_long_today,
            volume_long_his,
            volume_long_frozen_today,
            volume_long_frozen_his,
            open_price_long,
            open_cost_long,
            position_price_long,
            position_cost_long,
            margin_long,
            volume_short_today,
            volume_short_his,
            volume_short_frozen_today,
            volume_short_frozen_his,
            open_price_short,
            open_cost_short,
            position_price_short,
            position_cost_short,
            margin_short,
            float_profit_long,
            float_profit_short,
            position_profit_long,
            position_profit_short,
            last_price,
            timestamp,
        };

        // 获取或创建 account WAL manager
        let account_wal_mgr = self.get_or_create_account_wal(user_id)?;

        // 持久化 WAL record
        account_wal_mgr.append(record).map_err(|e| {
            ExchangeError::StorageError(format!("Failed to append PositionSnapshot: {}", e))
        })?;

        log::debug!(
            "📝 PositionSnapshot WAL: user={}, instrument={}, long={}/{}, short={}/{}",
            user_id,
            instrument_id,
            volume_long_today,
            volume_long_his,
            volume_short_today,
            volume_short_his
        );

        Ok(())
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
    /// ✨ 注意：参数实际是 account_id（由订单的 user_id 字段传入，但其值为 account_id）
    /// @yutiansut @quantaxis
    fn push_account_update(&self, account_id: &str) -> Result<(), ExchangeError> {
        // ✨ 修复：使用 get_account 而非 get_default_account，因为传入的是 account_id
        let account = self.account_mgr.get_account(account_id)?;
        // ✨ 使用 write() 以便调用 get_margin() 动态计算 @yutiansut @quantaxis
        let mut acc = account.write();

        // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
        let position_margin = acc.get_margin();
        let frozen_margin = acc.get_frozen_margin();
        let margin = position_margin + frozen_margin;

        let notification = AccountUpdateNotification {
            user_id: account_id.to_string(), // ✨ 使用 account_id @yutiansut @quantaxis
            // ⚠️ `acc.accounts.<派生字段>` 盘中**永远是开户时的初始值** ——
            // qars 只在 `settle()` 里整体重建 self.accounts(account.rs:574),
            // 成交/撤单都不回写。实测 NOISE_SELL 成交 6.8 万笔后
            // accounts.balance 仍是 1,500,000,000(init_cash)、risk_ratio 仍是 0.0。
            // 本处已持写锁(为 get_margin 取的),改用动态 getter 零额外成本。
            // @yutiansut @quantaxis
            balance: acc.get_balance(),
            available: acc.money,
            margin,  // ✨ 修复: 使用动态计算的 margin
            position_profit: acc.get_positionprofit(),
            risk_ratio: acc.safe_risk_ratio(),
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        self.send_notification(Notification::AccountUpdate(notification))?;

        // DIFF 协议：推送账户更新 patch（如果设置了 SnapshotManager）
        if let Some(snapshot_mgr) = self.snapshot_mgr.get() {
            // ✨ 同时推 positions —— 此前只推 accounts,持仓明细一条都不发。
            //    @yutiansut @quantaxis
            //
            // 实测(WS DIFF,有持仓的账户,25 条 rtn_data):patch 顶层键是
            //   ["accounts","factors","ins_list","klines","notify","orders","quotes","trades"]
            // —— 8 个段都在,**唯独没有 positions**。前端持仓页因此只能靠 HTTP 轮询,
            // 成交后持仓变化不会实时推过去。
            // 本处已持账户写锁(为上面 get_margin 取的),顺带产出持仓是零额外成本。
            let positions_patch: serde_json::Map<String, serde_json::Value> = acc
                .hold
                .iter()
                .map(|(code, pos)| {
                    // ✨ 必须用 `volume_*_unmut()`,不能用 `today + his`。@yutiansut @quantaxis
                    //
                    // qars 的真实不变量是
                    // `volume_long = today + his + frozen_today + frozen_his`(position.rs:403),
                    // `today + his` **漏掉被挂单冻结的部分**。
                    //
                    // 而 HTTP 侧 `handlers.rs:997` 用的是 `pos.volume_long()`(含冻结)。
                    // 于是同一个持仓:**页面加载显示 285,下一次 WS 推送到达就变成 216**
                    // —— 数字自己会跳,用户无从判断哪个是真的。
                    // 线上实测 `ACC_55361002` 的 IF2502:HTTP 285 / 本处 216(frozen 69)。
                    let vl = pos.volume_long_unmut();
                    let vs = pos.volume_short_unmut();
                    (
                        code.clone(),
                        serde_json::json!({
                            "instrument_id": code,
                            "volume_long": vl,
                            "volume_short": vs,
                            "open_price_long": pos.open_price_long,
                            "open_price_short": pos.open_price_short,
                            // ⚠️ 不用 `float_profit_long()` —— 它是 `&mut self`
                            //    (内部会重算并缓存),这里只有 `&` 借用。
                            //    用公开字段按同一公式直接算(position.rs:326-336)。
                            "float_profit": (pos.lastest_price * vl * pos.preset.unit_table as f64
                                             - pos.open_cost_long)
                                          + (pos.open_cost_short
                                             - pos.lastest_price * vs * pos.preset.unit_table as f64),
                            "margin": pos.margin_long + pos.margin_short,
                            // ✨ 补推冻结量 @yutiansut @quantaxis
                            // 前端持仓表有「可平量 = 持仓 − 冻结」列;挂平仓单只动冻结、
                            // 不动 volume_long/short,不推的话该列会停在旧值直到成交。
                            "volume_long_frozen": pos.volume_long_frozen_today
                                                + pos.volume_long_frozen_his,
                            "volume_short_frozen": pos.volume_short_frozen_today
                                                 + pos.volume_short_frozen_his,
                        }),
                    )
                })
                .collect();

            let patch = serde_json::json!({
                "accounts": {
                    account_id: {  // ✨ 使用 account_id @yutiansut @quantaxis
                        // 同上:盘中不可读 acc.accounts 的派生字段 @yutiansut @quantaxis
                        "balance": acc.get_balance(),
                        "available": acc.money,
                        "margin": margin,  // ✨ 修复: 使用动态计算的 margin
                        "position_profit": acc.get_positionprofit(),
                        "risk_ratio": acc.safe_risk_ratio(),
                    }
                },
                "positions": positions_patch
            });

            if self.legacy_snapshot_active(account_id) {
                snapshot_mgr.push_patch_now(account_id, patch); // 同步,保 FIFO
            }

            // ✨ DIFF 标准结构:`trade.<account>.accounts.CNY` / `positions.<EXCHANGE.INSTRUMENT>`
            //    = QIFI Account / Position(与快照、HTTP QIFI 同一套结构)。仍在同一把账户写锁内。
            //    @yutiansut @quantaxis
            let views = self.account_views(&mut acc);
            drop(acc);
            for (section, key, val) in views {
                self.publish_trade_section(account_id, section, &key, val);
            }
        }

        Ok(())
    }

    /// 推送订单状态通知并同步 DIFF 快照
    fn emit_order_status(&self, status: OrderStatusNotification) -> Result<(), ExchangeError> {
        self.send_notification(Notification::OrderStatus(status.clone()))?;

        if let Some(snapshot_mgr) = self.snapshot_mgr.get() {
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

            // 同步写入:保证同账户补丁 FIFO(原先逐条 tokio::spawn 可乱序)@yutiansut @quantaxis
            if self.legacy_snapshot_active(&status.user_id) {
                snapshot_mgr.push_patch_now(&status.user_id, patch);
            }
        }

        Ok(())
    }

    /// 推发送成交通知并同步 DIFF 快照
    fn emit_trade_notification(&self, trade: TradeNotification) -> Result<(), ExchangeError> {
        self.send_notification(Notification::Trade(trade.clone()))?;

        if let Some(snapshot_mgr) = self.snapshot_mgr.get() {
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

            if self.legacy_snapshot_active(&trade.user_id) {
                snapshot_mgr.push_patch_now(&trade.user_id, patch); // 同步,保 FIFO
            }
        }

        Ok(())
    }

    /// 发送通知
    fn send_notification(&self, notification: Notification) -> Result<(), ExchangeError> {
        // 发送到全局通道
        // ⚠️ 不要用 `?` 传播 —— 这条是诊断旁路,满了必须继续走下面 4 条真实投递路径。
        //    若在此返回 Err,会把「内存泄漏」变成「通知全断」的功能故障。
        if self.trade_sender.try_send(notification.clone()).is_err() {
            NOTIFICATION_TAP_DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        // 发送到用户特定的订阅者
        let user_id = match &notification {
            Notification::Trade(t) => &t.user_id,
            Notification::AccountUpdate(a) => &a.user_id,
            Notification::OrderStatus(o) => &o.user_id,
        };

        if let Some(subs) = self.subscribers.get(user_id) {
            // ✨ 投递的同时剪掉已断连的订阅者 @yutiansut @quantaxis
            //
            // TradeGateway **没有 unsubscribe**(全仓无此方法),WS 会话在
            // `websocket/mod.rs:126` 只订阅、断连时不清理。Receiver 被 drop 后
            // Sender 变成死引用永远留在这个 Vec 里,而原写法 `let _ = send(..)`
            // 把错误吞掉 —— 于是每条通知都要为每个死订阅者白 clone 一次,
            // 连接抖动越多、开销越大。
            //
            // `send()` 只在接收端全部 drop 时返回 Err,所以 retain 恰好剪掉死的。
            // 锁序未变(仍是 DashMap 分片锁 → 内层 RwLock),不引入新的环。
            // 无界 channel 的 send 不阻塞,写锁持有时间极短。
            subs.write()
                .retain(|sender| sender.send(notification.clone()).is_ok());
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

    // ❌ 已删除 `test_handle_accepted` —— 它是个**空壳**:测试体被整段注释掉,
    //    只剩一个 `create_test_gateway()`,却标着 `#[ignore] // TODO: Phase 3 重构后更新`。
    //    去掉 ignore 一跑就「通过」—— **通过但零覆盖,比失败更坏**。
    //
    //    而它注释里想做的事(subscribe → handle_accepted → 校验通知),
    //    `test_handle_accepted_notification`(本文件下方)**早就完整做了**,
    //    且断言到 `order_id` / `status` 字段。所以这是遗留重复,不是覆盖缺口。
    //    @yutiansut @quantaxis

    #[tokio::test]
    async fn test_diff_snapshot_manager_integration() {
        use crate::protocol::diff::snapshot::SnapshotManager;

        let (mut gateway, _, account_id) = create_test_gateway();

        // 创建并设置 SnapshotManager
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        gateway.set_snapshot_manager(snapshot_mgr.clone());

        // 验证 SnapshotManager 已设置
        assert!(gateway.snapshot_manager().is_some());

        // 初始化用户快照（使用 account_id 作为 DIFF snapshot key）
        snapshot_mgr.initialize_user(&account_id).await;

        // 启动 peek 任务
        let peek_task = tokio::spawn({
            let snapshot_mgr = snapshot_mgr.clone();
            let account_id = account_id.clone();
            async move { snapshot_mgr.peek(&account_id).await }
        });

        // 等待一小段时间确保 peek 任务开始等待
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 推送账户更新（这会触发 DIFF patch）
        // 注意：push_account_update 需要 account_id（ACC_xxx 格式）
        gateway.push_account_update(&account_id).unwrap();

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

        // 初始化用户快照（使用 account_id 作为 DIFF snapshot key）
        snapshot_mgr.initialize_user(&account_id).await;

        // 启动 peek 任务
        let peek_task = tokio::spawn({
            let snapshot_mgr = snapshot_mgr.clone();
            let account_id = account_id.clone();
            async move { snapshot_mgr.peek(&account_id).await }
        });

        // 等待 peek 开始
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 推送多个账户更新（模拟多次成交）
        // 注意：push_account_update 需要 account_id（ACC_xxx 格式）
        gateway.push_account_update(&account_id).unwrap();

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
        let (gateway, account_mgr, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let exchange_order_id = 1i64;
        let order_id = "O001";
        let volume = 10.0;
        let price = 50000.0;

        // 先在账户中创建订单（关键步骤！）
        let qa_order_id = {
            let account = account_mgr.get_account(&account_id).unwrap();
            let mut acc = account.write();
            let order = acc.buy_open(instrument_id, volume, "2025-12-17 16:53:36", price).unwrap();
            order.order_id.clone()
        };

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
                Some("counter_party"),
                &qa_order_id,       // 使用真实的 qa_order_id
                Some("O_counter"),  // opposite_order_id_str
                true,               // is_taker
            )
            .unwrap();

        // 验证 trade_id 是递增的
        assert_eq!(trade_id, 1);

        // 为第二次成交创建新订单
        let qa_order_id_2 = {
            let account = account_mgr.get_account(&account_id).unwrap();
            let mut acc = account.write();
            let order = acc.buy_open(instrument_id, 5.0, "2025-12-17 16:53:37", 50100.0).unwrap();
            order.order_id.clone()
        };

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
                Some("counter_party"),
                &qa_order_id_2,     // 使用新的 qa_order_id
                Some("O_counter2"), // opposite_order_id_str
                true,               // is_taker
            )
            .unwrap();

        assert_eq!(trade_id_2, 2);
        assert!(trade_id_2 > trade_id);
    }

    #[test]
    fn test_handle_cancel_accepted_new() {
        let (gateway, account_mgr, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";
        let exchange_order_id = 1i64;
        let order_id = "O001";

        // 先在账户中创建订单（关键步骤！）
        let qa_order_id = {
            let account = account_mgr.get_account(&account_id).unwrap();
            let mut acc = account.write();
            let order = acc.buy_open(instrument_id, 10.0, "2025-12-17 16:53:36", 50000.0).unwrap();
            order.order_id.clone()
        };

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
            5.0, // remaining_volume
            &qa_order_id, // 使用真实的 qa_order_id
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
        let (gateway, account_mgr, account_id) = create_test_gateway();

        let instrument_id = "SHFE.cu2501";

        // 先在账户中创建订单（关键步骤！）
        let qa_order_id = {
            let account = account_mgr.get_account(&account_id).unwrap();
            let mut acc = account.write();
            let order = acc.buy_open(instrument_id, 10.0, "2025-12-17 16:53:36", 50000.0).unwrap();
            order.order_id.clone()
        };

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
                Some("counter_party"),
                &qa_order_id,       // 使用真实的 qa_order_id
                Some("O_counter"),  // opposite_order_id_str
                true,               // is_taker
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

    // ==================== TradeNotification 测试 @yutiansut @quantaxis ====================

    /// 测试 TradeNotification 创建
    #[test]
    fn test_trade_notification_creation() {
        let notification = TradeNotification {
            trade_id: "T12345".to_string(),
            user_id: "user1".to_string(),
            order_id: "O12345".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 1702800000000,
            commission: 25.5,
        };

        assert_eq!(notification.trade_id, "T12345");
        assert_eq!(notification.user_id, "user1");
        assert_eq!(notification.order_id, "O12345");
        assert_eq!(notification.instrument_id, "cu2501");
        assert_eq!(notification.direction, "BUY");
        assert_eq!(notification.offset, "OPEN");
        assert_eq!(notification.price, 85000.0);
        assert_eq!(notification.volume, 10.0);
        assert_eq!(notification.timestamp, 1702800000000);
        assert_eq!(notification.commission, 25.5);
    }

    /// 测试 TradeNotification Clone
    #[test]
    fn test_trade_notification_clone() {
        let notification = TradeNotification {
            trade_id: "T12345".to_string(),
            user_id: "user1".to_string(),
            order_id: "O12345".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(),
            price: 86000.0,
            volume: 5.0,
            timestamp: 1702800000000,
            commission: 12.9,
        };

        let cloned = notification.clone();
        assert_eq!(notification.trade_id, cloned.trade_id);
        assert_eq!(notification.user_id, cloned.user_id);
        assert_eq!(notification.price, cloned.price);
    }

    // ==================== AccountUpdateNotification 测试 @yutiansut @quantaxis ====================

    /// 测试 AccountUpdateNotification 创建
    #[test]
    fn test_account_update_notification_creation() {
        let notification = AccountUpdateNotification {
            user_id: "user1".to_string(),
            balance: 1000000.0,
            available: 950000.0,
            margin: 50000.0,
            position_profit: 1500.0,
            risk_ratio: 0.05,
            timestamp: 1702800000000,
        };

        assert_eq!(notification.user_id, "user1");
        assert_eq!(notification.balance, 1000000.0);
        assert_eq!(notification.available, 950000.0);
        assert_eq!(notification.margin, 50000.0);
        assert_eq!(notification.position_profit, 1500.0);
        assert_eq!(notification.risk_ratio, 0.05);
        assert_eq!(notification.timestamp, 1702800000000);
    }

    /// 测试 AccountUpdateNotification Clone
    #[test]
    fn test_account_update_notification_clone() {
        let notification = AccountUpdateNotification {
            user_id: "user1".to_string(),
            balance: 500000.0,
            available: 450000.0,
            margin: 50000.0,
            position_profit: -500.0,
            risk_ratio: 0.1,
            timestamp: 1702800000000,
        };

        let cloned = notification.clone();
        assert_eq!(notification.user_id, cloned.user_id);
        assert_eq!(notification.balance, cloned.balance);
        assert_eq!(notification.margin, cloned.margin);
    }

    // ==================== OrderStatusNotification 测试 @yutiansut @quantaxis ====================

    /// 测试 OrderStatusNotification 创建
    #[test]
    fn test_order_status_notification_creation() {
        let notification = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO12345".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 10.0,
            price: 85000.0,
            status: "ACCEPTED".to_string(),
            timestamp: 1702800000000,
            order_id: "O12345".to_string(),
            user_id: "user1".to_string(),
            reason: None,
        };

        assert_eq!(notification.exchange_id, "SHFE");
        assert_eq!(notification.instrument_id, "cu2501");
        assert_eq!(notification.exchange_order_id, "EXO12345");
        assert_eq!(notification.direction, "BUY");
        assert_eq!(notification.offset, "OPEN");
        assert_eq!(notification.price_type, "LIMIT");
        assert_eq!(notification.volume, 10.0);
        assert_eq!(notification.price, 85000.0);
        assert_eq!(notification.status, "ACCEPTED");
        assert_eq!(notification.order_id, "O12345");
        assert_eq!(notification.user_id, "user1");
        assert!(notification.reason.is_none());
    }

    /// 测试 OrderStatusNotification 带原因
    #[test]
    fn test_order_status_notification_with_reason() {
        let notification = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO12345".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 0.0,
            price: 85000.0,
            status: "REJECTED".to_string(),
            timestamp: 1702800000000,
            order_id: "O12345".to_string(),
            user_id: "user1".to_string(),
            reason: Some("Insufficient margin".to_string()),
        };

        assert_eq!(notification.status, "REJECTED");
        assert_eq!(notification.reason, Some("Insufficient margin".to_string()));
    }

    /// 测试 OrderStatusNotification Clone
    #[test]
    fn test_order_status_notification_clone() {
        let notification = OrderStatusNotification {
            exchange_id: "CFFEX".to_string(),
            instrument_id: "IF2512".to_string(),
            exchange_order_id: "EXO67890".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(),
            price_type: "MARKET".to_string(),
            volume: 5.0,
            price: 0.0, // 市价单
            status: "FILLED".to_string(),
            timestamp: 1702800000000,
            order_id: "O67890".to_string(),
            user_id: "user2".to_string(),
            reason: None,
        };

        let cloned = notification.clone();
        assert_eq!(notification.exchange_id, cloned.exchange_id);
        assert_eq!(notification.instrument_id, cloned.instrument_id);
        assert_eq!(notification.order_id, cloned.order_id);
    }

    // ==================== Notification 枚举测试 @yutiansut @quantaxis ====================

    /// 测试 Notification::Trade 变体
    #[test]
    fn test_notification_trade_variant() {
        let trade = TradeNotification {
            trade_id: "T001".to_string(),
            user_id: "user1".to_string(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 1702800000000,
            commission: 25.5,
        };

        let notification = Notification::Trade(trade.clone());

        if let Notification::Trade(t) = notification {
            assert_eq!(t.trade_id, "T001");
        } else {
            panic!("Expected Trade variant");
        }
    }

    /// 测试 Notification::AccountUpdate 变体
    #[test]
    fn test_notification_account_update_variant() {
        let update = AccountUpdateNotification {
            user_id: "user1".to_string(),
            balance: 1000000.0,
            available: 950000.0,
            margin: 50000.0,
            position_profit: 1500.0,
            risk_ratio: 0.05,
            timestamp: 1702800000000,
        };

        let notification = Notification::AccountUpdate(update.clone());

        if let Notification::AccountUpdate(a) = notification {
            assert_eq!(a.user_id, "user1");
            assert_eq!(a.balance, 1000000.0);
        } else {
            panic!("Expected AccountUpdate variant");
        }
    }

    /// 测试 Notification::OrderStatus 变体
    #[test]
    fn test_notification_order_status_variant() {
        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 10.0,
            price: 85000.0,
            status: "ACCEPTED".to_string(),
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: "user1".to_string(),
            reason: None,
        };

        let notification = Notification::OrderStatus(status.clone());

        if let Notification::OrderStatus(o) = notification {
            assert_eq!(o.order_id, "O001");
            assert_eq!(o.status, "ACCEPTED");
        } else {
            panic!("Expected OrderStatus variant");
        }
    }

    // ==================== TradeGateway 构建器测试 @yutiansut @quantaxis ====================

    /// 测试 TradeGateway::new()
    #[test]
    fn test_trade_gateway_new() {
        let account_mgr = Arc::new(AccountManager::new());
        let gateway = TradeGateway::new(account_mgr);

        // 验证初始状态
        assert!(gateway.notification_broker.is_none());
        assert!(gateway.snapshot_mgr.get().is_none());
        assert!(gateway.trade_recorder.is_none());
        assert!(gateway.market_data_service.get().is_none());
        assert_eq!(gateway.wal_root, "./data/wal");
    }

    /// 测试 with_wal_root
    #[test]
    fn test_with_wal_root() {
        let account_mgr = Arc::new(AccountManager::new());
        let gateway = TradeGateway::new(account_mgr)
            .with_wal_root("/custom/wal/path");

        assert_eq!(gateway.wal_root, "/custom/wal/path");
    }

    /// 测试 with_notification_broker
    #[test]
    fn test_with_notification_broker() {
        let account_mgr = Arc::new(AccountManager::new());
        let broker = Arc::new(NotificationBroker::new());
        let gateway = TradeGateway::new(account_mgr)
            .with_notification_broker(broker);

        assert!(gateway.notification_broker.is_some());
    }

    /// 测试 set_notification_broker
    #[test]
    fn test_set_notification_broker() {
        let account_mgr = Arc::new(AccountManager::new());
        let mut gateway = TradeGateway::new(account_mgr);

        assert!(gateway.notification_broker.is_none());

        let broker = Arc::new(NotificationBroker::new());
        gateway.set_notification_broker(broker);

        assert!(gateway.notification_broker.is_some());
    }

    // ==================== 订阅测试 @yutiansut @quantaxis ====================

    /// 测试 subscribe_global
    #[test]
    fn test_subscribe_global() {
        let (gateway, _, account_id) = create_test_gateway();

        let receiver = gateway.subscribe_global();

        // 发送通知
        let notification = Notification::AccountUpdate(AccountUpdateNotification {
            user_id: account_id.clone(),
            balance: 1000000.0,
            available: 950000.0,
            margin: 50000.0,
            position_profit: 0.0,
            risk_ratio: 0.05,
            timestamp: Utc::now().timestamp_nanos_opt().unwrap_or(0),
        });

        gateway.send_notification(notification).unwrap();

        // 全局订阅者应收到通知
        let received = receiver.try_recv().unwrap();
        if let Notification::AccountUpdate(a) = received {
            assert_eq!(a.user_id, account_id);
        } else {
            panic!("Expected AccountUpdate notification");
        }
    }

    /// 测试 get_receiver
    #[test]
    fn test_get_receiver() {
        let (gateway, _, account_id) = create_test_gateway();

        let receiver = gateway.get_receiver();

        // 发送通知
        let notification = Notification::Trade(TradeNotification {
            trade_id: "T001".to_string(),
            user_id: account_id.clone(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 0,
            commission: 25.5,
        });

        gateway.send_notification(notification).unwrap();

        // 主接收器应收到通知
        let received = receiver.try_recv().unwrap();
        if let Notification::Trade(t) = received {
            assert_eq!(t.trade_id, "T001");
        } else {
            panic!("Expected Trade notification");
        }
    }

    /// 测试多用户订阅
    #[test]
    fn test_multiple_user_subscriptions() {
        let account_mgr = Arc::new(AccountManager::new());

        // 创建两个账户
        let req1 = OpenAccountRequest {
            user_id: "user1".to_string(),
            account_id: Some("user1".to_string()),
            account_name: "User 1".to_string(),
            init_cash: 500000.0,
            account_type: AccountType::Individual,
        };
        let account_id_1 = account_mgr.open_account(req1).unwrap();

        let req2 = OpenAccountRequest {
            user_id: "user2".to_string(),
            account_id: Some("user2".to_string()),
            account_name: "User 2".to_string(),
            init_cash: 500000.0,
            account_type: AccountType::Individual,
        };
        let account_id_2 = account_mgr.open_account(req2).unwrap();

        let gateway = TradeGateway::new(account_mgr.clone())
            .with_wal_root("./output/testexchange/wal_multi");

        // 各用户订阅
        let receiver1 = gateway.subscribe_user(account_id_1.clone());
        let receiver2 = gateway.subscribe_user(account_id_2.clone());

        // 向 user1 发送通知
        let notification1 = Notification::Trade(TradeNotification {
            trade_id: "T001".to_string(),
            user_id: account_id_1.clone(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 0,
            commission: 25.5,
        });
        gateway.send_notification(notification1).unwrap();

        // user1 应收到通知
        assert!(receiver1.try_recv().is_ok());
        // user2 不应收到 user1 的通知
        assert!(receiver2.try_recv().is_err());
    }

    // ==================== 成交ID生成测试 @yutiansut @quantaxis ====================

    /// 测试成交ID唯一性
    #[test]
    fn test_generate_trade_id_unique() {
        let (gateway, _, _) = create_test_gateway();

        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let id = gateway.generate_trade_id();
            assert!(ids.insert(id.clone()), "Duplicate trade ID generated: {}", id);
        }
    }

    /// 测试成交ID格式
    #[test]
    fn test_generate_trade_id_format() {
        let (gateway, _, _) = create_test_gateway();

        for _ in 0..10 {
            let id = gateway.generate_trade_id();
            assert!(id.starts_with('T'), "Trade ID should start with 'T': {}", id);
            assert!(id.len() > 10, "Trade ID should be longer than 10 chars: {}", id);
        }
    }

    // ==================== handle_cancelled 测试 @yutiansut @quantaxis ====================

    /// 测试 handle_cancelled
    #[test]
    fn test_handle_cancelled() {
        let (gateway, _, account_id) = create_test_gateway();

        let receiver = gateway.subscribe_user(account_id.clone());

        let result = gateway.handle_cancelled(
            "O001",
            &account_id,
            "SHFE",
            "cu2501",
            "EXO001",
            "BUY",
            "OPEN",
            "LIMIT",
            85000.0,
            5.0, // 剩余量
        );

        assert!(result.is_ok());

        // 验证通知已发送
        let received = receiver.try_recv().unwrap();
        if let Notification::OrderStatus(o) = received {
            assert_eq!(o.order_id, "O001");
            assert_eq!(o.status, "CANCELLED");
            assert_eq!(o.volume, 5.0);
        } else {
            panic!("Expected OrderStatus notification");
        }
    }

    // ==================== handle_accepted 测试 @yutiansut @quantaxis ====================

    /// 测试 handle_accepted
    #[test]
    fn test_handle_accepted_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let receiver = gateway.subscribe_user(account_id.clone());

        let result = gateway.handle_accepted(
            "O001",
            &account_id,
            "SHFE",
            "cu2501",
            "EXO001",
            "BUY",
            "OPEN",
            "LIMIT",
            85000.0,
            10.0,
        );

        assert!(result.is_ok());

        // 验证通知已发送
        let received = receiver.try_recv().unwrap();
        if let Notification::OrderStatus(o) = received {
            assert_eq!(o.order_id, "O001");
            assert_eq!(o.status, "ACCEPTED");
            assert_eq!(o.volume, 10.0);
            assert_eq!(o.price, 85000.0);
        } else {
            panic!("Expected OrderStatus notification");
        }
    }

    // ==================== 通知发送测试 @yutiansut @quantaxis ====================

    /// 测试 send_notification 发送到多个渠道
    #[test]
    fn test_send_notification_multiple_channels() {
        let (gateway, _, account_id) = create_test_gateway();

        let user_receiver = gateway.subscribe_user(account_id.clone());
        let global_receiver = gateway.subscribe_global();
        let main_receiver = gateway.get_receiver();

        let notification = Notification::Trade(TradeNotification {
            trade_id: "T001".to_string(),
            user_id: account_id.clone(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 0,
            commission: 25.5,
        });

        gateway.send_notification(notification).unwrap();

        // 所有渠道都应收到通知
        assert!(user_receiver.try_recv().is_ok(), "User receiver should receive");
        assert!(global_receiver.try_recv().is_ok(), "Global receiver should receive");
        assert!(main_receiver.try_recv().is_ok(), "Main receiver should receive");
    }

    // ==================== 边界条件测试 @yutiansut @quantaxis ====================

    /// 测试零价格成交通知结构
    #[test]
    fn test_trade_notification_zero_price() {
        let notification = TradeNotification {
            trade_id: "T001".to_string(),
            user_id: "user001".to_string(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 0.0, // 零价格（市价单可能发生）
            commission: 0.0,
            timestamp: 1702800000000,
        };

        assert_eq!(notification.price, 0.0);
        assert_eq!(notification.volume, 10.0);
    }

    /// 测试零数量成交通知结构
    #[test]
    fn test_trade_notification_zero_volume() {
        let notification = TradeNotification {
            trade_id: "T001".to_string(),
            user_id: "user001".to_string(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 0.0, // 零数量
            price: 85000.0,
            commission: 0.0,
            timestamp: 1702800000000,
        };

        assert_eq!(notification.volume, 0.0);
        assert_eq!(notification.price, 85000.0);
    }

    /// 测试大数量成交通知结构
    #[test]
    fn test_trade_notification_large_volume() {
        let notification = TradeNotification {
            trade_id: "T001".to_string(),
            user_id: "user001".to_string(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 1000000.0, // 大数量
            price: 85000.0,
            commission: 1000.0,
            timestamp: 1702800000000,
        };

        assert_eq!(notification.volume, 1000000.0);
        assert!(notification.volume > 0.0);
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发发送通知
    #[test]
    fn test_concurrent_send_notifications() {
        use std::thread;

        let (gateway, _, account_id) = create_test_gateway();
        let gateway = Arc::new(gateway);

        let receiver = gateway.subscribe_global();
        let mut handles = vec![];

        for i in 0..10 {
            let gateway_clone = gateway.clone();
            let account_id_clone = account_id.clone();

            handles.push(thread::spawn(move || {
                let notification = Notification::Trade(TradeNotification {
                    trade_id: format!("T{:03}", i),
                    user_id: account_id_clone,
                    order_id: format!("O{:03}", i),
                    instrument_id: "cu2501".to_string(),
                    direction: "BUY".to_string(),
                    offset: "OPEN".to_string(),
                    price: 85000.0 + i as f64 * 10.0,
                    volume: 1.0 + i as f64,
                    timestamp: 0,
                    commission: 0.1,
                });
                gateway_clone.send_notification(notification)
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }

        // 验证收到了通知
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 10, "Should receive all 10 notifications");
    }

    /// 测试并发订阅
    #[test]
    fn test_concurrent_subscriptions() {
        use std::thread;

        let account_mgr = Arc::new(AccountManager::new());

        // 创建多个账户
        for i in 0..5 {
            let req = OpenAccountRequest {
                user_id: format!("user{}", i),
                account_id: Some(format!("user{}", i)),
                account_name: format!("User {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            account_mgr.open_account(req).unwrap();
        }

        let gateway = Arc::new(
            TradeGateway::new(account_mgr.clone())
                .with_wal_root("./output/testexchange/wal_concurrent")
        );

        let mut handles = vec![];

        for i in 0..5 {
            let gateway_clone = gateway.clone();
            let user_id = format!("user{}", i);

            handles.push(thread::spawn(move || {
                let _receiver = gateway_clone.subscribe_user(user_id.clone());
                // 验证订阅成功（不会 panic）
                true
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            assert!(handle.join().unwrap());
        }
    }

    // ==================== convert_to_new_notification 测试 @yutiansut @quantaxis ====================

    /// 测试 Trade 通知转换
    #[test]
    fn test_convert_trade_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let trade = TradeNotification {
            trade_id: "T001".to_string(),
            user_id: account_id.clone(),
            order_id: "O001".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price: 85000.0,
            volume: 10.0,
            timestamp: 1702800000000,
            commission: 25.5,
        };

        let old_notification = Notification::Trade(trade);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::TradeExecuted);
    }

    /// 测试 AccountUpdate 通知转换
    #[test]
    fn test_convert_account_update_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let update = AccountUpdateNotification {
            user_id: account_id.clone(),
            balance: 1000000.0,
            available: 950000.0,
            margin: 50000.0,
            position_profit: 1500.0,
            risk_ratio: 0.05,
            timestamp: 1702800000000,
        };

        let old_notification = Notification::AccountUpdate(update);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::AccountUpdate);
    }

    /// 测试 OrderStatus ACCEPTED 通知转换
    #[test]
    fn test_convert_order_accepted_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 10.0,
            price: 85000.0,
            status: "ACCEPTED".to_string(),
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: account_id.clone(),
            reason: None,
        };

        let old_notification = Notification::OrderStatus(status);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::OrderAccepted);
    }

    /// 测试 OrderStatus FILLED 通知转换
    #[test]
    fn test_convert_order_filled_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 10.0,
            price: 85000.0,
            status: "FILLED".to_string(),
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: account_id.clone(),
            reason: None,
        };

        let old_notification = Notification::OrderStatus(status);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::OrderFilled);
    }

    /// 测试 OrderStatus CANCELLED 通知转换
    #[test]
    fn test_convert_order_cancelled_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 5.0,
            price: 85000.0,
            status: "CANCELLED".to_string(),
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: account_id.clone(),
            reason: Some("User cancelled".to_string()),
        };

        let old_notification = Notification::OrderStatus(status);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::OrderCanceled);
    }

    /// 测试 OrderStatus REJECTED 通知转换
    #[test]
    fn test_convert_order_rejected_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 0.0,
            price: 85000.0,
            status: "REJECTED".to_string(),
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: account_id.clone(),
            reason: Some("Insufficient margin".to_string()),
        };

        let old_notification = Notification::OrderStatus(status);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_some());
        let new_notif = new_notification.unwrap();
        assert_eq!(new_notif.message_type, NotificationType::OrderRejected);
    }

    /// 测试未知状态通知转换（返回 None）
    #[test]
    fn test_convert_unknown_status_notification() {
        let (gateway, _, account_id) = create_test_gateway();

        let status = OrderStatusNotification {
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2501".to_string(),
            exchange_order_id: "EXO001".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            price_type: "LIMIT".to_string(),
            volume: 10.0,
            price: 85000.0,
            status: "UNKNOWN_STATUS".to_string(), // 未知状态
            timestamp: 1702800000000,
            order_id: "O001".to_string(),
            user_id: account_id.clone(),
            reason: None,
        };

        let old_notification = Notification::OrderStatus(status);
        let new_notification = gateway.convert_to_new_notification(&old_notification);

        assert!(new_notification.is_none());
    }
}

#[cfg(test)]
mod last_msg_tests {
    use super::fill_last_msg;

    /// QIFI 契约:`status=FINISHED` 时,「已撤单」与「全部成交」只能靠 `last_msg` 区分。
    /// 实测本库 410,380 笔订单 `last_msg` **全为空串**
    /// (326,915 笔撤单 / 82,781 笔全成,在 QIFI 结构里无法区分)。@yutiansut @quantaxis
    #[test]
    fn full_fill_and_partial_fill() {
        assert_eq!(fill_last_msg("", 0.0).as_deref(), Some("全部成交"));
        assert_eq!(fill_last_msg("", -0.0).as_deref(), Some("全部成交"));
        assert_eq!(fill_last_msg("", 3.0).as_deref(), Some("部分成交"));
    }

    /// **不覆盖已有值** —— 撤单路径先写了「已撤单」,后到的成交回报不能把它抹掉。
    /// 这是本函数存在的主要理由,不是顺带的边界。
    #[test]
    fn never_overwrites_existing() {
        assert_eq!(fill_last_msg("已撤单", 5.0), None);
        assert_eq!(fill_last_msg("已撤单", 0.0), None, "即便看起来是全成也不覆盖");
        assert_eq!(fill_last_msg("部分成交", 2.0), None);
    }

    /// **负对照**:空串必须被当成「未写过」,否则这个函数永远返回 None,
    /// 上面两个测试也能过 —— 而那正是修复前的现状(全库 last_msg 为空)。
    #[test]
    fn empty_is_treated_as_unset() {
        assert!(fill_last_msg("", 1.0).is_some(), "空串必须触发写入");
        assert!(fill_last_msg(" ", 1.0).is_none(), "非空(哪怕是空格)一律不覆盖");
    }
}
