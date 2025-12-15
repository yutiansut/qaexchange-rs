//! 风险监控模块
//!
//! 负责实时监控账户风险状态、保证金使用情况、强平记录等
//!
//! ## 盘中风控监控 (Phase P0-2)
//! @yutiansut @quantaxis
//!
//! - **实时监控循环**: 后台线程持续监控所有账户风险
//! - **风险预警**: 达到阈值时自动告警
//! - **自动强平触发**: 风险超限时自动触发强平流程

use crate::exchange::AccountManager;
use crate::ExchangeError;
use chrono::Local;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// 风险预警事件
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlert {
    /// 事件ID
    pub alert_id: String,
    /// 账户ID
    pub account_id: String,
    /// 预警类型
    pub alert_type: RiskAlertType,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 当前风险率
    pub risk_ratio: f64,
    /// 预警时间
    pub alert_time: String,
    /// 预警消息
    pub message: String,
    /// 是否已处理
    pub handled: bool,
}

/// 风险预警类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskAlertType {
    /// 风险等级提升
    LevelEscalation,
    /// 临近强平线
    NearLiquidation,
    /// 触发强平
    LiquidationTriggered,
    /// 保证金不足
    MarginInsufficient,
    /// 可用资金为负
    NegativeAvailable,
}

/// 盘中风控配置
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMonitorConfig {
    /// 监控间隔（毫秒）
    pub monitor_interval_ms: u64,
    /// 预警阈值（风险率）
    pub warning_threshold: f64,
    /// 强平阈值（风险率）
    pub liquidation_threshold: f64,
    /// 是否启用自动强平
    pub auto_liquidation_enabled: bool,
    /// 预警保留数量
    pub max_alerts_per_account: usize,
}

impl Default for RiskMonitorConfig {
    fn default() -> Self {
        Self {
            monitor_interval_ms: 1000, // 1秒检查一次
            warning_threshold: 0.80,   // 80% 开始预警
            liquidation_threshold: 1.0, // 100% 触发强平
            auto_liquidation_enabled: true,
            max_alerts_per_account: 100,
        }
    }
}

/// 监控统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorStats {
    /// 总监控次数
    pub total_checks: u64,
    /// 发现的高风险账户次数
    pub high_risk_detected: u64,
    /// 触发的强平次数
    pub liquidations_triggered: u64,
    /// 发出的预警次数
    pub alerts_sent: u64,
    /// 最后一次检查时间
    pub last_check_time: Option<String>,
    /// 平均检查耗时（微秒）
    pub avg_check_duration_us: u64,
}

/// 强平回调函数类型
pub type LiquidationCallback = Arc<dyn Fn(&str, f64) + Send + Sync>;

/// 风险监控器
/// @yutiansut @quantaxis
pub struct RiskMonitor {
    account_mgr: Arc<AccountManager>,
    /// 强平记录 (user_id -> Vec<LiquidationRecord>)
    liquidation_records: DashMap<String, Vec<LiquidationRecord>>,
    /// 强平序列号
    liquidation_seq: AtomicU64,

    // ========== 盘中监控扩展 ==========
    /// 风险预警记录 (account_id -> Vec<RiskAlert>)
    risk_alerts: DashMap<String, Vec<RiskAlert>>,
    /// 预警序列号
    alert_seq: AtomicU64,
    /// 监控配置
    config: RwLock<RiskMonitorConfig>,
    /// 监控统计
    stats: RwLock<MonitorStats>,
    /// 上次各账户风险等级 (用于检测等级变化)
    last_risk_levels: DashMap<String, RiskLevel>,
    /// 监控线程是否运行
    monitor_running: AtomicBool,
    /// 强平回调（可选，用于触发外部强平流程）
    liquidation_callback: RwLock<Option<LiquidationCallback>>,
}

impl RiskMonitor {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            liquidation_records: DashMap::new(),
            liquidation_seq: AtomicU64::new(1),
            risk_alerts: DashMap::new(),
            alert_seq: AtomicU64::new(1),
            config: RwLock::new(RiskMonitorConfig::default()),
            stats: RwLock::new(MonitorStats::default()),
            last_risk_levels: DashMap::new(),
            monitor_running: AtomicBool::new(false),
            liquidation_callback: RwLock::new(None),
        }
    }

    /// 设置强平回调
    pub fn set_liquidation_callback(&self, callback: LiquidationCallback) {
        *self.liquidation_callback.write() = Some(callback);
    }

    /// 更新监控配置
    pub fn update_config(&self, config: RiskMonitorConfig) {
        log::info!("[RiskMonitor] Config updated: interval={}ms, warning={:.0}%, liquidation={:.0}%",
            config.monitor_interval_ms,
            config.warning_threshold * 100.0,
            config.liquidation_threshold * 100.0
        );
        *self.config.write() = config;
    }

    /// 获取当前配置
    pub fn get_config(&self) -> RiskMonitorConfig {
        self.config.read().clone()
    }

    /// 获取监控统计
    pub fn get_monitor_stats(&self) -> MonitorStats {
        self.stats.read().clone()
    }

    /// 启动盘中风控监控
    ///
    /// 启动后台线程，定期扫描所有账户的风险状态
    pub fn start_monitoring(self: &Arc<Self>) {
        if self.monitor_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            log::warn!("[RiskMonitor] Monitoring already running");
            return;
        }

        let monitor = Arc::clone(self);
        std::thread::spawn(move || {
            log::info!("[RiskMonitor] Intra-day risk monitoring started");

            while monitor.monitor_running.load(Ordering::SeqCst) {
                let interval = {
                    let config = monitor.config.read();
                    Duration::from_millis(config.monitor_interval_ms)
                };

                // 执行一次风险检查
                monitor.do_risk_check();

                // 等待下一次检查
                std::thread::sleep(interval);
            }

            log::info!("[RiskMonitor] Intra-day risk monitoring stopped");
        });
    }

    /// 停止盘中监控
    pub fn stop_monitoring(&self) {
        self.monitor_running.store(false, Ordering::SeqCst);
    }

    /// 是否正在监控
    pub fn is_monitoring(&self) -> bool {
        self.monitor_running.load(Ordering::SeqCst)
    }

    /// 执行一次风险检查
    fn do_risk_check(&self) {
        let start = Instant::now();
        let config = self.config.read().clone();
        let accounts = self.account_mgr.get_all_accounts();

        let mut high_risk_count = 0u64;
        let mut liquidation_count = 0u64;
        let mut alert_count = 0u64;

        for account in accounts.iter() {
            let mut acc = account.write();
            let account_id = acc.account_cookie.clone();
            let risk_ratio = acc.get_riskratio();
            let available = acc.money;
            let current_level = RiskLevel::from_risk_ratio(risk_ratio);

            // 检测风险等级变化
            let last_level = self.last_risk_levels
                .get(&account_id)
                .map(|l| *l)
                .unwrap_or(RiskLevel::Low);

            // 等级上升时发出预警
            if self.is_level_escalation(last_level, current_level) {
                self.create_alert(
                    &account_id,
                    RiskAlertType::LevelEscalation,
                    current_level,
                    risk_ratio,
                    format!("风险等级从 {:?} 升级到 {:?}", last_level, current_level),
                );
                alert_count += 1;
            }

            // 更新最后风险等级
            self.last_risk_levels.insert(account_id.clone(), current_level);

            // 高风险检测
            if matches!(current_level, RiskLevel::High | RiskLevel::Critical) {
                high_risk_count += 1;
            }

            // 可用资金为负检测
            if available < 0.0 {
                self.create_alert(
                    &account_id,
                    RiskAlertType::NegativeAvailable,
                    current_level,
                    risk_ratio,
                    format!("可用资金为负: {:.2}", available),
                );
                alert_count += 1;
            }

            // 临近强平线检测 (95% ~ 100%)
            if risk_ratio >= 0.95 && risk_ratio < config.liquidation_threshold {
                self.create_alert(
                    &account_id,
                    RiskAlertType::NearLiquidation,
                    current_level,
                    risk_ratio,
                    format!("临近强平线，风险率: {:.2}%", risk_ratio * 100.0),
                );
                alert_count += 1;
            }

            // 触发强平检测
            if risk_ratio >= config.liquidation_threshold && config.auto_liquidation_enabled {
                self.create_alert(
                    &account_id,
                    RiskAlertType::LiquidationTriggered,
                    current_level,
                    risk_ratio,
                    format!("触发强平，风险率: {:.2}%", risk_ratio * 100.0),
                );
                alert_count += 1;
                liquidation_count += 1;

                // 调用强平回调
                if let Some(ref callback) = *self.liquidation_callback.read() {
                    log::warn!("[RiskMonitor] Triggering liquidation for account {}, risk_ratio={:.2}%",
                        account_id, risk_ratio * 100.0);
                    callback(&account_id, risk_ratio);
                }
            }
        }

        // 更新统计
        let elapsed = start.elapsed();
        {
            let mut stats = self.stats.write();
            stats.total_checks += 1;
            stats.high_risk_detected += high_risk_count;
            stats.liquidations_triggered += liquidation_count;
            stats.alerts_sent += alert_count;
            stats.last_check_time = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

            // 更新平均耗时
            let new_duration = elapsed.as_micros() as u64;
            stats.avg_check_duration_us = if stats.total_checks == 1 {
                new_duration
            } else {
                (stats.avg_check_duration_us * (stats.total_checks - 1) + new_duration) / stats.total_checks
            };
        }

        if high_risk_count > 0 || liquidation_count > 0 {
            log::info!("[RiskMonitor] Check completed: high_risk={}, liquidations={}, alerts={}, elapsed={:?}",
                high_risk_count, liquidation_count, alert_count, elapsed);
        }
    }

    /// 判断风险等级是否上升
    fn is_level_escalation(&self, old: RiskLevel, new: RiskLevel) -> bool {
        let level_value = |l: RiskLevel| match l {
            RiskLevel::Low => 0,
            RiskLevel::Medium => 1,
            RiskLevel::High => 2,
            RiskLevel::Critical => 3,
        };
        level_value(new) > level_value(old)
    }

    /// 创建风险预警
    fn create_alert(
        &self,
        account_id: &str,
        alert_type: RiskAlertType,
        risk_level: RiskLevel,
        risk_ratio: f64,
        message: String,
    ) -> RiskAlert {
        let seq = self.alert_seq.fetch_add(1, Ordering::SeqCst);
        let now = Local::now();
        let config = self.config.read();

        let alert = RiskAlert {
            alert_id: format!("ALERT{}{:08}", now.format("%Y%m%d"), seq),
            account_id: account_id.to_string(),
            alert_type,
            risk_level,
            risk_ratio,
            alert_time: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            message,
            handled: false,
        };

        // 保存预警
        self.risk_alerts
            .entry(account_id.to_string())
            .and_modify(|alerts| {
                alerts.push(alert.clone());
                // 保留最近的N条
                if alerts.len() > config.max_alerts_per_account {
                    let drop = alerts.len() - config.max_alerts_per_account;
                    alerts.drain(0..drop);
                }
            })
            .or_insert_with(|| vec![alert.clone()]);

        log::warn!("[RiskAlert] {} - {} ({}): {}",
            alert.alert_id, account_id, format!("{:?}", alert_type), alert.message);

        alert
    }

    /// 获取账户的风险预警
    pub fn get_risk_alerts(&self, account_id: &str) -> Vec<RiskAlert> {
        self.risk_alerts
            .get(account_id)
            .map(|alerts| alerts.value().clone())
            .unwrap_or_default()
    }

    /// 获取所有未处理的风险预警
    pub fn get_pending_alerts(&self) -> Vec<RiskAlert> {
        self.risk_alerts
            .iter()
            .flat_map(|entry| entry.value().clone())
            .filter(|alert| !alert.handled)
            .collect()
    }

    /// 标记预警为已处理
    pub fn mark_alert_handled(&self, alert_id: &str) -> bool {
        for mut entry in self.risk_alerts.iter_mut() {
            for alert in entry.value_mut().iter_mut() {
                if alert.alert_id == alert_id {
                    alert.handled = true;
                    return true;
                }
            }
        }
        false
    }

    /// 清除账户的所有预警
    pub fn clear_alerts(&self, account_id: &str) {
        self.risk_alerts.remove(account_id);
    }

    /// 获取所有风险账户
    pub fn get_risk_accounts(&self, risk_level_filter: Option<RiskLevel>) -> Vec<RiskAccount> {
        let accounts = self.account_mgr.get_all_accounts();

        accounts
            .iter()
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

        let high_risk_count = risk_accounts
            .iter()
            .filter(|acc| matches!(acc.risk_level, RiskLevel::High | RiskLevel::Critical))
            .count();

        let critical_risk_count = risk_accounts
            .iter()
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
        let seq = self
            .liquidation_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

        log::warn!(
            "Liquidation recorded: user={}, loss={}, record_id={}",
            user_id,
            record.total_loss,
            record.record_id
        );

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
    use crate::core::account_ext::{AccountType, OpenAccountRequest};
    use crate::exchange::AccountManager;

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
            account_id: Some("test_user".to_string()), // 使用固定account_id
            account_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };
        let account_id = account_mgr.open_account(req).unwrap();
        assert_eq!(account_id, "test_user");

        // 获取风险账户
        let risk_accounts = risk_monitor.get_risk_accounts(None);
        assert_eq!(risk_accounts.len(), 1);
        assert_eq!(risk_accounts[0].user_id, "test_user"); // user_id字段在RiskAccountInfo中实际存储的是account_id
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
