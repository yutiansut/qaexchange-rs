// 查询引擎 - 基于 Polars DataFrame

use super::scanner::SSTableScanner;
use super::types::*;
use polars::io::SerWriter;
use polars::prelude::*;
use polars::sql::SQLContext;
use std::path::Path;

/// 查询引擎
///
/// 功能:
/// - SQL 查询
/// - 结构化查询
/// - 时间序列查询
/// - 聚合分析
pub struct QueryEngine {
    /// SSTable 扫描器
    scanner: SSTableScanner,
}

impl QueryEngine {
    /// 创建新的查询引擎
    pub fn new() -> Self {
        Self {
            scanner: SSTableScanner::new(),
        }
    }

    /// 添加数据目录
    pub fn add_data_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<(), String> {
        self.scanner.scan_directory(dir)
    }

    /// 添加 Parquet 文件
    pub fn add_parquet_file<P: AsRef<Path>>(&mut self, path: P) {
        self.scanner.add_olap_sstable(path);
    }

    /// 执行查询
    pub fn execute(&self, request: QueryRequest) -> Result<QueryResponse, String> {
        let start = std::time::Instant::now();

        let df = match &request.query_type {
            QueryType::Sql { query } => self.execute_sql(query)?,
            QueryType::Structured { select, from } => {
                self.execute_structured(select, from, &request)?
            }
            QueryType::TimeSeries {
                metrics,
                dimensions,
                granularity,
            } => self.execute_timeseries(metrics, dimensions, *granularity, &request)?,
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;

        self.dataframe_to_response(df, elapsed_ms)
    }

    /// 执行 SQL 查询并返回 DataFrame
    ///
    /// 公开接口，用于直接获取查询结果
    ///
    /// # Example
    /// ```ignore
    /// let df = engine.sql("SELECT * FROM data WHERE timestamp > 1000")?;
    /// ```
    pub fn sql(&self, query: &str) -> Result<DataFrame, String> {
        self.execute_sql(query)
    }

    /// 执行 SQL 查询 (内部实现)
    fn execute_sql(&self, query: &str) -> Result<DataFrame, String> {
        // 获取 Parquet 文件路径
        let parquet_paths = self.scanner.get_parquet_paths();

        if parquet_paths.is_empty() {
            return Err("No data files found".to_string());
        }

        // 使用 Polars LazyFrame + SQL
        // 读取第一个 Parquet 文件作为表
        let df = LazyFrame::scan_parquet(
            PlPath::new(parquet_paths[0].to_str().unwrap()),
            ScanArgsParquet::default(),
        )
        .map_err(|e| format!("Scan parquet failed: {}", e))?;

        // 如果有多个文件，合并它们
        let mut df = df;
        for path in &parquet_paths[1..] {
            let other = LazyFrame::scan_parquet(
                PlPath::new(path.to_str().unwrap()),
                ScanArgsParquet::default(),
            )
            .map_err(|e| format!("Scan parquet failed: {}", e))?;

            df = concat(vec![df, other], UnionArgs::default())
                .map_err(|e| format!("Concat failed: {}", e))?;
        }

        // 注册表
        let mut ctx = SQLContext::new();
        ctx.register("data", df);

        // 执行 SQL
        ctx.execute(query)
            .map_err(|e| format!("SQL execution failed: {}", e))?
            .collect()
            .map_err(|e| format!("Collect failed: {}", e))
    }

    /// 执行结构化查询
    fn execute_structured(
        &self,
        select: &[String],
        _from: &str,
        request: &QueryRequest,
    ) -> Result<DataFrame, String> {
        let parquet_paths = self.scanner.get_parquet_paths();

        if parquet_paths.is_empty() {
            return Err("No data files found".to_string());
        }

        // 读取 Parquet 文件
        let mut df = LazyFrame::scan_parquet(
            PlPath::new(parquet_paths[0].to_str().unwrap()),
            ScanArgsParquet::default(),
        )
        .map_err(|e| format!("Scan parquet failed: {}", e))?;

        // 合并多个文件
        for path in &parquet_paths[1..] {
            let other = LazyFrame::scan_parquet(
                PlPath::new(path.to_str().unwrap()),
                ScanArgsParquet::default(),
            )
            .map_err(|e| format!("Scan parquet failed: {}", e))?;

            df = concat(vec![df, other], UnionArgs::default())
                .map_err(|e| format!("Concat failed: {}", e))?;
        }

        // 应用时间范围过滤
        if let Some(time_range) = &request.time_range {
            df = df.filter(
                col("timestamp")
                    .gt_eq(lit(time_range.start))
                    .and(col("timestamp").lt_eq(lit(time_range.end))),
            );
        }

        // 应用其他过滤条件
        if let Some(filters) = &request.filters {
            for filter in filters {
                df = self.apply_filter(df, filter)?;
            }
        }

        // 选择列
        if !select.is_empty() && select[0] != "*" {
            let cols: Vec<Expr> = select.iter().map(|s| col(s)).collect();
            df = df.select(&cols);
        }

        // 应用聚合
        if let Some(aggs) = &request.aggregations {
            df = self.apply_aggregations(df, aggs)?;
        }

        // 应用排序
        if let Some(order_by) = &request.order_by {
            for order in order_by {
                df = df.sort(
                    vec![&order.column],
                    SortMultipleOptions::default()
                        .with_order_descending(order.descending)
                        .with_nulls_last(true)
                        .with_multithreaded(true)
                        .with_maintain_order(false),
                );
            }
        }

        // 应用限制
        if let Some(limit) = request.limit {
            df = df.limit(limit as u32);
        }

        df.collect().map_err(|e| format!("Collect failed: {}", e))
    }

    /// 执行时间序列查询
    fn execute_timeseries(
        &self,
        metrics: &[String],
        dimensions: &[String],
        granularity: Option<i64>,
        request: &QueryRequest,
    ) -> Result<DataFrame, String> {
        let parquet_paths = self.scanner.get_parquet_paths();

        if parquet_paths.is_empty() {
            return Err("No data files found".to_string());
        }

        let mut df = LazyFrame::scan_parquet(
            PlPath::new(parquet_paths[0].to_str().unwrap()),
            ScanArgsParquet::default(),
        )
        .map_err(|e| format!("Scan parquet failed: {}", e))?;

        // 合并文件
        for path in &parquet_paths[1..] {
            let other = LazyFrame::scan_parquet(
                PlPath::new(path.to_str().unwrap()),
                ScanArgsParquet::default(),
            )
            .map_err(|e| format!("Scan parquet failed: {}", e))?;

            df = concat(vec![df, other], UnionArgs::default())
                .map_err(|e| format!("Concat failed: {}", e))?;
        }

        // 时间范围过滤
        if let Some(time_range) = &request.time_range {
            df = df.filter(
                col("timestamp")
                    .gt_eq(lit(time_range.start))
                    .and(col("timestamp").lt_eq(lit(time_range.end))),
            );
        }

        // 时间粒度聚合
        if let Some(granularity_secs) = granularity {
            let granularity_ns = granularity_secs * 1_000_000_000;
            df = df.with_column(
                (col("timestamp") / lit(granularity_ns) * lit(granularity_ns)).alias("time_bucket"),
            );
        }

        // 构建分组依据
        let mut group_by = vec![];
        if granularity.is_some() {
            group_by.push("time_bucket");
        }
        group_by.extend(dimensions.iter().map(|s| s.as_str()));

        // 构建聚合表达式
        let mut agg_exprs = vec![];
        for metric in metrics {
            agg_exprs.push(col(metric).sum().alias(&format!("{}_sum", metric)));
            agg_exprs.push(col(metric).mean().alias(&format!("{}_avg", metric)));
            agg_exprs.push(col(metric).min().alias(&format!("{}_min", metric)));
            agg_exprs.push(col(metric).max().alias(&format!("{}_max", metric)));
            agg_exprs.push(col(metric).count().alias(&format!("{}_count", metric)));
        }

        // 执行分组聚合
        df = df.group_by(&group_by).agg(&agg_exprs);

        df.collect().map_err(|e| format!("Collect failed: {}", e))
    }

    /// 应用过滤条件
    fn apply_filter(&self, df: LazyFrame, filter: &Filter) -> Result<LazyFrame, String> {
        let col_expr = col(&filter.column);

        let filter_expr = match (&filter.op, &filter.value) {
            (FilterOp::Eq, FilterValue::Int(v)) => col_expr.eq(lit(*v)),
            (FilterOp::Eq, FilterValue::Float(v)) => col_expr.eq(lit(*v)),
            (FilterOp::Eq, FilterValue::String(v)) => col_expr.eq(lit(v.as_str())),

            (FilterOp::Ne, FilterValue::Int(v)) => col_expr.neq(lit(*v)),
            (FilterOp::Ne, FilterValue::Float(v)) => col_expr.neq(lit(*v)),

            (FilterOp::Gt, FilterValue::Int(v)) => col_expr.gt(lit(*v)),
            (FilterOp::Gt, FilterValue::Float(v)) => col_expr.gt(lit(*v)),

            (FilterOp::Gte, FilterValue::Int(v)) => col_expr.gt_eq(lit(*v)),
            (FilterOp::Gte, FilterValue::Float(v)) => col_expr.gt_eq(lit(*v)),

            (FilterOp::Lt, FilterValue::Int(v)) => col_expr.lt(lit(*v)),
            (FilterOp::Lt, FilterValue::Float(v)) => col_expr.lt(lit(*v)),

            (FilterOp::Lte, FilterValue::Int(v)) => col_expr.lt_eq(lit(*v)),
            (FilterOp::Lte, FilterValue::Float(v)) => col_expr.lt_eq(lit(*v)),

            (FilterOp::In, FilterValue::IntList(v)) => {
                col_expr.is_in(lit(Series::new("".into(), v)), false)
            }
            (FilterOp::NotIn, FilterValue::IntList(v)) => {
                col_expr.is_in(lit(Series::new("".into(), v)), false).not()
            }

            _ => {
                return Err(format!(
                    "Unsupported filter combination: {:?} {:?}",
                    filter.op, filter.value
                ))
            }
        };

        Ok(df.filter(filter_expr))
    }

    /// 应用聚合
    fn apply_aggregations(&self, df: LazyFrame, aggs: &[Aggregation]) -> Result<LazyFrame, String> {
        let agg_exprs: Vec<Expr> = aggs
            .iter()
            .map(|agg| {
                let col_expr = col(&agg.column);
                let expr = match agg.agg_type {
                    AggType::Count => col_expr.count(),
                    AggType::Sum => col_expr.sum(),
                    AggType::Avg => col_expr.mean(),
                    AggType::Min => col_expr.min(),
                    AggType::Max => col_expr.max(),
                    AggType::First => col_expr.first(),
                    AggType::Last => col_expr.last(),
                };

                if let Some(alias) = &agg.alias {
                    expr.alias(alias)
                } else {
                    expr.alias(&format!("{}_{:?}", agg.column, agg.agg_type))
                }
            })
            .collect();

        Ok(df.select(&agg_exprs))
    }

    /// 将 DataFrame 转换为响应
    fn dataframe_to_response(
        &self,
        df: DataFrame,
        elapsed_ms: u64,
    ) -> Result<QueryResponse, String> {
        let columns: Vec<String> = df
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let dtypes: Vec<String> = df.dtypes().iter().map(|dt| format!("{:?}", dt)).collect();

        let row_count = df.height();

        // 转换为 JSON (简化版本 - 返回摘要信息)
        let data = serde_json::json!({
            "row_count": row_count,
            "columns": columns,
            "sample_note": "Full DataFrame serialization can be implemented via custom methods"
        });

        Ok(QueryResponse {
            columns,
            dtypes,
            data,
            row_count,
            elapsed_ms,
        })
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memtable::olap::{create_olap_schema, OlapMemTable};
    use crate::storage::memtable::types::MemTableKey;
    use crate::storage::sstable::olap_parquet::ParquetSSTableWriter;
    use crate::storage::wal::WalRecord;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn create_test_data(tmp_dir: &tempfile::TempDir) -> PathBuf {
        let file_path = tmp_dir.path().join("test.parquet");

        let records: Vec<(MemTableKey, WalRecord)> = (0..100)
            .map(|i| {
                let key = MemTableKey {
                    timestamp: 1000 + i,
                    sequence: i as u64,
                };

                let record = WalRecord::OrderInsert {
                    order_id: i as u64,
                    user_id: [1u8; 32],
                    instrument_id: [2u8; 16],
                    direction: (i % 2) as u8,
                    offset: 0,
                    price: 100.0 + i as f64,
                    volume: 10.0,
                    timestamp: key.timestamp,
                };

                (key, record)
            })
            .collect();

        let memtable = OlapMemTable::from_records(records);

        let mut writer =
            ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

        writer.write_chunk(memtable.chunk()).unwrap();
        writer.finish().unwrap();

        file_path
    }

    #[test]
    fn test_query_engine_structured() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = create_test_data(&tmp_dir);

        let mut engine = QueryEngine::new();
        engine.add_parquet_file(&file_path);

        let request = QueryRequest {
            query_type: QueryType::Structured {
                select: vec!["timestamp".to_string(), "price".to_string()],
                from: "data".to_string(),
            },
            time_range: Some(TimeRange {
                start: 1010,
                end: 1020,
            }),
            filters: None,
            aggregations: None,
            order_by: None,
            limit: Some(5),
        };

        let response = engine.execute(request).unwrap();
        assert_eq!(response.row_count, 5);
        assert!(response.columns.contains(&"timestamp".to_string()));
        assert!(response.columns.contains(&"price".to_string()));
    }

    #[test]
    fn test_query_engine_aggregation() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = create_test_data(&tmp_dir);

        let mut engine = QueryEngine::new();
        engine.add_parquet_file(&file_path);

        let request = QueryRequest {
            query_type: QueryType::Structured {
                select: vec![],
                from: "data".to_string(),
            },
            time_range: None,
            filters: None,
            aggregations: Some(vec![
                Aggregation {
                    agg_type: AggType::Count,
                    column: "price".to_string(),
                    alias: Some("total_count".to_string()),
                },
                Aggregation {
                    agg_type: AggType::Avg,
                    column: "price".to_string(),
                    alias: Some("avg_price".to_string()),
                },
            ]),
            order_by: None,
            limit: None,
        };

        let response = engine.execute(request).unwrap();
        assert_eq!(response.row_count, 1);
        assert!(response.columns.contains(&"total_count".to_string()));
        assert!(response.columns.contains(&"avg_price".to_string()));
    }
}
