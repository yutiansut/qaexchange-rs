// 统一查询引擎 - 流批一体化查询接口
//
// 架构设计：
//
// ┌─────────────────────────────────────────────────────────────────────┐
// │                     UnifiedQueryEngine                               │
// │                                                                      │
// │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐     │
// │  │  StreamBuffer   │  │ OltpBatchAdapter │  │  QueryEngine    │     │
// │  │  (实时数据)      │  │ (OLTP + OLAP)   │  │  (SQL/Polars)   │     │
// │  │  P99 < 1μs      │  │ P99 < 100μs     │  │  P99 < 10ms     │     │
// │  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘     │
// │           │                    │                    │               │
// │           └────────────────────┼────────────────────┘               │
// │                                │                                    │
// │                       ┌────────▼────────┐                          │
// │                       │   智能路由器     │                          │
// │                       │ (时间范围自动选择)│                          │
// │                       └─────────────────┘                          │
// └─────────────────────────────────────────────────────────────────────┘
//
// 查询路由策略：
// 1. 实时数据 (< stream_window): StreamBuffer
// 2. 近期数据 (stream_window ~ olap_cutoff): OLTP SSTable
// 3. 历史数据 (> olap_cutoff): OLAP Parquet
// 4. 跨边界查询: 自动合并多个数据源
//
// @yutiansut @quantaxis

use crate::query::engine::QueryEngine;
use crate::query::hybrid::{
    AggregateOp, AggregateResult, Aggregation, BatchDataSource, BatchQueryError, HybridConfig,
    HybridQueryEngine, QueryResult, Record, RecordValue, StreamBuffer,
};
use crate::storage::hybrid::batch_source::OltpBatchAdapter;
use crate::storage::hybrid::oltp::OltpHybridStorage;
use std::sync::Arc;
use std::time::Duration;

/// 统一查询引擎配置
#[derive(Debug, Clone)]
pub struct UnifiedQueryConfig {
    /// 流数据最大延迟
    pub stream_max_latency: Duration,

    /// 流数据优先时间窗口（秒）
    /// 此时间内的数据优先从 StreamBuffer 获取
    pub stream_window_seconds: i64,

    /// 批查询超时
    pub batch_timeout: Duration,

    /// 是否启用 SQL 查询
    pub enable_sql: bool,

    /// 查询结果缓存时间（毫秒）
    pub cache_ttl_ms: u64,
}

impl Default for UnifiedQueryConfig {
    fn default() -> Self {
        Self {
            stream_max_latency: Duration::from_millis(100),
            stream_window_seconds: 60, // 1 分钟内优先使用流数据
            batch_timeout: Duration::from_secs(30),
            enable_sql: true,
            cache_ttl_ms: 100, // 100ms 缓存
        }
    }
}

/// 统一查询引擎
///
/// 提供流批一体化的查询接口：
/// - 实时流查询（StreamBuffer）
/// - OLTP 查询（MemTable + SSTable）
/// - OLAP 查询（Parquet，带谓词下推）
/// - SQL 查询（Polars）
pub struct UnifiedQueryEngine {
    /// 混合查询引擎（流 + 批）
    hybrid_engine: HybridQueryEngine,

    /// OLTP + OLAP 批数据源
    batch_adapter: Option<OltpBatchAdapter>,

    /// SQL 查询引擎
    sql_engine: Option<QueryEngine>,

    /// 配置
    config: UnifiedQueryConfig,
}

impl UnifiedQueryEngine {
    /// 创建统一查询引擎（仅流数据）
    pub fn new(config: UnifiedQueryConfig) -> Self {
        let hybrid_config = HybridConfig {
            stream_max_latency: config.stream_max_latency,
            batch_timeout: config.batch_timeout,
            stream_priority_window: Duration::from_secs(config.stream_window_seconds as u64),
            ..Default::default()
        };

        Self {
            hybrid_engine: HybridQueryEngine::new(hybrid_config),
            batch_adapter: None,
            sql_engine: if config.enable_sql {
                Some(QueryEngine::new())
            } else {
                None
            },
            config,
        }
    }

    /// 绑定 OLTP + OLAP 存储
    ///
    /// 推荐使用此方法获得完整的流批一体化能力
    pub fn with_storage(mut self, storage: Arc<OltpHybridStorage>) -> Self {
        // 创建带 OLAP 支持的批数据适配器
        let adapter = OltpBatchAdapter::new_with_olap(storage);

        // 将适配器设置为混合引擎的批数据源
        let boxed: Arc<dyn BatchDataSource> = Arc::new(BatchAdapterWrapper(adapter.clone()));
        self.hybrid_engine = self.hybrid_engine.with_batch_source(boxed);
        self.batch_adapter = Some(adapter);

        self
    }

    /// 获取流数据缓存引用
    pub fn stream_buffer(&self) -> Arc<StreamBuffer> {
        self.hybrid_engine.stream_buffer()
    }

    /// 推送实时数据到流缓存
    pub fn push_stream(&self, record: Record) {
        self.hybrid_engine.push_stream(record);
    }

    /// 执行时间范围查询
    ///
    /// 自动路由策略：
    /// 1. 完全在流窗口内：仅查询 StreamBuffer
    /// 2. 完全在 OLAP 边界外：仅查询 OLAP
    /// 3. 跨边界：合并多个数据源
    ///
    /// # Arguments
    /// * `key` - 查询键（如品种 ID）
    /// * `start_ts` - 起始时间戳（纳秒）
    /// * `end_ts` - 结束时间戳（纳秒）
    /// * `fields` - 需要返回的字段（空为全部）
    pub async fn query(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        fields: &[String],
    ) -> Result<QueryResult, UnifiedQueryError> {
        self.hybrid_engine
            .query(key, start_ts, end_ts, fields)
            .await
            .map_err(|e| UnifiedQueryError::HybridError(format!("{}", e)))
    }

    /// 执行聚合查询
    ///
    /// 支持的聚合操作：Count, Sum, Avg, Min, Max, First, Last
    pub async fn aggregate(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        aggregations: &[Aggregation],
    ) -> Result<AggregateResult, UnifiedQueryError> {
        let adapter = self
            .batch_adapter
            .as_ref()
            .ok_or_else(|| UnifiedQueryError::NotConfigured("Storage not configured".to_string()))?;

        adapter
            .aggregate(key, start_ts, end_ts, aggregations)
            .await
            .map_err(|e| UnifiedQueryError::BatchError(format!("{}", e)))
    }

    /// 执行 SQL 查询
    ///
    /// 使用 Polars DataFrame 执行复杂的 SQL 分析
    ///
    /// # Example SQL
    /// ```sql
    /// SELECT instrument_id, AVG(price) as avg_price, SUM(volume) as total_volume
    /// FROM trades
    /// WHERE timestamp >= 1000 AND timestamp <= 2000
    /// GROUP BY instrument_id
    /// ORDER BY total_volume DESC
    /// LIMIT 10
    /// ```
    pub fn sql_query(&self, sql: &str) -> Result<polars::frame::DataFrame, UnifiedQueryError> {
        let engine = self
            .sql_engine
            .as_ref()
            .ok_or_else(|| UnifiedQueryError::NotConfigured("SQL engine not enabled".to_string()))?;

        engine
            .sql(sql)
            .map_err(|e| UnifiedQueryError::SqlError(e.to_string()))
    }

    /// 刷新 OLAP 文件列表
    ///
    /// 在 OLTP → OLAP 转换完成后调用
    pub fn refresh_olap(&mut self) {
        if let Some(ref mut adapter) = self.batch_adapter {
            adapter.refresh_olap();
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> UnifiedQueryStats {
        let stream_buffer = self.hybrid_engine.stream_buffer();

        UnifiedQueryStats {
            stream_buffer_size: stream_buffer.len(),
            stream_buffer_keys: stream_buffer.keys().len(),
            has_oltp_olap: self.batch_adapter.is_some(),
            sql_enabled: self.sql_engine.is_some(),
        }
    }
}

/// 统一查询错误
#[derive(Debug)]
pub enum UnifiedQueryError {
    NotConfigured(String),
    HybridError(String),
    BatchError(String),
    SqlError(String),
    IoError(String),
}

impl std::fmt::Display for UnifiedQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnifiedQueryError::NotConfigured(msg) => write!(f, "Not configured: {}", msg),
            UnifiedQueryError::HybridError(msg) => write!(f, "Hybrid query error: {}", msg),
            UnifiedQueryError::BatchError(msg) => write!(f, "Batch query error: {}", msg),
            UnifiedQueryError::SqlError(msg) => write!(f, "SQL error: {}", msg),
            UnifiedQueryError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for UnifiedQueryError {}

/// 统一查询统计
#[derive(Debug, Clone)]
pub struct UnifiedQueryStats {
    pub stream_buffer_size: usize,
    pub stream_buffer_keys: usize,
    pub has_oltp_olap: bool,
    pub sql_enabled: bool,
}

/// BatchDataSource 包装器
///
/// 将 OltpBatchAdapter 包装为 Arc<dyn BatchDataSource>
struct BatchAdapterWrapper(OltpBatchAdapter);

#[async_trait::async_trait]
impl BatchDataSource for BatchAdapterWrapper {
    async fn query(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        fields: &[String],
    ) -> Result<Vec<Record>, BatchQueryError> {
        self.0.query(key, start_ts, end_ts, fields).await
    }

    async fn aggregate(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        aggregations: &[Aggregation],
    ) -> Result<AggregateResult, BatchQueryError> {
        self.0.aggregate(key, start_ts, end_ts, aggregations).await
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 便捷构造器
// ═══════════════════════════════════════════════════════════════════════════

/// 快速创建统一查询引擎
///
/// # Example
/// ```ignore
/// let engine = unified_query_engine(storage)
///     .with_sql(true)
///     .build();
/// ```
pub fn unified_query_engine(storage: Arc<OltpHybridStorage>) -> UnifiedQueryEngineBuilder {
    UnifiedQueryEngineBuilder::new(storage)
}

/// 统一查询引擎构造器
pub struct UnifiedQueryEngineBuilder {
    storage: Arc<OltpHybridStorage>,
    config: UnifiedQueryConfig,
}

impl UnifiedQueryEngineBuilder {
    pub fn new(storage: Arc<OltpHybridStorage>) -> Self {
        Self {
            storage,
            config: UnifiedQueryConfig::default(),
        }
    }

    /// 设置流窗口大小（秒）
    pub fn stream_window(mut self, seconds: i64) -> Self {
        self.config.stream_window_seconds = seconds;
        self
    }

    /// 启用/禁用 SQL 查询
    pub fn with_sql(mut self, enabled: bool) -> Self {
        self.config.enable_sql = enabled;
        self
    }

    /// 设置批查询超时
    pub fn batch_timeout(mut self, timeout: Duration) -> Self {
        self.config.batch_timeout = timeout;
        self
    }

    /// 设置缓存 TTL（毫秒）
    pub fn cache_ttl(mut self, ttl_ms: u64) -> Self {
        self.config.cache_ttl_ms = ttl_ms;
        self
    }

    /// 构建查询引擎
    pub fn build(self) -> UnifiedQueryEngine {
        UnifiedQueryEngine::new(self.config).with_storage(self.storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::hybrid::oltp::OltpHybridConfig;
    use crate::storage::wal::record::WalRecord;
    use tempfile::tempdir;

    fn create_order_record(order_id: u64, timestamp: i64) -> WalRecord {
        WalRecord::OrderInsert {
            order_id,
            user_id: [1u8; 32],
            instrument_id: [2u8; 16],
            direction: 0,
            offset: 0,
            price: 4000.0 + order_id as f64,
            volume: 10.0,
            timestamp,
        }
    }

    #[tokio::test]
    async fn test_unified_query_engine() {
        let tmp_dir = tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            enable_olap_conversion: false,
            ..Default::default()
        };

        let storage = Arc::new(OltpHybridStorage::create("IF2501", config).unwrap());

        // 写入测试数据
        for i in 0..100 {
            let record = create_order_record(i, 1000 + i as i64 * 10);
            storage.write(record).unwrap();
        }

        // 创建统一查询引擎
        let engine = unified_query_engine(storage).with_sql(false).build();

        // 验证统计
        let stats = engine.stats();
        assert!(stats.has_oltp_olap);
        assert!(!stats.sql_enabled);

        // 执行查询
        let result = engine.query("IF2501", 1000, 1500, &[]).await.unwrap();
        assert!(!result.records.is_empty());
    }

    #[tokio::test]
    async fn test_unified_aggregation() {
        let tmp_dir = tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            enable_olap_conversion: false,
            ..Default::default()
        };

        let storage = Arc::new(OltpHybridStorage::create("IF2501", config).unwrap());

        // 写入测试数据
        for i in 0..10 {
            let record = create_order_record(i, 1000 + i as i64);
            storage.write(record).unwrap();
        }

        let engine = unified_query_engine(storage).build();

        // 聚合查询
        let result = engine
            .aggregate(
                "IF2501",
                1000,
                1010,
                &[
                    Aggregation {
                        field: "price".to_string(),
                        op: AggregateOp::Avg,
                        alias: "avg_price".to_string(),
                    },
                    Aggregation {
                        field: "volume".to_string(),
                        op: AggregateOp::Sum,
                        alias: "total_volume".to_string(),
                    },
                ],
            )
            .await
            .unwrap();

        assert!(result.values.contains_key("avg_price"));
        assert!(result.values.contains_key("total_volume"));
    }

    #[test]
    fn test_stream_buffer_push() {
        let engine = UnifiedQueryEngine::new(UnifiedQueryConfig::default());

        // 推送流数据
        let record = Record::new("test_key", chrono::Utc::now().timestamp())
            .with_value("price", RecordValue::Float(100.5));

        engine.push_stream(record);

        // 验证缓存
        let stats = engine.stats();
        assert_eq!(stats.stream_buffer_size, 1);
        assert_eq!(stats.stream_buffer_keys, 1);
    }
}
