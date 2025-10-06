# WebSocket 协议文档

**WebSocket URL**: `ws://localhost:8081/ws`
**协议版本**: v1.0
**消息格式**: JSON

---

## 📋 目录

- [连接建立](#连接建立)
- [认证流程](#认证流程)
- [客户端消息](#客户端消息)
- [服务端消息](#服务端消息)
- [心跳机制](#心跳机制)
- [错误处理](#错误处理)
- [完整示例](#完整示例)

---

## 连接建立

### 连接 URL

```
ws://localhost:8081/ws?user_id=<user_id>
```

**查询参数**:
- `user_id` (optional): 用户ID，提供后自动订阅该用户的成交推送

### JavaScript 连接示例

```javascript
// 基础连接
const ws = new WebSocket('ws://localhost:8081/ws?user_id=user001');

// 连接打开
ws.onopen = () => {
  console.log('WebSocket 已连接');
};

// 接收消息
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  console.log('收到消息:', message);
};

// 连接关闭
ws.onclose = () => {
  console.log('WebSocket 已断开');
};

// 错误处理
ws.onerror = (error) => {
  console.error('WebSocket 错误:', error);
};
```

### Python 连接示例

```python
import websocket
import json

def on_message(ws, message):
    data = json.loads(message)
    print(f"收到消息: {data}")

def on_open(ws):
    print("WebSocket 已连接")
    # 发送认证消息
    ws.send(json.dumps({
        "type": "auth",
        "user_id": "user001",
        "token": "your_token"
    }))

ws = websocket.WebSocketApp(
    "ws://localhost:8081/ws?user_id=user001",
    on_message=on_message,
    on_open=on_open
)
ws.run_forever()
```

---

## 认证流程

### 1. 发送认证消息

连接建立后，客户端应立即发送认证消息：

**客户端 → 服务端**:
```json
{
  "type": "auth",
  "user_id": "user001",
  "token": "your_token_here"
}
```

### 2. 认证响应

**服务端 → 客户端**:

**成功**:
```json
{
  "type": "auth_response",
  "success": true,
  "user_id": "user001",
  "message": "Authentication successful"
}
```

**失败**:
```json
{
  "type": "auth_response",
  "success": false,
  "user_id": "",
  "message": "Invalid credentials"
}
```

### 认证示例

```javascript
const ws = new WebSocket('ws://localhost:8081/ws?user_id=user001');

ws.onopen = () => {
  // 发送认证
  ws.send(JSON.stringify({
    type: 'auth',
    user_id: 'user001',
    token: 'your_token_here'
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'auth_response') {
    if (msg.success) {
      console.log('认证成功');
      // 可以开始订阅和交易
    } else {
      console.error('认证失败:', msg.message);
      ws.close();
    }
  }
};
```

---

## 客户端消息

### 消息格式

所有客户端消息均为 JSON 格式，包含 `type` 字段标识消息类型。

### 1. 认证 (Auth)

```json
{
  "type": "auth",
  "user_id": "user001",
  "token": "your_token"
}
```

### 2. 订阅 (Subscribe)

订阅行情或成交推送。

```json
{
  "type": "subscribe",
  "channels": ["trade", "orderbook", "ticker"],
  "instruments": ["IX2301", "IF2301"]
}
```

**参数说明**:
- `channels`: 订阅的频道
  - `trade`: 成交推送
  - `orderbook`: 订单簿（Level2）
  - `ticker`: 逐笔成交
- `instruments`: 订阅的合约列表

**示例**:
```javascript
// 订阅成交推送
ws.send(JSON.stringify({
  type: 'subscribe',
  channels: ['trade'],
  instruments: ['IX2301', 'IF2301']
}));
```

### 3. 取消订阅 (Unsubscribe)

```json
{
  "type": "unsubscribe",
  "channels": ["trade"],
  "instruments": ["IX2301"]
}
```

### 4. 提交订单 (SubmitOrder)

通过 WebSocket 提交订单。

```json
{
  "type": "submit_order",
  "instrument_id": "IX2301",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 10,
  "price": 120.0,
  "order_type": "LIMIT"
}
```

**示例**:
```javascript
// 提交买单
function submitOrder() {
  ws.send(JSON.stringify({
    type: 'submit_order',
    instrument_id: 'IX2301',
    direction: 'BUY',
    offset: 'OPEN',
    volume: 10,
    price: 120.0,
    order_type: 'LIMIT'
  }));
}
```

### 5. 撤单 (CancelOrder)

```json
{
  "type": "cancel_order",
  "order_id": "O17251234567890000001"
}
```

### 6. 查询订单 (QueryOrder)

```json
{
  "type": "query_order",
  "order_id": "O17251234567890000001"
}
```

### 7. 查询账户 (QueryAccount)

```json
{
  "type": "query_account"
}
```

### 8. 查询持仓 (QueryPosition)

```json
{
  "type": "query_position",
  "instrument_id": "IX2301"  // 可选，不填查询所有持仓
}
```

### 9. 心跳 (Ping)

```json
{
  "type": "ping"
}
```

---

## 服务端消息

### 消息格式

所有服务端消息均为 JSON 格式，包含 `type` 字段标识消息类型。

### 1. 认证响应 (AuthResponse)

```json
{
  "type": "auth_response",
  "success": true,
  "user_id": "user001",
  "message": "Authentication successful"
}
```

### 2. 订阅响应 (SubscribeResponse)

```json
{
  "type": "subscribe_response",
  "success": true,
  "channels": ["trade"],
  "instruments": ["IX2301"],
  "message": "Subscribed successfully"
}
```

### 3. 订单响应 (OrderResponse)

提交订单或撤单后的响应。

```json
{
  "type": "order_response",
  "success": true,
  "order_id": "O17251234567890000001",
  "error_code": null,
  "error_message": null
}
```

**失败示例**:
```json
{
  "type": "order_response",
  "success": false,
  "order_id": null,
  "error_code": 1001,
  "error_message": "Insufficient funds"
}
```

### 4. 成交推送 (Trade)

订单成交后的实时推送。

```json
{
  "type": "trade",
  "trade_id": "T17251234567890000001",
  "order_id": "O17251234567890000001",
  "instrument_id": "IX2301",
  "direction": "BUY",
  "offset": "OPEN",
  "price": 120.0,
  "volume": 10.0,
  "timestamp": 1696320001000
}
```

**处理示例**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'trade') {
    console.log(`成交: ${msg.direction} ${msg.volume}手 @${msg.price}`);
    // 更新 UI
    updateTradeList(msg);
  }
};
```

### 5. 订单状态推送 (OrderStatus)

订单状态变化时的推送。

```json
{
  "type": "order_status",
  "order_id": "O17251234567890000001",
  "status": "PartiallyFilled",
  "filled_volume": 5.0,
  "remaining_volume": 5.0,
  "timestamp": 1696320001000
}
```

**订单状态**:
- `Submitted`: 已提交
- `PartiallyFilled`: 部分成交
- `Filled`: 全部成交
- `Cancelled`: 已撤单

**处理示例**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'order_status') {
    console.log(`订单 ${msg.order_id} 状态: ${msg.status}`);
    console.log(`成交量: ${msg.filled_volume}, 剩余: ${msg.remaining_volume}`);
    // 更新订单列表
    updateOrderStatus(msg.order_id, msg.status);
  }
};
```

### 6. 账户更新推送 (AccountUpdate)

账户资金变化时的推送。

```json
{
  "type": "account_update",
  "balance": 1005000.0,
  "available": 955000.0,
  "frozen": 50000.0,
  "margin": 50000.0,
  "profit": 5000.0,
  "risk_ratio": 0.05,
  "timestamp": 1696320001000
}
```

**处理示例**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'account_update') {
    console.log('账户更新:');
    console.log(`  余额: ${msg.balance}`);
    console.log(`  可用: ${msg.available}`);
    console.log(`  盈亏: ${msg.profit}`);
    // 更新账户显示
    updateAccountDisplay(msg);
  }
};
```

### 7. 订单簿推送 (OrderBook)

Level2 订单簿数据推送。

```json
{
  "type": "orderbook",
  "instrument_id": "IX2301",
  "bids": [
    { "price": 119.5, "volume": 100.0, "order_count": 5 },
    { "price": 119.0, "volume": 200.0, "order_count": 8 }
  ],
  "asks": [
    { "price": 120.0, "volume": 150.0, "order_count": 6 },
    { "price": 120.5, "volume": 180.0, "order_count": 7 }
  ],
  "timestamp": 1696320001000
}
```

**处理示例**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'orderbook') {
    console.log(`${msg.instrument_id} 订单簿:`);
    console.log('买盘:', msg.bids);
    console.log('卖盘:', msg.asks);
    // 更新深度图
    updateOrderBook(msg);
  }
};
```

### 8. 逐笔成交推送 (Ticker)

```json
{
  "type": "ticker",
  "instrument_id": "IX2301",
  "last_price": 120.0,
  "volume": 10.0,
  "timestamp": 1696320001000
}
```

### 9. 查询响应 (QueryResponse)

查询操作的响应。

```json
{
  "type": "query_response",
  "request_type": "query_account",
  "data": {
    "account": {
      "user_id": "user001",
      "balance": 1000000.0,
      "available": 950000.0,
      ...
    }
  }
}
```

### 10. 错误消息 (Error)

```json
{
  "type": "error",
  "code": 401,
  "message": "Not authenticated"
}
```

### 11. 心跳响应 (Pong)

```json
{
  "type": "pong"
}
```

---

## 心跳机制

### 客户端主动心跳

建议每 5 秒发送一次 Ping：

```javascript
// 心跳定时器
setInterval(() => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'ping' }));
  }
}, 5000);

// 处理 Pong
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'pong') {
    console.log('心跳正常');
  }
};
```

### 服务端超时检测

- 服务端每 5 秒发送 Ping
- 10 秒内未收到任何消息，服务端主动断开连接

---

## 错误处理

### 错误码

| 错误码 | 说明 |
|--------|------|
| 400 | 消息格式错误 |
| 401 | 未认证 |
| 1001 | 资金不足 |
| 1002 | 订单不存在 |
| 1003 | 账户不存在 |

### 错误处理示例

```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'error') {
    switch (msg.code) {
      case 401:
        console.error('未认证，请先登录');
        // 重新认证
        authenticate();
        break;
      case 1001:
        console.error('资金不足');
        alert('资金不足，无法下单');
        break;
      default:
        console.error('错误:', msg.message);
    }
  }
};
```

---

## 完整示例

### React WebSocket Hook

```javascript
import { useEffect, useRef, useState } from 'react';

function useWebSocket(url, userId, token) {
  const ws = useRef(null);
  const [isConnected, setIsConnected] = useState(false);
  const [messages, setMessages] = useState([]);

  useEffect(() => {
    // 创建 WebSocket 连接
    ws.current = new WebSocket(`${url}?user_id=${userId}`);

    ws.current.onopen = () => {
      console.log('WebSocket 已连接');
      setIsConnected(true);

      // 发送认证
      ws.current.send(JSON.stringify({
        type: 'auth',
        user_id: userId,
        token: token
      }));
    };

    ws.current.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      setMessages(prev => [...prev, msg]);

      // 处理不同类型的消息
      switch (msg.type) {
        case 'auth_response':
          if (msg.success) {
            console.log('认证成功');
            // 订阅成交推送
            ws.current.send(JSON.stringify({
              type: 'subscribe',
              channels: ['trade', 'account_update'],
              instruments: ['IX2301']
            }));
          }
          break;

        case 'trade':
          console.log('收到成交:', msg);
          break;

        case 'account_update':
          console.log('账户更新:', msg);
          break;

        case 'order_status':
          console.log('订单状态:', msg);
          break;
      }
    };

    ws.current.onclose = () => {
      console.log('WebSocket 已断开');
      setIsConnected(false);
    };

    ws.current.onerror = (error) => {
      console.error('WebSocket 错误:', error);
    };

    // 心跳
    const heartbeat = setInterval(() => {
      if (ws.current?.readyState === WebSocket.OPEN) {
        ws.current.send(JSON.stringify({ type: 'ping' }));
      }
    }, 5000);

    // 清理
    return () => {
      clearInterval(heartbeat);
      ws.current?.close();
    };
  }, [url, userId, token]);

  // 发送消息
  const sendMessage = (message) => {
    if (ws.current?.readyState === WebSocket.OPEN) {
      ws.current.send(JSON.stringify(message));
    }
  };

  return { isConnected, messages, sendMessage };
}

// 使用示例
function TradingComponent() {
  const { isConnected, messages, sendMessage } = useWebSocket(
    'ws://localhost:8081/ws',
    'user001',
    'your_token'
  );

  const submitOrder = () => {
    sendMessage({
      type: 'submit_order',
      instrument_id: 'IX2301',
      direction: 'BUY',
      offset: 'OPEN',
      volume: 10,
      price: 120.0,
      order_type: 'LIMIT'
    });
  };

  return (
    <div>
      <p>连接状态: {isConnected ? '已连接' : '未连接'}</p>
      <button onClick={submitOrder} disabled={!isConnected}>
        提交订单
      </button>
      <div>
        <h3>消息列表</h3>
        {messages.map((msg, i) => (
          <div key={i}>{JSON.stringify(msg)}</div>
        ))}
      </div>
    </div>
  );
}
```

### Vue WebSocket 组件

```vue
<template>
  <div>
    <p>连接状态: {{ isConnected ? '已连接' : '未连接' }}</p>
    <button @click="submitOrder" :disabled="!isConnected">提交订单</button>

    <div v-for="(msg, i) in messages" :key="i">
      {{ msg.type }}: {{ msg }}
    </div>
  </div>
</template>

<script>
export default {
  data() {
    return {
      ws: null,
      isConnected: false,
      messages: []
    };
  },

  mounted() {
    this.connect();
  },

  methods: {
    connect() {
      this.ws = new WebSocket('ws://localhost:8081/ws?user_id=user001');

      this.ws.onopen = () => {
        this.isConnected = true;
        // 认证
        this.send({
          type: 'auth',
          user_id: 'user001',
          token: 'your_token'
        });
      };

      this.ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);
        this.messages.push(msg);

        if (msg.type === 'auth_response' && msg.success) {
          // 订阅
          this.send({
            type: 'subscribe',
            channels: ['trade'],
            instruments: ['IX2301']
          });
        }
      };

      this.ws.onclose = () => {
        this.isConnected = false;
      };
    },

    send(message) {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify(message));
      }
    },

    submitOrder() {
      this.send({
        type: 'submit_order',
        instrument_id: 'IX2301',
        direction: 'BUY',
        offset: 'OPEN',
        volume: 10,
        price: 120.0,
        order_type: 'LIMIT'
      });
    }
  },

  beforeUnmount() {
    this.ws?.close();
  }
};
</script>
```

### Python 完整示例

```python
import websocket
import json
import threading
import time

class TradingWebSocket:
    def __init__(self, url, user_id, token):
        self.url = f"{url}?user_id={user_id}"
        self.user_id = user_id
        self.token = token
        self.ws = None
        self.is_authenticated = False

    def on_open(self, ws):
        print("WebSocket 已连接")
        # 发送认证
        self.send({
            "type": "auth",
            "user_id": self.user_id,
            "token": self.token
        })

        # 启动心跳
        def heartbeat():
            while self.ws:
                time.sleep(5)
                self.send({"type": "ping"})

        threading.Thread(target=heartbeat, daemon=True).start()

    def on_message(self, ws, message):
        msg = json.loads(message)
        print(f"收到消息: {msg}")

        if msg["type"] == "auth_response":
            if msg["success"]:
                self.is_authenticated = True
                print("认证成功")
                # 订阅
                self.send({
                    "type": "subscribe",
                    "channels": ["trade"],
                    "instruments": ["IX2301"]
                })

        elif msg["type"] == "trade":
            print(f"成交: {msg['direction']} {msg['volume']}手 @{msg['price']}")

        elif msg["type"] == "order_status":
            print(f"订单状态: {msg['status']}")

    def on_close(self, ws, close_status_code, close_msg):
        print("WebSocket 已断开")

    def on_error(self, ws, error):
        print(f"WebSocket 错误: {error}")

    def send(self, message):
        if self.ws:
            self.ws.send(json.dumps(message))

    def submit_order(self, instrument_id, direction, offset, volume, price):
        if not self.is_authenticated:
            print("未认证，无法下单")
            return

        self.send({
            "type": "submit_order",
            "instrument_id": instrument_id,
            "direction": direction,
            "offset": offset,
            "volume": volume,
            "price": price,
            "order_type": "LIMIT"
        })

    def run(self):
        self.ws = websocket.WebSocketApp(
            self.url,
            on_open=self.on_open,
            on_message=self.on_message,
            on_close=self.on_close,
            on_error=self.on_error
        )
        self.ws.run_forever()

# 使用
if __name__ == "__main__":
    trading_ws = TradingWebSocket(
        "ws://localhost:8081/ws",
        "user001",
        "your_token"
    )

    # 在另一个线程中运行
    threading.Thread(target=trading_ws.run, daemon=True).start()

    # 等待认证
    time.sleep(2)

    # 提交订单
    trading_ws.submit_order(
        instrument_id="IX2301",
        direction="BUY",
        offset="OPEN",
        volume=10,
        price=120.0
    )

    # 保持运行
    input("按回车键退出...\n")
```

---

## 消息流程图

```
客户端                                    服务端
  |                                        |
  |--- WebSocket 连接 ------------------->|
  |                                        |
  |<-- 连接成功 --------------------------|
  |                                        |
  |--- Auth (认证) ---------------------->|
  |                                        |
  |<-- AuthResponse (认证成功) ------------|
  |                                        |
  |--- Subscribe (订阅) ------------------>|
  |                                        |
  |<-- SubscribeResponse (订阅成功) --------|
  |                                        |
  |--- SubmitOrder (下单) ---------------->|
  |                                        |
  |<-- OrderResponse (下单成功) ------------|
  |                                        |
  |<-- OrderStatus (订单状态变化) ----------|
  |                                        |
  |<-- Trade (成交推送) --------------------|
  |                                        |
  |<-- AccountUpdate (账户更新) ------------|
  |                                        |
  |--- Ping (心跳) ------------------------>|
  |                                        |
  |<-- Pong (心跳响应) ---------------------|
  |                                        |
```

---

## 最佳实践

### 1. 连接管理

```javascript
class WebSocketManager {
  constructor(url, userId, token) {
    this.url = url;
    this.userId = userId;
    this.token = token;
    this.ws = null;
    this.reconnectInterval = 5000;
    this.isManualClose = false;
  }

  connect() {
    this.ws = new WebSocket(`${this.url}?user_id=${this.userId}`);

    this.ws.onopen = () => this.handleOpen();
    this.ws.onmessage = (e) => this.handleMessage(e);
    this.ws.onclose = () => this.handleClose();
    this.ws.onerror = (e) => this.handleError(e);
  }

  handleOpen() {
    console.log('连接成功');
    this.authenticate();
  }

  handleClose() {
    console.log('连接断开');
    if (!this.isManualClose) {
      // 自动重连
      setTimeout(() => this.connect(), this.reconnectInterval);
    }
  }

  authenticate() {
    this.send({
      type: 'auth',
      user_id: this.userId,
      token: this.token
    });
  }

  send(message) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    }
  }

  close() {
    this.isManualClose = true;
    this.ws?.close();
  }
}
```

### 2. 消息队列

```javascript
class MessageQueue {
  constructor(ws) {
    this.ws = ws;
    this.queue = [];
    this.isProcessing = false;
  }

  enqueue(message) {
    this.queue.push(message);
    this.process();
  }

  async process() {
    if (this.isProcessing || this.queue.length === 0) return;

    this.isProcessing = true;

    while (this.queue.length > 0) {
      const message = this.queue.shift();
      this.ws.send(JSON.stringify(message));
      await new Promise(resolve => setTimeout(resolve, 10)); // 限流
    }

    this.isProcessing = false;
  }
}
```

### 3. 事件订阅

```javascript
class EventEmitter {
  constructor() {
    this.events = {};
  }

  on(event, callback) {
    if (!this.events[event]) {
      this.events[event] = [];
    }
    this.events[event].push(callback);
  }

  emit(event, data) {
    if (this.events[event]) {
      this.events[event].forEach(callback => callback(data));
    }
  }
}

// 使用
const emitter = new EventEmitter();

emitter.on('trade', (trade) => {
  console.log('收到成交:', trade);
});

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  emitter.emit(msg.type, msg);
};
```

---

**文档版本**: v1.0
**最后更新**: 2025-10-03
