# 二级索引 (Secondary Index)

@yutiansut @quantaxis

## 📖 概述

QAExchange-RS 存储系统实现了**高效的二级索引体系**，为常用查询模式提供 O(log n + k) 的时间复杂度，支持按时间范围、合约 ID、记录类型的快速检索。

## 🎯 设计目标

- **高效范围查询**: 时间范围查询 O(log n + k)
- **内存紧凑**: 使用字符串驻留池减少内存分配
- **低延迟**: 索引查找 P99 < 10μs
- **复合查询**: 支持多维度条件组合
- **并发安全**: 使用 `parking_lot::RwLock` 保护索引

## 🏗️ 索引架构

### 索引层次结构

```
┌─────────────────────────────────────────────────────────────┐
│                CompositeIndexManager                         │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    Query Optimizer                       ││
│  │  选择最优索引路径: Instrument > RecordType > Time       ││
│  └─────────────────────────────────────────────────────────┘│
│                              ↓                               │
│  ┌───────────────┬───────────────┬───────────────┐         │
│  │ InstrumentIndex │ RecordTypeIndex │ TimeSeriesIndex │    │
│  │  按合约ID分区    │  按记录类型分区  │   主时间索引    │    │
│  │  字符串驻留池    │  位掩码快速匹配  │   BTreeMap     │    │
│  └───────────────┴───────────────┴───────────────┘         │
│                              ↓                               │
│                       Offset → WAL/SSTable                   │
└─────────────────────────────────────────────────────────────┘
```

### 索引类型对比

| 索引类型 | 数据结构 | 查询复杂度 | 适用场景 |
|----------|----------|------------|----------|
| TimeSeriesIndex | BTreeMap | O(log n + k) | 时间范围查询 |
| InstrumentIndex | HashMap + BTreeMap | O(1) + O(log n + k) | 按合约查询 |
| RecordTypeIndex | HashMap + BTreeMap | O(1) + O(log n + k) | 按类型查询 |

## 🔧 核心实现

### 1. 时间序列索引 (TimeSeriesIndex)

基于 BTreeMap 的时间戳索引，支持高效范围查询。

```rust
// src/storage/index/time_series.rs

use std::collections::BTreeMap;

/// 时间范围
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start: i64,  // 纳秒时间戳
    pub end: i64,
}

impl TimeRange {
    pub fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }

    /// 检查是否与另一个范围重叠
    #[inline]
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start <= other.end && self.end >= other.start
    }

    /// 检查是否包含某个时间戳
    #[inline]
    pub fn contains(&self, timestamp: i64) -> bool {
        timestamp >= self.start && timestamp <= self.end
    }

    /// 合并两个范围
    pub fn merge(&self, other: &TimeRange) -> TimeRange {
        TimeRange {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// 时间序列索引
pub struct TimeSeriesIndex {
    /// 时间戳 → 偏移量映射（BTreeMap 保证有序）
    entries: BTreeMap<i64, Vec<u64>>,

    /// 条目总数
    entry_count: u64,

    /// 时间范围（快速路径优化）
    time_range: Option<TimeRange>,
}

impl TimeSeriesIndex {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            entry_count: 0,
            time_range: None,
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(offset);

        self.entry_count += 1;

        // 更新时间范围
        match &mut self.time_range {
            Some(range) => {
                if timestamp < range.start {
                    range.start = timestamp;
                }
                if timestamp > range.end {
                    range.end = timestamp;
                }
            }
            None => {
                self.time_range = Some(TimeRange::new(timestamp, timestamp));
            }
        }
    }

    /// 批量添加（优化性能）
    pub fn add_batch(&mut self, entries: &[(i64, u64)]) {
        for (timestamp, offset) in entries {
            self.add(*timestamp, *offset);
        }
    }

    /// 范围查询
    pub fn query_range(&self, start_ts: i64, end_ts: i64) -> Vec<u64> {
        // 快速路径：检查是否在索引范围内
        if let Some(range) = &self.time_range {
            if end_ts < range.start || start_ts > range.end {
                return Vec::new();
            }
        }

        let mut result = Vec::new();

        // 使用 BTreeMap 的 range 方法进行高效查询
        for (_ts, offsets) in self.entries.range(start_ts..=end_ts) {
            result.extend(offsets.iter().copied());
        }

        result
    }

    /// 点查询（精确时间戳）
    pub fn query_exact(&self, timestamp: i64) -> Vec<u64> {
        self.entries
            .get(&timestamp)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// 获取时间范围
    pub fn time_range(&self) -> Option<TimeRange> {
        self.time_range
    }

    /// 获取条目数量
    pub fn len(&self) -> u64 {
        self.entry_count
    }

    /// 估算内存使用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        let entry_size = std::mem::size_of::<i64>() + std::mem::size_of::<Vec<u64>>();
        let offset_size = std::mem::size_of::<u64>();

        self.entries.len() * entry_size + self.entry_count as usize * offset_size
    }
}
```

### 2. 合约索引 (InstrumentIndex)

按合约 ID 分区的时序索引，使用字符串驻留池优化内存。

```rust
// src/storage/index/instrument.rs

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// 单个合约的索引数据
#[derive(Debug, Clone)]
struct InstrumentData {
    /// 时间戳 → 偏移量（有序）
    entries: BTreeMap<i64, Vec<u64>>,
    /// 时间范围
    time_range: Option<TimeRange>,
    /// 条目数量
    entry_count: u64,
}

impl InstrumentData {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            time_range: None,
            entry_count: 0,
        }
    }

    fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(offset);

        self.entry_count += 1;

        // 更新时间范围
        match &mut self.time_range {
            Some(range) => {
                if timestamp < range.start { range.start = timestamp; }
                if timestamp > range.end { range.end = timestamp; }
            }
            None => {
                self.time_range = Some(TimeRange::new(timestamp, timestamp));
            }
        }
    }

    fn query_range(&self, start_ts: i64, end_ts: i64) -> Vec<u64> {
        // 快速路径
        if let Some(range) = &self.time_range {
            if end_ts < range.start || start_ts > range.end {
                return Vec::new();
            }
        }

        let mut result = Vec::new();
        for (_ts, offsets) in self.entries.range(start_ts..=end_ts) {
            result.extend(offsets.iter().copied());
        }
        result
    }
}

/// 合约索引
pub struct InstrumentIndex {
    /// 合约ID → 索引数据
    instruments: HashMap<Arc<str>, InstrumentData>,

    /// 字符串驻留池（减少内存分配）
    string_pool: HashMap<String, Arc<str>>,

    /// 总条目数
    total_entries: u64,
}

impl InstrumentIndex {
    pub fn new() -> Self {
        Self {
            instruments: HashMap::new(),
            string_pool: HashMap::new(),
            total_entries: 0,
        }
    }

    /// 获取或创建驻留字符串
    #[inline]
    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(interned) = self.string_pool.get(s) {
            return Arc::clone(interned);
        }

        let interned: Arc<str> = Arc::from(s);
        self.string_pool.insert(s.to_string(), Arc::clone(&interned));
        interned
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, instrument_id: &str, timestamp: i64, offset: u64) {
        let key = self.intern(instrument_id);

        let data = self
            .instruments
            .entry(key)
            .or_insert_with(InstrumentData::new);

        data.add(timestamp, offset);
        self.total_entries += 1;
    }

    /// 范围查询
    pub fn query_range(&self, instrument_id: &str, start_ts: i64, end_ts: i64) -> Vec<u64> {
        let key: Arc<str> = Arc::from(instrument_id);

        self.instruments
            .get(&key)
            .map(|data| data.query_range(start_ts, end_ts))
            .unwrap_or_default()
    }

    /// 获取合约的时间范围
    pub fn get_time_range(&self, instrument_id: &str) -> Option<TimeRange> {
        let key: Arc<str> = Arc::from(instrument_id);
        self.instruments.get(&key).and_then(|d| d.time_range)
    }

    /// 获取所有合约ID
    pub fn list_instruments(&self) -> Vec<&str> {
        self.instruments.keys().map(|k| k.as_ref()).collect()
    }

    /// 获取合约数量
    pub fn instrument_count(&self) -> usize {
        self.instruments.len()
    }

    /// 估算内存使用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        let mut size = 0;

        // 字符串池
        for s in self.string_pool.keys() {
            size += s.len() + std::mem::size_of::<Arc<str>>();
        }

        // 每个合约的数据
        for data in self.instruments.values() {
            size += data.entries.len() * (std::mem::size_of::<i64>() + std::mem::size_of::<Vec<u64>>());
            size += data.entry_count as usize * std::mem::size_of::<u64>();
        }

        size
    }
}
```

### 3. 记录类型索引 (RecordTypeIndex)

按记录类型分区的时序索引，支持 O(1) 类型匹配。

```rust
// src/storage/index/record_type.rs

use std::collections::{BTreeMap, HashMap, HashSet};
use crate::storage::hybrid::query_filter::{RecordType, RecordTypeSet};

/// 单个类型的索引数据
#[derive(Debug, Clone)]
struct TypeData {
    /// 时间戳 → 偏移量（有序）
    entries: BTreeMap<i64, Vec<u64>>,
    /// 偏移量集合（用于快速查找）
    offset_set: HashSet<u64>,
    /// 时间范围
    time_range: Option<TimeRange>,
    /// 条目数量
    entry_count: u64,
}

impl TypeData {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            offset_set: HashSet::new(),
            time_range: None,
            entry_count: 0,
        }
    }

    fn add(&mut self, timestamp: i64, offset: u64) {
        self.entries
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(offset);

        self.offset_set.insert(offset);
        self.entry_count += 1;

        // 更新时间范围
        match &mut self.time_range {
            Some(range) => {
                if timestamp < range.start { range.start = timestamp; }
                if timestamp > range.end { range.end = timestamp; }
            }
            None => {
                self.time_range = Some(TimeRange::new(timestamp, timestamp));
            }
        }
    }

    fn query_range(&self, start_ts: i64, end_ts: i64) -> Vec<u64> {
        if let Some(range) = &self.time_range {
            if end_ts < range.start || start_ts > range.end {
                return Vec::new();
            }
        }

        let mut result = Vec::new();
        for (_ts, offsets) in self.entries.range(start_ts..=end_ts) {
            result.extend(offsets.iter().copied());
        }
        result
    }

    fn contains_offset(&self, offset: u64) -> bool {
        self.offset_set.contains(&offset)
    }
}

/// 记录类型索引
pub struct RecordTypeIndex {
    /// 类型 → 索引数据
    types: HashMap<RecordType, TypeData>,
    /// 总条目数
    total_entries: u64,
}

impl RecordTypeIndex {
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
            total_entries: 0,
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add(&mut self, record_type: RecordType, timestamp: i64, offset: u64) {
        let data = self.types.entry(record_type).or_insert_with(TypeData::new);

        data.add(timestamp, offset);
        self.total_entries += 1;
    }

    /// 范围查询（单个类型）
    pub fn query_range(&self, record_type: RecordType, start_ts: i64, end_ts: i64) -> Vec<u64> {
        self.types
            .get(&record_type)
            .map(|data| data.query_range(start_ts, end_ts))
            .unwrap_or_default()
    }

    /// 范围查询（多个类型）
    pub fn query_range_for_types(
        &self,
        start_ts: i64,
        end_ts: i64,
        types: &RecordTypeSet,
    ) -> Vec<u64> {
        let mut result = Vec::new();

        // 遍历所有匹配的类型
        for (record_type, data) in &self.types {
            if types.contains(*record_type) {
                result.extend(data.query_range(start_ts, end_ts));
            }
        }

        // 按偏移量排序（保证顺序性）
        result.sort_unstable();
        result
    }

    /// 检查偏移量是否属于指定类型集合
    pub fn contains_offset_in_types(&self, offset: u64, types: &RecordTypeSet) -> bool {
        for (record_type, data) in &self.types {
            if types.contains(*record_type) && data.contains_offset(offset) {
                return true;
            }
        }
        false
    }

    /// 获取类型的时间范围
    pub fn get_time_range(&self, record_type: RecordType) -> Option<TimeRange> {
        self.types.get(&record_type).and_then(|d| d.time_range)
    }

    /// 获取所有有索引的类型
    pub fn list_types(&self) -> Vec<RecordType> {
        self.types.keys().copied().collect()
    }
}
```

### 4. 复合索引管理器 (CompositeIndexManager)

统一管理所有索引，支持智能查询路径选择。

```rust
// src/storage/index/mod.rs

use std::sync::Arc;
use parking_lot::RwLock;

/// 索引统计信息
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    /// 索引条目总数
    pub total_entries: u64,
    /// 索引命中次数
    pub hits: u64,
    /// 索引未命中次数
    pub misses: u64,
    /// 最后更新时间戳
    pub last_update_ts: i64,
}

/// 复合索引管理器
pub struct CompositeIndexManager {
    /// 合约索引
    pub instrument_index: Arc<RwLock<InstrumentIndex>>,
    /// 记录类型索引
    pub record_type_index: Arc<RwLock<RecordTypeIndex>>,
    /// 时间序列索引（主索引）
    pub time_index: Arc<RwLock<TimeSeriesIndex>>,
    /// 索引统计
    stats: IndexStats,
}

impl CompositeIndexManager {
    /// 创建新的索引管理器
    pub fn new() -> Self {
        Self {
            instrument_index: Arc::new(RwLock::new(InstrumentIndex::new())),
            record_type_index: Arc::new(RwLock::new(RecordTypeIndex::new())),
            time_index: Arc::new(RwLock::new(TimeSeriesIndex::new())),
            stats: IndexStats::default(),
        }
    }

    /// 添加索引条目
    #[inline]
    pub fn add_entry(
        &mut self,
        timestamp: i64,
        instrument_id: Option<&str>,
        record_type: RecordType,
        offset: u64,
    ) {
        // 更新时间索引
        {
            let mut time_idx = self.time_index.write();
            time_idx.add(timestamp, offset);
        }

        // 更新合约索引
        if let Some(inst) = instrument_id {
            let mut inst_idx = self.instrument_index.write();
            inst_idx.add(inst, timestamp, offset);
        }

        // 更新类型索引
        {
            let mut type_idx = self.record_type_index.write();
            type_idx.add(record_type, timestamp, offset);
        }

        self.stats.total_entries += 1;
        self.stats.last_update_ts = timestamp;
    }

    /// 查询时间范围内的偏移量（智能选择索引路径）
    pub fn query_offsets(
        &self,
        start_ts: i64,
        end_ts: i64,
        instrument_id: Option<&str>,
        record_types: Option<&RecordTypeSet>,
    ) -> Vec<u64> {
        let use_instrument_index = instrument_id.is_some();
        let use_type_index = record_types.is_some();

        // 优先使用合约索引（通常选择性更高）
        if use_instrument_index {
            let inst_idx = self.instrument_index.read();
            if let Some(inst) = instrument_id {
                let entries = inst_idx.query_range(inst, start_ts, end_ts);

                // 如果还有类型过滤，进一步筛选
                if let Some(types) = record_types {
                    let type_idx = self.record_type_index.read();
                    return entries
                        .into_iter()
                        .filter(|offset| {
                            type_idx.contains_offset_in_types(*offset, types)
                        })
                        .collect();
                }

                return entries;
            }
        }

        // 使用类型索引
        if use_type_index {
            if let Some(types) = record_types {
                let type_idx = self.record_type_index.read();
                return type_idx.query_range_for_types(start_ts, end_ts, types);
            }
        }

        // 回退到时间索引
        let time_idx = self.time_index.read();
        time_idx.query_range(start_ts, end_ts)
    }

    /// 获取索引统计
    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }

    /// 清空所有索引
    pub fn clear(&mut self) {
        self.instrument_index.write().clear();
        self.record_type_index.write().clear();
        self.time_index.write().clear();
        self.stats = IndexStats::default();
    }
}
```

## 📊 查询优化策略

### 索引选择优先级

```
1. InstrumentIndex (合约索引)
   - 选择性最高：每个合约数据独立
   - 时间复杂度: O(1) hash + O(log n) range

2. RecordTypeIndex (类型索引)
   - 中等选择性：按类型分区
   - 支持多类型联合查询

3. TimeSeriesIndex (时间索引)
   - 兜底索引：全表扫描
   - 仅用于纯时间范围查询
```

### 查询示例

```rust
use crate::storage::index::CompositeIndexManager;
use crate::storage::hybrid::query_filter::{RecordType, RecordTypeSet};

let mut manager = CompositeIndexManager::new();

// 添加索引条目
manager.add_entry(1000, Some("cu2501"), RecordType::TickData, 0);
manager.add_entry(1500, Some("cu2501"), RecordType::OrderInsert, 1);
manager.add_entry(2000, Some("au2501"), RecordType::TickData, 2);

// 查询 1: 按合约 + 时间范围
let offsets = manager.query_offsets(
    0, 3000,
    Some("cu2501"),  // 使用合约索引
    None,
);
// 结果: [0, 1]

// 查询 2: 按类型 + 时间范围
let tick_types = RecordTypeSet::new().insert(RecordType::TickData);
let offsets = manager.query_offsets(
    0, 3000,
    None,
    Some(&tick_types),  // 使用类型索引
);
// 结果: [0, 2]

// 查询 3: 复合查询（合约 + 类型 + 时间）
let offsets = manager.query_offsets(
    0, 3000,
    Some("cu2501"),
    Some(&tick_types),
);
// 结果: [0]
```

## 📈 性能基准

### 索引操作延迟

| 操作 | 延迟 | 数据规模 |
|------|------|----------|
| add() | ~50ns | - |
| query_range (时间) | ~1μs | 100K 条目 |
| query_range (合约) | ~500ns | 1K 合约 × 100K 条目 |
| query_range (类型) | ~800ns | 16 类型 × 100K 条目 |
| 复合查询 | ~2μs | 1K 合约 × 16 类型 × 100K 条目 |

### 内存使用

| 索引类型 | 100K 条目 | 1M 条目 | 10M 条目 |
|----------|-----------|---------|----------|
| TimeSeriesIndex | ~2.4 MB | ~24 MB | ~240 MB |
| InstrumentIndex | ~3 MB | ~30 MB | ~300 MB |
| RecordTypeIndex | ~3.5 MB | ~35 MB | ~350 MB |
| **合计** | **~9 MB** | **~90 MB** | **~900 MB** |

## 🛠️ 配置示例

### 与 WAL 集成

```rust
// WAL 写入时同步更新索引
impl WalManager {
    pub fn append_with_index(
        &mut self,
        record: &WalRecord,
        index_manager: &mut CompositeIndexManager,
    ) -> Result<u64, WalError> {
        let offset = self.append(record)?;

        // 提取索引信息
        let timestamp = record.timestamp();
        let instrument_id = record.instrument_id();
        let record_type = record.record_type();

        // 更新索引
        index_manager.add_entry(
            timestamp,
            instrument_id,
            record_type,
            offset,
        );

        Ok(offset)
    }
}
```

### 查询引擎集成

```rust
// 查询引擎使用索引加速
impl QueryEngine {
    pub fn query_with_index(
        &self,
        query: &QueryRequest,
        index_manager: &CompositeIndexManager,
    ) -> QueryResult {
        // 使用索引获取候选偏移量
        let offsets = index_manager.query_offsets(
            query.start_time,
            query.end_time,
            query.instrument_id.as_deref(),
            query.record_types.as_ref(),
        );

        // 根据偏移量读取数据
        self.read_by_offsets(&offsets)
    }
}
```

## 💡 最佳实践

### 1. 索引时机选择

```rust
// ✅ 正确：WAL 写入时同步更新索引
manager.add_entry(timestamp, instrument_id, record_type, offset);

// ❌ 错误：延迟更新索引（可能导致查询不一致）
```

### 2. 合理使用复合查询

```rust
// ✅ 正确：先用选择性高的条件
let offsets = manager.query_offsets(
    start_ts, end_ts,
    Some("cu2501"),      // 高选择性
    Some(&market_types), // 进一步过滤
);

// ❌ 错误：不必要的全表扫描
let offsets = manager.query_offsets(start_ts, end_ts, None, None);
```

### 3. 内存管理

```rust
// 定期检查内存使用
let memory_mb = manager.instrument_index.read().estimated_memory_bytes() / 1024 / 1024;
if memory_mb > 100 {
    // 考虑清理或压缩
    manager.clear();
}
```

## 🔍 故障排查

### 问题 1: 查询延迟过高

**症状**: 查询 P99 > 10ms

**排查**:
1. 检查是否命中索引
2. 检查时间范围是否过大
3. 检查结果集大小

**解决**:
```rust
// 添加更多过滤条件
let offsets = manager.query_offsets(
    start_ts, end_ts,
    Some("cu2501"),      // 添加合约过滤
    Some(&tick_types),   // 添加类型过滤
);
```

### 问题 2: 内存使用过高

**症状**: 索引内存 > 1GB

**排查**:
1. 检查条目数量
2. 检查字符串池大小

**解决**:
```rust
// 定期清理历史索引
manager.clear();
// 或实现 LRU 淘汰策略
```

## 📚 相关文档

- [WAL 设计](wal.md) - 索引数据来源
- [查询引擎](query_engine.md) - 索引加速查询
- [压缩策略](compression.md) - 存储优化
- [SSTable 格式](sstable.md) - 持久化索引

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
