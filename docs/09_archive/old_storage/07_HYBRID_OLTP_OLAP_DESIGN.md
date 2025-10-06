# OLTP/OLAP 混合架构设计

## 架构理念

**核心思想**：根据数据的"热度"和查询模式，智能路由到不同的存储引擎。

```
实时交易数据流:
  ↓
WAL (持久化)
  ↓
OLTP MemTable (热数据，< 5 分钟)
  ↓
OLTP SSTable (温数据，5 分钟 - 1 小时)
  ↓ [批量转换]
OLAP MemTable (Arrow2 Batch，1 小时内)
  ↓
OLAP SSTable (Parquet，> 1 小时，压缩存储)
```

## 分层存储策略

### L0: 热数据层 (OLTP)
- **时间范围**: 最近 5 分钟
- **存储**: OLTP MemTable (SkipMap)
- **查询延迟**: P99 < 10μs
- **写入延迟**: P99 < 100μs (含 WAL fsync)
- **用例**:
  - 实时订单查询
  - 实时成交查询
  - WebSocket 推送

### L1: 温数据层 (OLTP)
- **时间范围**: 5 分钟 - 1 小时
- **存储**: OLTP SSTable (rkyv)
- **查询延迟**: P99 < 1ms
- **用例**:
  - 近期历史订单
  - 成交记录查询
  - 对账核对

### L2: 分析层 (OLAP)
- **时间范围**: 1 小时 - 7 天
- **存储**: OLAP SSTable (Parquet)
- **查询延迟**: 秒级 (取决于扫描量)
- **用例**:
  - 日内分析
  - 统计报表
  - 风控分析

### L3: 冷数据层 (OLAP 压缩)
- **时间范围**: > 7 天
- **存储**: Parquet (高压缩率)
- **查询延迟**: 分钟级
- **用例**:
  - 历史回测
  - 合规审计
  - 长期分析

## 数据转换策略

### 1. 时间触发转换 (Time-based Compaction)

```rust
// OLTP → OLAP 转换规则
struct ConversionPolicy {
    // L0 → L1: MemTable 达到大小阈值或时间阈值
    memtable_flush_threshold: usize,      // 10 MB
    memtable_flush_interval: Duration,    // 5 分钟

    // L1 → L2: SSTable 老化转换
    sstable_age_threshold: Duration,      // 1 小时
    sstable_batch_size: usize,            // 一次转换 10-100 个 SSTable

    // L2 → L3: Parquet 压缩
    parquet_age_threshold: Duration,      // 7 天
    parquet_compression: CompressionCodec, // Zstd
}
```

### 2. 批量转换 (Batch Conversion)

**OLTP SSTable → OLAP Parquet**:
- 累积 N 个 OLTP SSTable
- 后台线程批量读取
- 转换为 Arrow2 RecordBatch
- 写入 Parquet (列式 + 压缩)
- 删除原 OLTP SSTable

**优点**:
- 减少 I/O 次数
- 提高压缩率 (批量压缩)
- 不阻塞实时写入

### 3. Settlement 时刻批量转换

```rust
// 每日结算后，将当天数据批量转换为 OLAP
// 优点：
//   1. 数据完整性好（交易日已结束）
//   2. 不影响日内交易性能
//   3. 可以添加聚合索引（最高价、最低价、成交量等）
async fn settlement_conversion(date: NaiveDate) {
    // 1. 读取当天所有 OLTP SSTable
    // 2. 按品种分组
    // 3. 转换为 Parquet（每个品种一个文件）
    // 4. 添加元数据（min/max price, volume 等）
    // 5. 删除旧 OLTP 文件
}
```

## 查询路由策略

### 智能路由器 (Query Router)

```rust
impl HybridQueryRouter {
    fn route_query(&self, query: &RangeQuery) -> QueryPlan {
        let now = current_timestamp();
        let age = now - query.start_ts;

        match age {
            // 热查询：只用 OLTP
            age if age < Duration::minutes(5) => {
                QueryPlan::OltpOnly {
                    sources: vec![self.oltp_memtable, self.oltp_sstables],
                    expected_latency: Microseconds(100),
                }
            }

            // 温查询：OLTP + OLAP 合并
            age if age < Duration::hours(1) => {
                QueryPlan::Hybrid {
                    oltp_sources: self.oltp_sstables_in_range(query),
                    olap_sources: self.olap_parquets_in_range(query),
                    merge_strategy: MergeStrategy::TimeOrdered,
                    expected_latency: Milliseconds(10),
                }
            }

            // 冷查询：只用 OLAP
            _ => {
                QueryPlan::OlapOnly {
                    sources: self.olap_parquets_in_range(query),
                    expected_latency: Seconds(1),
                }
            }
        }
    }
}
```

### 查询执行

```rust
// 并行查询 + 合并结果
async fn execute_hybrid_query(plan: QueryPlan) -> Vec<Record> {
    match plan {
        QueryPlan::OltpOnly { sources, .. } => {
            // 顺序扫描 OLTP 数据源
            sources.iter()
                .flat_map(|s| s.range_query(start, end))
                .collect()
        }

        QueryPlan::Hybrid { oltp_sources, olap_sources, .. } => {
            // 并行查询两侧
            let (oltp_results, olap_results) = tokio::join!(
                query_oltp_sources(oltp_sources),
                query_olap_sources(olap_sources),
            );

            // 归并排序（两侧都已排序）
            merge_sorted_results(oltp_results, olap_results)
        }

        QueryPlan::OlapOnly { sources, .. } => {
            // 使用 Polars 并行扫描 Parquet
            scan_parquet_parallel(sources)
                .filter(/* query predicates */)
                .collect()
        }
    }
}
```

## 资源隔离

### 1. 线程池隔离

```rust
struct HybridStorageRuntime {
    // OLTP 高优先级线程池
    oltp_pool: ThreadPool {
        threads: num_cpus(),
        priority: High,
    },

    // OLAP 后台线程池
    olap_pool: ThreadPool {
        threads: num_cpus() / 2,
        priority: Low,
    },

    // 转换专用线程池
    conversion_pool: ThreadPool {
        threads: 2,
        priority: BelowNormal,
    },
}
```

### 2. I/O 带宽控制

```rust
// 限制 OLAP 转换的 I/O 带宽，避免影响 OLTP
struct IoThrottler {
    max_read_bytes_per_sec: usize,  // 100 MB/s
    max_write_bytes_per_sec: usize, // 50 MB/s
}
```

### 3. 内存隔离

```rust
struct MemoryBudget {
    oltp_memtable_max: usize,      // 100 MB per instrument
    olap_batch_buffer_max: usize,  // 500 MB total
    conversion_buffer_max: usize,  // 200 MB
}
```

## Arrow2 设计

### Schema 定义

```rust
use arrow2::datatypes::*;

// 统一的 Record Schema (适用于 Order/Trade/AccountUpdate)
fn create_record_schema() -> Schema {
    Schema::from(vec![
        Field::new("timestamp", DataType::Int64, false),  // 纳秒时间戳
        Field::new("sequence", DataType::UInt64, false),  // WAL 序列号
        Field::new("record_type", DataType::UInt8, false), // 0=Order, 1=Trade, 2=Account

        // Order fields
        Field::new("order_id", DataType::UInt64, true),
        Field::new("user_id", DataType::Binary, true),
        Field::new("instrument_id", DataType::Binary, true),
        Field::new("direction", DataType::UInt8, true),
        Field::new("offset", DataType::UInt8, true),
        Field::new("price", DataType::Float64, true),
        Field::new("volume", DataType::Float64, true),

        // Trade fields
        Field::new("trade_id", DataType::UInt64, true),
        Field::new("exchange_order_id", DataType::UInt64, true),

        // Account fields
        Field::new("balance", DataType::Float64, true),
        Field::new("frozen", DataType::Float64, true),
    ])
}
```

### Batch 构建

```rust
// 批量构建 Arrow RecordBatch
fn build_record_batch(records: Vec<WalRecord>) -> RecordBatch {
    let mut timestamp_builder = PrimitiveArray::<i64>::builder(records.len());
    let mut order_id_builder = PrimitiveArray::<u64>::builder(records.len());
    // ... 其他字段 builder

    for record in records {
        match record {
            WalRecord::OrderInsert { timestamp, order_id, .. } => {
                timestamp_builder.push(Some(timestamp));
                order_id_builder.push(Some(order_id));
                // ... 填充其他字段
            }
            _ => { /* 处理其他类型 */ }
        }
    }

    RecordBatch::new(
        schema.clone(),
        vec![
            Arc::new(timestamp_builder.finish()),
            Arc::new(order_id_builder.finish()),
            // ...
        ],
    )
}
```

## Parquet 优化

### 1. 列式压缩

```rust
use parquet2::write::*;

let compression = CompressionOptions::Zstd {
    level: 3,           // 平衡压缩率和速度
    workers: 4,
};

let encoding = Encoding::DeltaBinaryPacked; // 时间戳增量编码
```

### 2. Row Group 大小

```rust
// 每个 Row Group 包含 10,000 条记录
// 优化：
//   - 太小：元数据开销大
//   - 太大：查询需要读取过多数据
const ROW_GROUP_SIZE: usize = 10_000;
```

### 3. 文件分区

```
storage/
  IF2501/
    oltp/
      wal/
      sstables/
    olap/
      parquet/
        date=20251003/
          hour=09/
            IF2501_09_00.parquet  # 09:00-09:59
            IF2501_09_01.parquet  # 每分钟一个文件
          hour=10/
            IF2501_10_00.parquet
```

**查询优化**：
- 按时间范围裁剪文件（partition pruning）
- 按 Row Group 统计过滤（min/max timestamp）

## 性能目标

| 操作 | OLTP | OLAP | 说明 |
|------|------|------|------|
| 写入延迟 (P99) | < 100μs | N/A | OLAP 不直接写入 |
| 点查询 (P99) | < 10μs | N/A | MemTable 查询 |
| 范围查询 (100 条) | < 1ms | < 10ms | 不同数据源 |
| 范围查询 (10K 条) | < 10ms | < 100ms | 批量扫描 |
| 聚合查询 (1 天) | N/A | < 1s | Parquet 列式扫描 |
| 转换吞吐量 | N/A | > 100K records/s | OLTP → OLAP |
| 存储压缩率 | 1x (无压缩) | 5-10x | Parquet + Zstd |

## 实现优先级

### Phase 2a: OLAP 基础 (本次实现)
1. ✅ OLTP 完整实现
2. ⏳ Arrow2 MemTable (批量构建)
3. ⏳ Parquet Writer (列式写入)
4. ⏳ Parquet Reader (范围查询)

### Phase 2b: Hybrid 路由 (下次实现)
5. ⏳ HybridQueryRouter (智能路由)
6. ⏳ OLTP → OLAP Converter (后台转换)
7. ⏳ 转换策略配置

### Phase 2c: 优化 (后续)
8. ⏳ 并行查询执行
9. ⏳ 查询结果缓存
10. ⏳ 自动化 Compaction

## 配置示例

```toml
[storage.hybrid]
# OLTP 配置
oltp_memtable_size = "10MB"
oltp_sstable_max_count = 100

# OLAP 配置
olap_batch_size = 10000
olap_compression = "zstd"
olap_compression_level = 3

# 转换策略
conversion_interval = "1h"        # 每小时转换一次
conversion_min_sstables = 10      # 至少累积 10 个 SSTable
conversion_time_threshold = "2h"  # 2 小时后的数据才转换

# 查询路由
hot_data_threshold = "5m"         # 5 分钟内的数据为热数据
warm_data_threshold = "1h"        # 1 小时内的数据为温数据
```

## 总结

这个设计实现了：
1. **性能优先**：OLTP 实时写入不受 OLAP 影响
2. **智能取舍**：根据数据热度自动路由
3. **批量优化**：OLAP 批量转换，提高吞吐
4. **资源隔离**：线程池、内存、I/O 分离
5. **可扩展性**：支持水平扩展（多品种并发）

**下一步**：实现 Arrow2 MemTable 和 Parquet SSTable
