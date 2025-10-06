# QAExchange Web 前端完善计划

## 📋 现状分析

### ✅ 已有功能（用户端）
1. **监控仪表盘** (`/dashboard`) - 系统概览和统计
2. **交易面板** (`/trade`) - 实时行情、订单簿、下单、撤单
3. **账户管理** (`/accounts`) - 开户、入金、出金、查询
4. **订单管理** (`/orders`) - 订单列表、查询、撤单
5. **持仓管理** (`/positions`) - 持仓列表、平仓
6. **成交记录** (`/trades`) - 成交历史查询

### 🔍 发现的缺口

#### 1. 数据格式问题
- ❌ 未集成 QIFI 标准数据格式
- ❌ 账户、持仓、订单、成交数据未使用 QIFI 协议
- ✅ qaotcweb 有完整的 QIFI 组件库可复用

#### 2. 管理端功能缺失
- ❌ 无合约管理（上市/下市/修改合约参数）
- ❌ 无风控监控（实时风险预警、强平记录）
- ❌ 无结算管理（日终结算、结算价设置）
- ❌ 无系统配置（交易时间、费率、保证金率）
- ✅ 只有简单的系统监控页面

#### 3. 用户端增强空间
- ❌ K线图表未实现（HQChart 已引入但未集成）
- ❌ 无资金曲线和账户分析
- ❌ 无交易报表和统计
- ❌ 无 WebSocket 实时推送（使用轮询）

---

## 🎯 完善计划

### 阶段一：QIFI 数据集成（优先级：高）

#### 1. 创建 QIFI 工具类
**文件**: `web/src/utils/qifi.js`

复用 qaotcweb 的 QIFI 处理逻辑：
```javascript
// 基于 /home/quantaxis/qapro/qaotcweb/src/components/qifi/libs/js/qifi.js
export class QifiAccount {
  // 解析 QIFI 账户数据
  static parseAccount(data) { }

  // 获取账户信息
  static getAccountInfo(qifiData) { }

  // 获取持仓列表
  static getPositions(qifiData) { }

  // 获取订单列表
  static getOrders(qifiData) { }

  // 获取成交列表
  static getTrades(qifiData) { }
}

// 行情五档数据处理
export class QifiQuotation {
  static parseFull(data) { }
}
```

#### 2. 后端 QIFI API 增强
**需要后端提供**:
```
GET  /api/qifi/account/:userId      # 返回 QIFI 格式账户数据
GET  /api/qifi/positions/:userId    # 返回 QIFI 格式持仓数据
GET  /api/qifi/orders/:userId       # 返回 QIFI 格式订单数据
GET  /api/qifi/trades/:userId       # 返回 QIFI 格式成交数据
```

#### 3. 前端组件改造
将现有组件改为使用 QIFI 数据格式：
- `web/src/views/accounts/index.vue` - 使用 QifiAccount.getAccountInfo()
- `web/src/views/positions/index.vue` - 使用 QifiAccount.getPositions()
- `web/src/views/orders/index.vue` - 使用 QifiAccount.getOrders()
- `web/src/views/trades/index.vue` - 使用 QifiAccount.getTrades()

---

### 阶段二：管理端核心功能（优先级：高）

#### 1. 合约管理页面
**文件**: `web/src/views/admin/instruments.vue`

**功能**:
- ✅ 合约列表展示（表格）
  - 合约代码、名称、类型、交易所
  - 合约乘数、最小变动价位、保证金率
  - 状态（正常/停牌/暂停交易/已下市）
  - 上市日期、到期日期
- ✅ 合约上市功能
  - 表单：合约代码、名称、类型、交易所、合约乘数等
  - 验证：代码唯一性、参数合法性
- ✅ 合约下市功能
  - 确认对话框
  - 检查是否有未平仓持仓
- ✅ 合约参数修改
  - 保证金率、手续费率、涨跌停板
  - 交易时间段

**API 需求**:
```
GET     /api/admin/instruments              # 获取所有合约
POST    /api/admin/instrument/create        # 上市新合约
PUT     /api/admin/instrument/:id/update    # 修改合约参数
DELETE  /api/admin/instrument/:id/delist    # 下市合约
PUT     /api/admin/instrument/:id/suspend   # 暂停交易
PUT     /api/admin/instrument/:id/resume    # 恢复交易
```

#### 2. 风控监控页面
**文件**: `web/src/views/admin/risk.vue`

**功能**:
- ✅ 实时风险监控
  - 账户列表（用户ID、权益、保证金、可用资金、风险率）
  - 风险率颜色预警（>80% 橙色，>90% 红色）
  - 按风险率排序
- ✅ 风险统计卡片
  - 高风险账户数（>80%）
  - 临近爆仓账户数（>90%）
  - 今日强平次数
  - 平均风险率
- ✅ 强平记录
  - 强平时间、用户ID、强平前权益、亏损金额
  - 强平合约、强平价格、强平数量
- ✅ 风险操作
  - 手动强平按钮（管理员权限）
  - 风险预警通知（WebSocket 推送）

**API 需求**:
```
GET  /api/admin/risk/accounts          # 获取所有账户风险信息
GET  /api/admin/risk/high-risk         # 获取高风险账户（>80%）
GET  /api/admin/risk/liquidations      # 获取强平记录
POST /api/admin/risk/force-liquidate   # 手动强平账户
```

#### 3. 结算管理页面
**文件**: `web/src/views/admin/settlement.vue`

**功能**:
- ✅ 日终结算操作
  - 结算日期选择
  - 结算价设置（按合约）
    - 表单：合约代码、结算价
    - 批量导入（CSV/Excel）
  - 执行结算按钮
    - 确认对话框（显示影响账户数）
    - 结算进度条
- ✅ 结算历史
  - 结算日期、合约数、账户数、总盈亏、总手续费
  - 结算状态（成功/失败/部分成功）
  - 结算详情（点击查看各账户结算结果）
- ✅ 结算统计
  - 本月结算次数
  - 总盈利账户数 / 总亏损账户数
  - 总手续费收入
  - 平均盈亏比

**API 需求**:
```
POST /api/admin/settlement/set-price           # 设置结算价
POST /api/admin/settlement/execute             # 执行日终结算
GET  /api/admin/settlement/history             # 获取结算历史
GET  /api/admin/settlement/detail/:date        # 获取结算详情
```

#### 4. 系统配置页面
**文件**: `web/src/views/admin/config.vue`

**功能**:
- ✅ 交易时间配置
  - 开盘时间、收盘时间
  - 夜盘时间、午休时间
  - 节假日设置
- ✅ 费率配置
  - 手续费率（按合约）
  - 保证金率（按合约）
  - 滑点设置
- ✅ 风控参数
  - 最大风险率（强平线）
  - 风险预警线（80%、90%）
  - 单账户最大持仓限制
  - 单合约最大持仓限制
- ✅ 系统参数
  - 默认初始资金
  - 最小入金/出金额度
  - WebSocket 推送频率

**API 需求**:
```
GET  /api/admin/config                 # 获取所有配置
PUT  /api/admin/config/trading-hours   # 更新交易时间
PUT  /api/admin/config/fees            # 更新费率
PUT  /api/admin/config/risk            # 更新风控参数
PUT  /api/admin/config/system          # 更新系统参数
```

---

### 阶段三：用户端增强（优先级：中）

#### 1. 账户资金曲线页面
**文件**: `web/src/views/user/account-curve.vue`

**功能**:
- ✅ 权益曲线图（ECharts 折线图）
  - 时间范围选择（今日/本周/本月/全部）
  - 数据点：日期、权益、可用资金、保证金
- ✅ 收益统计卡片
  - 累计收益、累计收益率
  - 最大回撤、最大回撤率
  - 盈利天数、亏损天数
  - 平均日收益
- ✅ 分析指标
  - 夏普比率
  - 最大连续盈利天数、最大连续亏损天数
  - 盈亏比（平均盈利/平均亏损）

**API 需求**:
```
GET  /api/user/equity-curve/:userId?start=&end=  # 获取权益曲线数据
GET  /api/user/statistics/:userId                # 获取统计指标
```

#### 2. 交易报表页面
**文件**: `web/src/views/user/reports.vue`

**功能**:
- ✅ 日报表
  - 日期、开盘权益、收盘权益、盈亏、盈亏率
  - 交易笔数、成交手数、手续费
  - 最大持仓、最大亏损
- ✅ 合约分析
  - 按合约分组统计
  - 合约代码、交易次数、盈亏、胜率
  - 饼图展示各合约盈亏占比
- ✅ 导出功能
  - 导出为 Excel
  - 导出为 PDF 报告

**API 需求**:
```
GET  /api/user/daily-reports/:userId?start=&end=    # 获取日报表
GET  /api/user/instrument-reports/:userId           # 获取合约分析
GET  /api/user/export/:userId?format=excel|pdf      # 导出报表
```

#### 3. K线图表页面（完善现有）
**文件**: `web/src/views/chart/index.vue`（已存在，需完善）

**功能**:
- ✅ HQChart 集成
  - 多周期K线（1分/5分/15分/30分/60分/日/周/月）
  - 主图指标（MA/BOLL/SAR）
  - 副图指标（MACD/KDJ/RSI/VOL）
- ✅ 成交标记
  - 在K线上标注用户的买入/卖出点位
  - 不同颜色区分买入/卖出
  - 点击标记显示订单详情
- ✅ 画线工具
  - 趋势线、水平线、矩形、斐波那契回调
  - 支持保存和加载画线

**依赖**:
- HQChart 已在 package.json 中引入
- 参考 qaotcweb 的 QIFI 图表组件

---

### 阶段四：实时推送（优先级：中）

#### 1. WebSocket 集成
**文件**: `web/src/utils/websocket.js`

**功能**:
- ✅ WebSocket 连接管理
  - 自动重连机制
  - 心跳检测
- ✅ 订阅主题
  - 账户更新（account_update）
  - 订单状态（order_status）
  - 成交通知（trade）
  - 持仓变化（position_update）
  - 行情推送（market_data）
- ✅ Vuex 集成
  - 接收 WebSocket 消息自动更新 store
  - 组件通过 store 获取实时数据

**后端 WebSocket 协议**:
```
客户端 → 服务端：
{
  "topic": "subscribe",
  "channels": ["account_update", "order_status", "trade"],
  "user_id": "user1"
}

服务端 → 客户端：
{
  "topic": "order_status",
  "data": { ... }
}
```

---

## 📅 实施时间表

### 第 1-2 天：QIFI 集成
- [x] 创建 QIFI 工具类
- [ ] 后端提供 QIFI API（可选，现有 API 也可转换）
- [ ] 改造前端组件使用 QIFI 格式

### 第 3-5 天：管理端核心功能
- [ ] 合约管理页面
- [ ] 风控监控页面
- [ ] 结算管理页面

### 第 6-7 天：用户端增强
- [ ] 账户资金曲线页面
- [ ] 交易报表页面
- [ ] K线图表完善（HQChart 集成）

### 第 8 天：WebSocket 实时推送
- [ ] WebSocket 客户端封装
- [ ] Vuex 集成
- [ ] 页面实时更新

---

## 🗂️ 新增文件清单

### 工具类
```
web/src/utils/
├── qifi.js              # QIFI 数据处理工具类（复用 qaotcweb）
├── websocket.js         # WebSocket 客户端封装
└── export.js            # 报表导出工具（Excel/PDF）
```

### 管理端页面
```
web/src/views/admin/
├── instruments.vue      # 合约管理
├── risk.vue             # 风控监控
├── settlement.vue       # 结算管理
├── config.vue           # 系统配置
└── components/
    ├── InstrumentForm.vue       # 合约表单组件
    ├── RiskAccountTable.vue     # 风险账户表格
    ├── LiquidationHistory.vue   # 强平记录
    └── SettlementPriceForm.vue  # 结算价设置表单
```

### 用户端页面
```
web/src/views/user/
├── account-curve.vue    # 账户资金曲线
├── reports.vue          # 交易报表
└── components/
    ├── EquityCurveChart.vue     # 权益曲线图
    ├── StatisticsCard.vue       # 统计卡片
    └── InstrumentAnalysis.vue   # 合约分析
```

### 路由配置
```javascript
// web/src/router/index.js 需要新增：
{
  path: '/admin',
  children: [
    { path: 'instruments', component: () => import('@/views/admin/instruments.vue') },
    { path: 'risk', component: () => import('@/views/admin/risk.vue') },
    { path: 'settlement', component: () => import('@/views/admin/settlement.vue') },
    { path: 'config', component: () => import('@/views/admin/config.vue') }
  ]
},
{
  path: '/user',
  children: [
    { path: 'account-curve', component: () => import('@/views/user/account-curve.vue') },
    { path: 'reports', component: () => import('@/views/user/reports.vue') }
  ]
}
```

---

## 🎨 UI/UX 改进建议

### 1. 菜单结构优化
```
当前菜单（平铺）：
- 监控仪表盘
- 交易面板
- 账户管理
- 订单管理
- 持仓管理
- 成交记录
- K线图表
- 系统监控

建议菜单（分组）：
📊 交易中心
  - 交易面板
  - K线图表
  - 我的账户
  - 我的订单
  - 我的持仓
  - 成交记录

📈 数据分析
  - 资金曲线
  - 交易报表
  - 统计分析

⚙️ 管理中心（管理员）
  - 系统监控
  - 合约管理
  - 风控监控
  - 结算管理
  - 系统配置
  - 用户管理
```

### 2. 角色权限控制
```javascript
// web/src/store/modules/user.js
state: {
  role: 'user', // 'user' | 'admin' | 'risk_manager'
  permissions: []
}

// 路由守卫
router.beforeEach((to, from, next) => {
  if (to.path.startsWith('/admin') && store.state.user.role !== 'admin') {
    next('/403')
  }
  next()
})
```

---

## 📊 后端 API 需求总结

### 新增 API（需要后端实现）

#### QIFI 格式 API（可选）
```
GET  /api/qifi/account/:userId
GET  /api/qifi/positions/:userId
GET  /api/qifi/orders/:userId
GET  /api/qifi/trades/:userId
```

#### 管理端 API
```
# 合约管理
GET     /api/admin/instruments
POST    /api/admin/instrument/create
PUT     /api/admin/instrument/:id/update
DELETE  /api/admin/instrument/:id/delist
PUT     /api/admin/instrument/:id/suspend
PUT     /api/admin/instrument/:id/resume

# 风控监控
GET  /api/admin/risk/accounts
GET  /api/admin/risk/high-risk
GET  /api/admin/risk/liquidations
POST /api/admin/risk/force-liquidate

# 结算管理
POST /api/admin/settlement/set-price
POST /api/admin/settlement/execute
GET  /api/admin/settlement/history
GET  /api/admin/settlement/detail/:date

# 系统配置
GET  /api/admin/config
PUT  /api/admin/config/trading-hours
PUT  /api/admin/config/fees
PUT  /api/admin/config/risk
PUT  /api/admin/config/system
```

#### 用户端 API
```
GET  /api/user/equity-curve/:userId?start=&end=
GET  /api/user/statistics/:userId
GET  /api/user/daily-reports/:userId?start=&end=
GET  /api/user/instrument-reports/:userId
GET  /api/user/export/:userId?format=excel|pdf
```

#### WebSocket 主题
```
account_update      # 账户更新
order_status        # 订单状态
trade               # 成交通知
position_update     # 持仓变化
market_data         # 行情推送
```

---

## ✅ 验收标准

### 功能完整性
- [ ] 所有页面可正常访问
- [ ] 所有 API 调用成功返回数据
- [ ] 表单验证完整
- [ ] 错误处理友好

### 数据一致性
- [ ] QIFI 格式数据解析正确
- [ ] 前后端数据字段匹配
- [ ] 实时推送数据同步

### 性能要求
- [ ] 页面首屏加载 < 2s
- [ ] API 响应时间 < 500ms
- [ ] 表格支持虚拟滚动（1000+ 行）
- [ ] WebSocket 消息延迟 < 100ms

### 兼容性
- [ ] Chrome/Firefox/Safari 最新版
- [ ] 响应式布局（>=1280px）
- [ ] 支持深色模式（可选）

---

## 🚀 快速开始

### 1. 创建 QIFI 工具类
```bash
# 复制 qaotcweb 的 QIFI 实现
cp /home/quantaxis/qapro/qaotcweb/src/components/qifi/libs/js/qifi.js \
   /home/quantaxis/qaexchange-rs/web/src/utils/qifi.js
```

### 2. 创建管理端目录
```bash
cd /home/quantaxis/qaexchange-rs/web/src/views
mkdir -p admin/components
mkdir -p user/components
```

### 3. 安装额外依赖（如需要）
```bash
cd /home/quantaxis/qaexchange-rs/web
npm install --save xlsx  # Excel 导出
npm install --save jspdf  # PDF 导出
```

---

## 📝 总结

本计划将 QAExchange Web 前端从**基础交易系统**升级为**完整的交易+管理平台**，包括：

1. **QIFI 标准集成** - 统一数据格式，便于对接多种系统
2. **管理端功能** - 合约管理、风控、结算、配置
3. **用户端增强** - 资金曲线、报表、K线图表
4. **实时推送** - WebSocket 替代轮询

预计开发周期：**8 个工作日**
代码量增加：约 **5000+ 行**（Vue 组件 + API + 工具类）

---

**下一步行动**：
1. 确认后端 API 开发计划（哪些已有、哪些需要新增）
2. 开始实施阶段一（QIFI 集成）
3. 并行开发管理端和用户端增强功能
