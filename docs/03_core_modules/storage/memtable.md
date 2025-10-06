# MemTable 实现

## 📖 概述

MemTable 是存储系统中的内存数据结构，提供高速写入和查询能力。QAExchange-RS 实现了 **OLTP** 和 **OLAP** 双体系 MemTable。

## 🎯 设计目标

- **OLTP (事务处理)**: 低延迟随机读写 (P99 < 10μs)
- **OLAP (分析查询)**: 高效列式存储和批量扫描
- **无锁设计**: 并发访问无阻塞
- **内存可控**: 达到阈值自动 flush 到 SSTable

## 🏗️ 双体系架构

### 1. OLTP MemTable (SkipMap)

基于 `crossbeam-skiplist` 的无锁跳表实现。

```rust
// src/storage/memtable/oltp.rs
use crossbeam_skiplist::SkipMap;

pub struct OltpMemTable {
    /// 无锁跳表
    map: Arc<SkipMap<Vec<u8>, Vec<u8>>>,

    /// 当前大小 (bytes)
    size_bytes: AtomicU64,

    /// 大小阈值
    max_size_bytes: u64,
}

impl OltpMemTable {
    /// 写入键值对
    pub fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let entry_size = key.len() + value.len() + 32; // 32 bytes overhead
        self.map.insert(key, value);
        self.size_bytes.fetch_add(entry_size as u64, Ordering::Relaxed);
        Ok(())
    }

    /// 查询键
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.map.get(key).map(|entry| entry.value().clone())
    }

    /// 范围扫描
    pub fn scan(&self, start: &[u8], end: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.map
            .range(start..end)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// 检查是否需要 flush
    pub fn should_flush(&self) -> bool {
        self.size_bytes.load(Ordering::Relaxed) >= self.max_size_bytes
    }
}
```

**性能特性**:
- 写入延迟: **P99 ~2.6μs**
- 读取延迟: P99 < 5μs
- 并发: 完全无锁,支持高并发
- 内存: O(log n) 平均深度

### 2. OLAP MemTable (Arrow2)

基于 Apache Arrow2 的列式存储实现。

```rust
// src/storage/memtable/olap.rs
use arrow2::array::*;
use arrow2::datatypes::*;
use arrow2::chunk::Chunk;

pub struct OlapMemTable {
    /// Arrow Schema
    schema: Schema,

    /// 列数据
    columns: Vec<Box<dyn Array>>,

    /// 行数
    row_count: usize,

    /// 容量
    capacity: usize,
}

impl OlapMemTable {
    /// 批量插入
    pub fn insert_batch(&mut self, batch: RecordBatch) -> Result<()> {
        // 追加列数据
        for (i, column) in batch.columns().iter().enumerate() {
            self.columns[i] = concatenate(&[&self.columns[i], column])?;
        }

        self.row_count += batch.num_rows();
        Ok(())
    }

    /// 列式查询
    pub fn select_columns(&self, column_names: &[&str]) -> Result<Chunk<Box<dyn Array>>> {
        let mut arrays = Vec::new();

        for name in column_names {
            let idx = self.schema.index_of(name)?;
            arrays.push(self.columns[idx].clone());
        }

        Ok(Chunk::new(arrays))
    }

    /// 过滤查询
    pub fn filter(&self, predicate: &BooleanArray) -> Result<Chunk<Box<dyn Array>>> {
        let filtered_columns: Vec<_> = self
            .columns
            .iter()
            .map(|col| filter(col.as_ref(), predicate))
            .collect::<Result<_, _>>()?;

        Ok(Chunk::new(filtered_columns))
    }
}
```

**性能特性**:
- 批量写入: > 1M rows/sec
- 列式扫描: > 10M rows/sec
- 压缩率: 高 (列式存储天然优势)
- 内存: 紧凑的列式布局

## 📊 数据流

### OLTP 路径 (低延迟)

```
WAL Record
    ↓
  rkyv 序列化
    ↓
OLTP MemTable (SkipMap)
    ↓ (达到阈值)
Flush to OLTP SSTable (rkyv 格式)
```

**使用场景**:
- 订单插入/更新
- 账户余额更新
- 成交记录写入
- 实时状态查询

### OLAP 路径 (高吞吐)

```
OLTP SSTable (多个文件)
    ↓
批量读取 + 反序列化
    ↓
转换为 Arrow RecordBatch
    ↓
OLAP MemTable (Arrow2)
    ↓ (达到阈值)
Flush to OLAP SSTable (Parquet 格式)
```

**使用场景**:
- 历史数据分析
- 批量数据导出
- 聚合统计查询
- BI 报表生成

## 🔄 Flush 机制

### 触发条件

```rust
pub struct FlushTrigger {
    /// 大小阈值 (bytes)
    size_threshold: u64,

    /// 时间阈值 (seconds)
    time_threshold: u64,

    /// 上次 flush 时间
    last_flush: Instant,
}

impl FlushTrigger {
    /// 检查是否需要 flush
    pub fn should_flush(&self, memtable: &OltpMemTable) -> bool {
        // 条件1: 大小超限
        if memtable.size_bytes() >= self.size_threshold {
            return true;
        }

        // 条件2: 时间超限
        if self.last_flush.elapsed().as_secs() >= self.time_threshold {
            return true;
        }

        false
    }
}
```

**默认配置**:
- OLTP: 256 MB 或 60 秒
- OLAP: 1 GB 或 300 秒

### Flush 流程

```rust
impl HybridStorage {
    /// Flush OLTP MemTable
    async fn flush_oltp(&mut self) -> Result<()> {
        // 1. 冻结当前 MemTable
        let frozen = std::mem::replace(&mut self.active_memtable, OltpMemTable::new());

        // 2. 创建 SSTable 写入器
        let sst_path = self.generate_sst_path();
        let mut writer = SstableWriter::new(sst_path)?;

        // 3. 遍历并写入
        for entry in frozen.iter() {
            writer.write(entry.key(), entry.value())?;
        }

        // 4. 完成写入
        writer.finish()?;

        // 5. 注册新 SSTable
        self.sst_manager.register(sst_path)?;

        Ok(())
    }
}
```

## 💡 优化技巧

### 1. 批量写入

```rust
// ❌ 不推荐: 逐条插入
for record in records {
    memtable.insert(record.key(), record.value())?;
}

// ✅ 推荐: 批量插入
let batch: Vec<_> = records.iter()
    .map(|r| (r.key(), r.value()))
    .collect();
memtable.insert_batch(batch)?;
```

### 2. 预分配容量

```rust
// 创建时指定容量
let memtable = OltpMemTable::with_capacity(256 * 1024 * 1024); // 256 MB
```

### 3. 读写分离

```rust
// 使用 Arc 实现多读单写
let memtable = Arc::new(RwLock::new(OltpMemTable::new()));

// 读操作 (并发)
{
    let reader = memtable.read();
    let value = reader.get(&key);
}

// 写操作 (独占)
{
    let mut writer = memtable.write();
    writer.insert(key, value)?;
}
```

## 📊 内存管理

### 大小估算

```rust
impl OltpMemTable {
    /// 估算条目大小
    fn estimate_entry_size(&self, key: &[u8], value: &[u8]) -> usize {
        // Key + Value + Overhead
        key.len() + value.len() +
        32 +  // SkipMap node overhead
        16    // Arc/RefCount overhead
    }

    /// 当前内存占用
    pub fn memory_usage(&self) -> usize {
        self.size_bytes.load(Ordering::Relaxed) as usize
    }
}
```

### 内存回收

```rust
impl HybridStorage {
    /// 主动触发 GC
    pub fn gc(&mut self) -> Result<()> {
        // 1. Flush 所有 MemTable
        self.flush_all()?;

        // 2. Drop 冻结的 MemTable
        self.frozen_memtables.clear();

        // 3. Compact SSTable
        self.compaction_trigger()?;

        Ok(())
    }
}
```

## 🛠️ 配置示例

```toml
# config/storage.toml
[memtable.oltp]
max_size_mb = 256
flush_interval_sec = 60
estimated_entry_size = 256

[memtable.olap]
max_size_mb = 1024
flush_interval_sec = 300
batch_size = 10000
```

## 📈 性能基准

| 操作 | OLTP (SkipMap) | OLAP (Arrow2) |
|------|----------------|---------------|
| 写入延迟 (P99) | 2.6 μs | - |
| 批量写入 | 100K/s | 1M/s |
| 读取延迟 (P99) | 5 μs | - |
| 范围扫描 | 1M/s | 10M/s |
| 内存占用 | 中 | 低 (压缩) |

## 📚 相关文档

- [WAL 设计](wal.md) - MemTable 数据来源
- [SSTable 格式](sstable.md) - MemTable 持久化目标
- [查询引擎](query_engine.md) - 如何查询 MemTable
- [存储架构](../../02_architecture/decoupled_storage.md) - 完整数据流

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
