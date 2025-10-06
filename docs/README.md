# QAExchange-RS 文档中心

**版本**: v1.0.0
**最后更新**: 2025-10-06

欢迎使用 QAExchange-RS 文档！本文档中心提供完整的系统架构、API 参考、集成指南和开发文档。

---

## 📚 文档导航

### 🚀 [01. 快速开始](./01_getting_started/)
新用户入门必读，快速搭建和运行 QAExchange-RS。

- [快速开始指南](./01_getting_started/quick_start.md) - 5分钟快速上手
- [构建检查清单](./01_getting_started/build_checklist.md) - 构建前必读

---

### 🏗️ [02. 系统架构](./02_architecture/)
深入了解 QAExchange-RS 的核心架构设计。

- [系统总览](./02_architecture/system_overview.md) - 整体架构与模块划分
- [高性能架构](./02_architecture/high_performance.md) - P99 < 100μs 延迟设计
- [数据模型](./02_architecture/data_models.md) - QIFI/TIFI/DIFF 协议详解
- [交易机制](./02_architecture/trading_mechanism.md) - 撮合引擎与交易流程
- [解耦存储架构](./02_architecture/decoupled_storage.md) - 零拷贝 + WAL 持久化

---

### ⚙️ [03. 核心模块](./03_core_modules/)
核心功能模块详细说明。

#### 存储系统
- [WAL 设计](./03_core_modules/storage/wal.md) - Write-Ahead Log 崩溃恢复
- [MemTable 实现](./03_core_modules/storage/memtable.md) - OLTP/OLAP 内存表
- [SSTable 格式](./03_core_modules/storage/sstable.md) - rkyv/Parquet 持久化
- [查询引擎](./03_core_modules/storage/query_engine.md) - Polars SQL 查询
- [复制系统](./03_core_modules/storage/replication.md) - 主从复制与故障转移

#### 通知系统
- [通知架构](./03_core_modules/notification/architecture.md) - 零拷贝通知推送
- [订阅管理](./03_core_modules/notification/subscription.md) - 订阅过滤与路由

---

### 📡 [04. API 参考](./04_api/)
完整的 API 文档和协议规范。

#### WebSocket API
- [协议规范](./04_api/websocket/protocol.md) - DIFF 协议完整定义
- [DIFF 协议详解](./04_api/websocket/diff_protocol.md) - 差分同步机制
- [快速开始](./04_api/websocket/quick_start.md) - WebSocket 客户端示例

#### HTTP API
- [用户 API](./04_api/http/user_api.md) - 用户/账户/订单管理接口
- [管理员 API](./04_api/http/admin_api.md) - 系统管理接口

#### 错误处理
- [错误码参考](./04_api/error_codes.md) - 完整错误码列表

---

### 🔌 [05. 集成指南](./05_integration/)
前端集成和序列化指南。

#### 前端集成
- [集成指南](./05_integration/frontend/integration_guide.md) - Vue.js 集成示例
- [API 使用指南](./05_integration/frontend/api_guide.md) - 前端 API 调用规范
- [集成检查清单](./05_integration/frontend/integration_checklist.md) - 集成验收标准

#### 序列化
- [序列化指南](./05_integration/serialization.md) - rkyv/JSON 序列化最佳实践

---

### 🛠️ [06. 开发指南](./06_development/)
开发、测试、部署文档。

- [测试指南](./06_development/testing.md) - 单元测试与集成测试
- [部署指南](./06_development/deployment.md) - 生产环境部署

---

### 📖 [07. 参考资料](./07_reference/)
术语表、常见问题、性能基准。

- [术语表](./07_reference/glossary.md) - 专业术语解释（待创建）
- [常见问题 FAQ](./07_reference/faq.md) - 常见问题解答（待创建）
- [性能基准](./07_reference/benchmarks.md) - 性能测试数据（待创建）

---

### 🎓 [08. 高级主题](./08_advanced/)
深度技术文档和实现报告。

#### Phase 报告
- [Phase 6-7 实现报告](./08_advanced/phase_reports/phase_6_7.md) - 复制系统与性能优化

#### 实现总结
- [市场数据实现](./08_advanced/implementation_summaries/market_data.md) - Phase 9 市场数据增强
- [管理功能实现](./08_advanced/implementation_summaries/management_features.md) - Phase 10 用户管理

#### 技术深度
- [市场数据增强](./08_advanced/technical_deep_dive/market_data_enhancement.md) - L1 缓存与 WAL 恢复

#### DIFF 测试报告
- [主测试报告](./08_advanced/diff_test_reports/main_report.md) - DIFF 协议测试结果

---

### 🗄️ [09. 归档](./09_archive/)
历史文档和已废弃的计划。

- [旧计划](./09_archive/old_plans/) - 已完成或废弃的计划文档
- [历史报告](./09_archive/historical_reports/) - 开发过程历史报告
- [已废弃](./09_archive/deprecated/) - 已废弃的功能文档

---

## 🔍 快速查找

### 按角色查找
- **新手开发者**: [快速开始](./01_getting_started/) → [系统架构](./02_architecture/)
- **前端开发者**: [WebSocket API](./04_api/websocket/) → [前端集成](./05_integration/frontend/)
- **后端开发者**: [核心模块](./03_core_modules/) → [开发指南](./06_development/)
- **运维工程师**: [部署指南](./06_development/deployment.md) → [性能基准](./07_reference/benchmarks.md)
- **架构师**: [高性能架构](./02_architecture/high_performance.md) → [高级主题](./08_advanced/)

### 按主题查找
- **性能优化**: [高性能架构](./02_architecture/high_performance.md), [解耦存储](./02_architecture/decoupled_storage.md)
- **数据持久化**: [WAL](./03_core_modules/storage/wal.md), [SSTable](./03_core_modules/storage/sstable.md)
- **协议集成**: [DIFF 协议](./04_api/websocket/diff_protocol.md), [数据模型](./02_architecture/data_models.md)
- **WebSocket**: [协议规范](./04_api/websocket/protocol.md), [前端集成](./05_integration/frontend/integration_guide.md)
- **测试部署**: [测试指南](./06_development/testing.md), [部署指南](./06_development/deployment.md)

---

## 📊 文档版本信息

| 模块 | 版本 | 最后更新 | 状态 |
|------|------|----------|------|
| 快速开始 | v1.0.0 | 2025-10-06 | ✅ 完整 |
| 系统架构 | v1.0.0 | 2025-10-06 | ✅ 完整 |
| 核心模块 | v0.9.0 | 2025-10-06 | 🚧 部分完成 |
| API 参考 | v1.0.0 | 2025-10-06 | ✅ 完整 |
| 集成指南 | v1.0.0 | 2025-10-06 | ✅ 完整 |
| 开发指南 | v0.8.0 | 2025-10-06 | 🚧 部分完成 |
| 参考资料 | v0.5.0 | 2025-10-06 | 🚧 计划中 |
| 高级主题 | v1.0.0 | 2025-10-06 | ✅ 完整 |
| 归档 | - | 2025-10-06 | ✅ 已归档 |

---

## 🤝 贡献文档

发现文档问题或想要改进？请参考 [贡献指南](./06_development/contributing.md)（待创建）。

---

## 📮 反馈与支持

- **问题报告**: 请提交 GitHub Issue
- **功能建议**: 请提交 Feature Request
- **文档改进**: 欢迎提交 Pull Request

---

**最后更新**: 2025-10-06
**维护者**: QAExchange-RS 开发团队
