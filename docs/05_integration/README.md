# 集成指南

前端集成和序列化指南。

## 📁 内容分类

### [前端集成](frontend/)
Vue.js/React/Angular 前端集成。

- **[集成指南](frontend/integration_guide.md)** - Vue.js 集成示例
- **[API 使用指南](frontend/api_guide.md)** - 前端 API 调用规范
- **[集成检查清单](frontend/integration_checklist.md)** - 集成验收标准

### 序列化
- **[序列化指南](serialization.md)** - rkyv/JSON 序列化最佳实践

## 🎯 集成要点

1. **WebSocket 连接**: 实时数据推送
2. **DIFF 协议**: 差分同步减少数据传输
3. **状态管理**: Vuex/Redux 管理业务截面
4. **错误处理**: 统一的错误处理机制

## 📦 推荐技术栈

### Vue.js
```javascript
- Vue 3+
- Vuex 4+ (状态管理)
- Axios (HTTP 客户端)
- 原生 WebSocket
```

### React
```javascript
- React 18+
- Redux Toolkit (状态管理)
- Axios (HTTP 客户端)
- 原生 WebSocket
```

### Angular
```typescript
- Angular 15+
- NgRx (状态管理)
- HttpClient (HTTP 客户端)
- RxJS WebSocket
```

## 🔗 相关文档

- [WebSocket 协议](../04_api/websocket/) - 协议规范
- [HTTP API](../04_api/http/) - REST API 参考

---

[返回文档中心](../README.md)
