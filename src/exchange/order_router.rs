//! 订单路由模块
//!
//! 负责订单的接收、风控检查、路由到撮合引擎以及撤单处理

use crate::core::{Order, QAOrder, QAOrderExt};
use crate::exchange::{AccountManager, InstrumentRegistry, TradeGateway};
use crate::market::MarketDataBroadcaster;
use crate::matching::engine::{ExchangeMatchingEngine, InstrumentAsset};
use crate::matching::{orders, Failed, OrderDirection, OrderType, Success};
use crate::risk::pre_trade_check::{OrderCheckRequest, PreTradeCheck, RiskCheckResult};
use crate::ExchangeError;
use chrono::Local;
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 时间条件枚举
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum TimeCondition {
    /// Immediate or Cancel - 立即成交剩余撤销
    IOC,
    /// Good for Session - 本节有效
    GFS,
    /// Good for Day - 当日有效 (默认)
    #[default]
    GFD,
    /// Good Till Date - 指定日期前有效
    GTD,
    /// Good Till Cancel - 撤销前有效
    GTC,
    /// Good for Auction - 集合竞价有效
    GFA,
}


impl std::fmt::Display for TimeCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeCondition::IOC => write!(f, "IOC"),
            TimeCondition::GFS => write!(f, "GFS"),
            TimeCondition::GFD => write!(f, "GFD"),
            TimeCondition::GTD => write!(f, "GTD"),
            TimeCondition::GTC => write!(f, "GTC"),
            TimeCondition::GFA => write!(f, "GFA"),
        }
    }
}

impl TimeCondition {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "IOC" => TimeCondition::IOC,
            "GFS" => TimeCondition::GFS,
            "GFD" => TimeCondition::GFD,
            "GTD" => TimeCondition::GTD,
            "GTC" => TimeCondition::GTC,
            "GFA" => TimeCondition::GFA,
            _ => TimeCondition::GFD, // 默认当日有效
        }
    }
}

/// 数量条件枚举
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum VolumeCondition {
    /// Any - 任何数量 (默认)
    #[default]
    ANY,
    /// Min - 最小数量
    MIN,
    /// All - 全部数量 (FOK)
    ALL,
}


impl std::fmt::Display for VolumeCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VolumeCondition::ANY => write!(f, "ANY"),
            VolumeCondition::MIN => write!(f, "MIN"),
            VolumeCondition::ALL => write!(f, "ALL"),
        }
    }
}

impl VolumeCondition {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "ANY" => VolumeCondition::ANY,
            "MIN" => VolumeCondition::MIN,
            "ALL" => VolumeCondition::ALL,
            _ => VolumeCondition::ANY, // 默认任意数量
        }
    }
}

/// 订单提交请求（交易层 - 只关心账户）
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub account_id: String, // 交易系统只关心账户ID
    pub instrument_id: String,
    pub direction: String, // BUY/SELL
    pub offset: String,    // OPEN/CLOSE/CLOSETODAY
    pub volume: f64,
    pub price: f64,
    pub order_type: String, // LIMIT/MARKET
    /// 时间条件: IOC/GFS/GFD/GTD/GTC/GFA
    #[serde(default)]
    pub time_condition: Option<TimeCondition>,
    /// 数量条件: ANY/MIN/ALL (ALL + IOC = FOK)
    #[serde(default)]
    pub volume_condition: Option<VolumeCondition>,
}

/// 撤单请求（交易层 - 只关心账户）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub account_id: String, // 交易系统只关心账户ID
    pub order_id: String,
}

/// 订单提交响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub success: bool,
    pub order_id: Option<String>,
    pub status: Option<String>, // 订单最终状态：submitted/filled/partially_filled/rejected
    pub error_message: Option<String>,
    pub error_code: Option<u32>,
}

/// 提交行为控制选项
#[derive(Clone, Debug)]
#[derive(Default)]
struct OrderSubmitOptions {
    /// 是否为强制（风险绕过）订单
    force: bool,
    /// 客户端编号(DIFF insert_order 的 order_id)。账户内唯一,重复 → CTP 22。@yutiansut @quantaxis
    client_order_id: Option<String>,
}

/// CTP 错误码(取自 LocalCTP `ctp_file/current/error.xml`)的提示文本 @yutiansut @quantaxis
pub fn ctp_error_prompt(error_id: i32) -> &'static str {
    match error_id {
        15 => "CTP:报单字段有误",
        16 => "CTP:找不到合约",
        22 => "CTP:报单错误：不允许重复报单",
        24 => "CTP:撤单已报送，不允许重复撤单",
        25 => "CTP:撤单找不到相应报单",
        26 => "CTP:报单已全成交或已撤销，不能再撤",
        30 => "CTP:平仓量超过持仓量",
        31 => "CTP:资金不足",
        50 => "CTP:平今仓位不足",
        51 => "CTP:平昨仓位不足",
        62 => "CTP:没有条件单交易权限",
        80 => "CTP:交易所不支持的价格类型",
        95 => "CTP：不支持的价格",
        116 => "CTP:下单频率限制",
        _ => "CTP：交易所返回的错误",
    }
}

/// 交易所内部拒单码 → CTP ErrorID(对照稿 v0,已与 hft_maker 确认)@yutiansut @quantaxis
pub fn ctp_error_id(code: u32, msg: &str) -> i32 {
    match code {
        22 => 22,
        15 => 15,
        1001 | 1004 => 31,                         // 资金不足 / 风险度过高
        1007 | 4006 => 16,                         // 合约不存在
        1003 | 1008 | 4000 | 4002 | 4003 | 4004 | 4007 => 15, // 报单字段有误
        4001 => {
            if msg.contains("Insufficient funds") || msg.contains("资金") {
                31
            } else if msg.contains("position") || msg.contains("持仓") || msg.contains("平仓") {
                30
            } else {
                31
            }
        }
        _ => 91, // 1002 持仓比例 / 1005 自成交 / 4005 数量条件 / 4100 4101 交易状态 等
    }
}


/// 订单状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// 等待风控
    PendingRisk,
    /// 风控通过，等待路由
    PendingRoute,
    /// 已提交到撮合引擎
    Submitted,
    /// 部分成交
    PartiallyFilled,
    /// 全部成交
    Filled,
    /// 已撤单
    Cancelled,
    /// 被拒绝
    Rejected,
}

/// 订单路由信息
/// @yutiansut @quantaxis
#[derive(Debug, Clone)]
struct OrderRouteInfo {
    order: Order,
    status: OrderStatus,
    submit_time: i64,
    update_time: i64,
    filled_volume: f64,                    // 已成交数量
    qa_order_id: String,                   // qars 内部订单ID (用于 receive_deal_sim)
    matching_engine_order_id: Option<u64>, // 撮合引擎订单ID (用于撤单)
    time_condition: TimeCondition,         // 时间条件 (IOC/GFD/GTC等)
    volume_condition: VolumeCondition,     // 数量条件 (ANY/MIN/ALL)
}

/// 订单统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatistics {
    pub total_count: usize,
    pub pending_count: usize,
    pub filled_count: usize,
    pub cancelled_count: usize,
    pub rejected_count: usize,
}

/// 成交统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeStatistics {
    pub total_count: u64,
    pub total_volume: f64,
    pub total_amount: f64,
}

/// 订单路由器
pub struct OrderRouter {
    /// 账户管理器
    account_mgr: Arc<AccountManager>,

    /// 风控检查器
    risk_checker: Arc<PreTradeCheck>,

    /// 撮合引擎
    matching_engine: Arc<ExchangeMatchingEngine>,

    /// 合约注册表
    instrument_registry: Arc<InstrumentRegistry>,

    /// 成交回报网关
    trade_gateway: Arc<TradeGateway>,

    /// 市场数据广播器（可选）
    market_broadcaster: Option<Arc<MarketDataBroadcaster>>,

    /// 存储管理器（可选，用于持久化行情数据）
    storage: Option<Arc<crate::storage::hybrid::OltpHybridStorage>>,

    /// 订单映射 (order_id -> OrderRouteInfo)
    orders: DashMap<String, Arc<RwLock<OrderRouteInfo>>>,

    /// 用户订单索引 (user_id -> Vec<order_id>)
    user_orders: DashMap<String, Arc<RwLock<Vec<String>>>>,

    /// ✨ 撮合引擎订单ID反向索引 ((instrument_id, matching_engine_order_id) -> order_id)
    /// @yutiansut @quantaxis
    ///
    /// ⚠️ 键**必须**带 instrument_id：qars2 的 `seq` 是每个 Orderbook 各一份的字段
    /// (`qars2/src/qamarket/matchengine/orderbook.rs:132,161`)，都从 `MIN_SEQUENCE_ID = 1`
    /// 起算。IF2501 的 engine_id 42 和 IH2501 的 engine_id 42 是两个毫不相干的订单。
    /// 原先用 `DashMap<u64, String>` 全局索引 → 后写者覆盖先写者，成交时把对手方
    /// 解析成另一个合约上、另一个账户的订单。
    engine_id_to_order: DashMap<(String, u64), String>,

    /// ✨ 撮合引擎订单ID → user_id 直接映射 (性能优化) @yutiansut @quantaxis
    /// 避免成交时两次查找（engine_id→order_id→user_id），直接 O(1) 获取
    engine_id_to_user: DashMap<(String, u64), String>,

    /// 订单序号生成器
    /// (账户, 客户端编号) → 交易所单号。客户端编号查重(CTP 22)与按客户端编号撤单用。
    /// 值为空串 = 已占用但订单在生成单号前就被拒了。@yutiansut @quantaxis
    client_orders: DashMap<(String, String), String>,

    /// 客户端编号映射文件路径(启动时由 main 设置一次)
    client_order_id_file: std::sync::OnceLock<String>,

    /// 客户端编号映射文件的常驻追加句柄(首次写入时打开)@yutiansut @quantaxis
    client_order_id_writer: parking_lot::Mutex<Option<std::fs::File>>,

    order_seq: AtomicU64,

    /// 统计：总成交笔数
    trade_count: AtomicU64,

    /// 统计：总成交量
    trade_volume: parking_lot::RwLock<f64>,

    /// 统计：总成交金额
    trade_amount: parking_lot::RwLock<f64>,

    // ========== 性能优化字段 ==========
    /// 快照频率控制：记录每个合约的上次快照时间
    last_snapshot_time: Arc<DashMap<String, Instant>>,

    /// 快照写入间隔（默认1秒）
    snapshot_interval: Duration,

    /// Tick数据批量缓冲区
    tick_buffer: Arc<Mutex<Vec<crate::storage::wal::record::WalRecord>>>,

    /// 批量写入线程停止信号
    flush_stop_signal: Arc<AtomicBool>,

    /// 优先级订单队列（可选）
    priority_queue: Option<Arc<crate::exchange::PriorityOrderQueue>>,

    /// 是否启用优先级队列
    priority_queue_enabled: AtomicBool,

    /// 交易状态机（可选） @yutiansut @quantaxis
    trading_state_machine: Option<Arc<crate::exchange::TradingStateMachine>>,

    /// 日终闸门:置位期间新单一律拒(4100 → CTP 91,与交易所闭市拒单同口径),撤单照常。
    /// @yutiansut @quantaxis
    ///
    /// 日终结算前必须先把活单撤干净:qars `settle()` 清空当日委托,撮合簿里若还留着
    /// 该账户的单,之后撤单(qars `cancel_order` 的 expect)或成交(`receive_deal_sim`
    /// 的 unwrap)都会 panic —— release 是 panic=abort,整个进程退出。
    eod_halt: AtomicBool,
    /// 已过闸门检查、尚未返回的下单数。置闸后等它归零,保证扫单时没有在途报单。
    inflight_submits: AtomicUsize,
}

/// 日终扫单结果(`cancel_all_active_orders`)@yutiansut @quantaxis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EodSweep {
    /// 走正常撤单路径撤掉的活单(释放冻结 + 推 DIFF「已撤单」)
    pub cancelled: usize,
    /// 撤单失败的活单(扫描后已自行终结等),见日志
    pub cancel_failed: usize,
    /// 撮合簿里有、路由侧已不是活单(或无记录)的残单,直接从簿上摘掉
    pub residual_removed: usize,
    /// 扫完后撮合簿里仍在的挂单数 —— 不为 0 就不能结算
    pub residual_left: usize,
}

/// `get_user_order_details` 的返回项。@yutiansut @quantaxis
///
/// ⚠️ **`qa_order_id` 是本结构存在的理由** —— 它是 qars 内部订单 ID,
/// 也是账户 QIFI `orders` map 的 key。同一笔订单在两处有**两个不同的 ID**:
///   OrderRouter: `O1788625578538000000001`
///   QIFI:        `abff8eb6-0649-5b52-b236-f4...`
/// `handlers.rs::query_user_orders` 把两边合并展示,只按 `order_id` 去重时
/// **同一笔单会显示成两条**(实测:下 1 单 → 页面 2 行,时间戳只差 14 微秒、
/// 合约/量/价全同,仅 ID 与 status 大小写不同)。带上 qa_order_id 才去得掉。
///
/// 原先这里返回 6 元组,加到 7 就没法读了 —— 改具名结构体。
#[derive(Debug, Clone)]
pub struct UserOrderDetail {
    pub order_id: String,
    /// qars 内部订单 ID = QIFI `orders` 的 key,跨源去重要用它
    pub qa_order_id: String,
    pub order: Order,
    pub status: OrderStatus,
    pub submit_time: i64,
    pub update_time: i64,
    pub filled_volume: f64,
}

impl OrderRouter {
    pub fn new(
        account_mgr: Arc<AccountManager>,
        matching_engine: Arc<ExchangeMatchingEngine>,
        instrument_registry: Arc<InstrumentRegistry>,
        trade_gateway: Arc<TradeGateway>,
    ) -> Self {
        let risk_checker = Arc::new(PreTradeCheck::new(account_mgr.clone()));

        Self {
            account_mgr,
            risk_checker,
            matching_engine,
            instrument_registry,
            trade_gateway,
            market_broadcaster: None,
            storage: None,
            orders: DashMap::new(),
            user_orders: DashMap::new(),
            engine_id_to_order: DashMap::new(),
            engine_id_to_user: DashMap::new(),
            client_orders: DashMap::new(),
            client_order_id_file: std::sync::OnceLock::new(),
            client_order_id_writer: parking_lot::Mutex::new(None),
            order_seq: AtomicU64::new(1),
            trade_count: AtomicU64::new(0),
            trade_volume: parking_lot::RwLock::new(0.0),
            trade_amount: parking_lot::RwLock::new(0.0),
            // 性能优化字段初始化
            last_snapshot_time: Arc::new(DashMap::new()),
            snapshot_interval: Duration::from_secs(1), // 默认1秒
            tick_buffer: Arc::new(Mutex::new(Vec::with_capacity(1000))),
            flush_stop_signal: Arc::new(AtomicBool::new(false)),
            priority_queue: None, // 默认不启用
            priority_queue_enabled: AtomicBool::new(false),
            trading_state_machine: None, // 默认不启用
            eod_halt: AtomicBool::new(false),
            inflight_submits: AtomicUsize::new(0),
        }
    }

    /// 设置市场数据广播器
    pub fn set_market_broadcaster(&mut self, broadcaster: Arc<MarketDataBroadcaster>) {
        self.market_broadcaster = Some(broadcaster);
    }

    /// 设置存储管理器（用于持久化行情数据）
    pub fn set_storage(&mut self, storage: Arc<crate::storage::hybrid::OltpHybridStorage>) {
        self.storage = Some(storage);
    }

    /// 设置交易状态机 @yutiansut @quantaxis
    pub fn set_trading_state_machine(
        &mut self,
        state_machine: Arc<crate::exchange::TradingStateMachine>,
    ) {
        self.trading_state_machine = Some(state_machine);
    }

    /// 获取交易状态机
    pub fn get_trading_state_machine(&self) -> Option<Arc<crate::exchange::TradingStateMachine>> {
        self.trading_state_machine.clone()
    }

    /// 启用优先级队列
    ///
    /// # 参数
    /// - `low_queue_limit`: 低优先级队列最大长度（默认100）
    /// - `critical_amount_threshold`: 大额订单阈值（默认1,000,000.0）
    pub fn enable_priority_queue(
        &mut self,
        low_queue_limit: usize,
        critical_amount_threshold: f64,
    ) {
        let queue = Arc::new(crate::exchange::PriorityOrderQueue::new(
            low_queue_limit,
            critical_amount_threshold,
        ));
        self.priority_queue = Some(queue);
        self.priority_queue_enabled.store(true, Ordering::SeqCst);
        log::info!(
            "✅ Priority queue enabled (low_limit={}, threshold={:.2})",
            low_queue_limit,
            critical_amount_threshold
        );
    }

    /// 禁用优先级队列
    pub fn disable_priority_queue(&mut self) {
        self.priority_queue_enabled.store(false, Ordering::SeqCst);
        log::info!("⚠️  Priority queue disabled");
    }

    /// 添加VIP用户到优先级队列
    pub fn add_vip_user(&self, user_id: String) {
        if let Some(ref queue) = self.priority_queue {
            queue.add_vip_user(user_id);
        }
    }

    /// 批量添加VIP用户
    pub fn add_vip_users(&self, users: Vec<String>) {
        if let Some(ref queue) = self.priority_queue {
            queue.add_vip_users(users);
        }
    }

    /// 创建带自定义风控检查器的路由器
    pub fn with_risk_checker(
        account_mgr: Arc<AccountManager>,
        risk_checker: Arc<PreTradeCheck>,
        matching_engine: Arc<ExchangeMatchingEngine>,
        instrument_registry: Arc<InstrumentRegistry>,
        trade_gateway: Arc<TradeGateway>,
    ) -> Self {
        Self {
            account_mgr,
            risk_checker,
            matching_engine,
            instrument_registry,
            trade_gateway,
            market_broadcaster: None,
            storage: None,
            orders: DashMap::new(),
            user_orders: DashMap::new(),
            engine_id_to_order: DashMap::new(),
            engine_id_to_user: DashMap::new(),
            client_orders: DashMap::new(),
            client_order_id_file: std::sync::OnceLock::new(),
            client_order_id_writer: parking_lot::Mutex::new(None),
            order_seq: AtomicU64::new(1),
            trade_count: AtomicU64::new(0),
            trade_volume: parking_lot::RwLock::new(0.0),
            trade_amount: parking_lot::RwLock::new(0.0),
            // 性能优化字段初始化
            last_snapshot_time: Arc::new(DashMap::new()),
            snapshot_interval: Duration::from_secs(1), // 默认1秒
            tick_buffer: Arc::new(Mutex::new(Vec::with_capacity(1000))),
            flush_stop_signal: Arc::new(AtomicBool::new(false)),
            priority_queue: None, // 默认不启用
            priority_queue_enabled: AtomicBool::new(false),
            trading_state_machine: None, // 默认不启用
            eod_halt: AtomicBool::new(false),
            inflight_submits: AtomicUsize::new(0),
        }
    }

    /// 提交订单 (核心方法)
    pub fn submit_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
        self.submit_and_publish(req, OrderSubmitOptions::default())
    }

    /// 提交强制订单（跳过风控/资金校验，用于强平等场景）
    pub fn submit_force_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
        self.submit_and_publish(req, OrderSubmitOptions { force: true, ..Default::default() })
    }

    /// 带客户端编号提交(DIFF insert_order)。编号账户内唯一,重复 → CTP 22;
    /// DIFF 回报 `trade.<account>.orders/trades` 以它为键。@yutiansut @quantaxis
    pub fn submit_order_with_client_id(
        &self,
        req: SubmitOrderRequest,
        client_order_id: Option<String>,
    ) -> SubmitOrderResponse {
        self.submit_and_publish(
            req,
            OrderSubmitOptions { client_order_id, ..Default::default() },
        )
    }

    /// 所有提交入口的公共外壳:提交完成后推一次订单最终视图(撮合途中的中间状态
    /// 由 `handle_success_result` 推)。重复推同一视图无害(Merge Patch 幂等)。
    fn submit_and_publish(
        &self,
        req: SubmitOrderRequest,
        opts: OrderSubmitOptions,
    ) -> SubmitOrderResponse {
        // 日终闸门:先登记在途、再看闸门(两边都 SeqCst,与 `begin_eod_halt` 的
        // 「先置闸、再等在途归零」配对)—— 任何一笔要么看到闸门被拒,要么被日终等到落定。
        // 拒单在一切副作用之前(客户端编号未占位、未冻结、未进撮合)。@yutiansut @quantaxis
        struct InflightGuard<'a>(&'a AtomicUsize);
        impl Drop for InflightGuard<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        self.inflight_submits.fetch_add(1, Ordering::SeqCst);
        let _inflight = InflightGuard(&self.inflight_submits);
        if self.eod_halt.load(Ordering::SeqCst) {
            return SubmitOrderResponse {
                success: false,
                order_id: None,
                status: Some("rejected".to_string()),
                error_message: Some("日终结算中,暂停接收报单".to_string()),
                error_code: Some(4100), // 交易状态拒绝 → CTP 91(交易所闭市拒单同口径)
            };
        }

        let resp = self.submit_order_with_options(req, opts);
        if let Some(ref oid) = resp.order_id {
            if self.orders.contains_key(oid) {
                let reason = if resp.success { None } else { resp.error_message.as_deref() };
                self.push_diff_order(oid, reason);
            }
        }
        resp
    }

    /// 客户端编号映射的持久化文件(`<storage>/client_order_ids.jsonl`,每行
    /// `{"account","client_order_id","exchange_order_id"}`)。QIFI 订单没有客户端编号字段、
    /// qars 也不收外部单号,所以单独记一份;启动时 `load_client_order_ids` 读回。@yutiansut @quantaxis
    pub fn set_client_order_id_file(&self, path: String) {
        let _ = self.client_order_id_file.set(path);
    }

    fn persist_client_order_id(&self, account_id: &str, client_order_id: &str, order_id: &str) {
        let Some(path) = self.client_order_id_file.get() else { return };
        use std::io::Write;
        let line = serde_json::json!({
            "account": account_id,
            "client_order_id": client_order_id,
            "exchange_order_id": order_id,
        });
        // 追加写、不 fsync(与 WAL 的 async 模式同一取舍);只有带客户端编号的单才写。
        // ⚡ 整行先拼好再一次 write(2),句柄常驻 —— 原来每笔都 open + 用 `Value` 的 Display
        //    往无缓冲 File 里逐段写(十几~几十次 write(2))+ close,而且就在撮合之前,
        //    落在走 DIFF 客户端编号的 hft_maker 下单路径上。@yutiansut @quantaxis
        let mut text = line.to_string();
        text.push('\n');
        let mut w = self.client_order_id_writer.lock();
        if w.is_none() {
            match std::fs::OpenOptions::new().create(true).append(true).open(path) {
                Ok(f) => *w = Some(f),
                Err(e) => {
                    log::warn!("客户端编号映射写入失败 {}: {}", path, e);
                    return;
                }
            }
        }
        if let Some(f) = w.as_mut() {
            if let Err(e) = f.write_all(text.as_bytes()) {
                log::warn!("客户端编号映射写入失败 {}: {}", path, e);
                *w = None; // 下次重新打开
            }
        }
    }

    /// 日终闸门置位,并等在途报单落定;`timeout` 内没落定返回 false
    /// (调用方应放弃本轮日终并 `end_eod_halt`)。@yutiansut @quantaxis
    pub fn begin_eod_halt(&self, timeout: Duration) -> bool {
        self.eod_halt.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + timeout;
        while self.inflight_submits.load(Ordering::SeqCst) > 0 {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        true
    }

    /// 日终闸门复位(恢复收单)
    pub fn end_eod_halt(&self) {
        self.eod_halt.store(false, Ordering::SeqCst);
    }

    /// 日终闸门是否置位
    pub fn is_eod_halted(&self) -> bool {
        self.eod_halt.load(Ordering::SeqCst)
    }

    /// 日终撤单:撤掉全部活单(真实交易所收盘时 GFD 单失效)。@yutiansut @quantaxis
    ///
    /// 活单走与客户端撤单**同一条路径**(`cancel_order`),账户冻结、撮合簿、DIFF 回报
    /// 三处一致;随后逐簿复核,撮合簿里路由已不认得的残单直接从簿上摘掉(它们没有可释放的
    /// 账户冻结 —— 结算时 qars 会把残余冻结一并退回)。调用前应已 `begin_eod_halt`。
    pub fn cancel_all_active_orders(&self) -> EodSweep {
        let mut sweep = EodSweep::default();

        // 先收集再撤:不在 DashMap 迭代期间持有条目锁去走撤单路径
        let active: Vec<(String, String)> = self
            .orders
            .iter()
            .filter_map(|e| {
                let info = e.value().read();
                matches!(info.status, OrderStatus::Submitted | OrderStatus::PartiallyFilled)
                    .then(|| (e.key().clone(), info.order.user_id.clone()))
            })
            .collect();
        for (order_id, account_id) in active {
            match self.cancel_order(CancelOrderRequest { account_id, order_id: order_id.clone() }) {
                Ok(()) => sweep.cancelled += 1,
                Err(e) => {
                    sweep.cancel_failed += 1;
                    log::warn!("[EOD] 撤单失败 {}: {}", order_id, e);
                    // 撮合簿里已没有这张单(实测:启动恢复时恢复单彼此交叉成交,
                    // `restore_orders_from_accounts` 只认 Accepted —— 隔离实例 64 笔):
                    // 路由侧按已撤收尾,否则每天日终都再撤一遍、风控活单计数也一直挂着。
                    // qars 侧的委托与冻结由随后的结算清空、退回。@yutiansut @quantaxis
                    let still_active = self.orders.get(&order_id).is_some_and(|i| {
                        matches!(i.read().status, OrderStatus::Submitted | OrderStatus::PartiallyFilled)
                    });
                    if still_active {
                        let _ = self.update_order_status(&order_id, OrderStatus::Cancelled);
                    }
                }
            }
        }

        // 复核撮合簿。锁序:先读簿收集、放锁,再撤单(撤单路径自己按「簿 → 账户」取锁)
        for instrument in self.matching_engine.get_instruments() {
            let Some(book) = self.matching_engine.get_orderbook(&instrument) else { continue };
            let resting: Vec<(u64, bool)> = {
                let ob = book.read();
                let bids = ob.bid_queue.get_sorted_orders().unwrap_or_default();
                let asks = ob.ask_queue.get_sorted_orders().unwrap_or_default();
                bids.iter()
                    .map(|o| (o.id, true))
                    .chain(asks.iter().map(|o| (o.id, false)))
                    .collect()
            };
            for (engine_id, is_bid) in resting {
                let routed = self
                    .engine_id_to_order
                    .get(&(instrument.clone(), engine_id))
                    .map(|e| e.value().clone());
                if let Some(order_id) = routed {
                    let account_id = self
                        .orders
                        .get(&order_id)
                        .map(|i| i.read().order.user_id.clone())
                        .unwrap_or_default();
                    if self
                        .cancel_order(CancelOrderRequest { account_id, order_id })
                        .is_ok()
                    {
                        sweep.cancelled += 1;
                        continue;
                    }
                }
                let removed = {
                    let mut ob = book.write();
                    if is_bid {
                        ob.bid_queue.cancel(engine_id)
                    } else {
                        ob.ask_queue.cancel(engine_id)
                    }
                };
                if removed {
                    sweep.residual_removed += 1;
                    log::warn!(
                        "[EOD] 摘除撮合簿残单 {} engine_id={}(路由侧不是活单)",
                        instrument,
                        engine_id
                    );
                }
            }
            let left = {
                let ob = book.read();
                ob.bid_queue.get_sorted_orders().map_or(0, |v| v.len())
                    + ob.ask_queue.get_sorted_orders().map_or(0, |v| v.len())
            };
            sweep.residual_left += left;
        }
        sweep
    }

    /// 各合约撮合簿最新价(日终结算价用;没成交过的合约 lastprice 为 0,跳过)
    pub fn last_prices(&self) -> Vec<(String, f64)> {
        self.matching_engine
            .get_instruments()
            .into_iter()
            .filter_map(|i| {
                let px = self.matching_engine.get_last_price(&i)?;
                (px.is_finite() && px > 0.0).then_some((i, px))
            })
            .collect()
    }

    /// 新交易日:客户端编号(CTP 报单引用)按交易日唯一 —— 清空查重表,
    /// 映射文件改名归档为 `<文件名>.<交易日>.jsonl`,之后的映射写进新文件。
    /// 返回清掉的条目数。@yutiansut @quantaxis
    pub fn roll_client_order_ids(&self, trading_day: &str) -> usize {
        // 持写入锁做改名,与 `persist_client_order_id` 互斥
        let mut writer = self.client_order_id_writer.lock();
        let n = self.client_orders.len();
        self.client_orders.clear();
        *writer = None; // 下一笔重新打开(新文件)
        if let Some(path) = self.client_order_id_file.get() {
            let p = std::path::Path::new(path);
            if p.exists() {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("client_order_ids");
                let archived = p.with_file_name(format!("{}.{}.jsonl", stem, trading_day));
                if let Err(e) = std::fs::rename(p, &archived) {
                    log::warn!("[EOD] 客户端编号映射归档失败 {} → {:?}: {}", path, archived, e);
                }
            }
        }
        n
    }

    /// 启动时读回客户端编号映射(须在 `restore_orders_from_accounts` 之后,单号才对得上)
    pub fn load_client_order_ids(&self) -> usize {
        let Some(path) = self.client_order_id_file.get() else { return 0 };
        let Ok(text) = std::fs::read_to_string(path) else { return 0 };
        let mut n = 0;
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let (Some(a), Some(c), Some(o)) = (
                v["account"].as_str(),
                v["client_order_id"].as_str(),
                v["exchange_order_id"].as_str(),
            ) else {
                continue;
            };
            self.client_orders
                .insert((a.to_string(), c.to_string()), o.to_string());
            self.trade_gateway.register_client_order_id(o, c);
            n += 1;
        }
        n
    }

    /// 把客户端编号或交易所单号解析成交易所单号(撤单用)
    pub fn resolve_order_id(&self, account_id: &str, id: &str) -> String {
        if self.orders.contains_key(id) {
            return id.to_string();
        }
        self.client_orders
            .get(&(account_id.to_string(), id.to_string()))
            .map(|e| e.value().clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| id.to_string())
    }

    /// DIFF 订单视图 `trade.<account>.orders.<key>`:QIFI Order 字段 + `status_code`(EIFI §4.1)。
    /// @yutiansut @quantaxis
    ///
    /// 终态口径(与 hft_maker 确认):全部成交 0;撤单(含 FAK 零成 / 部成撤余量)5,
    /// 用 `volume_left` 区分;拒单 7 + `error_id`。`exchange_order_id` = 交易所单号
    /// (HTTP 撤单 / 查询用的那个)。
    fn push_diff_order(&self, order_id: &str, reason: Option<&str>) {
        if let Some((account_id, key, value)) = self.diff_order_view(order_id, reason) {
            self.trade_gateway
                .publish_trade_section(&account_id, "orders", &key, value);
        }
    }

    /// 交易网关(DIFF 登录初始快照等需要直接推账户视图)
    pub fn trade_gateway(&self) -> &Arc<TradeGateway> {
        &self.trade_gateway
    }

    /// 某账户的活单视图(登录初始快照用):PendingRisk / PendingRoute / Submitted / PartiallyFilled
    pub fn diff_live_order_views(&self, account_id: &str) -> Vec<(String, serde_json::Value)> {
        let ids: Vec<String> = self
            .user_orders
            .get(account_id)
            .map(|v| v.read().clone())
            .unwrap_or_default();
        ids.iter()
            .filter(|oid| {
                self.orders.get(*oid).map_or(false, |i| {
                    matches!(
                        i.read().status,
                        OrderStatus::PendingRisk
                            | OrderStatus::PendingRoute
                            | OrderStatus::Submitted
                            | OrderStatus::PartiallyFilled
                    )
                })
            })
            .filter_map(|oid| self.diff_order_view(oid, None).map(|(_, k, v)| (k, v)))
            .collect()
    }

    /// 订单视图本体:`(账户, 键, QIFI Order + status_code)`
    fn diff_order_view(
        &self,
        order_id: &str,
        reason: Option<&str>,
    ) -> Option<(String, String, serde_json::Value)> {
        let info_arc = self.orders.get(order_id).map(|e| e.value().clone())?;
        let info = info_arc.read();
        let o = &info.order;
        let account_id = o.user_id.clone();
        let key = self.trade_gateway.diff_order_key(order_id);
        let filled = info.filled_volume;
        let volume_left = (o.volume_orign - filled).max(0.0);
        let (status, status_code, last_msg) = match info.status {
            OrderStatus::Filled => ("FINISHED", 0, "全部成交".to_string()),
            OrderStatus::Cancelled => (
                "FINISHED",
                5,
                if filled > 0.0 { "部分成交，剩余已撤单" } else { "已撤单" }.to_string(),
            ),
            OrderStatus::Rejected => (
                "FINISHED",
                7,
                format!("{}: {}", ctp_error_prompt(91), reason.unwrap_or("交易所拒单")),
            ),
            OrderStatus::PartiallyFilled => ("ALIVE", 1, "部分成交".to_string()),
            _ if filled > 0.0 => ("ALIVE", 1, "部分成交".to_string()),
            _ => ("ALIVE", 3, "未成交".to_string()),
        };
        let mut value = serde_json::json!({
            "user_id": account_id,
            "order_id": key,
            "exchange_id": self.trade_gateway.exchange_of(&o.instrument_id, &o.exchange_id),
            "instrument_id": o.instrument_id,
            "direction": o.direction,
            "offset": o.offset,
            "volume_orign": o.volume_orign,
            "price_type": o.price_type,
            "limit_price": o.limit_price,
            "time_condition": info.time_condition.to_string(),
            "volume_condition": info.volume_condition.to_string(),
            "insert_date_time": info.submit_time,
            "exchange_order_id": order_id,
            "status": status,
            "volume_left": volume_left,
            "last_msg": last_msg,
            "status_code": status_code,
        });
        if status_code == 7 {
            value["error_id"] = serde_json::json!(91);
        }
        drop(info);
        Some((account_id, key, value))
    }

    /// 在进入订单表**之前**就被拒的单(柜台风控等,两段式的第一段)推一张终态订单视图:
    /// FINISHED + status_code 7 + error_id,`exchange_order_id` 为空。
    /// FOK 不能全部成交按「交易所撤单」报 status_code 4。由 DIFF 下单路径调用。@yutiansut @quantaxis
    pub fn publish_rejected_order(
        &self,
        req: &SubmitOrderRequest,
        client_order_id: Option<&str>,
        exchange_id: &str,
        resp: &SubmitOrderResponse,
    ) {
        if let Some(ref oid) = resp.order_id {
            if self.orders.contains_key(oid) {
                return; // 已进订单表,由 push_diff_order 负责
            }
        }
        // ⚠️ 重复客户端编号(22)**绝不能**推订单视图:键就是那个重复的编号,
        //    会把原来那张(可能还活着的)订单覆盖成 FINISHED/7。CTP 也只回
        //    OnRspOrderInsert 错误,原单不受影响 —— 这里只靠 notify 告知。@yutiansut @quantaxis
        if resp.error_code == Some(22) {
            return;
        }
        let key = client_order_id
            .map(str::to_string)
            .or_else(|| resp.order_id.clone())
            .unwrap_or_else(|| self.generate_order_id());
        let code = resp.error_code.unwrap_or(0);
        let msg = resp.error_message.clone().unwrap_or_else(|| "拒单".to_string());
        let (status_code, error_id, last_msg) = if code == 4010 {
            (4, 0, "FOK 未能全部成交，已撤单".to_string())
        } else {
            let eid = ctp_error_id(code, &msg);
            (7, eid, format!("{}: {}", ctp_error_prompt(eid), msg))
        };
        let mut value = serde_json::json!({
            "user_id": req.account_id,
            "order_id": key,
            "exchange_id": self.trade_gateway.exchange_of(&req.instrument_id, exchange_id),
            "instrument_id": req.instrument_id,
            "direction": req.direction,
            "offset": req.offset,
            "volume_orign": req.volume,
            "price_type": req.order_type,
            "limit_price": req.price,
            "time_condition": req.time_condition.map(|t| t.to_string()).unwrap_or_else(|| "GFD".into()),
            "volume_condition": req.volume_condition.map(|v| v.to_string()).unwrap_or_else(|| "ANY".into()),
            "insert_date_time": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            "exchange_order_id": "",
            "status": "FINISHED",
            "volume_left": req.volume,
            "last_msg": last_msg,
            "status_code": status_code,
        });
        if error_id != 0 {
            value["error_id"] = serde_json::json!(error_id);
        }
        self.trade_gateway
            .publish_trade_section(&req.account_id, "orders", &key, value);
    }

    fn submit_order_with_options(
        &self,
        mut req: SubmitOrderRequest,
        opts: OrderSubmitOptions,
    ) -> SubmitOrderResponse {
        // 客户端编号查重(CTP 在柜台层就拒,先于一切校验)—— entry 原子占位,
        // 并发同号也只有一个能进 @yutiansut @quantaxis
        if let Some(ref cid) = opts.client_order_id {
            let reject = |code: u32, msg: String| SubmitOrderResponse {
                success: false,
                order_id: None,
                status: Some("rejected".to_string()),
                error_message: Some(msg),
                error_code: Some(code),
            };
            if cid.trim().is_empty() {
                return reject(15, "order_id 为空".to_string());
            }
            match self.client_orders.entry((req.account_id.clone(), cid.clone())) {
                dashmap::mapref::entry::Entry::Occupied(_) => {
                    return reject(22, format!("客户端编号重复: {}", cid));
                }
                dashmap::mapref::entry::Entry::Vacant(v) => {
                    v.insert(String::new());
                }
            }
        }

        // 0. offset 归一化 + 未知值拒单 @yutiansut @quantaxis
        //
        // ⚠️ 这一步以前没有,后果是**方向做反、真金白银**:
        //   前端 components/{BatchOrderForm,ConditionalOrderForm}.vue 的
        //   「平今/平昨」发的是 `CLOSE_TODAY` / `CLOSE_YESTERDAY`(带下划线,
        //   与 protocol/diff/types.rs:439 的常量一致),而 `calculate_towards`
        //   只匹配 `OPEN`/`CLOSE`/`CLOSETODAY`,未知值落 `_ => 2`(**买开**)。
        //   订单随后由 `QAOrder::new(.., towards, ..)` 从 towards 反推 offset,
        //   于是回读也变成 OPEN —— 全程无报错、界面显示「成功」。
        //
        //   实测(2026-09-04):
        //     offset=OPEN            → 回读 OPEN     ✓
        //     offset=CLOSETODAY      → 正确拒单(无持仓可平)
        //     offset=CLOSE_TODAY     → 回读 OPEN     ✗ 平今变成开仓
        //     offset=CLOSE_YESTERDAY → 回读 OPEN     ✗ 平昨变成开仓
        //
        //   `CLOSEYESTERDAY`(规范拼写)同样没有 match 分支,一并补上 ——
        //   risk/pre_trade_check.rs:348 认它,order_router 不认。
        match Self::normalize_direction(&req.direction) {
            Some(canonical) => req.direction = canonical.to_string(),
            None => {
                return SubmitOrderResponse {
                    success: false,
                    order_id: None,
                    status: Some("rejected".to_string()),
                    error_message: Some(format!(
                        "无效的买卖方向 direction={:?};合法值:BUY / SELL(大小写不敏感)",
                        req.direction
                    )),
                    error_code: Some(4004), // 无效买卖方向
                };
            }
        }
        match Self::normalize_offset(&req.offset) {
            Some(canonical) => req.offset = canonical.to_string(),
            None => {
                return SubmitOrderResponse {
                    success: false,
                    order_id: None,
                    status: Some("rejected".to_string()),
                    error_message: Some(format!(
                        "无效的开平标志 offset={:?};合法值:OPEN / CLOSE / CLOSETODAY / CLOSEYESTERDAY                          (也接受 CLOSE_TODAY / CLOSE_YESTERDAY 写法)",
                        req.offset
                    )),
                    error_code: Some(4003), // 无效开平标志(4002 已被「无行情」占用)
                };
            }
        }

        // 1. 生成订单ID（无锁操作）
        let order_id = self.generate_order_id();
        // 客户端编号 ↔ 交易所单号:必须在撮合**之前**登记 —— 撮合途中产生的成交补丁
        // 就要用客户端编号做键 @yutiansut @quantaxis
        if let Some(ref cid) = opts.client_order_id {
            self.client_orders
                .insert((req.account_id.clone(), cid.clone()), order_id.clone());
            self.trade_gateway.register_client_order_id(&order_id, cid);
            self.persist_client_order_id(&req.account_id, cid, &order_id);
        }

        // 1.5 市价单价格转换 @yutiansut @quantaxis
        // 市价单需要从行情获取实际价格：买单用卖一价，卖单用买一价
        let req = if req.order_type == "MARKET" && req.price <= 0.0 {
            let market_price = self.get_market_price_for_order(&req.instrument_id, &req.direction);
            if market_price <= 0.0 {
                log::warn!(
                    "Cannot get market price for MARKET order: instrument={}, direction={}",
                    req.instrument_id, req.direction
                );
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id.clone()),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!(
                        "No market price available for instrument {}",
                        req.instrument_id
                    )),
                    error_code: Some(4002), // 无行情
                };
            }
            log::info!(
                "MARKET order price converted: instrument={}, direction={}, price={}",
                req.instrument_id, req.direction, market_price
            );
            SubmitOrderRequest {
                price: market_price,
                ..req
            }
        } else {
            req
        };

        // 2. 预计算所需资金（无锁操作）
        //
        // ⚠️ 原实现漏了**合约乘数**,估算比真实保证金小两个数量级:
        //     BUY  OPEN: price * volume + commission          ← 没乘 multiplier
        //     SELL OPEN: price * volume * 0.2 + commission    ← 0.2 是写死的保证金率替代
        //   IF2501 一手 3800 点、乘数 300、保证金率 12% → 真实占用约 136,800,
        //   而估算给出 3,800。于是 7.1 的资金二次校验形同虚设:
        //   money 被真实冻结一路扣到接近 3,800 才第一次不通过,而那一单
        //   又冻掉 136,800 → money = 3,800 - 136,800 ≈ -133,000。
        //
        //   2026-09-04 压测实测:NOISE_BUY 可用 -211,730.51、
        //   NOISE_SELL 可用 -190,348.54,溢出量级正好是一单的真实保证金。
        //   随后风控持续拒单(505 笔 `available=-190348.54`)。
        //
        //   乘数与保证金率从 instrument_registry 取 —— 本函数 :526 处
        //   已经在用同一个 registry 做合约状态检查,没有额外开销。
        //   未注册的合约退回原有粗略口径,保持增量、不改变既有行为。
        //   @yutiansut @quantaxis
        let (multiplier, margin_rate, commission_rate) = self
            .instrument_registry
            .get(&req.instrument_id)
            .map(|i| (i.contract_multiplier as f64, i.margin_rate, i.commission_rate))
            .unwrap_or((1.0, 0.2, 0.0003));

        let notional = req.price * req.volume * multiplier;
        let estimated_commission = notional * commission_rate;
        let required_funds = if req.offset == "OPEN" {
            // 期货开仓(买卖同向占用):保证金 = 名义金额 × 保证金率
            notional * margin_rate + estimated_commission
        } else {
            // 平仓不占新保证金,只需手续费
            estimated_commission
        };

        // 2.4 合约状态检查 @yutiansut @quantaxis
        //
        // InstrumentRegistry::is_trading / status 此前只被自身单测消费（10 处断言），
        // 生产下单路径从未读取 —— 导致 /api/admin/instrument/{id}/suspend 与 /delist
        // 只改注册表的状态位，暂停/下市的合约照样可以下单成交。
        //
        // 刻意只在「合约已注册且状态非 Active」时拒单:
        // 未注册的合约保持原有行为（继续走到撮合引擎，由其返回 Instrument not found），
        // 使本补丁严格增量、不改变任何现有可交易合约的行为。
        //
        // 只拦下单，不拦撤单 —— 合约暂停后必须仍能撤掉挂单。
        // 合约所属交易所顺手留下,写进 QIFI 委托的 exchange_id(见下方 send_order 之后)@yutiansut @quantaxis
        let mut inst_exchange = String::new();
        if let Some(inst) = self.instrument_registry.get(&req.instrument_id) {
            use crate::exchange::instrument_registry::InstrumentStatus;
            inst_exchange = inst.exchange.clone();
            if inst.status != InstrumentStatus::Active {
                let reason = format!(
                    "Instrument {} is not tradable (status: {:?})",
                    req.instrument_id, inst.status
                );
                log::warn!("Order rejected by instrument status: {}", reason);
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id.clone()),
                    status: Some("rejected".to_string()),
                    error_message: Some(reason),
                    error_code: Some(4101), // 合约状态拒绝
                };
            }
        }
        if inst_exchange.is_empty() {
            // 未登记到注册表的合约退回行情侧合约配置(与 DIFF 视图同一口径)
            inst_exchange = self.trade_gateway.exchange_of(&req.instrument_id, "");
        }

        // 2.4.1 合约必须已上市(有撮合簿)—— 在冻结资金**之前**拒 @yutiansut @quantaxis
        //
        // CTP 对未上市合约直接拒 16「找不到合约」,不冻结任何资金。原来这里放行:按兜底乘数 1 /
        // 保证金率 0.2 冻结后到路由才失败,而路由失败只把路由侧委托标成 Rejected —— qars 账户里
        // 那笔委托仍是 SUBMITTED、冻结永不释放(撤单拒绝撤 Rejected 单;2026-09-06 生产快照
        // 6 笔挂在 NOSUCH0000 等合约上)。文案与错误码沿用路由失败分支(4006 → CTP 16)。
        if self.matching_engine.get_orderbook(&req.instrument_id).is_none() {
            log::warn!("Order rejected: instrument not listed: {}", req.instrument_id);
            return SubmitOrderResponse {
                success: false,
                order_id: Some(order_id.clone()),
                status: Some("rejected".to_string()),
                error_message: Some(format!("合约不存在或未上市:{}", req.instrument_id)),
                error_code: Some(4006),
            };
        }

        // 2.5 交易状态检查 @yutiansut @quantaxis
        if let Some(ref state_machine) = self.trading_state_machine {
            use crate::exchange::OrderValidation;
            match state_machine.validate_order(&req.instrument_id) {
                OrderValidation::Allowed => {}
                OrderValidation::Rejected(reason) => {
                    log::warn!(
                        "Order rejected by trading state: {} - {}",
                        req.instrument_id,
                        reason
                    );
                    return SubmitOrderResponse {
                        success: false,
                        order_id: Some(order_id.clone()),
                        status: Some("rejected".to_string()),
                        error_message: Some(reason),
                        error_code: Some(4100), // 交易状态拒绝
                    };
                }
            }
        }

        // 3. 风控检查（无锁操作，风控器内部使用 DashMap）
        if !opts.force {
            let risk_check_req = OrderCheckRequest {
                account_id: req.account_id.clone(),
                instrument_id: req.instrument_id.clone(),
                direction: req.direction.clone(),
                offset: req.offset.clone(),
                volume: req.volume,
                price: req.price,
                limit_price: req.price,
                price_type: req.order_type.clone(),
                // 合约参数在 :507 已从 instrument_registry 取好,直接传入,
                // 避免风控模块再查一次注册表。@yutiansut @quantaxis
                contract_multiplier: multiplier,
                margin_rate,
                commission_rate,
            };

            match self.risk_checker.check(&risk_check_req) {
                Ok(RiskCheckResult::Pass) => {}
                Ok(RiskCheckResult::Reject { reason, code }) => {
                    log::warn!("Order rejected by risk check: {:?} - {}", code, reason);
                    return SubmitOrderResponse {
                        success: false,
                        order_id: Some(order_id.clone()),
                        status: Some("rejected".to_string()),
                        error_message: Some(reason),
                        error_code: Some(code as u32),
                    };
                }
                Err(e) => {
                    log::error!("Risk check error: {}", e);
                    return SubmitOrderResponse {
                        success: false,
                        order_id: Some(order_id.clone()),
                        status: Some("rejected".to_string()),
                        // ⚠️ 码用 **4005(用户输入)** 而非 9999(未知错误)。@yutiansut @quantaxis
                        //
                        // 实测(2026-09-05)走到这个分支的都是**用户输入问题**:
                        //   volume=-1 → "Order volume -1 below minimum 1"
                        //   price=0   → "Invalid order price for LIMIT order"
                        // 归到 9999「未知」会让运维排查时把用户笔误当成系统故障。
                        //
                        // 措辞去掉 `Risk check error:` 前缀 —— 原输出是
                        // "Risk check error: Risk check failed: Order volume -1 below minimum 1",
                        // **同一件事说了三遍**。ExchangeError 的 Display 本身已完整。
                        // 对照 :465/:480 那两条(非法方向/开平)是中文且列出合法值,
                        // 这里保持同一水准。
                        error_message: Some(format!("下单参数不合法:{}", e)),
                        // 4005 已被「不支持的数量条件」占用(:692),这里用 4007。
                        error_code: Some(4007),
                    };
                }
            }
        } else {
            log::warn!(
                "⚠️  Force order submitted for account {} instrument {} volume {}",
                req.account_id,
                req.instrument_id,
                req.volume
            );
        }

        // 3.5 FOK 前置检查: 如果是 FOK 订单，必须确保能全部成交
        // @yutiansut @quantaxis
        let time_cond = req.time_condition.unwrap_or(TimeCondition::GFD);
        let volume_cond = req.volume_condition.unwrap_or(VolumeCondition::ANY);

        // ⚠️ MIN 未实现 —— 拒绝而非静默按 ANY 处理 @yutiansut @quantaxis
        //
        // `VolumeCondition::MIN`(最小成交量)全仓**只有 Display / FromStr / 单测**,
        // 没有任何撮合侧实现(`grep -rn 'VolumeCondition::MIN'` 除声明外零命中)。
        // 存进 `OrderRouteInfo.volume_condition` 的那份更是从头到尾没被读过
        // (编译器的 dead_code 警告能报出来,但被 lib.rs:40 的
        //  `#![allow(dead_code)]` 全局关掉了)。
        //
        // 于是 MIN 单会按 ANY 撮合:客户要求「至少成交 N 手」,实际可能只成交 1 手。
        // 静默降级订单条件比直接拒绝危险得多 —— 客户以为条件生效了。
        // 当前前端只发 ANY/ALL,所以这条是潜在契约缺口而非现行故障;
        // 真要支持 MIN,需在撮合侧实现后再放开。
        if volume_cond == VolumeCondition::MIN {
            return SubmitOrderResponse {
                success: false,
                order_id: Some(order_id.clone()),
                status: Some("rejected".to_string()),
                error_message: Some(
                    "volume_condition=MIN 暂未实现,不接受该条件的订单(可用 ANY 或 ALL)".to_string(),
                ),
                error_code: Some(4005), // 不支持的数量条件
            };
        }

        // FOK = IOC + ALL (立即全部成交或撤销)
        let is_fok = time_cond == TimeCondition::IOC && volume_cond == VolumeCondition::ALL;

        if is_fok && !opts.force {
            if !self.check_fok_fulfillable(&req.instrument_id, &req.direction, req.volume, req.price) {
                log::warn!(
                    "[FOK] Order rejected: cannot fill {} {} {} @ {} immediately",
                    req.volume, req.direction, req.instrument_id, req.price
                );
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id.clone()),
                    status: Some("rejected".to_string()),
                    error_message: Some("FOK order cannot be fully filled immediately".to_string()),
                    error_code: Some(4010),
                };
            }
            log::info!(
                "[FOK] Pre-check passed: {} {} {} @ {}",
                req.volume, req.direction, req.instrument_id, req.price
            );
        }

        // 4. 获取账户引用（无锁操作，DashMap get）
        let account = match self.account_mgr.get_account(&req.account_id) {
            Ok(acc) => acc,
            Err(e) => {
                log::error!("Account not found: {}: {}", req.account_id, e);
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!("Account not found: {}", e)),
                    error_code: Some(4000),
                };
            }
        };

        // 5. 乐观读取检查余额（读锁，快速失败）
        if !opts.force {
            let available = account.read().money;
            if available < required_funds {
                log::warn!(
                    "Insufficient funds (optimistic): account={}, available={:.2}, required={:.2}",
                    req.account_id,
                    available,
                    required_funds
                );
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!(
                        "Insufficient funds: available={:.2}, required={:.2}",
                        available, required_funds
                    )),
                    error_code: Some(4001),
                };
            }
        }

        // 6. 预构建订单数据（无锁操作）
        let towards = self.calculate_towards(&req.direction, &req.offset);
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut order = QAOrder::new(
            req.account_id.clone(),
            req.instrument_id.clone(),
            towards,
            "EXCHANGE".to_string(),
            current_time.clone(),
            req.volume,
            req.price,
            order_id.clone(),
        );
        // ✨ 回填权威的 direction / offset —— QAOrder::new 是**有损**的
        // @yutiansut @quantaxis
        //
        // qars 的 `QAOrder::new`(qars2 order.rs:51-56)从 towards **反推**方向与开平,
        // 但它只认四个值:
        //   ```
        //   1 | 2 => (BUY, OPEN),  -1 | -2 => (SELL, OPEN),
        //   3 => (BUY, CLOSE),     -3 => (SELL, CLOSE),
        //   _ => (BUY, OPEN),      // ← 平今的 ±4 掉进这里
        //   ```
        // 而 `calculate_towards` 对平今产出 ±4。后果(2026-09-04 直接解码
        // gateway_wal 的 rkyv 记录、以 SELL OPEN 作 ground truth 校准字节位实测):
        //   · 提交 SELL CLOSETODAY   → 落盘 **direction=BUY, offset=OPEN**(方向翻转!)
        //   · 提交 BUY  CLOSETODAY   → 落盘 offset=OPEN(平仓变开仓)
        //   · 提交 CLOSEYESTERDAY    → 落盘 offset=CLOSE(平昨的区分丢失)
        //
        // 这个降级后的 `order` 随后被 `handle_success_result` 当作权威值,
        // `&order.offset` / `&order.direction` 一路传到 `WalRecord::encode_*`,
        // 所以那两个编码函数**永远收不到** CLOSETODAY/CLOSEYESTERDAY ——
        // 光修编码是不够的,值在上游就已经没了。
        //
        // 账户持仓数学走的是 towards(未受影响),错的是**订单记录**:
        // QIFI orders、WAL、以及 API 返回给用户的委托。
        // `req.direction` / `req.offset` 在 :454 已归一化,是权威源,直接覆盖。
        order.direction = req.direction.clone();
        order.offset = req.offset.clone();

        // 7. 短时写锁：仅用于冻结资金 + send_order
        // 优化点：将锁范围缩小到最小必要操作
        let qa_order_id = {
            let mut acc = account.write();

            // 7.1 二次验证（写锁内，避免竞态）
            if !opts.force && acc.money < required_funds {
                log::warn!(
                    "Insufficient funds (double-check): account={}, available={:.2}, required={:.2}",
                    req.account_id,
                    acc.money,
                    required_funds
                );
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!(
                        "Insufficient funds: available={:.2}, required={:.2}",
                        acc.money, required_funds
                    )),
                    error_code: Some(4001),
                };
            }

            // 7.1.5 按合约覆盖的费率 / 保证金写进持仓副本(必要时先建仓),
            //       让本单的冻结保证金与之后的成交手续费都按覆盖值算 @yutiansut @quantaxis
            crate::core::account_ext::apply_preset_override(&mut acc, &req.instrument_id, true);

            // 7.2 执行 send_order（冻结资金）
            match acc.send_order(
                &req.instrument_id,
                req.volume,
                &current_time,
                towards,
                req.price,
                "",
                &req.order_type,
            ) {
                Ok(ref qa_order) => {
                    // ✨ 降为 debug + 内联:原来是热路径 info! 每笔订单刷屏,且 frozen_keys 每次克隆全量。
                    //    debug! 默认关闭且惰性求值,内联后关闭时零分配。@yutiansut @quantaxis
                    log::debug!(
                        "After send_order: qa_order_id={}, money={:.2}, frozen_count={}, frozen_keys={:?}",
                        qa_order.order_id,
                        acc.money,
                        acc.frozen.len(),
                        acc.frozen.keys().cloned().collect::<Vec<String>>()
                    );
                    // ✨ 把交易所单号写进账户 QIFI 订单的 exchange_order_id(QIFI 本义就是交易所单号),
                    //    随 60s 快照落盘 —— 重启恢复挂单时据此还原原单号,客户端用旧单号仍能撤单。
                    //    此前该字段恒为空串,恢复出来的挂单单号变成 qars UUID。@yutiansut @quantaxis
                    if let Some(o) = acc.dailyorders.get_mut(&qa_order.order_id) {
                        o.exchange_order_id = order_id.clone();
                        // ✨ qars sim 分支把 time_condition 写死成 "AND"、volume_condition 写死成 "GFD"、
                        //    exchange_id 写死成 ""(qars2 account.rs:1413-1432)—— 落盘 / 恢复 / HTTP 视图
                        //    看到的都是这三个占位值。按请求如实回填(缺省 GFD / ANY,与 TimeCondition /
                        //    VolumeCondition 的 Default 一致)。@yutiansut @quantaxis
                        o.time_condition = req.time_condition.unwrap_or_default().to_string();
                        o.volume_condition = req.volume_condition.unwrap_or_default().to_string();
                        if o.exchange_id.is_empty() {
                            o.exchange_id = inst_exchange.clone();
                        }
                    }
                    qa_order.order_id.clone()
                }
                Err(()) => {
                    // ✨ qars 的 `send_order` 返回 `Result<QAOrder, ()>` —— 错误类型就是
                    //    **unit**,不带任何原因。原写法直接 `format!("Insufficient
                    //    funds/margin: {:?}", e)`,于是:
                    //      · 无持仓平仓        → 报「资金不足」(定性就是错的)
                    //      · 手数为 0          → 同上
                    //      · 真的资金不足      → 同上
                    //    三种情况给出完全相同的消息,末尾还带个空的 `()`。
                    //    用户看到「Insufficient funds/margin: ()」无从下手。
                    //
                    //    qaexchange 自己有足够信息判定真实原因,这里补上。
                    //    @yutiansut @quantaxis
                    let is_close = req.offset != "OPEN";
                    let reason = if req.volume <= 0.0 {
                        "下单手数必须大于 0".to_string()
                    } else if is_close {
                        // 平仓失败最常见的原因是可平持仓不足(qars 在 hold 里找不到或数量不够)
                        let (long_avail, short_avail) = acc
                            .get_position(&req.instrument_id)
                            .map(|p| {
                                (
                                    // ✨ 可平量含冻结 —— `today + his` 漏掉挂单冻结的部分。
                                    // qars 的不变量是 `volume = today + his + frozen`
                                    // (position.rs:403)。用错公式会让提示里的
                                    // 「该方向可平 N」偏小:线上 ACC_55361002 的 IF2502
                                    // 实际持有 285 手,旧式只报 216。@yutiansut @quantaxis
                                    p.volume_long_unmut(),
                                    p.volume_short_unmut(),
                                )
                            })
                            .unwrap_or((0.0, 0.0));
                        let side_avail = if req.direction == "SELL" { long_avail } else { short_avail };
                        if side_avail < req.volume {
                            format!(
                                "可平持仓不足: {} 需平 {},该方向可平 {}(多 {} / 空 {})",
                                req.instrument_id, req.volume, side_avail, long_avail, short_avail
                            )
                        } else {
                            format!(
                                "平仓被账户拒绝(持仓 {} 足够,可能是手续费不足: 可用 {:.2})",
                                side_avail, acc.money
                            )
                        }
                    } else {
                        format!("资金不足: 可用 {:.2}", acc.money)
                    };
                    log::warn!(
                        "Order rejected for account {}: {} (offset={}, direction={}, volume={})",
                        req.account_id,
                        reason,
                        req.offset,
                        req.direction,
                        req.volume
                    );
                    return SubmitOrderResponse {
                        success: false,
                        order_id: Some(order_id),
                        status: Some("rejected".to_string()),
                        error_message: Some(reason),
                        error_code: Some(4001),
                    };
                }
            }
            // 写锁在此自动释放（RAII）
        };

        // ✨ 调试日志：显示 qa_order_id 和 towards 值 @yutiansut @quantaxis
        log::info!(
            "🔐 Order submitted: order_id={}, qa_order_id={}, towards={}, direction={}, offset={}",
            order_id,
            qa_order_id,
            towards,
            req.direction,
            req.offset
        );

        // 4. 存储订单信息
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let time_cond = req.time_condition.unwrap_or(TimeCondition::GFD);
        let volume_cond = req.volume_condition.unwrap_or(VolumeCondition::ANY);
        let route_info = OrderRouteInfo {
            order: order.clone(),
            status: OrderStatus::PendingRoute,
            submit_time: timestamp,
            update_time: timestamp,
            filled_volume: 0.0,
            qa_order_id: qa_order_id.clone(), // 存储 qars 订单ID
            matching_engine_order_id: None,   // 撮合引擎订单ID (在 Accepted 事件中设置)
            time_condition: time_cond,
            volume_condition: volume_cond,
        };

        self.orders
            .insert(order_id.clone(), Arc::new(RwLock::new(route_info)));

        // 5. 更新账户订单索引
        self.user_orders
            .entry(req.account_id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(order_id.clone());

        // 6. 注册活动订单 (风控追踪)
        self.risk_checker.register_active_order(
            &req.account_id,
            order_id.clone(),
            req.instrument_id.clone(),
            req.direction.clone(),
            req.price,              // ✅ price 作为 limit_price
            req.order_type.clone(), // ✅ order_type 作为 price_type
        );

        // 7. 路由到撮合引擎
        match self.route_to_matching_engine(&req.instrument_id, order, order_id.clone()) {
            Ok(_) => {
                log::info!("Order submitted successfully: {}", order_id);

                // 获取订单的最终状态（可能已经成交）
                let final_status = if let Some(order_info) = self.orders.get(&order_id) {
                    let info = order_info.read();
                    let status_str = match info.status {
                        OrderStatus::Filled => "filled",
                        OrderStatus::PartiallyFilled => "partially_filled",
                        OrderStatus::Cancelled => "cancelled",
                        OrderStatus::Rejected => "rejected",
                        _ => "submitted", // Submitted, PendingRoute, PendingRisk
                    };
                    log::debug!(
                        "Order {} final status: {:?} -> {}",
                        order_id,
                        info.status,
                        status_str
                    );
                    status_str
                } else {
                    log::warn!(
                        "Order {} not found in orders map when checking status",
                        order_id
                    );
                    "submitted"
                };

                // IOC 后置处理: 如果是 IOC 订单，自动撤销未成交部分
                // @yutiansut @quantaxis
                if time_cond == TimeCondition::IOC {
                    self.handle_ioc_remaining(&order_id, &req.account_id);

                    // 重新获取状态（可能已被撤销）
                    let updated_status = if let Some(order_info) = self.orders.get(&order_id) {
                        let info = order_info.read();
                        match info.status {
                            OrderStatus::Filled => "filled",
                            OrderStatus::PartiallyFilled => "partially_filled",
                            OrderStatus::Cancelled => "cancelled",
                            OrderStatus::Rejected => "rejected",
                            _ => "submitted",
                        }
                    } else {
                        final_status
                    };

                    log::info!("[IOC] Order {} final status after IOC handling: {}", order_id, updated_status);

                    return SubmitOrderResponse {
                        success: true,
                        order_id: Some(order_id),
                        status: Some(updated_status.to_string()),
                        error_message: None,
                        error_code: None,
                    };
                }

                SubmitOrderResponse {
                    success: true,
                    order_id: Some(order_id),
                    status: Some(final_status.to_string()),
                    error_message: None,
                    error_code: None,
                }
            }
            Err(e) => {
                log::error!("Failed to route order {}: {}", order_id, e);

                // 更新订单状态为拒绝
                if let Some(order_info) = self.orders.get(&order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Rejected;
                }

                SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    // ⚠️ 区分「用户给了不存在的合约」与「真的路由/撮合故障」。
                    // @yutiansut @quantaxis
                    //
                    // 实测:对不存在的合约下单,用户看到的是
                    //   "Routing error: Matching error: Orderbook not found for instrument: NOSUCH0000"
                    // —— **三层嵌套 + 暴露内部实现(orderbook)**,而且码是 5000(服务端错误段),
                    // 会让运维误以为是系统故障。这其实是最常见的用户输入错误之一。
                    //
                    // 判据用错误文本里的 "Orderbook not found":撮合引擎对未注册合约
                    // 就是这么报的(matching 层无该合约的簿)。
                    error_message: Some(if e.to_string().contains("Orderbook not found") {
                        format!("合约不存在或未上市:{}", req.instrument_id)
                    } else {
                        format!("订单路由失败:{}", e)
                    }),
                    error_code: Some(if e.to_string().contains("Orderbook not found") {
                        4006 // 合约不存在(用户输入)
                    } else {
                        5000 // 真的路由/撮合故障(服务端)
                    }),
                }
            }
        }
    }

    /// 路由订单到撮合引擎
    fn route_to_matching_engine(
        &self,
        instrument_id: &str,
        order: Order,
        order_id: String,
    ) -> Result<(), ExchangeError> {
        // 获取订单簿
        let orderbook = self
            .matching_engine
            .get_orderbook(instrument_id)
            .ok_or_else(|| {
                ExchangeError::MatchingError(format!(
                    "Orderbook not found for instrument: {}",
                    instrument_id
                ))
            })?;

        // 转换订单方向
        let direction = match order.direction.as_str() {
            "BUY" => OrderDirection::BUY,
            "SELL" => OrderDirection::SELL,
            _ => {
                return Err(ExchangeError::OrderError(format!(
                    "Invalid direction: {}",
                    order.direction
                )))
            }
        };

        // 创建撮合订单请求
        let asset = InstrumentAsset::from_code(instrument_id);
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let match_request = crate::matching::orders::new_limit_order_request(
            asset,
            direction,
            order.limit_price,
            order.volume_orign,
            timestamp,
        );

        // 提交到订单簿
        let mut ob = orderbook.write();
        let results = ob
            .process_order(match_request)
            .into_iter()
            .collect::<Vec<_>>();
        drop(ob); // 尽早释放锁

        // 处理撮合结果
        self.process_matching_results(&order_id, &order, results)?;

        Ok(())
    }

    /// 处理撮合引擎返回的结果
    ///
    /// 注意：matching engine可能返回多个Success事件：
    /// 1. Accepted - 订单被接受
    /// 2. Filled/PartiallyFilled - 新订单成交
    /// 3. Filled/PartiallyFilled - 对手单成交（opposite_order）
    ///
    /// 我们只处理新订单的事件，忽略对手单的事件
    fn process_matching_results(
        &self,
        order_id: &str,
        order: &Order,
        results: Vec<Result<Success, Failed>>,
    ) -> Result<(), ExchangeError> {
        let mut handled_accepted = false;
        // ✨ 记住 taker 自己的 engine_id,用它判别每个成交事件归属 @yutiansut @quantaxis
        //
        // 原先这里是 `handled_trade: bool` —— 一次性标志,只把**第一个**成交事件
        // 当作 taker,其余全部当 maker。但一张扫多档的单会产生多组事件,
        // qars2 的顺序是「主动方、对手方、主动方、对手方…」
        // (qars2 orderbook.rs:1103/1113 与 :1145/:1157 两分支都是先 taker 后 maker):
        //
        //   [0] PartiallyFilled{taker, v1}   → 记为 taker  ✓
        //   [1] Filled{maker1,   v1}         → 记为 maker  ✓
        //   [2] PartiallyFilled{taker, v2}   → 记为 maker  ✗ 这是 taker 自己
        //   [3] Filled{maker2,   v2}         → 记为 maker  ✓
        //   [4] Filled{taker,    v3}         → 记为 maker  ✗
        //
        // 后果:扫 N 档的单只有第 1 笔进 TradeRecorder。实测全市场
        // TradeRecorder 12730 腿 / QIFI 25632 条 = **49.7%**,正好丢一半
        // (平均扫 2 档)。
        //
        // 正确判别:成交事件的 match_order_id == taker 自己的 engine_id。
        // 首个成交事件必定属于 taker(见上面的发射顺序),由它确定该 id。
        let mut taker_engine_id: Option<u64> = None;

        log::debug!(
            "🔍 process_matching_results: order_id={}, user_id={}, results_count={}",
            order_id,
            order.user_id,
            results.len()
        );

        for (idx, result) in results.into_iter().enumerate() {
            log::debug!("🔍   Result[{}]: {:?}", idx, result);
            match result {
                Ok(success) => {
                    match success {
                        Success::Accepted { .. } => {
                            // 只处理第一个Accepted
                            if !handled_accepted {
                                log::debug!(
                                    "🔍     Processing Accepted event for order {}",
                                    order_id
                                );
                                // Accepted 事件不涉及成交记录，is_taker 参数无影响
                                self.handle_success_result(order_id, order, success, true)?;
                                handled_accepted = true;
                            } else {
                                log::debug!(
                                    "🔍     Skipping duplicate Accepted event for order {}",
                                    order_id
                                );
                            }
                        }
                        Success::Filled {
                            order_id: match_order_id,
                            opposite_order_id,
                            ..
                        }
                        | Success::PartiallyFilled {
                            order_id: match_order_id,
                            opposite_order_id,
                            ..
                        } => {
                            // 处理成交事件
                            // qars 会返回两个事件：新订单成交 + 对手单成交
                            // 我们需要更新对手单的状态（如果它属于我们管理的订单）

                            // ✨ 按 engine_id 判别归属,而不是「是不是第一个」 @yutiansut @quantaxis
                            let is_taker_event = match taker_engine_id {
                                None => {
                                    taker_engine_id = Some(match_order_id);
                                    true
                                }
                                Some(tid) => match_order_id == tid,
                            };

                            if is_taker_event {
                                // 新订单(taker - 主动方)的成交,可能有多笔(扫多档)
                                log::debug!(
                                    "🔍     Processing TAKER order trade: order_id={}, opposite={}",
                                    match_order_id,
                                    opposite_order_id
                                );
                                // ✨ is_taker=true: 主动方，记录成交到 TradeRecorder @yutiansut @quantaxis
                                self.handle_success_result(order_id, order, success.clone(), true)?;
                            } else {
                                // 第二个事件：对手单（挂单方）的成交
                                // qars 返回的第二个 Filled 事件中：
                                // - match_order_id = 对手单（挂单方）的 engine_id
                                // - opposite_order_id = 新订单（taker）的 engine_id
                                // 我们需要用 match_order_id 找到对手单的 order_id 来更新其账户
                                log::debug!("🔍     Processing MAKER order trade: maker_engine_id={}, taker_engine_id={}", match_order_id, opposite_order_id);

                                // ✨ 关键修复：使用 match_order_id（对手单的engine_id）查找对手单的 order_id
                                // 之前的 BUG：使用 opposite_order_id 查找，导致找到的是已处理的新订单
                                // @yutiansut @quantaxis
                                if let Some(maker_order_id_str) = self
                                    .engine_id_to_order
                                    .get(&(order.instrument_id.clone(), match_order_id))
                                {
                                    let maker_order_str = maker_order_id_str.value().clone();
                                    log::debug!("🔍     Found maker order mapping: engine_id={} → order_id={}", match_order_id, maker_order_str);

                                    // 如果挂单方（maker）在我们的订单簿中，更新它的状态
                                    if self.orders.contains_key(&maker_order_str) {
                                        log::debug!("🔍     Found maker order {} in our orderbook, updating status", maker_order_str);

                                        // 提取挂单方信息用于处理
                                        if let Some(maker_info) = self.orders.get(&maker_order_str) {
                                            let maker_order_data = maker_info.read().order.clone();
                                            // 处理挂单方的成交 - 更新其账户持仓和资金
                                            // ✨ is_taker=false: 被动方（maker），不记录成交到 TradeRecorder @yutiansut @quantaxis
                                            self.handle_success_result(
                                                &maker_order_str,
                                                &maker_order_data,
                                                success,
                                                false, // maker 不记录成交
                                            )?;
                                        }
                                    } else {
                                        log::warn!(
                                            "⚠️     Maker order {} not found in our orderbook (inconsistent state!)",
                                            maker_order_str
                                        );
                                    }
                                } else {
                                    log::debug!(
                                        "🔍     Maker order engine_id={} not in our exchange (external order), skipping",
                                        match_order_id
                                    );
                                }
                            }
                        }
                        _ => {
                            // 其他事件正常处理（Cancelled, Amended等）
                            // 不涉及成交记录，is_taker 参数无影响
                            self.handle_success_result(order_id, order, success, true)?;
                        }
                    }
                }
                Err(failed) => {
                    log::warn!("Order matching failed: {:?}", failed);

                    // Phase 6: 使用新的 handle_order_rejected_new (交易所推送REJECTED回报)
                    let reason = format!("{:?}", failed);
                    let _ = self.trade_gateway.handle_order_rejected_new(
                        &order.exchange_id,
                        &order.instrument_id,
                        &order.user_id,
                        order_id,
                        &order.direction,
                        &order.offset,
                        &order.price_type,
                        order.limit_price,
                        order.volume_orign,
                        &reason,
                    );

                    log::debug!("Order {} rejected, reason: {}", order_id, reason);

                    // 更新订单状态为拒绝
                    if let Some(order_info) = self.orders.get(order_id) {
                        let mut info = order_info.write();
                        info.status = OrderStatus::Rejected;
                        info.update_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    }
                    self.push_diff_order(order_id, Some(&reason));
                }
            }
        }
        Ok(())
    }

    /// 处理成功的撮合结果 (Phase 6: 使用新的回报机制)
    /// 处理成交结果
    /// @yutiansut @quantaxis
    /// is_taker: 是否为主动方（taker），只有 taker 才记录到 TradeRecorder
    fn handle_success_result(
        &self,
        order_id: &str,
        order: &Order,
        success: Success,
        is_taker: bool, // ✨ 是否为主动方
    ) -> Result<(), ExchangeError> {
        match success {
            Success::Accepted { id, order_type: _, ts } => {
                // 订单被接受，等待撮合
                log::info!("Order {} accepted at {}", order_id, ts);

                // 更新订单状态并存储撮合引擎订单ID
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Submitted;
                    info.update_time = ts;
                    info.matching_engine_order_id = Some(id); // 存储撮合引擎订单ID，用于撤单
                }

                // ✨ 存储反向映射: matching_engine_order_id → order_id / user_id @yutiansut @quantaxis
                // 用于在成交时通过对手单的matching_engine_order_id找到对应的order_id和user_id
                self.engine_id_to_order
                    .insert((order.instrument_id.clone(), id), order_id.to_string());
                self.engine_id_to_user
                    .insert((order.instrument_id.clone(), id), order.user_id.clone());
                log::debug!("💾 Stored reverse mapping: engine_id={} → order_id={}, user_id={}", id, order_id, order.user_id);

                // Phase 6: 使用新的 handle_order_accepted_new (交易所只推送ACCEPTED回报)
                let exchange_order_id = self.trade_gateway.handle_order_accepted_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    &order.user_id,
                    order_id,
                    &order.direction,
                    &order.offset,
                    &order.price_type,
                    order.limit_price,
                    order.volume_orign,
                )?;

                log::debug!(
                    "Order {} accepted, exchange_order_id={}",
                    order_id,
                    exchange_order_id
                );

                // 持久化订单簿tick数据（订单挂入导致bid/ask变化）
                self.persist_orderbook_tick(&order.instrument_id)?;

                // 广播订单簿更新（通知前端订单簿已变化）
                if let Some(ref broadcaster) = self.market_broadcaster {
                    // 获取更新后的bid/ask价格用于广播
                    if let Some(orderbook) =
                        self.matching_engine.get_orderbook(&order.instrument_id)
                    {
                        let _ob = orderbook.read();
                        let side = if order.direction == "BUY" {
                            "bid"
                        } else {
                            "ask"
                        };
                        broadcaster.broadcast_orderbook_update(
                            order.instrument_id.clone(),
                            side.to_string(),
                            order.limit_price,
                            order.volume_orign,
                        );
                    }
                }

                // 持久化订单簿快照（订单已进入订单簿）
                self.persist_orderbook_snapshot(&order.instrument_id)?;
            }
            Success::Filled {
                order_id: match_order_id,
                direction: _,
                order_type: _,
                price,
                volume,
                ts,
                opposite_order_id,
            } => {
                // 订单完全成交
                log::info!(
                    "Order {} filled: price={}, volume={}",
                    order_id,
                    price,
                    volume
                );

                // 更新订单状态和已成交量
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Filled;
                    info.update_time = ts;
                    // ✨ 必须累加,不能赋值 @yutiansut @quantaxis
                    //
                    // qars2 的 Success::Filled / PartiallyFilled 里的 volume 都是
                    // **本次撮合**的成交量,不是累计量
                    // (qars2/src/qamarket/matchengine/orderbook.rs:1099-1174 —— 两个分支
                    //  发出的 volume 分别是 `volume`(来单剩余) 和 `opposite_order.volume`,
                    //  都是单次撮合量)。
                    //
                    // 一张 10 手的单吃掉 3 档 (4+3+3) 会依次收到
                    //   PartiallyFilled{4} → PartiallyFilled{3} → Filled{3}
                    // 原先终态用 `=` 会把前两笔累计的 7 手抹掉,filled_volume 变成 3。
                    info.filled_volume += volume;
                }

                // 更新成交统计
                self.update_trade_stats(price, volume);

                // 行情副作用只在 taker 侧做 @yutiansut @quantaxis
                //
                // 一笔撮合是**一个**市场事件,但 qars2 会为买卖双方各发一个 Success
                // 事件,qaexchange 对 taker 和 maker 都调 handle_success_result
                // (:947 传 true / :973 传 false),于是下面这些广播和落盘全部跑两遍。
                // 叠加 trade_gateway.rs 里 mds 那一组(同样跑两遍),
                // 每笔成交一共产生 4 个 MarketDataEvent::Tick,
                // 而 KLineActor 对每个 Tick 无条件累加 volume
                // (kline_actor.rs:417) → K线成交量约 4 倍。
                if is_taker {
                    // ✨ 不在这里广播 Tick @yutiansut @quantaxis
                    // Tick 的唯一广播点是 MarketDataService::on_trade —— 它才是
                    // 行情摄入入口,且有单测锁定该契约。这里再广播一次会让
                    // 每笔成交重复计入 K线 volume。最新价仍从这里发(不进 K线聚合)。
                    if let Some(ref broadcaster) = self.market_broadcaster {
                        broadcaster.broadcast_last_price(order.instrument_id.clone(), price);
                    }

                    // 持久化Tick数据到WAL
                    self.persist_tick_data(&order.instrument_id, price, volume)?;

                    // 持久化订单簿快照（订单成交后订单簿发生变化）
                    self.persist_orderbook_snapshot(&order.instrument_id)?;
                }

                // 获取 qars 订单ID
                let qa_order_id = if let Some(order_info) = self.orders.get(order_id) {
                    order_info.read().qa_order_id.clone()
                } else {
                    log::error!("Order info not found for {}", order_id);
                    String::new()
                };

                // ✨ O(1) 直接查找对手方的 user_id @yutiansut @quantaxis
                // 性能优化：避免两次DashMap查找 + 一次RwLock读取
                let opposite_user_id: Option<String> = self
                    .engine_id_to_user
                    .get(&(order.instrument_id.clone(), opposite_order_id))
                    .map(|v| v.value().clone());

                // ✨ O(1) 查找对手方的真实订单ID @yutiansut @quantaxis
                let opposite_order_id_str: Option<String> = self
                    .engine_id_to_order
                    .get(&(order.instrument_id.clone(), opposite_order_id))
                    .map(|v| v.value().clone());

                log::debug!(
                    "⚡ Trade opposite lookup (O(1)): engine_id={} -> user_id={:?}, order_id={:?}",
                    opposite_order_id,
                    opposite_user_id,
                    opposite_order_id_str
                );

                // Phase 6: 使用新的 handle_trade_new (交易所只推送TRADE回报，不判断FILLED/PARTIAL)
                // 注意：这里假设我们使用已生成的exchange_order_id（从Accepted事件保存）
                // 简化实现：使用match_order_id作为exchange_order_id
                // ✨ 修复：传递qa_order_id用于调用receive_deal_sim @yutiansut @quantaxis
                let trade_id = self.trade_gateway.handle_trade_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    match_order_id as i64,
                    &order.user_id,
                    order_id,
                    &order.direction,
                    &order.offset,
                    volume,
                    price,
                    Some(opposite_order_id as i64),
                    opposite_user_id.as_deref(), // ✨ 传递对手方user_id
                    &qa_order_id, // ✨ 传递qars订单ID
                    opposite_order_id_str.as_deref(), // ✨ 传递对手方真实订单ID
                    is_taker, // ✨ 是否为主动方，只有 taker 记录成交 @yutiansut @quantaxis
                )?;

                log::debug!(
                    "Trade executed: trade_id={}, order_id={}, volume={}, price={}",
                    trade_id,
                    order_id,
                    volume,
                    price
                );

                // 从活动订单追踪中移除
                self.risk_checker
                    .remove_active_order(&order.user_id, order_id);
            }
            Success::PartiallyFilled {
                order_id: match_order_id,
                direction: _,
                order_type: _,
                price,
                volume,
                ts,
                opposite_order_id,
            } => {
                // 订单部分成交
                log::info!(
                    "Order {} partially filled: price={}, volume={}",
                    order_id,
                    price,
                    volume
                );

                // 更新订单状态和累计成交量
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::PartiallyFilled;
                    info.update_time = ts;
                    info.filled_volume += volume;
                }

                // 更新成交统计
                self.update_trade_stats(price, volume);

                // 行情副作用只在 taker 侧做 @yutiansut @quantaxis
                //
                // 一笔撮合是**一个**市场事件,但 qars2 会为买卖双方各发一个 Success
                // 事件,qaexchange 对 taker 和 maker 都调 handle_success_result
                // (:947 传 true / :973 传 false),于是下面这些广播和落盘全部跑两遍。
                // 叠加 trade_gateway.rs 里 mds 那一组(同样跑两遍),
                // 每笔成交一共产生 4 个 MarketDataEvent::Tick,
                // 而 KLineActor 对每个 Tick 无条件累加 volume
                // (kline_actor.rs:417) → K线成交量约 4 倍。
                if is_taker {
                    // ✨ 不在这里广播 Tick @yutiansut @quantaxis
                    // Tick 的唯一广播点是 MarketDataService::on_trade —— 它才是
                    // 行情摄入入口,且有单测锁定该契约。这里再广播一次会让
                    // 每笔成交重复计入 K线 volume。最新价仍从这里发(不进 K线聚合)。
                    if let Some(ref broadcaster) = self.market_broadcaster {
                        broadcaster.broadcast_last_price(order.instrument_id.clone(), price);
                    }

                    // 持久化Tick数据到WAL
                    self.persist_tick_data(&order.instrument_id, price, volume)?;

                    // 持久化订单簿快照（订单成交后订单簿发生变化）
                    self.persist_orderbook_snapshot(&order.instrument_id)?;
                }

                // 获取 qars 订单ID
                let qa_order_id = if let Some(order_info) = self.orders.get(order_id) {
                    order_info.read().qa_order_id.clone()
                } else {
                    log::error!("Order info not found for {}", order_id);
                    String::new()
                };

                // ✨ O(1) 直接查找对手方的 user_id @yutiansut @quantaxis
                // 性能优化：避免两次DashMap查找 + 一次RwLock读取
                let opposite_user_id: Option<String> = self
                    .engine_id_to_user
                    .get(&(order.instrument_id.clone(), opposite_order_id))
                    .map(|v| v.value().clone());

                // ✨ O(1) 查找对手方的真实订单ID @yutiansut @quantaxis
                let opposite_order_id_str: Option<String> = self
                    .engine_id_to_order
                    .get(&(order.instrument_id.clone(), opposite_order_id))
                    .map(|v| v.value().clone());

                log::debug!(
                    "⚡ Trade opposite lookup (O(1), partial): engine_id={} -> user_id={:?}, order_id={:?}",
                    opposite_order_id,
                    opposite_user_id,
                    opposite_order_id_str
                );

                // Phase 6: 使用新的 handle_trade_new (交易所不区分FILLED/PARTIAL，只推送TRADE)
                // ✨ 修复：传递qa_order_id用于调用receive_deal_sim @yutiansut @quantaxis
                let trade_id = self.trade_gateway.handle_trade_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    match_order_id as i64,
                    &order.user_id,
                    order_id,
                    &order.direction,
                    &order.offset,
                    volume,
                    price,
                    Some(opposite_order_id as i64),
                    opposite_user_id.as_deref(), // ✨ 传递对手方user_id
                    &qa_order_id, // ✨ 传递qars订单ID
                    opposite_order_id_str.as_deref(), // ✨ 传递对手方真实订单ID
                    is_taker, // ✨ 是否为主动方，只有 taker 记录成交 @yutiansut @quantaxis
                )?;

                log::debug!(
                    "Trade executed (partial): trade_id={}, order_id={}, volume={}, price={}",
                    trade_id,
                    order_id,
                    volume,
                    price
                );
            }
            Success::Cancelled { id, ts } => {
                // 订单被撤销
                log::info!("Order {} cancelled at {}", order_id, ts);

                // 更新订单状态，并获取 qa_order_id 用于释放冻结资金
                // ✨ 修复：获取 qa_order_id 传递给 handle_cancel_accepted_new @yutiansut @quantaxis
                let (qa_order_id, remaining_volume) = if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Cancelled;
                    info.update_time = ts;
                    let remaining = info.order.volume_orign - info.filled_volume;
                    (info.qa_order_id.clone(), remaining)
                } else {
                    (String::new(), order.volume_orign)
                };

                // Phase 6: 使用新的 handle_cancel_accepted_new (交易所推送CANCEL_ACCEPTED回报)
                // ✨ 修复：传递 qa_order_id 用于调用 qars cancel_order 释放冻结资金 @yutiansut @quantaxis
                self.trade_gateway.handle_cancel_accepted_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    id as i64, // 使用撮合引擎返回的ID作为exchange_order_id
                    &order.user_id,
                    order_id,
                    &order.direction,
                    &order.offset,
                    &order.price_type,
                    order.limit_price,
                    remaining_volume,
                    &qa_order_id, // ✨ 新增：传递 qars 订单ID 释放冻结资金
                )?;

                log::debug!(
                    "Order {} cancel accepted, exchange_order_id={}",
                    order_id,
                    id
                );

                // 持久化订单簿tick数据（撤单导致bid/ask变化）
                self.persist_orderbook_tick(&order.instrument_id)?;

                // 广播订单簿更新（通知前端订单簿已变化）
                if let Some(ref broadcaster) = self.market_broadcaster {
                    // 撤单后，该价格档位的挂单量减少或消失
                    if let Some(orderbook) =
                        self.matching_engine.get_orderbook(&order.instrument_id)
                    {
                        let ob = orderbook.read();
                        let side = if order.direction == "BUY" {
                            "bid"
                        } else {
                            "ask"
                        };

                        // 获取撤单后该价格档位的剩余挂单量
                        let remaining_volume = if order.direction == "BUY" {
                            ob.bid_queue
                                .get_sorted_orders()
                                .and_then(|orders| {
                                    orders
                                        .iter()
                                        .find(|o| o.price == order.limit_price)
                                        .map(|o| o.volume) // 在闭包内 map 以复制值
                                })
                                .unwrap_or(0.0)
                        } else {
                            ob.ask_queue
                                .get_sorted_orders()
                                .and_then(|orders| {
                                    orders
                                        .iter()
                                        .find(|o| o.price == order.limit_price)
                                        .map(|o| o.volume) // 在闭包内 map 以复制值
                                })
                                .unwrap_or(0.0)
                        };

                        broadcaster.broadcast_orderbook_update(
                            order.instrument_id.clone(),
                            side.to_string(),
                            order.limit_price,
                            remaining_volume, // 0表示该档位已清空
                        );
                    }
                }

                // 持久化订单簿快照（撤单后订单簿发生变化）
                self.persist_orderbook_snapshot(&order.instrument_id)?;

                // 从活动订单追踪中移除
                self.risk_checker
                    .remove_active_order(&order.user_id, order_id);
            }
            Success::Amended {
                id: _,
                price,
                volume,
                ts: _,
            } => {
                // 订单修改 (暂不处理，预留)
                log::info!(
                    "Order {} amended: price={}, volume={}",
                    order_id,
                    price,
                    volume
                );
            }
        }
        // ✨ 每次状态变化后推 DIFF 订单视图(taker 与被动成交的 maker 都走这里;
        //    成交补丁已在 handle_trade_new 里先推,保证「成交先于终态」)@yutiansut @quantaxis
        self.push_diff_order(order_id, None);
        Ok(())
    }

    /// 撤单
    pub fn cancel_order(&self, req: CancelOrderRequest) -> Result<(), ExchangeError> {
        // ✨ 认客户端编号:DIFF 客户端撤自己的单只知道自己的 order_id @yutiansut @quantaxis
        let req = CancelOrderRequest {
            order_id: self.resolve_order_id(&req.account_id, &req.order_id),
            account_id: req.account_id,
        };
        // 1. 验证订单存在
        let order_info = self.orders.get(&req.order_id).ok_or_else(|| {
            ExchangeError::OrderError(format!("Order not found: {}", req.order_id))
        })?;

        let info = order_info.write();

        // 2. 验证订单所有权
        if info.order.user_id != req.account_id {
            return Err(ExchangeError::OrderError(
                "Order does not belong to this account".to_string(),
            ));
        }

        // 2.5 交易状态检查 @yutiansut @quantaxis
        if let Some(ref state_machine) = self.trading_state_machine {
            use crate::exchange::OrderValidation;
            match state_machine.validate_cancel(&info.order.instrument_id) {
                OrderValidation::Allowed => {}
                OrderValidation::Rejected(reason) => {
                    return Err(ExchangeError::OrderError(format!(
                        "Cancel rejected by trading state: {}",
                        reason
                    )));
                }
            }
        }

        // 3. 检查订单状态是否可撤单
        if !matches!(
            info.status,
            OrderStatus::Submitted | OrderStatus::PartiallyFilled
        ) {
            return Err(ExchangeError::OrderError(format!(
                "Order cannot be cancelled in status: {:?}",
                info.status
            )));
        }

        // 4. 从撮合引擎撤单
        //
        // ✨ 没有撮合引擎订单号时**走本地撤单**,不能直接报错。@yutiansut @quantaxis
        //
        // 原实现在这里 `ok_or_else(...)?` 直接失败,后果是这类订单**永久卡死**:
        //   ① 未注册合约的下单是**被接受**的(:562 有意的兜底口径
        //      `unwrap_or((1.0, 0.2, 0.0003))`,注释写着「保持增量、不改变既有行为」);
        //   ② 但撮合引擎没有该合约的订单簿 ⇒ 订单永不成交、拿不到 engine order id;
        //   ③ 于是撤单在这一行失败 ⇒ **冻结的保证金没有任何回收途径**。
        //
        // 实测(2026-09-06 生产快照):3 个账户共 6 笔订单挂在
        // `NOSUCH0000` / `NOSUCH0` / `NOPE9999` 上,状态永远是 SUBMITTED。
        // 其中 `ACC_8cff51bb` 的 `frozen_margin = 400.00` 就是这么卡住的。
        //
        // 一笔从未进过撮合引擎的订单,**撤销它本来就只需要本地清理** ——
        // 这里合成一个 `Success::Cancelled` 交给既有的 `handle_success_result`,
        // 复用它那条已经验证过的「更新状态 + 释放冻结」路径,不另写一套。
        let matching_engine_order_id = match info.matching_engine_order_id {
            Some(id) => id,
            None => {
                let order = info.order.clone();
                drop(info);
                drop(order_info);
                log::warn!(
                    "Order {} has no matching-engine id (instrument {} likely not registered); \
                     cancelling locally and releasing frozen funds",
                    req.order_id,
                    order.instrument_id
                );
                let synthetic = crate::matching::Success::Cancelled {
                    id: 0,
                    ts: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                };
                self.handle_success_result(&req.order_id, &order, synthetic, true)?;
                return Ok(());
            }
        };

        let instrument_id = info.order.instrument_id.clone();
        let direction_str = info.order.direction.clone();
        // ✨ 保存订单信息用于后续处理 @yutiansut @quantaxis
        let order = info.order.clone();

        // 释放写锁，避免在调用撮合引擎时持有锁
        drop(info);
        drop(order_info);

        // 转换订单方向
        let direction = match direction_str.as_str() {
            "BUY" => OrderDirection::BUY,
            "SELL" => OrderDirection::SELL,
            _ => {
                return Err(ExchangeError::OrderError(format!(
                    "Invalid direction: {}",
                    direction_str
                )))
            }
        };

        // 创建撤单请求
        let _asset = InstrumentAsset::from_code(&instrument_id);
        let cancel_request = crate::matching::OrderRequest::CancelOrder {
            id: matching_engine_order_id,
            direction,
        };

        // 获取订单簿
        let orderbook = self
            .matching_engine
            .get_orderbook(&instrument_id)
            .ok_or_else(|| {
                ExchangeError::MatchingError(format!(
                    "Orderbook not found for instrument: {}",
                    instrument_id
                ))
            })?;

        // 提交撤单请求到撮合引擎
        let mut ob = orderbook.write();
        let results = ob
            .process_order(cancel_request)
            .into_iter()
            .collect::<Vec<_>>();
        drop(ob);

        // 处理撤单结果
        // ✨ 修复：必须调用 handle_success_result 来处理 Success::Cancelled 事件
        // 这样才能触发 handle_cancel_accepted_new 释放冻结资金 @yutiansut @quantaxis
        for result in results {
            match result {
                Ok(success) => {
                    log::info!("Cancel order success: {:?}", success);
                    // ✨ 调用 handle_success_result 处理撤单成功事件
                    // 这会触发 Success::Cancelled 分支，更新订单状态并释放冻结资金
                    // 撤单不涉及成交记录，is_taker 参数无影响
                    if let Err(e) = self.handle_success_result(&req.order_id, &order, success, true) {
                        log::error!("Failed to handle cancel success result: {:?}", e);
                    }
                }
                Err(failed) => {
                    log::error!("Cancel order failed: {:?}", failed);
                    return Err(ExchangeError::MatchingError(format!(
                        "Cancel order failed: {:?}",
                        failed
                    )));
                }
            }
        }

        log::info!("Order cancelled from matching engine: {}", req.order_id);
        Ok(())
    }

    /// IOC/FOK 处理：检查订单是否满足 FOK 条件
    /// @yutiansut @quantaxis
    ///
    /// FOK (Fill or Kill) 要求订单必须全部成交，否则全部撤销
    /// 此方法在订单提交前检查订单簿是否有足够的对手方挂单
    fn check_fok_fulfillable(
        &self,
        instrument_id: &str,
        direction: &str,
        volume: f64,
        price: f64,
    ) -> bool {
        // 获取订单簿
        let orderbook = match self.matching_engine.get_orderbook(instrument_id) {
            Some(ob) => ob,
            None => {
                log::warn!("[FOK] Orderbook not found for {}", instrument_id);
                return false;
            }
        };

        let ob = orderbook.read();

        // 计算对手方可成交量
        let available_volume = match direction {
            "BUY" => {
                // 买单需要检查卖单挂单量（价格 <= price）
                if let Some(asks) = ob.ask_queue.get_sorted_orders() {
                    asks.iter()
                        .filter(|o| o.price <= price)
                        .map(|o| o.volume)
                        .sum()
                } else {
                    0.0
                }
            }
            "SELL" => {
                // 卖单需要检查买单挂单量（价格 >= price）
                if let Some(bids) = ob.bid_queue.get_sorted_orders() {
                    bids.iter()
                        .filter(|o| o.price >= price)
                        .map(|o| o.volume)
                        .sum()
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };

        let fulfillable = available_volume >= volume;
        log::debug!(
            "[FOK] Check: instrument={}, direction={}, volume={}, price={}, available={}, fulfillable={}",
            instrument_id, direction, volume, price, available_volume, fulfillable
        );

        fulfillable
    }

    /// IOC/FOK 处理：在匹配后处理 IOC 剩余订单
    /// @yutiansut @quantaxis
    ///
    /// IOC (Immediate or Cancel) 要求立即成交可成交部分，剩余部分撤销
    /// 此方法在订单匹配完成后，检查并撤销未成交部分
    fn handle_ioc_remaining(&self, order_id: &str, account_id: &str) {
        // 获取订单信息
        let order_info = match self.orders.get(order_id) {
            Some(info) => info,
            None => {
                log::warn!("[IOC] Order not found: {}", order_id);
                return;
            }
        };

        let (status, time_condition, filled_volume, original_volume) = {
            let info = order_info.read();
            (
                info.status,
                info.time_condition,
                info.filled_volume,
                info.order.volume_orign,
            )
        };

        // 只处理 IOC 订单
        if time_condition != TimeCondition::IOC {
            return;
        }

        // 如果已完全成交或已撤销，不需要处理
        if matches!(status, OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected) {
            log::debug!("[IOC] Order {} already in final status: {:?}", order_id, status);
            return;
        }

        // 如果有未成交部分，自动撤销
        let remaining_volume = original_volume - filled_volume;
        if remaining_volume > 0.001 {  // 使用小数容差
            log::info!(
                "[IOC] Auto-cancelling remaining {} of order {} (filled: {}/{})",
                remaining_volume, order_id, filled_volume, original_volume
            );

            // 发送撤单请求
            let cancel_req = CancelOrderRequest {
                account_id: account_id.to_string(),
                order_id: order_id.to_string(),
            };

            match self.cancel_order(cancel_req) {
                Ok(_) => {
                    log::info!("[IOC] Successfully cancelled remaining order: {}", order_id);
                }
                Err(e) => {
                    log::error!("[IOC] Failed to cancel remaining order {}: {:?}", order_id, e);
                }
            }
        } else {
            log::debug!("[IOC] Order {} fully filled, no cancellation needed", order_id);
        }
    }

    /// 查询订单
    pub fn query_order(&self, order_id: &str) -> Option<Order> {
        self.orders
            .get(order_id)
            .map(|info| info.read().order.clone())
    }

    /// 查询用户所有订单
    pub fn query_user_orders(&self, user_id: &str) -> Vec<Order> {
        if let Some(order_ids) = self.user_orders.get(user_id) {
            order_ids
                .read()
                .iter()
                .filter_map(|order_id| self.query_order(order_id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取订单状态
    pub fn get_order_status(&self, order_id: &str) -> Option<OrderStatus> {
        self.orders.get(order_id).map(|info| info.read().status)
    }

    /// 更新订单状态 (由 TradeGateway 调用)
    pub fn update_order_status(
        &self,
        order_id: &str,
        status: OrderStatus,
    ) -> Result<(), ExchangeError> {
        let order_info = self
            .orders
            .get(order_id)
            .ok_or_else(|| ExchangeError::OrderError(format!("Order not found: {}", order_id)))?;

        let mut info = order_info.write();
        info.status = status;
        info.update_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // 如果订单完成，从风控追踪中移除
        if matches!(
            status,
            OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected
        ) {
            self.risk_checker
                .remove_active_order(&info.order.user_id, order_id);
        }

        Ok(())
    }

    /// 生成订单ID
    fn generate_order_id(&self) -> String {
        let seq = self.order_seq.fetch_add(1, Ordering::SeqCst);
        let timestamp = chrono::Utc::now().timestamp_millis();
        format!("O{}{:010}", timestamp, seq)
    }

    /// 获取市价单的执行价格 @yutiansut @quantaxis
    ///
    /// 买单：使用卖一价（ask_price）
    /// 卖单：使用买一价（bid_price）
    /// 如果没有对手盘，使用 last_price 或结算价
    fn get_market_price_for_order(&self, instrument_id: &str, direction: &str) -> f64 {
        // 1. 尝试从订单簿获取对手盘价格
        if let Some(orderbook) = self.matching_engine.get_orderbook(instrument_id) {
            let ob = orderbook.read();

            let price = match direction {
                "BUY" => {
                    // 买单用卖一价（ask）
                    ob.ask_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0)
                }
                "SELL" => {
                    // 卖单用买一价（bid）
                    ob.bid_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0)
                }
                _ => 0.0,
            };

            if price > 0.0 {
                return price;
            }

            // 2. 没有对手盘，使用 last_price（订单簿初始化时设置为 prev_close）
            if ob.lastprice > 0.0 {
                return ob.lastprice;
            }
        }

        // 3. 无行情可用
        0.0
    }
    /// direction 归一化 —— 与 `normalize_offset` 对称 @yutiansut @quantaxis
    ///
    /// ⚠️ 此前**完全没有校验**。`calculate_towards` 的兜底注释写着「已是不可达路径」,
    /// 那只对 offset 成立(offset 有 normalize_offset 把关),direction 从来没人管。
    ///
    /// 2026-09-04 实测(同一账户连发 4 单,价格远离市价故会挂住):
    ///   direction="buy"   → 200 接受,回查记为 **BUY**
    ///   direction="Buy"   → 200 接受,回查记为 **BUY**
    ///   direction="BOGUS" → **200 接受**,回查记为 **BUY**
    /// 四张单都真挂到了买盘上(随后一张 SELL 被自成交防范拦住,正好反证它们是活单)。
    /// **direction 里塞任何垃圾都会静默变成买单。**
    ///
    /// 大小写不敏感(客户端写 "buy" 是合理输入,归一化比拒绝友好);
    /// 其余一律返回 None,由调用方拒单 —— **不得**回退到任何默认方向。
    fn normalize_direction(direction: &str) -> Option<&'static str> {
        match direction.trim().to_ascii_uppercase().as_str() {
            "BUY" => Some("BUY"),
            "SELL" => Some("SELL"),
            _ => None,
        }
    }


    /// 开平标志归一化 —— 把各处的写法收敛到一套规范值
    ///
    /// 项目里同一个概念有三套拼写(实测 2026-09-04):
    ///   前端表单 + `protocol/diff/types.rs:439`  → `CLOSE_TODAY` / `CLOSE_YESTERDAY`
    ///   `order_router::calculate_towards`        → `CLOSETODAY`(无平昨分支)
    ///   `risk/pre_trade_check.rs:348`            → `CLOSETODAY` / `CLOSEYESTERDAY`
    /// 这里统一收到**无下划线**的规范形(与 pre_trade_check 一致),
    /// 同时接受带下划线的写法以兼容既有前端。
    /// 返回 `None` 表示非法 —— 调用方必须拒单,**不得**回退到任何默认方向。
    /// @yutiansut @quantaxis
    fn normalize_offset(offset: &str) -> Option<&'static str> {
        match offset.trim().to_ascii_uppercase().as_str() {
            "OPEN" => Some("OPEN"),
            "CLOSE" => Some("CLOSE"),
            "CLOSETODAY" | "CLOSE_TODAY" => Some("CLOSETODAY"),
            "CLOSEYESTERDAY" | "CLOSE_YESTERDAY" => Some("CLOSEYESTERDAY"),
            _ => None,
        }
    }

    /// 计算 towards (买卖方向 - 遵循 qars 定义)
    ///
    /// ⚠️ **不设默认分支**。原来是 `_ => 2`(默认买开)——
    /// 任何拼写不对的 offset 都会被静默做成买入开仓,方向直接做反。
    /// 入口 `submit_order_with_options` 已用 `normalize_offset` 把 offset
    /// 收敛并拒掉非法值,这里只需覆盖四个规范值;真出现意外组合时
    /// 记 error 日志并返回买开(保持函数签名不变),但那已是不可达路径。
    fn calculate_towards(&self, direction: &str, offset: &str) -> i32 {
        match (direction, offset) {
            ("BUY", "OPEN") => 2,    // 买开 = 2 (qars 标准)
            ("SELL", "OPEN") => -2,  // 卖开 = -2
            ("BUY", "CLOSE") => 3,   // 买平 = 3
            ("SELL", "CLOSE") => -3, // 卖平 = -3
            ("BUY", "CLOSEYESTERDAY") => 3,   // 平昨 = 普通平仓(qars 平昨优先)
            ("SELL", "CLOSEYESTERDAY") => -3,
            ("BUY", "CLOSETODAY") => 4,
            ("SELL", "CLOSETODAY") => -4,
            _ => {
                log::error!(
                    "calculate_towards 收到未归一化的组合 direction={:?} offset={:?} —— \
                     入口归一化被绕过,请检查调用路径",
                    direction, offset
                );
                2
            }
        }
    }

    /// 获取活动订单数量
    pub fn get_active_order_count(&self) -> usize {
        self.orders
            .iter()
            .filter(|entry| {
                let status = entry.value().read().status;
                matches!(
                    status,
                    OrderStatus::Submitted | OrderStatus::PartiallyFilled
                )
            })
            .count()
    }

    /// 获取风控检查器引用
    pub fn get_risk_checker(&self) -> Arc<PreTradeCheck> {
        self.risk_checker.clone()
    }

    /// 更新成交统计
    fn update_trade_stats(&self, price: f64, volume: f64) {
        self.trade_count.fetch_add(1, Ordering::SeqCst);
        *self.trade_volume.write() += volume;
        *self.trade_amount.write() += price * volume;
    }

    /// 持久化Tick数据到WAL
    fn persist_tick_data(
        &self,
        instrument_id: &str,
        price: f64,
        volume: f64,
    ) -> Result<(), ExchangeError> {
        if let Some(ref _storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            // 获取订单簿中的买卖价
            let (bid_price, ask_price) =
                if let Some(orderbook) = self.matching_engine.get_orderbook(instrument_id) {
                    let ob = orderbook.read();
                    let bid = ob
                        .bid_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0);
                    let ask = ob
                        .ask_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0);
                    (bid, ask)
                } else {
                    (0.0, 0.0)
                };

            // 创建TickData记录
            let tick_record = WalRecord::TickData {
                instrument_id: WalRecord::to_fixed_array_16(instrument_id),
                last_price: price,
                bid_price,
                ask_price,
                volume: volume as i64,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            };

            // ========== 性能优化：批量写入缓冲 ==========
            // 将tick数据写入缓冲区，由异步线程定期刷新（10ms间隔）
            self.tick_buffer.lock().push(tick_record);
            log::trace!(
                "Buffered tick data for {} (buffer size: {})",
                instrument_id,
                self.tick_buffer.lock().len()
            );
        }

        Ok(())
    }

    /// 持久化订单簿tick数据到WAL（订单挂入/撤销时调用，不更新last_price）
    ///
    /// 与 persist_tick_data 的区别：
    /// - persist_tick_data: 成交时调用，更新 last_price + bid/ask
    /// - persist_orderbook_tick: 订单簿变化时调用，只更新 bid/ask，保持 last_price 不变
    fn persist_orderbook_tick(&self, instrument_id: &str) -> Result<(), ExchangeError> {
        if let Some(ref _storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            // 获取订单簿中的买卖价
            let (bid_price, ask_price, last_price) =
                if let Some(orderbook) = self.matching_engine.get_orderbook(instrument_id) {
                    let ob = orderbook.read();
                    let bid = ob
                        .bid_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0);
                    let ask = ob
                        .ask_queue
                        .get_sorted_orders()
                        .and_then(|orders| orders.first().map(|o| o.price))
                        .unwrap_or(0.0);

                    // 尝试获取最后成交价（从订单簿的lastprice字段，或使用中间价）
                    let last = if ob.lastprice > 0.0 {
                        ob.lastprice
                    } else if bid > 0.0 && ask > 0.0 {
                        (bid + ask) / 2.0
                    } else if bid > 0.0 {
                        bid
                    } else if ask > 0.0 {
                        ask
                    } else {
                        0.0
                    };

                    (bid, ask, last)
                } else {
                    (0.0, 0.0, 0.0)
                };

            // 创建TickData记录（volume=0表示订单簿变化，非成交）
            let tick_record = WalRecord::TickData {
                instrument_id: WalRecord::to_fixed_array_16(instrument_id),
                last_price, // 保持上次成交价不变
                bid_price,
                ask_price,
                volume: 0, // 0表示订单簿变化，非成交tick
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            };

            // ========== 性能优化：批量写入缓冲 ==========
            // 将订单簿tick数据写入缓冲区，由异步线程定期刷新
            self.tick_buffer.lock().push(tick_record);
            log::trace!(
                "Buffered orderbook tick for {} (buffer size: {})",
                instrument_id,
                self.tick_buffer.lock().len()
            );
        }

        Ok(())
    }

    /// 持久化订单簿快照到WAL
    fn persist_orderbook_snapshot(&self, instrument_id: &str) -> Result<(), ExchangeError> {
        // ========== 性能优化：快照频率控制 ==========
        // 限流：最多每秒1次快照（防止高频写入）
        let now = Instant::now();
        if let Some(last_time) = self.last_snapshot_time.get(instrument_id) {
            if now.duration_since(*last_time) < self.snapshot_interval {
                // 跳过此次快照（距离上次快照时间太短）
                log::trace!(
                    "Skipping snapshot for {} (last snapshot: {:?} ago)",
                    instrument_id,
                    now.duration_since(*last_time)
                );
                return Ok(());
            }
        }

        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            // 获取订单簿快照
            // ⚠️ 「10档」必须是 10 个**价位**,不是前 10 张**订单** @yutiansut @quantaxis
            //    原写法 `get_sorted_orders().take(10)` 取的是逐笔订单:同价位重复出现、
            //    档数不足,行情接口的 WAL 回读路径据此给出 `1896.0×1, 1896.0×1, …`。
            //    聚合统一走 ExchangeMatchingEngine::price_levels(与行情 L3 同一实现)。
            if let Some((bids, asks, ob_last)) = self.matching_engine.price_levels(instrument_id, 10) {
                let bids: Vec<(f64, i64)> = bids.into_iter().map(|(p, v)| (p, v as i64)).collect();
                let asks: Vec<(f64, i64)> = asks.into_iter().map(|(p, v)| (p, v as i64)).collect();

                // 创建OrderBookSnapshot记录（10档，不足的用 (0.0, 0) 填充）
                let mut bids_array = [(0.0, 0i64); 10];
                let mut asks_array = [(0.0, 0i64); 10];

                for (i, (price, volume)) in bids.iter().enumerate() {
                    if i >= 10 {
                        break;
                    }
                    bids_array[i] = (*price, *volume);
                }

                for (i, (price, volume)) in asks.iter().enumerate() {
                    if i >= 10 {
                        break;
                    }
                    asks_array[i] = (*price, *volume);
                }

                // 最新价:用撮合簿的真实最新成交价;没有成交过才退回买一/卖一
                // (原实现直接拿买一当「最新价」写进记录)
                let last_price = if ob_last.is_finite() && ob_last > 0.0 {
                    ob_last
                } else {
                    bids.first()
                        .map(|(p, _)| *p)
                        .or_else(|| asks.first().map(|(p, _)| *p))
                        .unwrap_or(0.0)
                };

                let snapshot_record = WalRecord::OrderBookSnapshot {
                    instrument_id: WalRecord::to_fixed_array_16(instrument_id),
                    bids: bids_array,
                    asks: asks_array,
                    last_price,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                };

                // 写入WAL
                if let Err(e) = storage.write(snapshot_record) {
                    log::warn!("Failed to persist orderbook snapshot to WAL: {}", e);
                    // 不影响交易流程，只记录警告
                } else {
                    // 更新快照时间
                    self.last_snapshot_time
                        .insert(instrument_id.to_string(), now);
                    log::debug!(
                        "Persisted orderbook snapshot for {}: {} bids, {} asks",
                        instrument_id,
                        bids.len(),
                        asks.len()
                    );
                }
            }
        }

        Ok(())
    }

    /// 获取订单统计
    pub fn get_order_statistics(&self) -> OrderStatistics {
        let mut total_count = 0;
        let mut pending_count = 0;
        let mut filled_count = 0;
        let mut cancelled_count = 0;
        let mut rejected_count = 0;

        for entry in self.orders.iter() {
            total_count += 1;
            let status = entry.value().read().status;
            match status {
                OrderStatus::Submitted | OrderStatus::PartiallyFilled => pending_count += 1,
                OrderStatus::Filled => filled_count += 1,
                OrderStatus::Cancelled => cancelled_count += 1,
                OrderStatus::Rejected => rejected_count += 1,
                _ => {}
            }
        }

        OrderStatistics {
            total_count,
            pending_count,
            filled_count,
            cancelled_count,
            rejected_count,
        }
    }

    /// 获取成交统计
    pub fn get_trade_statistics(&self) -> TradeStatistics {
        TradeStatistics {
            total_count: self.trade_count.load(Ordering::SeqCst),
            total_volume: *self.trade_volume.read(),
            total_amount: *self.trade_amount.read(),
        }
    }

    // ========== 性能优化：批量刷新线程 ==========

    /// 启动批量刷新线程（异步定期刷新tick缓冲区）
    ///
    /// 性能优势：
    /// - 将多个单次写入合并为一次批量写入
    /// - 10ms刷新间隔，平衡延迟和吞吐量
    /// - 批量大小自适应（最多1000条/批）
    pub fn start_batch_flush_worker(&self) {
        if let Some(ref storage) = self.storage {
            let storage = storage.clone();
            let tick_buffer = self.tick_buffer.clone();
            let stop_signal = self.flush_stop_signal.clone();

            // 重置停止信号
            stop_signal.store(false, Ordering::SeqCst);

            // 启动后台刷新线程
            std::thread::spawn(move || {
                log::info!("Batch flush worker started (interval: 10ms, max_batch: 1000)");

                loop {
                    // 检查停止信号
                    if stop_signal.load(Ordering::SeqCst) {
                        log::info!("Batch flush worker received stop signal, exiting...");
                        break;
                    }

                    // 睡眠10ms
                    std::thread::sleep(Duration::from_millis(10));

                    // 从缓冲区取出所有记录
                    let mut buffer = tick_buffer.lock();
                    if buffer.is_empty() {
                        drop(buffer); // 尽早释放锁
                        continue;
                    }

                    // 取出缓冲区数据（最多1000条）
                    let batch_size = buffer.len().min(1000);
                    let batch: Vec<_> = buffer.drain(..batch_size).collect();
                    drop(buffer); // 释放锁

                    // 批量写入WAL
                    match storage.write_batch(batch.clone()) {
                        Ok(sequences) => {
                            log::debug!(
                                "Batch flushed {} tick records to WAL (seq: {} - {})",
                                batch.len(),
                                sequences.first().unwrap_or(&0),
                                sequences.last().unwrap_or(&0)
                            );
                        }
                        Err(e) => {
                            log::error!("Batch flush failed: {}, retrying...", e);
                            // 写入失败，重新放回缓冲区
                            let mut buffer = tick_buffer.lock();
                            for record in batch.into_iter().rev() {
                                buffer.insert(0, record);
                            }
                        }
                    }
                }

                // 线程退出前，刷新剩余数据
                let mut buffer = tick_buffer.lock();
                if !buffer.is_empty() {
                    let remaining: Vec<_> = buffer.drain(..).collect();
                    drop(buffer);
                    if let Err(e) = storage.write_batch(remaining.clone()) {
                        log::error!(
                            "Failed to flush remaining {} records on shutdown: {}",
                            remaining.len(),
                            e
                        );
                    } else {
                        log::info!("Flushed remaining {} records on shutdown", remaining.len());
                    }
                }

                log::info!("Batch flush worker stopped");
            });
        } else {
            log::warn!("Cannot start batch flush worker: storage not set");
        }
    }

    /// 停止批量刷新线程
    pub fn stop_batch_flush_worker(&self) {
        log::info!("Stopping batch flush worker...");
        self.flush_stop_signal.store(true, Ordering::SeqCst);
        // 等待线程退出（最多1秒）
        std::thread::sleep(Duration::from_millis(100));
    }

    /// 获取优先级队列统计信息
    pub fn get_priority_queue_stats(&self) -> Option<crate::exchange::PriorityQueueStatistics> {
        self.priority_queue.as_ref().map(|q| q.get_statistics())
    }

    /// 获取订单详细信息（包含时间戳和成交量）
    pub fn get_order_detail(&self, order_id: &str) -> Option<(Order, OrderStatus, i64, i64, f64)> {
        self.orders.get(order_id).map(|info| {
            let i = info.read();
            (
                i.order.clone(),
                i.status,
                i.submit_time,
                i.update_time,
                i.filled_volume,
            )
        })
    }

    /// 获取用户所有订单的详细信息 (order_id, order, status, submit_time, update_time, filled_volume)
    pub fn get_user_order_details(&self, user_id: &str) -> Vec<UserOrderDetail> {
        if let Some(order_ids) = self.user_orders.get(user_id) {
            order_ids
                .read()
                .iter()
                .filter_map(|order_id| {
                    self.orders.get(order_id).map(|info| {
                        let i = info.read();
                        UserOrderDetail {
                            order_id: order_id.clone(),
                            qa_order_id: i.qa_order_id.clone(),
                            order: i.order.clone(),
                            status: i.status,
                            submit_time: i.submit_time,
                            update_time: i.update_time,
                            filled_volume: i.filled_volume,
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取所有订单的详细信息 (管理端)
    /// @yutiansut @quantaxis
    pub fn get_all_orders(&self) -> Vec<(String, Order, OrderStatus, i64, i64, f64)> {
        self.orders
            .iter()
            .map(|entry| {
                let order_id = entry.key().clone();
                let info = entry.value().read();
                (
                    order_id,
                    info.order.clone(),
                    info.status,
                    info.submit_time,
                    info.update_time,
                    info.filled_volume,
                )
            })
            .collect()
    }

    /// 获取订单总数
    pub fn get_order_count(&self) -> usize {
        self.orders.len()
    }

    /// 从账户的 dailyorders 恢复订单索引
    /// 在服务器重启后调用，从账户快照中恢复待处理订单到 order_router
    /// ✨ 修复：同时将订单重新提交到撮合引擎订单簿，以支持撤单操作 @yutiansut @quantaxis
    pub fn restore_orders_from_accounts(&self) {
        let accounts = self.account_mgr.get_all_accounts();
        let mut restored_count = 0;
        let mut orderbook_restored_count = 0;

        for account_arc in accounts {
            // ⚠️ **先把待恢复订单克隆出来并立刻释放账户读锁**,再去碰 orderbook。
            //    @yutiansut @quantaxis
            //
            // 原写法在整个内层循环期间持有 `account_arc.read()`,而循环体内会取
            // `orderbook.write()`(见下方 `let mut ob = orderbook.write()`)——
            // 锁序是 **account.read() → orderbook.write()**。
            // 而**全仓的既定不变量是 orderbook 锁 → account 锁**(撮合/撤单路径都是
            // 这个方向)—— 见 `risk/risk_monitor.rs:369-383` 的注释,那里记录了完全同型的
            // 一次事故:`do_risk_check` 曾在 `account.write()` 内调 `get_last_price()`
            // (→`orderbook.read()`),构成反向边,实测分别在 60 秒 / 9 分钟 / 26.5 分钟挂死。
            // 本函数是该不变量**最后一处未修的违反点**。
            //
            // 实测(2026-09-05 03:46):部署后启动挂死 —— 14 个线程全部
            // `futex_wait_queue_me`、无一在干活,8094/8095 未监听,
            // 日志冻结在恢复第 216 条订单处;kill 后重启即正常(755 条全恢复)
            // ⇒ 是**间歇性竞态**,意味着系统有概率启动不起来。
            //
            // 修法:锁范围收窄到只做数据克隆。`dailyorders` 的值是 QIFI Order(可 Clone),
            // 克隆 361 条活单的代价可忽略,换来的是恢复期间**不再跨锁**。
            let (account_id, pending): (String, Vec<(String, _)>) = {
                let account = account_arc.read();
                let id = account.account_cookie.clone();
                let list = account
                    .dailyorders
                    .iter()
                    .filter(|(_, o)| o.status == "SUBMITTED" || o.status == "ALIVE")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                (id, list)
            }; // ← 账户读锁在此释放

            for (qa_id, order) in &pending {
                // ✨ 以快照里的交易所单号恢复(下单时写入的 exchange_order_id);
                //    老快照该字段为空,退回 qars 键。qa_order_id 仍是 qars 键(账户操作要用)。
                //    @yutiansut @quantaxis
                let order_id: &String = if order.exchange_order_id.is_empty() {
                    qa_id
                } else {
                    &order.exchange_order_id
                };
                // 只恢复待处理订单 (SUBMITTED/ALIVE)
                if order.status != "SUBMITTED" && order.status != "ALIVE" {
                    continue;
                }

                // 检查是否已经存在
                if self.orders.contains_key(order_id) {
                    continue;
                }

                // 解析订单状态
                let status = match order.status.as_str() {
                    "SUBMITTED" => OrderStatus::Submitted,
                    "ALIVE" => OrderStatus::PartiallyFilled,
                    _ => continue,
                };

                // 计算已成交量和剩余量
                let filled_volume = order.volume_orign - order.volume_left;
                let remaining_volume = order.volume_left;

                // ✨ 关键修复：将订单重新提交到撮合引擎订单簿 @yutiansut @quantaxis
                let mut matching_engine_order_id: Option<u64> = None;

                // 只有剩余数量 > 0 的订单才需要恢复到订单簿
                if remaining_volume > 0.0 {
                    if let Some(orderbook) = self.matching_engine.get_orderbook(&order.instrument_id) {
                        // 转换订单方向
                        let direction = match order.direction.as_str() {
                            "BUY" => OrderDirection::BUY,
                            "SELL" => OrderDirection::SELL,
                            _ => {
                                log::warn!("⚠️ Invalid direction for order {}: {}", order_id, order.direction);
                                continue;
                            }
                        };

                        // 创建撮合订单请求（使用剩余量，不是原始量）
                        let asset = InstrumentAsset::from_code(&order.instrument_id);
                        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

                        let match_request = crate::matching::orders::new_limit_order_request(
                            asset,
                            direction,
                            order.limit_price,
                            remaining_volume,  // 使用剩余量
                            timestamp,
                        );

                        // 提交到订单簿
                        let mut ob = orderbook.write();
                        let results: Vec<_> = ob.process_order(match_request).into_iter().collect();
                        drop(ob);

                        // 从结果中获取 matching_engine_order_id
                        for result in results {
                            if let Ok(Success::Accepted { id, .. }) = result {
                                matching_engine_order_id = Some(id);

                                // ✨ 存储反向映射 @yutiansut @quantaxis
                                self.engine_id_to_order
                                    .insert((order.instrument_id.clone(), id), order_id.clone());
                                self.engine_id_to_user
                                    .insert((order.instrument_id.clone(), id), order.user_id.clone());

                                orderbook_restored_count += 1;
                                log::info!(
                                    "📚 Restored order to orderbook: order_id={}, engine_id={}, instrument={}, volume={}",
                                    order_id, id, order.instrument_id, remaining_volume
                                );
                                break;
                            }
                        }
                    } else {
                        log::warn!(
                            "⚠️ Orderbook not found for instrument {}, order {} cannot be restored to orderbook",
                            order.instrument_id, order_id
                        );
                    }
                }

                // 创建 OrderRouteInfo
                let info = OrderRouteInfo {
                    order: order.clone(),
                    status,
                    // ⚠️ 必须保持**纳秒**,与实时下单路径(:912 `timestamp_nanos_opt`)一致
                    // @yutiansut @quantaxis
                    //
                    // 原先这里 `/ 1_000_000_000` 转成了秒,于是同一个
                    // `OrderInfo.submit_time` 字段有两种单位:
                    //   · 实时下单  → 1788528083266660725(纳秒)
                    //   · 重启恢复  → 1788515691(秒)
                    // 前端 `orders/index.vue:83` 的注释明写「submit_time 是纳秒」并按纳秒
                    // 格式化 —— 重启后所有恢复出来的订单在界面上会显示成 **1970 年**。
                    // 任何排序、日期筛选也会对其中一种算错。
                    submit_time: order.insert_date_time,
                    update_time: chrono::Utc::now()
                        .timestamp_nanos_opt()
                        .unwrap_or(order.insert_date_time),
                    filled_volume,
                    qa_order_id: qa_id.clone(),
                    matching_engine_order_id, // ✨ 现在有值了！
                    time_condition: TimeCondition::GFD,
                    volume_condition: VolumeCondition::ANY,
                };

                // 添加到订单映射
                self.orders
                    .insert(order_id.clone(), Arc::new(RwLock::new(info)));

                // 更新用户订单索引 (注意：QIFI 的 user_id 实际上是 account_id)
                self.user_orders
                    .entry(account_id.clone())
                    .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
                    .write()
                    .push(order_id.clone());

                restored_count += 1;
                log::info!(
                    "🔄 Restored order: account={}, order_id={}, instrument={}, status={:?}, volume_left={}, engine_id={:?}",
                    account_id,
                    order_id,
                    order.instrument_id,
                    status,
                    order.volume_left,
                    matching_engine_order_id
                );
            }
        }

        if restored_count > 0 {
            log::info!(
                "✅ Restored {} pending orders from account snapshots ({} restored to orderbook)",
                restored_count,
                orderbook_restored_count
            );
        } else {
            log::debug!("No pending orders to restore from accounts");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{AccountType, OpenAccountRequest};
    use crate::exchange::instrument_registry::InstrumentInfo;

    pub(super) fn create_test_router() -> OrderRouter {
        // 创建账户管理器
        let account_mgr = Arc::new(AccountManager::new());
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: Some("test_user".to_string()), // 使用固定ID以便测试
            account_name: "Test User".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        let account_id = account_mgr.open_account(req).unwrap();
        assert_eq!(account_id, "test_user"); // 验证账户ID正确

        // 创建撮合引擎
        let matching_engine = Arc::new(ExchangeMatchingEngine::new());
        matching_engine
            .register_instrument("IX2301".to_string(), 120.0)
            .unwrap();

        // 创建合约注册表
        let instrument_registry = Arc::new(InstrumentRegistry::new());
        instrument_registry
            .register(InstrumentInfo {
                instrument_id: "IX2301".to_string(),
                instrument_name: "IX2301".to_string(),
                instrument_type:
                    crate::exchange::instrument_registry::InstrumentType::CommodityFuture,
                exchange: "SHFE".to_string(),
                contract_multiplier: 1,
                price_tick: 0.01,
                margin_rate: 0.1,
                commission_rate: 0.0005,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: crate::exchange::instrument_registry::InstrumentStatus::Active,
                list_date: Some("2023-01-01".to_string()),
                expire_date: Some("2023-12-31".to_string()),
                created_at: "2023-01-01T00:00:00Z".to_string(),
                updated_at: "2023-01-01T00:00:00Z".to_string(),
            })
            .unwrap();

        // 创建成交回报网关
        // ⚠️ 测试必须显式给 wal_root,否则落到**仓库根目录**的 `./data/wal/`
        //    @yutiansut @quantaxis
        //
        // `TradeGateway::new` 的默认 wal_root 是 `"./data/wal"`(trade_gateway.rs:173)
        // —— **相对当前工作目录**,而 `cargo test` 的 CWD 就是仓库根。
        // 实测:跑 `cargo test --lib exchange::order_router` 会在
        // `data/wal/__ACCOUNT__/test_user/`、`data/wal/IX2301/` 写真实 WAL 文件
        // (IX2301 是本测试夹具里的合约名,一眼可辨)。
        // 该目录里还躺着 2026-09-03 遗留的 **554 MB**。
        //
        // 同文件族的 `trade_gateway.rs:1995/2909/3195` 早就用
        // `./output/testexchange/wal*`,只有这里漏了。照抄它。
        let trade_gateway =
            Arc::new(TradeGateway::new(account_mgr.clone())
                .with_wal_root("./output/testexchange/wal_order_router"));

        OrderRouter::new(
            account_mgr,
            matching_engine,
            instrument_registry,
            trade_gateway,
        )
    }

    // ==================== TimeCondition 测试 @yutiansut @quantaxis ====================

    /// 测试 TimeCondition Default trait
    #[test]
    fn test_time_condition_default() {
        let tc: TimeCondition = TimeCondition::default();
        assert_eq!(tc, TimeCondition::GFD);
    }

    /// 测试 TimeCondition 所有枚举值
    #[test]
    fn test_time_condition_values() {
        assert_eq!(TimeCondition::IOC.to_string(), "IOC");
        assert_eq!(TimeCondition::GFS.to_string(), "GFS");
        assert_eq!(TimeCondition::GFD.to_string(), "GFD");
        assert_eq!(TimeCondition::GTD.to_string(), "GTD");
        assert_eq!(TimeCondition::GTC.to_string(), "GTC");
        assert_eq!(TimeCondition::GFA.to_string(), "GFA");
    }

    /// 测试 TimeCondition from_str
    #[test]
    fn test_time_condition_from_str() {
        assert_eq!(TimeCondition::from_str("IOC"), TimeCondition::IOC);
        assert_eq!(TimeCondition::from_str("GFS"), TimeCondition::GFS);
        assert_eq!(TimeCondition::from_str("GFD"), TimeCondition::GFD);
        assert_eq!(TimeCondition::from_str("GTD"), TimeCondition::GTD);
        assert_eq!(TimeCondition::from_str("GTC"), TimeCondition::GTC);
        assert_eq!(TimeCondition::from_str("GFA"), TimeCondition::GFA);
        // 小写
        assert_eq!(TimeCondition::from_str("ioc"), TimeCondition::IOC);
        // 未知值默认 GFD
        assert_eq!(TimeCondition::from_str("UNKNOWN"), TimeCondition::GFD);
    }

    // ==================== VolumeCondition 测试 @yutiansut @quantaxis ====================

    /// 测试 VolumeCondition Default trait
    #[test]
    fn test_volume_condition_default() {
        let vc: VolumeCondition = VolumeCondition::default();
        assert_eq!(vc, VolumeCondition::ANY);
    }

    /// 测试 VolumeCondition 所有枚举值
    #[test]
    fn test_volume_condition_values() {
        assert_eq!(VolumeCondition::ANY.to_string(), "ANY");
        assert_eq!(VolumeCondition::MIN.to_string(), "MIN");
        assert_eq!(VolumeCondition::ALL.to_string(), "ALL");
    }

    /// 测试 VolumeCondition from_str
    #[test]
    fn test_volume_condition_from_str() {
        assert_eq!(VolumeCondition::from_str("ANY"), VolumeCondition::ANY);
        assert_eq!(VolumeCondition::from_str("MIN"), VolumeCondition::MIN);
        assert_eq!(VolumeCondition::from_str("ALL"), VolumeCondition::ALL);
        // 小写
        assert_eq!(VolumeCondition::from_str("all"), VolumeCondition::ALL);
        // 未知值默认 ANY
        assert_eq!(VolumeCondition::from_str("UNKNOWN"), VolumeCondition::ANY);
    }

    // ==================== OrderStatus 测试 @yutiansut @quantaxis ====================

    /// 测试 OrderStatus 枚举值
    #[test]
    fn test_order_status_values() {
        let statuses = vec![
            OrderStatus::PendingRisk,
            OrderStatus::PendingRoute,
            OrderStatus::Submitted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
        ];

        // 测试相等性
        assert_eq!(OrderStatus::Submitted, OrderStatus::Submitted);
        assert_ne!(OrderStatus::Submitted, OrderStatus::Filled);

        // 验证所有状态都可以被创建
        assert_eq!(statuses.len(), 7);
    }

    // ==================== 订单提交测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_submit_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(response.success);
        assert!(response.order_id.is_some());
        assert!(response.error_message.is_none());
    }

    #[test]
    fn test_submit_order_insufficient_funds() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 100000.0, // 超大数量
            price: 1000.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(!response.success);
        assert!(response.error_message.is_some());
    }

    #[test]
    fn test_query_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(response.success);

        let order_id = response.order_id.unwrap();
        let order = router.query_order(&order_id);
        assert!(order.is_some());

        let order = order.unwrap();
        assert_eq!(order.user_id, "test_user");
        assert_eq!(order.instrument_id, "IX2301");
    }

    #[test]
    fn test_query_user_orders() {
        let router = create_test_router();

        // 提交多个订单
        for i in 0..3 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 10.0 + i as f64,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        let orders = router.query_user_orders("test_user");
        assert_eq!(orders.len(), 3);
    }

    #[test]
    fn test_generate_order_id() {
        let router = create_test_router();

        let id1 = router.generate_order_id();
        let id2 = router.generate_order_id();

        assert_ne!(id1, id2);
        assert!(id1.starts_with('O'));
        assert!(id2.starts_with('O'));
    }

    #[test]
    fn test_complete_order_flow_with_matching() {
        // 完整的订单流程集成测试：风控 -> 路由 -> 撮合 -> 成交 -> 账户更新

        // 1. 创建路由器和两个测试账户（避免自成交）
        let router = create_test_router();

        // 创建第二个账户用于卖单
        let req2 = OpenAccountRequest {
            user_id: "test_user_2".to_string(),
            account_id: Some("test_user_2".to_string()),
            account_name: "Test User 2".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        router.account_mgr.open_account(req2).unwrap();

        let trade_receiver = router.trade_gateway.subscribe_user("test_user".to_string());

        // 2. 获取初始账户状态（使用user_id获取默认账户）
        let account = router.account_mgr.get_default_account("test_user").unwrap();
        let init_balance = account.read().accounts.balance;
        log::info!("Initial balance: {}", init_balance);

        // 3. 提交买单（账户1）
        let buy_req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let buy_response = router.submit_order(buy_req);
        assert!(
            buy_response.success,
            "Buy order submission failed: {:?}",
            buy_response.error_message
        );
        let buy_order_id = buy_response.order_id.unwrap();
        log::info!("Buy order submitted: {}", buy_order_id);

        // 4. 提交卖单（账户2，避免自成交）
        let sell_req = SubmitOrderRequest {
            account_id: "test_user_2".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(), // 使用OPEN，因为是不同账户
            volume: 5.0,                // 部分成交
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let sell_response = router.submit_order(sell_req);
        assert!(
            sell_response.success,
            "Sell order submission failed: {:?}",
            sell_response.error_message
        );
        let sell_order_id = sell_response.order_id.unwrap();
        log::info!("Sell order submitted: {}", sell_order_id);

        // 5. 检查是否收到成交通知（可选）
        // 注意：由于撮合是同步的，通知应该已经发送
        let mut notifications = Vec::new();
        while let Ok(notification) = trade_receiver.try_recv() {
            log::info!("Received notification: {:?}", notification);
            notifications.push(notification);
        }

        // ⚠️ 原先此处只 log 不断言,理由写的是「通知系统可能使用新的 NotificationBroker」。
        // 那句话本身指向了一条真实的分叉(见 tasks/DECISIONS-PENDING.md),
        // 但把唯一能发现它的断言取消掉了 —— 于是分叉长期无人察觉。
        //
        // 本测试用的 `subscribe_user` 正是 WebSocket 会话走的同一条路
        // (service/websocket/mod.rs:126),所以这里**必须**收到通知;
        // 收不到 = 前端实时推送已断。@yutiansut @quantaxis
        assert!(
            !notifications.is_empty(),
            "subscribe_user 没收到任何通知 —— 这条路同时是 WS 前端推送通道,断了前端就看不到成交"
        );

        // 6. 查询订单状态
        let buy_order = router.query_order(&buy_order_id).unwrap();
        log::info!("Buy order status: {:?}", buy_order.status);

        // 7. 验证账户状态已更新
        // ⚠️ 原先这里只有 `final_balance > 0.0`,而账户初始就有 100 万 ——
        // 撮合即使完全不工作,该断言照样通过。测试名写的是
        // 「风控 -> 路由 -> 撮合 -> 成交 -> 账户更新」,后三项一项都没验。
        // 另外本文件测试模块不初始化 logger(env_logger 只在 main.rs:1588),
        // 所以上面那些 log::info! 在 `cargo test` 下**零输出**,不能当观测手段。
        // @yutiansut @quantaxis
        let account = router.account_mgr.get_default_account("test_user").unwrap();

        // ① 撮合真的发生了:买 10 手、卖 5 手 → 买单应为部分成交
        //    复用既有公开 API get_order_status(order_router.rs:1955)
        let buy_status = router
            .get_order_status(&buy_order_id)
            .expect("买单应该还在 orders 表里");
        assert_eq!(
            buy_status,
            OrderStatus::PartiallyFilled,
            "买 10 手 / 卖 5 手,买单应为 PartiallyFilled,实际 {:?} —— 撮合没发生或状态没回写",
            buy_status
        );

        // ② 成交落到了账户持仓上(复用 trade_gateway.rs:1212 的既有读法)
        let pos_volume = {
            let acc = account.read();
            // qars 的 get_position 需要 &mut self;只读场景用 get_position_unmut
            // (qars2 account.rs:462),避免为了读一个数升级成写锁。
            acc.get_position_unmut("IX2301")
                .map(|p| p.volume_long_today + p.volume_long_his)
                .unwrap_or(0.0)
        };
        assert_eq!(
            pos_volume, 5.0,
            "买方多头持仓应为 5 手,实际 {} —— 成交没写进账户",
            pos_volume
        );

        // ③ 账户被更新过 —— **必须断言计算值,不能断言 `accounts.*` 快照字段**。
        //
        // qars 里 `accounts.balance` / `accounts.margin` 是**从不自动刷新**的缓存:
        // qars2 `account.rs` 全文没有一处 `accounts.margin = `。真值来自
        // `get_balance()`(account.rs:522)/ `get_margin()`(:493),而 QIFI 投影
        // `get_accountmessage()`(:405)取的正是这些计算值 —— 所以对外输出是对的。
        //
        // 生产里那些快照字段由 `RiskMonitor::do_risk_check`(risk_monitor.rs:446-452)
        // 每秒回写一次;单元测试不跑风控循环,于是快照恒为构造值。
        // 实测本用例:snap_balance=1000000 / snap_margin=0,
        // 而 calc_balance=999999.808 / calc_margin=600 —— 断言快照会得到假失败。
        // @yutiansut @quantaxis
        let (calc_balance, calc_margin) = {
            let mut acc = account.write();
            (acc.get_balance(), acc.get_margin())
        };

        // 开 5 手必然占用保证金
        assert!(
            calc_margin > 0.0,
            "开仓 5 手后保证金应 > 0,实际 {} —— 成交没影响账户",
            calc_margin
        );

        // 手续费必然扣减权益(本用例 commission = 0.192)
        assert!(
            calc_balance < init_balance,
            "扣过手续费后权益应低于初始 {},实际 {}",
            init_balance, calc_balance
        );

        let final_balance = calc_balance;
        assert!(final_balance > 0.0, "Account balance should be positive");
        assert!(final_balance > 0.0, "Account balance should be positive");

        log::info!("Complete order flow test passed!");
    }

    // ==================== 撤单测试 @yutiansut @quantaxis ====================

    /// 测试撤单 - 订单不存在
    #[test]
    fn test_cancel_order_not_found() {
        let router = create_test_router();

        let req = CancelOrderRequest {
            account_id: "test_user".to_string(),
            order_id: "NON_EXISTENT_ORDER".to_string(),
        };

        let result = router.cancel_order(req);
        assert!(result.is_err());

        if let Err(ExchangeError::OrderError(msg)) = result {
            assert!(msg.contains("Order not found"));
        } else {
            panic!("Expected OrderError");
        }
    }

    /// 测试撤单 - 订单不属于当前账户
    #[test]
    fn test_cancel_order_wrong_owner() {
        let router = create_test_router();

        // 创建第二个账户
        let req2 = OpenAccountRequest {
            user_id: "test_user_2".to_string(),
            account_id: Some("test_user_2".to_string()),
            account_name: "Test User 2".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        router.account_mgr.open_account(req2).unwrap();

        // test_user 提交订单
        let submit_req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(submit_req);
        assert!(response.success);
        let order_id = response.order_id.unwrap();

        // test_user_2 尝试撤单
        let cancel_req = CancelOrderRequest {
            account_id: "test_user_2".to_string(),
            order_id: order_id,
        };

        let result = router.cancel_order(cancel_req);
        assert!(result.is_err());

        if let Err(ExchangeError::OrderError(msg)) = result {
            assert!(msg.contains("does not belong"));
        } else {
            panic!("Expected OrderError about ownership");
        }
    }

    // ==================== 订单统计测试 @yutiansut @quantaxis ====================

    /// 测试订单统计 - 初始状态
    #[test]
    fn test_get_order_statistics_empty() {
        let router = create_test_router();

        let stats = router.get_order_statistics();
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.filled_count, 0);
        assert_eq!(stats.cancelled_count, 0);
        assert_eq!(stats.rejected_count, 0);
    }

    /// 测试订单统计 - 提交后
    #[test]
    fn test_get_order_statistics_after_submit() {
        let router = create_test_router();

        // 提交订单
        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        router.submit_order(req);

        let stats = router.get_order_statistics();
        assert_eq!(stats.total_count, 1);
        // 订单提交后应为 pending 状态
        assert!(stats.pending_count >= 0); // 可能是submitted或其他状态
    }

    /// 测试订单统计 - 多订单
    #[test]
    fn test_get_order_statistics_multiple() {
        let router = create_test_router();

        // 提交多个订单
        for _ in 0..5 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        let stats = router.get_order_statistics();
        assert_eq!(stats.total_count, 5);
    }

    // ==================== 成交统计测试 @yutiansut @quantaxis ====================

    /// 测试成交统计 - 初始状态
    #[test]
    fn test_get_trade_statistics_empty() {
        let router = create_test_router();

        let stats = router.get_trade_statistics();
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.total_volume, 0.0);
        assert_eq!(stats.total_amount, 0.0);
    }

    // ==================== 订单详情测试 @yutiansut @quantaxis ====================

    /// 测试获取订单详情 - 不存在
    #[test]
    fn test_get_order_detail_not_found() {
        let router = create_test_router();

        let detail = router.get_order_detail("NON_EXISTENT");
        assert!(detail.is_none());
    }

    /// 测试获取订单详情 - 存在
    #[test]
    fn test_get_order_detail_exists() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(response.success);
        let order_id = response.order_id.unwrap();

        let detail = router.get_order_detail(&order_id);
        assert!(detail.is_some());

        let (order, status, submit_time, update_time, filled_volume) = detail.unwrap();
        assert_eq!(order.user_id, "test_user");
        assert_eq!(order.instrument_id, "IX2301");
        assert!(submit_time > 0);
        assert!(update_time >= submit_time);
        assert_eq!(filled_volume, 0.0);
    }

    // ==================== 用户订单详情测试 @yutiansut @quantaxis ====================

    /// 测试获取用户订单详情 - 用户不存在
    #[test]
    fn test_get_user_order_details_not_found() {
        let router = create_test_router();

        let details = router.get_user_order_details("NON_EXISTENT_USER");
        assert!(details.is_empty());
    }

    /// 测试获取用户订单详情 - 存在订单
    #[test]
    fn test_get_user_order_details_exists() {
        let router = create_test_router();

        // 提交多个订单
        for i in 0..3 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0 + i as f64,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        let details = router.get_user_order_details("test_user");
        assert_eq!(details.len(), 3);

        // 验证每个订单详情
        for d in details {
            assert!(!d.order_id.is_empty());
            assert_eq!(d.order.user_id, "test_user");
            assert!(d.submit_time > 0);
            assert!(d.update_time >= d.submit_time);
            // qa_order_id 是跨源去重的钥匙 —— 它不能是空的,否则
            // handlers.rs 的 seen 种子少一半,同一笔单会显示成两条。
            // @yutiansut @quantaxis
            assert!(
                !d.qa_order_id.is_empty(),
                "qa_order_id 为空会让 QIFI/OrderRouter 跨源去重失效"
            );
        }
    }

    // ==================== 全部订单测试 @yutiansut @quantaxis ====================

    /// 测试获取所有订单 - 空
    #[test]
    fn test_get_all_orders_empty() {
        let router = create_test_router();

        let orders = router.get_all_orders();
        assert!(orders.is_empty());
    }

    /// 测试获取所有订单 - 多用户
    #[test]
    fn test_get_all_orders_multiple_users() {
        let router = create_test_router();

        // 创建第二个账户
        let req2 = OpenAccountRequest {
            user_id: "test_user_2".to_string(),
            account_id: Some("test_user_2".to_string()),
            account_name: "Test User 2".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        router.account_mgr.open_account(req2).unwrap();

        // 用户1提交订单
        for _ in 0..2 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        // 用户2提交订单
        for _ in 0..3 {
            let req = SubmitOrderRequest {
                account_id: "test_user_2".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "SELL".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        let orders = router.get_all_orders();
        assert_eq!(orders.len(), 5);
    }

    /// 回归:CLOSEYESTERDAY 成交后双方账户都要入账,QIFI 委托 / 成交字段如实回填 @yutiansut @quantaxis
    ///
    /// 此前 `TradeGateway::update_account` 的开平映射没有平昨分支:撮合簿里已经成交,
    /// 账户侧却报 "Invalid direction/offset" 返回 —— 持仓不减、同批其余成交也被跳过。
    #[test]
    fn close_yesterday_fill_updates_both_accounts_and_qifi_fields() {
        let router = create_test_router();
        router
            .account_mgr
            .open_account(OpenAccountRequest {
                user_id: "test_user_cy".to_string(),
                account_id: Some("test_user_cy".to_string()),
                account_name: "Close Yesterday".to_string(),
                init_cash: 1_000_000.0,
                account_type: AccountType::Individual,
            })
            .unwrap();
        let order = |acc: &str, dir: &str, off: &str, vol: f64| SubmitOrderRequest {
            account_id: acc.to_string(),
            instrument_id: "IX2301".to_string(),
            direction: dir.to_string(),
            offset: off.to_string(),
            volume: vol,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };
        for (acc, dir, off, vol) in [
            ("test_user", "BUY", "OPEN", 2.0),
            ("test_user_cy", "SELL", "OPEN", 2.0),
            ("test_user", "SELL", "CLOSEYESTERDAY", 1.0),
            ("test_user_cy", "BUY", "CLOSEYESTERDAY", 1.0),
        ] {
            let r = router.submit_order(order(acc, dir, off, vol));
            assert!(r.success, "{} {} {}: {:?}", acc, dir, off, r.error_message);
        }
        let long_acc = router.account_mgr.get_account("test_user").unwrap();
        let short_acc = router.account_mgr.get_account("test_user_cy").unwrap();
        let long = long_acc.read().hold.get("IX2301").map(|p| p.volume_long_unmut());
        let short = short_acc.read().hold.get("IX2301").map(|p| p.volume_short_unmut());
        assert_eq!(long, Some(1.0), "多头 2 手平掉 1 手");
        assert_eq!(short, Some(1.0), "空头 2 手平掉 1 手");

        // QIFI:委托 time/volume_condition 与 exchange_id 按请求 / 合约注册表回填;成交带交易所与成交号
        let acc = long_acc.read();
        assert!(acc.dailyorders.values().all(|o| o.time_condition == "GFD"
            && o.volume_condition == "ANY"
            && o.exchange_id == "SHFE"));
        assert_eq!(acc.dailytrades.len(), 2);
        assert!(acc
            .dailytrades
            .values()
            .all(|t| t.exchange_id == "SHFE" && !t.exchange_trade_id.is_empty()));
        let pos = acc.hold.get("IX2301").unwrap();
        assert_eq!(pos.exchange_id, "SHFE");
        assert!((pos.position_price_long - 120.0).abs() < 1e-9, "均价 {}", pos.position_price_long);
    }

    /// 回归:未上市合约在冻结资金之前拒单(4006 → CTP 16),账户里不留冻结 @yutiansut @quantaxis
    #[test]
    fn unlisted_instrument_rejected_before_freezing_funds() {
        let router = create_test_router();
        let acc = router.account_mgr.get_account("test_user").unwrap();
        let money_before = acc.read().money;
        let r = router.submit_order(SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "NOSUCH0000".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 1.0,
            price: 100.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        });
        assert!(!r.success);
        assert_eq!(r.error_code, Some(4006));
        let a = acc.read();
        assert_eq!(a.money, money_before, "不应冻结资金");
        assert!(a.frozen.is_empty(), "不应留下冻结条目");
        assert!(a.dailyorders.is_empty(), "不应在账户里留下委托");
    }

    // ==================== 日终(闭市 → 撤活单 → 结算 → 开市)@yutiansut @quantaxis ====================

    fn eod_order(acc: &str, dir: &str, off: &str, vol: f64, px: f64) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: acc.to_string(),
            instrument_id: "IX2301".to_string(),
            direction: dir.to_string(),
            offset: off.to_string(),
            volume: vol,
            price: px,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        }
    }

    /// 两个账户:成交 1 手建仓,另各挂 1 张不成交的单(买 119 / 卖 121)
    fn eod_fixture() -> OrderRouter {
        let router = create_test_router();
        router
            .account_mgr
            .open_account(OpenAccountRequest {
                user_id: "test_user_eod".to_string(),
                account_id: Some("test_user_eod".to_string()),
                account_name: "EOD".to_string(),
                init_cash: 1_000_000.0,
                account_type: AccountType::Individual,
            })
            .unwrap();
        for r in [
            eod_order("test_user", "BUY", "OPEN", 1.0, 120.0),
            eod_order("test_user_eod", "SELL", "OPEN", 1.0, 120.0),
            eod_order("test_user", "BUY", "OPEN", 2.0, 119.0),
            eod_order("test_user_eod", "SELL", "OPEN", 1.0, 121.0),
        ] {
            let resp = router.submit_order(r);
            assert!(resp.success, "{:?}", resp.error_message);
        }
        router
    }

    fn resting_count(router: &OrderRouter) -> usize {
        let book = router.matching_engine.get_orderbook("IX2301").unwrap();
        let ob = book.read();
        ob.bid_queue.get_sorted_orders().map_or(0, |v| v.len())
            + ob.ask_queue.get_sorted_orders().map_or(0, |v| v.len())
    }

    /// 闭市期间新单拒 4100(→ CTP 91)且无任何副作用;扫单撤光活单、释放冻结;开市后照常收单
    #[test]
    fn eod_halt_rejects_new_orders_and_sweep_cancels_all_resting() {
        let router = eod_fixture();
        assert_eq!(resting_count(&router), 2);

        assert!(router.begin_eod_halt(Duration::from_secs(1)));
        let r = router.submit_order(eod_order("test_user", "BUY", "OPEN", 1.0, 118.0));
        assert!(!r.success);
        assert_eq!(r.error_code, Some(4100));
        assert_eq!(ctp_error_id(4100, ""), 91);
        let acc = router.account_mgr.get_account("test_user").unwrap();
        assert_eq!(acc.read().dailyorders.len(), 2, "闭市拒单不进账户");

        let sweep = router.cancel_all_active_orders();
        assert_eq!(sweep.cancelled, 2, "{:?}", sweep);
        assert_eq!(sweep.residual_left, 0);
        assert_eq!(resting_count(&router), 0);
        for id in ["test_user", "test_user_eod"] {
            let acc = router.account_mgr.get_account(id).unwrap();
            let a = acc.read();
            assert!(a.frozen.is_empty(), "{} 冻结应全部释放: {:?}", id, a.frozen.keys());
            assert!(a.dailyorders.values().all(|o| o.status == "FINISHED"), "{} 无活单", id);
        }

        router.end_eod_halt();
        assert!(router.submit_order(eod_order("test_user", "BUY", "OPEN", 1.0, 118.0)).success);
    }

    /// 路由认为是活单、撮合簿里却已没有(启动恢复时恢复单交叉成交):日终撤单失败后路由侧
    /// 按已撤收尾,下一次日终不再重复失败 @yutiansut @quantaxis
    #[test]
    fn eod_sweep_retires_router_orders_missing_from_book() {
        let router = eod_fixture();
        // 模拟:把 test_user 的 119 买单从簿上直接摘掉,路由仍记为 Submitted
        let (order_id, engine_id) = router
            .orders
            .iter()
            .find_map(|e| {
                let i = e.value().read();
                (i.order.user_id == "test_user" && i.status == OrderStatus::Submitted)
                    .then(|| (e.key().clone(), i.matching_engine_order_id.unwrap()))
            })
            .unwrap();
        let book = router.matching_engine.get_orderbook("IX2301").unwrap();
        assert!(book.write().bid_queue.cancel(engine_id));

        let first = router.cancel_all_active_orders();
        assert_eq!((first.cancelled, first.cancel_failed, first.residual_left), (1, 1, 0));
        assert_eq!(router.get_order_status(&order_id), Some(OrderStatus::Cancelled));
        let second = router.cancel_all_active_orders();
        assert_eq!((second.cancelled, second.cancel_failed), (0, 0), "{:?}", second);
    }

    /// 回归:结算之后再撤单 / 成交不得 panic(release panic=abort 会让整个交易所退出)。
    /// 走完整日终:今仓 → 昨仓、当日委托/成交清空、客户端编号按新交易日可重用;
    /// 快照归档到 history/<交易日>/,结算后快照不再带当日历史。@yutiansut @quantaxis
    #[test]
    fn end_of_day_settles_then_trading_resumes_without_panic() {
        let router = Arc::new(eod_fixture());
        // 带客户端编号的挂单(DIFF 下单路径):结算后同号应可重用(CTP 报单引用按交易日)
        let r = router.submit_order_with_client_id(
            eod_order("test_user", "BUY", "OPEN", 1.0, 118.0),
            Some("eod_c1".to_string()),
        );
        assert!(r.success);
        let dup = router.submit_order_with_client_id(
            eod_order("test_user", "BUY", "OPEN", 1.0, 118.0),
            Some("eod_c1".to_string()),
        );
        assert_eq!(dup.error_code, Some(22), "当日内重复编号拒 22");

        let dir = tempfile::tempdir().unwrap();
        let snap_dir = dir.path().join("snapshots").to_string_lossy().into_owned();
        let engine = crate::exchange::SettlementEngine::new(router.account_mgr.clone());
        engine.set_order_router(router.clone());
        engine.set_snapshot_dir(snap_dir.clone());

        let report = engine.run_end_of_day("2026-11-08").expect("日终应成功");
        assert_eq!(report.sweep.cancelled, 3, "{:?}", report.sweep);
        assert_eq!(report.sweep.residual_left, 0);
        assert!(report.settlement.settled_accounts >= 2);
        assert_eq!(report.settlement.settlement_date, "2026-11-08");
        assert_eq!(report.settlement_prices.get("IX2301"), Some(&120.0));
        assert!(report.client_order_ids_cleared >= 1);
        assert!(!router.is_eod_halted(), "日终后必须开市");
        assert_eq!(resting_count(&router), 0);

        // 账户:今仓 → 昨仓,当日委托/成交清空,冻结退回
        let long_acc = router.account_mgr.get_account("test_user").unwrap();
        {
            let a = long_acc.read();
            assert!(a.dailyorders.is_empty() && a.dailytrades.is_empty() && a.frozen.is_empty());
            let p = a.hold.get("IX2301").unwrap();
            assert_eq!((p.volume_long_today, p.volume_long_his), (0.0, 1.0));
        }

        // 归档 = 结算前完整快照(带当日委托);结算后快照不带
        let archived = format!("{}/test_user.json", report.archive_dir);
        let qifi: crate::QIFI =
            serde_json::from_str(&std::fs::read_to_string(&archived).unwrap()).unwrap();
        assert_eq!(qifi.orders.len(), 3, "归档里是结算前的 3 笔委托");
        let live: crate::QIFI = serde_json::from_str(
            &std::fs::read_to_string(format!("{}/test_user.json", snap_dir)).unwrap(),
        )
        .unwrap();
        assert!(live.orders.is_empty() && live.trades.is_empty());

        // 新交易日:同一客户端编号可重用;挂单 → 撤单、对手成交都不 panic
        let r = router.submit_order_with_client_id(
            eod_order("test_user", "BUY", "OPEN", 1.0, 118.0),
            Some("eod_c1".to_string()),
        );
        assert!(r.success, "新交易日同号可用: {:?}", r.error_message);
        router
            .cancel_order(CancelOrderRequest {
                account_id: "test_user".to_string(),
                order_id: r.order_id.unwrap(),
            })
            .expect("结算后撤单");
        assert!(router.submit_order(eod_order("test_user", "SELL", "CLOSE", 1.0, 120.0)).success);
        assert!(router.submit_order(eod_order("test_user_eod", "BUY", "CLOSE", 1.0, 120.0)).success);
        let a = long_acc.read();
        assert_eq!(a.hold.get("IX2301").map(|p| p.volume_long_unmut()).unwrap_or(0.0), 0.0, "昨仓平掉");
        assert_eq!(a.dailytrades.len(), 1);
    }

    /// 纵深防御:绕过日终扫单直接结算(旧的手动结算接口就是这样),残留挂单之后被撤 / 被成交,
    /// 网关守卫拦在 qars 之前 —— 不 panic。没有守卫时本测试在 qars
    /// `cancel_order` 的 expect(account.rs:1237)处 panic。@yutiansut @quantaxis
    #[test]
    fn settlement_without_sweep_no_longer_panics_on_cancel_or_fill() {
        let router = Arc::new(eod_fixture());
        let engine = crate::exchange::SettlementEngine::new(router.account_mgr.clone());
        engine.daily_settlement_for("2026-11-08").unwrap();
        assert_eq!(resting_count(&router), 2, "未扫单:挂单仍在簿上,账户当日委托已清");

        // 撤残留的买单(test_user 119):qars 撤单被跳过,不 panic
        let bid = router
            .orders
            .iter()
            .find(|e| {
                let i = e.value().read();
                i.order.user_id == "test_user" && i.status == OrderStatus::Submitted
            })
            .map(|e| e.key().clone())
            .unwrap();
        let _ = router.cancel_order(CancelOrderRequest {
            account_id: "test_user".to_string(),
            order_id: bid,
        });
        // 吃掉残留的卖单(test_user_eod 121):挂单方成交不入账,不 panic
        let _ = router.submit_order(eod_order("test_user", "BUY", "OPEN", 1.0, 121.0));
    }

    // ==================== 订单计数测试 @yutiansut @quantaxis ====================

    /// 测试订单计数 - 初始
    #[test]
    fn test_get_order_count_empty() {
        let router = create_test_router();

        assert_eq!(router.get_order_count(), 0);
    }

    /// 测试订单计数 - 增长
    #[test]
    fn test_get_order_count_growth() {
        let router = create_test_router();

        for i in 0..10 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
            assert_eq!(router.get_order_count(), i + 1);
        }
    }

    // ==================== 查询订单测试 @yutiansut @quantaxis ====================

    /// 测试查询订单 - 不存在
    #[test]
    fn test_query_order_not_found() {
        let router = create_test_router();

        let order = router.query_order("NON_EXISTENT");
        assert!(order.is_none());
    }

    /// 测试查询用户订单 - 用户不存在
    #[test]
    fn test_query_user_orders_not_found() {
        let router = create_test_router();

        let orders = router.query_user_orders("NON_EXISTENT_USER");
        assert!(orders.is_empty());
    }

    // ==================== 订单ID生成测试 @yutiansut @quantaxis ====================

    /// 测试订单ID唯一性
    #[test]
    fn test_generate_order_id_unique() {
        let router = create_test_router();

        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = router.generate_order_id();
            assert!(ids.insert(id.clone()), "Duplicate order ID generated: {}", id);
        }
    }

    /// 测试订单ID格式
    #[test]
    fn test_generate_order_id_format() {
        let router = create_test_router();

        for _ in 0..10 {
            let id = router.generate_order_id();
            assert!(id.starts_with('O'), "Order ID should start with 'O': {}", id);
            assert!(id.len() > 10, "Order ID should be longer than 10 chars: {}", id);
        }
    }

    // ==================== SubmitOrderRequest 测试 @yutiansut @quantaxis ====================

    /// 测试 SubmitOrderRequest 创建
    #[test]
    fn test_submit_order_request_creation() {
        let req = SubmitOrderRequest {
            account_id: "user1".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 85000.0,
            order_type: "LIMIT".to_string(),
            time_condition: Some(TimeCondition::GFD),
            volume_condition: Some(VolumeCondition::ANY),
        };

        assert_eq!(req.account_id, "user1");
        assert_eq!(req.instrument_id, "cu2501");
        assert_eq!(req.direction, "BUY");
        assert_eq!(req.offset, "OPEN");
        assert_eq!(req.volume, 10.0);
        assert_eq!(req.price, 85000.0);
        assert_eq!(req.order_type, "LIMIT");
        assert_eq!(req.time_condition, Some(TimeCondition::GFD));
        assert_eq!(req.volume_condition, Some(VolumeCondition::ANY));
    }

    /// 测试 SubmitOrderRequest Clone
    #[test]
    fn test_submit_order_request_clone() {
        let req = SubmitOrderRequest {
            account_id: "user1".to_string(),
            instrument_id: "cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 85000.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let cloned = req.clone();
        assert_eq!(req.account_id, cloned.account_id);
        assert_eq!(req.instrument_id, cloned.instrument_id);
    }

    // ==================== CancelOrderRequest 测试 @yutiansut @quantaxis ====================

    /// 测试 CancelOrderRequest 创建
    #[test]
    fn test_cancel_order_request_creation() {
        let req = CancelOrderRequest {
            account_id: "user1".to_string(),
            order_id: "O12345".to_string(),
        };

        assert_eq!(req.account_id, "user1");
        assert_eq!(req.order_id, "O12345");
    }

    /// 测试 CancelOrderRequest Clone
    #[test]
    fn test_cancel_order_request_clone() {
        let req = CancelOrderRequest {
            account_id: "user1".to_string(),
            order_id: "O12345".to_string(),
        };

        let cloned = req.clone();
        assert_eq!(req.account_id, cloned.account_id);
        assert_eq!(req.order_id, cloned.order_id);
    }

    // ==================== SubmitOrderResponse 测试 @yutiansut @quantaxis ====================

    /// 测试 SubmitOrderResponse 成功
    #[test]
    fn test_submit_order_response_success() {
        let resp = SubmitOrderResponse {
            success: true,
            order_id: Some("O12345".to_string()),
            status: Some("submitted".to_string()),
            error_message: None,
            error_code: None,
        };

        assert!(resp.success);
        assert_eq!(resp.order_id, Some("O12345".to_string()));
        assert_eq!(resp.status, Some("submitted".to_string()));
        assert!(resp.error_message.is_none());
        assert!(resp.error_code.is_none());
    }

    /// 测试 SubmitOrderResponse 失败
    #[test]
    fn test_submit_order_response_failure() {
        let resp = SubmitOrderResponse {
            success: false,
            order_id: None,
            status: Some("rejected".to_string()),
            error_message: Some("Insufficient funds".to_string()),
            error_code: Some(4001),
        };

        assert!(!resp.success);
        assert!(resp.order_id.is_none());
        assert_eq!(resp.status, Some("rejected".to_string()));
        assert_eq!(resp.error_message, Some("Insufficient funds".to_string()));
        assert_eq!(resp.error_code, Some(4001));
    }

    // ==================== OrderStatistics 测试 @yutiansut @quantaxis ====================

    /// 测试 OrderStatistics 字段
    #[test]
    fn test_order_statistics_fields() {
        let stats = OrderStatistics {
            total_count: 100,
            pending_count: 20,
            filled_count: 70,
            cancelled_count: 5,
            rejected_count: 5,
        };

        assert_eq!(stats.total_count, 100);
        assert_eq!(stats.pending_count, 20);
        assert_eq!(stats.filled_count, 70);
        assert_eq!(stats.cancelled_count, 5);
        assert_eq!(stats.rejected_count, 5);

        // 验证总数一致性（注意：pending包含submitted和partially_filled）
        assert!(stats.pending_count + stats.filled_count + stats.cancelled_count + stats.rejected_count <= stats.total_count);
    }

    /// 测试 OrderStatistics Clone
    #[test]
    fn test_order_statistics_clone() {
        let stats = OrderStatistics {
            total_count: 50,
            pending_count: 10,
            filled_count: 30,
            cancelled_count: 5,
            rejected_count: 5,
        };

        let cloned = stats.clone();
        assert_eq!(stats.total_count, cloned.total_count);
        assert_eq!(stats.pending_count, cloned.pending_count);
    }

    // ==================== TradeStatistics 测试 @yutiansut @quantaxis ====================

    /// 测试 TradeStatistics 字段
    #[test]
    fn test_trade_statistics_fields() {
        let stats = TradeStatistics {
            total_count: 1000,
            total_volume: 50000.0,
            total_amount: 4250000000.0,
        };

        assert_eq!(stats.total_count, 1000);
        assert_eq!(stats.total_volume, 50000.0);
        assert_eq!(stats.total_amount, 4250000000.0);
    }

    /// 测试 TradeStatistics Clone
    #[test]
    fn test_trade_statistics_clone() {
        let stats = TradeStatistics {
            total_count: 500,
            total_volume: 25000.0,
            total_amount: 2125000000.0,
        };

        let cloned = stats.clone();
        assert_eq!(stats.total_count, cloned.total_count);
        assert_eq!(stats.total_volume, cloned.total_volume);
    }

    // ==================== OrderStatus 更多测试 @yutiansut @quantaxis ====================

    /// 测试 OrderStatus Copy trait
    #[test]
    fn test_order_status_copy() {
        let status1 = OrderStatus::Submitted;
        let status2 = status1; // Copy
        let status3 = status1.clone(); // Clone

        assert_eq!(status1, status2);
        assert_eq!(status1, status3);
    }

    /// 测试 OrderStatus 所有状态
    #[test]
    fn test_order_status_all_values() {
        let statuses = [
            OrderStatus::PendingRisk,
            OrderStatus::PendingRoute,
            OrderStatus::Submitted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
        ];

        // 验证没有重复
        for (i, s1) in statuses.iter().enumerate() {
            for (j, s2) in statuses.iter().enumerate() {
                if i != j {
                    assert_ne!(s1, s2);
                }
            }
        }
    }

    // ==================== 订单类型测试 @yutiansut @quantaxis ====================

    /// 测试 LIMIT 订单
    #[test]
    fn test_submit_limit_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: Some(TimeCondition::GFD),
            volume_condition: Some(VolumeCondition::ANY),
        };

        let response = router.submit_order(req);
        assert!(response.success);
    }

    /// 测试卖单
    #[test]
    fn test_submit_sell_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(),
            volume: 5.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(response.success);
    }

    // ==================== 时间/数量条件测试 @yutiansut @quantaxis ====================

    /// 测试 IOC 订单
    #[test]
    fn test_submit_ioc_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 5.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: Some(TimeCondition::IOC),
            volume_condition: Some(VolumeCondition::ANY),
        };

        let response = router.submit_order(req);
        // IOC订单可能被拒绝（如果没有对手方），或成功提交
        // 这里只验证请求不会导致panic
        assert!(response.order_id.is_some() || response.error_message.is_some());
    }

    /// 测试 GTC 订单
    #[test]
    fn test_submit_gtc_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 5.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: Some(TimeCondition::GTC),
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(response.success);
    }

    /// 测试 ALL (FOK) 数量条件
    #[test]
    fn test_submit_fok_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 5.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: Some(TimeCondition::IOC),
            volume_condition: Some(VolumeCondition::ALL), // FOK = IOC + ALL
        };

        let response = router.submit_order(req);
        // FOK订单在没有对手方时会被拒绝
        // 这里只验证请求不会导致panic
        assert!(response.order_id.is_some() || response.error_message.is_some());
    }

    // ==================== 合约不存在测试 @yutiansut @quantaxis ====================

    /// 测试提交订单到不存在的合约
    #[test]
    fn test_submit_order_instrument_not_found() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "NON_EXISTENT_INSTRUMENT".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(!response.success);
        assert!(response.error_message.is_some());
    }

    // ==================== 账户不存在测试 @yutiansut @quantaxis ====================

    /// 测试提交订单到不存在的账户
    #[test]
    fn test_submit_order_account_not_found() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "NON_EXISTENT_ACCOUNT".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        assert!(!response.success);
        assert!(response.error_message.is_some());
    }

    // ==================== 边界条件测试 @yutiansut @quantaxis ====================

    /// 测试零价格订单
    #[test]
    fn test_submit_order_zero_price() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 0.0, // 零价格
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        // 零价格可能被拒绝
        // 这里只验证不会panic
        assert!(response.order_id.is_some() || response.error_message.is_some());
    }

    /// 测试零数量订单
    #[test]
    fn test_submit_order_zero_volume() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 0.0, // 零数量
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        // 零数量可能被拒绝
        // 这里只验证不会panic
        assert!(response.order_id.is_some() || response.error_message.is_some());
    }

    /// 测试负数量订单
    #[test]
    fn test_submit_order_negative_volume() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: -10.0, // 负数量
            price: 120.0,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        };

        let response = router.submit_order(req);
        // 负数量应该被拒绝
        // 这里只验证不会panic
        assert!(response.order_id.is_some() || response.error_message.is_some());
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发提交订单
    #[test]
    fn test_concurrent_submit_orders() {
        use std::thread;

        let router = Arc::new(create_test_router());
        let mut handles = vec![];

        for i in 0..10 {
            let router_clone = router.clone();
            handles.push(thread::spawn(move || {
                let req = SubmitOrderRequest {
                    account_id: "test_user".to_string(),
                    instrument_id: "IX2301".to_string(),
                    direction: if i % 2 == 0 { "BUY" } else { "SELL" }.to_string(),
                    offset: "OPEN".to_string(),
                    volume: 1.0,
                    price: 120.0,
                    order_type: "LIMIT".to_string(),
                    time_condition: None,
                    volume_condition: None,
                };
                router_clone.submit_order(req)
            }));
        }

        let mut success_count = 0;
        for handle in handles {
            let response = handle.join().unwrap();
            if response.success {
                success_count += 1;
            }
        }

        // 至少有一些订单应该成功
        assert!(success_count > 0, "No orders were successful");

        // 验证订单数量
        assert!(router.get_order_count() >= success_count);
    }

    /// 测试并发查询订单
    #[test]
    fn test_concurrent_query_orders() {
        use std::thread;

        let router = Arc::new(create_test_router());

        // 先提交一些订单
        for _ in 0..5 {
            let req = SubmitOrderRequest {
                account_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 1.0,
                price: 120.0,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };
            router.submit_order(req);
        }

        let mut handles = vec![];

        for _ in 0..10 {
            let router_clone = router.clone();
            handles.push(thread::spawn(move || {
                // 并发查询
                let _ = router_clone.query_user_orders("test_user");
                let _ = router_clone.get_order_statistics();
                let _ = router_clone.get_trade_statistics();
                let _ = router_clone.get_all_orders();
                router_clone.get_order_count()
            }));
        }

        for handle in handles {
            let count = handle.join().unwrap();
            assert!(count >= 0);
        }
    }
}

/// ✨ DIFF 交易回报(第二期 2a,2026-11-08):`trade.<account>.orders/trades` = QIFI 切片、
/// 客户端编号、CTP 错误码、成交先于终态、seqno。@yutiansut @quantaxis
#[cfg(test)]
mod diff_report_tests {
    use super::tests_support::*;
    use super::*;
    use crate::protocol::diff::snapshot::SnapshotManager;
    use serde_json::Value;

    fn limit(acc: &str, dir: &str, vol: f64, px: f64) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: acc.to_string(),
            instrument_id: "IX2301".to_string(),
            direction: dir.to_string(),
            offset: "OPEN".to_string(),
            volume: vol,
            price: px,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        }
    }

    async fn order_view(sm: &SnapshotManager, user: &str, acc: &str, key: &str) -> Value {
        let snap = sm.get_snapshot(user).await.expect("snapshot");
        snap["trade"][acc]["orders"][key].clone()
    }

    #[tokio::test]
    async fn client_id_lifecycle_dup_cancel_trade_and_reject() {
        let (router, sm) = router_with_snapshots();

        // ① 挂单:键 = 客户端编号,ALIVE / status_code 3 / exchange_order_id = 交易所单号
        let r = router.submit_order_with_client_id(limit("test_user", "BUY", 1.0, 120.0), Some("hftm1_1".into()));
        assert!(r.success, "{:?}", r);
        let v = order_view(&sm, "test_user", "test_user", "hftm1_1").await;
        assert_eq!(v["status"], "ALIVE");
        assert_eq!(v["status_code"], 3);
        assert_eq!(v["volume_left"], 1.0);
        assert_eq!(v["exchange_order_id"], r.order_id.clone().unwrap());

        // ② 重复客户端编号 → CTP 22,且不影响原单(DIFF 下单路径会调 publish_rejected_order,
        //    它必须对 22 什么都不推,否则会把原单覆盖成 FINISHED/7)
        let dup_req = limit("test_user", "BUY", 1.0, 119.0);
        let dup = router.submit_order_with_client_id(dup_req.clone(), Some("hftm1_1".into()));
        assert!(!dup.success);
        assert_eq!(dup.error_code, Some(22));
        assert_eq!(ctp_error_id(22, ""), 22);
        router.publish_rejected_order(&dup_req, Some("hftm1_1"), "SHFE", &dup);
        let v = order_view(&sm, "test_user", "test_user", "hftm1_1").await;
        assert_eq!(v["status"], "ALIVE", "重复编号不得覆盖原单: {:?}", v);

        // ③ 按客户端编号撤单 → FINISHED / 5,volume_left 不变
        router
            .cancel_order(CancelOrderRequest { account_id: "test_user".into(), order_id: "hftm1_1".into() })
            .expect("cancel by client id");
        let v = order_view(&sm, "test_user", "test_user", "hftm1_1").await;
        assert_eq!(v["status"], "FINISHED");
        assert_eq!(v["status_code"], 5);
        assert_eq!(v["volume_left"], 1.0);
        assert_eq!(v["last_msg"], "已撤单");

        // ④ 成交:对手先挂卖单,再以客户端编号吃掉 → 成交键 `hftm1_2|<成交号>`,
        //    且在补丁流里**成交先于**使订单 FINISHED 的订单补丁
        let _ = sm.peek("test_user").await; // 清空待发补丁,只看下面这一笔
        let s = router.submit_order(limit("test_user2", "SELL", 1.0, 120.0));
        assert!(s.success, "{:?}", s);
        let _ = sm.peek("test_user").await;
        let b = router.submit_order_with_client_id(limit("test_user", "BUY", 1.0, 120.0), Some("hftm1_2".into()));
        assert!(b.success, "{:?}", b);
        let patches = sm.peek("test_user").await.expect("patches after trade");
        let mut trade_seq = None;
        let mut finished_seq = None;
        for p in &patches {
            if let Some(trades) = p["trade"]["test_user"]["trades"].as_object() {
                for (k, t) in trades {
                    assert!(k.starts_with("hftm1_2|"), "trade key {}", k);
                    assert_eq!(t["order_id"], "hftm1_2");
                    assert!(t["commission"].as_f64().unwrap() >= 0.0);
                    trade_seq.get_or_insert(t["seqno"].as_u64().unwrap());
                }
            }
            if let Some(o) = p["trade"]["test_user"]["orders"]["hftm1_2"].as_object() {
                if o["status"] == "FINISHED" {
                    finished_seq.get_or_insert(o["seqno"].as_u64().unwrap());
                }
            }
        }
        let (ts, fs) = (trade_seq.expect("trade patch"), finished_seq.expect("finished patch"));
        assert!(ts < fs, "成交补丁 seqno {} 必须先于 FINISHED 订单补丁 {}", ts, fs);
        let v = order_view(&sm, "test_user", "test_user", "hftm1_2").await;
        assert_eq!((v["status_code"].as_i64(), v["volume_left"].as_f64()), (Some(0), Some(0.0)));

        // ⑤ 柜台层拒单 → FINISHED / 7 / error_id 按映射表 / exchange_order_id 为空
        //    (本夹具合约乘数 1,资金不足打不出来 —— 10 万手先撞单笔上限 4007 → CTP 15;
        //     资金不足 → 31 的映射由 ctp_error_mapping_table 覆盖)
        let big = limit("test_user", "BUY", 100000.0, 120.0);
        let rj = router.submit_order_with_client_id(big.clone(), Some("hftm1_3".into()));
        assert!(!rj.success);
        router.publish_rejected_order(&big, Some("hftm1_3"), "SHFE", &rj);
        let v = order_view(&sm, "test_user", "test_user", "hftm1_3").await;
        assert_eq!(v["status"], "FINISHED");
        assert_eq!(v["status_code"], 7);
        let expect = ctp_error_id(rj.error_code.unwrap(), rj.error_message.as_deref().unwrap_or(""));
        assert_eq!(v["error_id"], expect, "{:?} / {:?}", rj, v);
        assert_eq!(expect, 15, "单笔超量属于报单字段有误");
        assert_eq!(v["exchange_order_id"], "");
        assert_eq!(v["volume_left"], 100000.0);
    }

    #[test]
    fn ctp_error_mapping_table() {
        assert_eq!(ctp_error_id(1001, ""), 31);
        assert_eq!(ctp_error_id(4006, ""), 16);
        assert_eq!(ctp_error_id(4004, ""), 15);
        assert_eq!(ctp_error_id(4001, "Insufficient funds: available=1"), 31);
        assert_eq!(ctp_error_id(4001, "可平持仓不足"), 30);
        assert_eq!(ctp_error_id(1005, "self trade"), 91);
        assert_eq!(ctp_error_prompt(26), "CTP:报单已全成交或已撤销，不能再撤");
    }
}

#[cfg(test)]
mod tests_support {
    use super::*;
    use crate::core::account_ext::{AccountType, OpenAccountRequest};
    use crate::protocol::diff::snapshot::SnapshotManager;

    /// `tests::create_test_router` 同款夹具 + 第二个账户 + 接上 SnapshotManager
    pub fn router_with_snapshots() -> (OrderRouter, Arc<SnapshotManager>) {
        let router = super::tests::create_test_router();
        router
            .account_mgr
            .open_account(OpenAccountRequest {
                user_id: "test_user2".to_string(),
                account_id: Some("test_user2".to_string()),
                account_name: "Counterparty".to_string(),
                init_cash: 1_000_000.0,
                account_type: AccountType::Individual,
            })
            .unwrap();
        let sm = Arc::new(SnapshotManager::new());
        router.trade_gateway.set_snapshot_manager(sm.clone());
        // 回报只推给有连接在听的主题 —— 测试里模拟两个账户各有一条连接(连接 ID = 主题)
        sm.register_connection("test_user", "test_user");
        sm.register_connection("test_user2", "test_user2");
        (router, sm)
    }
}

/// ✨ 平仓冻结(hft_maker C2,2026-11-08):挂着的平仓单必须冻结可平量,
/// 再发超量平仓单应被拒(CTP 30「平仓量超过持仓量」)。@yutiansut @quantaxis
#[cfg(test)]
mod close_freeze_tests {
    use super::tests_support::*;
    use super::*;

    fn req(acc: &str, dir: &str, off: &str, vol: f64, px: f64) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: acc.to_string(),
            instrument_id: "IX2301".to_string(),
            direction: dir.to_string(),
            offset: off.to_string(),
            volume: vol,
            price: px,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        }
    }

    #[test]
    fn pending_close_freezes_and_over_close_is_rejected() {
        let (router, _sm) = router_with_snapshots();
        // 建 1 手多头
        assert!(router.submit_order(req("test_user2", "SELL", "OPEN", 1.0, 120.0)).success);
        let b = router.submit_order(req("test_user", "BUY", "OPEN", 1.0, 120.0));
        assert!(b.success, "{:?}", b);
        // 挂 1 手平仓(远离市价,不会成交)
        let c1 = router.submit_order(req("test_user", "SELL", "CLOSE", 1.0, 130.0));
        assert!(c1.success, "首张平仓单应被接受: {:?}", c1);
        // 再平 1 手:可平量已被冻结 → 必须拒
        let c2 = router.submit_order(req("test_user", "SELL", "CLOSE", 1.0, 131.0));
        assert!(!c2.success, "超量平仓应被拒,实际: {:?}", c2);
        let eid = ctp_error_id(c2.error_code.unwrap_or(0), c2.error_message.as_deref().unwrap_or(""));
        assert_eq!(eid, 30, "应映射为 CTP 30 平仓量超过持仓量: {:?}", c2);
    }
}

/// ✨ 重启后客户端编号与交易所单号都还在(2026-11-08)@yutiansut @quantaxis
///
/// 修复前:恢复出来的挂单以 qars UUID 为单号、客户端编号映射只在内存 ——
/// 实测部署重启后 `cancel(client_id)` 与 `cancel(O…原单号)` 都报 Order not found。
#[cfg(test)]
mod restart_id_tests {
    use super::tests_support::*;
    use super::*;
    use crate::exchange::instrument_registry::InstrumentInfo;

    fn req(dir: &str, px: f64) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: dir.to_string(),
            offset: "OPEN".to_string(),
            volume: 1.0,
            price: px,
            order_type: "LIMIT".to_string(),
            time_condition: None,
            volume_condition: None,
        }
    }

    /// 与 `tests::create_test_router` 同款合约夹具,但复用给定的账户管理器(模拟「快照读回后」)
    fn router_on(account_mgr: Arc<AccountManager>) -> OrderRouter {
        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("IX2301".to_string(), 120.0).unwrap();
        let registry = Arc::new(InstrumentRegistry::new());
        registry
            .register(InstrumentInfo {
                instrument_id: "IX2301".to_string(),
                instrument_name: "IX2301".to_string(),
                instrument_type: crate::exchange::instrument_registry::InstrumentType::CommodityFuture,
                exchange: "SHFE".to_string(),
                contract_multiplier: 1,
                price_tick: 0.01,
                margin_rate: 0.1,
                commission_rate: 0.0005,
                limit_up_rate: 0.1,
                limit_down_rate: 0.1,
                status: crate::exchange::instrument_registry::InstrumentStatus::Active,
                list_date: None,
                expire_date: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .unwrap();
        let gw = Arc::new(
            TradeGateway::new(account_mgr.clone()).with_wal_root("./output/testexchange/wal_restart_ids"),
        );
        OrderRouter::new(account_mgr, engine, registry, gw)
    }

    #[test]
    fn client_and_exchange_ids_survive_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("client_order_ids.jsonl").to_str().unwrap().to_string();
        let (r1, _sm) = router_with_snapshots();
        r1.set_client_order_id_file(file.clone());
        let a = r1.submit_order_with_client_id(req("BUY", 100.0), Some("keep_1".into()));
        let b = r1.submit_order_with_client_id(req("BUY", 99.0), Some("keep_2".into()));
        assert!(a.success && b.success);
        let xid_b = b.order_id.clone().unwrap();

        // 「重启」:新撮合引擎 / 新网关 / 新路由,账户状态沿用(= 快照读回后的内存态)
        let r2 = router_on(r1.account_mgr.clone());
        r2.set_client_order_id_file(file);
        r2.restore_orders_from_accounts();
        assert_eq!(r2.load_client_order_ids(), 2);

        // 按客户端编号撤
        r2.cancel_order(CancelOrderRequest { account_id: "test_user".into(), order_id: "keep_1".into() })
            .expect("重启后应能按客户端编号撤单");
        // 按原交易所单号撤
        r2.cancel_order(CancelOrderRequest { account_id: "test_user".into(), order_id: xid_b.clone() })
            .expect("重启后应能按原交易所单号撤单");
        assert!(r2.orders.contains_key(&xid_b), "恢复后的单号应是原交易所单号 {}", xid_b);
    }
}
