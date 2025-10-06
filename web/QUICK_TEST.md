# WebSocket 快速测试指南

5分钟快速验证 WebSocket 集成是否成功。

## 🚀 快速启动

### 1. 启动后端 (终端 1)

```bash
cd /home/quantaxis/qaexchange-rs
cargo run --bin qaexchange-server
```

**预期输出**:
```
Starting QAExchange Server...
HTTP Server listening on 0.0.0.0:8000
WebSocket Server listening on 0.0.0.0:8001
```

### 2. 启动前端 (终端 2)

```bash
cd /home/quantaxis/qaexchange-rs/web
npm run serve
```

**预期输出**:
```
App running at:
  - Local:   http://localhost:8080/
```

---

## ✅ 测试检查清单

### 步骤 1: 登录测试

1. 访问 http://localhost:8080/#/login
2. 输入用户名密码登录（或先注册）

**✓ 检查**: 浏览器控制台应看到:
```
[App] Initializing WebSocket...
[WebSocket] Connected
```

### 步骤 2: WebSocket 测试页面

1. 访问 http://localhost:8080/#/websocket-test

**✓ 检查**:
- [ ] 连接状态显示 "CONNECTED" (绿色)
- [ ] 账户余额有数据显示
- [ ] 顶部状态栏完整显示

### 步骤 3: 订阅行情

1. 点击 "订阅行情" 按钮
2. 选择 `SHFE.cu2501`, `SHFE.ag2506`
3. 点击 "订阅"

**✓ 检查**:
- [ ] 弹出成功提示
- [ ] 行情面板显示实时数据
- [ ] 最新价、买一、卖一有数据

### 步骤 4: 下单测试

1. 填写订单信息:
   - 合约: `SHFE.cu2501`
   - 方向: `买入`
   - 开平: `开仓`
   - 价格: `50000`
   - 数量: `1`
2. 点击 "提交订单"

**✓ 检查**:
- [ ] 弹出 "订单已提交" 提示
- [ ] 订单列表显示新订单
- [ ] 订单状态为 "已接受" 或 "待提交"

### 步骤 5: 撤单测试

1. 点击订单列表中的 "撤单" 按钮
2. 确认撤单

**✓ 检查**:
- [ ] 订单状态变为 "已撤单"
- [ ] 订单从活跃订单列表消失

---

## 🐛 故障排除

### 问题 1: 后端启动失败

```bash
# 检查端口占用
netstat -tuln | grep 8000
netstat -tuln | grep 8001

# 如果端口被占用，杀掉进程
kill -9 $(lsof -t -i:8000)
kill -9 $(lsof -t -i:8001)
```

### 问题 2: WebSocket 连接失败

**检查后端日志**:
```bash
# 应看到 WebSocket 连接日志
```

**检查前端控制台**:
```
打开浏览器 F12 -> Console
查看是否有错误信息
```

**检查 Network**:
```
F12 -> Network -> WS
查看 WebSocket 连接状态
```

### 问题 3: 前端编译错误

```bash
cd /home/quantaxis/qaexchange-rs/web

# 清除缓存重新安装
rm -rf node_modules package-lock.json
npm install
npm run serve
```

---

## 📊 预期结果截图

### 登录后控制台输出
```
[App] Initializing WebSocket...
[WebSocket] Initializing...
[WebSocket] Initialized
[WebSocket] Connecting...
[WebSocket] State changed: DISCONNECTED -> CONNECTING
[WebSocket] Connected
[WebSocket] State changed: CONNECTING -> CONNECTED
[App] WebSocket initialized successfully
```

### WebSocket 测试页面

应看到:
```
┌─────────────────────────────────────────────┐
│ 连接状态: CONNECTED                          │
│ 账户余额: ¥100,000.00                        │
│ 可用资金: ¥95,000.00                         │
│ 浮动盈亏: +¥500.00                           │
│ 风险率: 5.0%                                 │
│                                              │
│ [连接] [断开] [重连] [订阅行情] [查看快照]    │
└─────────────────────────────────────────────┘

实时行情                    持仓 (2)
┌──────────────────┐       ┌──────────────────┐
│ SHFE.cu2501      │       │ SHFE.cu2501      │
│ 最新价: 50,250   │       │ 多头: 5 @ 50,000 │
│ 买一: 50,240 x 3 │       │ 浮动盈亏: +250   │
│ 卖一: 50,260 x 5 │       └──────────────────┘
└──────────────────┘

下单                        订单 (1)
┌──────────────────┐       ┌──────────────────┐
│ 合约: cu2501     │       │ order_123        │
│ 买入 开仓        │       │ 买 开            │
│ 限价: 50000      │       │ 限价: 50000      │
│ 数量: 1          │       │ 已接受           │
│ [提交订单]       │       │ [撤单]           │
└──────────────────┘       └──────────────────┘
```

---

## ✨ 成功标志

如果以上所有检查都通过，说明 WebSocket 集成成功！

你现在可以:

1. ✅ 实时查看账户数据
2. ✅ 实时查看行情数据
3. ✅ 实时查看持仓和订单
4. ✅ 通过 WebSocket 下单和撤单
5. ✅ 自动重连和心跳保活

---

## 📖 下一步

- 阅读完整文档: [WEBSOCKET_INTEGRATION.md](WEBSOCKET_INTEGRATION.md)
- 查看使用示例: [src/websocket/README.md](src/websocket/README.md)
- 集成到交易页面: [src/views/trade/index.vue](src/views/trade/index.vue)

---

**Happy Trading!** 🎉
