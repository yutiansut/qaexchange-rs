# WebSocket 集成和测试指南

本文档提供 QAExchange 前端 WebSocket 模块的完整集成和测试指南。

## 📋 目录

- [概述](#概述)
- [已完成的集成工作](#已完成的集成工作)
- [前置条件](#前置条件)
- [启动后端服务](#启动后端服务)
- [启动前端应用](#启动前端应用)
- [测试流程](#测试流程)
- [常见问题](#常见问题)
- [高级功能](#高级功能)

---

## 概述

WebSocket 模块已完全集成到 QAExchange 前端应用中，提供以下功能：

- ✅ **自动连接管理** - 用户登录后自动初始化 WebSocket
- ✅ **实时数据同步** - 账户、持仓、订单、行情实时更新
- ✅ **DIFF 协议支持** - 完整的 DIFF 协议实现（peek_message + rtn_data）
- ✅ **断线重连** - 自动重连机制，网络恢复后自动连接
- ✅ **心跳保活** - ping/pong 心跳检测，保持连接活跃
- ✅ **Vuex 集成** - 全局状态管理，所有组件可访问实时数据
- ✅ **测试页面** - 专用的 WebSocket 测试页面，方便调试

---

## 已完成的集成工作

### 1. 核心模块 (`web/src/websocket/`)

```
websocket/
├── index.js                    # 模块导出
├── WebSocketManager.js         # WebSocket 连接管理器 (570 行)
├── SnapshotManager.js          # 业务截面管理器 (357 行)
├── DiffProtocol.js             # DIFF 协议封装 (296 行)
├── utils/
│   ├── jsonMergePatch.js      # JSON Merge Patch (RFC 7386)
│   └── logger.js              # 日志工具
├── README.md                   # 使用文档 (1000+ 行)
└── examples/                   # 示例代码
    ├── basic-usage.js         # 基础用法示例
    ├── vue-component.vue      # Vue 组件示例
    └── trading-component.vue  # 完整交易组件示例
```

### 2. Vuex 集成 (`web/src/store/modules/websocket.js`)

已创建 WebSocket Vuex 模块，提供：

- **状态管理**: connectionState, snapshot, subscribedInstruments
- **Actions**: initWebSocket, connectWebSocket, subscribeQuote, insertOrder, cancelOrder
- **Getters**: account, positions, orders, quotes, activeOrders

### 3. 应用集成 (`web/src/App.vue`)

已在 App.vue 中实现：

- 监听登录状态，自动初始化/销毁 WebSocket
- 生命周期管理（mounted/beforeDestroy）
- 错误处理，不阻塞应用启动

### 4. 测试页面 (`web/src/views/WebSocketTest.vue`)

专用测试页面，包含：

- 连接状态监控
- 实时账户信息显示
- 行情订阅和显示
- 下单功能
- 持仓管理
- 订单管理和撤单
- 业务快照查看

### 5. 路由配置 (`web/src/router/index.js`)

已添加路由：`/websocket-test`

### 6. 环境配置

- `.env.development` - 开发环境配置
- `.env.production` - 生产环境配置

---

## 前置条件

### 1. 后端服务

确保已安装 Rust 和相关依赖：

```bash
# 检查 Rust 版本
rustc --version

# 应输出: rustc 1.91.0-nightly (或更高版本)
```

### 2. 前端环境

确保已安装 Node.js 和 npm：

```bash
# 检查 Node.js 版本
node --version

# 应输出: v14.0.0 或更高版本

# 检查 npm 版本
npm --version
```

### 3. 依赖安装

```bash
# 进入前端目录
cd /home/quantaxis/qaexchange-rs/web

# 安装依赖
npm install
```

---

## 启动后端服务

### 方法 1: 启动完整服务器

```bash
cd /home/quantaxis/qaexchange-rs

# 编译并启动服务器
cargo run --bin qaexchange-server

# 服务器将启动在:
# - HTTP: http://localhost:8000
# - WebSocket: ws://localhost:8001/ws
```

### 方法 2: 仅启动 WebSocket 服务

如果只想测试 WebSocket 功能：

```bash
cd /home/quantaxis/qaexchange-rs

# 运行 WebSocket 示例
cargo run --example websocket_server
```

### 验证后端启动成功

打开浏览器访问:

```
http://localhost:8000/health
```

应返回:

```json
{
  "status": "ok",
  "timestamp": "2025-01-06T..."
}
```

---

## 启动前端应用

### 开发模式

```bash
cd /home/quantaxis/qaexchange-rs/web

# 启动开发服务器
npm run serve

# 应输出:
# App running at:
# - Local:   http://localhost:8080/
# - Network: http://192.168.x.x:8080/
```

### 访问应用

打开浏览器访问:

```
http://localhost:8080/
```

---

## 测试流程

### 步骤 1: 用户登录

1. 访问 `http://localhost:8080/#/login`
2. 输入用户名和密码（如果没有用户，先注册）
3. 登录成功后，WebSocket 将自动初始化并连接

**验证**: 打开浏览器控制台，应看到：

```
[App] Initializing WebSocket...
[WebSocket] Initializing...
[WebSocket] Initialized
[WebSocket] Connecting...
[WebSocket] Connected
[WebSocket] State changed: DISCONNECTED -> CONNECTING
[WebSocket] State changed: CONNECTING -> CONNECTED
[App] WebSocket initialized successfully
```

### 步骤 2: 访问 WebSocket 测试页面

1. 点击侧边栏菜单 "WebSocket 测试" 或直接访问:

   ```
   http://localhost:8080/#/websocket-test
   ```

2. 页面加载后，应看到：
   - 顶部状态栏显示 "连接状态: CONNECTED"（绿色标签）
   - 账户余额、可用资金等信息显示

### 步骤 3: 订阅行情

1. 点击 "订阅行情" 按钮
2. 选择合约（如 `SHFE.cu2501`, `SHFE.ag2506`）
3. 点击 "订阅"

**验证**:
- 行情面板应显示实时行情数据
- 控制台应看到:

  ```
  [WebSocket] Subscribing to quotes: ["SHFE.cu2501", "SHFE.ag2506"]
  [WebSocket] Sent peek_message
  ```

### 步骤 4: 查看实时数据更新

观察以下数据是否实时更新：

- **行情数据**: 最新价、买一价/卖一价、成交量、持仓量
- **账户数据**: 余额、可用资金、浮动盈亏、风险率
- **持仓数据**: 多头/空头持仓、持仓均价、浮动盈亏
- **订单数据**: 订单状态、剩余数量

### 步骤 5: 测试下单功能

1. 在 "下单" 面板中填写订单信息：
   - 合约: `SHFE.cu2501`
   - 方向: `买入`
   - 开平: `开仓`
   - 价格类型: `限价`
   - 委托价格: `50000`
   - 委托量: `1`

2. 点击 "提交订单"

**验证**:
- 应弹出成功提示: "订单已提交: order_xxx"
- 订单列表应显示新订单
- 订单状态应从 "待提交" → "已接受"
- 控制台应看到:

  ```
  [WebSocket] Inserting order: {...}
  ```

### 步骤 6: 测试撤单功能

1. 在订单列表中找到刚才提交的订单
2. 点击 "撤单" 按钮
3. 确认撤单

**验证**:
- 订单状态应变为 "已撤单"
- 订单从活跃订单列表中消失

### 步骤 7: 测试断线重连

1. 停止后端服务器（Ctrl+C）
2. 观察前端状态变化

**验证**:
- 连接状态应变为 "RECONNECTING"（黄色标签）
- 控制台应看到:

  ```
  [WebSocket] Connection closed unexpectedly
  [WebSocket] State changed: CONNECTED -> RECONNECTING
  [WebSocket] Reconnecting... attempt 1/10
  ```

3. 重新启动后端服务器

**验证**:
- 连接状态应自动恢复为 "CONNECTED"
- 数据继续正常更新

### 步骤 8: 查看业务快照

1. 点击 "查看快照" 按钮
2. 查看完整的业务截面 JSON 数据

**验证**:
- 应看到完整的数据结构:

  ```json
  {
    "accounts": { "CNY": {...} },
    "positions": { "SHFE.cu2501": {...} },
    "orders": { "order_xxx": {...} },
    "trades": {},
    "quotes": { "SHFE.cu2501": {...} },
    "notify": {}
  }
  ```

---

## 常见问题

### Q1: WebSocket 无法连接

**现象**: 连接状态一直显示 "CONNECTING" 或 "DISCONNECTED"

**解决方案**:

1. 检查后端服务器是否启动:
   ```bash
   curl http://localhost:8000/health
   ```

2. 检查 WebSocket 端口是否正确（默认 8001）:
   ```bash
   netstat -tuln | grep 8001
   ```

3. 检查环境变量配置:
   ```bash
   # web/.env.development
   VUE_APP_WS_URL=ws://localhost:8001/ws
   ```

4. 查看浏览器控制台错误信息

### Q2: 收不到数据更新

**现象**: WebSocket 已连接，但数据不更新

**解决方案**:

1. 确保已订阅行情:
   ```javascript
   // 在测试页面点击 "订阅行情"
   ```

2. 检查 peek_message 是否正常发送:
   ```
   # 控制台应定期看到:
   [WebSocket] Sent peek_message
   ```

3. 检查后端是否有数据更新:
   ```bash
   # 查看后端日志
   ```

4. 检查网络连接

### Q3: 登录后 WebSocket 未自动初始化

**现象**: 登录成功但 WebSocket 未连接

**解决方案**:

1. 检查 App.vue 中的集成代码是否正确

2. 检查 Vuex store 是否正确注册 websocket 模块

3. 手动刷新页面

4. 查看控制台是否有错误信息

### Q4: 订单提交失败

**现象**: 点击 "提交订单" 后报错

**解决方案**:

1. 检查账户资金是否充足

2. 检查订单参数是否正确:
   - 合约代码格式: `EXCHANGE.instrument_id`
   - 价格和数量必须大于 0

3. 检查 WebSocket 连接状态

4. 查看后端日志中的错误信息

### Q5: 页面刷新后 WebSocket 断开

**现象**: 刷新页面后需要手动重连

**解决方案**:

这是正常行为。页面刷新会销毁所有 WebSocket 连接。如果登录状态保持（token 存在），WebSocket 会在页面加载后自动重连。

如果未自动重连:

1. 检查 localStorage 中的 token 是否存在
2. 检查 App.vue 的 mounted 钩子是否正确执行

### Q6: 数据更新太频繁，页面卡顿

**现象**: 数据更新导致页面性能问题

**解决方案**:

在 `web/src/store/modules/websocket.js` 中添加节流：

```javascript
import { throttle } from 'lodash'

// 在 onWebSocketMessage action 中
onWebSocketMessage: throttle(({ state, commit }, message) => {
  if (state.ws) {
    const snapshot = state.ws.getSnapshot()
    commit('SET_SNAPSHOT', snapshot)
  }
}, 100)  // 100ms 更新一次
```

---

## 高级功能

### 1. 在其他组件中使用 WebSocket 数据

#### 方法 1: 使用 Vuex Getters

```vue
<template>
  <div>
    <div>余额: {{ account?.balance }}</div>
    <div>持仓: {{ positions }}</div>
  </div>
</template>

<script>
import { mapGetters } from 'vuex'

export default {
  computed: {
    ...mapGetters('websocket', ['account', 'positions'])
  }
}
</script>
```

#### 方法 2: 直接访问 Store

```vue
<script>
export default {
  computed: {
    account() {
      return this.$store.getters['websocket/account']()
    },

    cuQuote() {
      return this.$store.getters['websocket/quote']('SHFE.cu2501')
    }
  }
}
</script>
```

### 2. 监听特定数据变化

```javascript
// 在组件中
mounted() {
  const ws = this.$store.getters['websocket/ws']
  if (ws) {
    const snapshotManager = ws.getSnapshotManager()

    // 监听 cu2501 价格变化
    this.unsubscribe = snapshotManager.onPathChange(
      'quotes.SHFE.cu2501.last_price',
      (newPrice, oldPrice) => {
        console.log('价格变化:', oldPrice, '->', newPrice)
      }
    )
  }
},

beforeDestroy() {
  // 取消监听
  if (this.unsubscribe) {
    this.unsubscribe()
  }
}
```

### 3. 自定义订阅合约

修改 `.env.development`:

```bash
# 默认订阅的合约（登录后自动订阅）
VUE_APP_DEFAULT_INSTRUMENTS=SHFE.cu2501,SHFE.ag2506,DCE.i2505
```

### 4. 调整日志级别

开发时查看详细日志:

```javascript
// web/src/store/modules/websocket.js
config: {
  logLevel: 'DEBUG'  // DEBUG/INFO/WARN/ERROR/NONE
}
```

生产环境关闭日志:

```javascript
config: {
  logLevel: 'NONE'
}
```

### 5. 自定义重连策略

```javascript
// web/src/store/modules/websocket.js
config: {
  autoReconnect: true,
  reconnectInterval: 5000,        // 5秒重连间隔
  reconnectMaxAttempts: 20        // 最多重连20次
}
```

### 6. 手动控制 WebSocket 连接

```javascript
// 在组件中
methods: {
  async connectWebSocket() {
    try {
      await this.$store.dispatch('websocket/connectWebSocket')
      this.$message.success('WebSocket 连接成功')
    } catch (error) {
      this.$message.error('WebSocket 连接失败')
    }
  },

  disconnectWebSocket() {
    this.$store.dispatch('websocket/disconnectWebSocket')
    this.$message.info('WebSocket 已断开')
  }
}
```

---

## 性能优化建议

### 1. 使用路径监听代替全局监听

```javascript
// ❌ 不推荐：监听整个 snapshot
this.$watch(
  () => this.$store.state.websocket.snapshot,
  (newValue) => {
    // 每次 snapshot 更新都触发
  },
  { deep: true }
)

// ✅ 推荐：只监听需要的字段
const ws = this.$store.getters['websocket/ws']
const snapshotManager = ws.getSnapshotManager()

snapshotManager.onPathChange('quotes.SHFE.cu2501.last_price', (newPrice) => {
  // 只在价格变化时触发
  this.updateChart(newPrice)
})
```

### 2. 使用节流限制更新频率

```javascript
import { throttle } from 'lodash'

export default {
  methods: {
    updateData: throttle(function() {
      this.snapshot = this.$store.state.websocket.snapshot
    }, 100)  // 100ms 更新一次
  },

  mounted() {
    this.$store.watch(
      state => state.websocket.snapshot,
      this.updateData
    )
  }
}
```

### 3. 按需订阅行情

只订阅当前需要的合约，不要订阅过多：

```javascript
// ✅ 推荐：按需订阅
this.$store.dispatch('websocket/subscribeQuote', ['SHFE.cu2501'])

// ❌ 不推荐：订阅过多合约
this.$store.dispatch('websocket/subscribeQuote', [
  'SHFE.cu2501', 'SHFE.ag2506', 'DCE.i2505', 'CZCE.RM505',
  'CFFEX.IF2501', ...  // 太多了
])
```

---

## 调试技巧

### 1. 查看 WebSocket 消息

打开浏览器开发者工具:

1. 切换到 "Network" 标签
2. 过滤 "WS" (WebSocket)
3. 点击 WebSocket 连接
4. 查看 "Messages" 标签

可以看到所有收发的消息。

### 2. 查看业务快照

在任意组件中:

```javascript
// 打印完整快照
console.log(this.$store.state.websocket.snapshot)

// 打印特定数据
console.log(this.$store.getters['websocket/account']())
console.log(this.$store.getters['websocket/positions'])
console.log(this.$store.getters['websocket/quotes'])
```

### 3. 使用 Vue Devtools

安装 Vue Devtools 浏览器扩展后:

1. 打开 DevTools
2. 切换到 "Vue" 标签
3. 选择 "Vuex"
4. 查看 `websocket` 模块的状态

可以实时查看所有 WebSocket 数据。

---

## 下一步

完成基础测试后，可以：

1. **集成到现有交易页面**: 将 WebSocket 数据集成到 `/trade` 页面
2. **实现实时 K 线图**: 使用 WebSocket 行情数据更新 K 线图
3. **添加通知提示**: 监听 `notify` 字段，显示系统通知
4. **实现风控预警**: 监听风险率变化，超过阈值时预警

---

## 附录

### 相关文档

- [WebSocket 模块使用文档](src/websocket/README.md)
- [DIFF 协议文档](../docs/05_apis/websocket/diff_protocol.md)
- [后端 WebSocket API 文档](../docs/05_apis/websocket/README.md)

### 技术支持

如遇到问题，请检查:

1. 浏览器控制台错误信息
2. 后端服务器日志
3. Network 标签中的 WebSocket 连接状态

如问题无法解决，请提供:

- 浏览器控制台完整日志
- 后端服务器日志
- 重现步骤

---

**测试愉快！** 🎉
