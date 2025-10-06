# 前后端数据对接实现清单

## 📊 整体进度
- ✅ 已完成: 4/15
- 🚧 进行中: 0/15
- ❌ 待实现: 11/15

---

## ✅ 已完成功能

### 1. 用户认证系统
- ✅ **后端**: `/api/auth/register` - 用户注册
- ✅ **后端**: `/api/auth/login` - 用户登录
- ✅ **后端**: `/api/auth/user/{user_id}` - 获取用户信息
- ✅ **前端**: `views/login.vue` - 登录页面已对接
- ✅ **前端**: `views/register.vue` - 注册页面已对接

### 2. 系统监控
- ✅ **后端**: `/api/monitoring/system` - 系统监控数据
- ✅ **前端**: `views/dashboard/index.vue` - 监控面板已对接
- ✅ **前端**: `views/monitoring/index.vue` - 详细监控已对接

### 3. 管理端功能（已对接API）
- ✅ **后端**: 合约管理API完整
- ✅ **后端**: 结算管理API完整
- ✅ **后端**: 风控监控API完整
- ✅ **后端**: 账户管理API完整
- ✅ **前端**: `views/admin/*` - 管理页面已对接

---

## ❌ 待实现功能

### 4. 用户账户管理 - accounts/index.vue
**状态**: ❌ 部分对接，缺少开户/入金/出金功能

#### 后端API检查:
- ✅ `POST /api/account/open` - 开户 (已实现)
- ✅ `GET /api/account/{user_id}` - 查询账户 (已实现)
- ✅ `POST /api/account/deposit` - 入金 (已实现)
- ✅ `POST /api/account/withdraw` - 出金 (已实现)

#### 前端问题:
- ❌ 页面调用 `queryAccount(userId)` 但当前用户未传递
- ❌ 开户对话框功能不完整
- ❌ 入金/出金对话框缺失
- ❌ 需要集成Vuex currentUser状态

#### 需要实现:
1. 修改 `handleQuery()` 使用当前登录用户ID
2. 完善开户对话框，调用 `openAccount` API
3. 添加入金/出金对话框
4. 实时刷新账户数据

---

### 5. 订单管理 - orders/index.vue
**状态**: ❌ 未对接，显示假数据

#### 后端API检查:
- ✅ `POST /api/order/submit` - 提交订单 (已实现)
- ✅ `POST /api/order/cancel` - 撤单 (已实现)
- ✅ `GET /api/order/{order_id}` - 查询订单 (已实现)
- ✅ `GET /api/order/user/{user_id}` - 查询用户订单 (已实现)

#### 前端问题:
- ❌ 完全使用模拟数据
- ❌ 未调用任何后端API
- ❌ 撤单功能未实现

#### 需要实现:
1. 调用 `queryUserOrders(currentUser)` 获取订单列表
2. 实现撤单功能，调用 `cancelOrder` API
3. 添加订单状态筛选
4. 添加自动刷新机制

---

### 6. 持仓管理 - positions/index.vue
**状态**: ❌ 未对接，显示假数据

#### 后端API检查:
- ✅ `GET /api/position/{user_id}` - 查询持仓 (已实现)

#### 前端问题:
- ❌ 完全使用模拟数据
- ❌ 未调用任何后端API

#### 需要实现:
1. 调用 `queryPosition(currentUser)` 获取持仓数据
2. 实时显示持仓盈亏
3. 添加平仓功能（需要后端实现平仓API）
4. 添加持仓汇总统计

---

### 7. 成交记录 - trades/index.vue
**状态**: ❌ 未对接，显示假数据

#### 后端API检查:
- ❌ 缺少 `GET /api/trades/user/{user_id}` API

#### 前端问题:
- ❌ 完全使用模拟数据
- ❌ 后端缺少成交记录查询API

#### 需要实现:
**后端**:
1. 实现 `GET /api/trades/user/{user_id}` 查询用户成交记录
2. 支持分页、筛选（时间范围、合约）

**前端**:
1. 调用成交记录API
2. 添加时间筛选器
3. 添加合约筛选器
4. 计算成交汇总统计

---

### 8. 交易面板 - trade/index.vue
**状态**: ❌ 部分对接，缺少关键功能

#### 后端API检查:
- ✅ `POST /api/order/submit` - 提交订单 (已实现)
- ✅ `GET /api/market/instruments` - 获取合约列表 (已实现)
- ✅ `GET /api/market/orderbook/{instrument_id}` - 获取订单簿 (已实现)
- ❌ WebSocket实时行情 (需要连接)

#### 前端问题:
- ❌ 下单功能未完整实现
- ❌ 未显示实时订单簿
- ❌ 未显示实时成交
- ❌ WebSocket未连接

#### 需要实现:
**后端**:
1. 确保WebSocket服务正常运行

**前端**:
1. 实现下单表单，调用 `submitOrder` API
2. 连接WebSocket获取实时行情
3. 显示订单簿深度数据
4. 显示最近成交记录
5. 添加快速下单功能

---

### 9. K线图表 - chart/index.vue
**状态**: ❌ 未实现，显示占位符

#### 后端API检查:
- ❌ 缺少 K线数据API
- ❌ 缺少历史行情数据API

#### 需要实现:
**后端**:
1. 实现 `GET /api/market/kline/{instrument_id}` K线数据API
2. 支持多种周期（1分钟、5分钟、15分钟、1小时、日线）
3. 支持历史数据查询

**前端**:
1. 集成 ECharts 或 TradingView
2. 调用K线数据API
3. 实时更新最新K线
4. 支持周期切换
5. 添加技术指标（均线、MACD、KDJ等）

---

### 10. 资金曲线 - user/account-curve.vue
**状态**: ❌ 未对接，显示假数据

#### 后端API检查:
- ❌ 缺少账户历史曲线数据API

#### 需要实现:
**后端**:
1. 实现 `GET /api/account/{user_id}/curve` 账户权益曲线API
2. 返回每日权益、可用资金、保证金历史数据
3. 支持时间范围筛选

**前端**:
1. 调用曲线数据API
2. 使用 ECharts 绘制曲线图
3. 显示收益率统计
4. 添加时间范围选择器

---

## 🔧 需要新增的后端API

### 1. 成交记录查询
```
GET /api/trades/user/{user_id}
参数: page, size, start_date, end_date, instrument_id
返回: 成交列表、总数、汇总统计
```

### 2. K线数据查询
```
GET /api/market/kline/{instrument_id}
参数: period (1m/5m/15m/1h/1d), start_time, end_time, limit
返回: OHLCV数据数组
```

### 3. 账户权益曲线
```
GET /api/account/{user_id}/curve
参数: start_date, end_date
返回: 每日权益、可用资金、保证金数据
```

### 4. 平仓功能
```
POST /api/order/close
参数: user_id, instrument_id, direction, volume
返回: 平仓结果
```

---

## 📋 实现优先级

### P0 - 核心交易功能（立即实现）
1. ✅ 用户认证系统（已完成）
2. **订单管理** - 查询、撤单
3. **持仓管理** - 查询、平仓
4. **交易面板** - 下单功能

### P1 - 重要功能（本周完成）
5. **成交记录** - 历史查询
6. **账户管理** - 开户、入金、出金
7. **WebSocket实时行情** - 订单簿、成交

### P2 - 增强功能（下周完成）
8. **K线图表** - 历史K线、实时更新
9. **资金曲线** - 权益曲线、收益统计
10. **风控监控** - 实时风险指标

---

## 🚀 快速开始实现步骤

### 第一步：修复账户页面（立即执行）
```javascript
// 1. 修改 accounts/index.vue
methods: {
  async fetchAccount() {
    const userId = this.$store.getters.currentUser
    const data = await queryAccount(userId)
    this.accountList = [data] // 显示当前用户账户
  }
}
```

### 第二步：实现订单管理（优先）
```javascript
// 2. 修改 orders/index.vue
async fetchOrders() {
  const userId = this.$store.getters.currentUser
  const data = await queryUserOrders(userId)
  this.orderList = data
}

async handleCancel(orderId) {
  await cancelOrder({ order_id: orderId })
  this.$message.success('撤单成功')
  this.fetchOrders()
}
```

### 第三步：实现持仓管理
```javascript
// 3. 修改 positions/index.vue
async fetchPositions() {
  const userId = this.$store.getters.currentUser
  const data = await queryPosition(userId)
  this.positionList = data.positions || []
}
```

---

**文档创建时间**: 2025-10-04
**状态**: 待逐一实现
