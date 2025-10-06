# Arrow2 查询引擎设计

> 基于 Arrow2 + Polars 的高性能列式存储和查询引擎

**版本**: v1.0.0
**最后更新**: 2025-10-03

---

## 📋 目录

- [架构概览](#架构概览)
- [Arrow2 集成方案](#arrow2-集成方案)
- [查询引擎设计](#查询引擎设计)
- [性能优化](#性能优化)
- [实施计划](#实施计划)

---

## 架构概览

### 设计目标

1. **列式存储**：使用 Arrow2 列式格式，优化分析查询
2. **零拷贝查询**：Arrow2 内存映射，避免数据拷贝
3. **向量化执行**：SIMD 加速，100x 性能提升
4. **SQL 支持**：Polars SQL API，兼容标准 SQL
5. **流式计算**：支持增量查询和实时聚合

### 数据流

```
写入路径 (OLTP):
OrderRequest → WAL (rkyv) → MemTable → SSTable (Arrow2 IPC)
                ↓
             确认返回

查询路径 (OLAP):
SQL Query → Polars Parser → Arrow2 Query Engine → Arrow2 RecordBatch
                                    ↓
                           [MemTable | SSTable (mmap)]
                                    ↓
                           零拷贝 + 向量化执行
```

### 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                      应用层                                  │
│  OLTP (写入) | OLAP (查询分析)                              │
└──────────────────┬──────────────────┬──────────────────────┘
                   ↓                  ↓
┌─────────────────────────────────────────────────────────────┐
│         存储引擎 (Hybrid OLTP + OLAP)                        │
│                                                              │
│  ┌──────────────────┐    ┌──────────────────────┐          │
│  │  OLTP Path       │    │  OLAP Path           │          │
│  │                  │    │                      │          │
│  │  WAL (rkyv)      │    │  Query Engine        │          │
│  │  MemTable        │    │  (Polars + Arrow2)   │          │
│  │  SSTable (rkyv)  │    │                      │          │
│  └──────┬───────────┘    └──────┬───────────────┘          │
│         ↓                        ↓                          │
│  ┌──────────────────────────────────────────────┐          │
│  │  Arrow2 Columnar Storage                      │          │
│  │                                               │          │
│  │  ┌────────────┐  ┌────────────┐             │          │
│  │  │ RecordBatch│  │  Parquet   │             │          │
│  │  │ (MemTable) │  │  (SSTable) │             │          │
│  │  └────────────┘  └────────────┘             │          │
│  └──────────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────────┘
```

---

## Arrow2 集成方案

### 1. 列式 MemTable

**当前 MemTable**：行式存储（SkipMap）
```rust
// 现有实现 - 适合 OLTP
SkipMap<Vec<u8>, Vec<u8>>  // Key-Value 行式存储
```

**Arrow2 MemTable**：列式存储（适合 OLAP）
```rust
// src/storage/memtable/arrow_memtable.rs

use arrow2::array::*;
use arrow2::chunk::Chunk;
use arrow2::datatypes::*;
use std::sync::Arc;
use parking_lot::RwLock;

/// Arrow2 列式 MemTable（用于分析查询）
pub struct ArrowMemTable {
    schema: Arc<Schema>,
    chunks: Arc<RwLock<Vec<Chunk<Arc<dyn Array>>>>>,
    max_size: usize,  // 最大 128MB
    current_size: usize,
}

impl ArrowMemTable {
    pub fn new(schema: Schema) -> Self {
        Self {
            schema: Arc::new(schema),
            chunks: Arc::new(RwLock::new(Vec::new())),
            max_size: 128 * 1024 * 1024,
            current_size: 0,
        }
    }

    /// 插入一批数据（RecordBatch）
    pub fn insert_batch(&mut self, chunk: Chunk<Arc<dyn Array>>) -> Result<(), String> {
        let batch_size = chunk.len();

        if self.current_size + batch_size > self.max_size {
            return Err("MemTable full".to_string());
        }

        self.chunks.write().push(chunk);
        self.current_size += batch_size;

        Ok(())
    }

    /// 查询数据（返回 Arrow2 RecordBatch）
    pub fn query(&self, filter: Option<&dyn Fn(&Chunk<Arc<dyn Array>>) -> bool>)
        -> Vec<Chunk<Arc<dyn Array>>>
    {
        let chunks = self.chunks.read();

        if let Some(f) = filter {
            chunks.iter()
                .filter(|chunk| f(chunk))
                .cloned()
                .collect()
        } else {
            chunks.clone()
        }
    }

    /// 转换为 Polars DataFrame
    pub fn to_polars_df(&self) -> Result<polars::prelude::DataFrame, String> {
        use polars::prelude::*;

        let chunks = self.chunks.read();

        // 将 Arrow2 Chunk 转换为 Polars DataFrame
        // Polars 内部使用 Arrow2，可以零拷贝转换
        let mut dfs = Vec::new();

        for chunk in chunks.iter() {
            // TODO: Arrow2 → Polars 转换
            // let df = DataFrame::from_arrow_chunk(chunk, &self.schema)?;
            // dfs.push(df);
        }

        // 合并所有 DataFrame
        // Ok(concat_df(&dfs)?)
        Err("Not implemented".to_string())
    }
}
```

### 2. Parquet SSTable

**当前 SSTable**：自定义二进制格式（rkyv）

**Arrow2 SSTable**：Parquet 格式（列式压缩）

```rust
// src/storage/sstable/parquet_sstable.rs

use arrow2::io::parquet::write::*;
use arrow2::io::parquet::read::*;
use arrow2::chunk::Chunk;
use arrow2::datatypes::*;
use std::fs::File;

/// Parquet SSTable Builder
pub struct ParquetSSTableBuilder {
    file: File,
    schema: Arc<Schema>,
    writer: FileWriter<File>,
    row_count: usize,
}

impl ParquetSSTableBuilder {
    pub fn new(file_path: &str, schema: Schema) -> Result<Self, String> {
        let file = File::create(file_path)
            .map_err(|e| format!("Create file failed: {}", e))?;

        // Parquet 写入配置
        let options = WriteOptions {
            write_statistics: true,
            compression: CompressionOptions::Snappy,  // Snappy 压缩
            version: Version::V2,
        };

        let encodings = schema.fields.iter()
            .map(|f| Encoding::Plain)  // 默认编码
            .collect();

        let writer = FileWriter::try_new(
            file,
            schema.clone(),
            options,
        ).map_err(|e| format!("Create writer failed: {}", e))?;

        Ok(Self {
            file,
            schema: Arc::new(schema),
            writer,
            row_count: 0,
        })
    }

    /// 添加一批数据
    pub fn add_batch(&mut self, chunk: Chunk<Arc<dyn Array>>) -> Result<(), String> {
        self.writer.write(&chunk)
            .map_err(|e| format!("Write batch failed: {}", e))?;

        self.row_count += chunk.len();

        Ok(())
    }

    /// 完成写入
    pub fn finish(self) -> Result<(), String> {
        self.writer.end(None)
            .map_err(|e| format!("Finish failed: {}", e))?;

        Ok(())
    }
}

/// Parquet SSTable Reader
pub struct ParquetSSTableReader {
    reader: FileReader<File>,
    schema: Arc<Schema>,
    row_count: usize,
}

impl ParquetSSTableReader {
    pub fn open(file_path: &str) -> Result<Self, String> {
        let mut file = File::open(file_path)
            .map_err(|e| format!("Open file failed: {}", e))?;

        let metadata = read_metadata(&mut file)
            .map_err(|e| format!("Read metadata failed: {}", e))?;

        let schema = infer_schema(&metadata)
            .map_err(|e| format!("Infer schema failed: {}", e))?;

        let row_count = metadata.num_rows;

        let reader = FileReader::try_new(file, None, None, None, None)
            .map_err(|e| format!("Create reader failed: {}", e))?;

        Ok(Self {
            reader,
            schema: Arc::new(schema),
            row_count,
        })
    }

    /// 读取所有数据（零拷贝）
    pub fn read_all(&mut self) -> Result<Vec<Chunk<Arc<dyn Array>>>, String> {
        let mut chunks = Vec::new();

        for maybe_chunk in &mut self.reader {
            let chunk = maybe_chunk
                .map_err(|e| format!("Read chunk failed: {}", e))?;

            chunks.push(chunk);
        }

        Ok(chunks)
    }

    /// 读取指定行范围
    pub fn read_range(&mut self, start: usize, end: usize)
        -> Result<Chunk<Arc<dyn Array>>, String>
    {
        // TODO: 实现范围读取
        Err("Not implemented".to_string())
    }

    /// 转换为 Polars DataFrame
    pub fn to_polars_df(&mut self) -> Result<polars::prelude::DataFrame, String> {
        use polars::prelude::*;

        // Polars 原生支持 Parquet
        let df = LazyFrame::scan_parquet(
            self.file_path.clone(),
            ScanArgsParquet::default()
        )
        .map_err(|e| format!("Scan parquet failed: {}", e))?
        .collect()
        .map_err(|e| format!("Collect failed: {}", e))?;

        Ok(df)
    }
}
```

### 3. 双模式存储

**设计思路**：
- **OLTP 路径**：rkyv + SkipMap（低延迟写入）
- **OLAP 路径**：Arrow2 + Parquet（高效查询）
- **异步转换**：后台线程将 OLTP 数据转换为 OLAP 格式

```rust
// src/storage/hybrid_storage.rs

pub struct HybridStorage {
    // OLTP 路径
    oltp_memtable: Arc<MemTableManager>,  // rkyv SkipMap
    oltp_sstables: Arc<RwLock<Vec<SSTableReader>>>,  // rkyv SSTable

    // OLAP 路径
    olap_memtable: Arc<RwLock<ArrowMemTable>>,  // Arrow2 RecordBatch
    olap_sstables: Arc<RwLock<Vec<ParquetSSTableReader>>>,  // Parquet

    // 转换器（OLTP → OLAP）
    converter: Arc<OltpToOlapConverter>,
}

impl HybridStorage {
    /// 写入数据（OLTP 路径）
    pub fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        // 1. 写入 OLTP MemTable（低延迟）
        self.oltp_memtable.insert(key.clone(), value.clone())?;

        // 2. 异步转换为 OLAP 格式
        let converter = self.converter.clone();
        let olap_memtable = self.olap_memtable.clone();

        tokio::spawn(async move {
            if let Ok(chunk) = converter.convert(key, value) {
                olap_memtable.write().insert_batch(chunk).ok();
            }
        });

        Ok(())
    }

    /// 查询数据（OLAP 路径）
    pub fn query(&self, sql: &str) -> Result<polars::prelude::DataFrame, String> {
        use polars::prelude::*;

        // 1. 从 OLAP MemTable 获取数据
        let memtable_df = self.olap_memtable.read().to_polars_df()?;

        // 2. 从 OLAP SSTable 获取数据
        let mut sstable_dfs = Vec::new();
        for sstable in self.olap_sstables.read().iter() {
            sstable_dfs.push(sstable.to_polars_df()?);
        }

        // 3. 合并所有数据
        let mut all_dfs = vec![memtable_df];
        all_dfs.extend(sstable_dfs);

        let combined = concat_df(&all_dfs)?;

        // 4. 执行 SQL 查询
        let ctx = SQLContext::new();
        ctx.register("data", combined.lazy());

        let result = ctx.execute(sql)?
            .collect()?;

        Ok(result)
    }
}
```

---

## 查询引擎设计

### 1. Schema 定义

```rust
// src/storage/query/schema.rs

use arrow2::datatypes::*;

/// 订单表 Schema
pub fn order_schema() -> Schema {
    Schema::from(vec![
        Field::new("order_id", DataType::Utf8, false),
        Field::new("user_id", DataType::Utf8, false),
        Field::new("instrument_id", DataType::Utf8, false),
        Field::new("direction", DataType::UInt8, false),  // 0=BUY, 1=SELL
        Field::new("offset", DataType::UInt8, false),     // 0=OPEN, 1=CLOSE
        Field::new("price", DataType::Float64, false),
        Field::new("volume", DataType::Float64, false),
        Field::new("timestamp", DataType::Int64, false),
    ])
}

/// 成交表 Schema
pub fn trade_schema() -> Schema {
    Schema::from(vec![
        Field::new("trade_id", DataType::Utf8, false),
        Field::new("order_id", DataType::Utf8, false),
        Field::new("instrument_id", DataType::Utf8, false),
        Field::new("price", DataType::Float64, false),
        Field::new("volume", DataType::Float64, false),
        Field::new("timestamp", DataType::Int64, false),
    ])
}

/// 账户表 Schema
pub fn account_schema() -> Schema {
    Schema::from(vec![
        Field::new("user_id", DataType::Utf8, false),
        Field::new("balance", DataType::Float64, false),
        Field::new("available", DataType::Float64, false),
        Field::new("frozen", DataType::Float64, false),
        Field::new("margin", DataType::Float64, false),
        Field::new("timestamp", DataType::Int64, false),
    ])
}
```

### 2. 查询引擎

```rust
// src/storage/query/engine.rs

use polars::prelude::*;
use std::sync::Arc;

pub struct QueryEngine {
    storage: Arc<HybridStorage>,
    ctx: SQLContext,
}

impl QueryEngine {
    pub fn new(storage: Arc<HybridStorage>) -> Self {
        let ctx = SQLContext::new();

        Self { storage, ctx }
    }

    /// 执行 SQL 查询
    pub fn query(&mut self, sql: &str) -> Result<DataFrame, String> {
        // 注册表
        self.register_tables()?;

        // 执行查询
        let result = self.ctx.execute(sql)
            .map_err(|e| format!("Query failed: {}", e))?
            .collect()
            .map_err(|e| format!("Collect failed: {}", e))?;

        Ok(result)
    }

    /// 注册表
    fn register_tables(&mut self) -> Result<(), String> {
        // 订单表
        let orders_df = self.storage.get_orders()?;
        self.ctx.register("orders", orders_df.lazy());

        // 成交表
        let trades_df = self.storage.get_trades()?;
        self.ctx.register("trades", trades_df.lazy());

        // 账户表
        let accounts_df = self.storage.get_accounts()?;
        self.ctx.register("accounts", accounts_df.lazy());

        Ok(())
    }

    /// 预定义查询：用户交易统计
    pub fn user_trade_stats(&mut self, user_id: &str) -> Result<DataFrame, String> {
        let sql = format!(r#"
            SELECT
                instrument_id,
                COUNT(*) as trade_count,
                SUM(volume) as total_volume,
                AVG(price) as avg_price,
                MIN(price) as min_price,
                MAX(price) as max_price
            FROM trades
            WHERE order_id IN (
                SELECT order_id FROM orders WHERE user_id = '{}'
            )
            GROUP BY instrument_id
            ORDER BY total_volume DESC
        "#, user_id);

        self.query(&sql)
    }

    /// 预定义查询：实时行情统计
    pub fn market_stats(&mut self, instrument_id: &str, window_secs: i64)
        -> Result<DataFrame, String>
    {
        let now = chrono::Utc::now().timestamp();
        let start_time = now - window_secs;

        let sql = format!(r#"
            SELECT
                COUNT(*) as trade_count,
                SUM(volume) as volume,
                AVG(price) as vwap,
                MIN(price) as low,
                MAX(price) as high,
                FIRST(price) as open,
                LAST(price) as close
            FROM trades
            WHERE instrument_id = '{}'
              AND timestamp >= {}
        "#, instrument_id, start_time);

        self.query(&sql)
    }

    /// 预定义查询：账户盈亏分析
    pub fn account_pnl(&mut self, user_id: &str) -> Result<DataFrame, String> {
        let sql = format!(r#"
            SELECT
                a.user_id,
                a.balance,
                a.available,
                a.frozen,
                a.margin,
                SUM(CASE WHEN o.direction = 0 THEN t.volume * t.price ELSE -t.volume * t.price END) as pnl
            FROM accounts a
            LEFT JOIN orders o ON a.user_id = o.user_id
            LEFT JOIN trades t ON o.order_id = t.order_id
            WHERE a.user_id = '{}'
            GROUP BY a.user_id, a.balance, a.available, a.frozen, a.margin
        "#, user_id);

        self.query(&sql)
    }
}
```

### 3. 流式查询

```rust
// src/storage/query/streaming.rs

use polars::prelude::*;
use tokio::sync::mpsc;

pub struct StreamingQueryEngine {
    engine: Arc<RwLock<QueryEngine>>,
    update_interval: Duration,
}

impl StreamingQueryEngine {
    /// 流式查询：实时更新
    pub async fn stream_query(
        &self,
        sql: String,
    ) -> mpsc::UnboundedReceiver<DataFrame> {
        let (tx, rx) = mpsc::unbounded_channel();

        let engine = self.engine.clone();
        let interval = self.update_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                let mut eng = engine.write();

                if let Ok(df) = eng.query(&sql) {
                    if tx.send(df).is_err() {
                        break;  // 接收方已关闭
                    }
                }
            }
        });

        rx
    }

    /// 流式聚合：增量计算
    pub async fn stream_aggregate(
        &self,
        table: &str,
        group_by: Vec<String>,
        aggregates: Vec<String>,
    ) -> mpsc::UnboundedReceiver<DataFrame> {
        // TODO: 实现增量聚合
        let (tx, rx) = mpsc::unbounded_channel();
        rx
    }
}
```

---

## 性能优化

### 1. 零拷贝查询

```rust
// Arrow2 零拷贝优势
// 1. mmap 文件映射：直接访问磁盘数据，不需要读入内存
// 2. 列式存储：只读取需要的列，减少 I/O
// 3. SIMD 向量化：批量处理，100x 性能提升

// 示例：读取 Parquet 文件（零拷贝）
let reader = ParquetSSTableReader::open("orders.parquet")?;

// 只读取需要的列
let projection = vec![0, 2, 5];  // order_id, instrument_id, price

// 零拷贝读取
let chunks = reader.read_with_projection(&projection)?;
```

### 2. 谓词下推

```rust
// Parquet 文件支持谓词下推（Predicate Pushdown）
// 在读取时就过滤数据，减少内存占用

use arrow2::io::parquet::read::*;

// 示例：只读取 price > 100 的数据
let filter = |row_group: &RowGroupMetaData| -> bool {
    // 检查 row group 的统计信息
    if let Some(stats) = row_group.column(5).statistics() {
        if let Some(max) = stats.max_value {
            // 如果最大值都 < 100，跳过这个 row group
            return decode_float64(max) > 100.0;
        }
    }
    true
};

let reader = FileReader::try_new(file, Some(filter), None, None, None)?;
```

### 3. 向量化执行

```rust
// Polars 自动进行向量化执行

let df = self.query(r#"
    SELECT
        SUM(volume * price) as total_value
    FROM trades
    WHERE instrument_id = 'IX2401'
"#)?;

// Polars 内部：
// 1. 将 volume 和 price 列读入 Arrow2 Array
// 2. 使用 SIMD 指令批量计算 volume * price
// 3. 使用 SIMD 指令批量求和
// → 比逐行计算快 100x
```

### 4. 并行查询

```rust
// Polars 自动并行化查询

let df = LazyFrame::scan_parquet("trades.parquet")?
    .filter(col("timestamp").gt(start_time))
    .groupby([col("instrument_id")])
    .agg([
        col("volume").sum().alias("total_volume"),
        col("price").mean().alias("avg_price"),
    ])
    .collect()?;  // 自动并行执行

// Polars 会：
// 1. 并行读取多个 Parquet 文件
// 2. 并行过滤
// 3. 并行分组聚合
// 4. 合并结果
```

---

## 实施计划

### Phase 8: Arrow2 查询引擎 (2 周)

#### Week 1: Arrow2 集成

- [ ] 实现 ArrowMemTable
  - [ ] RecordBatch 存储
  - [ ] 查询接口
  - [ ] Polars 转换

- [ ] 实现 ParquetSSTableBuilder
  - [ ] Parquet 写入
  - [ ] 压缩配置（Snappy）
  - [ ] Schema 定义

- [ ] 实现 ParquetSSTableReader
  - [ ] Parquet 读取
  - [ ] 零拷贝 mmap
  - [ ] 谓词下推

#### Week 2: 查询引擎

- [ ] 实现 HybridStorage
  - [ ] OLTP/OLAP 双路径
  - [ ] 异步转换

- [ ] 实现 QueryEngine
  - [ ] Polars SQL 集成
  - [ ] 表注册
  - [ ] 预定义查询

- [ ] 实现 StreamingQueryEngine
  - [ ] 流式查询
  - [ ] 增量聚合

- [ ] 性能测试
  - [ ] 查询延迟
  - [ ] 吞吐量
  - [ ] 内存占用

---

## 性能目标

| 指标 | 目标 | 实现方式 |
|------|------|---------|
| **点查询延迟** | P99 < 1ms | MemTable 索引 |
| **扫描吞吐** | > 1GB/s | 零拷贝 mmap |
| **聚合查询** | > 100M rows/s | SIMD 向量化 |
| **JOIN 性能** | > 10M rows/s | Hash Join |
| **存储压缩比** | 5-10x | Parquet 列式压缩 |

---

## 使用示例

### 完整示例：OLTP + OLAP

```rust
use qaexchange::storage::{HybridStorage, QueryEngine};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建混合存储
    let storage = Arc::new(HybridStorage::new("/data"));

    // 2. OLTP 写入（低延迟）
    storage.insert(
        order_id.as_bytes().to_vec(),
        order_data.to_rkyv_bytes()?,
    )?;

    // 3. OLAP 查询（高吞吐）
    let mut engine = QueryEngine::new(storage.clone());

    // SQL 查询
    let stats = engine.query(r#"
        SELECT
            instrument_id,
            COUNT(*) as trade_count,
            SUM(volume) as total_volume,
            AVG(price) as avg_price
        FROM trades
        WHERE timestamp >= 1672531200
        GROUP BY instrument_id
        ORDER BY total_volume DESC
        LIMIT 10
    "#)?;

    println!("{:?}", stats);

    // 预定义查询
    let user_stats = engine.user_trade_stats("user_001")?;
    println!("{:?}", user_stats);

    // 流式查询
    let streaming = StreamingQueryEngine::new(engine.clone());
    let mut rx = streaming.stream_query(
        "SELECT * FROM trades WHERE timestamp > NOW() - 3600".to_string()
    ).await;

    while let Some(df) = rx.recv().await {
        println!("Real-time update: {:?}", df);
    }

    Ok(())
}
```

---

## 相关链接

- [Arrow2 Documentation](https://jorgecarleitao.github.io/arrow2/)
- [Polars Documentation](https://pola-rs.github.io/polars/)
- [Parquet Format Specification](https://parquet.apache.org/docs/)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
