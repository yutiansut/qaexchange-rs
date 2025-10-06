# 功能映射矩阵

**版本**: v1.0
**更新时间**: 2025-10-05
**状态**: ✅ 已完成前后端对接

---

## 📋 目录

- [用户端功能](#用户端功能)
- [管理端功能](#管理端功能)
- [WebSocket 实时功能](#websocket-实时功能)
- [功能状态说明](#功能状态说明)

---

## 用户端功能

### 1. 认证和用户管理

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 用户登录 | `views/login.vue` | `/login` | `/auth/login` | POST | ✅ | JWT认证 |
| 用户注册 | `views/register.vue` | `/register` | `/auth/register` | POST | ✅ | 创建用户 |
| 获取当前用户 | - | - | `/auth/current-user` | GET | ✅ | Token验证 |

---

### 2. 账户管理

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 查看账户信息 | `views/accounts/index.vue` | `/accounts` | `/api/account/{user_id}` | GET | ✅ | QIFI格式 |
| 账户详情 | `views/accounts/index.vue` | `/accounts` | `/api/account/detail/{user_id}` | GET | ✅ | 完整切片 |
| 开户申请 | - | - | `/api/account/open` | POST | ✅ | 管理端功能 |
| 入金 | `views/accounts/index.vue` | `/accounts` | `/api/account/deposit` | POST | ✅ | 资金操作 |
| 出金 | `views/accounts/index.vue` | `/accounts` | `/api/account/withdraw` | POST | ✅ | 资金操作 |
| 账户资金曲线 | `views/user/account-curve.vue` | `/account-curve` | `/api/account/{user_id}` | GET | ✅ | 基于历史数据 |

---

### 3. 交易下单

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 市价/限价下单 | `views/trade/index.vue` | `/trade` | `/api/order/submit` | POST | ✅ | 开仓 |
| 平仓下单 | `views/trade/components/CloseForm.vue` | `/trade` | `/api/order/submit` | POST | ✅ | 平仓 |
| 撤单 | `views/orders/index.vue` | `/orders` | `/api/order/cancel` | POST | ✅ | 订单管理 |
| 查询订单 | `views/orders/index.vue` | `/orders` | `/api/order/{order_id}` | GET | ✅ | 单个订单 |
| 用户订单列表 | `views/orders/index.vue` | `/orders` | `/api/order/user/{user_id}` | GET | ✅ | 所有订单 |

---

### 4. 持仓管理

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 查看持仓 | `views/positions/index.vue` | `/positions` | `/api/position/{user_id}` | GET | ✅ | 实时持仓 |
| 持仓盈亏 | `views/positions/index.vue` | `/positions` | - | - | ✅ | 前端计算 |
| 平仓操作 | `views/positions/index.vue` | `/positions` | `/api/order/submit` | POST | ✅ | 调用下单API |

---

### 5. 成交记录

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 用户成交列表 | `views/trades/index.vue` | `/trades` | `/api/order/user/{user_id}/trades` | GET | ✅ | 历史成交 |
| 成交详情 | `views/trades/index.vue` | `/trades` | - | - | ✅ | 列表展示 |

---

### 6. 行情数据

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 实时行情 | `views/chart/index.vue` | `/chart` | `/api/market/tick/{instrument_id}` | GET | ✅ | 轮询/WebSocket |
| K线图表 | `views/chart/index.vue` | `/chart` | - | - | ⚠️ | TradingView |
| 订单簿 | `views/trade/index.vue` | `/trade` | `/api/market/orderbook/{instrument_id}` | GET | ✅ | 盘口数据 |
| 最近成交 | `views/trade/index.vue` | `/trade` | `/api/market/recent-trades/{instrument_id}` | GET | ✅ | 市场成交 |

---

### 7. 仪表盘

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 账户概览 | `views/dashboard/index.vue` | `/dashboard` | `/api/account/{user_id}` | GET | ✅ | 资金统计 |
| 持仓概览 | `views/dashboard/index.vue` | `/dashboard` | `/api/position/{user_id}` | GET | ✅ | 持仓统计 |
| 订单概览 | `views/dashboard/index.vue` | `/dashboard` | `/api/order/user/{user_id}` | GET | ✅ | 订单统计 |
| 盈亏图表 | `views/dashboard/index.vue` | `/dashboard` | - | - | ✅ | 前端计算 |

---

## 管理端功能

### 8. 合约管理

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 合约列表 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instruments` | GET | ✅ | 所有合约 |
| 创建合约 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instrument/create` | POST | ✅ | 上市新合约 |
| 更新合约 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instrument/{id}/update` | PUT | ✅ | 修改参数 |
| 暂停交易 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instrument/{id}/suspend` | PUT | ✅ | 临时暂停 |
| 恢复交易 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instrument/{id}/resume` | PUT | ✅ | 恢复交易 |
| 下市合约 | `views/admin/instruments.vue` | `/admin-instruments` | `/admin/instrument/{id}/delist` | DELETE | ✅ | 永久下市 |

**关键实现**:
- 下市前检查所有账户是否有未平仓持仓
- 返回详细错误信息（包含持仓账户列表）

---

### 9. 结算管理

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 设置结算价 | `views/admin/settlement.vue` | `/admin-settlement` | `/admin/settlement/set-price` | POST | ✅ | 单个合约 |
| 批量设置结算价 | `views/admin/settlement.vue` | `/admin-settlement` | `/admin/settlement/batch-set-prices` | POST | ✅ | 多个合约 |
| 执行日终结算 | `views/admin/settlement.vue` | `/admin-settlement` | `/admin/settlement/execute` | POST | ✅ | 全账户结算 |
| 结算历史 | `views/admin/settlement.vue` | `/admin-settlement` | `/admin/settlement/history` | GET | ✅ | 支持日期筛选 |
| 结算详情 | `views/admin/settlement.vue` | `/admin-settlement` | `/admin/settlement/detail/{date}` | GET | ✅ | 单日详情 |

**关键实现**:
- 两步结算流程：设置结算价 → 执行结算
- 遍历所有账户计算盈亏
- 自动识别并记录强平账户
- 计算累计手续费和总盈亏

---

### 10. 风控监控

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 风险账户列表 | `views/admin/risk.vue` | `/admin-risk` | `/admin/risk/accounts` | GET | ⚠️ | 后端未实现 |
| 保证金监控 | `views/admin/risk.vue` | `/admin-risk` | `/admin/risk/margin-summary` | GET | ⚠️ | 后端未实现 |
| 强平记录 | `views/admin/risk.vue` | `/admin-risk` | `/admin/risk/liquidations` | GET | ⚠️ | 后端未实现 |

**状态说明**:
- ⚠️ 前端已实现，后端API待开发
- 前端有fallback逻辑（从账户数据计算）

---

### 11. 账户管理（管理端）

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 所有账户列表 | `views/admin/accounts.vue` | `/admin-accounts` | `/api/account/list` | GET | ✅ | 管理员视图 |
| 账户详情 | `views/admin/accounts.vue` | `/admin-accounts` | `/api/account/detail/{user_id}` | GET | ✅ | 完整信息 |
| 审核开户 | `views/admin/accounts.vue` | `/admin-accounts` | `/api/account/open` | POST | ✅ | 管理员开户 |
| 资金调整 | `views/admin/accounts.vue` | `/admin-accounts` | `/api/account/deposit` | POST | ✅ | 管理员操作 |

---

### 12. 交易管理（管理端）

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 所有交易记录 | `views/admin/transactions.vue` | `/admin-transactions` | `/api/market/transactions` | GET | ✅ | 全市场成交 |
| 订单统计 | `views/admin/transactions.vue` | `/admin-transactions` | `/api/market/order-stats` | GET | ✅ | 统计数据 |

---

### 13. 系统监控

| 功能 | 前端页面 | 路由 | 后端API | HTTP方法 | 状态 | 备注 |
|------|---------|------|---------|---------|------|------|
| 系统状态 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/system` | GET | ✅ | CPU/内存/磁盘 |
| 存储监控 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/storage` | GET | ✅ | WAL/MemTable/SSTable |
| 账户监控 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/accounts` | GET | ✅ | 账户数统计 |
| 订单监控 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/orders` | GET | ✅ | 订单统计 |
| 成交监控 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/trades` | GET | ✅ | 成交统计 |
| 生成报告 | `views/monitoring/index.vue` | `/monitoring` | `/monitoring/report` | POST | ✅ | 导出报告 |

---

## WebSocket 实时功能

### 14. 实时推送

| 功能 | 客户端订阅 | 服务端推送消息 | 状态 | 备注 |
|------|-----------|---------------|------|------|
| 用户认证 | `ClientMessage::Auth` | `ServerMessage::AuthResponse` | ✅ | 连接时认证 |
| 订阅频道 | `ClientMessage::Subscribe` | - | ✅ | 订阅行情/交易 |
| 实时行情 | - | `ServerMessage::Tick` | ✅ | 行情推送 |
| 订单簿快照 | - | `ServerMessage::OrderBook` | ✅ | Level2数据 |
| 订单状态更新 | - | `ServerMessage::OrderStatus` | ✅ | 订单变化 |
| 成交推送 | - | `ServerMessage::Trade` | ✅ | 新成交 |
| 账户更新 | - | `ServerMessage::AccountUpdate` | ✅ | 资金/持仓变化 |
| 心跳 | `ClientMessage::Ping` | `ServerMessage::Pong` | ✅ | 10秒超时 |

**WebSocket 连接**:
- URL: `ws://host:port/ws?user_id=<user_id>`
- 协议: JSON 消息
- 心跳: 10秒间隔

---

## 功能状态说明

### ✅ 已完成（38个功能）
前后端完全对接，功能正常运行

### ⚠️ 部分完成（3个功能）
- 风险账户列表 - 前端完成，后端API待开发
- 保证金监控 - 前端完成，后端API待开发
- 强平记录 - 前端完成，后端API待开发

### ❌ 未实现（0个功能）
无

---

## 功能统计

| 模块 | 前端页面 | 后端API | 完成度 |
|------|---------|---------|--------|
| 认证和用户管理 | 2个 | 3个 | ✅ 100% |
| 账户管理 | 2个 | 6个 | ✅ 100% |
| 交易下单 | 2个 | 5个 | ✅ 100% |
| 持仓管理 | 1个 | 1个 | ✅ 100% |
| 成交记录 | 1个 | 1个 | ✅ 100% |
| 行情数据 | 2个 | 4个 | ✅ 100% |
| 仪表盘 | 1个 | 3个 | ✅ 100% |
| 合约管理 | 1个 | 6个 | ✅ 100% |
| 结算管理 | 1个 | 5个 | ✅ 100% |
| 风控监控 | 1个 | 3个 | ⚠️ 前端完成 |
| 账户管理（管理端） | 1个 | 4个 | ✅ 100% |
| 交易管理 | 1个 | 2个 | ✅ 100% |
| 系统监控 | 1个 | 6个 | ✅ 100% |
| WebSocket | - | 8个 | ✅ 100% |
| **总计** | **17个页面** | **42个API** | **✅ 95%** |

---

## API 分类统计

### HTTP API (42个)
```
账户管理:    6个 ✅
订单管理:    5个 ✅
持仓管理:    1个 ✅
合约管理:    6个 ✅
结算管理:    5个 ✅
风控管理:    3个 ⚠️
市场数据:    5个 ✅
系统监控:    6个 ✅
认证管理:    3个 ✅
系统:        2个 ✅
```

### WebSocket 消息 (8个)
```
客户端→服务端: 4个 ✅
服务端→客户端: 7个 ✅
```

---

## 技术栈

### 后端
- **框架**: Actix-web 4.4
- **语言**: Rust 1.91.0
- **核心库**: qars (../qars2)
- **并发**: Tokio + DashMap
- **存储**: WAL + MemTable + SSTable

### 前端
- **框架**: Vue 2.6.11
- **UI库**: Element UI + vxe-table
- **图表**: ECharts + TradingView
- **路由**: Vue Router
- **HTTP**: Axios

---

**文档版本**: 1.0
**最后更新**: 2025-10-05
**维护者**: QAExchange Team
