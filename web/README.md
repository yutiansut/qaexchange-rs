# QAExchange Web 前端

基于 Vue2 + Element UI + VXE Table + ECharts 的交易所监控和交易系统前端。

## 技术栈

- **框架**: Vue 2.6.14
- **UI组件**: Element UI 2.13.2
- **表格**: VXE Table 2.9.2
- **图表**: ECharts 4.9.0 + HQChart 1.1.0 (待集成)
- **状态管理**: Vuex 3.1.3
- **路由**: Vue Router 3.1.7
- **HTTP**: Axios 0.19.2
- **日期处理**: dayjs 1.8.17

## 项目结构

```
web/
├── public/              # 静态资源
├── src/
│   ├── api/            # API 接口定义
│   ├── assets/         # 资源文件
│   ├── layout/         # 布局组件
│   ├── router/         # 路由配置
│   ├── store/          # Vuex 状态管理
│   ├── views/          # 页面组件
│   │   ├── dashboard/  # 监控仪表盘
│   │   ├── trade/      # 交易面板（核心功能）
│   │   ├── accounts/   # 账户管理
│   │   ├── orders/     # 订单管理
│   │   ├── positions/  # 持仓管理
│   │   ├── trades/     # 成交记录
│   │   └── ...
│   ├── App.vue         # 根组件
│   └── main.js         # 入口文件
├── package.json        # 依赖配置
└── vue.config.js       # Vue CLI 配置
```

## 快速开始

### 1. 安装依赖

```bash
cd /home/quantaxis/qaexchange-rs/web
npm install
```

### 2. 启动后端服务

在另一个终端窗口：

```bash
cd /home/quantaxis/qaexchange-rs
cargo run --bin qaexchange-server
```

后端将在 `http://127.0.0.1:8094` 启动

### 3. 启动前端开发服务器

```bash
npm run dev
```

前端将在 `http://localhost:8096` 启动，并自动打开浏览器。

## 功能模块

### ✅ 已完成

#### 1. 监控仪表盘 (`/dashboard`)
- 系统统计卡片（账户数、订单数、成交数等）
- 实时数据可视化图表（ECharts）
- 自动刷新机制（每 10 秒）

#### 2. 交易面板 (`/trade`) ⭐ 核心功能
- **实时行情显示**
  - 最新价、买一价、卖一价
  - 涨跌幅实时计算
  - 每 2 秒自动刷新

- **订单簿展示**
  - 买卖五档/十档可切换
  - 深度可视化（volume bar）
  - 点击价格快速下单

- **下单面板**
  - 买入/卖出/平仓三个标签页
  - 限价单/市价单支持
  - 价格快捷调整（±1/±2/±5）
  - 数量快捷设置（1/5/10/20手）
  - 预估金额和保证金计算

- **当前委托**
  - 实时委托列表
  - 快速撤单功能

#### 3. 账户管理 (`/accounts`)
- 账户列表展示
- 开户/入金/出金功能
- 账户余额和风险率监控

#### 4. 订单管理 (`/orders`)
- 订单列表查询
- 按用户/合约筛选
- 订单提交和撤单
- 订单状态实时更新

#### 5. 持仓管理 (`/positions`)
- 持仓列表展示
- 浮动盈亏计算
- 持仓统计汇总
- 快速平仓功能

#### 6. 成交记录 (`/trades`)
- 成交历史查询
- 按时间/合约筛选
- 成交统计分析
- 导出功能（待实现）

### 📋 待完成

#### 7. K线图表 (`/chart`)
- HQChart 集成
- 多周期K线（1分/5分/15分/30分/日/周/月）
- 技术指标（MA/MACD/KDJ/BOLL）
- 画线工具

## API 对接

前端通过以下 API 与后端通信：

### 账户相关
```
POST   /api/account/open        # 开户
GET    /api/account/:userId     # 查询账户
POST   /api/account/deposit     # 入金
POST   /api/account/withdraw    # 出金
```

### 订单相关
```
POST   /api/order/submit        # 提交订单
POST   /api/order/cancel        # 撤单
GET    /api/order/:orderId      # 查询订单
GET    /api/order/user/:userId  # 查询用户订单
```

### 持仓相关
```
GET    /api/position/:userId    # 查询持仓
```

### 市场数据 ⭐ 新增
```
GET    /api/market/instruments                  # 获取合约列表
GET    /api/market/orderbook/:instrument?depth=5  # 获取订单簿
GET    /api/market/tick/:instrument             # 获取Tick行情
GET    /api/market/trades/:instrument?limit=20  # 获取最近成交
```

### 管理员接口
```
GET    /api/monitoring/system                  # 系统监控
GET    /api/monitoring/accounts                # 账户统计
GET    /api/monitoring/orders                  # 订单统计
GET    /api/monitoring/trades                  # 成交统计
GET    /api/monitoring/storage                 # 存储统计
GET    /api/admin/market/order-stats           # 市场订单统计
```

## 开发指南

### 添加新页面

1. 在 `src/views/` 创建页面组件
2. 在 `src/router/index.js` 添加路由配置
3. 在 `src/layout/index.vue` 添加菜单项
4. 在 `src/api/index.js` 添加 API 函数（如需要）

### 调用 API

```javascript
import { getTick, getOrderBook } from '@/api'

// 在组件中使用
async loadData() {
  try {
    const tick = await getTick('IF2501')
    const orderbook = await getOrderBook('IF2501', 5)
    console.log(tick, orderbook)
  } catch (error) {
    this.$message.error('加载失败')
  }
}
```

### 状态管理

全局状态通过 Vuex 管理：

```javascript
// 在组件中使用
import { mapState, mapActions } from 'vuex'

export default {
  computed: {
    ...mapState(['currentUser', 'monitoring'])
  },
  methods: {
    ...mapActions(['setCurrentUser', 'startAutoRefresh'])
  }
}
```

## 构建部署

### 开发环境
```bash
npm run dev
```

### 生产构建
```bash
npm run build
```

构建产物在 `dist/` 目录，可直接部署到静态服务器。

### 配置代理

在 `vue.config.js` 中配置开发代理：

```javascript
module.exports = {
  devServer: {
    port: 8096,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8094',
        changeOrigin: true
      }
    }
  }
}
```

## 常见问题

### 1. 后端连接失败
确保后端服务已启动在 `http://127.0.0.1:8094`

### 2. OpenSSL 错误
项目配置了 `NODE_OPTIONS=--openssl-legacy-provider` 解决 Node 17+ 的兼容问题。

### 3. 依赖安装失败
尝试清除缓存：
```bash
rm -rf node_modules package-lock.json
npm install
```

## 性能优化建议

1. **按需加载**: 路由使用 `() => import()` 懒加载
2. **虚拟滚动**: VXE Table 自动启用虚拟滚动
3. **防抖节流**: 对高频操作（搜索、滚动）添加防抖
4. **缓存优化**: 对不常变化的数据（合约列表）添加缓存
5. **WebSocket**: 后续可用 WebSocket 替代轮询，减少请求

## 文档

- [实施计划](./IMPLEMENTATION_PLAN.md) - 详细的功能清单和开发计划
- [API 文档](./MARKET_API_DOCUMENTATION.md) - 市场数据 API 使用文档

## License

MIT
