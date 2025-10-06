# WebSocket 集成指南 - DIFF 协议接入

**版本**: v1.0
**最后更新**: 2025-10-06
**适用对象**: 客户端开发者、策略开发者

本文档详细说明如何接入 QAExchange WebSocket 服务，实现基于 DIFF (Differential Information Flow for Finance) 协议的实时数据流式传输。

---

## 📋 目录

1. [协议概览](#1-协议概览)
2. [连接建立](#2-连接建立)
3. [认证机制](#3-认证机制)
4. [数据同步机制](#4-数据同步机制)
5. [业务截面管理](#5-业务截面管理)
6. [行情订阅](#6-行情订阅)
7. [交易指令](#7-交易指令)
8. [错误处理](#8-错误处理)
9. [客户端实现示例](#9-客户端实现示例)
10. [性能优化建议](#10-性能优化建议)

---

## 1. 协议概览

### 1.1 DIFF 协议简介

DIFF (Differential Information Flow for Finance) 是 QAExchange 的核心 WebSocket 通信协议，基于 **JSON Merge Patch (RFC 7386)** 实现增量数据同步。

**核心理念**:
> 将异步的事件回调转为同步的数据访问，通过本地业务截面镜像简化客户端编码。

**协议特点**:
- ✅ **增量更新**: 仅传输变化的字段
- ✅ **业务自恰**: 数据满足一致性约束（如 `balance = static_balance + float_profit`）
- ✅ **事务保证**: `rtn_data.data` 数组作为原子事务
- ✅ **向后兼容**: 可包含未定义字段

### 1.2 协议层次

```
┌─────────────────────────────────────────────────┐
│             应用层 (Business Logic)              │
├─────────────────────────────────────────────────┤
│         DIFF 协议 (Snapshot + Patch)            │
├─────────────────────────────────────────────────┤
│         WebSocket (Full-Duplex)                 │
├─────────────────────────────────────────────────┤
│         TCP/IP + TLS (Optional)                 │
└─────────────────────────────────────────────────┘
```

---

## 2. 连接建立

### 2.1 WebSocket URL

```
ws://host:port/ws
wss://host:port/ws  (TLS加密)
```

**默认端口**:
- HTTP: 8000
- WebSocket: 8000 (同HTTP端口)

### 2.2 连接参数

| 参数 | 说明 | 是否必需 |
|------|------|----------|
| `ping_interval` | 心跳间隔（秒） | 可选，默认20s |
| `ping_timeout` | 心跳超时（秒） | 可选，默认10s |
| `max_size` | 最大消息大小（字节） | 可选，默认10MB |

### 2.3 连接示例

#### Python (websockets)

```python
import asyncio
import websockets

async def connect():
    uri = "ws://localhost:8000/ws"
    async with websockets.connect(
        uri,
        ping_interval=20,
        ping_timeout=10,
        max_size=10 * 1024 * 1024
    ) as websocket:
        print("Connected!")
        # ... 业务逻辑
```

#### JavaScript (WebSocket API)

```javascript
const ws = new WebSocket('ws://localhost:8000/ws');

ws.onopen = () => {
    console.log('Connected!');
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    handleMessage(data);
};

ws.onerror = (error) => {
    console.error('WebSocket error:', error);
};

ws.onclose = () => {
    console.log('Disconnected');
};
```

---

## 3. 认证机制

### 3.1 认证消息

**客户端发送** (`req_login`):

```json
{
    "aid": "req_login",
    "bid": "qaexchange",
    "user_name": "user123",
    "password": "token_or_password"
}
```

**字段说明**:
- `aid`: 固定为 `"req_login"`
- `bid`: 业务标识，固定为 `"qaexchange"`
- `user_name`: 用户名或user_id
- `password`: 认证令牌（推荐）或密码

### 3.2 认证响应

服务端通过 `notify` 返回认证结果：

```json
{
    "aid": "rtn_data",
    "data": [
        {
            "notify": {
                "auth_result": {
                    "type": "MESSAGE",
                    "level": "INFO",
                    "code": 1000,
                    "content": "认证成功"
                }
            }
        }
    ]
}
```

### 3.3 认证流程

```
Client                          Server
  │                               │
  ├─── req_login ────────────────>│
  │   {user_name, password}       │
  │                               │ (验证凭证)
  │<──── rtn_data ────────────────┤
  │   {notify: {auth_result}}     │
  │                               │
  ├─── peek_message ─────────────>│ (认证通过后开始数据流)
```

---

## 4. 数据同步机制

### 4.1 peek_message / rtn_data 机制

DIFF 协议的核心是 **peek_message + rtn_data** 循环：

```
Client                          Server
  │                               │
  ├─── peek_message ─────────────>│
  │                               │ (等待数据更新)
  │                               │
  │<──── rtn_data ────────────────┤ (有更新时推送)
  │   {data: [patch1, patch2]}    │
  │                               │
  │ (应用所有patch)                │
  │                               │
  ├─── peek_message ─────────────>│
  │                               ...
```

### 4.2 peek_message

**客户端发送**:

```json
{
    "aid": "peek_message"
}
```

**语义**:
- 请求获取业务截面更新
- 如果服务端有更新立即返回 `rtn_data`
- 如果无更新则**阻塞等待**，直到有数据变化

### 4.3 rtn_data

**服务端推送**:

```json
{
    "aid": "rtn_data",
    "data": [
        {"balance": 10100.0},
        {"float_profit": 100.0},
        {"quotes": {"SHFE.cu2501": {"last_price": 75100.0}}}
    ]
}
```

**字段说明**:
- `aid`: 固定为 `"rtn_data"`
- `data`: JSON Merge Patch 数组（按顺序应用）

### 4.4 JSON Merge Patch 规则

根据 RFC 7386：

| 场景 | Patch | 原始数据 | 结果 |
|------|-------|----------|------|
| 新增字段 | `{"b": 2}` | `{"a": 1}` | `{"a": 1, "b": 2}` |
| 修改字段 | `{"a": 3}` | `{"a": 1}` | `{"a": 3}` |
| 删除字段 | `{"a": null}` | `{"a": 1, "b": 2}` | `{"b": 2}` |
| 递归合并 | `{"obj": {"c": 3}}` | `{"obj": {"a": 1}}` | `{"obj": {"a": 1, "c": 3}}` |

**示例代码** (Python):

```python
def apply_json_merge_patch(target: dict, patch: dict) -> dict:
    """应用 JSON Merge Patch (RFC 7386)"""
    if not isinstance(patch, dict):
        return patch

    if not isinstance(target, dict):
        target = {}

    for key, value in patch.items():
        if value is None:
            # 删除key
            target.pop(key, None)
        elif isinstance(value, dict) and isinstance(target.get(key), dict):
            # 递归合并
            target[key] = apply_json_merge_patch(target[key], value)
        else:
            # 替换value
            target[key] = value

    return target
```

---

## 5. 业务截面管理

### 5.1 业务截面结构

客户端应维护一个**业务截面**（Business Snapshot），镜像服务端状态：

```json
{
    "trade": {
        "user1": {
            "accounts": {
                "CNY": {
                    "user_id": "user1",
                    "balance": 100000.0,
                    "available": 95000.0,
                    "margin": 5000.0,
                    "risk_ratio": 0.05
                }
            },
            "positions": {
                "SHFE.cu2501": {
                    "instrument_id": "SHFE.cu2501",
                    "volume_long": 10,
                    "volume_short": 0,
                    "float_profit": 500.0
                }
            },
            "orders": {
                "order123": {
                    "order_id": "order123",
                    "status": "ALIVE",
                    "volume_left": 5
                }
            },
            "trades": {
                "trade456": {
                    "trade_id": "trade456",
                    "price": 75000.0,
                    "volume": 5
                }
            }
        }
    },
    "quotes": {
        "SHFE.cu2501": {
            "instrument_id": "SHFE.cu2501",
            "last_price": 75000.0,
            "bid_price1": 74990.0,
            "ask_price1": 75010.0
        }
    },
    "ins_list": "SHFE.cu2501,CFFEX.IF2501"
}
```

### 5.2 截面更新流程

1. **接收 `rtn_data`**: 获取 patch 数组
2. **按顺序应用**: 依次应用每个 patch（事务性）
3. **清理空对象**: 删除所有字段为空的对象
4. **触发回调**: 检测变化并触发业务回调

**Python 实现示例**:

```python
class BusinessSnapshot:
    def __init__(self):
        self._data = {}
        self._lock = threading.RLock()

    def apply_patch(self, patches: list):
        with self._lock:
            old_data = copy.deepcopy(self._data)

            try:
                # 按顺序应用所有patch（事务）
                for patch in patches:
                    self._data = apply_json_merge_patch(self._data, patch)

                # 清理空对象
                self._data = clean_empty_objects(self._data)

                # 触发回调
                self._trigger_callbacks(old_data, self._data)
            except Exception as e:
                # 回滚
                self._data = old_data
                raise

    def get_account(self, user_id: str):
        with self._lock:
            return self._data.get("trade", {}).get(user_id, {}).get("accounts", {})
```

---

## 6. 行情订阅

### 6.1 订阅行情

**客户端发送** (`subscribe_quote`):

```json
{
    "aid": "subscribe_quote",
    "ins_list": "SHFE.cu2501,SHFE.cu2502,CFFEX.IF2501"
}
```

**字段说明**:
- `aid`: 固定为 `"subscribe_quote"`
- `ins_list`: 合约列表（逗号分隔）

**重要特性**:
- ⚠️ **覆盖式订阅**: 后一次订阅会**覆盖**前一次
- 📝 合约代码必须包含交易所前缀（如 `SHFE.cu2501`）
- 🔄 发送空 `ins_list` 可取消所有订阅

### 6.2 行情数据推送

服务端通过 `rtn_data` 推送行情：

```json
{
    "aid": "rtn_data",
    "data": [
        {
            "quotes": {
                "SHFE.cu2501": {
                    "instrument_id": "SHFE.cu2501",
                    "datetime": "2025-10-06 14:30:00.000000",
                    "last_price": 75100.0,
                    "bid_price1": 75090.0,
                    "ask_price1": 75110.0,
                    "bid_volume1": 10,
                    "ask_volume1": 5,
                    "volume": 123456,
                    "amount": 9234567890.0,
                    "open_interest": 45678
                }
            }
        },
        {
            "ins_list": "SHFE.cu2501,SHFE.cu2502,CFFEX.IF2501"
        }
    ]
}
```

### 6.3 订阅确认

订阅成功后，`ins_list` 字段会更新为当前订阅列表：

```python
# 检查订阅是否生效
def check_subscription(snapshot):
    ins_list = snapshot.get("ins_list", "")
    subscribed = [s.strip() for s in ins_list.split(",") if s.strip()]
    print(f"当前订阅: {subscribed}")
```

---

## 7. 交易指令

### 7.1 提交订单

**客户端发送** (`insert_order`):

```json
{
    "aid": "insert_order",
    "user_id": "user123",
    "account_id": "account456",
    "order_id": "order789",
    "exchange_id": "SHFE",
    "instrument_id": "cu2501",
    "direction": "BUY",
    "offset": "OPEN",
    "volume": 1,
    "price_type": "LIMIT",
    "limit_price": 75000.0,
    "time_condition": "GFD",
    "volume_condition": "ANY"
}
```

**字段说明**:
- `aid`: 固定为 `"insert_order"`
- `user_id`: 用户ID（必需）
- `account_id`: 交易账户ID（推荐显式传递）
- `order_id`: 订单ID（可选，系统自动生成）
- `direction`: `"BUY"` 或 `"SELL"`
- `offset`: `"OPEN"`, `"CLOSE"`, `"CLOSETODAY"`
- `price_type`: `"LIMIT"`, `"MARKET"`, `"ANY"`

### 7.2 撤单

**客户端发送** (`cancel_order`):

```json
{
    "aid": "cancel_order",
    "user_id": "user123",
    "account_id": "account456",
    "order_id": "order789"
}
```

### 7.3 订单回报

订单状态通过 `rtn_data` 推送：

```json
{
    "aid": "rtn_data",
    "data": [
        {
            "trade": {
                "user1": {
                    "orders": {
                        "order789": {
                            "order_id": "order789",
                            "status": "ALIVE",
                            "volume_left": 1,
                            "insert_date_time": 1696579200000
                        }
                    }
                }
            }
        }
    ]
}
```

**订单状态**:
- `PENDING`: 待提交
- `ALIVE`: 已提交，未成交
- `PARTIALLY_FILLED`: 部分成交
- `FILLED`: 完全成交
- `CANCELLED`: 已撤销
- `REJECTED`: 已拒绝

---

## 8. 错误处理

### 8.1 通知消息

服务端通过 `notify` 发送错误和通知：

```json
{
    "aid": "rtn_data",
    "data": [
        {
            "notify": {
                "error_001": {
                    "type": "MESSAGE",
                    "level": "ERROR",
                    "code": 2001,
                    "content": "订单提交失败: 可用资金不足"
                }
            }
        }
    ]
}
```

**通知级别**:
- `INFO`: 普通消息
- `WARNING`: 警告
- `ERROR`: 错误

### 8.2 连接异常处理

```python
async def websocket_with_reconnect(uri, max_retries=5):
    retry_count = 0

    while retry_count < max_retries:
        try:
            async with websockets.connect(uri) as websocket:
                # 重置重试计数
                retry_count = 0

                # 业务逻辑
                await handle_messages(websocket)

        except websockets.ConnectionClosed:
            retry_count += 1
            wait_time = min(2 ** retry_count, 30)
            print(f"连接断开，{wait_time}秒后重连 (重试 {retry_count}/{max_retries})")
            await asyncio.sleep(wait_time)

        except Exception as e:
            print(f"异常: {e}")
            break
```

---

## 9. 客户端实现示例

### 9.1 完整 Python 客户端

```python
import asyncio
import websockets
import json
from typing import Dict, Any

class QAWebSocketClient:
    def __init__(self, uri: str):
        self.uri = uri
        self.ws = None
        self.snapshot = {}
        self.authenticated = False

    async def connect(self):
        """连接到服务器"""
        self.ws = await websockets.connect(self.uri)
        print("WebSocket 连接成功")

    async def authenticate(self, user_name: str, password: str):
        """认证"""
        await self.send({
            "aid": "req_login",
            "bid": "qaexchange",
            "user_name": user_name,
            "password": password
        })
        self.authenticated = True

    async def subscribe_quote(self, instruments: list):
        """订阅行情"""
        await self.send({
            "aid": "subscribe_quote",
            "ins_list": ",".join(instruments)
        })

    async def peek_message(self):
        """请求数据更新"""
        await self.send({"aid": "peek_message"})

    async def send(self, message: Dict[str, Any]):
        """发送消息"""
        await self.ws.send(json.dumps(message))

    async def receive_loop(self):
        """接收循环"""
        async for message in self.ws:
            data = json.loads(message)
            await self.handle_message(data)

    async def handle_message(self, data: Dict[str, Any]):
        """处理消息"""
        if data.get("aid") == "rtn_data":
            patches = data.get("data", [])
            self.apply_patches(patches)

    def apply_patches(self, patches: list):
        """应用 JSON Merge Patch"""
        for patch in patches:
            self.snapshot = apply_json_merge_patch(self.snapshot, patch)

        # 触发业务回调
        self.on_snapshot_update()

    def on_snapshot_update(self):
        """业务截面更新回调（用户自定义）"""
        quotes = self.snapshot.get("quotes", {})
        for inst_id, quote in quotes.items():
            print(f"行情: {inst_id} @ {quote.get('last_price')}")

# 使用示例
async def main():
    client = QAWebSocketClient("ws://localhost:8000/ws")

    # 连接
    await client.connect()

    # 认证
    await client.authenticate("user123", "password")

    # 订阅行情
    await client.subscribe_quote(["SHFE.cu2501", "CFFEX.IF2501"])

    # 启动peek循环
    async def peek_loop():
        while True:
            await client.peek_message()
            await asyncio.sleep(0.1)

    # 并发运行
    await asyncio.gather(
        client.receive_loop(),
        peek_loop()
    )

asyncio.run(main())
```

### 9.2 JavaScript 客户端

```javascript
class QAWebSocketClient {
    constructor(url) {
        this.url = url;
        this.ws = null;
        this.snapshot = {};
        this.authenticated = false;
    }

    connect() {
        return new Promise((resolve, reject) => {
            this.ws = new WebSocket(this.url);

            this.ws.onopen = () => {
                console.log('WebSocket 连接成功');
                resolve();
            };

            this.ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                this.handleMessage(data);
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket 错误:', error);
                reject(error);
            };
        });
    }

    authenticate(userName, password) {
        this.send({
            aid: 'req_login',
            bid: 'qaexchange',
            user_name: userName,
            password: password
        });
        this.authenticated = true;
    }

    subscribeQuote(instruments) {
        this.send({
            aid: 'subscribe_quote',
            ins_list: instruments.join(',')
        });
    }

    peekMessage() {
        this.send({ aid: 'peek_message' });
    }

    send(message) {
        this.ws.send(JSON.stringify(message));
    }

    handleMessage(data) {
        if (data.aid === 'rtn_data') {
            const patches = data.data || [];
            this.applyPatches(patches);
        }
    }

    applyPatches(patches) {
        patches.forEach(patch => {
            this.snapshot = this.jsonMergePatch(this.snapshot, patch);
        });

        this.onSnapshotUpdate();
    }

    jsonMergePatch(target, patch) {
        if (typeof patch !== 'object' || patch === null) {
            return patch;
        }

        if (typeof target !== 'object' || target === null) {
            target = {};
        }

        for (const key in patch) {
            const value = patch[key];
            if (value === null) {
                delete target[key];
            } else if (typeof value === 'object' && typeof target[key] === 'object') {
                target[key] = this.jsonMergePatch(target[key], value);
            } else {
                target[key] = value;
            }
        }

        return target;
    }

    onSnapshotUpdate() {
        // 用户自定义回调
        const quotes = this.snapshot.quotes || {};
        for (const instId in quotes) {
            const quote = quotes[instId];
            console.log(`行情: ${instId} @ ${quote.last_price}`);
        }
    }

    startPeekLoop(interval = 100) {
        setInterval(() => {
            if (this.authenticated) {
                this.peekMessage();
            }
        }, interval);
    }
}

// 使用示例
async function main() {
    const client = new QAWebSocketClient('ws://localhost:8000/ws');

    await client.connect();
    client.authenticate('user123', 'password');
    client.subscribeQuote(['SHFE.cu2501', 'CFFEX.IF2501']);
    client.startPeekLoop();
}

main();
```

---

## 10. 性能优化建议

### 10.1 连接管理

✅ **推荐做法**:
- 使用长连接，避免频繁重连
- 实现指数退避重连策略
- 设置合理的心跳间隔（推荐20秒）

❌ **避免做法**:
- 短连接频繁建立/断开
- 无限制的重连尝试
- 心跳间隔过短（< 5秒）

### 10.2 数据订阅

✅ **推荐做法**:
- 仅订阅需要的合约
- 批量订阅（一次性发送）
- 定期检查 `ins_list` 确认订阅状态

❌ **避免做法**:
- 订阅大量不使用的合约
- 频繁修改订阅列表
- 重复订阅相同合约

### 10.3 消息处理

✅ **推荐做法**:
- 异步处理消息（避免阻塞接收循环）
- 批量应用 patch（事务性）
- 缓存热点数据

❌ **避免做法**:
- 在消息回调中执行耗时操作
- 单独应用每个 patch（破坏事务性）
- 频繁深拷贝整个截面

### 10.4 内存管理

✅ **推荐做法**:
```python
# 定期清理过期数据
def clean_old_trades(snapshot, max_age_seconds=3600):
    now = time.time()
    trades = snapshot.get("trade", {}).get("user1", {}).get("trades", {})

    old_trades = [
        trade_id for trade_id, trade in trades.items()
        if now - trade.get("trade_date_time", 0) / 1000 > max_age_seconds
    ]

    for trade_id in old_trades:
        del trades[trade_id]
```

### 10.5 错误恢复

✅ **推荐做法**:
```python
# 快照版本控制
class VersionedSnapshot:
    def __init__(self):
        self.data = {}
        self.version = 0
        self.history = []  # 保留最近N个版本

    def apply_patch(self, patches):
        old_version = self.version
        try:
            # 应用patch
            for patch in patches:
                self.data = apply_json_merge_patch(self.data, patch)
            self.version += 1

            # 记录历史
            self.history.append((self.version, copy.deepcopy(self.data)))
            if len(self.history) > 10:
                self.history.pop(0)
        except Exception as e:
            # 回滚到上一个版本
            self.rollback(old_version)
            raise
```

---

## 📚 相关文档

- [DIFF 协议详解](../04_api/websocket/diff_protocol.md) - 协议规范完整定义
- [WebSocket 协议说明](../04_api/websocket/protocol.md) - 消息格式和字段定义
- [前端集成指南](../05_integration/frontend/integration_guide.md) - Vue.js 集成示例
- [序列化指南](../05_integration/serialization.md) - rkyv/JSON 最佳实践

---

## 🆘 常见问题

### Q1: peek_message 会阻塞多久？

**A**: 服务端在有数据更新时立即返回 `rtn_data`。如果无更新，会阻塞等待，直到：
1. 有新的数据变化
2. 超时（默认30秒）
3. 连接断开

推荐在客户端实现**自动 peek 循环**，每收到一次 `rtn_data` 后立即发送下一个 `peek_message`。

### Q2: 如何知道订阅是否成功？

**A**: 检查业务截面中的 `ins_list` 字段：

```python
ins_list = snapshot.get("ins_list", "")
if "SHFE.cu2501" in ins_list:
    print("订阅成功")
```

### Q3: 订单提交后何时能看到回报？

**A**: 订单回报通过 `rtn_data` 异步推送，通常在几毫秒内到达。确保：
1. 已认证
2. 正在运行 `peek_message` 循环
3. 注册了订单更新回调

### Q4: 如何处理网络断线重连？

**A**: 实现指数退避重连策略，重连后：
1. 重新认证
2. 重新订阅行情
3. 查询当前持仓和订单状态（可选）

---

**最后更新**: 2025-10-06
**作者**: @yutiansut @quantaxis
