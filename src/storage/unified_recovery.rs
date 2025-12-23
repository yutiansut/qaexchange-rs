//! 统一恢复管理器 - 流批一体化数据恢复
//!
//! 架构设计：
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                    UnifiedRecoveryManager                           │
//! │                                                                     │
//! │  ┌─────────────────────────────────────────────────────────────┐   │
//! │  │ 账户恢复          │ 行情恢复           │ 因子恢复            │   │
//! │  │ - AccountOpen     │ - TickData         │ - FactorUpdate      │   │
//! │  │ - AccountUpdate   │ - OrderBook        │ - FactorSnapshot    │   │
//! │  │ - UserRegister    │ - KLineFinished    │                     │   │
//! │  └─────────────────────────────────────────────────────────────┘   │
//! │  ┌─────────────────────────────────────────────────────────────┐   │
//! │  │ 交易恢复          │ 交易所逐笔恢复                           │   │
//! │  │ - OrderInsert     │ - ExchangeOrderRecord                   │   │
//! │  │ - TradeExecuted   │ - ExchangeTradeRecord                   │   │
//! │  │                   │ - ExchangeResponseRecord                │   │
//! │  └─────────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────────┘
//!
//! @yutiansut @quantaxis

use crate::storage::wal::manager::WalManager;
use crate::storage::wal::record::WalRecord;
use crate::ExchangeError;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════════════════
// 恢复统计
// ═══════════════════════════════════════════════════════════════════════════

/// 恢复统计信息
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// 总记录数
    pub total_records: u64,
    /// 账户相关记录
    pub account_records: u64,
    /// 用户相关记录
    pub user_records: u64,
    /// 订单相关记录
    pub order_records: u64,
    /// 成交相关记录
    pub trade_records: u64,
    /// 行情相关记录
    pub market_data_records: u64,
    /// K线相关记录
    pub kline_records: u64,
    /// 因子相关记录
    pub factor_records: u64,
    /// 交易所逐笔记录
    pub exchange_records: u64,
    /// 检查点记录
    pub checkpoint_records: u64,
    /// 订单状态更新记录 (Phase 14)
    pub order_status_records: u64,
    /// 持仓快照记录 (Phase 14)
    pub position_snapshot_records: u64,
    /// 账户快照记录 (Phase 14)
    pub account_snapshot_records: u64,
    /// 恢复耗时（毫秒）
    pub recovery_time_ms: u128,
    /// 错误数量
    pub error_count: u64,
}

impl RecoveryStats {
    /// 创建新的统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录分类
    pub fn record(&mut self, record: &WalRecord) {
        self.total_records += 1;
        match record {
            WalRecord::AccountOpen { .. } | WalRecord::AccountUpdate { .. } => {
                self.account_records += 1;
            }
            WalRecord::UserRegister { .. } | WalRecord::AccountBind { .. } | WalRecord::UserRoleUpdate { .. } => {
                self.user_records += 1;
            }
            WalRecord::OrderInsert { .. } => {
                self.order_records += 1;
            }
            WalRecord::TradeExecuted { .. } => {
                self.trade_records += 1;
            }
            WalRecord::TickData { .. }
            | WalRecord::OrderBookSnapshot { .. }
            | WalRecord::OrderBookDelta { .. } => {
                self.market_data_records += 1;
            }
            WalRecord::KLineFinished { .. } => {
                self.kline_records += 1;
            }
            WalRecord::FactorUpdate { .. } | WalRecord::FactorSnapshot { .. } => {
                self.factor_records += 1;
            }
            WalRecord::ExchangeOrderRecord { .. }
            | WalRecord::ExchangeTradeRecord { .. }
            | WalRecord::ExchangeResponseRecord { .. } => {
                self.exchange_records += 1;
            }
            WalRecord::Checkpoint { .. } => {
                self.checkpoint_records += 1;
            }
            // Phase 14: 订单生命周期和账户恢复增强
            WalRecord::OrderStatusUpdate { .. } => {
                self.order_status_records += 1;
            }
            WalRecord::PositionSnapshot { .. } => {
                self.position_snapshot_records += 1;
            }
            WalRecord::AccountSnapshot { .. } => {
                self.account_snapshot_records += 1;
            }
        }
    }

    /// 打印恢复报告
    pub fn print_report(&self) {
        log::info!("═══════════════════════════════════════════════════════════");
        log::info!("               WAL 恢复统计报告                              ");
        log::info!("═══════════════════════════════════════════════════════════");
        log::info!("总记录数:        {}", self.total_records);
        log::info!("───────────────────────────────────────────────────────────");
        log::info!("账户记录:        {}", self.account_records);
        log::info!("用户记录:        {}", self.user_records);
        log::info!("订单记录:        {}", self.order_records);
        log::info!("成交记录:        {}", self.trade_records);
        log::info!("行情记录:        {}", self.market_data_records);
        log::info!("K线记录:         {}", self.kline_records);
        log::info!("因子记录:        {}", self.factor_records);
        log::info!("交易所逐笔:      {}", self.exchange_records);
        log::info!("检查点:          {}", self.checkpoint_records);
        log::info!("订单状态更新:    {}", self.order_status_records);
        log::info!("持仓快照:        {}", self.position_snapshot_records);
        log::info!("账户快照:        {}", self.account_snapshot_records);
        log::info!("───────────────────────────────────────────────────────────");
        log::info!("恢复耗时:        {} ms", self.recovery_time_ms);
        log::info!("错误数量:        {}", self.error_count);
        log::info!("═══════════════════════════════════════════════════════════");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 恢复数据容器
// ═══════════════════════════════════════════════════════════════════════════

/// 账户状态（恢复用）
#[derive(Debug, Clone)]
pub struct RecoveredAccount {
    pub account_id: String,
    pub user_id: String,
    pub account_name: String,
    pub init_cash: f64,
    pub account_type: u8,
    pub created_at: i64,
    pub balance: f64,
    pub available: f64,
    pub frozen: f64,
    pub deposit: f64,
    pub withdraw: f64,
    pub margin: f64,
    pub last_sequence: u64,
}

/// 用户状态（恢复用）
#[derive(Debug, Clone)]
pub struct RecoveredUser {
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub created_at: i64,
    pub account_ids: Vec<String>,
}

/// K线数据（恢复用）
#[derive(Debug, Clone)]
pub struct RecoveredKLine {
    pub instrument_id: String,
    pub period: i32,
    pub kline_timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub amount: f64,
    pub open_oi: i64,
    pub close_oi: i64,
}

/// 因子状态（恢复用）
#[derive(Debug, Clone)]
pub struct RecoveredFactor {
    pub instrument_id: String,
    pub factor_id: String,
    pub factor_type: u8,
    pub value: f64,
    pub values: Vec<f64>,
    pub is_valid: bool,
    pub source_timestamp: i64,
    pub timestamp: i64,
}

/// 订单状态（恢复用）Phase 14
/// @yutiansut @quantaxis
#[derive(Debug, Clone)]
pub struct RecoveredOrder {
    pub order_id: String,
    pub user_id: String,
    pub instrument_id: String,
    pub exchange_id: String,
    pub direction: u8,        // 0=BUY, 1=SELL
    pub offset: u8,           // 0=OPEN, 1=CLOSE, 2=CLOSETODAY
    pub status: u8,           // 0=ALIVE, 1=FINISHED, 2=CANCELLED, 3=REJECTED, 4=PARTIALLY_FILLED
    pub volume_orign: f64,    // 原始委托量
    pub volume_left: f64,     // 剩余未成交量
    pub volume_filled: f64,   // 已成交量
    pub limit_price: f64,     // 委托价格
    pub avg_price: f64,       // 成交均价
    pub frozen_margin: f64,   // 冻结保证金
    pub frozen_amount: f64,   // 冻结资金
    pub last_msg: String,     // 最后消息
    pub timestamp: i64,       // 时间戳
    pub last_sequence: u64,   // 最后更新的序列号
}

/// 持仓状态（恢复用）Phase 14
/// @yutiansut @quantaxis
#[derive(Debug, Clone)]
pub struct RecoveredPosition {
    pub user_id: String,
    pub instrument_id: String,
    pub exchange_id: String,
    // 多头
    pub volume_long_today: f64,
    pub volume_long_his: f64,
    pub volume_long_frozen_today: f64,
    pub volume_long_frozen_his: f64,
    pub open_price_long: f64,
    pub open_cost_long: f64,
    pub position_price_long: f64,
    pub position_cost_long: f64,
    pub margin_long: f64,
    // 空头
    pub volume_short_today: f64,
    pub volume_short_his: f64,
    pub volume_short_frozen_today: f64,
    pub volume_short_frozen_his: f64,
    pub open_price_short: f64,
    pub open_cost_short: f64,
    pub position_price_short: f64,
    pub position_cost_short: f64,
    pub margin_short: f64,
    // 盈亏
    pub float_profit_long: f64,
    pub float_profit_short: f64,
    pub position_profit_long: f64,
    pub position_profit_short: f64,
    pub last_price: f64,
    pub timestamp: i64,
    pub last_sequence: u64,
}

/// 统一恢复结果
#[derive(Debug, Clone, Default)]
pub struct UnifiedRecoveryResult {
    /// 恢复的账户
    pub accounts: HashMap<String, RecoveredAccount>,
    /// 恢复的用户
    pub users: HashMap<String, RecoveredUser>,
    /// 恢复的K线（按合约+周期分组）
    pub klines: HashMap<String, Vec<RecoveredKLine>>,
    /// 恢复的因子（按合约+因子ID分组）
    pub factors: HashMap<String, RecoveredFactor>,
    /// 恢复的订单（按账户+订单ID分组）Phase 14
    pub orders: HashMap<String, RecoveredOrder>,
    /// 恢复的持仓（按账户+合约分组）Phase 14
    pub positions: HashMap<String, RecoveredPosition>,
    /// 最后的检查点序列号
    pub last_checkpoint_sequence: u64,
    /// 最后处理的序列号
    pub last_sequence: u64,
    /// 统计信息
    pub stats: RecoveryStats,
}

// ═══════════════════════════════════════════════════════════════════════════
// 恢复配置
// ═══════════════════════════════════════════════════════════════════════════

/// 恢复配置
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// 是否恢复账户数据
    pub recover_accounts: bool,
    /// 是否恢复用户数据
    pub recover_users: bool,
    /// 是否恢复K线数据
    pub recover_klines: bool,
    /// 是否恢复因子数据
    pub recover_factors: bool,
    /// 是否恢复订单历史
    pub recover_orders: bool,
    /// 是否恢复成交历史
    pub recover_trades: bool,
    /// 起始时间戳（纳秒），0表示从头开始
    pub start_timestamp: i64,
    /// 结束时间戳（纳秒），0表示到最新
    pub end_timestamp: i64,
    /// 只恢复指定合约（空表示全部）
    pub instruments: Vec<String>,
    /// 从检查点恢复（如果可用）
    pub use_checkpoint: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            recover_accounts: true,
            recover_users: true,
            recover_klines: true,
            recover_factors: true,
            recover_orders: false, // 默认不恢复订单历史
            recover_trades: false, // 默认不恢复成交历史
            start_timestamp: 0,
            end_timestamp: 0,
            instruments: Vec::new(),
            use_checkpoint: true,
        }
    }
}

impl RecoveryConfig {
    /// 只恢复账户
    pub fn accounts_only() -> Self {
        Self {
            recover_accounts: true,
            recover_users: true,
            recover_klines: false,
            recover_factors: false,
            ..Default::default()
        }
    }

    /// 只恢复行情数据
    pub fn market_data_only() -> Self {
        Self {
            recover_accounts: false,
            recover_users: false,
            recover_klines: true,
            recover_factors: false,
            ..Default::default()
        }
    }

    /// 只恢复因子数据
    pub fn factors_only() -> Self {
        Self {
            recover_accounts: false,
            recover_users: false,
            recover_klines: false,
            recover_factors: true,
            ..Default::default()
        }
    }

    /// 全量恢复（包括订单和成交历史）
    pub fn full_recovery() -> Self {
        Self {
            recover_accounts: true,
            recover_users: true,
            recover_klines: true,
            recover_factors: true,
            recover_orders: true,
            recover_trades: true,
            ..Default::default()
        }
    }

    /// 设置时间范围
    pub fn with_time_range(mut self, start: i64, end: i64) -> Self {
        self.start_timestamp = start;
        self.end_timestamp = end;
        self
    }

    /// 设置合约过滤
    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.instruments = instruments;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 统一恢复管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 统一恢复管理器
///
/// 提供流批一体化的数据恢复功能：
/// - 账户数据恢复
/// - 用户数据恢复
/// - K线数据恢复
/// - 因子数据恢复
/// - 支持时间范围和合约过滤
pub struct UnifiedRecoveryManager {
    /// WAL 目录路径
    wal_dir: String,
    /// 恢复配置
    config: RecoveryConfig,
}

impl UnifiedRecoveryManager {
    /// 创建恢复管理器
    pub fn new(wal_dir: impl Into<String>) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            config: RecoveryConfig::default(),
        }
    }

    /// 设置恢复配置
    pub fn with_config(mut self, config: RecoveryConfig) -> Self {
        self.config = config;
        self
    }

    /// 执行统一恢复
    pub fn recover(&self) -> Result<UnifiedRecoveryResult, ExchangeError> {
        let start_time = Instant::now();
        let mut result = UnifiedRecoveryResult::default();

        // 恢复账户WAL
        if self.config.recover_accounts || self.config.recover_users {
            self.recover_account_wal(&mut result)?;
        }

        // 恢复合约WAL（K线、因子等）
        if self.config.recover_klines || self.config.recover_factors {
            self.recover_instrument_wals(&mut result)?;
        }

        result.stats.recovery_time_ms = start_time.elapsed().as_millis();
        result.stats.print_report();

        Ok(result)
    }

    /// 恢复账户WAL
    fn recover_account_wal(&self, result: &mut UnifiedRecoveryResult) -> Result<(), ExchangeError> {
        let account_wal_dir = format!("{}/__ACCOUNT__", self.wal_dir);
        let wal_path = Path::new(&account_wal_dir);

        if !wal_path.exists() {
            log::info!("No account WAL directory found at {}", account_wal_dir);
            return Ok(());
        }

        log::info!("Recovering account WAL from {}", account_wal_dir);

        let wal_manager = WalManager::new(&account_wal_dir);

        wal_manager
            .replay(|entry| {
                // 时间范围过滤
                if self.config.start_timestamp > 0 && entry.timestamp < self.config.start_timestamp
                {
                    return Ok(());
                }
                if self.config.end_timestamp > 0 && entry.timestamp > self.config.end_timestamp {
                    return Ok(());
                }

                // 检查点处理
                if let WalRecord::Checkpoint { sequence, .. } = &entry.record {
                    if self.config.use_checkpoint && *sequence > result.last_checkpoint_sequence {
                        result.last_checkpoint_sequence = *sequence;
                    }
                }

                result.stats.record(&entry.record);
                result.last_sequence = result.last_sequence.max(entry.sequence);

                self.process_record(entry.sequence, entry.record, result);
                Ok(())
            })
            .map_err(|e| ExchangeError::StorageError(format!("Account WAL replay failed: {}", e)))?;

        log::info!(
            "Account WAL recovery completed: {} accounts, {} users",
            result.accounts.len(),
            result.users.len()
        );

        Ok(())
    }

    /// 恢复合约WAL
    fn recover_instrument_wals(
        &self,
        result: &mut UnifiedRecoveryResult,
    ) -> Result<(), ExchangeError> {
        let wal_path = Path::new(&self.wal_dir);

        if !wal_path.exists() {
            log::info!("No WAL directory found at {}", self.wal_dir);
            return Ok(());
        }

        // 遍历所有子目录（每个合约一个目录）
        let entries = std::fs::read_dir(&self.wal_dir)
            .map_err(|e| ExchangeError::StorageError(format!("Failed to read WAL dir: {}", e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // 跳过特殊目录
            if dir_name.starts_with("__") || dir_name.starts_with('.') {
                continue;
            }

            // 合约过滤
            if !self.config.instruments.is_empty()
                && !self.config.instruments.contains(&dir_name.to_string())
            {
                continue;
            }

            log::debug!("Recovering instrument WAL: {}", dir_name);
            self.recover_single_instrument_wal(dir_name, &path, result)?;
        }

        log::info!(
            "Instrument WAL recovery completed: {} kline series, {} factors",
            result.klines.len(),
            result.factors.len()
        );

        Ok(())
    }

    /// 恢复单个合约WAL
    fn recover_single_instrument_wal(
        &self,
        instrument_id: &str,
        wal_path: &Path,
        result: &mut UnifiedRecoveryResult,
    ) -> Result<(), ExchangeError> {
        let wal_path_str = wal_path.to_str().ok_or_else(|| {
            ExchangeError::StorageError(format!(
                "Invalid WAL path encoding: {:?}",
                wal_path
            ))
        })?;
        let wal_manager = WalManager::new(wal_path_str);

        wal_manager
            .replay(|entry| {
                // 时间范围过滤
                if self.config.start_timestamp > 0 && entry.timestamp < self.config.start_timestamp
                {
                    return Ok(());
                }
                if self.config.end_timestamp > 0 && entry.timestamp > self.config.end_timestamp {
                    return Ok(());
                }

                result.stats.record(&entry.record);
                result.last_sequence = result.last_sequence.max(entry.sequence);

                self.process_instrument_record(instrument_id, entry.sequence, entry.record, result);
                Ok(())
            })
            .map_err(|e| {
                ExchangeError::StorageError(format!(
                    "Instrument {} WAL replay failed: {}",
                    instrument_id, e
                ))
            })?;

        Ok(())
    }

    /// 处理单条WAL记录
    fn process_record(
        &self,
        sequence: u64,
        record: WalRecord,
        result: &mut UnifiedRecoveryResult,
    ) {
        match record {
            // ═══════════════════════════════════════════════════════════════════
            // 账户数据恢复
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::AccountOpen {
                account_id,
                user_id,
                account_name,
                init_cash,
                account_type,
                timestamp,
            } if self.config.recover_accounts => {
                let account_id_str = WalRecord::from_fixed_array(&account_id);
                let user_id_str = WalRecord::from_fixed_array(&user_id);
                let account_name_str = WalRecord::from_fixed_array(&account_name);

                result.accounts.insert(
                    account_id_str.clone(),
                    RecoveredAccount {
                        account_id: account_id_str,
                        user_id: user_id_str,
                        account_name: account_name_str,
                        init_cash,
                        account_type,
                        created_at: timestamp,
                        balance: init_cash,
                        available: init_cash,
                        frozen: 0.0,
                        deposit: 0.0,
                        withdraw: 0.0,
                        margin: 0.0,
                        last_sequence: sequence,
                    },
                );
            }

            WalRecord::AccountUpdate {
                user_id,
                balance,
                available,
                frozen,
                margin,
                ..
            } if self.config.recover_accounts => {
                let user_id_str = WalRecord::from_fixed_array(&user_id);

                // 尝试用 user_id 作为 account_id 查找
                if let Some(account) = result.accounts.get_mut(&user_id_str) {
                    if sequence > account.last_sequence {
                        account.balance = balance;
                        account.available = available;
                        account.frozen = frozen;
                        account.margin = margin;
                        account.last_sequence = sequence;
                    }
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // 用户数据恢复
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::UserRegister {
                user_id,
                username,
                password_hash,
                phone,
                email,
                created_at,
                roles_bitmask,
            } if self.config.recover_users => {
                let user_id_str = WalRecord::from_fixed_array(&user_id);
                let username_str = WalRecord::from_fixed_array(&username);
                let password_hash_str = WalRecord::from_fixed_array(&password_hash);
                let phone_str = WalRecord::from_fixed_array(&phone);
                let email_str = WalRecord::from_fixed_array(&email);

                result.users.insert(
                    user_id_str.clone(),
                    RecoveredUser {
                        user_id: user_id_str,
                        username: username_str,
                        password_hash: password_hash_str,
                        phone: if phone_str.is_empty() {
                            None
                        } else {
                            Some(phone_str)
                        },
                        email: if email_str.is_empty() {
                            None
                        } else {
                            Some(email_str)
                        },
                        created_at,
                        account_ids: Vec::new(),
                    },
                );
            }

            WalRecord::AccountBind {
                user_id,
                account_id,
                ..
            } if self.config.recover_users => {
                let user_id_str = WalRecord::from_fixed_array(&user_id);
                let account_id_str = WalRecord::from_fixed_array(&account_id);

                if let Some(user) = result.users.get_mut(&user_id_str) {
                    if !user.account_ids.contains(&account_id_str) {
                        user.account_ids.push(account_id_str);
                    }
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // Phase 14: 订单生命周期恢复
            // @yutiansut @quantaxis
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::OrderStatusUpdate {
                order_id,
                user_id,
                instrument_id,
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
                last_msg,
                timestamp,
            } if self.config.recover_orders => {
                let order_id_str = WalRecord::from_fixed_array(&order_id);
                let user_id_str = WalRecord::from_fixed_array(&user_id);
                let instrument_id_str = WalRecord::from_fixed_array(&instrument_id);
                let last_msg_str = WalRecord::from_fixed_array(&last_msg);

                // 提取 exchange_id 从 instrument_id (格式: EXCHANGE.SYMBOL)
                let exchange_id = instrument_id_str
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let key = format!("{}_{}", user_id_str, order_id_str);

                // 只保留最新状态（基于 sequence）
                let should_update = result
                    .orders
                    .get(&key)
                    .map(|existing| sequence > existing.last_sequence)
                    .unwrap_or(true);

                if should_update {
                    result.orders.insert(
                        key,
                        RecoveredOrder {
                            order_id: order_id_str,
                            user_id: user_id_str,
                            instrument_id: instrument_id_str,
                            exchange_id,
                            direction,
                            offset,
                            status,
                            volume_orign,
                            volume_left,
                            volume_filled,
                            limit_price,
                            avg_price,
                            frozen_margin,
                            frozen_amount,
                            last_msg: last_msg_str,
                            timestamp,
                            last_sequence: sequence,
                        },
                    );
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // Phase 14: 持仓快照恢复
            // @yutiansut @quantaxis
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::PositionSnapshot {
                user_id,
                instrument_id,
                exchange_id,
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
            } if self.config.recover_orders => {
                let user_id_str = WalRecord::from_fixed_array(&user_id);
                let instrument_id_str = WalRecord::from_fixed_array(&instrument_id);
                let exchange_id_str = WalRecord::from_fixed_array(&exchange_id);

                let key = format!("{}_{}", user_id_str, instrument_id_str);

                // 只保留最新快照（基于 sequence）
                let should_update = result
                    .positions
                    .get(&key)
                    .map(|existing| sequence > existing.last_sequence)
                    .unwrap_or(true);

                if should_update {
                    result.positions.insert(
                        key,
                        RecoveredPosition {
                            user_id: user_id_str,
                            instrument_id: instrument_id_str,
                            exchange_id: exchange_id_str,
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
                            last_sequence: sequence,
                        },
                    );
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // Phase 14: 账户快照恢复（更完整的账户状态）
            // @yutiansut @quantaxis
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::AccountSnapshot {
                account_id,
                user_id,
                balance,
                available,
                frozen,
                margin,
                frozen_margin,
                frozen_commission,
                close_profit,
                position_profit,
                float_profit,
                commission,
                deposit,
                withdraw,
                pre_balance,
                static_balance,
                position_count,
                order_count,
                trade_count,
                checkpoint_id,
                last_sequence: snapshot_seq,
                timestamp,
            } if self.config.recover_accounts => {
                let account_id_str = WalRecord::from_fixed_array(&account_id);
                let user_id_str = WalRecord::from_fixed_array(&user_id);

                // 只更新已存在的账户，或创建新账户
                if let Some(account) = result.accounts.get_mut(&account_id_str) {
                    if sequence > account.last_sequence {
                        account.balance = balance;
                        account.available = available;
                        account.frozen = frozen;
                        account.margin = margin;
                        account.deposit = deposit;
                        account.withdraw = withdraw;
                        account.last_sequence = sequence;
                    }
                } else {
                    // 从快照创建账户（可能是从 checkpoint 恢复）
                    result.accounts.insert(
                        account_id_str.clone(),
                        RecoveredAccount {
                            account_id: account_id_str,
                            user_id: user_id_str,
                            account_name: String::new(), // 快照不包含名称
                            init_cash: pre_balance,
                            account_type: 0, // 快照不包含类型
                            created_at: timestamp,
                            balance,
                            available,
                            frozen,
                            deposit,
                            withdraw,
                            margin,
                            last_sequence: sequence,
                        },
                    );
                }

                // 更新检查点序列号
                if snapshot_seq > result.last_checkpoint_sequence {
                    result.last_checkpoint_sequence = snapshot_seq;
                }

                log::debug!(
                    "Recovered AccountSnapshot: account={}, balance={}, positions={}, orders={}",
                    WalRecord::from_fixed_array(&account_id),
                    balance,
                    position_count,
                    order_count
                );
            }

            // 其他记录类型跳过
            _ => {}
        }
    }

    /// 处理合约WAL记录
    fn process_instrument_record(
        &self,
        instrument_id: &str,
        _sequence: u64,
        record: WalRecord,
        result: &mut UnifiedRecoveryResult,
    ) {
        match record {
            // ═══════════════════════════════════════════════════════════════════
            // K线数据恢复
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::KLineFinished {
                instrument_id: _,
                period,
                kline_timestamp,
                open,
                high,
                low,
                close,
                volume,
                amount,
                open_oi,
                close_oi,
                ..
            } if self.config.recover_klines => {
                let key = format!("{}_{}", instrument_id, period);

                let klines = result.klines.entry(key).or_default();

                klines.push(RecoveredKLine {
                    instrument_id: instrument_id.to_string(),
                    period,
                    kline_timestamp,
                    open,
                    high,
                    low,
                    close,
                    volume,
                    amount,
                    open_oi,
                    close_oi,
                });
            }

            // ═══════════════════════════════════════════════════════════════════
            // 因子数据恢复
            // ═══════════════════════════════════════════════════════════════════
            WalRecord::FactorUpdate {
                instrument_id: _,
                factor_id,
                factor_type,
                value,
                values,
                value_count,
                is_valid,
                source_timestamp,
                timestamp,
            } if self.config.recover_factors => {
                let factor_id_str = WalRecord::from_fixed_array(&factor_id);
                let key = format!("{}_{}", instrument_id, factor_id_str);

                // 只保留最新的因子值
                let values_vec: Vec<f64> = values[..value_count as usize].to_vec();

                result.factors.insert(
                    key,
                    RecoveredFactor {
                        instrument_id: instrument_id.to_string(),
                        factor_id: factor_id_str,
                        factor_type,
                        value,
                        values: values_vec,
                        is_valid,
                        source_timestamp,
                        timestamp,
                    },
                );
            }

            WalRecord::FactorSnapshot {
                instrument_id: factor_instrument_id,
                factor_count,
                factor_ids,
                values,
                timestamp,
                ..
            } if self.config.recover_factors => {
                let inst_id = WalRecord::from_fixed_array(&factor_instrument_id);
                let inst_id = if inst_id.is_empty() {
                    instrument_id.to_string()
                } else {
                    inst_id
                };

                // 从快照恢复所有因子
                for i in 0..(factor_count as usize).min(32) {
                    let factor_id_str = WalRecord::from_fixed_array(&factor_ids[i]);
                    if factor_id_str.is_empty() {
                        continue;
                    }

                    let key = format!("{}_{}", inst_id, factor_id_str);

                    result.factors.insert(
                        key,
                        RecoveredFactor {
                            instrument_id: inst_id.clone(),
                            factor_id: factor_id_str,
                            factor_type: 0, // Scalar from snapshot
                            value: values[i],
                            values: vec![values[i]],
                            is_valid: true,
                            source_timestamp: timestamp,
                            timestamp,
                        },
                    );
                }
            }

            // 其他记录类型跳过
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 便捷函数
// ═══════════════════════════════════════════════════════════════════════════

/// 快速恢复账户数据
pub fn recover_accounts(wal_dir: &str) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(RecoveryConfig::accounts_only())
        .recover()
}

/// 快速恢复K线数据
pub fn recover_klines(wal_dir: &str) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(RecoveryConfig::market_data_only())
        .recover()
}

/// 快速恢复因子数据
pub fn recover_factors(wal_dir: &str) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(RecoveryConfig::factors_only())
        .recover()
}

/// 全量恢复
pub fn full_recovery(wal_dir: &str) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(RecoveryConfig::full_recovery())
        .recover()
}

/// 按时间范围恢复
pub fn recover_time_range(
    wal_dir: &str,
    start_ts: i64,
    end_ts: i64,
) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(
            RecoveryConfig::full_recovery()
                .with_time_range(start_ts, end_ts),
        )
        .recover()
}

/// 按合约恢复
pub fn recover_instruments(
    wal_dir: &str,
    instruments: Vec<String>,
) -> Result<UnifiedRecoveryResult, ExchangeError> {
    UnifiedRecoveryManager::new(wal_dir)
        .with_config(
            RecoveryConfig::full_recovery()
                .with_instruments(instruments),
        )
        .recover()
}

// ═══════════════════════════════════════════════════════════════════════════
// 服务集成 - 与 MarketDataService 和 FactorEngine 集成
// ═══════════════════════════════════════════════════════════════════════════

use crate::market::{MarketDataCache, TickData};

/// 将恢复的 K 线数据填充到 MarketDataCache
pub fn populate_market_cache(
    result: &UnifiedRecoveryResult,
    cache: &Arc<MarketDataCache>,
) -> Result<usize, ExchangeError> {
    let mut populated = 0;

    // 从 K 线数据中提取最新价格作为 Tick
    for klines in result.klines.values() {
        if klines.is_empty() {
            continue;
        }

        // 获取最新 K 线（按 kline_timestamp 排序）
        let latest = klines.iter().max_by_key(|k| k.kline_timestamp);
        if let Some(kline) = latest {
            let tick = TickData {
                instrument_id: kline.instrument_id.clone(),
                timestamp: kline.kline_timestamp,
                last_price: kline.close,
                bid_price: None,
                ask_price: None,
                volume: kline.volume,
            };

            cache.update_tick(kline.instrument_id.clone(), tick);
            populated += 1;
        }
    }

    log::info!(
        "✅ [Unified Recovery] Populated {} instruments to MarketDataCache",
        populated
    );

    Ok(populated)
}

/// 将恢复的因子数据转换为 StateCache 格式
///
/// 注意：此函数返回可用于 StateCache.restore_from_snapshot() 的数据
pub fn prepare_factor_snapshot(
    result: &UnifiedRecoveryResult,
) -> Result<crate::factor::state::GlobalStateSnapshot, ExchangeError> {
    use crate::factor::state::{
        GlobalStateSnapshot, InstrumentStateSnapshot, SerializableFactorValue,
    };
    use std::collections::HashMap as StdHashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut instruments_map: StdHashMap<String, InstrumentStateSnapshot> = StdHashMap::new();

    // 按合约分组因子数据
    for factor in result.factors.values() {
        let instrument_snapshot = instruments_map
            .entry(factor.instrument_id.clone())
            .or_insert_with(|| InstrumentStateSnapshot {
                instrument_id: factor.instrument_id.clone(),
                rolling_states: StdHashMap::new(),
                welford_states: StdHashMap::new(),
                ema_states: StdHashMap::new(),
                rsi_states: StdHashMap::new(),
                custom_values: StdHashMap::new(),
                update_count: 0,
                timestamp_ms: 0,
            });

        // 根据 factor_type 存储值
        let factor_value = match factor.factor_type {
            0 => SerializableFactorValue::Scalar(factor.value),
            1 => SerializableFactorValue::Vector(factor.values.clone()),
            2 => SerializableFactorValue::Optional(
                if factor.is_valid { Some(factor.value) } else { None }
            ),
            _ => SerializableFactorValue::Scalar(factor.value),
        };

        instrument_snapshot.custom_values.insert(
            factor.factor_id.clone(),
            factor_value,
        );

        // 更新时间戳和计数
        if factor.timestamp > instrument_snapshot.timestamp_ms as i64 {
            instrument_snapshot.timestamp_ms = factor.timestamp as u64;
        }
        instrument_snapshot.update_count += 1;
    }

    let instruments: Vec<InstrumentStateSnapshot> = instruments_map.into_values().collect();
    let checkpoint_id = result.last_sequence;

    Ok(GlobalStateSnapshot {
        version: GlobalStateSnapshot::CURRENT_VERSION,
        instruments,
        checkpoint_id,
        created_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    })
}

/// 恢复并自动填充到 MarketDataCache
pub fn recover_and_populate_cache(
    wal_dir: &str,
    cache: &Arc<MarketDataCache>,
) -> Result<(UnifiedRecoveryResult, usize), ExchangeError> {
    let result = recover_klines(wal_dir)?;
    let count = populate_market_cache(&result, cache)?;
    Ok((result, count))
}

/// 打印恢复摘要
pub fn print_recovery_summary(result: &UnifiedRecoveryResult) {
    log::info!("═══════════════════════════════════════════════════════════");
    log::info!("           统一恢复系统 - 恢复摘要                          ");
    log::info!("═══════════════════════════════════════════════════════════");
    log::info!(
        "📊 总记录数: {} | 最终序列号: {}",
        result.stats.total_records,
        result.last_sequence
    );
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("📦 账户数据:");
    log::info!("   - Account 记录:  {}", result.stats.account_records);
    log::info!("   - 账户快照:      {}", result.stats.account_snapshot_records);
    log::info!("   - 恢复账户数:    {}", result.accounts.len());
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("👤 用户数据:");
    log::info!("   - User 记录:     {}", result.stats.user_records);
    log::info!("   - 恢复用户数:    {}", result.users.len());
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("📋 订单数据 (Phase 14):");
    log::info!("   - Order 记录:    {}", result.stats.order_records);
    log::info!("   - 状态更新:      {}", result.stats.order_status_records);
    log::info!("   - 恢复订单数:    {}", result.orders.len());
    // 统计活跃订单
    let active_orders = result
        .orders
        .values()
        .filter(|o| o.status == 0 || o.status == 4) // ALIVE or PARTIALLY_FILLED
        .count();
    log::info!("   - 活跃订单:      {}", active_orders);
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("📊 持仓数据 (Phase 14):");
    log::info!("   - 持仓快照:      {}", result.stats.position_snapshot_records);
    log::info!("   - 恢复持仓数:    {}", result.positions.len());
    // 统计有效持仓
    let active_positions = result
        .positions
        .values()
        .filter(|p| {
            p.volume_long_today + p.volume_long_his > 0.0
                || p.volume_short_today + p.volume_short_his > 0.0
        })
        .count();
    log::info!("   - 有效持仓:      {}", active_positions);
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("📈 行情数据:");
    log::info!("   - MarketData:    {}", result.stats.market_data_records);
    log::info!("   - KLine:         {}", result.stats.kline_records);
    log::info!("   - 恢复K线合约数: {}", result.klines.len());
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("🧮 因子数据:");
    log::info!("   - Factor 记录:   {}", result.stats.factor_records);
    log::info!("   - 恢复因子数:    {}", result.factors.len());
    log::info!("───────────────────────────────────────────────────────────");
    log::info!("📋 交易所数据:");
    log::info!("   - Trade:         {}", result.stats.trade_records);
    log::info!("   - Exchange:      {}", result.stats.exchange_records);
    log::info!("───────────────────────────────────────────────────────────");
    log::info!(
        "⏱️  恢复耗时: {}ms",
        result.stats.recovery_time_ms
    );
    log::info!("═══════════════════════════════════════════════════════════");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert!(config.recover_accounts);
        assert!(config.recover_users);
        assert!(config.recover_klines);
        assert!(config.recover_factors);
        assert!(!config.recover_orders);
        assert!(!config.recover_trades);
    }

    #[test]
    fn test_recovery_config_accounts_only() {
        let config = RecoveryConfig::accounts_only();
        assert!(config.recover_accounts);
        assert!(config.recover_users);
        assert!(!config.recover_klines);
        assert!(!config.recover_factors);
    }

    #[test]
    fn test_recovery_stats() {
        let mut stats = RecoveryStats::new();

        // 模拟记录
        stats.record(&WalRecord::Checkpoint {
            sequence: 1,
            timestamp: 0,
        });
        stats.record(&WalRecord::Checkpoint {
            sequence: 2,
            timestamp: 0,
        });

        assert_eq!(stats.total_records, 2);
        assert_eq!(stats.checkpoint_records, 2);
    }

    #[test]
    fn test_recovery_manager_creation() {
        let manager = UnifiedRecoveryManager::new("/tmp/wal_test");
        assert_eq!(manager.wal_dir, "/tmp/wal_test");
    }
}
