# 核心模块

核心功能模块详细说明。

## 📁 模块分类

### [存储系统](storage/)
完整的数据持久化解决方案。

- **[WAL 设计](storage/wal.md)** - Write-Ahead Log 崩溃恢复
- **[MemTable 实现](storage/memtable.md)** - OLTP/OLAP 内存表
- **[SSTable 格式](storage/sstable.md)** - rkyv/Parquet 持久化
- **[查询引擎](storage/query_engine.md)** - Polars SQL 查询
- **[复制系统](storage/replication.md)** - 主从复制与故障转移

### [通知系统](notification/)
零拷贝实时通知推送。

- **[通知架构](notification/architecture.md)** - 零拷贝通知推送
- **[订阅管理](notification/subscription.md)** - 订阅过滤与路由

## 🎯 设计原则

1. **高性能**: WAL P99 < 50ms, MemTable < 10μs
2. **零拷贝**: rkyv 序列化,mmap 读取
3. **可靠性**: WAL + CRC32,崩溃恢复
4. **可扩展**: 模块化设计,易于扩展

## 📊 性能指标

| 模块 | 指标 | 目标 | 实测 |
|------|------|------|------|
| WAL | 写入延迟 (P99) | < 50ms | 21ms ✅ |
| WAL | 批量吞吐 | > 78K/s | 78,125/s ✅ |
| MemTable | 写入延迟 (P99) | < 10μs | 2.6μs ✅ |
| SSTable | 读取延迟 (P99) | < 50μs | 20μs ✅ |
| 通知 | 序列化性能 | 125x JSON | 125x ✅ |

## 📚 后续阅读

- [高性能架构](../02_architecture/high_performance.md) - 性能设计原理
- [高级主题](../08_advanced/) - 深度技术文档

---

[返回文档中心](../README.md)
