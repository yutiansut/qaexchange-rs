# QAExchange Web 前端实施计划

## 一、系统架构设计

### 1.1 用户角色划分

#### 交易用户 (Trader)
- 注册/登录
- 入金/出金
- 查看行情
- 下单/撤单
- 查看个人订单
- 查看个人成交
- 查看个人持仓
- 查看个人账户

#### 交易所管理员 (Admin)
- 标的管理（上市/下市）
- 全市场订单查看
- 全市场成交查看
- 订单簿查看
- 成交异动监控
- 系统状态监控
- 账户管理（所有账户）
- 数据分析报表

---

## 二、前端功能清单

### 2.1 用户端页面 (User Portal)

#### 2.1.1 认证模块
- [x] 登录页 `/login`
- [x] 注册页 `/register`
- [ ] 忘记密码
- [ ] 用户信息修改

#### 2.1.2 交易模块
- [x] 交易面板 `/trade` ✅
  - [x] 实时行情展示（Tick 数据，每 2 秒刷新）
  - [x] 订单簿（买卖五档/十档可切换）
  - [x] 下单面板（限价/市价）
  - [x] 快速撤单
  - [x] 当前委托列表
  - [x] 价格和数量快捷设置
  - [x] 预估金额和保证金计算
- [ ] K线图表 `/chart`
  - [ ] HQChart 集成
  - [ ] 多周期切换（1分/5分/15分/30分/日/周/月）
  - [ ] 技术指标（MA/MACD/KDJ/BOLL）
  - [ ] 画线工具

#### 2.1.3 资产模块
- [ ] 我的账户 `/account`
  - [ ] 资金概览
  - [ ] 资金明细
  - [ ] 出入金记录
- [ ] 入金 `/deposit`
- [ ] 出金 `/withdraw`

#### 2.1.4 订单持仓模块
- [x] 我的订单 `/orders` ✅
  - [x] 当日委托
  - [x] 按用户/合约筛选
  - [x] 订单状态展示
  - [x] 快速下单和撤单
- [x] 我的成交 `/trades` ✅
  - [x] 成交历史列表
  - [x] 按时间/合约筛选
  - [x] 成交统计（今日成交、成交金额、买卖笔数）
  - [ ] 导出功能
- [x] 我的持仓 `/positions` ✅
  - [x] 持仓列表
  - [x] 浮动盈亏计算
  - [x] 持仓统计汇总（总市值、总盈亏、盈亏比）
  - [x] 快速平仓功能

---

### 2.2 管理员端页面 (Admin Portal)

#### 2.2.1 监控仪表盘
- [x] 系统监控 `/admin/dashboard`
  - [x] 账户统计
  - [x] 订单统计
  - [x] 成交统计
  - [x] 存储统计
  - [ ] 实时更新（WebSocket）

#### 2.2.2 标的管理
- [ ] 合约管理 `/admin/instruments`
  - [ ] 合约列表
  - [ ] 上市新合约
  - [ ] 下市/停牌
  - [ ] 合约参数设置

#### 2.2.3 市场监控
- [ ] 全市场订单 `/admin/market-orders`
  - [ ] 实时订单流
  - [ ] 大额订单预警
  - [ ] 订单统计分析
- [ ] 全市场成交 `/admin/market-trades`
  - [ ] 实时成交流
  - [ ] 成交异动监控
  - [ ] 成交分析图表
- [ ] 订单簿查看 `/admin/orderbook`
  - [ ] 实时订单簿
  - [ ] 深度图
  - [ ] 买卖压力分析

#### 2.2.4 用户管理
- [x] 账户管理 `/admin/accounts`
  - [x] 所有账户列表
  - [x] 账户详情
  - [ ] 风险控制
  - [ ] 账户冻结/解冻
- [ ] 用户订单查询 `/admin/user-orders/:userId`
- [ ] 用户成交查询 `/admin/user-trades/:userId`
- [ ] 用户持仓查询 `/admin/user-positions/:userId`

#### 2.2.5 数据分析
- [ ] 交易分析 `/admin/analytics`
  - [ ] 成交量分析
  - [ ] 活跃度分析
  - [ ] 用户行为分析
- [ ] 报表中心 `/admin/reports`
  - [ ] 日报表
  - [ ] 月报表
  - [ ] 导出功能

---

## 三、后端缺失功能清单

### 3.1 认证授权 API
- [ ] `POST /api/auth/register` - 用户注册
- [ ] `POST /api/auth/login` - 用户登录
- [ ] `POST /api/auth/logout` - 用户登出
- [ ] `GET /api/auth/profile` - 获取用户信息
- [ ] `PUT /api/auth/profile` - 更新用户信息

### 3.2 行情数据 API ✅
- [x] `GET /api/market/instruments` - 获取所有合约
- [x] `GET /api/market/tick/:instrument` - 实时行情（Tick 数据）
- [x] `GET /api/market/orderbook/:instrument?depth=5` - 订单簿（买卖盘深度）
- [x] `GET /api/market/trades/:instrument?limit=20` - 最新成交（待实现完整版）
- [ ] `GET /api/market/kline/:instrument` - K线数据（需要 K 线聚合模块）

### 3.3 合约管理 API
- [x] `GET /api/market/instruments` - 获取所有合约（已实现）
- [ ] `POST /api/instruments` - 上市新合约（管理员）
- [ ] `PUT /api/instruments/:id` - 更新合约信息（管理员）
- [ ] `DELETE /api/instruments/:id` - 下市合约（管理员）

### 3.4 市场数据 API（管理员）✅
- [x] `GET /api/admin/market/order-stats` - 市场订单统计（总订单/买单/卖单）
- [ ] `GET /api/admin/orders` - 全市场订单列表（需要实现）
- [ ] `GET /api/admin/trades` - 全市场成交列表（需要从 TradeGateway 获取）

### 3.5 WebSocket 推送
- [ ] `ws://host:port/ws/market` - 行情推送
- [ ] `ws://host:port/ws/orderbook` - 订单簿推送
- [ ] `ws://host:port/ws/trades` - 成交推送
- [x] `ws://host:port/ws?user_id=xxx` - 用户订单/成交推送（已有）

---

## 四、技术方案

### 4.1 前端技术栈
- **框架**: Vue 2.6
- **UI组件**: Element UI 2.13
- **表格**: VXE Table 2.9
- **图表**: ECharts 4.9 + HQChart 1.1
- **状态管理**: Vuex 3.1
- **路由**: Vue Router 3.1
- **HTTP**: Axios 0.19
- **WebSocket**: vue-socket.io 3.0

### 4.2 后端技术方案

#### 4.2.1 架构原则：业务逻辑与网络层解耦 ⭐

**关键设计理念**：
> "网络部分是独立的，它只是调用模块的功能"

**实现方式**：
```
业务逻辑层 (src/market/)
    ↓ 提供服务接口
网络层 (src/service/http/market.rs)
    ↓ 处理 HTTP 请求/响应
客户端
```

**示例**：
- ✅ **业务逻辑**: `MarketDataService::get_orderbook_snapshot()` - 纯业务逻辑，不依赖网络框架
- ✅ **网络层**: `get_orderbook()` HTTP handler - 仅处理 HTTP 参数解析和响应格式化

**优势**：
1. 业务逻辑可复用（HTTP / WebSocket / gRPC）
2. 易于单元测试（无需启动 HTTP 服务器）
3. 关注点分离，代码可维护性高

#### 4.2.2 数据获取方案
- **订单簿数据**: 从 `ExchangeMatchingEngine` 获取实时买卖盘
- **行情数据**: 从撮合引擎获取最新成交价
- **K线数据**: 需要新增 K线聚合模块（基于成交数据）
- **认证**: JWT Token 或 Session（建议 JWT）

---

## 五、实施优先级

### Phase 1: 核心交易功能（高优先级）✅
1. [x] 基础框架搭建
2. [x] API 接口层
3. [x] 布局组件
4. [ ] 后端：订单簿 API
5. [ ] 后端：行情数据 API
6. [ ] 前端：交易面板
7. [ ] 前端：我的订单/持仓

### Phase 2: 图表与行情（高优先级）
1. [ ] 后端：K线数据聚合
2. [ ] 后端：K线数据 API
3. [ ] 前端：HQChart 集成
4. [ ] 前端：实时行情展示
5. [ ] WebSocket 行情推送

### Phase 3: 管理员功能（中优先级）
1. [ ] 后端：合约管理 API
2. [ ] 后端：全市场数据 API
3. [ ] 前端：管理员仪表盘增强
4. [ ] 前端：合约管理页面
5. [ ] 前端：市场监控页面

### Phase 4: 认证与用户中心（中优先级）
1. [ ] 后端：认证系统
2. [ ] 前端：登录/注册页
3. [ ] 前端：用户中心
4. [ ] 前端：权限控制

### Phase 5: 数据分析与报表（低优先级）
1. [ ] 后端：数据分析 API
2. [ ] 前端：分析图表
3. [ ] 前端：报表导出

---

## 六、开发时间估算

| 阶段 | 功能 | 预计时间 |
|-----|------|---------|
| Phase 1 | 核心交易功能 | 2-3天 |
| Phase 2 | 图表与行情 | 2-3天 |
| Phase 3 | 管理员功能 | 2-3天 |
| Phase 4 | 认证与用户 | 1-2天 |
| Phase 5 | 数据分析 | 1-2天 |
| **总计** | | **8-13天** |

---

## 七、当前进度

### 已完成 ✅

#### 后端实现
- [x] **后端市场数据 API（解耦架构）**
  - [x] 业务逻辑层：`src/market/mod.rs` - MarketDataService
  - [x] 网络层：`src/service/http/market.rs` - HTTP 处理器
  - [x] 订单簿查询 API (`GET /api/market/orderbook/:id?depth=5`)
  - [x] 合约列表 API (`GET /api/market/instruments`)
  - [x] Tick 行情 API (`GET /api/market/tick/:id`)
  - [x] 最近成交 API (`GET /api/market/trades/:id?limit=20`)
  - [x] 市场订单统计 API (`GET /api/admin/market/order-stats`)

#### 前端实现
- [x] 项目基础结构（Vue2 + Element UI + VXE Table + ECharts）
- [x] API 接口层（`src/api/index.js` - 完整 API 封装）
- [x] 布局组件（`src/layout/index.vue` - 顶部导航 + 用户切换）
- [x] **监控仪表盘** (`/dashboard`)
  - [x] 6 个统计卡片（账户、订单、成交、存储）
  - [x] 3 个可视化图表（ECharts）
  - [x] 自动刷新（每 10 秒）
- [x] **账户管理页面** (`/accounts`)
  - [x] 账户列表展示（VXE Table）
  - [x] 开户/入金/出金功能
  - [x] 账户查询和筛选
- [x] **订单管理页面** (`/orders`)
  - [x] 订单列表展示
  - [x] 下单对话框（完整表单）
  - [x] 撤单功能
  - [x] 按用户/合约筛选
- [x] **持仓管理页面** (`/positions`) ⭐
  - [x] 持仓列表展示
  - [x] 持仓统计汇总（总市值、浮动盈亏、盈亏比）
  - [x] 快速平仓对话框
  - [x] 盈亏颜色标识
- [x] **成交记录页面** (`/trades`) ⭐
  - [x] 成交历史列表
  - [x] 成交统计（今日成交、成交金额、买卖笔数）
  - [x] 按时间/合约筛选
  - [x] 导出按钮（待接入）
- [x] **核心交易面板** (`/trade`) ⭐⭐⭐
  - [x] 合约选择器（下拉选择）
  - [x] 实时行情显示（最新价、涨跌幅、买一卖一）
  - [x] 订单簿展示（买卖五档/十档，depth bar 可视化）
  - [x] 点击价格快速填充
  - [x] 下单面板（买入/卖出/平仓三个标签页）
  - [x] 限价单和市价单支持
  - [x] 价格快捷调整（±1/±2/±5）
  - [x] 数量快捷设置（1/5/10/20 手）
  - [x] 预估金额和保证金计算
  - [x] 当前委托列表（实时刷新）
  - [x] 快速撤单功能
  - [x] 自动刷新机制（每 2 秒）

#### 文档
- [x] 实施计划文档 (`IMPLEMENTATION_PLAN.md`)
- [x] 市场数据 API 文档 (`MARKET_API_DOCUMENTATION.md`)
- [x] 前端 README (`README.md`)
- [x] 测试指南 (`TESTING_GUIDE.md`)
- [x] 启动脚本 (`start_dev.sh`)

### 待完成 📋
- [ ] HQChart K 线图表集成
- [ ] WebSocket 实时推送（行情/订单簿）
- [ ] 认证系统（JWT Token）
- [ ] 后端 K 线数据聚合模块
- [ ] 合约管理页面（管理员上市/下市）
- [ ] 用户权限控制（普通用户 vs 管理员）
