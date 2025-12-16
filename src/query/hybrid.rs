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
}
