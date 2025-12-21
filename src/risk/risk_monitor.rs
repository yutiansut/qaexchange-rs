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
            .or_default()
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

    // ==================== RiskLevel 测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_risk_level_from_ratio() {
        assert_eq!(RiskLevel::from_risk_ratio(0.5), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(0.7), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_risk_ratio(0.85), RiskLevel::High);
        assert_eq!(RiskLevel::from_risk_ratio(0.96), RiskLevel::Critical);
    }

    /// 测试 RiskLevel 边界值
    #[test]
    fn test_risk_level_boundary_values() {
        // 边界值测试
        assert_eq!(RiskLevel::from_risk_ratio(0.0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(0.59), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(0.6), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_risk_ratio(0.79), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_risk_ratio(0.8), RiskLevel::High);
        assert_eq!(RiskLevel::from_risk_ratio(0.94), RiskLevel::High);
        assert_eq!(RiskLevel::from_risk_ratio(0.95), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_risk_ratio(1.0), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_risk_ratio(1.5), RiskLevel::Critical);
    }

    /// 测试负数风险率
    #[test]
    fn test_risk_level_negative_ratio() {
        assert_eq!(RiskLevel::from_risk_ratio(-0.1), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(-1.0), RiskLevel::Low);
    }

    // ==================== RiskMonitorConfig 测试 @yutiansut @quantaxis ====================

    /// 测试默认配置
    #[test]
    fn test_risk_monitor_config_default() {
        let config = RiskMonitorConfig::default();
        assert_eq!(config.monitor_interval_ms, 1000);
        assert_eq!(config.warning_threshold, 0.80);
        assert_eq!(config.liquidation_threshold, 1.0);
        assert!(config.auto_liquidation_enabled);
        assert_eq!(config.max_alerts_per_account, 100);
    }

    /// 测试自定义配置
    #[test]
    fn test_risk_monitor_config_custom() {
        let config = RiskMonitorConfig {
            monitor_interval_ms: 500,
            warning_threshold: 0.70,
            liquidation_threshold: 0.95,
            auto_liquidation_enabled: false,
            max_alerts_per_account: 50,
        };
        assert_eq!(config.monitor_interval_ms, 500);
        assert_eq!(config.warning_threshold, 0.70);
        assert_eq!(config.liquidation_threshold, 0.95);
        assert!(!config.auto_liquidation_enabled);
        assert_eq!(config.max_alerts_per_account, 50);
    }

    // ==================== RiskMonitor 基础测试 @yutiansut @quantaxis ====================

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

    /// 测试 RiskMonitor 创建
    #[test]
    fn test_risk_monitor_new() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        // 验证初始状态
        assert!(!monitor.is_monitoring());
        assert_eq!(monitor.get_total_liquidation_count(), 0);

        let stats = monitor.get_monitor_stats();
        assert_eq!(stats.total_checks, 0);
        assert_eq!(stats.alerts_sent, 0);
    }

    /// 测试配置更新和获取
    #[test]
    fn test_update_and_get_config() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 获取默认配置
        let default_config = monitor.get_config();
        assert_eq!(default_config.monitor_interval_ms, 1000);

        // 更新配置
        let new_config = RiskMonitorConfig {
            monitor_interval_ms: 2000,
            warning_threshold: 0.75,
            liquidation_threshold: 0.98,
            auto_liquidation_enabled: false,
            max_alerts_per_account: 200,
        };
        monitor.update_config(new_config.clone());

        // 验证更新后的配置
        let updated = monitor.get_config();
        assert_eq!(updated.monitor_interval_ms, 2000);
        assert_eq!(updated.warning_threshold, 0.75);
        assert_eq!(updated.liquidation_threshold, 0.98);
        assert!(!updated.auto_liquidation_enabled);
        assert_eq!(updated.max_alerts_per_account, 200);
    }

    // ==================== 强平回调测试 @yutiansut @quantaxis ====================

    /// 测试设置强平回调
    #[test]
    fn test_set_liquidation_callback() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let callback_count = Arc::new(AtomicU32::new(0));
        let callback_count_clone = callback_count.clone();

        // 设置回调
        monitor.set_liquidation_callback(Arc::new(move |_account_id, _risk_ratio| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // 验证回调已设置（通过内部状态）
        // 实际调用需要通过 do_risk_check
    }

    // ==================== 监控启停测试 @yutiansut @quantaxis ====================

    /// 测试监控启动和停止
    #[test]
    fn test_start_stop_monitoring() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = Arc::new(RiskMonitor::new(account_mgr));

        // 初始状态不在监控
        assert!(!monitor.is_monitoring());

        // 启动监控
        monitor.start_monitoring();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(monitor.is_monitoring());

        // 停止监控
        monitor.stop_monitoring();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!monitor.is_monitoring());
    }

    /// 测试重复启动监控
    #[test]
    fn test_duplicate_start_monitoring() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = Arc::new(RiskMonitor::new(account_mgr));

        // 第一次启动
        monitor.start_monitoring();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(monitor.is_monitoring());

        // 第二次启动（应被忽略）
        monitor.start_monitoring();
        assert!(monitor.is_monitoring());

        // 停止
        monitor.stop_monitoring();
    }

    // ==================== 风险预警测试 @yutiansut @quantaxis ====================

    /// 测试获取空预警列表
    #[test]
    fn test_get_risk_alerts_empty() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let alerts = monitor.get_risk_alerts("non_existent");
        assert!(alerts.is_empty());
    }

    /// 测试获取所有未处理预警（空）
    #[test]
    fn test_get_pending_alerts_empty() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let pending = monitor.get_pending_alerts();
        assert!(pending.is_empty());
    }

    /// 测试标记不存在的预警
    #[test]
    fn test_mark_nonexistent_alert_handled() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let result = monitor.mark_alert_handled("ALERT_NOT_EXIST");
        assert!(!result);
    }

    /// 测试清除预警
    #[test]
    fn test_clear_alerts() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 清除不存在账户的预警（应不报错）
        monitor.clear_alerts("non_existent");

        // 验证没有预警
        let alerts = monitor.get_risk_alerts("non_existent");
        assert!(alerts.is_empty());
    }

    // ==================== 风险账户查询测试 @yutiansut @quantaxis ====================

    /// 测试获取风险账户 - 按等级过滤
    #[test]
    fn test_get_risk_accounts_with_filter() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        // 创建账户
        let req = OpenAccountRequest {
            user_id: "filter_user".to_string(),
            account_id: Some("filter_user".to_string()),
            account_name: "Filter User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 过滤 Low 风险账户
        let low_risk = monitor.get_risk_accounts(Some(RiskLevel::Low));
        assert_eq!(low_risk.len(), 1);

        // 过滤 High 风险账户（应为空）
        let high_risk = monitor.get_risk_accounts(Some(RiskLevel::High));
        assert!(high_risk.is_empty());

        // 过滤 Critical 风险账户（应为空）
        let critical = monitor.get_risk_accounts(Some(RiskLevel::Critical));
        assert!(critical.is_empty());
    }

    /// 测试获取高风险账户
    #[test]
    fn test_get_high_risk_accounts() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        // 创建低风险账户
        let req = OpenAccountRequest {
            user_id: "low_risk".to_string(),
            account_id: Some("low_risk".to_string()),
            account_name: "Low Risk".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 没有高风险账户
        let high_risk = monitor.get_high_risk_accounts();
        assert!(high_risk.is_empty());
    }

    /// 测试获取临界风险账户
    #[test]
    fn test_get_critical_risk_accounts() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        // 创建账户
        let req = OpenAccountRequest {
            user_id: "normal".to_string(),
            account_id: Some("normal".to_string()),
            account_name: "Normal".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        // 没有临界风险账户
        let critical = monitor.get_critical_risk_accounts();
        assert!(critical.is_empty());
    }

    // ==================== 保证金汇总测试 @yutiansut @quantaxis ====================

    /// 测试空账户的保证金汇总
    #[test]
    fn test_margin_summary_empty() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let summary = monitor.get_margin_summary();
        assert_eq!(summary.total_accounts, 0);
        assert_eq!(summary.total_margin_used, 0.0);
        assert_eq!(summary.average_risk_ratio, 0.0);
        assert_eq!(summary.high_risk_count, 0);
        assert_eq!(summary.critical_risk_count, 0);
    }

    /// 测试多账户的保证金汇总
    #[test]
    fn test_margin_summary_multiple_accounts() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        // 创建多个账户
        for i in 0..3 {
            let req = OpenAccountRequest {
                user_id: format!("user_{}", i),
                account_id: Some(format!("user_{}", i)),
                account_name: format!("User {}", i),
                init_cash: 100000.0 * (i + 1) as f64,
                account_type: AccountType::Individual,
            };
            account_mgr.open_account(req).unwrap();
        }

        let summary = monitor.get_margin_summary();
        assert_eq!(summary.total_accounts, 3);
        // 所有账户都是低风险（无持仓）
        assert_eq!(summary.high_risk_count, 0);
    }

    // ==================== 强平记录测试 @yutiansut @quantaxis ====================

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

    /// 测试多次强平记录
    #[test]
    fn test_multiple_liquidation_records() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 记录多次强平
        let record1 = monitor.record_liquidation(
            "user1".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );

        let record2 = monitor.record_liquidation(
            "user1".to_string(),
            0.99,
            45000.0,
            40000.0,
            vec!["IC2501".to_string()],
            Some("二次强平".to_string()),
        );

        // 验证不同的 record_id
        assert_ne!(record1.record_id, record2.record_id);

        // 验证同一用户有两条记录
        let records = monitor.get_liquidation_records("user1");
        assert_eq!(records.len(), 2);
    }

    /// 测试获取不存在用户的强平记录
    #[test]
    fn test_get_liquidation_records_nonexistent_user() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let records = monitor.get_liquidation_records("not_exist");
        assert!(records.is_empty());
    }

    /// 测试获取所有强平记录
    #[test]
    fn test_get_all_liquidation_records() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 多用户强平记录
        monitor.record_liquidation(
            "user_a".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );

        monitor.record_liquidation(
            "user_b".to_string(),
            0.97,
            60000.0,
            55000.0,
            vec!["IC2501".to_string()],
            None,
        );

        let all_records = monitor.get_all_liquidation_records();
        assert_eq!(all_records.len(), 2);
    }

    /// 测试按日期范围获取强平记录
    #[test]
    fn test_get_liquidation_records_by_date_range() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 记录强平
        monitor.record_liquidation(
            "user1".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );

        // 使用当前日期范围查询
        let today = Local::now().format("%Y-%m-%d").to_string();
        let tomorrow = (Local::now() + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();

        let records = monitor.get_liquidation_records_by_date_range(&today, &tomorrow);
        assert_eq!(records.len(), 1);

        // 使用过去日期范围查询（应为空）
        let records_past = monitor.get_liquidation_records_by_date_range("2020-01-01", "2020-12-31");
        assert!(records_past.is_empty());
    }

    /// 测试强平记录总数
    #[test]
    fn test_get_total_liquidation_count() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 初始为0
        assert_eq!(monitor.get_total_liquidation_count(), 0);

        // 添加记录
        monitor.record_liquidation(
            "user1".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );
        assert_eq!(monitor.get_total_liquidation_count(), 1);

        monitor.record_liquidation(
            "user2".to_string(),
            0.97,
            60000.0,
            55000.0,
            vec!["IC2501".to_string()],
            None,
        );
        assert_eq!(monitor.get_total_liquidation_count(), 2);

        monitor.record_liquidation(
            "user1".to_string(),
            0.99,
            45000.0,
            40000.0,
            vec!["IM2501".to_string()],
            None,
        );
        assert_eq!(monitor.get_total_liquidation_count(), 3);
    }

    /// 测试强平记录无备注
    #[test]
    fn test_liquidation_record_without_remark() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let record = monitor.record_liquidation(
            "user1".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );

        assert!(record.remark.is_none());
    }

    /// 测试强平记录多合约
    #[test]
    fn test_liquidation_record_multiple_instruments() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let instruments = vec![
            "IF2501".to_string(),
            "IC2501".to_string(),
            "IH2501".to_string(),
        ];

        let record = monitor.record_liquidation(
            "user1".to_string(),
            0.99,
            100000.0,
            80000.0,
            instruments.clone(),
            Some("多合约强平".to_string()),
        );

        assert_eq!(record.instruments_closed.len(), 3);
        assert_eq!(record.total_loss, 20000.0);
    }

    // ==================== RiskAccount 结构体测试 @yutiansut @quantaxis ====================

    /// 测试 RiskAccount 字段正确性
    #[test]
    fn test_risk_account_fields() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr.clone());

        let req = OpenAccountRequest {
            user_id: "field_test".to_string(),
            account_id: Some("field_test".to_string()),
            account_name: "Field Test".to_string(),
            init_cash: 500000.0,
            account_type: AccountType::Individual,
        };
        account_mgr.open_account(req).unwrap();

        let accounts = monitor.get_risk_accounts(None);
        assert_eq!(accounts.len(), 1);

        let acc = &accounts[0];
        assert_eq!(acc.user_id, "field_test");
        assert!(acc.balance > 0.0);
        assert!(acc.available > 0.0);
        assert_eq!(acc.risk_level, RiskLevel::Low);
        assert_eq!(acc.position_count, 0);
    }

    // ==================== MonitorStats 测试 @yutiansut @quantaxis ====================

    /// 测试监控统计初始状态
    #[test]
    fn test_monitor_stats_initial() {
        let stats = MonitorStats::default();

        assert_eq!(stats.total_checks, 0);
        assert_eq!(stats.high_risk_detected, 0);
        assert_eq!(stats.liquidations_triggered, 0);
        assert_eq!(stats.alerts_sent, 0);
        assert!(stats.last_check_time.is_none());
        assert_eq!(stats.avg_check_duration_us, 0);
    }

    // ==================== RiskAlertType 测试 @yutiansut @quantaxis ====================

    /// 测试 RiskAlertType 枚举值
    #[test]
    fn test_risk_alert_type_values() {
        let level_escalation = RiskAlertType::LevelEscalation;
        let near_liquidation = RiskAlertType::NearLiquidation;
        let liquidation_triggered = RiskAlertType::LiquidationTriggered;
        let margin_insufficient = RiskAlertType::MarginInsufficient;
        let negative_available = RiskAlertType::NegativeAvailable;

        // 测试相等性
        assert_eq!(level_escalation, RiskAlertType::LevelEscalation);
        assert_ne!(level_escalation, RiskAlertType::NearLiquidation);
        assert_ne!(near_liquidation, liquidation_triggered);
        assert_ne!(margin_insufficient, negative_available);
    }

    // ==================== 等级升级判断测试 @yutiansut @quantaxis ====================

    /// 测试等级升级判断（通过监控流程间接测试）
    #[test]
    fn test_risk_level_comparison() {
        // Low -> Medium = 升级
        assert!(level_value(RiskLevel::Medium) > level_value(RiskLevel::Low));
        // Medium -> High = 升级
        assert!(level_value(RiskLevel::High) > level_value(RiskLevel::Medium));
        // High -> Critical = 升级
        assert!(level_value(RiskLevel::Critical) > level_value(RiskLevel::High));
        // Critical -> Low = 降级
        assert!(level_value(RiskLevel::Low) < level_value(RiskLevel::Critical));
    }

    fn level_value(l: RiskLevel) -> u8 {
        match l {
            RiskLevel::Low => 0,
            RiskLevel::Medium => 1,
            RiskLevel::High => 2,
            RiskLevel::Critical => 3,
        }
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发记录强平
    #[test]
    fn test_concurrent_liquidation_records() {
        use std::thread;

        let account_mgr = Arc::new(AccountManager::new());
        let monitor = Arc::new(RiskMonitor::new(account_mgr));

        let mut handles = vec![];

        for i in 0..10 {
            let monitor_clone = monitor.clone();
            handles.push(thread::spawn(move || {
                monitor_clone.record_liquidation(
                    format!("user_{}", i),
                    0.98,
                    50000.0,
                    45000.0,
                    vec![format!("IF25{:02}", i)],
                    None,
                );
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有记录都已创建
        assert_eq!(monitor.get_total_liquidation_count(), 10);
    }

    /// 测试并发获取风险账户
    #[test]
    fn test_concurrent_get_risk_accounts() {
        use std::thread;

        let account_mgr = Arc::new(AccountManager::new());

        // 创建多个账户
        for i in 0..5 {
            let req = OpenAccountRequest {
                user_id: format!("concurrent_{}", i),
                account_id: Some(format!("concurrent_{}", i)),
                account_name: format!("Concurrent {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            account_mgr.open_account(req).unwrap();
        }

        let monitor = Arc::new(RiskMonitor::new(account_mgr));

        let mut handles = vec![];
        for _ in 0..10 {
            let monitor_clone = monitor.clone();
            handles.push(thread::spawn(move || {
                let accounts = monitor_clone.get_risk_accounts(None);
                assert_eq!(accounts.len(), 5);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // ==================== LiquidationRecord 结构体测试 @yutiansut @quantaxis ====================

    /// 测试 LiquidationRecord 字段计算
    #[test]
    fn test_liquidation_record_total_loss_calculation() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        // 亏损情况
        let loss_record = monitor.record_liquidation(
            "loss_user".to_string(),
            0.98,
            100000.0,  // before
            80000.0,   // after
            vec!["IF2501".to_string()],
            None,
        );
        assert_eq!(loss_record.total_loss, 20000.0);

        // 盈利情况（理论上强平后余额增加不太可能，但数学上可以）
        let gain_record = monitor.record_liquidation(
            "gain_user".to_string(),
            0.98,
            80000.0,   // before
            85000.0,   // after (假设平仓后释放保证金)
            vec!["IC2501".to_string()],
            None,
        );
        assert_eq!(gain_record.total_loss, -5000.0); // 负值表示盈利
    }

    /// 测试 LiquidationRecord ID 格式
    #[test]
    fn test_liquidation_record_id_format() {
        let account_mgr = Arc::new(AccountManager::new());
        let monitor = RiskMonitor::new(account_mgr);

        let record = monitor.record_liquidation(
            "user1".to_string(),
            0.98,
            50000.0,
            45000.0,
            vec!["IF2501".to_string()],
            None,
        );

        // 验证 ID 格式: LIQ + 日期 + 序号
        assert!(record.record_id.starts_with("LIQ"));
        assert!(record.record_id.len() > 10);
    }

    // ==================== MarginSummary 测试 @yutiansut @quantaxis ====================

    /// 测试 MarginSummary 结构
    #[test]
    fn test_margin_summary_structure() {
        let summary = MarginSummary {
            total_accounts: 10,
            total_margin_used: 500000.0,
            total_available: 1000000.0,
            average_risk_ratio: 0.33,
            high_risk_count: 2,
            critical_risk_count: 1,
        };

        assert_eq!(summary.total_accounts, 10);
        assert_eq!(summary.total_margin_used, 500000.0);
        assert_eq!(summary.total_available, 1000000.0);
        assert_eq!(summary.average_risk_ratio, 0.33);
        assert_eq!(summary.high_risk_count, 2);
        assert_eq!(summary.critical_risk_count, 1);
    }
}
