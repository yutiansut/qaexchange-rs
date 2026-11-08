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
        // ⚠️ 非有限值必须先拦掉,否则全部掉进最后的 else = Critical。
        //    @yutiansut @quantaxis
        //
        // `risk_ratio = margin / balance`(qars `get_riskratio()`)。
        // 自动创建的默认账户 `init_cash = 0` ⇒ `0.0 / 0.0 = NaN`。
        // 而 NaN 参与 `<` 比较**恒为 false**,于是
        //   NaN < 0.6 → false;NaN < 0.8 → false;NaN < 0.95 → false → else → Critical
        // 实测线上 57 个账户里有 19 个是这种空账户,**全部被判为 Critical**,
        // 与 `high_risk_count = critical_risk_count = 19` 精确吻合;
        // 服务端日志里那批 `[RiskAlert] LevelEscalation` 也是它们发的。
        // 33% 的风控告警是假的 —— 会把真正危险的账户淹掉。
        //
        // 两种非有限要分开对待,不能一律当 Low:
        //   NaN  = 0/0    → 既无权益也无保证金,**没有风险敞口** → Low
        //   +Inf = x/0    → 有保证金却零权益,**是真的危险**   → Critical
        if ratio.is_nan() {
            return RiskLevel::Low;
        }
        if ratio.is_infinite() {
            return if ratio > 0.0 {
                RiskLevel::Critical
            } else {
                RiskLevel::Low
            };
        }
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

    /// 日志抑制:`(account_id, alert_type)` → 上次**打印**的时刻。@yutiansut @quantaxis
    ///
    /// 告警本身照常产生并入库(上限 `max_alerts_per_account`),这里只抑制**日志**。
    /// 起因:实测日志里 **32,903 条 `[RiskAlert]`,全部来自同一个账户、同一种类型
    /// (`NegativeAvailable`),占全部 WARN 的 94%**。风控哨兵每个周期重扫一遍,
    /// 同一个未解决的状态就被反复打印,把「grep WARN 找问题」这条运维路径彻底淹掉。
    ///
    /// 只在**状态首次出现**或**冷却期满**时打印,并在打印时带上期间被抑制的条数,
    /// 这样既不丢信息也不刷屏。
    alert_log_last: DashMap<(String, String), std::time::Instant>,
    /// 冷却期内被抑制的条数,打印时一并报出并清零。
    alert_log_suppressed: DashMap<(String, String), u64>,
    /// 预警序列号
    alert_seq: AtomicU64,
    /// 监控配置
    config: RwLock<RiskMonitorConfig>,
    /// 监控统计
    stats: RwLock<MonitorStats>,
    /// 上次各账户风险等级 (用于检测等级变化)
    last_risk_levels: DashMap<String, RiskLevel>,

    /// 每账户上次记录的「现金-权益」缺口,用于只在缺口**变化**时告警。
    /// 存量分叉每秒刷屏无意义;缺口变动才意味着刚刚又漏了一次。
    /// @yutiansut @quantaxis
    last_cash_gap: DashMap<String, f64>,
    /// 监控线程是否运行
    monitor_running: AtomicBool,
    /// 强平回调（可选，用于触发外部强平流程）
    liquidation_callback: RwLock<Option<LiquidationCallback>>,

    /// 行情源（可选）—— 用于盯市刷价 @yutiansut @quantaxis
    ///
    /// ⚠️ 没有它,持仓的 `last_price` **只在自己成交时更新**
    /// (qars `receive_deal_sim` 用成交价调 `on_price_change`),
    /// 于是「一个账户的持仓价 = 它最后一笔成交的价格,不成交就永不变」。
    ///
    /// 实测(2026-09-04,5 小时压测):同一秒写入的快照里
    /// IC2501 有 5628.2 / 5338.8 / 5325.2 三个值,滞后最多 289.4 点(5.1%);
    /// 全系统 Σposition_profit = +2.53 亿(封闭系统里应为 0)。
    ///
    /// 后果最重的是**最需要盯市的账户刷价最少** —— 做市商大量挂撤、
    /// 少量成交,持仓价停在建仓那一刻。极端情形:3800 建多仓后不再成交,
    /// 价格跌到 3000,账面浮亏仍是 0,风险率不动,**强平永不触发**。
    ///
    /// 用可选注入而非改 `new()` 签名:`RiskMonitor::new` 有 30 处测试调用点。
    price_source: RwLock<Option<Arc<crate::matching::engine::ExchangeMatchingEngine>>>,
}

impl RiskMonitor {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            liquidation_records: DashMap::new(),
            liquidation_seq: AtomicU64::new(1),
            risk_alerts: DashMap::new(),
            alert_log_last: DashMap::new(),
            alert_log_suppressed: DashMap::new(),
            alert_seq: AtomicU64::new(1),
            config: RwLock::new(RiskMonitorConfig::default()),
            stats: RwLock::new(MonitorStats::default()),
            last_risk_levels: DashMap::new(),
            last_cash_gap: DashMap::new(),
            monitor_running: AtomicBool::new(false),
            liquidation_callback: RwLock::new(None),
            price_source: RwLock::new(None),
        }
    }

    /// 设置行情源(启用盯市刷价)
    ///
    /// 不设则退化为原有行为:持仓价只在成交时更新。@yutiansut @quantaxis
    pub fn set_price_source(
        &self,
        engine: Arc<crate::matching::engine::ExchangeMatchingEngine>,
    ) {
        *self.price_source.write() = Some(engine);
        log::info!("✅ RiskMonitor 已接入行情源,启用盯市刷价");
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

        // ✨ 行情快照必须在**账户锁之外**取 —— 这是死锁的根因 @yutiansut @quantaxis
        //
        // 原实现在 `let mut acc = account.write();` 之内调
        // `engine.get_last_price()`(→ `orderbook.read()`),形成锁序
        //     账户写锁 → 订单簿读锁
        // 而这是全仓库唯一的这个方向。撮合/撤单路径是反向的
        // (订单簿锁 → 账户锁),两者相遇即成环。
        //
        // 2026-09-04 实测:带此改动的二进制分别在 60 秒 / 9 分钟 / 26.5 分钟挂死;
        // 挂死瞬间的 core 里,本线程正持 `account.write()` 阻塞在 `lock_shared_slow`,
        // 同时 8 个 `get_account_stats` 线程堵在 `account.read()` 上。
        // 延迟曲线是**阶跃**(上一采样 10ms 全绿,下一采样三个业务端点同时永久超时),
        // 确认是死锁而非锁护送。
        //
        // 改法:每轮开始时一次性取全部合约的最新价,之后进账户锁只做纯内存更新。
        // 副作用是更快 —— N 个合约查 1 次,而不是「账户数 × 持仓数」次。
        let price_snapshot: std::collections::HashMap<String, f64> = {
            match self.price_source.read().as_ref() {
                Some(engine) => engine
                    .get_instruments()
                    .into_iter()
                    .filter_map(|code| {
                        engine
                            .get_last_price(&code)
                            .filter(|p| *p > 0.0)
                            .map(|p| (code, p))
                    })
                    .collect(),
                None => std::collections::HashMap::new(),
            }
        };
        let price_ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for account in accounts.iter() {
            let mut acc = account.write();
            let account_id = acc.account_cookie.clone();

            // 盯市刷价 —— 必须在算 risk_ratio 之前。
            // 价格来自本轮开始时在**锁外**取好的快照(见上方注释),
            // 这里只做纯内存更新,不再碰任何外部锁。
            if !price_snapshot.is_empty() {
                for code in acc.get_code_subscribed() {
                    if let Some(&px) = price_snapshot.get(&code) {
                        acc.on_price_change(code, px, price_ts.clone());
                    }
                }
            }

            let risk_ratio = acc.get_riskratio();
            let available = acc.money;
            let current_level = RiskLevel::from_risk_ratio(risk_ratio);

            // ✨ 把现算的派生量回写到 QIFI 缓存字段 @yutiansut @quantaxis
            //
            // qars 只在 `settle()` 里整体重建 `self.accounts`(account.rs:574),
            // 成交/撤单都不回写,所以 `acc.accounts.balance / risk_ratio / ...`
            // **盘中永远是开户初始值**。后果最重的一处是
            // `risk/pre_trade_check.rs:310` 的 `check_risk_ratio` ——
            // 它读 `acc.accounts.risk_ratio`(恒为 0.0),
            // `if 0.0 >= reject_threshold` 永不成立,**这道风控从不生效**。
            // 实测 NOISE_SELL 真实风险率约 89%,该字段仍是 0.0,
            // 它一路下单到资金耗尽,风险率闸门全程没拦过。
            //
            // 本循环每 `monitor_interval_ms`(默认 1000ms)对每个账户
            // **已经持有写锁、且已经算出 risk_ratio**,顺手回写零额外开销:
            //   · `check_risk_ratio` 保持读锁不变即可拿到 ≤1s 新鲜的值,
            //     不必在每笔订单的热路径上升级成写锁;
            //   · 其余所有读缓存字段的地方(data_query / monitoring 等)一并受益。
            // qars 内部无任何条件逻辑依赖这些字段(仅一处测试断言),写回安全;
            // `settle()` 之后会被整体覆盖,不影响日终口径。
            let live_balance = acc.get_balance();
            let live_margin = acc.get_margin();
            let live_position_profit = acc.get_positionprofit();
            let live_float_profit = acc.get_floatprofit();
            let live_close_profit = acc.get_closeprofit();
            let live_frozen_margin = acc.get_frozen_margin();
            acc.accounts.risk_ratio = risk_ratio;
            acc.accounts.balance = live_balance;
            acc.accounts.available = available;
            acc.accounts.margin = live_margin;
            acc.accounts.position_profit = live_position_profit;
            acc.accounts.float_profit = live_float_profit;
            acc.accounts.close_profit = live_close_profit;
            acc.accounts.frozen_margin = live_frozen_margin;

            // ── 现金 ↔ 权益 对账 ──────────────────────────────────────
            // @yutiansut @quantaxis
            //
            // 为什么必须做:`get_balance()`(qars2 account.rs:522)**一次都没碰
            // `self.money`** —— 权益走 `accounts.*` 那组字段,现金走
            // `money -= frozen` / `money -= margin - close_profit + commission`
            // 那条路径。**两条账互相独立,任何一次不一致产生后没有任何机制会让它们
            // 重新对上**;`new_from_qifi`(account.rs:191)恢复时又逐字段照抄,
            // 于是分叉永久保留且永不显形。
            //
            // ⚠️ 注意「balance 恒等式」检查不出这个 —— 那条式子就是 `get_balance()`
            // 的字面实现,是同义反复,78 个账户 0 违反也不构成正确性证据。
            // **唯一独立的检验就是拿 `money` 对**。2026-09-05 实测:78 个生产账户里
            // **13 个对不上,合计 ¥27,508,069**(一个账户 deposit 被重复计入 2000 万、
            // 一个 deposit 为负 -1009 万、另外 10 个零成交零出入金却现金短少约 242 万)。
            //
            // 挂在这里的理由:本循环每秒已遍历全部账户、已持写锁、上面几行已经把
            // 需要的值全算出来了 —— **零额外开销,不需要新线程也不需要新文件**。
            //
            // 只告警不改数:在弄清「钱是什么时候、因为什么少的」之前改数会掩盖证据。
            let frozen_money: f64 = acc.frozen.values().map(|f| f.money).sum();
            let expected_money = acc.accounts.static_balance + acc.accounts.deposit
                - acc.accounts.withdraw + live_close_profit
                - acc.accounts.commission - live_margin - frozen_money;
            let cash_gap = acc.money - expected_money;
            // 容差取相对值,避免大账户被浮点噪声刷屏;1e-6 相对 + 1 分钱绝对
            if cash_gap.abs() > (expected_money.abs() * 1e-6).max(0.01) {
                let prev = self.last_cash_gap.get(&account_id).map(|v| *v).unwrap_or(0.0);
                // 只在**缺口发生变化**时告警 —— 存量分叉每秒刷屏没有意义,
                // 而缺口变动意味着「就在刚才又漏了一次」,那才是要抓的时刻。
                if (cash_gap - prev).abs() > 0.01 {
                    log::error!(
                        "💰 [CashReconcile] 现金与权益对不上: account={}, money={:.2}, \
                         期望={:.2}, 缺口={:.2} (上次 {:.2}, 本次变动 {:.2}) | \
                         static_balance={:.2} deposit={:.2} withdraw={:.2} \
                         close_profit={:.2} commission={:.2} margin={:.2} frozen={:.2}",
                        account_id, acc.money, expected_money, cash_gap, prev,
                        cash_gap - prev,
                        acc.accounts.static_balance, acc.accounts.deposit,
                        acc.accounts.withdraw, live_close_profit,
                        acc.accounts.commission, live_margin, frozen_money
                    );
                    self.last_cash_gap.insert(account_id.clone(), cash_gap);
                }
            } else if self.last_cash_gap.contains_key(&account_id) {
                // 缺口消失了也值得记一笔
                log::info!("💰 [CashReconcile] 账户 {} 的现金缺口已消除", account_id);
                self.last_cash_gap.remove(&account_id);
            }

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

        // ✨ 同 (账户, 类型) 的重复告警按冷却期抑制日志。@yutiansut @quantaxis
        // 冷却期 60 秒:够短,状态变化不会被埋太久;够长,能把每周期一条压成每分钟一条。
        const ALERT_LOG_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);
        let key = (account_id.to_string(), format!("{:?}", alert_type));
        let now_i = std::time::Instant::now();
        let should_log = match self.alert_log_last.get(&key) {
            Some(last) => now_i.duration_since(*last.value()) >= ALERT_LOG_COOLDOWN,
            None => true, // 首次出现必打
        };
        if should_log {
            self.alert_log_last.insert(key.clone(), now_i);
            let suppressed = self
                .alert_log_suppressed
                .remove(&key)
                .map(|(_, n)| n)
                .unwrap_or(0);
            if suppressed > 0 {
                log::warn!(
                    "[RiskAlert] {} - {} ({}): {}(距上次打印期间另有 {} 条同类被抑制)",
                    alert.alert_id, account_id, key.1, alert.message, suppressed
                );
            } else {
                log::warn!("[RiskAlert] {} - {} ({}): {}",
                    alert.alert_id, account_id, key.1, alert.message);
            }
        } else {
            *self.alert_log_suppressed.entry(key).or_insert(0) += 1;
        }

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
                // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
                let position_margin = acc.get_margin();
                let frozen_margin = acc.get_frozen_margin();
                let margin_used = position_margin + frozen_margin;
                let unrealized_pnl = acc.get_positionprofit();
                let raw_risk_ratio = acc.get_riskratio();
                let position_count = acc.hold.len();
                // 等级用**原始值**判(需要区分 NaN 与 +Inf,见 from_risk_ratio)
                let risk_level = RiskLevel::from_risk_ratio(raw_risk_ratio);
                // ⚠️ 但对外**序列化**的值必须是有限数 —— serde_json 会把
                //    NaN / Inf 静默写成 `null`,前端 `|| 0` 一兜就成了「0.0%」,
                //    一个看不出来的错值。NaN(0/0,无敞口) → 0.0;
                //    +Inf(有保证金零权益) → 用一个明确超阈的有限数表达。
                //    @yutiansut @quantaxis
                let risk_ratio = if raw_risk_ratio.is_nan() {
                    0.0
                } else if raw_risk_ratio.is_infinite() {
                    if raw_risk_ratio > 0.0 { 999.0 } else { 0.0 }
                } else {
                    raw_risk_ratio
                };

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
        // ⚠️ 平均值必须跳过非有限项 —— 一个 NaN 就会把整个和污染成 NaN,
        //    而 serde_json 把非有限 f64 **静默序列化成 `null`**
        //    (`value/ser.rs` → `Number::from_f64` 对非有限返回 None)。
        //    前端 `risk.vue:409` 的 `|| 0` 再把 null 兜成 0 ⇒
        //    「平均风险率」在**成功路径**上恒显示 0.0%。
        //    实测:57 个账户里 19 个 NaN,真值应为 1.8% 而非 0.0%。
        //    @yutiansut @quantaxis
        let finite: Vec<f64> = risk_accounts
            .iter()
            .map(|acc| acc.risk_ratio)
            .filter(|r| r.is_finite())
            .collect();
        let average_risk_ratio = if !finite.is_empty() {
            finite.iter().sum::<f64>() / finite.len() as f64
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

    /// 非有限风险率不得掉进 `else` 分支 —— 那会把空账户误判为 Critical
    ///
    /// 回归锁定:`risk_ratio = margin / balance`,自动创建的默认账户
    /// `init_cash = 0` ⇒ `0.0/0.0 = NaN`。NaN 参与 `<` 恒为 false,
    /// 旧实现里三个 `<` 全 false 后落到 else = Critical。
    /// 实测线上 57 个账户里 19 个是这种空账户,全部被误报为最高危。
    /// @yutiansut @quantaxis
    #[test]
    fn test_risk_level_non_finite_ratio() {
        // 0/0:既无权益也无保证金,没有风险敞口
        assert_eq!(RiskLevel::from_risk_ratio(f64::NAN), RiskLevel::Low);
        assert_eq!(RiskLevel::from_risk_ratio(0.0 / 0.0), RiskLevel::Low);
        // x/0:有保证金却零权益,是真的危险
        assert_eq!(RiskLevel::from_risk_ratio(f64::INFINITY), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_risk_ratio(1.0 / 0.0), RiskLevel::Critical);
        // 负无穷不是风险
        assert_eq!(RiskLevel::from_risk_ratio(f64::NEG_INFINITY), RiskLevel::Low);
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

#[cfg(test)]
mod alert_log_suppression_tests {
    use dashmap::DashMap;
    use std::time::{Duration, Instant};

    /// 抑制判据的纯逻辑复刻 —— 与 `create_alert` 里那段保持同一形状。
    /// @yutiansut @quantaxis
    ///
    /// 起因:实测日志 **32,903 条 `[RiskAlert]` 全部来自同一账户同一类型**,
    /// 占全部 WARN 的 **94%** —— 风控哨兵每周期重扫,同一个未解决状态被反复打印。
    fn should_log(
        last: &DashMap<(String, String), Instant>,
        key: &(String, String),
        now: Instant,
        cooldown: Duration,
    ) -> bool {
        match last.get(key) {
            Some(t) => now.duration_since(*t.value()) >= cooldown,
            None => true,
        }
    }

    #[test]
    fn first_occurrence_always_logs() {
        let last = DashMap::new();
        let k = ("ACC_1".to_string(), "NegativeAvailable".to_string());
        assert!(should_log(&last, &k, Instant::now(), Duration::from_secs(60)),
                "首次出现必须打印 —— 抑制不能把新问题也埋掉");
    }

    #[test]
    fn repeat_within_cooldown_is_suppressed() {
        let last = DashMap::new();
        let k = ("ACC_1".to_string(), "NegativeAvailable".to_string());
        let t0 = Instant::now();
        last.insert(k.clone(), t0);
        assert!(!should_log(&last, &k, t0 + Duration::from_secs(1), Duration::from_secs(60)));
        assert!(!should_log(&last, &k, t0 + Duration::from_secs(59), Duration::from_secs(60)));
    }

    #[test]
    fn logs_again_after_cooldown() {
        let last = DashMap::new();
        let k = ("ACC_1".to_string(), "NegativeAvailable".to_string());
        let t0 = Instant::now();
        last.insert(k.clone(), t0);
        assert!(should_log(&last, &k, t0 + Duration::from_secs(60), Duration::from_secs(60)),
                "冷却期满必须重新打印,否则问题会永久消失");
    }

    /// **负对照**:不同账户、不同类型互不影响 —— 否则一个刷屏账户会把
    /// 其它账户的告警一起压掉,那比刷屏更危险。
    #[test]
    fn different_keys_are_independent() {
        let last = DashMap::new();
        let t0 = Instant::now();
        last.insert(("ACC_1".to_string(), "NegativeAvailable".to_string()), t0);
        let cd = Duration::from_secs(60);
        assert!(should_log(&last, &("ACC_2".to_string(), "NegativeAvailable".to_string()), t0, cd),
                "另一个账户的同类告警不受影响");
        assert!(should_log(&last, &("ACC_1".to_string(), "HighRisk".to_string()), t0, cd),
                "同一账户的另一种类型不受影响");
    }

    /// 被抑制的条数必须累计,打印时报出 —— 不能静默丢弃。
    #[test]
    fn suppressed_count_accumulates_and_resets() {
        let sup: DashMap<(String, String), u64> = DashMap::new();
        let k = ("ACC_1".to_string(), "NegativeAvailable".to_string());
        for _ in 0..7 { *sup.entry(k.clone()).or_insert(0) += 1; }
        assert_eq!(sup.get(&k).map(|v| *v.value()), Some(7));
        let taken = sup.remove(&k).map(|(_, n)| n).unwrap_or(0);
        assert_eq!(taken, 7, "打印时应取出全部抑制计数");
        assert!(sup.get(&k).is_none(), "取出后必须清零,否则会重复计数");
    }
}
