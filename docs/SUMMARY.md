# Summary

[介绍](README.md)

---

# 快速开始

- [快速开始](01_getting_started/README.md)
  - [快速入门指南](01_getting_started/quick_start.md)
  - [完整构建清单](01_getting_started/build_checklist.md)

---

# 系统架构

- [架构设计](02_architecture/README.md)
  - [系统概览](02_architecture/system_overview.md)
  - [高性能架构](02_architecture/high_performance.md)
  - [Actix Actor 架构](02_architecture/actor_architecture.md)
  - [交易机制](02_architecture/trading_mechanism.md)
  - [数据模型](02_architecture/data_models.md)
  - [存储解耦架构](02_architecture/decoupled_storage.md)

---

# 核心模块

- [核心模块](03_core_modules/README.md)
  - [存储系统]()
    - [WAL 设计](03_core_modules/storage/wal.md)
    - [MemTable 实现](03_core_modules/storage/memtable.md)
    - [SSTable 格式](03_core_modules/storage/sstable.md)
    - [压缩策略](03_core_modules/storage/compression.md)
    - [二级索引](03_core_modules/storage/index.md)
    - [查询引擎](03_core_modules/storage/query_engine.md)
    - [复制系统](03_core_modules/storage/replication.md)
  - [撮合引擎]()
    - [撮合引擎模块](03_core_modules/matching/README.md)
    - [测试指南](03_core_modules/matching/testing.md)
    - [性能基准](03_core_modules/matching/benchmark.md)
    - [压力测试](03_core_modules/matching/stress_testing.md)
  - [市场数据模块]()
    - [市场数据模块](03_core_modules/market/README.md)
    - [快照生成器](03_core_modules/market/snapshot_generator.md)
    - [K线聚合系统](03_core_modules/market/kline.md)
    - [数据生产测试](03_core_modules/market/data_production_testing.md)
  - [通知系统]()
    - [架构设计](03_core_modules/notification/architecture.md)
    - [订阅过滤](03_core_modules/notification/subscription.md)
  - [因子计算系统]()
    - [因子系统概览](03_core_modules/factor/README.md)
    - [因子 WAL 集成](03_core_modules/factor/wal_persister.md)
  - [集群管理]()
    - [集群管理系统](03_core_modules/cluster/README.md)

---

# API 文档

- [API 文档](04_api/README.md)
  - [完整协议文档](protocol/README.md)
  - [错误码](04_api/error_codes.md)
  - [管理端集成](04_api/admin_integration.md)
  - [HTTP API]()
    - [用户 API](04_api/http/user_api.md)
    - [管理 API](04_api/http/admin_api.md)
  - [WebSocket API]()
    - [协议说明](04_api/websocket/protocol.md)
    - [DIFF 协议](04_api/websocket/diff_protocol.md)
    - [快速开始](04_api/websocket/quick_start.md)

---

# 集成指南

- [集成指南](05_integration/README.md)
  - [DIFF 协议集成](05_integration/diff_protocol.md)
  - [序列化指南](05_integration/serialization.md)
  - [前端集成]()
    - [集成指南](05_integration/frontend/integration_guide.md)
    - [API 使用指南](05_integration/frontend/api_guide.md)
    - [集成清单](05_integration/frontend/integration_checklist.md)

---

# 开发指南

- [开发指南](06_development/README.md)
  - [WebSocket 集成指南](06_development/websocket_integration.md)
  - [测试指南](06_development/testing.md)
  - [部署指南](06_development/deployment.md)

---

# 参考资料

- [参考资料](07_reference/README.md)
  - [术语表](07_reference/glossary.md)
  - [常见问题 FAQ](07_reference/faq.md)
  - [性能基准](07_reference/benchmarks.md)
  - [功能矩阵](07_reference/feature_matrix.md)
  - [性能指标](07_reference/performance.md)

---

# 高级主题

- [高级主题](08_advanced/README.md)
  - [实现总结]()
    - [市场数据增强](08_advanced/implementation_summaries/market_data.md)
    - [管理功能](08_advanced/implementation_summaries/management_features.md)
    - [K线聚合系统](08_advanced/implementation_summaries/kline_system.md)
    - [K线实时推送系统](06_development/KLINE_IMPLEMENTATION_SUMMARY.md)
  - [技术深度解析]()
    - [市场数据增强实现](08_advanced/technical_deep_dive/market_data_enhancement.md)
  - [阶段报告]()
    - [Phase 6-7 实现总结](08_advanced/phase_reports/phase_6_7.md)
    - [Phase 8 查询引擎](08_advanced/phase_reports/phase_8.md)
  - [测试报告]()
    - [DIFF 协议测试报告](08_advanced/diff_test_reports/main_report.md)
