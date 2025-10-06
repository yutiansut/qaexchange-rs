# QAExchange 市场数据 API 文档

## 概述

本文档描述已实现的市场数据 API，遵循**业务逻辑与网络层解耦**的架构原则。

## 架构说明

### 业务逻辑层
- **文件**: `src/market/mod.rs`
- **核心组件**: `MarketDataService`
- **职责**: 纯业务逻辑，从撮合引擎获取数据并格式化

### 网络层
- **文件**: `src/service/http/market.rs`
- **职责**: HTTP 请求解析、响应格式化、错误处理
- **原则**: 仅调用 `MarketDataService` 的方法，不包含业务逻辑

---

## API 端点

### 1. 获取合约列表

**端点**: `GET /api/market/instruments`

**描述**: 获取交易所所有可交易合约信息

**请求参数**: 无

**响应示例**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IF2501",
      "name": "IF2501 期货",
      "multiplier": 300.0,
      "tick_size": 0.2,
      "last_price": 3800.5,
      "status": "Trading"
    },
    {
      "instrument_id": "IF2502",
      "name": "IF2502 期货",
      "multiplier": 300.0,
      "tick_size": 0.2,
      "last_price": 3820.0,
      "status": "Trading"
    }
  ],
  "error": null
}
```

**字段说明**:
- `instrument_id`: 合约代码
- `name`: 合约名称
- `multiplier`: 合约乘数
- `tick_size`: 最小价格变动单位
- `last_price`: 最新成交价（可能为 null）
- `status`: 交易状态（Trading/Halted/Closed）

---

### 2. 获取订单簿（买卖盘深度）

**端点**: `GET /api/market/orderbook/{instrument_id}`

**描述**: 获取指定合约的订单簿快照（买卖五档或自定义档位）

**路径参数**:
- `instrument_id`: 合约代码（如 `IF2501`）

**查询参数**:
- `depth`: 档位深度（默认 5），可选值 1-20

**请求示例**:
```
GET /api/market/orderbook/IF2501?depth=5
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "instrument_id": "IF2501",
    "timestamp": 1704096000000,
    "bids": [
      {"price": 3800.2, "volume": 5},
      {"price": 3800.0, "volume": 10},
      {"price": 3799.8, "volume": 8},
      {"price": 3799.6, "volume": 12},
      {"price": 3799.4, "volume": 6}
    ],
    "asks": [
      {"price": 3800.4, "volume": 7},
      {"price": 3800.6, "volume": 9},
      {"price": 3800.8, "volume": 11},
      {"price": 3801.0, "volume": 5},
      {"price": 3801.2, "volume": 8}
    ],
    "last_price": 3800.2
  },
  "error": null
}
```

**字段说明**:
- `bids`: 买盘（降序排列，从高到低）
- `asks`: 卖盘（升序排列，从低到高）
- `price`: 价格档位
- `volume`: 该价格的挂单总量
- `last_price`: 最新成交价
- `timestamp`: 快照时间戳（毫秒）

**错误响应**:
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 500,
    "message": "Failed to get orderbook: Instrument not found: INVALID"
  }
}
```

---

### 3. 获取 Tick 行情数据

**端点**: `GET /api/market/tick/{instrument_id}`

**描述**: 获取指定合约的实时 Tick 行情

**路径参数**:
- `instrument_id`: 合约代码

**请求示例**:
```
GET /api/market/tick/IF2501
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "instrument_id": "IF2501",
    "timestamp": 1704096000000,
    "last_price": 3800.2,
    "bid_price": 3800.0,
    "ask_price": 3800.4,
    "volume": 0
  },
  "error": null
}
```

**字段说明**:
- `last_price`: 最新成交价
- `bid_price`: 最优买价（买一价）
- `ask_price`: 最优卖价（卖一价）
- `volume`: 当日累计成交量（TODO: 待实现）

---

### 4. 获取最近成交记录

**端点**: `GET /api/market/trades/{instrument_id}`

**描述**: 获取指定合约的最近成交记录

**路径参数**:
- `instrument_id`: 合约代码

**查询参数**:
- `limit`: 返回记录数量（默认 20）

**请求示例**:
```
GET /api/market/trades/IF2501?limit=10
```

**响应示例**:
```json
{
  "success": true,
  "data": [],
  "error": null
}
```

**注意**: 当前返回空列表，因为 `ExchangeMatchingEngine` 不存储历史成交记录。需要从 `TradeGateway` 或持久化存储中获取。

**待实现完整版响应**:
```json
{
  "success": true,
  "data": [
    {
      "trade_id": "T20240101-000001",
      "instrument_id": "IF2501",
      "price": 3800.2,
      "volume": 5,
      "timestamp": 1704096000000,
      "direction": "BUY"
    }
  ],
  "error": null
}
```

---

### 5. 获取市场订单统计（管理员功能）

**端点**: `GET /api/admin/market/order-stats`

**描述**: 获取全市场所有合约的订单统计信息

**请求参数**: 无

**请求示例**:
```
GET /api/admin/market/order-stats
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "total_orders": 150,
    "total_bids": 80,
    "total_asks": 70
  },
  "error": null
}
```

**字段说明**:
- `total_orders`: 全市场总订单数
- `total_bids`: 买单总数
- `total_asks`: 卖单总数

---

## 通用响应格式

所有 API 遵循统一的响应格式：

### 成功响应
```json
{
  "success": true,
  "data": { /* 具体数据 */ },
  "error": null
}
```

### 错误响应
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 500,
    "message": "错误描述"
  }
}
```

---

## 实现细节

### 数据来源
- **订单簿**: 从 `ExchangeMatchingEngine::get_orderbook()` 获取
- **合约列表**: 从 `ExchangeMatchingEngine::get_instruments()` 获取
- **最新成交价**: 从 `Orderbook.lastprice` 字段获取
- **订单统计**: 遍历所有合约的订单簿聚合

### 性能考量
1. 所有读取操作使用 `RwLock::read()` 共享锁
2. 订单簿查询通过 `get_sorted_orders()` 方法，无需额外排序
3. 价格档位聚合使用 HashMap，时间复杂度 O(n)
4. MarketDataService 可被多个网络层（HTTP/WebSocket）复用

### 并发安全
- `ExchangeMatchingEngine` 使用 `DashMap` 和 `Arc<RwLock<Orderbook>>`
- 多个 HTTP 请求可并发读取订单簿数据
- 写操作（下单/撤单）不阻塞读操作

---

## 前端集成示例

### 使用 Axios 调用

```javascript
import request from '@/api/request'

// 获取合约列表
export function getInstruments() {
  return request({
    url: '/api/market/instruments',
    method: 'get'
  })
}

// 获取订单簿
export function getOrderBook(instrumentId, depth = 5) {
  return request({
    url: `/api/market/orderbook/${instrumentId}`,
    method: 'get',
    params: { depth }
  })
}

// 获取 Tick 数据
export function getTick(instrumentId) {
  return request({
    url: `/api/market/tick/${instrumentId}`,
    method: 'get'
  })
}

// 获取市场统计（管理员）
export function getMarketStats() {
  return request({
    url: '/api/admin/market/order-stats',
    method: 'get'
  })
}
```

### Vue 组件使用示例

```vue
<template>
  <div>
    <h3>{{ instrumentId }} 订单簿</h3>
    <div class="orderbook">
      <div class="bids">
        <div v-for="level in orderbook.bids" :key="level.price">
          {{ level.price }} - {{ level.volume }}
        </div>
      </div>
      <div class="asks">
        <div v-for="level in orderbook.asks" :key="level.price">
          {{ level.price }} - {{ level.volume }}
        </div>
      </div>
    </div>
  </div>
</template>

<script>
import { getOrderBook } from '@/api'

export default {
  data() {
    return {
      instrumentId: 'IF2501',
      orderbook: { bids: [], asks: [] }
    }
  },
  mounted() {
    this.loadOrderBook()
    setInterval(this.loadOrderBook, 1000)  // 每秒刷新
  },
  methods: {
    async loadOrderBook() {
      try {
        const data = await getOrderBook(this.instrumentId, 5)
        this.orderbook = data
      } catch (error) {
        console.error('Failed to load orderbook:', error)
      }
    }
  }
}
</script>
```

---

## 后续优化方向

1. **WebSocket 推送**: 实时推送订单簿变化，减少轮询
2. **成交记录持久化**: 从 TradeGateway 获取历史成交
3. **K线数据**: 实现 K线聚合模块，提供各周期 K线
4. **缓存机制**: 对高频查询的数据（如合约列表）添加缓存
5. **限流保护**: 防止恶意高频查询

---

## 测试建议

### 单元测试
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_data_service() {
        let engine = Arc::new(ExchangeMatchingEngine::new());
        engine.register_instrument("TEST".to_string(), 100.0).unwrap();

        let service = MarketDataService::new(engine);
        let instruments = service.get_instruments().unwrap();

        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].instrument_id, "TEST");
    }
}
```

### 集成测试
```bash
# 启动服务器
cargo run --bin qaexchange-server

# 测试合约列表
curl http://127.0.0.1:8094/api/market/instruments

# 测试订单簿
curl http://127.0.0.1:8094/api/market/orderbook/IF2501?depth=5

# 测试 Tick
curl http://127.0.0.1:8094/api/market/tick/IF2501
```

---

## 版本历史

- **v1.0** (2025-01-04): 初始版本
  - 实现基础市场数据 API
  - 遵循业务逻辑与网络层解耦原则
  - 支持订单簿、合约列表、Tick 数据查询
