# QAExchange 前后端交互协议文档

> @yutiansut @quantaxis

本文档定义了 QAExchange 系统前后端交互的完整协议规范，包括 HTTP REST API 和 WebSocket 实时通信协议。

---

## 目录

1. [概述](#1-概述)
2. [HTTP REST API](#2-http-rest-api)
3. [WebSocket 协议](#3-websocket-协议)
4. [DIFF 协议详解](#4-diff-协议详解)
5. [数据结构定义](#5-数据结构定义)
6. [错误码定义](#6-错误码定义)
7. [前端集成指南](#7-前端集成指南)

---

## 1. 概述

### 1.1 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                      前端应用 (Vue.js)                        │
├─────────────────────────────────────────────────────────────┤
│   HTTP REST API (8094)    │    WebSocket DIFF (8095)        │
│   - 用户认证               │    - 实时行情推送                 │
│   - 账户管理               │    - 订单状态推送                 │
│   - 订单操作               │    - 业务截面同步                 │
│   - 数据查询               │    - K线数据订阅                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     后端服务 (Rust)                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ HTTP Server │  │  WS Server  │  │ 撮合引擎     │         │
│  │ (Actix-web) │  │ (Actix-ws)  │  │ (Orderbook) │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 端口配置

| 服务 | 端口 | 用途 |
|------|------|------|
| HTTP API | 8094 | REST API 接口 |
| WebSocket (DIFF) | 8095 | 实时数据推送 |
| 前端开发服务器 | 8096 | Vue.js 开发服务器 |

### 1.3 通用响应格式

所有 HTTP API 响应均使用统一格式：

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

失败响应：

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": 1001,
    "message": "错误描述"
  }
}
```

---

## 2. HTTP REST API

### 2.1 用户认证 (`/api/auth`)

#### 2.1.1 用户注册

```http
POST /api/auth/register
Content-Type: application/json

{
  "username": "string",
  "password": "string"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "uuid-string",
    "username": "string",
    "created_at": "2025-12-15T10:00:00Z"
  }
}
```

#### 2.1.2 用户登录

```http
POST /api/auth/login
Content-Type: application/json

{
  "username": "string",
  "password": "string"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "uuid-string",
    "username": "string",
    "token": "jwt-token-string"
  }
}
```

#### 2.1.3 获取当前用户信息

```http
GET /api/auth/user/{user_id}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "uuid-string",
    "username": "string",
    "account_ids": ["account1", "account2"],
    "created_at": "2025-12-15T10:00:00Z"
  }
}
```

#### 2.1.4 获取所有用户列表（管理员）

```http
GET /api/auth/users
```

---

### 2.2 用户账户管理 (`/api/user`)

#### 2.2.1 为用户创建交易账户

```http
POST /api/user/{user_id}/account/create
Content-Type: application/json

{
  "account_id": "string (可选，不传则自动生成)",
  "initial_balance": 1000000.0
}
```

#### 2.2.2 获取用户的所有交易账户

```http
GET /api/user/{user_id}/accounts
```

**响应**:
```json
{
  "success": true,
  "data": {
    "accounts": [
      {
        "account_id": "account1",
        "balance": 1000000.0,
        "available": 950000.0,
        "margin": 50000.0
      }
    ]
  }
}
```

---

### 2.3 账户管理 (`/api/account`)

#### 2.3.1 开户

```http
POST /api/account/open
Content-Type: application/json

{
  "user_id": "string",
  "account_id": "string (可选)",
  "initial_balance": 1000000.0
}
```

#### 2.3.2 查询账户

```http
GET /api/account/{account_id}
```

**响应**（QIFI 格式）:
```json
{
  "success": true,
  "data": {
    "account_cookie": "account1",
    "accounts": {
      "user_id": "user1",
      "currency": "CNY",
      "pre_balance": 1000000.0,
      "balance": 1050000.0,
      "available": 900000.0,
      "margin": 100000.0,
      "frozen_margin": 0.0,
      "float_profit": 50000.0,
      "position_profit": 30000.0,
      "risk_ratio": 0.1
    },
    "positions": { ... },
    "orders": { ... },
    "trades": { ... }
  }
}
```

#### 2.3.3 存款

```http
POST /api/account/deposit
Content-Type: application/json

{
  "account_id": "string",
  "amount": 100000.0
}
```

#### 2.3.4 取款

```http
POST /api/account/withdraw
Content-Type: application/json

{
  "account_id": "string",
  "amount": 50000.0
}
```

#### 2.3.5 获取账户权益曲线

```http
GET /api/account/{user_id}/equity-curve
```

**响应**:
```json
{
  "success": true,
  "data": {
    "equity_curve": [
      { "timestamp": 1702627200000, "equity": 1000000.0 },
      { "timestamp": 1702713600000, "equity": 1050000.0 }
    ]
  }
}
```

#### 2.3.6 银期转账 (Phase 11) @yutiansut @quantaxis

```http
POST /api/account/transfer
Content-Type: application/json

{
  "account_id": "string",
  "bank_id": "string",
  "amount": 100000.0,
  "bank_password": "string",
  "future_password": "string"
}
```

**字段说明**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| amount | float | 是 | 金额（正数入金，负数出金） |
| bank_password | string | 是 | 银行密码 |
| future_password | string | 是 | 期货资金密码 |

**响应**:
```json
{
  "success": true,
  "data": {
    "transfer_id": "XFER_xxxx",
    "datetime": 1702627200000,
    "amount": 100000.0,
    "error_id": 0,
    "error_msg": "成功"
  }
}
```

#### 2.3.7 获取签约银行列表 (Phase 11) @yutiansut @quantaxis

```http
GET /api/account/{account_id}/banks
```

**响应**:
```json
{
  "success": true,
  "data": [
    { "id": "ICBC", "name": "工商银行" },
    { "id": "CCB", "name": "建设银行" },
    { "id": "ABC", "name": "农业银行" }
  ]
}
```

#### 2.3.8 获取转账记录 (Phase 11) @yutiansut @quantaxis

```http
GET /api/account/{account_id}/transfers
```

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "id": "XFER_xxxx",
      "datetime": 1702627200000,
      "currency": "CNY",
      "amount": 100000.0,
      "error_id": 0,
      "error_msg": "成功",
      "bank_id": "ICBC",
      "bank_name": "工商银行"
    }
  ]
}
```

---

### 2.4 订单管理 (`/api/order`)

#### 2.4.1 提交订单

```http
POST /api/order/submit
Content-Type: application/json

{
  "user_id": "string",
  "account_id": "string (推荐)",
  "instrument_id": "cu2512",
  "exchange_id": "SHFE",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 10,
  "price": 75000.0,
  "price_type": "LIMIT"
}
```

**字段说明**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| user_id | string | 是 | 用户ID |
| account_id | string | 推荐 | 交易账户ID |
| instrument_id | string | 是 | 合约代码 |
| exchange_id | string | 是 | 交易所代码 (SHFE/DCE/CZCE/CFFEX/INE) |
| direction | string | 是 | 方向 (BUY/SELL) |
| offset | string | 是 | 开平 (OPEN/CLOSE/CLOSETODAY/CLOSEYESTERDAY) |
| volume | int | 是 | 数量 |
| price | float | 条件 | 价格（限价单必填） |
| price_type | string | 是 | 价格类型 (LIMIT/MARKET/ANY) |

#### 2.4.2 撤单

```http
POST /api/order/cancel
Content-Type: application/json

{
  "user_id": "string",
  "account_id": "string (推荐)",
  "order_id": "string"
}
```

#### 2.4.3 查询订单

```http
GET /api/order/{order_id}
```

#### 2.4.4 查询用户订单

```http
GET /api/order/user/{user_id}
```

#### 2.4.5 批量下单 (Phase 11) @yutiansut @quantaxis

```http
POST /api/order/batch
Content-Type: application/json

{
  "account_id": "string",
  "orders": [
    {
      "instrument_id": "cu2512",
      "direction": "BUY",
      "offset": "OPEN",
      "volume": 10,
      "price": 75000.0,
      "order_type": "LIMIT"
    },
    {
      "instrument_id": "au2512",
      "direction": "SELL",
      "offset": "CLOSE",
      "volume": 5,
      "price": 480.0,
      "order_type": "LIMIT"
    }
  ]
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "total": 2,
    "success_count": 2,
    "failed_count": 0,
    "results": [
      {
        "success": true,
        "order_id": "ORD_xxx1",
        "instrument_id": "cu2512"
      },
      {
        "success": true,
        "order_id": "ORD_xxx2",
        "instrument_id": "au2512"
      }
    ]
  }
}
```

#### 2.4.6 批量撤单 (Phase 11) @yutiansut @quantaxis

```http
POST /api/order/batch-cancel
Content-Type: application/json

{
  "account_id": "string",
  "order_ids": ["ORD_xxx1", "ORD_xxx2"]
}
```

#### 2.4.7 修改订单 (Phase 11) @yutiansut @quantaxis

```http
PUT /api/order/modify/{order_id}
Content-Type: application/json

{
  "account_id": "string",
  "new_price": 75500.0,
  "new_volume": 8
}
```

#### 2.4.8 创建条件单 (Phase 11) @yutiansut @quantaxis

```http
POST /api/order/conditional
Content-Type: application/json

{
  "account_id": "string",
  "instrument_id": "cu2512",
  "direction": "SELL",
  "offset": "CLOSE",
  "volume": 10,
  "order_type": "MARKET",
  "limit_price": null,
  "condition_type": "StopLoss",
  "trigger_price": 74000.0,
  "trigger_condition": "LessOrEqual",
  "valid_until": 1702800000000
}
```

**字段说明**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| condition_type | string | 是 | 条件类型 (StopLoss/TakeProfit/PriceTouch) |
| trigger_price | float | 是 | 触发价格 |
| trigger_condition | string | 是 | 触发条件 (GreaterOrEqual/LessOrEqual) |
| valid_until | int | 否 | 有效期（毫秒时间戳） |

**响应**:
```json
{
  "success": true,
  "data": {
    "conditional_order_id": "COND_xxxx",
    "status": "Pending",
    "created_at": 1702627200000
  }
}
```

#### 2.4.9 查询条件单列表 (Phase 11) @yutiansut @quantaxis

```http
GET /api/order/conditional/list?account_id={account_id}
```

#### 2.4.10 取消条件单 (Phase 11) @yutiansut @quantaxis

```http
DELETE /api/order/conditional/{conditional_order_id}
```

#### 2.4.11 条件单统计 (Phase 11) @yutiansut @quantaxis

```http
GET /api/order/conditional/statistics
```

**响应**:
```json
{
  "success": true,
  "data": {
    "total": 100,
    "pending": 30,
    "triggered": 50,
    "cancelled": 15,
    "expired": 3,
    "failed": 2
  }
}
```

---

### 2.5 持仓查询 (`/api/position`)

#### 2.5.1 按账户查询持仓

```http
GET /api/position/account/{account_id}
```

#### 2.5.2 按用户查询所有持仓

```http
GET /api/position/user/{user_id}
```

**响应**（QIFI Position 格式）:
```json
{
  "success": true,
  "data": {
    "positions": {
      "SHFE.cu2512": {
        "exchange_id": "SHFE",
        "instrument_id": "cu2512",
        "volume_long": 10,
        "volume_short": 0,
        "open_price_long": 75000.0,
        "float_profit_long": 5000.0,
        "margin_long": 50000.0,
        "last_price": 75500.0
      }
    }
  }
}
```

---

### 2.6 成交记录查询 (`/api/trades`)

#### 2.6.1 按账户查询成交

```http
GET /api/trades/account/{account_id}
```

#### 2.6.2 按用户查询成交

```http
GET /api/trades/user/{user_id}
```

---

### 2.7 市场数据 (`/api/market`)

#### 2.7.1 获取合约列表

```http
GET /api/market/instruments
```

#### 2.7.2 获取订单簿

```http
GET /api/market/orderbook/{instrument_id}?depth=5
```

**响应**:
```json
{
  "success": true,
  "data": {
    "instrument_id": "cu2512",
    "bids": [
      { "price": 75000.0, "volume": 100, "order_count": 5 }
    ],
    "asks": [
      { "price": 75010.0, "volume": 80, "order_count": 3 }
    ],
    "timestamp": 1702627200000
  }
}
```

#### 2.7.3 获取 Tick 行情

```http
GET /api/market/tick/{instrument_id}
```

#### 2.7.4 获取最近成交

```http
GET /api/market/trades/{instrument_id}?limit=20
```

#### 2.7.5 获取K线数据

```http
GET /api/market/kline/{instrument_id}?period=5&count=500
```

**参数说明**:
- `period`: K线周期 (0=日线, 4=1分钟, 5=5分钟, 6=15分钟, 7=30分钟, 8=60分钟)
- `count`: 返回条数

---

### 2.8 监控统计 (`/api/monitoring`)

```http
GET /api/monitoring/system    # 系统监控
GET /api/monitoring/accounts  # 账户统计
GET /api/monitoring/orders    # 订单统计
GET /api/monitoring/trades    # 成交统计
GET /api/monitoring/storage   # 存储统计
GET /api/monitoring/report    # 生成报告
```

---

### 2.9 管理端路由

#### 2.9.1 账户管理 (`/api/management`)

```http
GET /api/management/accounts                # 所有账户列表
GET /api/management/account/{user_id}/detail # 账户详情
GET /api/management/orders                  # 全市场订单
GET /api/management/trades                  # 全市场成交
POST /api/management/deposit                # 管理端入金
POST /api/management/withdraw               # 管理端出金
GET /api/management/transactions/{user_id}  # 资金流水
```

#### 2.9.2 风控监控 (`/api/management/risk`)

```http
GET /api/management/risk/accounts           # 风险账户列表
GET /api/management/risk/margin-summary     # 保证金汇总
GET /api/management/risk/liquidations       # 强平记录
POST /api/management/risk/force-liquidate   # 触发强平
```

#### 2.9.3 合约管理 (`/api/admin`)

```http
GET /api/admin/instruments                  # 所有合约
POST /api/admin/instrument/create           # 创建合约
PUT /api/admin/instrument/{id}/update       # 更新合约
PUT /api/admin/instrument/{id}/suspend      # 暂停交易
PUT /api/admin/instrument/{id}/resume       # 恢复交易
DELETE /api/admin/instrument/{id}/delist    # 下市合约
```

#### 2.9.4 结算管理 (`/api/admin/settlement`)

```http
POST /api/admin/settlement/set-price        # 设置结算价
POST /api/admin/settlement/batch-set-prices # 批量设置结算价
POST /api/admin/settlement/execute          # 执行日终结算
GET /api/admin/settlement/history           # 结算历史
GET /api/admin/settlement/detail/{date}     # 结算详情
```

---

## 3. WebSocket 协议

### 3.1 连接地址

```
ws://{host}:8095/ws/diff?user_id={user_id}
```

**开发环境**（通过 Vue proxy）:
```
ws://{frontend_host}:8096/ws/diff?user_id={user_id}
```

### 3.2 消息格式

所有 WebSocket 消息均为 JSON 格式，通过 `aid` 字段标识消息类型。

### 3.3 客户端消息类型

#### 3.3.1 业务截面更新请求

```json
{
  "aid": "peek_message"
}
```

客户端发送此消息请求服务端推送最新的业务数据更新。

#### 3.3.2 登录请求

```json
{
  "aid": "req_login",
  "bid": "broker_id (可选)",
  "user_name": "username",
  "password": "password"
}
```

#### 3.3.3 订阅行情

```json
{
  "aid": "subscribe_quote",
  "ins_list": "SHFE.cu2512,CFFEX.IF2501"
}
```

**注意**:
- 合约代码必须带交易所前缀
- 多个合约用逗号分隔
- 后一次订阅会覆盖前一次

#### 3.3.4 下单

```json
{
  "aid": "insert_order",
  "user_id": "user1",
  "account_id": "account1 (推荐)",
  "order_id": "order_001 (可选)",
  "exchange_id": "SHFE",
  "instrument_id": "cu2512",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 10,
  "price_type": "LIMIT",
  "limit_price": 75000.0,
  "volume_condition": "ANY",
  "time_condition": "GFD"
}
```

#### 3.3.5 撤单

```json
{
  "aid": "cancel_order",
  "user_id": "user1",
  "account_id": "account1 (推荐)",
  "order_id": "order_001"
}
```

#### 3.3.6 订阅图表数据（K线）

```json
{
  "aid": "set_chart",
  "chart_id": "chart_main",
  "ins_list": "SHFE.cu2512",
  "duration": 300000000000,
  "view_width": 500
}
```

**duration 参数说明**:
| 周期 | duration (ns) |
|------|---------------|
| Tick | 0 |
| 1分钟 | 60000000000 |
| 5分钟 | 300000000000 |
| 15分钟 | 900000000000 |
| 30分钟 | 1800000000000 |
| 60分钟 | 3600000000000 |
| 日线 | 86400000000000 |

---

### 3.4 服务端消息类型

#### 3.4.1 业务截面更新 (`rtn_data`)

```json
{
  "aid": "rtn_data",
  "data": [
    { "balance": 1050000.0 },
    { "quotes": { "SHFE.cu2512": { "last_price": 75500.0 } } },
    { "positions": { "SHFE.cu2512": { "float_profit": 5000.0 } } }
  ]
}
```

**处理规则**:
- `data` 数组中每个元素是一个 **JSON Merge Patch** (RFC 7386)
- 客户端需按顺序应用所有 patch 到本地业务截面
- 处理完整个数组后，业务截面才是一致的

---

## 4. DIFF 协议详解

### 4.1 设计理念

DIFF (Differential Information Flow for Finance) 协议将**异步事件回调转为同步数据访问**：

```
传统模式:                    DIFF模式:
┌─────────┐                 ┌─────────┐
│ 事件回调 │                 │ 业务截面 │
│ onTrade │                 │ snapshot│
│ onOrder │    ──────>      │ {       │
│ onQuote │                 │   trade │
│ ...     │                 │   order │
└─────────┘                 │   quote │
                            │ }       │
                            └─────────┘
```

### 4.2 业务截面结构

```json
{
  "ins_list": "SHFE.cu2512,CFFEX.IF2501",
  "quotes": {
    "SHFE.cu2512": {
      "instrument_id": "SHFE.cu2512",
      "datetime": "2025-12-15 10:30:00.000000",
      "last_price": 75500.0,
      "bid_price1": 75490.0,
      "bid_volume1": 50,
      "ask_price1": 75510.0,
      "ask_volume1": 30,
      "volume": 12345,
      "open_interest": 98765
    }
  },
  "trade": {
    "user1": {
      "user_id": "user1",
      "accounts": {
        "CNY": {
          "balance": 1050000.0,
          "available": 900000.0,
          "margin": 100000.0,
          "float_profit": 50000.0
        }
      },
      "positions": {
        "SHFE.cu2512": {
          "volume_long": 10,
          "open_price_long": 75000.0,
          "float_profit_long": 5000.0
        }
      },
      "orders": {
        "order_001": {
          "order_id": "order_001",
          "status": "FINISHED",
          "volume_left": 0
        }
      },
      "trades": {
        "trade_001": {
          "trade_id": "trade_001",
          "price": 75000.0,
          "volume": 10
        }
      }
    }
  },
  "klines": {
    "SHFE.cu2512": {
      "300000000000": {
        "last_id": 1234,
        "data": {
          "1230": {
            "datetime": 1702627200000000000,
            "open": 75000.0,
            "high": 75600.0,
            "low": 74900.0,
            "close": 75500.0,
            "volume": 1000
          }
        }
      }
    }
  },
  "notify": {
    "1001": {
      "type": "MESSAGE",
      "level": "INFO",
      "code": 1000,
      "content": "登录成功"
    }
  }
}
```

### 4.3 数据同步流程

```
┌──────────┐                         ┌──────────┐
│  Client  │                         │  Server  │
└────┬─────┘                         └────┬─────┘
     │                                    │
     │  1. connect (ws://host/ws/diff)    │
     │ ─────────────────────────────────> │
     │                                    │
     │  2. { "aid": "peek_message" }      │
     │ ─────────────────────────────────> │
     │                                    │
     │  3. { "aid": "rtn_data", ... }     │
     │ <───────────────────────────────── │
     │                                    │
     │  [收到数据后立即发送下一个 peek]    │
     │  4. { "aid": "peek_message" }      │
     │ ─────────────────────────────────> │
     │                                    │
     │  [服务端无更新则等待]               │
     │  ...                               │
     │                                    │
     │  5. { "aid": "rtn_data", ... }     │
     │ <───────────────────────────────── │
     │                                    │
```

### 4.4 JSON Merge Patch 示例

**原始截面**:
```json
{
  "balance": 1000000.0,
  "positions": {
    "SHFE.cu2512": { "volume": 10, "profit": 0 }
  }
}
```

**收到 Patch**:
```json
[
  { "balance": 1050000.0 },
  { "positions": { "SHFE.cu2512": { "profit": 5000 } } }
]
```

**更新后截面**:
```json
{
  "balance": 1050000.0,
  "positions": {
    "SHFE.cu2512": { "volume": 10, "profit": 5000 }
  }
}
```

---

## 5. 数据结构定义

### 5.1 QIFI 账户结构

```typescript
interface QIFIAccount {
  account_cookie: string;
  accounts: {
    user_id: string;
    currency: string;
    pre_balance: number;      // 昨日权益
    deposit: number;          // 入金
    withdraw: number;         // 出金
    static_balance: number;   // 静态权益
    close_profit: number;     // 平仓盈亏
    commission: number;       // 手续费
    float_profit: number;     // 浮动盈亏
    position_profit: number;  // 持仓盈亏
    balance: number;          // 当前权益
    margin: number;           // 保证金
    frozen_margin: number;    // 冻结保证金
    available: number;        // 可用资金
    risk_ratio: number;       // 风险度
  };
  positions: Record<string, Position>;
  orders: Record<string, Order>;
  trades: Record<string, Trade>;
}
```

### 5.2 持仓结构

```typescript
interface Position {
  user_id: string;
  exchange_id: string;
  instrument_id: string;
  volume_long: number;
  volume_long_today: number;
  volume_long_his: number;
  volume_short: number;
  volume_short_today: number;
  volume_short_his: number;
  open_price_long: number;
  open_price_short: number;
  position_price_long: number;
  position_price_short: number;
  open_cost_long: number;
  open_cost_short: number;
  position_cost_long: number;
  position_cost_short: number;
  float_profit_long: number;
  float_profit_short: number;
  float_profit: number;
  position_profit_long: number;
  position_profit_short: number;
  position_profit: number;
  margin_long: number;
  margin_short: number;
  margin: number;
  last_price: number;
}
```

### 5.3 订单结构

```typescript
interface Order {
  user_id: string;
  order_id: string;
  exchange_id: string;
  instrument_id: string;
  direction: 'BUY' | 'SELL';
  offset: 'OPEN' | 'CLOSE' | 'CLOSETODAY' | 'CLOSEYESTERDAY';
  volume_orign: number;
  volume_left: number;
  price_type: 'LIMIT' | 'MARKET' | 'ANY';
  limit_price: number;
  time_condition: 'IOC' | 'GFS' | 'GFD' | 'GTD' | 'GTC' | 'GFA';
  volume_condition: 'ANY' | 'MIN' | 'ALL';
  insert_date_time: number;     // 纳秒时间戳
  exchange_order_id: string;
  status: 'ALIVE' | 'FINISHED';
  last_msg: string;
}
```

### 5.4 成交结构

```typescript
interface Trade {
  user_id: string;
  trade_id: string;
  order_id: string;
  exchange_id: string;
  instrument_id: string;
  exchange_trade_id: string;
  direction: 'BUY' | 'SELL';
  offset: 'OPEN' | 'CLOSE';
  price: number;
  volume: number;
  trade_date_time: number;      // 纳秒时间戳
  commission: number;
}
```

### 5.5 行情结构

```typescript
interface Quote {
  instrument_id: string;
  datetime: string;
  last_price: number;
  highest: number;
  lowest: number;
  open: number;
  close: number;
  pre_close: number;
  pre_settlement: number;
  settlement: number;
  upper_limit: number;
  lower_limit: number;
  bid_price1: number;
  bid_volume1: number;
  ask_price1: number;
  ask_volume1: number;
  volume: number;
  amount: number;
  open_interest: number;
  pre_open_interest: number;
}
```

### 5.6 K线结构

```typescript
interface Kline {
  datetime: number;     // 纳秒时间戳
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  open_oi: number;
  close_oi: number;
}
```

---

## 6. 错误码定义

### 6.1 通用错误码

| 错误码 | 说明 |
|--------|------|
| 1000 | 成功 |
| 1001 | 参数错误 |
| 1002 | 认证失败 |
| 1003 | 权限不足 |
| 1004 | 资源不存在 |
| 1005 | 服务内部错误 |

### 6.2 账户错误码

| 错误码 | 说明 |
|--------|------|
| 2001 | 账户不存在 |
| 2002 | 账户已存在 |
| 2003 | 余额不足 |
| 2004 | 保证金不足 |
| 2005 | 账户被冻结 |

### 6.3 订单错误码

| 错误码 | 说明 |
|--------|------|
| 3001 | 订单不存在 |
| 3002 | 订单已完成 |
| 3003 | 合约不存在 |
| 3004 | 合约已停牌 |
| 3005 | 超出持仓限制 |
| 3006 | 价格超出涨跌停 |
| 3007 | 数量不合法 |

### 6.4 风控错误码

| 错误码 | 说明 |
|--------|------|
| 4001 | 风险度超限 |
| 4002 | 触发强平 |
| 4003 | 超出保证金率 |

---

## 7. 前端集成指南

### 7.1 环境变量配置

**`.env.development`**:
```bash
# 使用相对路径，通过 Vue devServer 代理
VUE_APP_WS_URL=/ws/diff
VUE_APP_API_BASE_URL=/api
VUE_APP_DEFAULT_INSTRUMENTS=CFFEX.IF2501,SHFE.cu2501
```

### 7.2 Vue.config.js 代理配置

```javascript
module.exports = {
  devServer: {
    port: 8096,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8094',
        changeOrigin: true
      },
      '/ws': {
        target: 'http://127.0.0.1:8095',
        changeOrigin: true,
        ws: true
      }
    }
  }
}
```

### 7.3 WebSocket 连接示例

```javascript
class WebSocketManager {
  constructor(options = {}) {
    this.options = {
      url: process.env.VUE_APP_WS_URL || '/ws/diff',
      userId: null,
      ...options
    }
    this.snapshot = {}
  }

  connect(userId) {
    let wsUrl = this.options.url
    if (wsUrl.startsWith('/')) {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      wsUrl = `${protocol}//${window.location.host}${wsUrl}`
    }

    this.ws = new WebSocket(`${wsUrl}?user_id=${userId}`)

    this.ws.onopen = () => {
      this.sendPeekMessage()
    }

    this.ws.onmessage = (event) => {
      this.handleMessage(JSON.parse(event.data))
    }
  }

  sendPeekMessage() {
    this.ws.send(JSON.stringify({ aid: 'peek_message' }))
  }

  handleMessage(message) {
    if (message.aid === 'rtn_data') {
      // 应用 JSON Merge Patch
      for (const patch of message.data) {
        this.applyPatch(this.snapshot, patch)
      }
      // 发送下一个 peek_message
      this.sendPeekMessage()
    }
  }

  applyPatch(target, patch) {
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) {
        delete target[key]
      } else if (typeof value === 'object' && !Array.isArray(value)) {
        if (!target[key]) target[key] = {}
        this.applyPatch(target[key], value)
      } else {
        target[key] = value
      }
    }
  }
}
```

### 7.4 Vuex Store 集成

```javascript
// store/modules/websocket.js
export default {
  namespaced: true,
  state: {
    connected: false,
    snapshot: {}
  },
  mutations: {
    SET_CONNECTED(state, connected) {
      state.connected = connected
    },
    UPDATE_SNAPSHOT(state, patches) {
      for (const patch of patches) {
        applyMergePatch(state.snapshot, patch)
      }
    }
  },
  getters: {
    quotes: state => state.snapshot.quotes || {},
    positions: state => state.snapshot.trade?.[state.userId]?.positions || {},
    orders: state => state.snapshot.trade?.[state.userId]?.orders || {},
    account: state => state.snapshot.trade?.[state.userId]?.accounts?.CNY || {}
  }
}
```

---

## 附录

### A. 交易所代码

| 代码 | 名称 |
|------|------|
| SHFE | 上海期货交易所 |
| DCE | 大连商品交易所 |
| CZCE | 郑州商品交易所 |
| CFFEX | 中国金融期货交易所 |
| INE | 上海国际能源交易中心 |
| USER | 用户自定义组合 |

### B. 订单状态

| 状态 | 说明 |
|------|------|
| ALIVE | 活跃（未成交/部分成交） |
| FINISHED | 已完成（全部成交/已撤销） |

### C. 价格类型

| 类型 | 说明 |
|------|------|
| LIMIT | 限价 |
| MARKET | 市价 |
| ANY | 任意价 |
| BEST | 最优价 |
| FIVELEVEL | 五档价 |

### D. 时间条件

| 条件 | 说明 |
|------|------|
| IOC | 立即完成，否则撤销 |
| GFS | 本节有效 |
| GFD | 当日有效 |
| GTD | 指定日期前有效 |
| GTC | 撤销前有效 |
| GFA | 集合竞价有效 |

---

**文档版本**: 1.0
**最后更新**: 2025-12-15
**维护者**: @yutiansut @quantaxis
