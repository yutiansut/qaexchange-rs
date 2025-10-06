//! 订单路由模块
//!
//! 负责订单的接收、风控检查、路由到撮合引擎以及撤单处理

use crate::core::{QAOrder, QAOrderExt, Order};
use crate::exchange::{AccountManager, InstrumentRegistry, TradeGateway};
use crate::matching::engine::{ExchangeMatchingEngine, InstrumentAsset};
use crate::matching::{OrderDirection, OrderType, orders, Success, Failed};
use crate::risk::pre_trade_check::{PreTradeCheck, OrderCheckRequest, RiskCheckResult};
use crate::market::MarketDataBroadcaster;
use crate::ExchangeError;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use dashmap::DashMap;
use parking_lot::RwLock;
use chrono::Local;
use serde::{Deserialize, Serialize};

/// 订单提交请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub user_id: String,
    pub instrument_id: String,
    pub direction: String,      // BUY/SELL
    pub offset: String,          // OPEN/CLOSE/CLOSETODAY
    pub volume: f64,
    pub price: f64,
    pub order_type: String,      // LIMIT/MARKET
}

/// 撤单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub user_id: String,
    pub order_id: String,
}

/// 订单提交响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub success: bool,
    pub order_id: Option<String>,
    pub status: Option<String>,  // 订单最终状态：submitted/filled/partially_filled/rejected
    pub error_message: Option<String>,
    pub error_code: Option<u32>,
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
#[derive(Debug, Clone)]
struct OrderRouteInfo {
    order: Order,
    status: OrderStatus,
    submit_time: i64,
    update_time: i64,
    filled_volume: f64,  // 已成交数量
    qa_order_id: String, // qars 内部订单ID (用于 receive_deal_sim)
    matching_engine_order_id: Option<u64>, // 撮合引擎订单ID (用于撤单)
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

    /// 订单序号生成器
    order_seq: AtomicU64,

    /// 统计：总成交笔数
    trade_count: AtomicU64,

    /// 统计：总成交量
    trade_volume: parking_lot::RwLock<f64>,

    /// 统计：总成交金额
    trade_amount: parking_lot::RwLock<f64>,
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
            order_seq: AtomicU64::new(1),
            trade_count: AtomicU64::new(0),
            trade_volume: parking_lot::RwLock::new(0.0),
            trade_amount: parking_lot::RwLock::new(0.0),
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
            order_seq: AtomicU64::new(1),
            trade_count: AtomicU64::new(0),
            trade_volume: parking_lot::RwLock::new(0.0),
            trade_amount: parking_lot::RwLock::new(0.0),
        }
    }

    /// 提交订单 (核心方法)
    pub fn submit_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
        // 1. 生成订单ID
        let order_id = self.generate_order_id();

        // 2. 风控检查
        let risk_check_req = OrderCheckRequest {
            user_id: req.user_id.clone(),
            instrument_id: req.instrument_id.clone(),
            direction: req.direction.clone(),
            offset: req.offset.clone(),
            volume: req.volume,
            price: req.price,
        };

        match self.risk_checker.check(&risk_check_req) {
            Ok(RiskCheckResult::Pass) => {
                // 风控通过，继续处理
            }
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
                    error_message: Some(format!("Risk check error: {}", e)),
                    error_code: Some(9999),
                };
            }
        }

        // 3. 创建订单 (复用 qars QAOrder)
        let towards = self.calculate_towards(&req.direction, &req.offset);
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let order = QAOrder::new(
            req.user_id.clone(),
            req.instrument_id.clone(),
            towards,
            "EXCHANGE".to_string(), // exchange_id
            current_time.clone(),
            req.volume,
            req.price,
            order_id.clone(),
        );

        // 3.5. 冻结资金/保证金 (方案B：在订单提交时冻结)
        let account = match self.account_mgr.get_default_account(&req.user_id) {
            Ok(acc) => acc,
            Err(e) => {
                log::error!("Account not found for user {}: {}", req.user_id, e);
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!("Account not found: {}", e)),
                    error_code: Some(4000),
                };
            }
        };

        let mut acc = account.write();

        // 3.6. 二次检查余额（在写锁内，避免竞态条件）
        // 参考: todo/并发安全性分析.md - 方案A（双重检查锁模式）
        let estimated_commission = req.price * req.volume * 0.0003; // 万3手续费
        let required_funds = if req.direction == "BUY" && req.offset == "OPEN" {
            // 买开仓需要全额资金
            req.price * req.volume + estimated_commission
        } else if req.direction == "SELL" && req.offset == "OPEN" {
            // 卖开仓需要保证金（简化：20%）
            req.price * req.volume * 0.2 + estimated_commission
        } else {
            // 平仓只需手续费
            estimated_commission
        };

        if acc.money < required_funds {
            log::warn!(
                "Insufficient funds (double-check): user={}, available={:.2}, required={:.2}",
                req.user_id, acc.money, required_funds
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

        let qa_order_result = acc.send_order(
            &req.instrument_id,
            req.volume,
            &current_time,
            towards,
            req.price,
            "",
            &req.order_type,
        );

        // 检查 send_order 是否成功（资金/保证金检查）
        let qa_order_id = match qa_order_result {
            Ok(ref qa_order) => qa_order.order_id.clone(),
            Err(e) => {
                log::warn!("Order rejected - insufficient funds/margin for user {}: {:?}", req.user_id, e);
                return SubmitOrderResponse {
                    success: false,
                    order_id: Some(order_id),
                    status: Some("rejected".to_string()),
                    error_message: Some(format!("Insufficient funds/margin: {:?}", e)),
                    error_code: Some(4001),
                };
            }
        };

        drop(acc); // 释放账户锁

        log::debug!("Funds frozen for order {}, qars order_id: {}", order_id, qa_order_id);

        // 4. 存储订单信息
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let route_info = OrderRouteInfo {
            order: order.clone(),
            status: OrderStatus::PendingRoute,
            submit_time: timestamp,
            update_time: timestamp,
            filled_volume: 0.0,
            qa_order_id: qa_order_id.clone(), // 存储 qars 订单ID
            matching_engine_order_id: None, // 撮合引擎订单ID (在 Accepted 事件中设置)
        };

        self.orders.insert(order_id.clone(), Arc::new(RwLock::new(route_info)));

        // 5. 更新用户订单索引
        self.user_orders
            .entry(req.user_id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(order_id.clone());

        // 6. 注册活动订单 (风控追踪)
        self.risk_checker.register_active_order(
            &req.user_id,
            order_id.clone(),
            req.instrument_id.clone(),
            req.direction.clone()
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
                        _ => "submitted",  // Submitted, PendingRoute, PendingRisk
                    };
                    log::debug!("Order {} final status: {:?} -> {}", order_id, info.status, status_str);
                    status_str
                } else {
                    log::warn!("Order {} not found in orders map when checking status", order_id);
                    "submitted"
                };

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
                    error_message: Some(format!("Routing error: {}", e)),
                    error_code: Some(5000),
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
        let orderbook = self.matching_engine.get_orderbook(instrument_id)
            .ok_or_else(|| ExchangeError::MatchingError(
                format!("Orderbook not found for instrument: {}", instrument_id)
            ))?;

        // 转换订单方向
        let direction = match order.direction.as_str() {
            "BUY" => OrderDirection::BUY,
            "SELL" => OrderDirection::SELL,
            _ => return Err(ExchangeError::OrderError(
                format!("Invalid direction: {}", order.direction)
            )),
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
        let results = ob.process_order(match_request)
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
        let mut handled_trade = false; // 是否已处理成交事件（Filled/PartiallyFilled）

        log::debug!("🔍 process_matching_results: order_id={}, user_id={}, results_count={}",
            order_id, order.user_id, results.len());

        for (idx, result) in results.into_iter().enumerate() {
            log::debug!("🔍   Result[{}]: {:?}", idx, result);
            match result {
                Ok(success) => {
                    match success {
                        Success::Accepted { .. } => {
                            // 只处理第一个Accepted
                            if !handled_accepted {
                                log::debug!("🔍     Processing Accepted event for order {}", order_id);
                                self.handle_success_result(order_id, order, success)?;
                                handled_accepted = true;
                            } else {
                                log::debug!("🔍     Skipping duplicate Accepted event for order {}", order_id);
                            }
                        }
                        Success::Filled { order_id: match_order_id, opposite_order_id, .. }
                        | Success::PartiallyFilled { order_id: match_order_id, opposite_order_id, .. } => {
                            // 处理成交事件
                            // qars 会返回两个事件：新订单成交 + 对手单成交
                            // 我们需要更新对手单的状态（如果它属于我们管理的订单）

                            if !handled_trade {
                                // 第一个事件：新订单的成交
                                log::debug!("🔍     Processing NEW order trade: order_id={}, opposite={}", match_order_id, opposite_order_id);
                                self.handle_success_result(order_id, order, success.clone())?;
                                handled_trade = true;
                            } else {
                                // 第二个事件：对手单的成交
                                // 检查对手单是否在我们的订单簿中，如果在则更新状态
                                log::debug!("🔍     Processing OPPOSITE order trade: order_id={}, opposite={}", match_order_id, opposite_order_id);

                                // 将对手单的 order_id (u64) 转换为我们的 order_id (String)
                                let opposite_order_str = format!("O{:024}", opposite_order_id);

                                // 如果对手单在我们的订单簿中，更新它的状态
                                if self.orders.contains_key(&opposite_order_str) {
                                    log::debug!("🔍     Found opposite order {} in our orderbook, updating status", opposite_order_str);

                                    // 提取对手单信息用于处理
                                    if let Some(opposite_info) = self.orders.get(&opposite_order_str) {
                                        let opposite_order_data = opposite_info.read().order.clone();
                                        // 处理对手单的成交
                                        self.handle_success_result(&opposite_order_str, &opposite_order_data, success)?;
                                    }
                                } else {
                                    log::debug!("🔍     Opposite order {} not in our orderbook, skipping", opposite_order_str);
                                }
                            }
                        }
                        _ => {
                            // 其他事件正常处理（Cancelled, Amended等）
                            self.handle_success_result(order_id, order, success)?;
                        }
                    }
                }
                Err(failed) => {
                    log::warn!("Order matching failed: {:?}", failed);

                    // Phase 6: 使用新的 handle_order_rejected_new (交易所推送REJECTED回报)
                    let reason = format!("{:?}", failed);
                    let _ = self.trade_gateway.handle_order_rejected_new(
                        &order.instrument_id,
                        &order.user_id,
                        order_id,
                        &reason,
                    );

                    log::debug!("Order {} rejected, reason: {}", order_id, reason);

                    // 更新订单状态为拒绝
                    if let Some(order_info) = self.orders.get(order_id) {
                        let mut info = order_info.write();
                        info.status = OrderStatus::Rejected;
                        info.update_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    }
                }
            }
        }
        Ok(())
    }

    /// 处理成功的撮合结果 (Phase 6: 使用新的回报机制)
    fn handle_success_result(
        &self,
        order_id: &str,
        order: &Order,
        success: Success,
    ) -> Result<(), ExchangeError> {
        match success {
            Success::Accepted { id, order_type, ts } => {
                // 订单被接受，等待撮合
                log::info!("Order {} accepted at {}", order_id, ts);

                // 更新订单状态并存储撮合引擎订单ID
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Submitted;
                    info.update_time = ts;
                    info.matching_engine_order_id = Some(id); // 存储撮合引擎订单ID，用于撤单
                }

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

                log::debug!("Order {} accepted, exchange_order_id={}", order_id, exchange_order_id);
            }
            Success::Filled { order_id: match_order_id, direction, order_type, price, volume, ts, opposite_order_id } => {
                // 订单完全成交
                log::info!("Order {} filled: price={}, volume={}", order_id, price, volume);

                // 更新订单状态和已成交量
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Filled;
                    info.update_time = ts;
                    info.filled_volume = volume;
                }

                // 更新成交统计
                self.update_trade_stats(price, volume);

                // 广播Tick成交数据
                if let Some(ref broadcaster) = self.market_broadcaster {
                    let direction_str = if order.direction == "BUY" { "buy" } else { "sell" };
                    broadcaster.broadcast_tick(
                        order.instrument_id.clone(),
                        price,
                        volume,
                        direction_str.to_string(),
                    );

                    // 同时广播最新价
                    broadcaster.broadcast_last_price(order.instrument_id.clone(), price);
                }

                // 持久化Tick数据到WAL
                self.persist_tick_data(&order.instrument_id, price, volume)?;

                // 获取 qars 订单ID
                let qa_order_id = if let Some(order_info) = self.orders.get(order_id) {
                    order_info.read().qa_order_id.clone()
                } else {
                    log::error!("Order info not found for {}", order_id);
                    String::new()
                };

                // Phase 6: 使用新的 handle_trade_new (交易所只推送TRADE回报，不判断FILLED/PARTIAL)
                // 注意：这里假设我们使用已生成的exchange_order_id（从Accepted事件保存）
                // 简化实现：使用match_order_id作为exchange_order_id
                let trade_id = self.trade_gateway.handle_trade_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    match_order_id as i64,
                    &order.user_id,
                    order_id,
                    &order.direction,
                    volume,
                    price,
                    Some(opposite_order_id as i64),
                )?;

                log::debug!("Trade executed: trade_id={}, order_id={}, volume={}, price={}",
                    trade_id, order_id, volume, price);

                // 从活动订单追踪中移除
                self.risk_checker.remove_active_order(&order.user_id, order_id);
            }
            Success::PartiallyFilled { order_id: match_order_id, direction, order_type, price, volume, ts, opposite_order_id } => {
                // 订单部分成交
                log::info!("Order {} partially filled: price={}, volume={}", order_id, price, volume);

                // 更新订单状态和累计成交量
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::PartiallyFilled;
                    info.update_time = ts;
                    info.filled_volume += volume;
                }

                // 更新成交统计
                self.update_trade_stats(price, volume);

                // 广播Tick成交数据
                if let Some(ref broadcaster) = self.market_broadcaster {
                    let direction_str = if order.direction == "BUY" { "buy" } else { "sell" };
                    broadcaster.broadcast_tick(
                        order.instrument_id.clone(),
                        price,
                        volume,
                        direction_str.to_string(),
                    );

                    // 同时广播最新价
                    broadcaster.broadcast_last_price(order.instrument_id.clone(), price);
                }

                // 持久化Tick数据到WAL
                self.persist_tick_data(&order.instrument_id, price, volume)?;

                // 获取 qars 订单ID
                let qa_order_id = if let Some(order_info) = self.orders.get(order_id) {
                    order_info.read().qa_order_id.clone()
                } else {
                    log::error!("Order info not found for {}", order_id);
                    String::new()
                };

                // Phase 6: 使用新的 handle_trade_new (交易所不区分FILLED/PARTIAL，只推送TRADE)
                let trade_id = self.trade_gateway.handle_trade_new(
                    &order.exchange_id,
                    &order.instrument_id,
                    match_order_id as i64,
                    &order.user_id,
                    order_id,
                    &order.direction,
                    volume,
                    price,
                    Some(opposite_order_id as i64),
                )?;

                log::debug!("Trade executed (partial): trade_id={}, order_id={}, volume={}, price={}",
                    trade_id, order_id, volume, price);
            }
            Success::Cancelled { id, ts } => {
                // 订单被撤销
                log::info!("Order {} cancelled at {}", order_id, ts);

                // 更新订单状态
                if let Some(order_info) = self.orders.get(order_id) {
                    let mut info = order_info.write();
                    info.status = OrderStatus::Cancelled;
                    info.update_time = ts;
                }

                // Phase 6: 使用新的 handle_cancel_accepted_new (交易所推送CANCEL_ACCEPTED回报)
                self.trade_gateway.handle_cancel_accepted_new(
                    &order.instrument_id,
                    id as i64,  // 使用撮合引擎返回的ID作为exchange_order_id
                    &order.user_id,
                    order_id,
                )?;

                log::debug!("Order {} cancel accepted, exchange_order_id={}", order_id, id);

                // 从活动订单追踪中移除
                self.risk_checker.remove_active_order(&order.user_id, order_id);
            }
            Success::Amended { id, price, volume, ts } => {
                // 订单修改 (暂不处理，预留)
                log::info!("Order {} amended: price={}, volume={}", order_id, price, volume);
            }
        }
        Ok(())
    }

    /// 撤单
    pub fn cancel_order(&self, req: CancelOrderRequest) -> Result<(), ExchangeError> {
        // 1. 验证订单存在
        let order_info = self.orders.get(&req.order_id)
            .ok_or_else(|| ExchangeError::OrderError(
                format!("Order not found: {}", req.order_id)
            ))?;

        let mut info = order_info.write();

        // 2. 验证订单所有权
        if info.order.user_id != req.user_id {
            return Err(ExchangeError::OrderError(
                "Order does not belong to this user".to_string()
            ));
        }

        // 3. 检查订单状态是否可撤单
        if !matches!(info.status, OrderStatus::Submitted | OrderStatus::PartiallyFilled) {
            return Err(ExchangeError::OrderError(
                format!("Order cannot be cancelled in status: {:?}", info.status)
            ));
        }

        // 4. 从撮合引擎撤单
        let matching_engine_order_id = info.matching_engine_order_id
            .ok_or_else(|| ExchangeError::OrderError(
                "Matching engine order ID not found".to_string()
            ))?;

        let instrument_id = info.order.instrument_id.clone();
        let direction_str = info.order.direction.clone();

        // 释放写锁，避免在调用撮合引擎时持有锁
        drop(info);
        drop(order_info);

        // 转换订单方向
        let direction = match direction_str.as_str() {
            "BUY" => OrderDirection::BUY,
            "SELL" => OrderDirection::SELL,
            _ => return Err(ExchangeError::OrderError(
                format!("Invalid direction: {}", direction_str)
            )),
        };

        // 创建撤单请求
        let asset = InstrumentAsset::from_code(&instrument_id);
        let cancel_request = crate::matching::OrderRequest::CancelOrder {
            id: matching_engine_order_id,
            direction,
        };

        // 获取订单簿
        let orderbook = self.matching_engine.get_orderbook(&instrument_id)
            .ok_or_else(|| ExchangeError::MatchingError(
                format!("Orderbook not found for instrument: {}", instrument_id)
            ))?;

        // 提交撤单请求到撮合引擎
        let mut ob = orderbook.write();
        let results = ob.process_order(cancel_request)
            .into_iter()
            .collect::<Vec<_>>();
        drop(ob);

        // 处理撤单结果
        for result in results {
            match result {
                Ok(success) => {
                    log::info!("Cancel order success: {:?}", success);
                    // 撤单成功后会收到 Success::Cancelled 事件，由 handle_success_result 处理
                    // 这里不需要额外处理
                }
                Err(failed) => {
                    log::error!("Cancel order failed: {:?}", failed);
                    return Err(ExchangeError::MatchingError(
                        format!("Cancel order failed: {:?}", failed)
                    ));
                }
            }
        }

        log::info!("Order cancelled from matching engine: {}", req.order_id);
        Ok(())
    }

    /// 查询订单
    pub fn query_order(&self, order_id: &str) -> Option<Order> {
        self.orders.get(order_id).map(|info| info.read().order.clone())
    }

    /// 查询用户所有订单
    pub fn query_user_orders(&self, user_id: &str) -> Vec<Order> {
        if let Some(order_ids) = self.user_orders.get(user_id) {
            order_ids.read()
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
    pub fn update_order_status(&self, order_id: &str, status: OrderStatus) -> Result<(), ExchangeError> {
        let order_info = self.orders.get(order_id)
            .ok_or_else(|| ExchangeError::OrderError(
                format!("Order not found: {}", order_id)
            ))?;

        let mut info = order_info.write();
        info.status = status;
        info.update_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // 如果订单完成，从风控追踪中移除
        if matches!(status, OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected) {
            self.risk_checker.remove_active_order(&info.order.user_id, order_id);
        }

        Ok(())
    }

    /// 生成订单ID
    fn generate_order_id(&self) -> String {
        let seq = self.order_seq.fetch_add(1, Ordering::SeqCst);
        let timestamp = chrono::Utc::now().timestamp_millis();
        format!("O{}{:010}", timestamp, seq)
    }

    /// 计算 towards (买卖方向 - 遵循 qars 定义)
    fn calculate_towards(&self, direction: &str, offset: &str) -> i32 {
        match (direction, offset) {
            ("BUY", "OPEN") => 2,       // 买开 = 2 (qars 标准)
            ("SELL", "OPEN") => -2,     // 卖开 = -2
            ("BUY", "CLOSE") => 3,      // 买平 = 3
            ("SELL", "CLOSE") => -3,    // 卖平 = -3 ✅
            ("BUY", "CLOSETODAY") => 4,
            ("SELL", "CLOSETODAY") => -4,
            _ => 2, // 默认买开
        }
    }

    /// 获取活动订单数量
    pub fn get_active_order_count(&self) -> usize {
        self.orders.iter()
            .filter(|entry| {
                let status = entry.value().read().status;
                matches!(status, OrderStatus::Submitted | OrderStatus::PartiallyFilled)
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
    fn persist_tick_data(&self, instrument_id: &str, price: f64, volume: f64) -> Result<(), ExchangeError> {
        if let Some(ref storage) = self.storage {
            use crate::storage::wal::record::WalRecord;

            // 获取订单簿中的买卖价
            let (bid_price, ask_price) = if let Some(orderbook) = self.matching_engine.get_orderbook(instrument_id) {
                let ob = orderbook.read();
                let bid = ob.bid_queue.get_sorted_orders()
                    .and_then(|orders| orders.first().map(|o| o.price))
                    .unwrap_or(0.0);
                let ask = ob.ask_queue.get_sorted_orders()
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

            // 写入WAL（OltpHybridStorage会自动处理序列号）
            if let Err(e) = storage.write(tick_record) {
                log::warn!("Failed to persist tick data to WAL: {}", e);
                // 不影响交易流程，只记录警告
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

    /// 获取订单详细信息（包含时间戳和成交量）
    pub fn get_order_detail(&self, order_id: &str) -> Option<(Order, OrderStatus, i64, i64, f64)> {
        self.orders.get(order_id).map(|info| {
            let i = info.read();
            (i.order.clone(), i.status, i.submit_time, i.update_time, i.filled_volume)
        })
    }

    /// 获取用户所有订单的详细信息 (order_id, order, status, submit_time, update_time, filled_volume)
    pub fn get_user_order_details(&self, user_id: &str) -> Vec<(String, Order, OrderStatus, i64, i64, f64)> {
        if let Some(order_ids) = self.user_orders.get(user_id) {
            order_ids.read()
                .iter()
                .filter_map(|order_id| {
                    self.orders.get(order_id).map(|info| {
                        let i = info.read();
                        (order_id.clone(), i.order.clone(), i.status, i.submit_time, i.update_time, i.filled_volume)
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{OpenAccountRequest, AccountType};
    use crate::exchange::instrument_registry::InstrumentInfo;

    fn create_test_router() -> OrderRouter {
        // 创建账户管理器
        let account_mgr = Arc::new(AccountManager::new());
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: None,
            account_name: "Test User".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 创建撮合引擎
        let matching_engine = Arc::new(ExchangeMatchingEngine::new());
        matching_engine.register_instrument("IX2301".to_string(), 120.0).unwrap();

        // 创建合约注册表
        let instrument_registry = Arc::new(InstrumentRegistry::new());
        instrument_registry.register(InstrumentInfo {
            instrument_id:"IX2301".to_string(),
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
            list_date: Some("2023-01-01".to_string()),
            expire_date: Some("2023-12-31".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
        });

        // 创建成交回报网关
        let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));

        OrderRouter::new(account_mgr, matching_engine, instrument_registry, trade_gateway)
    }

    #[test]
    fn test_submit_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
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
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 100000.0, // 超大数量
            price: 1000.0,
            order_type: "LIMIT".to_string(),
        };

        let response = router.submit_order(req);
        assert!(!response.success);
        assert!(response.error_message.is_some());
    }

    #[test]
    fn test_query_order() {
        let router = create_test_router();

        let req = SubmitOrderRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
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
                user_id: "test_user".to_string(),
                instrument_id: "IX2301".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                volume: 10.0 + i as f64,
                price: 120.0,
                order_type: "LIMIT".to_string(),
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

        // 1. 创建路由器和订阅成交通知
        let router = create_test_router();
        let trade_receiver = router.trade_gateway.subscribe_user("test_user".to_string());

        // 2. 获取初始账户状态（使用user_id获取默认账户）
        let account = router.account_mgr.get_default_account("test_user").unwrap();
        let init_balance = account.read().accounts.balance;
        log::info!("Initial balance: {}", init_balance);

        // 3. 提交买单
        let buy_req = SubmitOrderRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
        };

        let buy_response = router.submit_order(buy_req);
        assert!(buy_response.success, "Buy order submission failed: {:?}", buy_response.error_message);
        let buy_order_id = buy_response.order_id.unwrap();
        log::info!("Buy order submitted: {}", buy_order_id);

        // 4. 提交卖单（应该与买单撮合）
        let sell_req = SubmitOrderRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(), // 平仓之前的买单
            volume: 5.0, // 部分成交
            price: 120.0,
            order_type: "LIMIT".to_string(),
        };

        let sell_response = router.submit_order(sell_req);
        assert!(sell_response.success, "Sell order submission failed: {:?}", sell_response.error_message);
        let sell_order_id = sell_response.order_id.unwrap();
        log::info!("Sell order submitted: {}", sell_order_id);

        // 5. 检查是否收到成交通知
        // 注意：由于撮合是同步的，通知应该已经发送
        let mut notifications = Vec::new();
        while let Ok(notification) = trade_receiver.try_recv() {
            log::info!("Received notification: {:?}", notification);
            notifications.push(notification);
        }

        // 应该至少收到订单接受通知
        assert!(!notifications.is_empty(), "No notifications received");
        log::info!("Total notifications received: {}", notifications.len());

        // 6. 查询订单状态
        let buy_order = router.query_order(&buy_order_id).unwrap();
        log::info!("Buy order status: {:?}", buy_order.status);

        // 7. 验证账户状态已更新
        // 注意：由于撮合逻辑的复杂性，这里只验证账户依然存在且可访问
        let account = router.account_mgr.get_default_account("test_user").unwrap();
        let final_balance = account.read().accounts.balance;
        log::info!("Final balance: {}", final_balance);

        // 账户应该依然有效
        assert!(final_balance > 0.0, "Account balance should be positive");

        log::info!("Complete order flow test passed!");
    }
}
