// BatchDataSource 实现 - 连接 OltpHybridStorage 与 HybridQueryEngine
//
// 架构位置：
// HybridQueryEngine
//   └─ BatchDataSource trait
//        └─ OltpBatchAdapter (本文件)
//             └─ OltpHybridStorage (OLTP 存储)
//             └─ ParquetSSTable (OLAP 存储，可选)
//
// 性能特性：
// - OLTP: P99 < 100μs (MemTable + SSTable)
// - OLAP: P99 < 10ms (Parquet 谓词下推)
// - 自动路由：根据时间范围选择最优数据源
//
// @yutiansut @quantaxis

use crate::query::hybrid::{
    AggregateOp, AggregateResult, Aggregation, BatchDataSource, BatchQueryError, Record,
    RecordValue,
};
use crate::storage::hybrid::oltp::OltpHybridStorage;
use crate::storage::sstable::olap_parquet::ParquetSSTable;
use crate::storage::wal::record::WalRecord;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// OLTP 批数据源适配器
///
/// 将 OltpHybridStorage 适配为 BatchDataSource trait
#[derive(Clone)]
pub struct OltpBatchAdapter {
    /// OLTP 存储引用
    storage: Arc<OltpHybridStorage>,
    /// OLAP Parquet 文件（可选，用于历史数据）
    olap_files: Vec<Arc<ParquetSSTable>>,
    /// OLAP 数据时间边界（早于此时间使用 OLAP）
    olap_cutoff_timestamp: i64,
}

impl OltpBatchAdapter {
    /// 创建新的适配器
    pub fn new(storage: Arc<OltpHybridStorage>) -> Self {
        Self {
            storage,
            olap_files: Vec::new(),
            olap_cutoff_timestamp: 0,
        }
    }

    /// 创建适配器并自动加载存储中的 OLAP 文件
    ///
    /// 这是推荐的创建方式，会自动获取已转换的 OLAP 文件
    pub fn new_with_olap(storage: Arc<OltpHybridStorage>) -> Self {
        let olap_files = storage.get_olap_files();
        let olap_cutoff_timestamp = storage.get_olap_cutoff_timestamp();

        Self {
            storage,
            olap_files,
            olap_cutoff_timestamp,
        }
    }

    /// 添加 OLAP Parquet 文件
    pub fn with_olap_file(mut self, parquet_path: PathBuf) -> Result<Self, String> {
        let sstable = Arc::new(ParquetSSTable::open(&parquet_path)?);

        // 更新 OLAP 时间边界
        let max_ts = sstable.metadata().max_timestamp;
        if max_ts > self.olap_cutoff_timestamp {
            self.olap_cutoff_timestamp = max_ts;
        }

        self.olap_files.push(sstable);
        Ok(self)
    }

    /// 添加多个 OLAP 文件
    pub fn with_olap_files(mut self, paths: Vec<PathBuf>) -> Result<Self, String> {
        for path in paths {
            self = self.with_olap_file(path)?;
        }
        Ok(self)
    }

    /// 设置 OLAP 时间边界
    pub fn with_olap_cutoff(mut self, timestamp: i64) -> Self {
        self.olap_cutoff_timestamp = timestamp;
        self
    }

    /// 刷新 OLAP 文件列表
    ///
    /// 在转换完成后调用以获取最新的 OLAP 文件
    pub fn refresh_olap(&mut self) {
        self.olap_files = self.storage.get_olap_files();
        self.olap_cutoff_timestamp = self.storage.get_olap_cutoff_timestamp();
    }

    /// 从 WalRecord 转换为 Record
    fn wal_record_to_record(&self, key: &str, timestamp: i64, record: &WalRecord) -> Record {
        let mut result = Record::new(key, timestamp);

        match record {
            WalRecord::OrderInsert {
                order_id,
                direction,
                offset,
                price,
                volume,
                ..
            } => {
                result = result
                    .with_value("record_type", RecordValue::String("OrderInsert".to_string()))
                    .with_value("order_id", RecordValue::Int(*order_id as i64))
                    .with_value("direction", RecordValue::Int(*direction as i64))
                    .with_value("offset", RecordValue::Int(*offset as i64))
                    .with_value("price", RecordValue::Float(*price))
                    .with_value("volume", RecordValue::Float(*volume));
            }
            WalRecord::TradeExecuted {
                trade_id,
                order_id,
                price,
                volume,
                ..
            } => {
                result = result
                    .with_value("record_type", RecordValue::String("TradeExecuted".to_string()))
                    .with_value("trade_id", RecordValue::Int(*trade_id as i64))
                    .with_value("order_id", RecordValue::Int(*order_id as i64))
                    .with_value("price", RecordValue::Float(*price))
                    .with_value("volume", RecordValue::Float(*volume));
            }
            WalRecord::AccountUpdate {
                balance,
                available,
                frozen,
                margin,
                ..
            } => {
                result = result
                    .with_value("record_type", RecordValue::String("AccountUpdate".to_string()))
                    .with_value("balance", RecordValue::Float(*balance))
                    .with_value("available", RecordValue::Float(*available))
                    .with_value("frozen", RecordValue::Float(*frozen))
                    .with_value("margin", RecordValue::Float(*margin));
            }
            WalRecord::KLineFinished {
                period,
                kline_timestamp,
                open,
                high,
                low,
                close,
                volume,
                amount,
                ..
            } => {
                result = result
                    .with_value("record_type", RecordValue::String("KLineFinished".to_string()))
                    .with_value("period", RecordValue::Int(*period as i64))
                    .with_value("kline_timestamp", RecordValue::Int(*kline_timestamp))
                    .with_value("open", RecordValue::Float(*open))
                    .with_value("high", RecordValue::Float(*high))
                    .with_value("low", RecordValue::Float(*low))
                    .with_value("close", RecordValue::Float(*close))
                    .with_value("volume", RecordValue::Int(*volume))
                    .with_value("amount", RecordValue::Float(*amount));
            }
            _ => {
                result = result.with_value("record_type", RecordValue::String("Other".to_string()));
            }
        }

        result
    }

    /// 查询 OLTP 存储
    fn query_oltp(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Record>, BatchQueryError> {
        let results = self
            .storage
            .range_query(start_ts, end_ts)
            .map_err(|e| BatchQueryError::IoError(e))?;

        Ok(results
            .into_iter()
            .map(|(ts, _seq, record)| self.wal_record_to_record(key, ts, &record))
            .collect())
    }

    /// 查询 OLAP Parquet 文件
    fn query_olap(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Record>, BatchQueryError> {
        use arrow2::array::PrimitiveArray;

        let mut all_records = Vec::new();

        for parquet in &self.olap_files {
            // 使用谓词下推的范围查询
            let chunks = parquet
                .range_query(start_ts, end_ts)
                .map_err(|e| BatchQueryError::IoError(e))?;

            for chunk in chunks {
                if chunk.is_empty() {
                    continue;
                }

                // 提取时间戳列
                let timestamp_array = chunk.arrays()[0]
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i64>>()
                    .ok_or_else(|| BatchQueryError::IoError("Missing timestamp column".to_string()))?;

                // 提取 price 列 (index 8)
                let price_array = chunk.arrays().get(8).and_then(|arr| {
                    arr.as_any().downcast_ref::<PrimitiveArray<f64>>()
                });

                // 提取 volume 列 (index 9)
                let volume_array = chunk.arrays().get(9).and_then(|arr| {
                    arr.as_any().downcast_ref::<PrimitiveArray<f64>>()
                });

                for i in 0..chunk.len() {
                    let timestamp = timestamp_array.value(i);
                    let mut record = Record::new(key, timestamp);

                    if let Some(price_arr) = price_array {
                        if let Some(price) = price_arr.get(i) {
                            record = record.with_value("price", RecordValue::Float(price));
                        }
                    }

                    if let Some(volume_arr) = volume_array {
                        if let Some(volume) = volume_arr.get(i) {
                            record = record.with_value("volume", RecordValue::Float(volume));
                        }
                    }

                    all_records.push(record);
                }
            }
        }

        Ok(all_records)
    }
}

#[async_trait::async_trait]
impl BatchDataSource for OltpBatchAdapter {
    /// 查询历史数据
    ///
    /// 自动路由策略：
    /// - 时间 < olap_cutoff: 查询 OLAP (Parquet)
    /// - 时间 >= olap_cutoff: 查询 OLTP (MemTable + SSTable)
    /// - 跨边界: 合并两者结果
    async fn query(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        _fields: &[String],
    ) -> Result<Vec<Record>, BatchQueryError> {
        let mut results = Vec::new();

        // 查询 OLAP (历史数据)
        if start_ts < self.olap_cutoff_timestamp && !self.olap_files.is_empty() {
            let olap_end = end_ts.min(self.olap_cutoff_timestamp);
            let olap_records = self.query_olap(key, start_ts, olap_end)?;
            results.extend(olap_records);
        }

        // 查询 OLTP (近期数据)
        if end_ts >= self.olap_cutoff_timestamp {
            let oltp_start = start_ts.max(self.olap_cutoff_timestamp);
            let oltp_records = self.query_oltp(key, oltp_start, end_ts)?;
            results.extend(oltp_records);
        }

        // 按时间戳排序
        results.sort_by_key(|r| r.timestamp);

        Ok(results)
    }

    /// 聚合查询
    async fn aggregate(
        &self,
        key: &str,
        start_ts: i64,
        end_ts: i64,
        aggregations: &[Aggregation],
    ) -> Result<AggregateResult, BatchQueryError> {
        // 先获取原始数据
        let records = self.query(key, start_ts, end_ts, &[]).await?;

        let mut result = AggregateResult::default();

        for agg in aggregations {
            let values: Vec<f64> = records
                .iter()
                .filter_map(|r| r.get_float(&agg.field))
                .collect();

            if values.is_empty() {
                continue;
            }

            let agg_value = match agg.op {
                AggregateOp::Count => values.len() as f64,
                AggregateOp::Sum => values.iter().sum(),
                AggregateOp::Avg => values.iter().sum::<f64>() / values.len() as f64,
                AggregateOp::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
                AggregateOp::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                AggregateOp::First => values.first().copied().unwrap_or(0.0),
                AggregateOp::Last => values.last().copied().unwrap_or(0.0),
            };

            result.values.insert(agg.alias.clone(), agg_value);
        }

        Ok(result)
    }
}

/// 为 OltpHybridStorage 提供便捷的 BatchDataSource 创建方法
impl OltpHybridStorage {
    /// 创建 BatchDataSource 适配器（仅 OLTP）
    pub fn as_batch_source(self: &Arc<Self>) -> OltpBatchAdapter {
        OltpBatchAdapter::new(Arc::clone(self))
    }

    /// 创建 BatchDataSource 适配器（OLTP + OLAP 混合）
    ///
    /// 推荐使用此方法以获得最佳查询性能：
    /// - 近期数据：OLTP SSTable (P99 < 100μs)
    /// - 历史数据：OLAP Parquet (谓词下推优化)
    pub fn as_hybrid_batch_source(self: &Arc<Self>) -> OltpBatchAdapter {
        OltpBatchAdapter::new_with_olap(Arc::clone(self))
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
    async fn test_batch_adapter_oltp_query() {
        let tmp_dir = tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 1024 * 1024,
            estimated_entry_size: 256,
            enable_olap_conversion: false, // 测试中禁用 OLAP 转换
            ..Default::default()
        };

        let storage = Arc::new(OltpHybridStorage::create("IF2501", config).unwrap());

        // 写入测试数据
        for i in 0..100 {
            let record = create_order_record(i, 1000 + i as i64 * 10);
            storage.write(record).unwrap();
        }

        // 创建 BatchDataSource
        let adapter = storage.as_batch_source();

        // 查询
        let results = adapter
            .query("IF2501", 1000, 1500, &[])
            .await
            .unwrap();

        assert_eq!(results.len(), 51); // timestamps 1000, 1010, ..., 1500
    }

    #[tokio::test]
    async fn test_batch_adapter_aggregation() {
        let tmp_dir = tempdir().unwrap();
        let config = OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 1024 * 1024,
            estimated_entry_size: 256,
            enable_olap_conversion: false, // 测试中禁用 OLAP 转换
            ..Default::default()
        };

        let storage = Arc::new(OltpHybridStorage::create("IF2501", config).unwrap());

        // 写入测试数据
        for i in 0..10 {
            let record = create_order_record(i, 1000 + i as i64);
            storage.write(record).unwrap();
        }

        let adapter = storage.as_batch_source();

        // 聚合查询
        let agg_result = adapter
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

        assert!(agg_result.values.contains_key("avg_price"));
        assert!(agg_result.values.contains_key("total_volume"));
        assert_eq!(agg_result.values.get("total_volume"), Some(&100.0)); // 10 * 10.0
    }
}
