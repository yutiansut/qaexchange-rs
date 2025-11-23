# 查询引擎 (Polars DataFrame)

## 📖 概述

QAExchange-RS 的查询引擎基于 **Polars 0.51** DataFrame 构建，提供高性能的数据分析和查询能力。支持 SQL 查询、结构化查询API 和时间序列聚合。

## 🎯 设计目标

- **高性能**: P99 < 10ms (100行查询)
- **灵活性**: 支持 SQL 和编程 API
- **零拷贝**: LazyFrame 延迟执行
- **大数据**: 扫描吞吐 > 1 GB/s
- **时间序列**: 原生支持时间聚合

## 🏗️ 架构设计

### 核心组件

```rust
// src/storage/query/engine.rs

use polars::prelude::*;
use polars::sql::SQLContext;

/// 查询引擎
pub struct QueryEngine {
    /// 数据源管理器
    data_sources: Arc<RwLock<HashMap<String, DataFrame>>>,

    /// SQL 上下文
    sql_context: Arc<Mutex<SQLContext>>,

    /// 查询配置
    config: QueryConfig,
}

impl QueryEngine {
    /// 创建查询引擎
    pub fn new(config: QueryConfig) -> Self {
        Self {
            data_sources: Arc::new(RwLock::new(HashMap::new())),
            sql_context: Arc::new(Mutex::new(SQLContext::new())),
            config,
        }
    }

    /// 注册数据源
    pub fn register_dataframe(&self, name: &str, df: DataFrame) -> Result<()> {
        // 存储到数据源
        self.data_sources.write().insert(name.to_string(), df.clone());

        // 注册到 SQL 上下文
        let mut ctx = self.sql_context.lock();
        ctx.register(name, df.lazy());

        Ok(())
    }

    /// 从 SSTable 加载数据源
    pub fn load_from_sstable(&self, name: &str, sstable_path: &Path) -> Result<()> {
        // 读取 Parquet 文件 (OLAP SSTable)
        let df = LazyFrame::scan_parquet(sstable_path, Default::default())?
            .collect()?;

        self.register_dataframe(name, df)
    }
}

#[derive(Debug, Clone)]
pub struct QueryConfig {
    /// 最大查询超时 (秒)
    pub max_query_timeout_secs: u64,

    /// 是否启用并行查询
    pub enable_parallelism: bool,

    /// 工作线程数 (默认 = CPU 核数)
    pub num_threads: Option<usize>,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_query_timeout_secs: 300, // 5 分钟
            enable_parallelism: true,
            num_threads: None, // 自动检测
        }
    }
}
```

## 💡 查询方式

### 1. SQL 查询

#### 基础查询

```rust
impl QueryEngine {
    /// 执行 SQL 查询
    pub fn execute_sql(&self, sql: &str) -> Result<DataFrame> {
        let mut ctx = self.sql_context.lock();

        // 执行查询
        let lf = ctx.execute(sql)?;

        // 收集结果
        let df = lf.collect()?;

        Ok(df)
    }
}

// 使用示例
let engine = QueryEngine::new(Default::default());

// 加载数据
engine.load_from_sstable("trades", Path::new("/data/trades.parquet"))?;
engine.load_from_sstable("orders", Path::new("/data/orders.parquet"))?;

// SQL 查询
let result = engine.execute_sql("
    SELECT
        instrument_id,
        COUNT(*) as trade_count,
        SUM(volume) as total_volume,
        AVG(price) as avg_price
    FROM trades
    WHERE timestamp > '2025-10-01'
    GROUP BY instrument_id
    ORDER BY total_volume DESC
    LIMIT 10
")?;

println!("{}", result);
```

#### JOIN 查询

```rust
let result = engine.execute_sql("
    SELECT
        o.order_id,
        o.user_id,
        t.trade_id,
        t.price,
        t.volume
    FROM orders o
    INNER JOIN trades t ON o.order_id = t.order_id
    WHERE o.user_id = 'user123'
    ORDER BY t.timestamp DESC
")?;
```

#### 窗口函数

```rust
let result = engine.execute_sql("
    SELECT
        instrument_id,
        timestamp,
        price,
        AVG(price) OVER (
            PARTITION BY instrument_id
            ORDER BY timestamp
            ROWS BETWEEN 5 PRECEDING AND CURRENT ROW
        ) as moving_avg_6
    FROM trades
    ORDER BY instrument_id, timestamp
")?;
```

### 2. 结构化查询 API

#### 基本操作

```rust
impl QueryEngine {
    /// 执行结构化查询
    pub fn query(&self, request: StructuredQuery) -> Result<DataFrame> {
        // 获取数据源
        let df = self.data_sources.read()
            .get(&request.table)
            .ok_or_else(|| anyhow!("Table not found: {}", request.table))?
            .clone();

        let mut lf = df.lazy();

        // 应用过滤
        if let Some(filter) = request.filter {
            lf = lf.filter(filter);
        }

        // 选择列
        if !request.columns.is_empty() {
            let cols: Vec<_> = request.columns.iter()
                .map(|c| col(c))
                .collect();
            lf = lf.select(&cols);
        }

        // 排序
        if let Some(sort) = request.sort {
            lf = lf.sort(&sort.column, sort.options);
        }

        // 限制结果数
        if let Some(limit) = request.limit {
            lf = lf.limit(limit);
        }

        // 执行并收集
        lf.collect()
    }
}

#[derive(Debug, Clone)]
pub struct StructuredQuery {
    /// 表名
    pub table: String,

    /// 选择的列
    pub columns: Vec<String>,

    /// 过滤条件
    pub filter: Option<Expr>,

    /// 排序
    pub sort: Option<SortSpec>,

    /// 限制结果数
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SortSpec {
    pub column: String,
    pub options: SortOptions,
}

// 使用示例
let query = StructuredQuery {
    table: "trades".to_string(),
    columns: vec!["instrument_id".to_string(), "price".to_string(), "volume".to_string()],
    filter: Some(col("timestamp").gt(lit("2025-10-01"))),
    sort: Some(SortSpec {
        column: "timestamp".to_string(),
        options: SortOptions {
            descending: true,
            ..Default::default()
        },
    }),
    limit: Some(1000),
};

let result = engine.query(query)?;
```

#### 聚合查询

```rust
impl QueryEngine {
    /// 执行聚合查询
    pub fn aggregate(&self, request: AggregateQuery) -> Result<DataFrame> {
        let df = self.data_sources.read()
            .get(&request.table)
            .ok_or_else(|| anyhow!("Table not found"))?
            .clone();

        let mut lf = df.lazy();

        // 应用过滤
        if let Some(filter) = request.filter {
            lf = lf.filter(filter);
        }

        // 分组聚合
        let agg_exprs: Vec<_> = request.aggregations.iter()
            .map(|agg| match agg {
                AggregationType::Sum(col_name) => col(col_name).sum().alias(&format!("sum_{}", col_name)),
                AggregationType::Avg(col_name) => col(col_name).mean().alias(&format!("avg_{}", col_name)),
                AggregationType::Count => col("*").count().alias("count"),
                AggregationType::Min(col_name) => col(col_name).min().alias(&format!("min_{}", col_name)),
                AggregationType::Max(col_name) => col(col_name).max().alias(&format!("max_{}", col_name)),
            })
            .collect();

        if !request.group_by.is_empty() {
            let group_cols: Vec<_> = request.group_by.iter().map(|c| col(c)).collect();
            lf = lf.groupby(&group_cols).agg(&agg_exprs);
        } else {
            lf = lf.select(&agg_exprs);
        }

        // 排序
        if let Some(sort) = request.sort {
            lf = lf.sort(&sort.column, sort.options);
        }

        lf.collect()
    }
}

#[derive(Debug, Clone)]
pub struct AggregateQuery {
    pub table: String,
    pub group_by: Vec<String>,
    pub aggregations: Vec<AggregationType>,
    pub filter: Option<Expr>,
    pub sort: Option<SortSpec>,
}

#[derive(Debug, Clone)]
pub enum AggregationType {
    Sum(String),
    Avg(String),
    Count,
    Min(String),
    Max(String),
}

// 使用示例
let query = AggregateQuery {
    table: "trades".to_string(),
    group_by: vec!["instrument_id".to_string()],
    aggregations: vec![
        AggregationType::Count,
        AggregationType::Sum("volume".to_string()),
        AggregationType::Avg("price".to_string()),
    ],
    filter: Some(col("timestamp").gt(lit("2025-10-01"))),
    sort: Some(SortSpec {
        column: "sum_volume".to_string(),
        options: SortOptions { descending: true, ..Default::default() },
    }),
};

let result = engine.aggregate(query)?;
```

### 3. 时间序列查询

#### 时间粒度聚合

```rust
impl QueryEngine {
    /// 时间序列聚合查询
    pub fn time_series_query(&self, request: TimeSeriesQuery) -> Result<DataFrame> {
        let df = self.data_sources.read()
            .get(&request.table)
            .ok_or_else(|| anyhow!("Table not found"))?
            .clone();

        let lf = df.lazy();

        // 解析时间粒度
        let duration = Self::parse_granularity(&request.granularity)?;

        // 准备聚合表达式
        let agg_exprs: Vec<_> = request.aggregations.iter()
            .map(|agg| match agg {
                AggregationType::Sum(col_name) => col(col_name).sum().alias(&format!("sum_{}", col_name)),
                AggregationType::Avg(col_name) => col(col_name).mean().alias(&format!("avg_{}", col_name)),
                AggregationType::Count => col("*").count().alias("count"),
                AggregationType::Min(col_name) => col(col_name).min().alias(&format!("min_{}", col_name)),
                AggregationType::Max(col_name) => col(col_name).max().alias(&format!("max_{}", col_name)),
            })
            .collect();

        // 动态分组
        let result_lf = lf.groupby_dynamic(
            col(&request.time_column),
            [],  // 空的额外分组列
            DynamicGroupOptions {
                every: duration,
                period: duration,
                offset: Duration::parse("0s")?,
                truncate: true,
                include_boundaries: false,
                closed_window: ClosedWindow::Left,
                start_by: StartBy::WindowBound,
            },
        )
        .agg(&agg_exprs);

        result_lf.collect()
    }

    /// 解析时间粒度字符串
    fn parse_granularity(granularity: &str) -> Result<Duration> {
        match granularity {
            "1s" => Ok(Duration::parse("1s")?),
            "5s" => Ok(Duration::parse("5s")?),
            "10s" => Ok(Duration::parse("10s")?),
            "30s" => Ok(Duration::parse("30s")?),
            "1m" => Ok(Duration::parse("1m")?),
            "5m" => Ok(Duration::parse("5m")?),
            "15m" => Ok(Duration::parse("15m")?),
            "30m" => Ok(Duration::parse("30m")?),
            "1h" => Ok(Duration::parse("1h")?),
            "4h" => Ok(Duration::parse("4h")?),
            "1d" => Ok(Duration::parse("1d")?),
            other => Err(anyhow!("Unsupported granularity: {}", other)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeSeriesQuery {
    /// 表名
    pub table: String,

    /// 时间列名
    pub time_column: String,

    /// 时间粒度 ("1s", "1m", "5m", "1h", "1d" 等)
    pub granularity: String,

    /// 聚合操作
    pub aggregations: Vec<AggregationType>,

    /// 时间范围 (可选)
    pub time_range: Option<(i64, i64)>,
}

// 使用示例: 计算每分钟的 OHLCV
let query = TimeSeriesQuery {
    table: "trades".to_string(),
    time_column: "timestamp".to_string(),
    granularity: "1m".to_string(),
    aggregations: vec![
        AggregationType::Count,
        AggregationType::Sum("volume".to_string()),
        AggregationType::Avg("price".to_string()),
        AggregationType::Min("price".to_string()),
        AggregationType::Max("price".to_string()),
    ],
    time_range: Some((1696118400000, 1696204800000)), // 2023-10-01 to 2023-10-02
};

let ohlcv = engine.time_series_query(query)?;
```

#### K线生成

```rust
impl QueryEngine {
    /// 生成 K 线数据
    pub fn generate_klines(
        &self,
        table: &str,
        granularity: &str,
        time_range: Option<(i64, i64)>,
    ) -> Result<DataFrame> {
        let df = self.data_sources.read()
            .get(table)
            .ok_or_else(|| anyhow!("Table not found"))?
            .clone();

        let mut lf = df.lazy();

        // 应用时间过滤
        if let Some((start, end)) = time_range {
            lf = lf.filter(
                col("timestamp").gt_eq(lit(start))
                    .and(col("timestamp").lt(lit(end)))
            );
        }

        // 解析粒度
        let duration = Self::parse_granularity(granularity)?;

        // K线聚合
        let kline_lf = lf.groupby_dynamic(
            col("timestamp"),
            [],
            DynamicGroupOptions {
                every: duration,
                period: duration,
                offset: Duration::parse("0s")?,
                truncate: true,
                include_boundaries: true,
                closed_window: ClosedWindow::Left,
                start_by: StartBy::WindowBound,
            },
        )
        .agg(&[
            col("price").first().alias("open"),
            col("price").max().alias("high"),
            col("price").min().alias("low"),
            col("price").last().alias("close"),
            col("volume").sum().alias("volume"),
            col("volume").count().alias("trade_count"),
        ]);

        kline_lf.collect()
    }
}

// 使用示例: 生成 5 分钟 K 线
let klines = engine.generate_klines(
    "trades",
    "5m",
    Some((1696118400000, 1696204800000)),
)?;

println!("{}", klines);
// 输出:
// ┌─────────────┬────────┬────────┬────────┬────────┬────────┬─────────────┐
// │ timestamp   │ open   │ high   │ low    │ close  │ volume │ trade_count │
// ├─────────────┼────────┼────────┼────────┼────────┼────────┼─────────────┤
// │ 2023-10-01  │ 3250.0 │ 3255.0 │ 3248.0 │ 3253.0 │ 12500  │ 45          │
// │ 00:00:00    │        │        │        │        │        │             │
// │ 2023-10-01  │ 3253.0 │ 3258.0 │ 3252.0 │ 3256.0 │ 8900   │ 32          │
// │ 00:05:00    │        │        │        │        │        │             │
// │ ...         │ ...    │ ...    │ ...    │ ...    │ ...    │ ...         │
// └─────────────┴────────┴────────┴────────┴────────┴────────┴─────────────┘
```

## 📊 SSTable 扫描器

### OLAP SSTable 扫描

```rust
// src/storage/query/scanner.rs

use arrow2::io::parquet::read::*;

pub struct SstableScanner {
    /// SSTable 根目录
    base_path: PathBuf,
}

impl SstableScanner {
    /// 扫描所有 Parquet 文件
    pub fn scan_all_sstables(&self, instrument: &str) -> Result<DataFrame> {
        let pattern = format!("{}/{}/**/*.parquet", self.base_path.display(), instrument);
        let files = glob::glob(&pattern)?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        if files.is_empty() {
            return Err(anyhow!("No SSTable files found for instrument: {}", instrument));
        }

        // 使用 Polars scan_parquet 批量读取
        let lf = LazyFrame::scan_parquet_files(
            files,
            Default::default(),
        )?;

        lf.collect()
    }

    /// 扫描指定时间范围
    pub fn scan_time_range(
        &self,
        instrument: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<DataFrame> {
        let df = self.scan_all_sstables(instrument)?;

        df.lazy()
            .filter(
                col("timestamp").gt_eq(lit(start_time))
                    .and(col("timestamp").lt(lit(end_time)))
            )
            .collect()
    }

    /// 扫描特定列 (列剪裁)
    pub fn scan_columns(
        &self,
        instrument: &str,
        columns: &[&str],
    ) -> Result<DataFrame> {
        let pattern = format!("{}/{}/**/*.parquet", self.base_path.display(), instrument);
        let files = glob::glob(&pattern)?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        // Parquet 自动进行列剪裁
        let args = ScanArgsParquet {
            n_rows: None,
            cache: true,
            parallel: ParallelStrategy::Auto,
            rechunk: true,
            row_count: None,
            low_memory: false,
            cloud_options: None,
            use_statistics: true, // 启用统计信息加速
        };

        let lf = LazyFrame::scan_parquet_files(files, args)?;

        // 选择列
        let cols: Vec<_> = columns.iter().map(|c| col(c)).collect();
        lf.select(&cols).collect()
    }
}
```

## ⚡ 性能优化

### 1. LazyFrame 延迟执行

```rust
// ✅ 推荐: 使用 LazyFrame 进行查询优化
let result = df.lazy()
    .filter(col("timestamp").gt(lit("2025-10-01")))
    .select(&[col("instrument_id"), col("price")])
    .groupby(&[col("instrument_id")])
    .agg(&[col("price").mean()])
    .sort("instrument_id", Default::default())
    .collect()?; // 最后才执行

// ❌ 不推荐: 立即执行每个操作
let result = df
    .filter(&col("timestamp").gt(lit("2025-10-01")))?
    .select(&["instrument_id", "price"])?
    .groupby(&["instrument_id"])?
    .mean()?;
```

**优势**:
- 查询计划优化 (predicate pushdown, projection pushdown)
- 减少中间数据拷贝
- 自动并行化

### 2. 谓词下推 (Predicate Pushdown)

```rust
// Polars 自动将过滤条件下推到 Parquet 读取层
let lf = LazyFrame::scan_parquet(path, Default::default())?
    .filter(col("timestamp").gt(lit("2025-10-01"))) // ← 自动下推
    .select(&[col("instrument_id"), col("price")]);  // ← 列剪裁

// 只读取满足条件的 Row Group 和列
let result = lf.collect()?;
```

### 3. 列剪裁 (Projection Pushdown)

```rust
// 只读取需要的列，减少 I/O
let lf = LazyFrame::scan_parquet(path, Default::default())?
    .select(&[col("instrument_id"), col("price"), col("volume")]); // 只读取3列

let result = lf.collect()?;
```

### 4. 并行查询

```rust
// 设置并行度
std::env::set_var("POLARS_MAX_THREADS", "8");

// 或通过 API
let lf = df.lazy()
    .with_streaming(true) // 启用流式执行
    .collect()?;
```

## 📊 性能指标

| 操作 | 数据量 | 延迟 | 吞吐 | 状态 |
|------|--------|------|------|------|
| SQL 查询 (简单) | 100 rows | ~5ms | - | ✅ |
| SQL 查询 (JOIN) | 10K rows | ~35ms | - | ✅ |
| 聚合查询 | 100K rows | ~50ms | 2M rows/s | ✅ |
| 时间序列聚合 | 1M rows | ~120ms | 8M rows/s | ✅ |
| Parquet 全表扫描 | 1GB | ~700ms | 1.5 GB/s | ✅ |
| Parquet 列剪裁 | 1GB (3/10 列) | ~250ms | 4 GB/s | ✅ |
| Parquet 谓词下推 | 1GB (1% 匹配) | ~50ms | 20 GB/s | ✅ |

## 🛠️ 配置示例

```toml
# config/query.toml
[query_engine]
max_query_timeout_secs = 300
enable_parallelism = true
num_threads = 8  # 或留空自动检测

[lazy_frame]
enable_streaming = true
chunk_size = 100000

[parquet]
use_statistics = true  # 启用统计信息
parallel_strategy = "auto"  # "auto", "columns", "row_groups"
```

## 💡 最佳实践

### 1. 使用 LazyFrame

```rust
// ✅ 总是使用 LazyFrame
let result = df.lazy()
    .filter(...)
    .select(...)
    .groupby(...)
    .collect()?;

// ❌ 避免链式 DataFrame 操作
let result = df.filter(...)?.select(...)?.groupby(...)?;
```

### 2. 提前过滤数据

```rust
// ✅ 过滤放在最前面
let lf = df.lazy()
    .filter(col("timestamp").gt(lit("2025-10-01"))) // 先过滤
    .select(&cols)
    .groupby(&group_cols);

// ❌ 过滤放在后面
let lf = df.lazy()
    .select(&cols)
    .groupby(&group_cols)
    .filter(col("timestamp").gt(lit("2025-10-01"))); // 后过滤,效率低
```

### 3. 选择合适的聚合粒度

```rust
// 对于分钟级数据,使用 5m 而不是 1s
let query = TimeSeriesQuery {
    granularity: "5m".to_string(), // ✅ 合理粒度
    // granularity: "1s".to_string(), // ❌ 粒度过细
    ...
};
```

## 🔍 故障排查

### 问题 1: 查询超时

**症状**: 查询执行超过 5 分钟

**解决方案**:
1. 增加超时时间
2. 启用流式执行
3. 减少数据量 (时间范围过滤)

```rust
config.max_query_timeout_secs = 600; // 10 分钟

// 启用流式执行
let lf = df.lazy()
    .with_streaming(true)
    .collect()?;
```

### 问题 2: 内存不足

**症状**: OOM (Out of Memory)

**解决方案**:
1. 使用 LazyFrame 延迟执行
2. 启用流式执行
3. 分批处理数据

```rust
// 流式处理大数据
let lf = LazyFrame::scan_parquet(path, Default::default())?
    .with_streaming(true);

// 或分批读取
for chunk in lf.collect_chunked()? {
    process_chunk(chunk)?;
}
```

### 问题 3: Parquet 读取慢

**症状**: 扫描 1GB Parquet 文件 > 2 秒

**排查步骤**:
1. 检查是否启用列剪裁
2. 检查是否启用谓词下推
3. 检查并行度设置

**解决方案**:
```rust
let args = ScanArgsParquet {
    use_statistics: true,  // 启用统计信息
    parallel: ParallelStrategy::Auto, // 自动并行
    ..Default::default()
};

let lf = LazyFrame::scan_parquet_files(files, args)?
    .filter(col("timestamp").gt(lit("2025-10-01"))) // 谓词下推
    .select(&[col("price"), col("volume")]); // 列剪裁
```

## 🔄 流批混合查询 ✨ NEW

`src/query/hybrid.rs` 提供流式和批处理混合的查询能力：

### 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                  流批混合查询架构                             │
│                                                             │
│    Client Query                                             │
│          │                                                  │
│          ▼                                                  │
│    ┌─────────────────────────────────────┐                 │
│    │         HybridQueryEngine           │                 │
│    │  ┌─────────────┬─────────────────┐  │                 │
│    │  │ StreamBuffer│   BatchSource   │  │                 │
│    │  │  (实时数据)  │   (历史数据)    │  │                 │
│    │  │  DashMap    │   Parquet/SST   │  │                 │
│    │  └─────────────┴─────────────────┘  │                 │
│    │           │               │         │                 │
│    │           └───────┬───────┘         │                 │
│    │                   ▼                 │                 │
│    │           MergeStrategy             │                 │
│    │   (ByTimestamp/StreamFirst/Latest)  │                 │
│    └─────────────────────────────────────┘                 │
│                   │                                         │
│                   ▼                                         │
│            QueryResult                                      │
└─────────────────────────────────────────────────────────────┘
```

### 流数据缓存 (StreamBuffer)

```rust
use qaexchange::query::hybrid::{StreamBuffer, Record, RecordValue};

// 创建流缓存（最大 5 分钟，最多 10000 条）
let buffer = StreamBuffer::new(Duration::from_secs(300), 10000);

// 推送实时数据
buffer.push(Record::new("cu2501", timestamp)
    .with_value("price", RecordValue::Float(85000.0))
    .with_value("volume", RecordValue::Int(100)));

// 查询最新 N 条
let latest = buffer.query_latest("cu2501", 10);

// 查询时间范围
let range = buffer.query_range("cu2501", start_ts, end_ts);
```

### 混合查询引擎

```rust
use qaexchange::query::hybrid::{HybridQueryEngine, HybridConfig, MergeStrategy};

// 配置混合查询
let config = HybridConfig {
    stream_max_latency: Duration::from_millis(100),  // 流数据最大延迟
    batch_timeout: Duration::from_secs(30),           // 批查询超时
    merge_strategy: MergeStrategy::ByTimestamp,       // 按时间戳合并
    stream_priority_window: Duration::from_secs(60),  // 1分钟内优先流数据
};

let engine = HybridQueryEngine::new(config)
    .with_batch_source(batch_data_source);

// 执行混合查询
let result = engine.query(
    "cu2501",
    start_ts,
    end_ts,
    &["price".to_string(), "volume".to_string()],
).await?;

println!("来源: {:?}", result.source);  // DataSource::Merged
println!("记录数: {}", result.records.len());
println!("执行时间: {:?}", result.execution_time);
```

### 合并策略

| 策略 | 说明 | 适用场景 |
|------|------|----------|
| `StreamFirst` | 流数据优先，批数据补充旧数据 | 实时性要求高 |
| `BatchFirst` | 批数据优先，流数据补充新数据 | 数据一致性要求高 |
| `ByTimestamp` | 按时间戳合并去重 | 通用场景 |
| `Latest` | 取最新数据 | 只关心最新状态 |

### 批处理数据源接口

```rust
#[async_trait]
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
```

### 聚合操作

```rust
use qaexchange::query::hybrid::{Aggregation, AggregateOp};

let aggregations = vec![
    Aggregation { field: "price".to_string(), op: AggregateOp::Avg, alias: "avg_price".to_string() },
    Aggregation { field: "volume".to_string(), op: AggregateOp::Sum, alias: "total_volume".to_string() },
    Aggregation { field: "price".to_string(), op: AggregateOp::Max, alias: "high".to_string() },
    Aggregation { field: "price".to_string(), op: AggregateOp::Min, alias: "low".to_string() },
];
```

### 性能指标

| 操作 | 延迟 | 说明 |
|------|------|------|
| StreamBuffer.push() | ~50 ns | DashMap 写入 |
| StreamBuffer.query_latest() | ~200 ns | DashMap 读取 |
| HybridQuery (缓存命中) | < 1 ms | 流数据直接返回 |
| HybridQuery (批查询) | < 50 ms | Parquet 扫描 + 合并 |

---

## 📚 相关文档

- [SSTable 格式](sstable.md) - Parquet SSTable 详细格式
- [MemTable 实现](memtable.md) - OLAP MemTable 与查询引擎集成
- [因子计算系统](../factor/README.md) - 因子 DAG 与查询引擎集成
- [Polars 官方文档](https://pola-rs.github.io/polars-book/) - 完整 API 参考
- [Arrow2 文档](https://jorgecarleitao.github.io/arrow2/) - 底层列式格式
- [查询引擎详细设计](../../storage/05_ARROW2_QUERY_ENGINE.md) - 架构细节

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
