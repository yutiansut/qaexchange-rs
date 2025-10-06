# 存储和分发系统实施计划

> 7 阶段实施路线图

**版本**: v1.0.0
**最后更新**: 2025-10-03

---

## 📋 目录

- [实施概览](#实施概览)
- [阶段规划](#阶段规划)
- [关键里程碑](#关键里程碑)
- [技术栈](#技术栈)
- [测试策略](#测试策略)

---

## 实施概览

### 总体目标

构建高性能、高可靠的存储和分发系统，支持：
- 写入延迟 P99 < 10μs
- 读取延迟 P99 < 100μs
- 分发延迟 P99 < 10μs
- 恢复时间 < 10s
- 主从复制延迟 < 100ms

### 实施时间线

```
Phase 1: WAL 实现 (1 周)
Phase 2: MemTable + SSTable (2 周)
Phase 3: Compaction (1 周)
Phase 4: 零拷贝分发 (2 周)
Phase 5: 故障恢复 (1 周)
Phase 6: 主从复制 (2 周)
Phase 7: 性能优化 (1 周)

总计: 10 周
```

### 依赖关系

```
Phase 1 (WAL)
    ↓
Phase 2 (MemTable + SSTable)
    ↓
Phase 3 (Compaction)
    ↓
Phase 4 (分发) + Phase 5 (恢复) [并行]
    ↓
Phase 6 (复制)
    ↓
Phase 7 (优化)
```

---

## 阶段规划

### Phase 1: WAL 实现 (1 周)

**目标**: 实现 Write-Ahead Log，保证数据持久化

**任务清单**:

- [ ] WAL 数据结构设计
  - [ ] `WalRecord` 枚举（OrderInsert, TradeExecuted, AccountUpdate）
  - [ ] `WalEntry` 结构（sequence, crc32, timestamp, record）
  - [ ] rkyv 序列化支持

- [ ] WAL 文件管理
  - [ ] `WalManager::new()` - 初始化
  - [ ] `WalManager::append()` - 追加单条记录（同步 fsync）
  - [ ] `WalManager::append_batch()` - 批量追加（减少 fsync）
  - [ ] 文件格式：`wal_{sequence:020}.log`
  - [ ] Header 设计（Magic, Version, Start Sequence）

- [ ] WAL 回放
  - [ ] `WalManager::replay()` - 回放回调
  - [ ] 顺序读取 WAL 文件
  - [ ] CRC32 校验

- [ ] Checkpoint 机制
  - [ ] `WalManager::checkpoint()` - 截断旧 WAL
  - [ ] Checkpoint 记录写入

**交付物**:
- `src/storage/wal/mod.rs`
- `src/storage/wal/manager.rs`
- `src/storage/wal/record.rs`
- 单元测试：`tests/wal_tests.rs`

**验收标准**:
- 写入延迟 P99 < 1ms（单条）
- 批量写入吞吐 > 100K entries/s
- 崩溃恢复测试通过
- CRC32 校验通过率 100%

---

### Phase 2: MemTable + SSTable (2 周)

**目标**: 实现内存表和磁盘文件

#### Week 1: MemTable

- [ ] MemTable 实现
  - [ ] 基于 `crossbeam_skiplist::SkipMap`
  - [ ] `MemTable::insert()` - O(log N) 插入
  - [ ] `MemTable::get()` - O(log N) 查询
  - [ ] 大小限制（128MB）
  - [ ] 并发安全（无锁）

- [ ] MemTable 管理器
  - [ ] `MemTableManager::new()`
  - [ ] Active MemTable
  - [ ] Immutable MemTable 列表
  - [ ] `rotate()` - 切换 MemTable

- [ ] 集成 WAL
  - [ ] 写入 WAL → 写入 MemTable
  - [ ] WAL 回放 → 重建 MemTable

#### Week 2: SSTable

- [ ] SSTable 构建器
  - [ ] `SSTableBuilder::new()`
  - [ ] `add()` - 添加排序 key-value
  - [ ] `finish()` - 写入文件
  - [ ] Bloom Filter 构建
  - [ ] 稀疏索引（每 4KB 一个条目）

- [ ] SSTable 读取器
  - [ ] `SSTableReader::open()` - mmap 打开
  - [ ] `get()` - 零拷贝查询
  - [ ] Bloom Filter 过滤
  - [ ] 二分查找索引
  - [ ] 顺序扫描数据块

- [ ] SSTable 文件格式
  - [ ] Header（Magic, Version, Min/Max Key, Offsets）
  - [ ] Data Block（Key-Value pairs）
  - [ ] Index Block（Sparse index）
  - [ ] Bloom Filter Block

**交付物**:
- `src/storage/memtable/mod.rs`
- `src/storage/memtable/manager.rs`
- `src/storage/sstable/builder.rs`
- `src/storage/sstable/reader.rs`
- `src/storage/sstable/bloom_filter.rs`

**验收标准**:
- MemTable 插入延迟 < 1μs
- SSTable 查询延迟 P99 < 100μs
- Bloom Filter 误判率 < 1%
- 零拷贝验证通过

---

### Phase 3: Compaction (1 周)

**目标**: 实现 LSM-Tree Compaction

- [ ] Leveled Compaction
  - [ ] 7 级结构（L0: 40MB, L1: 400MB, ...）
  - [ ] L0 触发条件（文件数 ≥ 4）
  - [ ] L1+ 触发条件（层级大小超限）

- [ ] Compaction 执行器
  - [ ] `CompactionExecutor::should_compact()` - 检查条件
  - [ ] `compact()` - 执行合并
  - [ ] K-way 归并排序
  - [ ] 后台线程执行

- [ ] 元数据管理
  - [ ] SSTable 元数据（level, min/max key, entry count）
  - [ ] Manifest 文件（记录 SSTable 变更）
  - [ ] 版本管理（MVCC）

**交付物**:
- `src/storage/compaction/mod.rs`
- `src/storage/compaction/executor.rs`
- `src/storage/compaction/manifest.rs`

**验收标准**:
- Compaction 不阻塞写入
- CPU 占用 < 10%
- 磁盘空间放大 < 1.5x
- 读取延迟不受影响

---

### Phase 4: 零拷贝分发 (2 周)

**目标**: 基于 iceoryx2 + rkyv 的实时数据分发

#### Week 1: Publisher + Subscriber

- [ ] 分发消息定义
  - [ ] `DistributionMessage` 枚举
  - [ ] TradeEvent, AccountUpdate, MarketL2, Heartbeat
  - [ ] rkyv 序列化

- [ ] Publisher 实现
  - [ ] 基于 iceoryx2 共享内存
  - [ ] `publish()` - 零拷贝发布
  - [ ] `publish_batch()` - 批量发布
  - [ ] 心跳机制

- [ ] Subscriber 实现
  - [ ] `DistributionSubscriber::new()`
  - [ ] 零拷贝接收
  - [ ] 回调处理

#### Week 2: 多级订阅

- [ ] Real-time 订阅
  - [ ] WebSocket 实时推送
  - [ ] 延迟 P99 < 10μs

- [ ] Delayed 订阅
  - [ ] 批量聚合（100ms）
  - [ ] 批量处理

- [ ] Historical 订阅
  - [ ] WAL Replay
  - [ ] 历史查询

**交付物**:
- `src/distribution/message.rs`
- `src/distribution/publisher.rs`
- `src/distribution/subscriber.rs`
- `src/distribution/realtime_subscriber.rs`
- `src/distribution/delayed_subscriber.rs`
- `src/distribution/historical_subscriber.rs`

**验收标准**:
- 分发延迟 P99 < 10μs
- 吞吐量 > 10M msg/s
- 零拷贝验证
- WebSocket 推送正常

---

### Phase 5: 故障恢复 (1 周)

**目标**: 实现快速恢复和 Snapshot

- [ ] WAL 恢复
  - [ ] `WalRecovery::recover()` - 回放 WAL
  - [ ] 重建 MemTable
  - [ ] Checkpoint 加载

- [ ] SSTable 恢复
  - [ ] `SSTableRecovery::recover()` - 扫描 SSTable
  - [ ] 构建 LSM-Tree

- [ ] Snapshot 机制
  - [ ] `SnapshotManager::create_snapshot()` - 生成快照
  - [ ] `load_snapshot()` - 加载快照
  - [ ] 自动快照（每 30min）

- [ ] 可靠性订阅
  - [ ] ACK 确认机制
  - [ ] 断点续传
  - [ ] Checkpoint 保存

**交付物**:
- `src/storage/recovery/wal_recovery.rs`
- `src/storage/recovery/sstable_recovery.rs`
- `src/storage/recovery/snapshot.rs`
- `src/distribution/reliable_publisher.rs`
- `src/distribution/resumable_subscriber.rs`

**验收标准**:
- 恢复时间 < 10s（10GB 数据）
- Snapshot 生成时间 < 5s
- 断点续传成功率 100%
- ACK 可靠性 99.99%

---

### Phase 6: 主从复制 (2 周)

**目标**: 实现主从复制和自动故障转移

#### Week 1: 复制机制

- [ ] Replication Master
  - [ ] `ReplicationMaster::new()`
  - [ ] `register_slave()` - 注册 Slave
  - [ ] `replicate()` - 复制 WAL
  - [ ] ACK 处理

- [ ] Replication Slave
  - [ ] `ReplicationSlave::start()` - 启动复制
  - [ ] WAL 接收和应用
  - [ ] 发送 ACK

- [ ] 同步 vs 异步
  - [ ] 同步复制（等待所有 Slave）
  - [ ] 异步复制（不等待）

#### Week 2: 故障转移

- [ ] 故障检测
  - [ ] `FailureDetector::start()` - 心跳检测
  - [ ] 节点状态（Healthy, Degraded, Failed）

- [ ] 自动故障转移
  - [ ] `FailoverCoordinator::failover()` - 执行转移
  - [ ] 选举新 Master
  - [ ] 重新配置 Slave

- [ ] 健康监控
  - [ ] `HealthMonitor::start()` - 健康检查
  - [ ] Publisher 状态监控

**交付物**:
- `src/storage/replication/master.rs`
- `src/storage/replication/slave.rs`
- `src/storage/failover/detector.rs`
- `src/storage/failover/coordinator.rs`
- `src/distribution/health_monitor.rs`

**验收标准**:
- 复制延迟 < 100ms
- 故障检测 < 5s
- 故障转移 < 30s
- 数据一致性 100%

---

### Phase 7: 性能优化 (1 周)

**目标**: 性能调优和压力测试

- [ ] WAL 优化
  - [ ] Group Commit（合并多个写入的 fsync）
  - [ ] 预分配空间（fallocate）
  - [ ] Direct I/O（可选）

- [ ] MemTable 优化
  - [ ] 并发插入优化
  - [ ] 内存池（减少分配）

- [ ] SSTable 优化
  - [ ] mmap 预读
  - [ ] 缓存热点 Index
  - [ ] LZ4 压缩（可选）

- [ ] 分发优化
  - [ ] CPU 亲和性
  - [ ] 批量处理
  - [ ] 预分配 Ring Buffer

- [ ] 压力测试
  - [ ] 百万级订单写入
  - [ ] 千万级消息分发
  - [ ] 崩溃恢复测试
  - [ ] 故障转移测试

**交付物**:
- 性能报告
- 压力测试结果
- 优化建议

**验收标准**:
- 所有性能目标达标
- 压力测试通过
- 生产就绪

---

## 关键里程碑

| 里程碑 | 时间 | 交付物 |
|--------|------|--------|
| **M1: WAL 完成** | Week 1 | WAL 读写，回放，Checkpoint |
| **M2: 存储引擎完成** | Week 3 | MemTable + SSTable |
| **M3: Compaction 完成** | Week 4 | LSM-Tree Compaction |
| **M4: 分发系统完成** | Week 6 | 零拷贝分发，多级订阅 |
| **M5: 恢复机制完成** | Week 7 | WAL 恢复，Snapshot |
| **M6: 复制系统完成** | Week 9 | 主从复制，故障转移 |
| **M7: 生产就绪** | Week 10 | 性能优化，压力测试 |

---

## 技术栈

### 核心依赖

```toml
[dependencies]
# 序列化
rkyv = { version = "0.7", features = ["validation"] }
serde = { version = "1.0", features = ["derive"] }

# 并发
crossbeam = "0.8"
crossbeam-skiplist = "0.1"
parking_lot = "0.12"
dashmap = "5.5"
tokio = { version = "1.35", features = ["full"] }

# 零拷贝分发（复用 qars）
# iceoryx2 = "0.3"  # qars 已集成

# 文件操作
memmap2 = "0.9"

# 哈希和校验
crc32fast = "1.3"

# 压缩（可选）
lz4 = "1.24"

# 日志
log = "0.4"
env_logger = "0.11"

# 时间
chrono = "0.4"
```

### 目录结构

```
src/storage/
├── mod.rs                  # 模块入口
├── wal/                    # WAL 实现
│   ├── mod.rs
│   ├── manager.rs
│   └── record.rs
├── memtable/               # MemTable 实现
│   ├── mod.rs
│   └── manager.rs
├── sstable/                # SSTable 实现
│   ├── mod.rs
│   ├── builder.rs
│   ├── reader.rs
│   └── bloom_filter.rs
├── compaction/             # Compaction 实现
│   ├── mod.rs
│   ├── executor.rs
│   └── manifest.rs
├── recovery/               # 恢复机制
│   ├── mod.rs
│   ├── wal_recovery.rs
│   ├── sstable_recovery.rs
│   └── snapshot.rs
├── replication/            # 主从复制
│   ├── mod.rs
│   ├── master.rs
│   └── slave.rs
├── failover/               # 故障转移
│   ├── mod.rs
│   ├── detector.rs
│   └── coordinator.rs
└── backup/                 # 备份
    ├── mod.rs
    └── full_backup.rs

src/distribution/
├── mod.rs
├── message.rs
├── publisher.rs
├── subscriber.rs
├── realtime_subscriber.rs
├── delayed_subscriber.rs
├── historical_subscriber.rs
├── reliable_publisher.rs
├── resumable_subscriber.rs
└── health_monitor.rs
```

---

## 测试策略

### 单元测试

```bash
# 测试覆盖率 > 80%
cargo test --lib

# 各模块单元测试
cargo test wal::tests
cargo test memtable::tests
cargo test sstable::tests
cargo test compaction::tests
cargo test distribution::tests
```

### 集成测试

```bash
# 端到端测试
cargo test --test integration_tests

# 测试场景
- test_wal_recovery: WAL 回放恢复
- test_sstable_query: SSTable 查询
- test_compaction: Compaction 合并
- test_distribution: 分发系统
- test_replication: 主从复制
- test_failover: 故障转移
```

### 压力测试

```bash
# 百万级订单写入
cargo run --release --example benchmark_storage

# 千万级消息分发
cargo run --release --example benchmark_distribution

# 崩溃恢复
cargo run --release --example crash_recovery_test

# 故障转移
cargo run --release --example failover_test
```

### 性能基准

```bash
# Criterion 基准测试
cargo bench

# 基准项目
- wal_append: WAL 追加延迟
- memtable_insert: MemTable 插入延迟
- sstable_get: SSTable 查询延迟
- distribution_publish: 分发延迟
- recovery_time: 恢复时间
```

---

## 验收标准

### 功能完整性

- [ ] WAL 读写正常
- [ ] MemTable 读写正常
- [ ] SSTable 读写正常
- [ ] Compaction 正常执行
- [ ] 分发系统正常
- [ ] 恢复机制正常
- [ ] 主从复制正常
- [ ] 故障转移正常

### 性能指标

- [ ] 写入延迟 P99 < 10μs
- [ ] 读取延迟 P99 < 100μs
- [ ] 分发延迟 P99 < 10μs
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

---

## 风险和缓解

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| WAL fsync 性能不达标 | 高 | 中 | Group Commit, 预分配空间 |
| iceoryx2 不稳定 | 高 | 低 | 降级到 tokio::mpsc |
| Compaction 阻塞写入 | 中 | 中 | 后台线程，增量压缩 |
| 故障转移数据丢失 | 高 | 低 | 同步复制，WAL 备份 |
| 性能目标未达成 | 中 | 中 | 提前压测，持续优化 |

---

## 下一步行动

1. **立即开始 Phase 1**：创建 `src/storage/wal/` 模块
2. **设置开发环境**：安装依赖，配置测试框架
3. **创建基准测试**：`benches/storage_bench.rs`
4. **编写集成测试**：`tests/integration_tests.rs`

---

## 相关链接

- [存储架构设计](01_STORAGE_ARCHITECTURE.md)
- [数据分发架构](02_DISTRIBUTION_ARCHITECTURE.md)
- [故障恢复设计](03_RECOVERY_DESIGN.md)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
