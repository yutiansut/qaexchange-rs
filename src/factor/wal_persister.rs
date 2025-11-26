//! 因子 WAL 持久化模块
//!
//! @yutiansut @quantaxis
//!
//! 设计理念：
//! - 将 FactorEngine 的计算结果自动持久化到 WAL
//! - 支持增量更新 (FactorUpdate) 和批量快照 (FactorSnapshot)
//! - 低延迟：使用 channel 异步写入，不阻塞计算路径
//! - 支持恢复：从 WAL 重建因子状态
//!
//! 架构：
//! ```
//! FactorEngine
//!      │
//!      ▼
//! FactorWalPersister (异步通道)
//!      │
//!      ▼
//! WalManager (持久化)
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam::channel::{bounded, Receiver, Sender, TryRecvError};
use parking_lot::RwLock;

use crate::storage::wal::record::WalRecord;

// ═══════════════════════════════════════════════════════════════════════════
// 因子更新消息
// ═══════════════════════════════════════════════════════════════════════════

/// 因子更新消息（发送到 WAL 写入线程）
#[derive(Debug, Clone)]
pub enum FactorWalMessage {
    /// 单因子更新
    Update {
        instrument_id: String,
        factor_id: String,
        value: f64,
        source_timestamp: i64,
    },
    /// 向量因子更新
    VectorUpdate {
        instrument_id: String,
        factor_id: String,
        values: Vec<f64>,
        source_timestamp: i64,
    },
    /// 批量快照
    Snapshot {
        instrument_id: String,
        factors: HashMap<String, f64>,
        checkpoint_id: u64,
    },
    /// 关闭信号
    Shutdown,
}

// ═══════════════════════════════════════════════════════════════════════════
// 因子 WAL 持久化器
// ═══════════════════════════════════════════════════════════════════════════

/// 因子 WAL 持久化器配置
#[derive(Debug, Clone)]
pub struct FactorWalConfig {
    /// 通道缓冲区大小
    pub channel_buffer_size: usize,
    /// 是否启用增量更新持久化
    pub persist_updates: bool,
    /// 快照间隔（更新次数）
    pub snapshot_interval: u64,
    /// 批量写入大小
    pub batch_size: usize,
}

impl Default for FactorWalConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 10000,
            persist_updates: true,
            snapshot_interval: 1000,
            batch_size: 100,
        }
    }
}

/// 因子 WAL 持久化器
///
/// 提供因子数据到 WAL 的异步持久化
pub struct FactorWalPersister {
    /// 发送端（用于因子引擎发送更新）
    tx: Sender<FactorWalMessage>,
    /// 配置
    config: FactorWalConfig,
    /// 更新计数器
    update_count: AtomicU64,
    /// 最后快照 ID
    last_checkpoint_id: AtomicU64,
    /// 是否已关闭
    shutdown: AtomicU64,
}

impl FactorWalPersister {
    /// 创建新的持久化器
    ///
    /// 返回 (持久化器, 接收端)
    /// 接收端用于 WAL 写入线程
    pub fn new(config: FactorWalConfig) -> (Self, Receiver<FactorWalMessage>) {
        let (tx, rx) = bounded(config.channel_buffer_size);

        let persister = Self {
            tx,
            config,
            update_count: AtomicU64::new(0),
            last_checkpoint_id: AtomicU64::new(0),
            shutdown: AtomicU64::new(0),
        };

        (persister, rx)
    }

    /// 使用默认配置创建
    pub fn default_new() -> (Self, Receiver<FactorWalMessage>) {
        Self::new(FactorWalConfig::default())
    }

    /// 持久化单因子更新
    ///
    /// # Arguments
    /// * `instrument_id` - 合约ID
    /// * `factor_id` - 因子ID
    /// * `value` - 因子值
    /// * `source_timestamp` - 数据源时间戳
    #[inline]
    pub fn persist_update(
        &self,
        instrument_id: &str,
        factor_id: &str,
        value: f64,
        source_timestamp: i64,
    ) -> Result<(), String> {
        if !self.config.persist_updates {
            return Ok(());
        }

        let msg = FactorWalMessage::Update {
            instrument_id: instrument_id.to_string(),
            factor_id: factor_id.to_string(),
            value,
            source_timestamp,
        };

        self.tx
            .try_send(msg)
            .map_err(|e| format!("Factor WAL send failed: {}", e))?;

        let count = self.update_count.fetch_add(1, Ordering::Relaxed);

        // 检查是否需要触发快照
        if count > 0 && count % self.config.snapshot_interval == 0 {
            log::debug!("Factor update count {} reached snapshot interval", count);
        }

        Ok(())
    }

    /// 持久化向量因子更新
    #[inline]
    pub fn persist_vector_update(
        &self,
        instrument_id: &str,
        factor_id: &str,
        values: &[f64],
        source_timestamp: i64,
    ) -> Result<(), String> {
        if !self.config.persist_updates {
            return Ok(());
        }

        let msg = FactorWalMessage::VectorUpdate {
            instrument_id: instrument_id.to_string(),
            factor_id: factor_id.to_string(),
            values: values.to_vec(),
            source_timestamp,
        };

        self.tx
            .try_send(msg)
            .map_err(|e| format!("Factor WAL vector send failed: {}", e))?;

        Ok(())
    }

    /// 持久化因子快照
    pub fn persist_snapshot(
        &self,
        instrument_id: &str,
        factors: HashMap<String, f64>,
    ) -> Result<u64, String> {
        let checkpoint_id = self.last_checkpoint_id.fetch_add(1, Ordering::SeqCst) + 1;

        let msg = FactorWalMessage::Snapshot {
            instrument_id: instrument_id.to_string(),
            factors,
            checkpoint_id,
        };

        self.tx
            .try_send(msg)
            .map_err(|e| format!("Factor WAL snapshot send failed: {}", e))?;

        Ok(checkpoint_id)
    }

    /// 批量持久化多个因子更新
    pub fn persist_batch(
        &self,
        instrument_id: &str,
        factors: &HashMap<String, f64>,
        source_timestamp: i64,
    ) -> Result<(), String> {
        if !self.config.persist_updates {
            return Ok(());
        }

        for (factor_id, value) in factors {
            self.persist_update(instrument_id, factor_id, *value, source_timestamp)?;
        }

        Ok(())
    }

    /// 获取更新计数
    pub fn update_count(&self) -> u64 {
        self.update_count.load(Ordering::Relaxed)
    }

    /// 获取最后快照 ID
    pub fn last_checkpoint_id(&self) -> u64 {
        self.last_checkpoint_id.load(Ordering::Relaxed)
    }

    /// 关闭持久化器
    pub fn shutdown(&self) {
        self.shutdown.store(1, Ordering::SeqCst);
        let _ = self.tx.try_send(FactorWalMessage::Shutdown);
    }

    /// 是否已关闭
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed) == 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WAL 记录转换
// ═══════════════════════════════════════════════════════════════════════════

/// 将因子更新消息转换为 WAL 记录
pub fn message_to_wal_record(msg: &FactorWalMessage) -> Option<WalRecord> {
    let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

    match msg {
        FactorWalMessage::Update {
            instrument_id,
            factor_id,
            value,
            source_timestamp,
        } => Some(WalRecord::FactorUpdate {
            instrument_id: WalRecord::to_fixed_array_16(instrument_id),
            factor_id: WalRecord::to_fixed_array_32(factor_id),
            factor_type: 0, // Scalar
            value: *value,
            values: [0.0; 8],
            value_count: 1,
            is_valid: true,
            source_timestamp: *source_timestamp,
            timestamp,
        }),

        FactorWalMessage::VectorUpdate {
            instrument_id,
            factor_id,
            values,
            source_timestamp,
        } => {
            let mut value_arr = [0.0f64; 8];
            let count = values.len().min(8);
            value_arr[..count].copy_from_slice(&values[..count]);

            Some(WalRecord::FactorUpdate {
                instrument_id: WalRecord::to_fixed_array_16(instrument_id),
                factor_id: WalRecord::to_fixed_array_32(factor_id),
                factor_type: 1, // Vector
                value: values.first().copied().unwrap_or(0.0),
                values: value_arr,
                value_count: count as u8,
                is_valid: true,
                source_timestamp: *source_timestamp,
                timestamp,
            })
        }

        FactorWalMessage::Snapshot {
            instrument_id,
            factors,
            checkpoint_id,
        } => {
            let factor_count = factors.len().min(32);
            let mut factor_ids = [[0u8; 32]; 32];
            let mut values_arr = [0.0f64; 32];

            for (i, (fid, val)) in factors.iter().take(32).enumerate() {
                factor_ids[i] = WalRecord::to_fixed_array_32(fid);
                values_arr[i] = *val;
            }

            Some(WalRecord::FactorSnapshot {
                instrument_id: WalRecord::to_fixed_array_16(instrument_id),
                snapshot_type: 1, // 全量
                factor_count: factor_count as u8,
                factor_ids,
                values: values_arr,
                update_count: 0,
                checkpoint_id: *checkpoint_id,
                timestamp,
            })
        }

        FactorWalMessage::Shutdown => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 带 WAL 的因子引擎包装器
// ═══════════════════════════════════════════════════════════════════════════

use super::engine::{FactorEngine, StreamFactorEngine};

/// 带 WAL 持久化的流式因子引擎
///
/// 包装 StreamFactorEngine，自动将更新写入 WAL
pub struct WalStreamFactorEngine {
    /// 内部因子引擎
    engine: StreamFactorEngine,
    /// WAL 持久化器
    persister: Arc<FactorWalPersister>,
    /// 当前合约 ID
    instrument_id: String,
}

impl WalStreamFactorEngine {
    /// 创建带 WAL 的因子引擎
    pub fn new(
        engine: StreamFactorEngine,
        persister: Arc<FactorWalPersister>,
        instrument_id: String,
    ) -> Self {
        Self {
            engine,
            persister,
            instrument_id,
        }
    }

    /// 更新因子并持久化
    #[inline]
    pub fn update(
        &mut self,
        factor_id: &str,
        value: f64,
        source_timestamp: i64,
    ) -> Result<f64, String> {
        // 先计算
        let result = self.engine.update(factor_id, value)?;

        // 异步持久化（不阻塞计算）
        if let Err(e) = self.persister.persist_update(
            &self.instrument_id,
            factor_id,
            result,
            source_timestamp,
        ) {
            log::warn!("Factor WAL persist failed: {}", e);
        }

        Ok(result)
    }

    /// 批量更新并持久化
    pub fn update_all(
        &mut self,
        source_value: f64,
        factor_ids: &[&str],
        source_timestamp: i64,
    ) -> HashMap<String, f64> {
        let results = self.engine.update_all(source_value, factor_ids);

        // 批量持久化
        if let Err(e) = self
            .persister
            .persist_batch(&self.instrument_id, &results, source_timestamp)
        {
            log::warn!("Factor WAL batch persist failed: {}", e);
        }

        results
    }

    /// 创建并持久化快照
    pub fn snapshot(&self) -> Result<u64, String> {
        let factors = self.engine.get_all().clone();
        self.persister.persist_snapshot(&self.instrument_id, factors)
    }

    /// 获取当前因子值
    pub fn get(&self, factor_id: &str) -> Option<f64> {
        self.engine.get(factor_id)
    }

    /// 获取所有当前值
    pub fn get_all(&self) -> &HashMap<String, f64> {
        self.engine.get_all()
    }

    /// 获取内部引擎的可变引用（用于初始化）
    pub fn engine_mut(&mut self) -> &mut StreamFactorEngine {
        &mut self.engine
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WAL 消费者（用于 WAL 写入线程）
// ═══════════════════════════════════════════════════════════════════════════

use crate::storage::wal::manager::WalManager;

/// 因子 WAL 消费者
///
/// 在单独的线程中运行，从 channel 接收因子更新并写入 WAL
pub struct FactorWalConsumer {
    rx: Receiver<FactorWalMessage>,
    wal_manager: Arc<RwLock<WalManager>>,
    batch_buffer: Vec<WalRecord>,
    batch_size: usize,
}

impl FactorWalConsumer {
    pub fn new(
        rx: Receiver<FactorWalMessage>,
        wal_manager: Arc<RwLock<WalManager>>,
        batch_size: usize,
    ) -> Self {
        Self {
            rx,
            wal_manager,
            batch_buffer: Vec::with_capacity(batch_size),
            batch_size,
        }
    }

    /// 运行消费循环（阻塞）
    pub fn run(&mut self) {
        log::info!("Factor WAL consumer started");

        loop {
            match self.rx.recv() {
                Ok(msg) => {
                    if matches!(msg, FactorWalMessage::Shutdown) {
                        // 刷新剩余数据
                        self.flush();
                        log::info!("Factor WAL consumer shutdown");
                        break;
                    }

                    if let Some(record) = message_to_wal_record(&msg) {
                        self.batch_buffer.push(record);

                        // 达到批量大小则写入
                        if self.batch_buffer.len() >= self.batch_size {
                            self.flush();
                        }
                    }
                }
                Err(_) => {
                    // Channel 关闭
                    self.flush();
                    log::info!("Factor WAL consumer channel closed");
                    break;
                }
            }
        }
    }

    /// 非阻塞运行一轮
    pub fn poll(&mut self) -> bool {
        let mut processed = false;

        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    if matches!(msg, FactorWalMessage::Shutdown) {
                        self.flush();
                        return false;
                    }

                    if let Some(record) = message_to_wal_record(&msg) {
                        self.batch_buffer.push(record);
                        processed = true;

                        if self.batch_buffer.len() >= self.batch_size {
                            self.flush();
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.flush();
                    return false;
                }
            }
        }

        // 定期刷新（即使未满批量）
        if !self.batch_buffer.is_empty() {
            self.flush();
        }

        processed
    }

    /// 刷新缓冲区到 WAL
    fn flush(&mut self) {
        if self.batch_buffer.is_empty() {
            return;
        }

        let wal = self.wal_manager.read();

        for record in self.batch_buffer.drain(..) {
            // 使用异步写入，利用 group commit 机制
            if let Err(e) = wal.append_async(record) {
                log::error!("Factor WAL append failed: {}", e);
            }
        }

        // 触发 group commit 刷盘
        if let Err(e) = wal.flush_group_commit() {
            log::error!("Factor WAL flush failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factor_wal_message_to_record() {
        let msg = FactorWalMessage::Update {
            instrument_id: "cu2501".to_string(),
            factor_id: "ma_5".to_string(),
            value: 75000.5,
            source_timestamp: 1234567890,
        };

        let record = message_to_wal_record(&msg);
        assert!(record.is_some());

        match record.unwrap() {
            WalRecord::FactorUpdate {
                factor_type,
                value,
                is_valid,
                ..
            } => {
                assert_eq!(factor_type, 0);
                assert!((value - 75000.5).abs() < 0.001);
                assert!(is_valid);
            }
            _ => panic!("Expected FactorUpdate"),
        }
    }

    #[test]
    fn test_factor_wal_vector_message() {
        let msg = FactorWalMessage::VectorUpdate {
            instrument_id: "cu2501".to_string(),
            factor_id: "macd".to_string(),
            values: vec![1.0, 2.0, 3.0],
            source_timestamp: 1234567890,
        };

        let record = message_to_wal_record(&msg);
        assert!(record.is_some());

        match record.unwrap() {
            WalRecord::FactorUpdate {
                factor_type,
                value_count,
                values,
                ..
            } => {
                assert_eq!(factor_type, 1);
                assert_eq!(value_count, 3);
                assert!((values[0] - 1.0).abs() < 0.001);
                assert!((values[1] - 2.0).abs() < 0.001);
                assert!((values[2] - 3.0).abs() < 0.001);
            }
            _ => panic!("Expected FactorUpdate"),
        }
    }

    #[test]
    fn test_factor_wal_snapshot_message() {
        let mut factors = HashMap::new();
        factors.insert("ma_5".to_string(), 100.0);
        factors.insert("ma_10".to_string(), 101.0);

        let msg = FactorWalMessage::Snapshot {
            instrument_id: "cu2501".to_string(),
            factors,
            checkpoint_id: 42,
        };

        let record = message_to_wal_record(&msg);
        assert!(record.is_some());

        match record.unwrap() {
            WalRecord::FactorSnapshot {
                factor_count,
                checkpoint_id,
                ..
            } => {
                assert_eq!(factor_count, 2);
                assert_eq!(checkpoint_id, 42);
            }
            _ => panic!("Expected FactorSnapshot"),
        }
    }

    #[test]
    fn test_persister_channel() {
        let (persister, rx) = FactorWalPersister::default_new();

        // 发送更新
        persister
            .persist_update("cu2501", "ma_5", 100.0, 1234567890)
            .unwrap();

        // 接收更新
        let msg = rx.try_recv().unwrap();
        match msg {
            FactorWalMessage::Update {
                instrument_id,
                factor_id,
                value,
                ..
            } => {
                assert_eq!(instrument_id, "cu2501");
                assert_eq!(factor_id, "ma_5");
                assert!((value - 100.0).abs() < 0.001);
            }
            _ => panic!("Expected Update message"),
        }

        assert_eq!(persister.update_count(), 1);
    }
}
