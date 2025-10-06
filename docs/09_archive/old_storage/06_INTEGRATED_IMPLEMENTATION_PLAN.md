# 集成实施计划：存储 + 分发 + Arrow2 查询引擎

> 8 阶段完整实施路线图（集成 Arrow2 查询引擎）

**版本**: v2.0.0
**最后更新**: 2025-10-03
**总时间**: 12 周

---

## 📋 目录

- [实施概览](#实施概览)
- [详细阶段规划](#详细阶段规划)
- [集成点和依赖](#集成点和依赖)
- [测试策略](#测试策略)
- [文档和 Changelog](#文档和-changelog)

---

## 实施概览

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                      qaexchange-rs                           │
│                                                              │
│  ┌────────────────────────────────────────────────────┐    │
│  │  OLTP 引擎 (Phase 1-3)                             │    │
│  │  WAL → MemTable (SkipMap) → SSTable (rkyv)         │    │
│  │  写入延迟: P99 < 10μs                              │    │
│  └────────────────────────────────────────────────────┘    │
│                            ↓                                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │  OLAP 引擎 (Phase 2-8 集成)                        │    │
│  │  Arrow2 MemTable → Parquet SSTable                  │    │
│  │  Polars SQL 查询引擎                               │    │
│  │  查询吞吐: > 100M rows/s                           │    │
│  └────────────────────────────────────────────────────┘    │
│                            ↓                                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │  分发系统 (Phase 4)                                 │    │
│  │  iceoryx2 零拷贝 + rkyv 序列化                     │    │
│  │  分发延迟: P99 < 10μs                              │    │
│  └────────────────────────────────────────────────────┘    │
│                            ↓                                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │  可靠性保障 (Phase 5-6)                            │    │
│  │  WAL 恢复 + Snapshot + 主从复制                    │    │
│  │  恢复时间: < 10s, RTO: < 30s                       │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### 时间线

```
Week 1      : Phase 1 - WAL 实现 ✅
Week 2-3    : Phase 2 - MemTable + SSTable + Arrow2 集成
Week 4      : Phase 3 - Compaction
Week 5-6    : Phase 4 - 零拷贝分发
Week 7      : Phase 5 - 故障恢复
Week 8-9    : Phase 6 - 主从复制
Week 10     : Phase 7 - 性能优化
Week 11-12  : Phase 8 - Arrow2 查询引擎完善

总计: 12 周
```

---

## 详细阶段规划

### Phase 1: WAL 实现 (Week 1) ✅

**目标**: 实现高性能 Write-Ahead Log

**已完成**:
- ✅ WalRecord 数据结构（rkyv 序列化）
- ✅ WalEntry 结构（sequence, crc32, timestamp）
- ✅ WalManager 核心功能
- ✅ WAL 文件格式（Header + Entry）
- ✅ 回放机制
- ✅ Checkpoint 截断
- ✅ CRC32 校验
- ✅ 单元测试框架

**待完成**:
- [ ] 修复编译错误（io::Error 转换、tempfile 依赖）
- [ ] 性能验证（P99 < 1ms）
- [ ] 更新 CHANGELOG v0.2.0

**交付物**:
- `src/storage/wal/` - 完整 WAL 实现
- `tests/wal_tests.rs` - 单元测试
- 性能报告

---

### Phase 2: MemTable + SSTable + Arrow2 (Week 2-3)

**目标**: 实现双模式存储（OLTP + OLAP）

#### Week 2: MemTable

**OLTP MemTable**:
- [ ] 基于 crossbeam-skiplist 的 SkipMap
- [ ] `MemTable::insert()` - O(log N)
- [ ] `MemTable::get()` - O(log N)
- [ ] 大小限制（128MB）
- [ ] Active/Immutable 切换

**OLAP MemTable** (新增):
- [ ] `ArrowMemTable` 基于 Arrow2 RecordBatch
- [ ] Schema 定义（Order/Trade/Account）
- [ ] 批量插入
- [ ] Polars DataFrame 转换

**异步转换器**:
- [ ] `OltpToOlapConverter` - 后台线程转换
- [ ] rkyv → Arrow2 RecordBatch
- [ ] 批量转换优化

#### Week 3: SSTable

**rkyv SSTable**:
- [ ] `SSTableBuilder` - 构建器
- [ ] `SSTableReader` - mmap 读取
- [ ] Bloom Filter（1% 误判率）
- [ ] 稀疏索引（每 4KB）

**Parquet SSTable** (新增):
- [ ] `ParquetSSTableBuilder` - Arrow2 写入
- [ ] Snappy 压缩
- [ ] `ParquetSSTableReader` - 零拷贝读取
- [ ] 谓词下推优化

**混合存储**:
- [ ] `HybridStorage` - 统一接口
- [ ] OLTP 路径（rkyv）
- [ ] OLAP 路径（Parquet）
- [ ] 自动选择策略

**测试**:
- [ ] MemTable 单元测试
- [ ] SSTable 单元测试
- [ ] 性能基准测试（插入/查询）
- [ ] 压缩比测试

**交付物**:
- `src/storage/memtable/` - MemTable 实现
- `src/storage/sstable/` - SSTable 实现
- `src/storage/arrow/` - Arrow2 集成
- `src/storage/hybrid_storage.rs` - 混合存储
- 性能报告

---

### Phase 3: Compaction (Week 4)

**目标**: 实现 LSM-Tree Compaction

**任务清单**:
- [ ] Leveled Compaction（7 层）
- [ ] 触发条件（L0: 文件数 ≥ 4）
- [ ] K-way 归并排序
- [ ] 后台线程执行
- [ ] Manifest 文件（记录 SSTable 变更）
- [ ] 版本管理（MVCC）

**Arrow2 集成**:
- [ ] Parquet Compaction（列式合并）
- [ ] Schema 演化支持
- [ ] 统计信息更新

**测试**:
- [ ] Compaction 功能测试
- [ ] CPU 占用测试（< 10%）
- [ ] 空间放大测试（< 1.5x）

**交付物**:
- `src/storage/compaction/` - Compaction 实现
- 性能报告

---

### Phase 4: 零拷贝分发 (Week 5-6)

**目标**: 基于 iceoryx2 + rkyv 的实时数据分发

#### Week 5: Publisher + Subscriber

**任务清单**:
- [ ] `DistributionMessage` 枚举（rkyv 序列化）
- [ ] `DistributionPublisher` - iceoryx2 集成
- [ ] `DistributionSubscriber` - 零拷贝接收
- [ ] 心跳机制
- [ ] 批量发布优化

#### Week 6: 多级订阅 + Arrow2 集成

**任务清单**:
- [ ] Real-time 订阅（WebSocket 推送）
- [ ] Delayed 订阅（批量聚合）
- [ ] Historical 订阅（WAL Replay）

**Arrow2 集成** (新增):
- [ ] Arrow2 消息格式（列式批量传输）
- [ ] Arrow Flight 协议（可选）
- [ ] Parquet 流式传输

**测试**:
- [ ] 分发延迟测试（P99 < 10μs）
- [ ] 吞吐量测试（> 10M msg/s）
- [ ] 可靠性测试（ACK 确认）

**交付物**:
- `src/distribution/` - 分发系统
- 性能报告

---

### Phase 5: 故障恢复 (Week 7)

**目标**: 快速恢复和 Snapshot

**任务清单**:
- [ ] `WalRecovery` - WAL 回放
- [ ] `SnapshotManager` - 快照管理
- [ ] Checkpoint 加载
- [ ] 自动快照（每 30min）
- [ ] 断点续传

**Arrow2 集成**:
- [ ] Parquet Snapshot（列式快照）
- [ ] 增量快照（Delta）
- [ ] Schema 版本管理

**测试**:
- [ ] 恢复时间测试（< 10s）
- [ ] 数据一致性测试（100%）
- [ ] 崩溃恢复测试

**交付物**:
- `src/storage/recovery/` - 恢复机制
- 性能报告

---

### Phase 6: 主从复制 (Week 8-9)

**目标**: 主从复制和自动故障转移

#### Week 8: 复制机制

**任务清单**:
- [ ] `ReplicationMaster` - WAL 复制
- [ ] `ReplicationSlave` - WAL 接收
- [ ] 同步/异步复制
- [ ] ACK 处理

**Arrow2 集成**:
- [ ] Parquet 增量复制（只复制变更的列）
- [ ] 列式传输优化

#### Week 9: 故障转移

**任务清单**:
- [ ] `FailureDetector` - 心跳检测
- [ ] `FailoverCoordinator` - 自动切换
- [ ] 选举新 Master
- [ ] 重新配置 Slave

**测试**:
- [ ] 复制延迟测试（< 100ms）
- [ ] 故障检测测试（< 5s）
- [ ] 故障转移测试（< 30s）
- [ ] 数据一致性测试

**交付物**:
- `src/storage/replication/` - 复制系统
- `src/storage/failover/` - 故障转移
- 性能报告

---

### Phase 7: 性能优化 (Week 10)

**目标**: 性能调优和压力测试

**任务清单**:
- [ ] WAL Group Commit
- [ ] 预分配空间（fallocate）
- [ ] Direct I/O（可选）
- [ ] MemTable 并发优化
- [ ] SSTable mmap 预读
- [ ] 缓存热点 Index
- [ ] CPU 亲和性
- [ ] 批量处理优化

**Arrow2 优化**:
- [ ] SIMD 向量化（自动）
- [ ] 谓词下推
- [ ] 列裁剪
- [ ] 并行查询

**压力测试**:
- [ ] 百万级订单写入
- [ ] 千万级消息分发
- [ ] 亿级查询扫描
- [ ] 崩溃恢复测试
- [ ] 故障转移测试

**交付物**:
- 性能报告
- 优化建议
- 生产就绪检查清单

---

### Phase 8: Arrow2 查询引擎 (Week 11-12)

**目标**: 完整的 SQL 查询引擎

#### Week 11: 查询引擎核心

**任务清单**:
- [ ] `QueryEngine` - Polars SQL 集成
- [ ] 表注册（orders/trades/accounts）
- [ ] 预定义查询（用户统计、行情分析、盈亏计算）
- [ ] 查询优化器
- [ ] 执行计划分析

#### Week 12: 流式查询 + 优化

**任务清单**:
- [ ] `StreamingQueryEngine` - 实时查询
- [ ] 增量聚合
- [ ] 物化视图
- [ ] 查询缓存

**高级功能**:
- [ ] JOIN 优化（Hash Join）
- [ ] 窗口函数
- [ ] UDF（用户自定义函数）
- [ ] 多表关联

**测试**:
- [ ] 查询延迟测试（点查询 < 1ms）
- [ ] 扫描吞吐测试（> 1GB/s）
- [ ] 聚合性能测试（> 100M rows/s）
- [ ] JOIN 性能测试（> 10M rows/s）
- [ ] 并发查询测试

**交付物**:
- `src/storage/query/` - 查询引擎
- SQL 用户手册
- 性能报告

---

## 集成点和依赖

### 模块依赖图

```
Phase 1 (WAL)
    ↓
Phase 2 (MemTable + SSTable)
    ↓ ← Phase 2 (Arrow2 MemTable + Parquet)
Phase 3 (Compaction)
    ↓
Phase 4 (分发) + Phase 5 (恢复) [并行]
    ↓
Phase 6 (复制)
    ↓
Phase 7 (优化)
    ↓
Phase 8 (查询引擎)
```

### 关键集成点

| 阶段 | 集成内容 | 依赖模块 |
|------|---------|---------|
| **Phase 2** | Arrow2 MemTable | WAL, crossbeam-skiplist, arrow2, polars |
| **Phase 2** | Parquet SSTable | rkyv SSTable, arrow2, parquet2 |
| **Phase 2** | HybridStorage | OLTP + OLAP 双路径 |
| **Phase 4** | Arrow2 分发 | iceoryx2, rkyv, arrow2 |
| **Phase 5** | Parquet Snapshot | WalRecovery, Arrow2 |
| **Phase 6** | 列式复制 | ReplicationMaster, Parquet |
| **Phase 8** | SQL 查询 | Polars, Arrow2, HybridStorage |

---

## 测试策略

### 单元测试（每个 Phase）

```bash
# 运行所有单元测试
cargo test --lib

# 测试覆盖率 > 80%
cargo tarpaulin --out Html
```

### 集成测试（Phase 2, 4, 6, 8）

```bash
# 端到端测试
cargo test --test integration_tests

# 关键场景
- test_hybrid_storage: OLTP + OLAP 混合
- test_arrow2_query: SQL 查询
- test_parquet_compaction: 列式压缩
- test_replication_with_parquet: 列式复制
```

### 性能基准测试（每个 Phase）

```bash
# Criterion 基准测试
cargo bench

# 关键指标
- wal_append: < 1ms
- memtable_insert: < 1μs
- sstable_get: < 100μs
- arrow2_scan: > 1GB/s
- sql_aggregate: > 100M rows/s
```

### 压力测试（Phase 7）

```bash
# 百万级写入
cargo run --release --example benchmark_storage_system

# 亿级查询
cargo run --release --example benchmark_arrow2_query

# 混合负载
cargo run --release --example benchmark_hybrid_workload
```

---

## 文档和 Changelog

### 文档更新计划

| Phase | 文档更新 |
|-------|---------|
| **Phase 1** | [CHANGELOG.md](../CHANGELOG.md) v0.2.0 - WAL 实现 |
| **Phase 2** | [CHANGELOG.md](../CHANGELOG.md) v0.3.0 - MemTable + SSTable<br>[05_ARROW2_QUERY_ENGINE.md](05_ARROW2_QUERY_ENGINE.md) - Arrow2 集成 |
| **Phase 3** | [CHANGELOG.md](../CHANGELOG.md) v0.4.0 - Compaction |
| **Phase 4** | [CHANGELOG.md](../CHANGELOG.md) v0.5.0 - 零拷贝分发 |
| **Phase 5** | [CHANGELOG.md](../CHANGELOG.md) v0.6.0 - 故障恢复 |
| **Phase 6** | [CHANGELOG.md](../CHANGELOG.md) v0.7.0 - 主从复制 |
| **Phase 7** | [CHANGELOG.md](../CHANGELOG.md) v0.8.0 - 性能优化 |
| **Phase 8** | [CHANGELOG.md](../CHANGELOG.md) v1.0.0 - 查询引擎<br>[QUERY_ENGINE_GUIDE.md](QUERY_ENGINE_GUIDE.md) - SQL 用户手册 |

### CHANGELOG 格式

```markdown
## [0.2.0] - 2025-10-XX

### Added
- ✅ WAL (Write-Ahead Log) 实现
  - 单条/批量写入
  - WAL 回放和 Checkpoint
  - CRC32 校验

### Performance
- 写入延迟 P99 < 1ms
- 批量写入 > 100K ops/s

### Tests
- 14 个单元测试全部通过
- 性能基准测试通过

---

## [0.3.0] - 2025-10-XX

### Added
- ✅ MemTable 实现（OLTP + OLAP 双模式）
  - SkipMap MemTable (OLTP)
  - Arrow2 MemTable (OLAP)
  - 异步转换器

- ✅ SSTable 实现（rkyv + Parquet）
  - rkyv SSTable (OLTP)
  - Parquet SSTable (OLAP)
  - HybridStorage 统一接口

### Performance
- MemTable 插入 P99 < 1μs
- SSTable 查询 P99 < 100μs
- Parquet 压缩比 5-10x

### Tests
- 28 个单元测试全部通过
- Arrow2 集成测试通过
```

---

## 验收标准

### 功能完整性

- [ ] WAL 读写正常
- [ ] MemTable 读写正常（OLTP + OLAP）
- [ ] SSTable 读写正常（rkyv + Parquet）
- [ ] Compaction 正常执行
- [ ] 分发系统正常
- [ ] 恢复机制正常
- [ ] 主从复制正常
- [ ] SQL 查询引擎正常

### 性能指标

- [ ] 写入延迟 P99 < 10μs
- [ ] 读取延迟 P99 < 100μs
- [ ] 分发延迟 P99 < 10μs
- [ ] 扫描吞吐 > 1GB/s
- [ ] SQL 聚合 > 100M rows/s
- [ ] 恢复时间 < 10s（10GB）
- [ ] 复制延迟 < 100ms
- [ ] 故障检测 < 5s
- [ ] 故障转移 < 30s

### 可靠性指标

- [ ] 数据一致性 100%
- [ ] 崩溃恢复成功率 100%
- [ ] 断点续传成功率 100%
- [ ] ACK 可靠性 99.99%
- [ ] 故障转移成功率 100%

### 生产就绪

- [ ] 文档完整
- [ ] 测试覆盖率 > 80%
- [ ] 无内存泄漏
- [ ] 无数据竞争
- [ ] 日志完善
- [ ] 监控完善
- [ ] SQL 用户手册

---

## 下一步行动

### 立即执行（Week 1）

1. **修复 Phase 1 编译错误**
   ```bash
   cargo add tempfile --dev
   # 修复 io::Error → String 转换
   # 添加 rkyv::Deserialize import
   ```

2. **运行 Phase 1 测试**
   ```bash
   cargo test storage::wal --lib
   cargo bench --bench storage_bench -- wal
   ```

3. **更新 CHANGELOG v0.2.0**
   - 记录 WAL 实现
   - 记录性能指标

4. **开始 Phase 2**
   - 创建 `src/storage/memtable/` 目录
   - 创建 `src/storage/arrow/` 目录
   - 实现 SkipMap MemTable

---

## 相关链接

- [存储架构设计](01_STORAGE_ARCHITECTURE.md)
- [数据分发架构](02_DISTRIBUTION_ARCHITECTURE.md)
- [故障恢复设计](03_RECOVERY_DESIGN.md)
- [原实施计划](04_IMPLEMENTATION_PLAN.md)
- [Arrow2 查询引擎设计](05_ARROW2_QUERY_ENGINE.md)
- [文档中心](README.md)

---

*创建时间: 2025-10-03*
*维护者: @yutiansut*
*状态: Phase 1 进行中*
