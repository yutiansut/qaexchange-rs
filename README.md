# QAEXCHANGE-RS

高性能量化交易所系统 - 基于 QARS 核心架构构建

## 项目概述

`qaexchange-rs` 是一个完整的交易所系统实现，支持万级账户并发交易，提供真实交易所级别的功能。

### 核心特性

✅ **账户管理**: 开户/入金/出金/查询 (复用 qars::qaaccount)
✅ **订单系统**: 下单/撤单/订单路由 (复用 qars::QAOrder)
✅ **撮合引擎**: 价格时间优先/集合竞价/连续交易 (复用 qars::matchengine)
🚧 **成交回报**: 实时成交推送/账户更新
🚧 **行情推送**: Level1/Level2/逐笔成交 (基于 qars::broadcast_hub)
🚧 **结算系统**: 日终结算/盯市盈亏/强平处理
🚧 **风控系统**: 盘前风控/持仓限额/自成交防范
🚧 **对外服务**: WebSocket + HTTP API (Actix-web)

## 快速开始

### 编译项目

```bash
cd /home/quantaxis/qaexchange-rs

# 编译库
cargo build --lib

# 编译服务器
cargo build --bin qaexchange-server

# 编译所有示例
cargo build --examples
```

### 运行示例

```bash
# 运行账户开户示例
cargo run --example start_exchange

# 预期输出:
# === QAEXCHANGE Demo ===
#
# ✓ Account opened: demo_user
#   Balance: 1000000
#   Available: 1000000
#
# Demo completed.
```

### 启动服务器

```bash
cargo run --bin qaexchange-server
```

## 项目架构

```
qaexchange-rs/
├── src/
│   ├── lib.rs                    # 库入口
│   ├── main.rs                   # 服务器入口
│   │
│   ├── core/                     # 核心模块 (复用 qars)
│   │   ├── account_ext.rs        # 账户扩展
│   │   └── order_ext.rs          # 订单扩展
│   │
│   ├── matching/                 # 撮合引擎
│   │   ├── engine.rs             # 撮合引擎封装
│   │   ├── auction.rs            # 集合竞价
│   │   └── trade_recorder.rs     # 成交记录器
│   │
│   ├── exchange/                 # 交易所业务
│   │   ├── account_mgr.rs        # 账户管理中心
│   │   ├── capital_mgr.rs        # 资金管理
│   │   ├── order_router.rs       # 订单路由
│   │   ├── trade_gateway.rs      # 成交回报网关
│   │   ├── settlement.rs         # 结算系统
│   │   └── instrument_registry.rs # 合约注册表
│   │
│   ├── risk/                     # 风控系统
│   ├── market/                   # 行情推送
│   ├── service/                  # 对外服务
│   │   ├── websocket/            # WebSocket 服务
│   │   └── http/                 # HTTP API
│   ├── storage/                  # 持久化存储
│   ├── protocol/                 # 协议层 (QIFI/TIFI/MIFI)
│   └── utils/                    # 工具模块
│
├── examples/                     # 示例代码
│   ├── start_exchange.rs         # 启动示例
│   ├── client_demo.rs            # 客户端示例
│   └── stress_test.rs            # 压力测试
│
└── tests/                        # 集成测试
```

## 核心复用能力

| 模块 | qars 复用 | 复用度 | 说明 |
|------|----------|--------|------|
| 账户系统 | `QA_Account` | ⭐⭐⭐⭐⭐ 95% | 完整复用账户/持仓管理 |
| 订单系统 | `QAOrder` + QIFI | ⭐⭐⭐⭐⭐ 90% | 订单结构和协议 |
| 撮合引擎 | `Orderbook` | ⭐⭐⭐⭐⭐ 98% | 价格时间优先撮合 |
| 协议层 | QIFI/TIFI/MIFI | ⭐⭐⭐⭐⭐ 100% | 完全复用 |
| 数据广播 | `broadcast_hub` | ⭐⭐⭐⭐⭐ 95% | 零拷贝行情推送 |

## 性能目标

基于 qars 性能基准测试:

| 指标 | 目标值 | 依据 |
|------|--------|------|
| **订单吞吐量** | > 100K orders/sec | 复用 `Orderbook` |
| **撮合延迟** | P99 < 100μs | qars 撮合引擎 |
| **行情推送延迟** | P99 < 10μs | `broadcast_hub` |
| **并发账户数** | > 10,000 | DashMap 无锁并发 |
| **并发订阅者** | > 1,000 | iceoryx2 零拷贝 |

## API 设计

### HTTP API (计划)

```
POST   /api/v1/account/open            # 开户
POST   /api/v1/account/deposit         # 入金
POST   /api/v1/account/withdraw        # 出金
GET    /api/v1/account/{user_id}       # 查询账户

POST   /api/v1/order/submit            # 下单
POST   /api/v1/order/cancel            # 撤单
GET    /api/v1/order/query             # 查询订单

GET    /api/v1/market/instruments      # 查询合约列表
GET    /api/v1/market/tick/{code}      # 查询行情
GET    /api/v1/market/trades/{code}    # 查询成交记录
```

### WebSocket API (计划)

```
ws://host:port/trade         # 交易通道 (下单/撤单/成交回报)
ws://host:port/market        # 行情通道 (Level1/Level2/逐笔)
```

## 数据流

```
客户端 (WebSocket/HTTP)
    ↓
Service Layer (service/)
    ↓
OrderRouter (订单路由)
    ├─> PreTradeCheck (风控检查)
    └─> ExchangeMatchingEngine (撮合)
            ↓
        TradeGateway (成交回报)
            ↓
        DataBroadcaster (广播推送)
            ↓
        订阅者 (客户端/监控系统)
```

## 开发状态

### ✅ 已完成

- [x] 项目架构设计
- [x] 核心模块框架
- [x] 账户管理 (AccountManager)
- [x] 撮合引擎封装 (ExchangeMatchingEngine)
- [x] 成交记录器 (TradeRecorder)
- [x] 合约注册表 (InstrumentRegistry)
- [x] 基础示例程序

### 🚧 进行中

- [ ] 订单路由完整实现
- [ ] 成交回报网关
- [ ] 结算系统
- [ ] 风控系统
- [ ] WebSocket 服务
- [ ] HTTP API 服务

### 📋 待开发

- [ ] 行情推送 (Level2)
- [ ] 集合竞价完善
- [ ] 数据持久化
- [ ] 监控指标
- [ ] 压力测试
- [ ] 文档完善

## 技术栈

### 核心依赖

- **qars (qa-rs)**: 核心账户/订单/撮合引擎 (本地依赖)
- **Actix-web**: Web 框架
- **Tokio**: 异步运行时
- **DashMap**: 无锁并发 HashMap
- **parking_lot**: 高性能锁
- **iceoryx2**: 零拷贝 IPC (通过 qars)

### 数据结构

- **QIFI**: 账户数据格式
- **TIFI**: 交易指令格式
- **MIFI**: 市场数据格式

## 参考真实交易所

设计参考:
- **CTP**: 上期技术综合交易平台
- **上交所**: 上海证券交易所
- **深交所**: 深圳证券交易所

核心流程:
1. 账户注册 → 2. 入金 → 3. 下单 → 4. 撮合 → 5. 成交回报 → 6. 结算

## 许可证

MIT

## 联系方式

基于 QUANTAXIS 项目构建
