//! 结算系统
//!
//! 负责日终结算、盯市盈亏计算、强平处理等

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use chrono::Utc;
use dashmap::DashMap;
use log;
use parking_lot::RwLock;
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

/// 强平订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceLiquidationOrder {
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub order_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

/// 强平执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceLiquidationResult {
    pub account_id: String,
    pub orders: Vec<ForceLiquidationOrder>,
}

/// 结算引擎
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
    order_router: RwLock<Option<Weak<OrderRouter>>>,

    /// 市场数据服务（用于获取强平价格参考）
    market_data_service: RwLock<Option<Arc<MarketDataService>>>,

    /// 风险监控器（记录强平）
    risk_monitor: RwLock<Option<Arc<RiskMonitor>>>,
}

impl SettlementEngine {
    /// 创建结算引擎
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            settlement_prices: Arc::new(DashMap::new()),
            force_close_threshold: 1.0, // 风险度 >= 100% 强平
            settlement_history: Arc::new(DashMap::new()),
            account_history: Arc::new(DashMap::new()),
            order_router: RwLock::new(None),
            market_data_service: RwLock::new(None),
            risk_monitor: RwLock::new(None),
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

    /// 执行日终结算
    pub fn daily_settlement(&self) -> Result<SettlementResult, ExchangeError> {
        let settlement_date = Utc::now().format("%Y-%m-%d").to_string();
        log::info!("Starting daily settlement for {}", settlement_date);

        // 获取所有账户
        let accounts = self.account_mgr.get_all_accounts();
        let total_accounts = accounts.len();
        let mut settled_accounts = 0;
        let mut failed_accounts = 0;
        let mut force_closed_accounts: Vec<String> = Vec::new();
        let mut total_commission = 0.0;
        let mut total_profit = 0.0;

        // 遍历所有账户进行结算
        for account in accounts.iter() {
            let user_id = {
                let acc = account.read();
                acc.account_cookie.clone()
            };

            match self.settle_account(&user_id, &settlement_date) {
                Ok(settlement) => {
                    settled_accounts += 1;
                    total_commission += settlement.commission;
                    total_profit += settlement.close_profit + settlement.position_profit;

                    if settlement.force_close {
                        force_closed_accounts.push(user_id.to_string());
                    }
                }
                Err(e) => {
                    failed_accounts += 1;
                    log::error!("Failed to settle account {}: {:?}", user_id, e);
                }
            }
        }

        let result = SettlementResult {
            settlement_date: settlement_date.clone(),
            total_accounts,
            settled_accounts,
            failed_accounts,
            force_closed_accounts,
            total_commission,
            total_profit,
        };

        // 保存结算结果
        self.settlement_history
            .insert(settlement_date, result.clone());

        log::info!(
            "Daily settlement completed: settled={}, failed={}, force_closed={}",
            settled_accounts,
            failed_accounts,
            result.force_closed_accounts.len()
        );

        Ok(result)
    }

    /// 结算单个账户
    fn settle_account(
        &self,
        user_id: &str,
        date: &str,
    ) -> Result<AccountSettlement, ExchangeError> {
        let mut account = self.account_mgr.get_account(user_id)?;
        let mut acc = account.write();

        // 记录结算前权益
        let pre_balance = acc.accounts.balance;

        // 1. 计算持仓盈亏（盯市）
        let mut position_profit = 0.0;
        for (code, pos) in acc.hold.iter() {
            if let Some(settlement_price) = self.settlement_prices.get(code) {
                // 多头盈亏
                let long_volume = pos.volume_long_today + pos.volume_long_his;
                if long_volume > 0.0 {
                    let long_profit =
                        (settlement_price.value() - pos.open_price_long) * long_volume;
                    position_profit += long_profit;
                }

                // 空头盈亏
                let short_volume = pos.volume_short_today + pos.volume_short_his;
                if short_volume > 0.0 {
                    let short_profit =
                        (pos.open_price_short - settlement_price.value()) * short_volume;
                    position_profit += short_profit;
                }
            }
        }

        // 2. 获取平仓盈亏
        let close_profit = acc.accounts.close_profit;

        // 3. 获取累计手续费（账户交易过程中已实时累计）
        let commission = acc.accounts.commission;

        // 4. 更新账户权益
        acc.accounts.balance = pre_balance + position_profit + close_profit - commission;
        acc.money = acc.accounts.balance - acc.accounts.margin;
        acc.accounts.available = acc.money; // 同步更新 QIFI 协议字段

        // 5. 计算风险度
        let risk_ratio = if acc.accounts.balance > 0.0 {
            acc.accounts.margin / acc.accounts.balance
        } else {
            999.0 // 资金为0或负数，风险极高
        };
        acc.accounts.risk_ratio = risk_ratio;

        // 6. 检查是否需要强平
        let mut force_close = false;
        if risk_ratio >= self.force_close_threshold {
            force_close = true;
            log::warn!(
                "Force closing account {}: risk_ratio={:.2}%",
                user_id,
                risk_ratio * 100.0
            );

            // 执行强平逻辑：清空所有持仓
            // 注意：实际生产环境应该通过 OrderRouter 提交市价单平仓
            // 这里采用简化方案：直接清空持仓（适用于模拟交易）
            drop(acc); // 释放写锁
            drop(account); // 释放账户引用

            if let Err(e) =
                self.force_liquidate_account(user_id, Some("Settlement risk threshold".to_string()))
            {
                log::error!("Failed to force close account {}: {}", user_id, e);
            } else {
                log::info!("Successfully force closed account {}", user_id);
            }

            // 重新获取账户引用（用于后续返回结算信息）
            account = self.account_mgr.get_account(user_id)?;
            acc = account.write();
        }

        let settlement = AccountSettlement {
            user_id: user_id.to_string(),
            date: date.to_string(),
            close_profit,
            position_profit,
            commission,
            pre_balance,
            balance: acc.accounts.balance,
            risk_ratio,
            force_close,
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

        if plans.is_empty() {
            log::info!(
                "Force liquidation skipped: account {} has no positions",
                account_id
            );
            return Ok(ForceLiquidationResult {
                account_id: account_id.to_string(),
                orders: Vec::new(),
            });
        }

        drop(acc); // 释放账户锁，避免阻塞撮合

        let mut orders = Vec::with_capacity(plans.len());
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
            };

            let response = order_router.submit_force_order(submit_req);
            let (status, error) = if response.success {
                (
                    response.status.unwrap_or_else(|| "submitted".to_string()),
                    None,
                )
            } else {
                (
                    response.status.unwrap_or_else(|| "rejected".to_string()),
                    response.error_message.clone(),
                )
            };

            orders.push(ForceLiquidationOrder {
                instrument_id: plan.instrument_id,
                direction: plan.direction,
                offset: plan.offset,
                volume: plan.volume,
                price,
                order_id: response.order_id,
                status,
                error,
            });
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

        if let Some(risk_monitor) = self.risk_monitor.read().clone() {
            let instruments: Vec<String> = orders.iter().map(|o| o.instrument_id.clone()).collect();
            risk_monitor.record_liquidation(
                account_id.to_string(),
                risk_ratio_before,
                balance_before,
                balance_after,
                instruments,
                remark,
            );
        }

        Ok(ForceLiquidationResult {
            account_id: account_id.to_string(),
            orders,
        })
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

    /// 设置强平阈值
    pub fn set_force_close_threshold(&mut self, threshold: f64) {
        self.force_close_threshold = threshold;
        log::info!("Force close threshold set to {:.2}%", threshold * 100.0);
    }
}

impl Default for SettlementEngine {
    fn default() -> Self {
        Self {
            account_mgr: Arc::new(AccountManager::new()),
            settlement_prices: Arc::new(DashMap::new()),
            force_close_threshold: 1.0,
            settlement_history: Arc::new(DashMap::new()),
            order_router: RwLock::new(None),
            market_data_service: RwLock::new(None),
            risk_monitor: RwLock::new(None),
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
