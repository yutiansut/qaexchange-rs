# API 参考

完整的 API 文档和协议规范。

## 📁 API 分类

### [WebSocket API](websocket/)
实时双向通信协议。

- **[协议规范](websocket/protocol.md)** - DIFF 协议完整定义
- **[DIFF 协议详解](websocket/diff_protocol.md)** - 差分同步机制
- **[快速开始](websocket/quick_start.md)** - WebSocket 客户端示例

### [HTTP API](http/)
RESTful API 接口。

- **[用户 API](http/user_api.md)** - 用户/账户/订单管理接口
- **[管理员 API](http/admin_api.md)** - 系统管理接口

### 错误处理
- **[错误码参考](error_codes.md)** - 完整错误码列表

## 🎯 API 设计原则

1. **RESTful**: HTTP API 遵循 REST 规范
2. **实时性**: WebSocket 提供实时推送
3. **差分同步**: DIFF 协议减少数据传输
4. **类型安全**: 严格的类型定义

## 📊 API 统计

| API 类型 | 端点数量 | 消息类型 |
|----------|----------|----------|
| HTTP (用户) | 10+ | - |
| HTTP (管理员) | 25+ | - |
| WebSocket | 1 | 15+ 消息 |

## 🔗 相关文档

- [前端集成](../05_integration/frontend/) - 前端对接指南
- [数据模型](../02_architecture/data_models.md) - QIFI/TIFI/DIFF 协议

---

[返回文档中心](../README.md)
