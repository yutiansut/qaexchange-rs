# 高级主题

深度技术文档和实现报告。

## 📁 内容分类

### [Phase 报告](phase_reports/)
各 Phase 的详细实现报告。

- **[Phase 6-7 实现报告](phase_reports/phase_6_7.md)** - 复制系统与性能优化

### [实现总结](implementation_summaries/)
功能模块实现总结文档。

- **[市场数据实现](implementation_summaries/market_data.md)** - Phase 9 市场数据增强
- **[管理功能实现](implementation_summaries/management_features.md)** - Phase 10 用户管理

### [技术深度](technical_deep_dive/)
深度技术探讨文档。

- **[市场数据增强](technical_deep_dive/market_data_enhancement.md)** - L1 缓存与 WAL 恢复

### [DIFF 测试报告](diff_test_reports/)
DIFF 协议测试结果。

- **[主测试报告](diff_test_reports/main_report.md)** - DIFF 协议测试结果

## 🎯 面向读者

- **架构师**: 系统设计决策与权衡
- **高级开发者**: 深度技术实现细节
- **研究人员**: 性能优化与算法

## 📊 涉及主题

1. **存储系统**: WAL + MemTable + SSTable + Compaction
2. **复制系统**: 主从复制 + 故障转移
3. **查询引擎**: Polars DataFrame + SQL
4. **市场数据**: L1 缓存 + WAL 恢复
5. **用户管理**: JWT + bcrypt
6. **性能优化**: 零拷贝 + mmap + Bloom Filter

## 🔗 相关文档

- [系统架构](../02_architecture/) - 基础架构设计
- [核心模块](../03_core_modules/) - 模块实现

---

[返回文档中心](../README.md)
