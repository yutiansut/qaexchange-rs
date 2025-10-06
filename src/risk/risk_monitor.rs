//! 风险监控模块
//!
//! 负责实时监控账户风险状态、保证金使用情况、强平记录等

use crate::exchange::AccountManager;
use crate::ExchangeError;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::Local;
use dashmap::DashMap;

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// 低风险 (< 60%)
    Low,
    /// 中风险 (60-80%)
    Medium,
    /// 高风险 (80-95%)
    High,
    /// 临界风险 (>= 95%)
    Critical,
}

impl RiskLevel {
    /// 根据风险率判断风险等级
    pub fn from_risk_ratio(ratio: f64) -> Self {
        if ratio < 0.6 {
            RiskLevel::Low
        } else if ratio < 0.8 {
            RiskLevel::Medium
        } else if ratio < 0.95 {
            RiskLevel::High
        } else {
            RiskLevel::Critical
        }
    }
}

/// 风险账户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAccount {
    /// 用户ID
    pub user_id: String,
    /// 账户余额(总权益)
    pub balance: f64,
    /// 可用资金
    pub available: f64,
    /// 保证金占用
    pub margin_used: f64,
    /// 风险率
    pub risk_ratio: f64,
    /// 未实现盈亏
    pub unrealized_pnl: f64,
    /// 持仓数量
    pub position_count: usize,
    /// 风险等级
    pub risk_level: RiskLevel,
}

/// 强平记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationRecord {
    /// 记录ID
    pub record_id: String,
    /// 用户ID
    pub user_id: String,
    /// 强平时间
    pub liquidation_time: String,
    /// 强平前风险率
    pub risk_ratio_before: f64,
    /// 强平前余额
    pub balance_before: f64,
    /// 强平后余额
    pub balance_after: f64,
    /// 平仓损失
    pub total_loss: f64,
    /// 平仓合约列表
    pub instruments_closed: Vec<String>,
    /// 备注
    pub remark: Option<String>,
}

/// 保证金监控汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginSummary {
    /// 总账户数
    pub total_accounts: usize,
    /// 总保证金占用
    pub total_margin_used: f64,
    /// 总可用资金
    pub total_available: f64,
    /// 平均风险率
    pub average_risk_ratio: f64,
    /// 高风险账户数
    pub high_risk_count: usize,
    /// 临界风险账户数
    pub critical_risk_count: usize,
}

/// 风险监控器
pub struct RiskMonitor {
    account_mgr: Arc<AccountManager>,
    /// 强平记录 (user_id -> Vec<LiquidationRecord>)
    liquidation_records: DashMap<String, Vec<LiquidationRecord>>,
    /// 强平序列号
    liquidation_seq: std::sync::atomic::AtomicU64,
}

impl RiskMonitor {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            liquidation_records: DashMap::new(),
            liquidation_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 获取所有风险账户
    pub fn get_risk_accounts(&self, risk_level_filter: Option<RiskLevel>) -> Vec<RiskAccount> {
        let accounts = self.account_mgr.get_all_accounts();

        accounts.iter()
            .filter_map(|account| {
                let mut acc = account.write();

                // 获取账户实时指标
                let balance = acc.get_balance();
                let available = acc.money;
                let margin_used = acc.get_margin();
                let unrealized_pnl = acc.get_positionprofit();
                let risk_ratio = acc.get_riskratio();
                let position_count = acc.hold.len();
                let risk_level = RiskLevel::from_risk_ratio(risk_ratio);

                // 应用风险等级过滤
                if let Some(filter) = risk_level_filter {
                    if risk_level != filter {
                        return None;
                    }
                }

                Some(RiskAccount {
                    user_id: acc.account_cookie.clone(),
                    balance,
                    available,
                    margin_used,
                    risk_ratio,
                    unrealized_pnl,
                    position_count,
                    risk_level,
                })
            })
            .collect()
    }

    /// 获取高风险账户 (风险率 >= 80%)
    pub fn get_high_risk_accounts(&self) -> Vec<RiskAccount> {
        self.get_risk_accounts(None)
            .into_iter()
            .filter(|acc| matches!(acc.risk_level, RiskLevel::High | RiskLevel::Critical))
            .collect()
    }

    /// 获取临界风险账户 (风险率 >= 95%)
    pub fn get_critical_risk_accounts(&self) -> Vec<RiskAccount> {
        self.get_risk_accounts(Some(RiskLevel::Critical))
    }

    /// 获取保证金监控汇总
    pub fn get_margin_summary(&self) -> MarginSummary {
        let risk_accounts = self.get_risk_accounts(None);

        let total_accounts = risk_accounts.len();
        let total_margin_used: f64 = risk_accounts.iter().map(|acc| acc.margin_used).sum();
        let total_available: f64 = risk_accounts.iter().map(|acc| acc.available).sum();
        let average_risk_ratio = if total_accounts > 0 {
            risk_accounts.iter().map(|acc| acc.risk_ratio).sum::<f64>() / total_accounts as f64
        } else {
            0.0
        };

        let high_risk_count = risk_accounts.iter()
            .filter(|acc| matches!(acc.risk_level, RiskLevel::High | RiskLevel::Critical))
            .count();

        let critical_risk_count = risk_accounts.iter()
            .filter(|acc| matches!(acc.risk_level, RiskLevel::Critical))
            .count();

        MarginSummary {
            total_accounts,
            total_margin_used,
            total_available,
            average_risk_ratio,
            high_risk_count,
            critical_risk_count,
        }
    }

    /// 记录强平
    pub fn record_liquidation(
        &self,
        user_id: String,
        risk_ratio_before: f64,
        balance_before: f64,
        balance_after: f64,
        instruments_closed: Vec<String>,
        remark: Option<String>,
    ) -> LiquidationRecord {
        let seq = self.liquidation_seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let now = Local::now();

        let record = LiquidationRecord {
            record_id: format!("LIQ{}{:08}", now.format("%Y%m%d"), seq),
            user_id: user_id.clone(),
            liquidation_time: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            risk_ratio_before,
            balance_before,
            balance_after,
            total_loss: balance_before - balance_after,
            instruments_closed,
            remark,
        };

        // 保存记录
        self.liquidation_records
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(record.clone());

        log::warn!("Liquidation recorded: user={}, loss={}, record_id={}",
            user_id, record.total_loss, record.record_id);

        record
    }

    /// 获取用户的强平记录
    pub fn get_liquidation_records(&self, user_id: &str) -> Vec<LiquidationRecord> {
        self.liquidation_records
            .get(user_id)
            .map(|records| records.value().clone())
            .unwrap_or_default()
    }

    /// 获取所有强平记录
    pub fn get_all_liquidation_records(&self) -> Vec<LiquidationRecord> {
        self.liquidation_records
            .iter()
            .flat_map(|entry| entry.value().clone())
            .collect()
    }

    /// 根据日期范围获取强平记录
    pub fn get_liquidation_records_by_date_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Vec<LiquidationRecord> {
        self.get_all_liquidation_records()
            .into_iter()
            .filter(|record| {
                record.liquidation_time.as_str() >= start_date
                && record.liquidation_time.as_str() <= end_date
            })
            .collect()
    }

    /// 获取强平记录总数
    pub fn get_total_liquidation_count(&self) -> usize {
        self.liquidation_records
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::AccountManager;
    use crate::core::account_ext::{OpenAccountRequest, AccountType};

    #[test]
    fn test_risk_level_from_ratio() {
        assert_eq!(RiskLevel::from_risk_ratio(0.5), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(0.7), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_risk_ratio(0.85), RiskLevel::High);
        assert_eq!(RiskLevel::from_risk_ratio(0.96), RiskLevel::Critical);
    }

    #[test]
    fn test_risk_monitor() {
        // 创建账户管理器和风险监控器
        let account_mgr = Arc::new(AccountManager::new());
        let risk_monitor = RiskMonitor::new(account_mgr.clone());

        // 开户
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: None,
            account_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 获取风险账户
        let risk_accounts = risk_monitor.get_risk_accounts(None);
        assert_eq!(risk_accounts.len(), 1);
        assert_eq!(risk_accounts[0].user_id, "test_user");
        assert_eq!(risk_accounts[0].risk_level, RiskLevel::Low);

        // 获取保证金汇总
        let summary = risk_monitor.get_margin_summary();
        assert_eq!(summary.total_accounts, 1);
        assert_eq!(summary.high_risk_count, 0);
    }

    #[test]
    fn test_liquidation_record() {
        let account_mgr = Arc::new(AccountManager::new());
        let risk_monitor = RiskMonitor::new(account_mgr.clone());

        // 记录强平
        let record = risk_monitor.record_liquidation(
            "test_user".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            Some("高风险强平".to_string()),
        );

        assert_eq!(record.user_id, "test_user");
        assert_eq!(record.total_loss, 5000.0);
        assert_eq!(record.instruments_closed.len(), 1);

        // 查询强平记录
        let records = risk_monitor.get_liquidation_records("test_user");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_id, record.record_id);
    }
}
