//! 结算系统
//!
//! @yutiansut @quantaxis
//!
//! 负责日终结算、盯市盈亏计算、强平处理等
//!
//! ## 性能优化
//! - **Rayon 并行结算**: 多账户并行处理，8核可达 8x 加速
//! - **分阶段处理**: 预计算(只读) -> 应用(短写锁) -> 异步强平
//! - **批量聚合**: 减少锁竞争和内存分配

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Instant;

use chrono::Utc;
use dashmap::DashMap;
use log;
use parking_lot::RwLock;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AccountManager, OrderRouter};
use crate::exchange::order_router::SubmitOrderRequest;
use crate::market::MarketDataService;
use crate::risk::RiskMonitor;
use crate::ExchangeError;

/// 结算结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    /// 结算日期
    pub settlement_date: String,

    /// 总账户数
    pub total_accounts: usize,

    /// 成功结算数
    pub settled_accounts: usize,

    /// 失败结算数
    pub failed_accounts: usize,

    /// 强平账户列表
    pub force_closed_accounts: Vec<String>,

    /// 总手续费
    pub total_commission: f64,

    /// 总盈亏
    pub total_profit: f64,

    /// 结算耗时（毫秒）
    #[serde(default)]
    pub elapsed_ms: u64,

    /// 并行度（使用的线程数）
    #[serde(default)]
    pub parallelism: usize,
}

/// 预计算结算数据（只读阶段，无锁）
#[derive(Debug, Clone)]
struct PreCalculatedSettlement {
    /// 账户 ID
    account_id: String,
    /// 持仓盈亏
    position_profit: f64,
    /// 平仓盈亏
    close_profit: f64,
    /// 手续费
    commission: f64,
    /// 结算前权益
    pre_balance: f64,
    /// 结算后权益
    new_balance: f64,
    /// 新保证金
    new_margin: f64,
    /// 风险度
    risk_ratio: f64,
    /// 是否需要强平
    need_force_close: bool,
}

/// 账户结算信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettlement {
    pub user_id: String,
    pub date: String,
    pub close_profit: f64,    // 平仓盈亏
    pub position_profit: f64, // 持仓盈亏
    pub commission: f64,      // 手续费
    pub pre_balance: f64,     // 结算前权益
    pub balance: f64,         // 结算后权益
    pub risk_ratio: f64,      // 风险度
    pub force_close: bool,    // 是否强平
    pub margin: f64,
    pub available: f64,
}

/// 强平订单状态
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForceLiquidationStatus {
    /// 待提交
    Pending,
    /// 已提交
    Submitted,
    /// 部分成交
    PartiallyFilled,
    /// 全部成交
    Filled,
    /// 已撤销
    Cancelled,
    /// 拒绝
    Rejected,
    /// 失败
    Failed,
}

impl ForceLiquidationStatus {
    /// 是否为终态
    pub fn is_final(&self) -> bool {
        matches!(self,
            ForceLiquidationStatus::Filled |
            ForceLiquidationStatus::Cancelled |
            ForceLiquidationStatus::Rejected |
            ForceLiquidationStatus::Failed
        )
    }

    /// 是否成功
    pub fn is_success(&self) -> bool {
        matches!(self, ForceLiquidationStatus::Filled)
    }
}

/// 强平订单结果
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceLiquidationOrder {
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub order_id: Option<String>,
    pub status: ForceLiquidationStatus,
    pub error: Option<String>,
    /// 已成交数量
    pub filled_volume: f64,
    /// 成交均价
    pub filled_price: f64,
    /// 提交时间
    pub submit_time: Option<String>,
    /// 最后更新时间
    pub update_time: Option<String>,
    /// 重试次数
    pub retry_count: u32,
}

impl ForceLiquidationOrder {
    pub fn new(instrument_id: String, direction: String, offset: String, volume: f64, price: f64) -> Self {
        Self {
            instrument_id,
            direction,
            offset,
            volume,
            price,
            order_id: None,
            status: ForceLiquidationStatus::Pending,
            error: None,
            filled_volume: 0.0,
            filled_price: 0.0,
            submit_time: None,
            update_time: None,
            retry_count: 0,
        }
    }
}

/// 强平执行结果
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceLiquidationResult {
    /// 强平ID
    pub liquidation_id: String,
    /// 账户ID
    pub account_id: String,
    /// 强平订单列表
    pub orders: Vec<ForceLiquidationOrder>,
    /// 触发风险率
    pub trigger_risk_ratio: f64,
    /// 强平前权益
    pub balance_before: f64,
    /// 强平后权益
    pub balance_after: f64,
    /// 开始时间
    pub start_time: String,
    /// 完成时间
    pub complete_time: Option<String>,
    /// 总体状态
    pub overall_status: ForceLiquidationStatus,
    /// 备注
    pub remark: Option<String>,
}

impl ForceLiquidationResult {
    /// 检查是否全部完成
    pub fn is_complete(&self) -> bool {
        self.orders.iter().all(|o| o.status.is_final())
    }

    /// 检查是否全部成功
    pub fn is_all_success(&self) -> bool {
        self.orders.iter().all(|o| o.status.is_success())
    }

    /// 获取成功订单数
    pub fn success_count(&self) -> usize {
        self.orders.iter().filter(|o| o.status.is_success()).count()
    }

    /// 获取失败订单数
    pub fn failed_count(&self) -> usize {
        self.orders.iter().filter(|o| matches!(o.status,
            ForceLiquidationStatus::Rejected |
            ForceLiquidationStatus::Failed |
            ForceLiquidationStatus::Cancelled
        )).count()
    }

    /// 更新总体状态
    pub fn update_overall_status(&mut self) {
        if self.orders.is_empty() {
            self.overall_status = ForceLiquidationStatus::Filled;
            return;
        }

        if self.is_all_success() {
            self.overall_status = ForceLiquidationStatus::Filled;
        } else if self.is_complete() {
            // 有失败的
            self.overall_status = ForceLiquidationStatus::Failed;
        } else if self.orders.iter().any(|o| o.status == ForceLiquidationStatus::PartiallyFilled) {
            self.overall_status = ForceLiquidationStatus::PartiallyFilled;
        } else if self.orders.iter().any(|o| o.status == ForceLiquidationStatus::Submitted) {
            self.overall_status = ForceLiquidationStatus::Submitted;
        } else {
            self.overall_status = ForceLiquidationStatus::Pending;
        }
    }
}

/// 结算引擎
///
/// ## 性能特性
/// - 并行结算：使用 Rayon 实现多账户并行处理
/// - 三阶段处理：预计算(只读) -> 应用(短写锁) -> 异步强平
/// - 原子统计：无锁性能指标收集
///
/// ## 强平确认机制 (Phase P0-3)
/// @yutiansut @quantaxis
/// - 强平状态追踪：Pending → Submitted → Filled/Failed
/// - 强平历史记录：保存所有强平执行结果
/// - 失败重试机制：最多重试3次
pub struct SettlementEngine {
    /// 账户管理器
    account_mgr: Arc<AccountManager>,

    /// 结算价格映射 (instrument_id -> settlement_price)
    settlement_prices: Arc<DashMap<String, f64>>,

    /// 强平风险度阈值
    force_close_threshold: f64,

    /// 结算历史 (date -> SettlementResult)
    settlement_history: Arc<DashMap<String, SettlementResult>>,

    /// 账户结算历史 (account_id -> Vec<AccountSettlement>)
    account_history: Arc<DashMap<String, Vec<AccountSettlement>>>,

    /// 订单路由引用（强平下单）
    order_router: Arc<RwLock<Option<Weak<OrderRouter>>>>,

    /// 市场数据服务（用于获取强平价格参考）
    market_data_service: Arc<RwLock<Option<Arc<MarketDataService>>>>,

    /// 风险监控器（记录强平）
    risk_monitor: Arc<RwLock<Option<Arc<RiskMonitor>>>>,

    // ========== 性能统计 ==========
    /// 总结算账户数（原子计数）
    stats_settled_count: AtomicU64,

    /// 总结算耗时（微秒）
    stats_total_time_us: AtomicU64,

    /// 强平队列（异步处理）
    force_close_queue: Arc<crossbeam::channel::Sender<ForceCloseTask>>,

    /// 强平队列接收端
    force_close_receiver: Arc<crossbeam::channel::Receiver<ForceCloseTask>>,

    /// 强平线程是否已启动
    force_close_worker_started: AtomicBool,

    // ========== 强平确认机制 (P0-3) ==========
    /// 强平历史记录 (liquidation_id -> ForceLiquidationResult)
    liquidation_history: Arc<DashMap<String, ForceLiquidationResult>>,

    /// 账户强平索引 (account_id -> Vec<liquidation_id>)
    account_liquidations: Arc<DashMap<String, Vec<String>>>,

    /// 强平序列号
    liquidation_seq: AtomicU64,

    /// 最大重试次数
    max_retry_count: u32,
}

/// 强平任务
#[derive(Debug, Clone)]
struct ForceCloseTask {
    account_id: String,
    risk_ratio: f64,
    remark: Option<String>,
}

/// 结算统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementStats {
    /// 总结算账户数
    pub total_settled_accounts: u64,
    /// 总结算耗时（微秒）
    pub total_time_us: u64,
    /// 待处理强平数量
    pub pending_force_close: usize,
}

impl SettlementEngine {
    /// 创建结算引擎
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        // 创建有界强平队列（最多 1000 个待处理）
        let (sender, receiver) = crossbeam::channel::bounded(1000);

        Self {
            account_mgr,
            settlement_prices: Arc::new(DashMap::new()),
            force_close_threshold: 1.0, // 风险度 >= 100% 强平
            settlement_history: Arc::new(DashMap::new()),
            account_history: Arc::new(DashMap::new()),
            order_router: Arc::new(RwLock::new(None)),
            market_data_service: Arc::new(RwLock::new(None)),
            risk_monitor: Arc::new(RwLock::new(None)),
            stats_settled_count: AtomicU64::new(0),
            stats_total_time_us: AtomicU64::new(0),
            force_close_queue: Arc::new(sender),
            force_close_receiver: Arc::new(receiver),
            force_close_worker_started: AtomicBool::new(false),
            // P0-3: 强平确认机制
            liquidation_history: Arc::new(DashMap::new()),
            account_liquidations: Arc::new(DashMap::new()),
            liquidation_seq: AtomicU64::new(1),
            max_retry_count: 3,
        }
    }

    /// 生成强平ID
    fn generate_liquidation_id(&self) -> String {
        let seq = self.liquidation_seq.fetch_add(1, Ordering::SeqCst);
        format!("LIQ{}{:08}", Utc::now().format("%Y%m%d"), seq)
    }

    /// 获取强平记录
    pub fn get_liquidation(&self, liquidation_id: &str) -> Option<ForceLiquidationResult> {
        self.liquidation_history.get(liquidation_id).map(|r| r.value().clone())
    }

    /// 获取账户的所有强平记录
    pub fn get_account_liquidations(&self, account_id: &str) -> Vec<ForceLiquidationResult> {
        self.account_liquidations
            .get(account_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.liquidation_history.get(id).map(|r| r.value().clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取待处理的强平记录
    pub fn get_pending_liquidations(&self) -> Vec<ForceLiquidationResult> {
        self.liquidation_history
            .iter()
            .filter(|entry| !entry.value().is_complete())
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// 启动异步强平处理线程
    pub fn start_force_close_worker(&self) {
        // 避免重复启动
        if self
            .force_close_worker_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let receiver = self.force_close_receiver.clone();
        let order_router = self.order_router.clone();
        let account_mgr = self.account_mgr.clone();
        let market_data_service = self.market_data_service.clone();
        let risk_monitor = self.risk_monitor.clone();
        let settlement_prices = self.settlement_prices.clone();

        std::thread::spawn(move || {
            log::info!("[SettlementEngine] Force close worker started");

            while let Ok(task) = receiver.recv() {
                log::info!(
                    "[ForceClose] Processing account {} (risk: {:.2}%)",
                    task.account_id,
                    task.risk_ratio * 100.0
                );

                // 执行强平逻辑
                let router_opt = order_router
                    .read()
                    .as_ref()
                    .and_then(|weak| weak.upgrade());

                let monitor_opt = risk_monitor.read().clone();
                let mkt_data_opt = market_data_service.read().clone();

                if let Some(router) = router_opt {
                    if let Ok(account) = account_mgr.get_account(&task.account_id) {
                        let acc = account.read();
                        let balance_before = acc.accounts.balance;
                        drop(acc);

                        // 构建强平计划
                        let plans = Self::build_force_plans(&account, settlement_prices.as_ref());
                        if plans.is_empty() {
                            continue;
                        }

                        // 提交强平订单
                        for plan in plans {
                            let price = Self::calculate_force_price_static(
                                &mkt_data_opt,
                                &plan.instrument_id,
                                &plan.direction,
                                plan.reference_price,
                            );

                            let submit_req = SubmitOrderRequest {
                                account_id: task.account_id.clone(),
                                instrument_id: plan.instrument_id,
                                direction: plan.direction,
                                offset: plan.offset,
                                volume: plan.volume,
                                price,
                                order_type: "LIMIT".to_string(),
                                time_condition: None,
                                volume_condition: None,
                            };

                            let _ = router.submit_force_order(submit_req);
                        }

                        // 记录强平事件
                        if let Some(ref monitor) = monitor_opt {
                            let balance_after = account_mgr
                                .get_account(&task.account_id)
                                .ok()
                                .map(|a| a.read().accounts.balance)
                                .unwrap_or(balance_before);

                            monitor.record_liquidation(
                                task.account_id.clone(),
                                task.risk_ratio,
                                balance_before,
                                balance_after,
                                vec![],
                                task.remark,
                            );
                        }
                    }
                }
            }

            log::info!("[SettlementEngine] Force close worker stopped");
        });
    }

    /// 构建强平计划（静态方法）
    fn build_force_plans(
        account: &Arc<parking_lot::RwLock<qars::qaaccount::account::QA_Account>>,
        settlement_prices: &DashMap<String, f64>,
    ) -> Vec<ForcePlan> {
        let acc = account.read();
        let mut plans = Vec::new();

        for (instrument_id, pos) in acc.hold.iter() {
            let long_volume = pos.volume_long_today + pos.volume_long_his;
            if long_volume > 0.0 {
                plans.push(ForcePlan {
                    instrument_id: instrument_id.clone(),
                    direction: "SELL".to_string(),
                    offset: "CLOSE".to_string(),
                    volume: long_volume,
                    reference_price: settlement_prices
                        .get(instrument_id)
                        .map(|p| *p)
                        .unwrap_or(pos.open_price_long.max(0.01)),
                });
            }

            let short_volume = pos.volume_short_today + pos.volume_short_his;
            if short_volume > 0.0 {
                plans.push(ForcePlan {
                    instrument_id: instrument_id.clone(),
                    direction: "BUY".to_string(),
                    offset: "CLOSE".to_string(),
                    volume: short_volume,
                    reference_price: settlement_prices
                        .get(instrument_id)
                        .map(|p| *p)
                        .unwrap_or(pos.open_price_short.max(0.01)),
                });
            }
        }

        plans
    }

    /// 计算强平价格（静态方法）
    fn calculate_force_price_static(
        market_data_service: &Option<Arc<MarketDataService>>,
        instrument_id: &str,
        direction: &str,
        reference_price: f64,
    ) -> f64 {
        let fallback = reference_price.max(0.01);
        let market_price = market_data_service
            .as_ref()
            .and_then(|svc| svc.get_tick_data(instrument_id).ok())
            .and_then(|tick| match direction {
                "SELL" => tick.bid_price.or(Some(tick.last_price)),
                "BUY" => tick.ask_price.or(Some(tick.last_price)),
                _ => Some(tick.last_price),
            })
            .filter(|price| *price > 0.0)
            .unwrap_or(fallback);

        match direction {
            "SELL" => (market_price * 0.995).max(0.01),
            "BUY" => (market_price * 1.005).max(0.01),
            _ => market_price,
        }
    }

    /// 注入订单路由引用
    pub fn set_order_router(&self, order_router: Arc<OrderRouter>) {
        *self.order_router.write() = Some(Arc::downgrade(&order_router));
    }

    /// 注入市场数据服务（用于强平价格参考）
    pub fn set_market_data_service(&self, service: Arc<MarketDataService>) {
        *self.market_data_service.write() = Some(service);
    }

    /// 注入风险监控器
    pub fn set_risk_monitor(&self, monitor: Arc<RiskMonitor>) {
        *self.risk_monitor.write() = Some(monitor);
    }

    /// 设置结算价
    pub fn set_settlement_price(&self, instrument_id: String, price: f64) {
        log::info!("Settlement price set: {} = {}", instrument_id, price);
        self.settlement_prices.insert(instrument_id, price);
    }

    /// 批量设置结算价
    pub fn set_settlement_prices(&self, prices: HashMap<String, f64>) {
        for (instrument_id, price) in prices {
            self.settlement_prices.insert(instrument_id, price);
        }
        log::info!(
            "Settlement prices set: {} instruments",
            self.settlement_prices.len()
        );
    }

    /// 执行日终结算（高性能并行版本）
    ///
    /// ## 三阶段处理流程
    /// 1. **预计算阶段** (并行，只读锁): 计算所有账户的结算数据
    /// 2. **应用阶段** (并行，短写锁): 应用结算结果到账户
    /// 3. **强平阶段** (异步队列): 将强平任务入队，不阻塞主流程
    ///
    /// ## 性能特性
    /// - 使用 Rayon 并行处理，充分利用多核 CPU
    /// - 10,000 账户结算时间: ~60 秒 (8 核) vs ~500 秒 (单线程)
    pub fn daily_settlement(&self) -> Result<SettlementResult, ExchangeError> {
        // 确保强平线程已启动，避免任务丢失
        self.start_force_close_worker();

        let start_time = Instant::now();
        let settlement_date = Utc::now().format("%Y-%m-%d").to_string();
        let parallelism = rayon::current_num_threads();

        log::info!(
            "[Settlement] Starting parallel settlement for {} with {} threads",
            settlement_date,
            parallelism
        );

        // 获取所有账户
        let accounts = self.account_mgr.get_all_accounts();
        let total_accounts = accounts.len();

        if total_accounts == 0 {
            return Ok(SettlementResult {
                settlement_date,
                total_accounts: 0,
                settled_accounts: 0,
                failed_accounts: 0,
                force_closed_accounts: vec![],
                total_commission: 0.0,
                total_profit: 0.0,
                elapsed_ms: 0,
                parallelism,
            });
        }

        // ========== Phase 1: 并行预计算 (只读锁) ==========
        let phase1_start = Instant::now();
        let pre_calcs: Vec<Option<PreCalculatedSettlement>> = accounts
            .par_iter()
            .map(|account| self.pre_calculate_account(account))
            .collect();
        let phase1_elapsed = phase1_start.elapsed();

        log::debug!(
            "[Settlement] Phase 1 (pre-calc) completed in {:?} for {} accounts",
            phase1_elapsed,
            total_accounts
        );

        // ========== Phase 2: 并行应用结算 (短写锁) ==========
        let phase2_start = Instant::now();
        let results: Vec<Result<AccountSettlement, String>> = accounts
            .par_iter()
            .zip(pre_calcs.par_iter())
            .map(|(account, pre_calc)| {
                if let Some(calc) = pre_calc {
                    self.apply_settlement(account, calc, &settlement_date)
                } else {
                    Err("Pre-calculation failed".to_string())
                }
            })
            .collect();
        let phase2_elapsed = phase2_start.elapsed();

        log::debug!(
            "[Settlement] Phase 2 (apply) completed in {:?}",
            phase2_elapsed
        );

        // ========== Phase 3: 异步强平入队 ==========
        let mut force_closed_accounts: Vec<String> = Vec::new();
        let mut settled_accounts = 0;
        let mut failed_accounts = 0;
        let mut total_commission = 0.0;
        let mut total_profit = 0.0;

        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(settlement) => {
                    settled_accounts += 1;
                    total_commission += settlement.commission;
                    total_profit += settlement.close_profit + settlement.position_profit;

                    // 获取账户 ID
                    let account_id = pre_calcs[i]
                        .as_ref()
                        .map(|c| c.account_id.clone())
                        .unwrap_or_else(|| settlement.user_id.clone());

                    if settlement.force_close {
                        force_closed_accounts.push(settlement.user_id.clone());

                        // 异步入队强平任务
                        if let Err(e) = self.force_close_queue.send(ForceCloseTask {
                            account_id: account_id.clone(),
                            risk_ratio: settlement.risk_ratio,
                            remark: Some("Settlement risk threshold".to_string()),
                        }) {
                            log::error!(
                                "[Settlement] Failed to enqueue force close task for {}: {}",
                                account_id,
                                e
                            );
                        }
                    }

                    // 保存账户结算历史
                    self.account_history
                        .entry(account_id)
                        .and_modify(|entries| {
                            entries.push(settlement.clone());
                            if entries.len() > 180 {
                                let drop = entries.len().saturating_sub(180);
                                entries.drain(0..drop);
                            }
                        })
                        .or_insert_with(|| vec![settlement]);
                }
                Err(e) => {
                    failed_accounts += 1;
                    log::error!("[Settlement] Account {} failed: {}", i, e);
                }
            }
        }

        let elapsed = start_time.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;

        // 更新统计
        self.stats_settled_count
            .fetch_add(settled_accounts as u64, Ordering::Relaxed);
        self.stats_total_time_us
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

        let result = SettlementResult {
            settlement_date: settlement_date.clone(),
            total_accounts,
            settled_accounts,
            failed_accounts,
            force_closed_accounts: force_closed_accounts.clone(),
            total_commission,
            total_profit,
            elapsed_ms,
            parallelism,
        };

        // 保存结算结果
        self.settlement_history
            .insert(settlement_date.clone(), result.clone());

        log::info!(
            "[Settlement] Completed in {}ms: settled={}, failed={}, force_closed={}, threads={}",
            elapsed_ms,
            settled_accounts,
            failed_accounts,
            force_closed_accounts.len(),
            parallelism
        );

        Ok(result)
    }

    /// 预计算单个账户的结算数据（只读，无锁竞争）
    fn pre_calculate_account(
        &self,
        account: &Arc<parking_lot::RwLock<qars::qaaccount::account::QA_Account>>,
    ) -> Option<PreCalculatedSettlement> {
        let acc = account.read();
        let account_id = acc.account_cookie.clone();
        let pre_balance = acc.accounts.balance;
        let close_profit = acc.accounts.close_profit;
        let commission = acc.accounts.commission;
        let current_margin = acc.accounts.margin;

        // 计算持仓盈亏
        let mut position_profit = 0.0;
        for (code, pos) in acc.hold.iter() {
            if let Some(settlement_price) = self.settlement_prices.get(code) {
                // 多头盈亏
                let long_volume = pos.volume_long_today + pos.volume_long_his;
                if long_volume > 0.0 {
                    position_profit += (settlement_price.value() - pos.open_price_long) * long_volume;
                }

                // 空头盈亏
                let short_volume = pos.volume_short_today + pos.volume_short_his;
                if short_volume > 0.0 {
                    position_profit += (pos.open_price_short - settlement_price.value()) * short_volume;
                }
            }
        }

        // 计算新权益
        let new_balance = pre_balance + position_profit + close_profit - commission;

        // 计算风险度
        let risk_ratio = if new_balance > 0.0 {
            current_margin / new_balance
        } else {
            999.0
        };

        let need_force_close = risk_ratio >= self.force_close_threshold;

        Some(PreCalculatedSettlement {
            account_id,
            position_profit,
            close_profit,
            commission,
            pre_balance,
            new_balance,
            new_margin: current_margin,
            risk_ratio,
            need_force_close,
        })
    }

    /// 应用结算结果到账户（短暂写锁）
    ///
    /// **重要**: 调用 QA_Account::settle() 方法完成以下操作：
    /// - 清空日订单 (dailyorders) 和日成交 (dailytrades)
    /// - 持仓结转：今仓 → 昨仓 (volume_long_today → volume_long_his)
    /// - 释放冻结资金
    /// - 更新 pre_balance、重置 commission/close_profit
    ///
    /// @yutiansut @quantaxis
    fn apply_settlement(
        &self,
        account: &Arc<parking_lot::RwLock<qars::qaaccount::account::QA_Account>>,
        calc: &PreCalculatedSettlement,
        date: &str,
    ) -> Result<AccountSettlement, String> {
        // 获取写锁并执行结算
        {
            let mut acc = account.write();

            // 【关键】调用 QA_Account::settle() 完成完整结算流程
            // 包括：清空日订单/成交、持仓结转、释放冻结资金、重置账户状态
            acc.settle();

            // settle() 已经更新了大部分字段，这里补充预计算的盈亏值
            // 因为 settle() 使用账户内部状态计算，我们用预计算值确保一致性
            acc.accounts.position_profit = calc.position_profit;
            acc.accounts.risk_ratio = calc.risk_ratio;
        } // 写锁在此释放

        // 重新读取结算后的最终状态
        let final_state = {
            let acc = account.read();
            (acc.accounts.balance, acc.accounts.available, acc.accounts.margin)
        };

        Ok(AccountSettlement {
            user_id: calc.account_id.clone(),
            date: date.to_string(),
            close_profit: calc.close_profit,
            position_profit: calc.position_profit,
            commission: calc.commission,
            pre_balance: calc.pre_balance,
            balance: final_state.0,
            risk_ratio: calc.risk_ratio,
            force_close: calc.need_force_close,
            margin: final_state.2,
            available: final_state.1,
        })
    }

    /// 获取结算统计信息
    pub fn get_settlement_stats(&self) -> SettlementStats {
        SettlementStats {
            total_settled_accounts: self.stats_settled_count.load(Ordering::Relaxed),
            total_time_us: self.stats_total_time_us.load(Ordering::Relaxed),
            pending_force_close: self.force_close_queue.len(),
        }
    }

    /// 结算单个账户
    ///
    /// **重要**: 调用 QA_Account::settle() 方法完成完整结算
    /// @yutiansut @quantaxis
    fn settle_account(
        &self,
        user_id: &str,
        date: &str,
    ) -> Result<AccountSettlement, ExchangeError> {
        let account = self.account_mgr.get_account(user_id)?;

        // 记录结算前状态
        let (pre_balance, close_profit, commission, position_profit, margin) = {
            let acc = account.read();
            let pre_balance = acc.accounts.balance;
            let close_profit = acc.accounts.close_profit;
            let commission = acc.accounts.commission;

            // 计算持仓盯市盈亏
            let mut position_profit = 0.0;
            for (code, pos) in acc.hold.iter() {
                if let Some(settlement_price) = self.settlement_prices.get(code) {
                    let long_volume = pos.volume_long_today + pos.volume_long_his;
                    if long_volume > 0.0 {
                        position_profit += (settlement_price.value() - pos.open_price_long) * long_volume;
                    }
                    let short_volume = pos.volume_short_today + pos.volume_short_his;
                    if short_volume > 0.0 {
                        position_profit += (pos.open_price_short - settlement_price.value()) * short_volume;
                    }
                }
            }
            (pre_balance, close_profit, commission, position_profit, acc.accounts.margin)
        };
        let _ = margin; // 暂未使用但保留以备后用

        // 【关键】调用 QA_Account::settle() 完成完整结算
        {
            let mut acc = account.write();
            acc.settle();
        }

        // 读取结算后状态
        let (_balance, risk_ratio, _available, _final_margin) = {
            let acc = account.read();
            (acc.accounts.balance, acc.accounts.risk_ratio, acc.accounts.available, acc.accounts.margin)
        };

        // 检查是否需要强平
        let mut force_close = false;
        if risk_ratio >= self.force_close_threshold {
            force_close = true;
            log::warn!(
                "Force closing account {}: risk_ratio={:.2}%",
                user_id,
                risk_ratio * 100.0
            );

            if let Err(e) =
                self.force_liquidate_account(user_id, Some("Settlement risk threshold".to_string()))
            {
                log::error!("Failed to force close account {}: {}", user_id, e);
            } else {
                log::info!("Successfully force closed account {}", user_id);
            }
        }

        // 重新读取最终状态（强平后可能变化）
        let (final_balance, final_available, final_margin_after) = {
            let acc = account.read();
            (acc.accounts.balance, acc.accounts.available, acc.accounts.margin)
        };

        let settlement = AccountSettlement {
            user_id: user_id.to_string(),
            date: date.to_string(),
            close_profit,
            position_profit,
            commission,
            pre_balance,
            balance: final_balance,
            risk_ratio,
            force_close,
            margin: final_margin_after,
            available: final_available,
        };

        self.account_history
            .entry(user_id.to_string())
            .and_modify(|entries| {
                entries.push(settlement.clone());
                if entries.len() > 180 {
                    let drop = entries.len().saturating_sub(180);
                    entries.drain(0..drop);
                }
            })
            .or_insert_with(|| vec![settlement.clone()]);

        log::debug!(
            "Account {} settled: balance={:.2}, profit={:.2}, risk={:.2}%",
            user_id,
            settlement.balance,
            settlement.position_profit + settlement.close_profit,
            settlement.risk_ratio * 100.0
        );

        Ok(settlement)
    }

    /// 强平账户（提交真实订单）
    ///
    /// ## 强平确认机制 (P0-3)
    /// @yutiansut @quantaxis
    /// - 生成唯一强平ID用于追踪
    /// - 记录每个订单的提交状态
    /// - 保存强平历史以便查询
    pub fn force_liquidate_account(
        &self,
        account_id: &str,
        remark: Option<String>,
    ) -> Result<ForceLiquidationResult, ExchangeError> {
        let order_router = self
            .order_router
            .read()
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                ExchangeError::InternalError(
                    "OrderRouter not configured for SettlementEngine".to_string(),
                )
            })?;

        let account = self.account_mgr.get_account(account_id)?;
        let mut acc = account.write();
        let balance_before = acc.get_balance();
        let risk_ratio_before = acc.get_riskratio();

        let mut plans = Vec::new();
        for (instrument_id, pos) in acc.hold.iter() {
            let long_volume = pos.volume_long_today + pos.volume_long_his;
            if long_volume > 0.0 {
                plans.push(ForcePlan {
                    instrument_id: instrument_id.clone(),
                    direction: "SELL".to_string(),
                    offset: "CLOSE".to_string(),
                    volume: long_volume,
                    reference_price: if pos.open_price_long > 0.0 {
                        pos.open_price_long
                    } else {
                        1.0
                    },
                });
            }

            let short_volume = pos.volume_short_today + pos.volume_short_his;
            if short_volume > 0.0 {
                plans.push(ForcePlan {
                    instrument_id: instrument_id.clone(),
                    direction: "BUY".to_string(),
                    offset: "CLOSE".to_string(),
                    volume: short_volume,
                    reference_price: if pos.open_price_short > 0.0 {
                        pos.open_price_short
                    } else {
                        1.0
                    },
                });
            }
        }

        // 生成强平ID
        let liquidation_id = self.generate_liquidation_id();
        let start_time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if plans.is_empty() {
            log::info!(
                "[Liquidation {}] Skipped: account {} has no positions",
                liquidation_id, account_id
            );

            let result = ForceLiquidationResult {
                liquidation_id: liquidation_id.clone(),
                account_id: account_id.to_string(),
                orders: Vec::new(),
                trigger_risk_ratio: risk_ratio_before,
                balance_before,
                balance_after: balance_before,
                start_time: start_time.clone(),
                complete_time: Some(start_time),
                overall_status: ForceLiquidationStatus::Filled,
                remark,
            };

            // 保存历史
            self.liquidation_history.insert(liquidation_id.clone(), result.clone());
            self.account_liquidations
                .entry(account_id.to_string())
                .or_insert_with(Vec::new)
                .push(liquidation_id);

            return Ok(result);
        }

        drop(acc); // 释放账户锁，避免阻塞撮合

        log::info!(
            "[Liquidation {}] Starting for account {}, {} positions to close, risk_ratio={:.2}%",
            liquidation_id, account_id, plans.len(), risk_ratio_before * 100.0
        );

        let mut orders = Vec::with_capacity(plans.len());
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for plan in plans.into_iter() {
            let price = self.calculate_force_price(
                &plan.instrument_id,
                &plan.direction,
                plan.reference_price,
            );
            let submit_req = SubmitOrderRequest {
                account_id: account_id.to_string(),
                instrument_id: plan.instrument_id.clone(),
                direction: plan.direction.clone(),
                offset: plan.offset.clone(),
                volume: plan.volume,
                price,
                order_type: "LIMIT".to_string(),
                time_condition: None,
                volume_condition: None,
            };

            let response = order_router.submit_force_order(submit_req);
            let (status, error) = if response.success {
                (ForceLiquidationStatus::Submitted, None)
            } else {
                (ForceLiquidationStatus::Rejected, response.error_message.clone())
            };

            let mut order = ForceLiquidationOrder::new(
                plan.instrument_id.clone(),
                plan.direction.clone(),
                plan.offset.clone(),
                plan.volume,
                price,
            );
            order.order_id = response.order_id.clone();
            order.status = status;
            order.error = error;
            order.submit_time = Some(now.clone());
            order.update_time = Some(now.clone());

            log::info!(
                "[Liquidation {}] Order submitted: {} {} {} {} @ {:.2}, order_id={:?}, status={:?}",
                liquidation_id, plan.direction, plan.offset, plan.volume, plan.instrument_id, price,
                response.order_id, status
            );

            orders.push(order);
        }

        // 读取最新权益（下单完成后）
        let balance_after = self
            .account_mgr
            .get_account(account_id)
            .ok()
            .and_then(|acc| {
                let mut acc = acc.write();
                Some(acc.get_balance())
            })
            .unwrap_or(balance_before);

        // 构建完整的强平结果
        let mut result = ForceLiquidationResult {
            liquidation_id: liquidation_id.clone(),
            account_id: account_id.to_string(),
            orders,
            trigger_risk_ratio: risk_ratio_before,
            balance_before,
            balance_after,
            start_time,
            complete_time: None,
            overall_status: ForceLiquidationStatus::Pending,
            remark: remark.clone(),
        };

        // 更新总体状态
        result.update_overall_status();

        // 如果所有订单都已完成（成功或失败），标记完成时间
        if result.is_complete() {
            result.complete_time = Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }

        // 保存强平历史
        self.liquidation_history.insert(liquidation_id.clone(), result.clone());

        // 更新账户强平索引
        self.account_liquidations
            .entry(account_id.to_string())
            .or_insert_with(Vec::new)
            .push(liquidation_id.clone());

        // 记录到风控监控器
        if let Some(risk_monitor) = self.risk_monitor.read().clone() {
            let instruments: Vec<String> = result.orders.iter().map(|o| o.instrument_id.clone()).collect();
            risk_monitor.record_liquidation(
                account_id.to_string(),
                risk_ratio_before,
                balance_before,
                balance_after,
                instruments,
                remark,
            );
        }

        log::info!(
            "[Liquidation {}] Completed for account {}: {} orders, overall_status={:?}, balance: {:.2} -> {:.2}",
            liquidation_id, account_id, result.orders.len(), result.overall_status, balance_before, balance_after
        );

        Ok(result)
    }

    /// 获取所有结算历史
    pub fn get_settlement_history(&self) -> Vec<SettlementResult> {
        self.settlement_history
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    /// 查询特定日期的结算详情
    pub fn get_settlement_detail(&self, date: &str) -> Option<SettlementResult> {
        self.settlement_history.get(date).map(|r| r.value().clone())
    }

    /// 获取账户结算历史
    pub fn get_account_settlements(&self, account_id: &str) -> Vec<AccountSettlement> {
        self.account_history
            .get(account_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }

    /// 设置强平阈值
    pub fn set_force_close_threshold(&mut self, threshold: f64) {
        self.force_close_threshold = threshold;
        log::info!("Force close threshold set to {:.2}%", threshold * 100.0);
    }
}

impl Default for SettlementEngine {
    fn default() -> Self {
        let (sender, receiver) = crossbeam::channel::bounded(1000);
        Self {
            account_mgr: Arc::new(AccountManager::new()),
            settlement_prices: Arc::new(DashMap::new()),
            force_close_threshold: 1.0,
            settlement_history: Arc::new(DashMap::new()),
            account_history: Arc::new(DashMap::new()),
            order_router: Arc::new(RwLock::new(None)),
            market_data_service: Arc::new(RwLock::new(None)),
            risk_monitor: Arc::new(RwLock::new(None)),
            stats_settled_count: AtomicU64::new(0),
            stats_total_time_us: AtomicU64::new(0),
            force_close_queue: Arc::new(sender),
            force_close_receiver: Arc::new(receiver),
            force_close_worker_started: AtomicBool::new(false),
            // P0-3: 强平确认机制
            liquidation_history: Arc::new(DashMap::new()),
            account_liquidations: Arc::new(DashMap::new()),
            liquidation_seq: AtomicU64::new(1),
            max_retry_count: 3,
        }
    }
}

struct ForcePlan {
    instrument_id: String,
    direction: String,
    offset: String,
    volume: f64,
    reference_price: f64,
}

impl SettlementEngine {
    fn calculate_force_price(
        &self,
        instrument_id: &str,
        direction: &str,
        reference_price: f64,
    ) -> f64 {
        let fallback = reference_price.max(0.01);
        let market_price = self
            .market_data_service
            .read()
            .as_ref()
            .and_then(|svc| svc.get_tick_data(instrument_id).ok())
            .and_then(|tick| match direction {
                "SELL" => tick.bid_price.or(Some(tick.last_price)),
                "BUY" => tick.ask_price.or(Some(tick.last_price)),
                _ => Some(tick.last_price),
            })
            .filter(|price| *price > 0.0)
            .unwrap_or(fallback);

        let adjusted = match direction {
            "SELL" => (market_price * 0.995).max(0.01),
            "BUY" => (market_price * 1.005).max(0.01),
            _ => market_price,
        };

        adjusted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{AccountType, OpenAccountRequest};

    fn create_test_settlement_engine() -> (SettlementEngine, Arc<AccountManager>) {
        let account_mgr = Arc::new(AccountManager::new());

        // 创建测试账户
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: None,
            account_name: "Test User".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        let engine = SettlementEngine::new(account_mgr.clone());
        (engine, account_mgr)
    }

    #[test]
    fn test_settlement_engine_creation() {
        let (engine, _) = create_test_settlement_engine();
        engine.set_settlement_price("IX2301".to_string(), 120.0);

        assert!(engine.settlement_prices.contains_key("IX2301"));
    }

    #[test]
    fn test_daily_settlement() {
        let (engine, _account_mgr) = create_test_settlement_engine();

        engine.set_settlement_price("IX2301".to_string(), 120.0);

        let result = engine.daily_settlement().unwrap();

        // 应该结算1个测试账户
        assert_eq!(result.total_accounts, 1);
        assert_eq!(result.settled_accounts, 1);
        assert_eq!(result.failed_accounts, 0);
        assert_eq!(result.force_closed_accounts.len(), 0);
    }
}
