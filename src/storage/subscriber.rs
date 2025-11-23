//! 存储订阅器 - 异步持久化
//!
//! **架构设计**: Event Sourcing + 异步存储
//!
//! ```
//! 主交易流程 (P99 < 100μs)
//! ├─ OrderRouter → MatchingEngine → TradeGateway
//! ├─ 生成 Notification (rkyv 零拷贝)
//! └─ 发送到 Channel (无阻塞)
//!            ↓
//!   [异步边界 - 完全解耦]
//!            ↓
//! 存储订阅器 (独立 Tokio 任务)
//! ├─ 接收 Notification
//! ├─ 转换为 WalRecord
//! └─ 写入 Storage (WAL + MemTable)
//! ```
//!
//! **优势**:
//! 1. 主流程零阻塞 (只有 channel send，~100ns)
//! 2. 存储可以批量写入 (提升吞吐)
//! 3. 存储故障不影响交易
//! 4. 可扩展到 iceoryx2 跨进程分发

use crate::notification::message::{Notification, NotificationPayload};
use crate::storage::hybrid::oltp::{OltpHybridConfig, OltpHybridStorage};
use crate::storage::wal::record::WalRecord;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

/// 存储订阅器配置
#[derive(Debug, Clone)]
pub struct StorageSubscriberConfig {
    /// Storage 配置
    pub storage_config: OltpHybridConfig,

    /// 批量写入大小（条数）
    pub batch_size: usize,

    /// 批量写入超时（毫秒）
    pub batch_timeout_ms: u64,

    /// 缓冲区大小
    pub buffer_size: usize,
}

impl Default for StorageSubscriberConfig {
    fn default() -> Self {
        Self {
            storage_config: OltpHybridConfig::default(),
            batch_size: 1000,     // 批量 1000 条
            batch_timeout_ms: 10, // 10ms 超时
            buffer_size: 10000,   // 缓冲 10K 条
        }
    }
}

/// 存储订阅器（独立 Tokio 任务）
pub struct StorageSubscriber {
    /// 品种 → Storage 映射
    storages: HashMap<String, Arc<OltpHybridStorage>>,

    /// 接收通知的 Channel（使用unbounded避免阻塞生产者）
    receiver: mpsc::UnboundedReceiver<Notification>,

    /// 配置
    config: StorageSubscriberConfig,

    /// 统计信息
    stats: Arc<parking_lot::Mutex<SubscriberStats>>,
}

/// 订阅器统计
#[derive(Debug, Default)]
pub struct SubscriberStats {
    pub total_received: u64,
    pub total_persisted: u64,
    pub total_batches: u64,
    pub total_errors: u64,
    pub last_error: Option<String>,
}

impl StorageSubscriber {
    /// 创建订阅器
    ///
    /// 返回：(订阅器实例, 通知发送器, 统计信息句柄)
    pub fn new(
        config: StorageSubscriberConfig,
    ) -> (
        Self,
        mpsc::UnboundedSender<Notification>,
        Arc<parking_lot::Mutex<SubscriberStats>>,
    ) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let stats = Arc::new(parking_lot::Mutex::new(SubscriberStats::default()));

        let subscriber = Self {
            storages: HashMap::new(),
            receiver,
            config,
            stats: stats.clone(),
        };

        (subscriber, sender, stats)
    }

    /// 获取或创建品种的 Storage
    fn get_or_create_storage(
        &mut self,
        instrument_id: &str,
    ) -> Result<Arc<OltpHybridStorage>, String> {
        if let Some(storage) = self.storages.get(instrument_id) {
            return Ok(storage.clone());
        }

        let storage = Arc::new(OltpHybridStorage::create(
            instrument_id,
            self.config.storage_config.clone(),
        )?);

        self.storages
            .insert(instrument_id.to_string(), storage.clone());
        Ok(storage)
    }

    /// 启动订阅器（阻塞运行）
    pub async fn run(mut self) {
        log::info!("StorageSubscriber started");

        let mut batch_buffer = Vec::with_capacity(self.config.batch_size);
        let mut flush_timer = interval(Duration::from_millis(self.config.batch_timeout_ms));

        loop {
            tokio::select! {
                // 接收通知
                Some(notification) = self.receiver.recv() => {
                    self.stats.lock().total_received += 1;
                    batch_buffer.push(notification);

                    // 达到批量大小立即 flush
                    if batch_buffer.len() >= self.config.batch_size {
                        self.flush_batch(&mut batch_buffer).await;
                    }
                }

                // 超时 flush
                _ = flush_timer.tick() => {
                    if !batch_buffer.is_empty() {
                        self.flush_batch(&mut batch_buffer).await;
                    }
                }
            }
        }
    }

    /// Flush 批量写入
    async fn flush_batch(&mut self, batch: &mut Vec<Notification>) {
        if batch.is_empty() {
            return;
        }

        let start = std::time::Instant::now();

        // 按品种分组
        let mut grouped: HashMap<String, Vec<WalRecord>> = HashMap::new();

        for notification in batch.drain(..) {
            if let Some((instrument_id, wal_record)) = self.convert_notification(notification) {
                grouped.entry(instrument_id).or_default().push(wal_record);
            }
        }

        // 批量写入各品种
        let mut total_persisted = 0;
        for (instrument_id, records) in grouped {
            match self.get_or_create_storage(&instrument_id) {
                Ok(storage) => match storage.write_batch(records.clone()) {
                    Ok(sequences) => {
                        total_persisted += sequences.len();
                        log::debug!(
                            "Persisted {} records for {} in {:?}",
                            sequences.len(),
                            instrument_id,
                            start.elapsed()
                        );
                    }
                    Err(e) => {
                        log::error!("Failed to persist batch for {}: {}", instrument_id, e);
                        let mut stats = self.stats.lock();
                        stats.total_errors += 1;
                        stats.last_error = Some(e);
                    }
                },
                Err(e) => {
                    log::error!("Failed to get storage for {}: {}", instrument_id, e);
                    let mut stats = self.stats.lock();
                    stats.total_errors += 1;
                    stats.last_error = Some(e);
                }
            }
        }

        // 更新统计
        let mut stats = self.stats.lock();
        stats.total_persisted += total_persisted as u64;
        stats.total_batches += 1;

        log::info!(
            "Batch flush: {} records in {:?} (total: {} received, {} persisted, {} errors)",
            total_persisted,
            start.elapsed(),
            stats.total_received,
            stats.total_persisted,
            stats.total_errors
        );
    }

    /// 转换 Notification → WalRecord
    fn convert_notification(&self, notification: Notification) -> Option<(String, WalRecord)> {
        match notification.payload {
            // 账户开户通知 -> WAL AccountOpen
            NotificationPayload::AccountOpen(account_open) => {
                let mut account_id_bytes = [0u8; 64];
                let acc_id_bytes = account_open.account_id.as_bytes();
                let acc_id_len = acc_id_bytes.len().min(64);
                account_id_bytes[..acc_id_len].copy_from_slice(&acc_id_bytes[..acc_id_len]);

                let mut user_id_bytes = [0u8; 32];
                let user_bytes = account_open.user_id.as_bytes();
                let copy_len = user_bytes.len().min(32);
                user_id_bytes[..copy_len].copy_from_slice(&user_bytes[..copy_len]);

                let mut account_name_bytes = [0u8; 64];
                let name_bytes = account_open.account_name.as_bytes();
                let name_copy_len = name_bytes.len().min(64);
                account_name_bytes[..name_copy_len].copy_from_slice(&name_bytes[..name_copy_len]);

                let record = WalRecord::AccountOpen {
                    account_id: account_id_bytes,
                    user_id: user_id_bytes,
                    account_name: account_name_bytes,
                    init_cash: account_open.init_cash,
                    account_type: account_open.account_type,
                    timestamp: account_open.timestamp,
                };

                // AccountOpen 使用特殊标记
                Some(("__ACCOUNT__".to_string(), record))
            }

            // 账户更新通知 -> WAL AccountUpdate
            NotificationPayload::AccountUpdate(account) => {
                let mut user_id_bytes = [0u8; 32];
                let user_bytes = account.user_id.as_bytes();
                let copy_len = user_bytes.len().min(32);
                user_id_bytes[..copy_len].copy_from_slice(&user_bytes[..copy_len]);

                let record = WalRecord::AccountUpdate {
                    user_id: user_id_bytes,
                    balance: account.balance,
                    available: account.available,
                    frozen: account.frozen,
                    margin: account.margin,
                    timestamp: account.timestamp,
                };

                // AccountUpdate 使用特殊标记
                Some(("__ACCOUNT__".to_string(), record))
            }

            // 成交通知 -> WAL TradeExecuted
            NotificationPayload::TradeExecuted(trade) => {
                let record = WalRecord::TradeExecuted {
                    trade_id: self.parse_id(&trade.trade_id),
                    order_id: self.parse_id(&trade.order_id),
                    exchange_order_id: self.parse_id(&trade.exchange_order_id),
                    price: trade.price,
                    volume: trade.volume,
                    timestamp: trade.timestamp,
                };
                Some((trade.instrument_id, record))
            }

            // 订单接受通知 -> WAL OrderInsert
            NotificationPayload::OrderAccepted(order) => {
                let mut user_id_bytes = [0u8; 32];
                let mut instrument_id_bytes = [0u8; 16];

                // 从 exchange_order_id 提取 user_id (简化实现)
                let user_bytes = order.exchange_order_id.as_bytes();
                let copy_len = user_bytes.len().min(32);
                user_id_bytes[..copy_len].copy_from_slice(&user_bytes[..copy_len]);

                let inst_bytes = order.instrument_id.as_bytes();
                let inst_copy_len = inst_bytes.len().min(16);
                instrument_id_bytes[..inst_copy_len].copy_from_slice(&inst_bytes[..inst_copy_len]);

                let record = WalRecord::OrderInsert {
                    order_id: self.parse_id(&order.order_id),
                    user_id: user_id_bytes,
                    instrument_id: instrument_id_bytes,
                    direction: if order.direction == "BUY" { 0 } else { 1 },
                    offset: if order.offset == "OPEN" { 0 } else { 1 },
                    price: order.price,
                    volume: order.volume,
                    timestamp: order.timestamp,
                };
                Some((order.instrument_id, record))
            }

            // 其他通知暂不持久化
            _ => None,
        }
    }

    /// 解析 ID (简化实现)
    fn parse_id(&self, id: &str) -> u64 {
        id.chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> SubscriberStats {
        self.stats.lock().clone()
    }
}

/// 克隆实现
impl Clone for SubscriberStats {
    fn clone(&self) -> Self {
        Self {
            total_received: self.total_received,
            total_persisted: self.total_persisted,
            total_batches: self.total_batches,
            total_errors: self.total_errors,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::message::{
        NotificationPayload, NotificationType, TradeExecutedNotify,
    };

    #[tokio::test]
    async fn test_storage_subscriber() {
        let config = StorageSubscriberConfig {
            batch_size: 10,
            batch_timeout_ms: 100,
            ..Default::default()
        };

        let (subscriber, sender, _stats) = StorageSubscriber::new(config);

        // 启动订阅器
        tokio::spawn(async move {
            subscriber.run().await;
        });

        // 发送测试通知
        for i in 0..50 {
            let payload = NotificationPayload::TradeExecuted(TradeExecutedNotify {
                trade_id: format!("T{}", i),
                order_id: format!("O{}", i),
                exchange_order_id: format!("EX_O{}", i),
                instrument_id: "IF2501".to_string(),
                direction: "BUY".to_string(),
                offset: "OPEN".to_string(),
                price: 3800.0,
                volume: 10.0,
                commission: 10.0,
                fill_type: "FULL".to_string(),
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            });

            let notification = Notification::new(
                NotificationType::TradeExecuted,
                Arc::from("test_user"),
                payload,
                "TestSuite",
            );

            sender.send(notification).unwrap();
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
