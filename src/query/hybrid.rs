//! 流批混合查询
//!
//! @yutiansut @quantaxis
//!
//! 提供流式和批处理混合的查询能力：
//! - 实时流查询 (低延迟)
//! - 批量历史查询 (高吞吐)
//! - 流批结果合并

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::sync::mpsc;

// ═══════════════════════════════════════════════════════════════════════════
// 流批混合查询类型
// ═══════════════════════════════════════════════════════════════════════════

/// 数据记录
#[derive(Debug, Clone)]
pub struct Record {
    pub key: String,
    pub timestamp: i64,
    pub values: HashMap<String, RecordValue>,
}

/// 记录值
#[derive(Debug, Clone)]
pub enum RecordValue {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

impl Record {
    pub fn new(key: impl Into<String>, timestamp: i64) -> Self {
        Self {
            key: key.into(),
            timestamp,
            values: HashMap::new(),
        }
    }

    pub fn with_value(mut self, field: impl Into<String>, value: RecordValue) -> Self {
        self.values.insert(field.into(), value);
        self
    }

    pub fn get(&self, field: &str) -> Option<&RecordValue> {
        self.values.get(field)
    }

    pub fn get_float(&self, field: &str) -> Option<f64> {
        match self.values.get(field)? {
            RecordValue::Float(f) => Some(*f),
            RecordValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
}

/// 查询结果
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub records: Vec<Record>,
    pub source: DataSource,
    pub execution_time: Duration,
    pub is_complete: bool,
}

/// 数据来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Stream,
    Batch,
    Merged,
}

// ═══════════════════════════════════════════════════════════════════════════
// 流数据源
// ═══════════════════════════════════════════════════════════════════════════

/// 流数据缓存
pub struct StreamBuffer {
    /// 数据缓存 (按 key 分组)
    buffer: DashMap<String, Vec<Record>>,
    /// 最大缓存时间
    max_age: Duration,
    /// 最大缓存条数
    max_size: usize,
}

impl StreamBuffer {
    pub fn new(max_age: Duration, max_size: usize) -> Self {
        Self {
            buffer: DashMap::new(),
            max_age,
            max_size,
        }
    }

    /// 添加记录
    pub fn push(&self, record: Record) {
        let key = record.key.clone();
        self.buffer.entry(key.clone()).or_insert_with(Vec::new).push(record);

        // 限制大小
        if let Some(mut entry) = self.buffer.get_mut(&key) {
            while entry.len() > self.max_size {
                entry.remove(0);
            }
        }
    }

    /// 查询最新 N 条记录
    pub fn query_latest(&self, key: &str, limit: usize) -> Vec<Record> {
        self.buffer
            .get(key)
            .map(|records| {
                let start = records.len().saturating_sub(limit);
                records[start..].to_vec()
            })
            .unwrap_or_default()
    }

    /// 查询时间范围内的记录
    pub fn query_range(&self, key: &str, start_ts: i64, end_ts: i64) -> Vec<Record> {
        self.buffer
            .get(key)
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.timestamp >= start_ts && r.timestamp <= end_ts)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 清理过期数据
    pub fn cleanup(&self, cutoff_ts: i64) {
        for mut entry in self.buffer.iter_mut() {
            entry.retain(|r| r.timestamp >= cutoff_ts);
        }
    }

    /// 获取所有 key
    pub fn keys(&self) -> Vec<String> {
        self.buffer.iter().map(|r| r.key().clone()).collect()
    }

    /// 缓存大小
    pub fn len(&self) -> usize {
        self.buffer.iter().map(|r| r.value().len()).sum()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for StreamBuffer {
    fn default() -> Self {
        Self::new(Duration::from_secs(300), 10000)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 批处理数据源接口
// ═══════════════════════════════════════════════════════════════════════════

/// 批处理查询接口
#[async_trait::async_trait]
pub trait BatchDataSource: Send + Sync {
    /// 查询历史数据
    async fn query(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        fields: &[String],
    ) -> Result<Vec<Record>, BatchQueryError>;

    /// 聚合查询
    async fn aggregate(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        aggregations: &[Aggregation],
    ) -> Result<AggregateResult, BatchQueryError>;
}

/// 批查询错误
#[derive(Debug)]
pub enum BatchQueryError {
    NotFound(String),
    Timeout,
    IoError(String),
    Other(String),
}

impl std::fmt::Display for BatchQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchQueryError::NotFound(msg) => write!(f, "Not found: {}", msg),
            BatchQueryError::Timeout => write!(f, "Query timeout"),
            BatchQueryError::IoError(msg) => write!(f, "IO error: {}", msg),
            BatchQueryError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

/// 聚合操作
#[derive(Debug, Clone)]
pub struct Aggregation {
    pub field: String,
    pub op: AggregateOp,
    pub alias: String,
}

/// 聚合操作类型
#[derive(Debug, Clone, Copy)]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    First,
    Last,
}

/// 聚合结果
#[derive(Debug, Clone, Default)]
pub struct AggregateResult {
    pub values: HashMap<String, f64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// 混合查询引擎
// ═══════════════════════════════════════════════════════════════════════════

/// 混合查询引擎
pub struct HybridQueryEngine {
    /// 流数据缓存
    stream_buffer: Arc<StreamBuffer>,
    /// 批处理数据源
    batch_source: Option<Arc<dyn BatchDataSource>>,
    /// 配置
    config: HybridConfig,
    /// 订阅者管理器 @yutiansut @quantaxis
    subscribers: Arc<DashMap<String, Vec<mpsc::Sender<Record>>>>,
}

/// 混合查询配置
#[derive(Debug, Clone)]
pub struct HybridConfig {
    /// 流数据最大延迟
    pub stream_max_latency: Duration,
    /// 批查询超时
    pub batch_timeout: Duration,
    /// 合并策略
    pub merge_strategy: MergeStrategy,
    /// 流数据优先时间窗口 (此时间内优先使用流数据)
    pub stream_priority_window: Duration,
}

/// 合并策略
#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    /// 流数据优先
    StreamFirst,
    /// 批数据优先
    BatchFirst,
    /// 按时间戳合并
    ByTimestamp,
    /// 取最新
    Latest,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            stream_max_latency: Duration::from_millis(100),
            batch_timeout: Duration::from_secs(30),
            merge_strategy: MergeStrategy::ByTimestamp,
            stream_priority_window: Duration::from_secs(60),
        }
    }
}

impl HybridQueryEngine {
    pub fn new(config: HybridConfig) -> Self {
        Self {
            stream_buffer: Arc::new(StreamBuffer::default()),
            batch_source: None,
            config,
            subscribers: Arc::new(DashMap::new()),
        }
    }

    /// 设置批处理数据源
    pub fn with_batch_source(mut self, source: Arc<dyn BatchDataSource>) -> Self {
        self.batch_source = Some(source);
        self
    }

    /// 获取流缓存引用
    pub fn stream_buffer(&self) -> Arc<StreamBuffer> {
        Arc::clone(&self.stream_buffer)
    }

    /// 推送流数据（同时通知订阅者）@yutiansut @quantaxis
    pub fn push_stream(&self, record: Record) {
        let key = record.key.clone();
        self.stream_buffer.push(record.clone());

        // 通知订阅该 key 的所有订阅者
        if let Some(mut senders) = self.subscribers.get_mut(&key) {
            // 移除已关闭的订阅者
            senders.retain(|tx| !tx.is_closed());

            // 发送给所有活跃订阅者
            for tx in senders.iter() {
                // 使用 try_send 避免阻塞
                let _ = tx.try_send(record.clone());
            }
        }
    }

    /// 执行混合查询
    pub async fn query(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        fields: &[String],
    ) -> Result<QueryResult, HybridQueryError> {
        let start_time = Instant::now();

        // 计算流数据和批数据的时间边界
        let now = chrono::Utc::now().timestamp();
        let stream_cutoff = now - self.config.stream_priority_window.as_secs() as i64;

        // 并行查询流和批数据
        let stream_future = async {
            let stream_start = start_ts.max(stream_cutoff);
            if stream_start < end_ts {
                self.stream_buffer.query_range(key, stream_start, end_ts)
            } else {
                Vec::new()
            }
        };

        let batch_future = async {
            if let Some(batch_source) = &self.batch_source {
                let batch_end = end_ts.min(stream_cutoff);
                if start_ts < batch_end {
                    batch_source
                        .query(key, start_ts, batch_end, fields)
                        .await
                        .ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        let (stream_records, batch_records) = tokio::join!(stream_future, batch_future);

        // 合并结果
        let merged = self.merge_results(stream_records, batch_records.unwrap_or_default());

        Ok(QueryResult {
            records: merged,
            source: DataSource::Merged,
            execution_time: start_time.elapsed(),
            is_complete: true,
        })
    }

    /// 合并流和批数据
    fn merge_results(&self, stream: Vec<Record>, batch: Vec<Record>) -> Vec<Record> {
        match self.config.merge_strategy {
            MergeStrategy::StreamFirst => {
                let mut result = stream;
                // 添加批数据中不在流数据时间范围内的记录
                let stream_min_ts = result.iter().map(|r| r.timestamp).min().unwrap_or(i64::MAX);
                result.extend(batch.into_iter().filter(|r| r.timestamp < stream_min_ts));
                result.sort_by_key(|r| r.timestamp);
                result
            }
            MergeStrategy::BatchFirst => {
                let mut result = batch;
                // 添加流数据中不在批数据时间范围内的记录
                let batch_max_ts = result.iter().map(|r| r.timestamp).max().unwrap_or(i64::MIN);
                result.extend(stream.into_iter().filter(|r| r.timestamp > batch_max_ts));
                result.sort_by_key(|r| r.timestamp);
                result
            }
            MergeStrategy::ByTimestamp => {
                let mut result = Vec::with_capacity(stream.len() + batch.len());
                result.extend(batch);
                result.extend(stream);
                result.sort_by_key(|r| r.timestamp);
                // 去重 (保留最新)
                result.dedup_by(|a, b| a.key == b.key && a.timestamp == b.timestamp);
                result
            }
            MergeStrategy::Latest => {
                let mut result = Vec::with_capacity(stream.len() + batch.len());
                result.extend(batch);
                result.extend(stream);
                result.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
                result
            }
        }
    }

    /// 执行流式订阅 @yutiansut @quantaxis
    ///
    /// 订阅指定 key 的数据流，当有新数据推送时会通过 channel 通知
    ///
    /// # 参数
    /// - `key`: 订阅的数据键（如合约ID）
    ///
    /// # 返回值
    /// - `mpsc::Receiver<Record>`: 接收新数据的 channel
    pub fn subscribe(&self, key: String) -> mpsc::Receiver<Record> {
        let (tx, rx) = mpsc::channel(1000);

        // 将发送端添加到订阅者列表
        self.subscribers
            .entry(key)
            .or_insert_with(Vec::new)
            .push(tx);

        rx
    }

    /// 取消订阅（通过丢弃 receiver 自动完成）@yutiansut @quantaxis
    ///
    /// 注意：当 receiver 被丢弃时，对应的 sender 的 try_send 会返回错误，
    /// 下次 push_stream 时会自动清理已关闭的订阅者
    pub fn unsubscribe(&self, key: &str) {
        if let Some(mut senders) = self.subscribers.get_mut(key) {
            // 移除所有已关闭的 channel
            senders.retain(|tx| !tx.is_closed());
        }
    }

    /// 获取订阅者数量 @yutiansut @quantaxis
    pub fn subscriber_count(&self, key: &str) -> usize {
        self.subscribers
            .get(key)
            .map(|s| s.iter().filter(|tx| !tx.is_closed()).count())
            .unwrap_or(0)
    }

    /// 批量订阅多个 key @yutiansut @quantaxis
    pub fn subscribe_many(&self, keys: Vec<String>) -> HashMap<String, mpsc::Receiver<Record>> {
        keys.into_iter()
            .map(|key| {
                let rx = self.subscribe(key.clone());
                (key, rx)
            })
            .collect()
    }
}

impl Default for HybridQueryEngine {
    fn default() -> Self {
        Self::new(HybridConfig::default())
    }
}

/// 混合查询错误
#[derive(Debug)]
pub enum HybridQueryError {
    StreamError(String),
    BatchError(BatchQueryError),
    MergeError(String),
    Timeout,
}

impl std::fmt::Display for HybridQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HybridQueryError::StreamError(msg) => write!(f, "Stream error: {}", msg),
            HybridQueryError::BatchError(e) => write!(f, "Batch error: {}", e),
            HybridQueryError::MergeError(msg) => write!(f, "Merge error: {}", msg),
            HybridQueryError::Timeout => write!(f, "Query timeout"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_buffer() {
        let buffer = StreamBuffer::default();

        // 添加记录
        for i in 0..100 {
            let record = Record::new("test_key", i * 1000)
                .with_value("price", RecordValue::Float(100.0 + i as f64));
            buffer.push(record);
        }

        // 查询最新 10 条
        let latest = buffer.query_latest("test_key", 10);
        assert_eq!(latest.len(), 10);
        assert_eq!(latest[0].timestamp, 90000);

        // 查询时间范围
        let range = buffer.query_range("test_key", 50000, 60000);
        assert_eq!(range.len(), 11); // 50, 51, ..., 60
    }

    #[test]
    fn test_record() {
        let record = Record::new("key1", 1234567890)
            .with_value("price", RecordValue::Float(100.5))
            .with_value("volume", RecordValue::Int(1000))
            .with_value("symbol", RecordValue::String("cu2501".to_string()));

        assert_eq!(record.get_float("price"), Some(100.5));
        assert_eq!(record.get_float("volume"), Some(1000.0));
    }

    #[tokio::test]
    async fn test_hybrid_engine() {
        let engine = HybridQueryEngine::default();

        // 添加流数据
        let now = chrono::Utc::now().timestamp();
        for i in 0..10 {
            let record = Record::new("cu2501", now - 10 + i)
                .with_value("price", RecordValue::Float(80000.0 + i as f64 * 10.0));
            engine.push_stream(record);
        }

        // 查询
        let result = engine
            .query("cu2501", now - 20, now, &["price".to_string()])
            .await
            .unwrap();

        assert!(!result.records.is_empty());
        assert_eq!(result.source, DataSource::Merged);
    }

    // ==================== 订阅逻辑测试 @yutiansut @quantaxis ====================

    /// 测试单个订阅功能
    /// 业务场景: 前端订阅某个合约的实时行情数据流
    ///
    /// 订阅机制:
    ///   1. subscribe(key) 创建一个 mpsc channel (容量 1000)
    ///   2. 返回 Receiver 给调用方用于接收数据
    ///   3. Sender 存储在 subscribers DashMap 中
    ///   4. 当有新数据 push_stream 时，遍历所有 Sender 发送数据
    #[tokio::test]
    async fn test_subscribe_single_key() {
        let engine = HybridQueryEngine::default();

        // 订阅合约 cu2501
        let mut rx = engine.subscribe("cu2501".to_string());

        // 验证订阅者计数
        // subscriber_count 统计该 key 下活跃的 channel 数量
        assert_eq!(engine.subscriber_count("cu2501"), 1, "订阅后应有1个订阅者");
        assert_eq!(engine.subscriber_count("au2501"), 0, "未订阅的key应为0");

        // 推送数据并验证接收
        // push_stream 会同时:
        //   1. 将数据存入 stream_buffer
        //   2. 通过 try_send 发送给所有订阅者
        let record = Record::new("cu2501", 1000)
            .with_value("price", RecordValue::Float(80000.0));
        engine.push_stream(record);

        // 异步接收数据
        // Receiver.recv() 会阻塞直到有数据或 channel 关闭
        let received = rx.recv().await;
        assert!(received.is_some(), "应收到推送的数据");

        let data = received.unwrap();
        assert_eq!(data.key, "cu2501", "key 应匹配");
        assert_eq!(data.timestamp, 1000, "timestamp 应匹配");
        assert_eq!(data.get_float("price"), Some(80000.0), "price 应匹配");
    }

    /// 测试多个订阅者订阅同一个 key
    /// 业务场景: 多个前端客户端同时订阅同一合约
    ///
    /// 并发安全:
    ///   - subscribers 使用 DashMap，支持并发读写
    ///   - 每个订阅者有独立的 channel，互不影响
    ///   - push_stream 会广播给所有订阅者
    #[tokio::test]
    async fn test_subscribe_multiple_subscribers() {
        let engine = HybridQueryEngine::default();

        // 3个客户端同时订阅 cu2501
        let mut rx1 = engine.subscribe("cu2501".to_string());
        let mut rx2 = engine.subscribe("cu2501".to_string());
        let mut rx3 = engine.subscribe("cu2501".to_string());

        // 验证订阅者计数
        assert_eq!(engine.subscriber_count("cu2501"), 3, "应有3个订阅者");

        // 推送1条数据
        let record = Record::new("cu2501", 2000)
            .with_value("price", RecordValue::Float(81000.0));
        engine.push_stream(record);

        // 所有订阅者都应收到数据
        // 这是广播模式：一份数据发送给所有订阅者
        let r1 = rx1.recv().await;
        let r2 = rx2.recv().await;
        let r3 = rx3.recv().await;

        assert!(r1.is_some() && r2.is_some() && r3.is_some(),
            "所有订阅者都应收到数据");

        // 验证数据内容一致
        assert_eq!(r1.unwrap().get_float("price"), Some(81000.0));
        assert_eq!(r2.unwrap().get_float("price"), Some(81000.0));
        assert_eq!(r3.unwrap().get_float("price"), Some(81000.0));
    }

    /// 测试订阅不同的 key
    /// 业务场景: 同一客户端订阅多个合约
    ///
    /// 数据隔离:
    ///   - 不同 key 的订阅者完全独立
    ///   - push_stream 只通知匹配 key 的订阅者
    #[tokio::test]
    async fn test_subscribe_different_keys() {
        let engine = HybridQueryEngine::default();

        // 订阅不同合约
        let mut rx_cu = engine.subscribe("cu2501".to_string());
        let mut rx_au = engine.subscribe("au2501".to_string());

        // 验证各自的订阅者计数
        assert_eq!(engine.subscriber_count("cu2501"), 1);
        assert_eq!(engine.subscriber_count("au2501"), 1);

        // 推送 cu2501 数据
        let cu_record = Record::new("cu2501", 3000)
            .with_value("price", RecordValue::Float(82000.0));
        engine.push_stream(cu_record);

        // cu2501 订阅者应收到数据
        let cu_received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx_cu.recv()
        ).await;
        assert!(cu_received.is_ok(), "cu2501订阅者应收到数据");

        // au2501 订阅者不应收到 cu2501 的数据
        // 使用 timeout 避免无限等待
        let au_received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx_au.recv()
        ).await;
        assert!(au_received.is_err(), "au2501订阅者不应收到cu2501数据");

        // 推送 au2501 数据
        let au_record = Record::new("au2501", 4000)
            .with_value("price", RecordValue::Float(650.0));
        engine.push_stream(au_record);

        // 现在 au2501 订阅者应收到数据
        let au_received2 = rx_au.recv().await;
        assert!(au_received2.is_some(), "au2501订阅者应收到自己的数据");
        assert_eq!(au_received2.unwrap().get_float("price"), Some(650.0));
    }

    /// 测试取消订阅（通过丢弃 Receiver）
    /// 业务场景: 客户端断开连接或主动取消订阅
    ///
    /// 自动清理机制:
    ///   1. 当 Receiver 被 drop，对应的 channel 关闭
    ///   2. Sender.is_closed() 返回 true
    ///   3. 下次 push_stream 时，senders.retain() 会移除已关闭的 Sender
    ///   4. 调用 unsubscribe() 可以立即清理
    #[tokio::test]
    async fn test_unsubscribe_by_drop() {
        let engine = HybridQueryEngine::default();

        // 创建订阅
        let rx = engine.subscribe("cu2501".to_string());
        assert_eq!(engine.subscriber_count("cu2501"), 1);

        // 丢弃 Receiver 触发取消订阅
        drop(rx);

        // 立即调用 unsubscribe 清理
        // 这会遍历 senders 并移除 is_closed() 为 true 的
        engine.unsubscribe("cu2501");

        // 验证订阅者已被清理
        assert_eq!(engine.subscriber_count("cu2501"), 0, "取消订阅后应为0");
    }

    /// 测试部分订阅者取消订阅
    /// 业务场景: 部分客户端断开，其他客户端继续接收数据
    #[tokio::test]
    async fn test_partial_unsubscribe() {
        let engine = HybridQueryEngine::default();

        // 3个订阅者
        let rx1 = engine.subscribe("cu2501".to_string());
        let mut rx2 = engine.subscribe("cu2501".to_string());
        let rx3 = engine.subscribe("cu2501".to_string());

        assert_eq!(engine.subscriber_count("cu2501"), 3);

        // 丢弃 rx1 和 rx3
        drop(rx1);
        drop(rx3);

        // 推送数据（这会触发自动清理）
        let record = Record::new("cu2501", 5000)
            .with_value("price", RecordValue::Float(83000.0));
        engine.push_stream(record);

        // rx2 仍然应该收到数据
        let received = rx2.recv().await;
        assert!(received.is_some(), "剩余订阅者应收到数据");

        // 验证订阅者计数（清理后只剩1个）
        assert_eq!(engine.subscriber_count("cu2501"), 1, "应只剩1个订阅者");
    }

    /// 测试批量订阅多个 key
    /// 业务场景: 前端一次性订阅多个合约
    ///
    /// API: subscribe_many(keys) -> HashMap<key, Receiver>
    ///   - 返回每个 key 对应的 Receiver
    ///   - 内部循环调用 subscribe()
    #[tokio::test]
    async fn test_subscribe_many() {
        let engine = HybridQueryEngine::default();

        // 批量订阅 3 个合约
        let keys = vec![
            "cu2501".to_string(),
            "au2501".to_string(),
            "ag2501".to_string(),
        ];
        let mut receivers = engine.subscribe_many(keys);

        // 验证返回了3个 Receiver
        assert_eq!(receivers.len(), 3, "应返回3个Receiver");
        assert!(receivers.contains_key("cu2501"));
        assert!(receivers.contains_key("au2501"));
        assert!(receivers.contains_key("ag2501"));

        // 验证各 key 的订阅者计数
        assert_eq!(engine.subscriber_count("cu2501"), 1);
        assert_eq!(engine.subscriber_count("au2501"), 1);
        assert_eq!(engine.subscriber_count("ag2501"), 1);

        // 推送数据并验证接收
        engine.push_stream(Record::new("cu2501", 6000)
            .with_value("price", RecordValue::Float(84000.0)));
        engine.push_stream(Record::new("au2501", 7000)
            .with_value("price", RecordValue::Float(660.0)));
        engine.push_stream(Record::new("ag2501", 8000)
            .with_value("price", RecordValue::Float(8.5)));

        // 验证各 Receiver 收到对应数据
        // 注意: 需要逐个获取避免借用冲突
        {
            let cu_rx = receivers.get_mut("cu2501").unwrap();
            assert_eq!(cu_rx.recv().await.unwrap().get_float("price"), Some(84000.0));
        }
        {
            let au_rx = receivers.get_mut("au2501").unwrap();
            assert_eq!(au_rx.recv().await.unwrap().get_float("price"), Some(660.0));
        }
        {
            let ag_rx = receivers.get_mut("ag2501").unwrap();
            assert_eq!(ag_rx.recv().await.unwrap().get_float("price"), Some(8.5));
        }
    }

    /// 测试 channel 容量限制和背压
    /// 业务场景: 订阅者处理速度跟不上数据推送速度
    ///
    /// 背压机制:
    ///   - channel 容量为 1000
    ///   - try_send 不会阻塞，满了直接丢弃
    ///   - 避免慢消费者拖慢整个系统
    #[tokio::test]
    async fn test_channel_backpressure() {
        let engine = HybridQueryEngine::default();

        // 创建订阅但不消费
        let _rx = engine.subscribe("cu2501".to_string());

        // 推送超过 channel 容量的数据
        // channel 容量为 1000，推送 1100 条
        for i in 0..1100 {
            let record = Record::new("cu2501", i as i64)
                .with_value("seq", RecordValue::Int(i));
            engine.push_stream(record);
        }

        // 不会 panic，try_send 在 channel 满时会静默失败
        // 验证数据仍在 stream_buffer 中
        let buffer = engine.stream_buffer();
        let buffered = buffer.query_latest("cu2501", 2000);
        assert_eq!(buffered.len(), 1100, "stream_buffer 应包含所有数据");
    }

    /// 测试 push_stream 同时更新 buffer 和通知订阅者
    /// 验证数据一致性: buffer 和 channel 收到相同数据
    #[tokio::test]
    async fn test_push_stream_dual_path() {
        let engine = HybridQueryEngine::default();

        let mut rx = engine.subscribe("cu2501".to_string());

        // 推送数据
        let record = Record::new("cu2501", 9000)
            .with_value("price", RecordValue::Float(85000.0))
            .with_value("volume", RecordValue::Int(100));
        engine.push_stream(record);

        // 路径1: 通过 channel 接收
        let from_channel = rx.recv().await.unwrap();

        // 路径2: 通过 buffer 查询
        let buffer = engine.stream_buffer();
        let from_buffer = buffer.query_latest("cu2501", 1);

        // 验证两条路径数据一致
        assert_eq!(from_channel.key, from_buffer[0].key);
        assert_eq!(from_channel.timestamp, from_buffer[0].timestamp);
        assert_eq!(
            from_channel.get_float("price"),
            from_buffer[0].get_float("price")
        );
        assert_eq!(
            from_channel.get_float("volume"),
            from_buffer[0].get_float("volume")
        );
    }

    /// 测试空 key 订阅
    /// 边界情况: 确保空字符串 key 也能正常工作
    #[tokio::test]
    async fn test_subscribe_empty_key() {
        let engine = HybridQueryEngine::default();

        // 订阅空 key
        let mut rx = engine.subscribe("".to_string());
        assert_eq!(engine.subscriber_count(""), 1);

        // 推送空 key 数据
        engine.push_stream(Record::new("", 10000)
            .with_value("test", RecordValue::Bool(true)));

        let received = rx.recv().await;
        assert!(received.is_some(), "空key也应能接收数据");
    }

    /// 测试订阅者计数的准确性
    /// 验证 subscriber_count 只统计活跃的 channel
    #[test]
    fn test_subscriber_count_accuracy() {
        let engine = HybridQueryEngine::default();

        // 初始为 0
        assert_eq!(engine.subscriber_count("cu2501"), 0);

        // 添加订阅
        let rx1 = engine.subscribe("cu2501".to_string());
        assert_eq!(engine.subscriber_count("cu2501"), 1);

        let rx2 = engine.subscribe("cu2501".to_string());
        assert_eq!(engine.subscriber_count("cu2501"), 2);

        // 丢弃一个
        drop(rx1);

        // 注意: subscriber_count 会过滤 is_closed()
        // 但由于 DashMap 的惰性清理，可能仍显示 2
        // 调用 unsubscribe 强制清理
        engine.unsubscribe("cu2501");
        assert_eq!(engine.subscriber_count("cu2501"), 1, "清理后应为1");

        drop(rx2);
        engine.unsubscribe("cu2501");
        assert_eq!(engine.subscriber_count("cu2501"), 0, "全部清理后应为0");
    }
}
