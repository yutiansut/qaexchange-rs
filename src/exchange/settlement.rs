//! 结算系统
//!
//! 负责日终结算、盯市盈亏计算、强平处理等

use std::sync::Arc;
use std::collections::HashMap;
use dashmap::DashMap;
use chrono::{Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use log;

use super::AccountManager;
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
    pub close_profit: f64,      // 平仓盈亏
    pub position_profit: f64,   // 持仓盈亏
    pub commission: f64,        // 手续费
    pub pre_balance: f64,       // 结算前权益
    pub balance: f64,           // 结算后权益
    pub risk_ratio: f64,        // 风险度
    pub force_close: bool,      // 是否强平
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
}

impl SettlementEngine {
    /// 创建结算引擎
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            settlement_prices: Arc::new(DashMap::new()),
            force_close_threshold: 1.0, // 风险度 >= 100% 强平
            settlement_history: Arc::new(DashMap::new()),
        }
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
        log::info!("Settlement prices set: {} instruments", self.settlement_prices.len());
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
        self.settlement_history.insert(settlement_date, result.clone());

        log::info!("Daily settlement completed: settled={}, failed={}, force_closed={}",
            settled_accounts, failed_accounts, result.force_closed_accounts.len());

        Ok(result)
    }

    /// 结算单个账户
    fn settle_account(&self, user_id: &str, date: &str) -> Result<AccountSettlement, ExchangeError> {
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
                    let long_profit = (settlement_price.value() - pos.open_price_long) * long_volume;
                    position_profit += long_profit;
                }

                // 空头盈亏
                let short_volume = pos.volume_short_today + pos.volume_short_his;
                if short_volume > 0.0 {
                    let short_profit = (pos.open_price_short - settlement_price.value()) * short_volume;
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
        acc.accounts.available = acc.money;  // 同步更新 QIFI 协议字段

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
            log::warn!("Force closing account {}: risk_ratio={:.2}%", user_id, risk_ratio * 100.0);

            // 执行强平逻辑：清空所有持仓
            // 注意：实际生产环境应该通过 OrderRouter 提交市价单平仓
            // 这里采用简化方案：直接清空持仓（适用于模拟交易）
            drop(acc); // 释放写锁
            drop(account); // 释放账户引用

            if let Err(e) = self.force_close_account(user_id) {
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

        log::debug!("Account {} settled: balance={:.2}, profit={:.2}, risk={:.2}%",
            user_id, settlement.balance, settlement.position_profit + settlement.close_profit,
            settlement.risk_ratio * 100.0);

        Ok(settlement)
    }

    /// 强平账户
    pub fn force_close_account(&self, user_id: &str) -> Result<(), ExchangeError> {
        let account = self.account_mgr.get_account(user_id)?;
        let mut acc = account.write();

        // 清空所有持仓
        acc.hold.clear();
        acc.accounts.margin = 0.0;
        acc.money = acc.accounts.balance;
        acc.accounts.available = acc.money;  // 同步更新 QIFI 协议字段

        log::warn!("Force closed account {}: all positions cleared", user_id);

        Ok(())
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{OpenAccountRequest, AccountType};

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
