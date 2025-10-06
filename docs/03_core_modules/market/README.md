# 市场数据模块 (Market Data Module)

市场数据模块负责处理交易所的行情数据生成、分发和查询，是 QAExchange 的核心数据服务层。

---

## 模块组成

| 组件 | 文件 | 描述 | 状态 |
|------|------|------|------|
| **快照生成器** | `snapshot_generator.rs` | 每秒级别市场快照生成 | ✅ 完成 |
| **市场数据服务** | `mod.rs` | 业务逻辑层，统一数据访问接口 | ✅ 完成 |
| **数据广播器** | `broadcaster.rs` | Level2 行情广播（订单簿、Tick） | ✅ 完成 |
| **快照广播服务** | `snapshot_broadcaster.rs` | Tokio 异步快照广播 | ✅ 完成 |
| **数据缓存** | `cache.rs` | L1 缓存（DashMap，100ms TTL） | ✅ 完成 |
| **数据恢复** | `recovery.rs` | 从 WAL 恢复市场数据 | ✅ 完成 |

---

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                     MarketDataService                        │
│                   (业务逻辑层 - 统一入口)                       │
├─────────────────────────────────────────────────────────────┤
│  - 订单簿查询 (get_orderbook_snapshot)                       │
│  - Tick 数据查询 (get_tick_data)                             │
│  - 合约列表查询 (get_instruments)                            │
│  - 成交记录查询 (get_recent_trades)                          │
└────────────┬────────────────────────────────────────────────┘
             │
    ┌────────┴────────┐
    ▼                 ▼
┌──────────────┐  ┌──────────────────┐
│  L1 Cache    │  │ SnapshotGenerator│
│  (DashMap)   │  │  (每秒级别)       │
│  100ms TTL   │  │  - OHLC          │
│              │  │  - 买卖5档        │
└──────────────┘  │  - 成交统计       │
                  └──────────────────┘
         ▲                 │
         │                 ▼
    ┌────┴─────────────────────┐
    │   MarketDataBroadcaster   │ (实时行情广播)
    │   - OrderBookSnapshot     │
    │   - Tick                  │
    │   - LastPrice             │
    └───────────────────────────┘
              │
              ▼
     ┌────────────────┐
     │  Subscribers   │ (WebSocket/IPC)
     └────────────────┘
```

---

## 核心功能

### 1. 市场快照生成

**快照生成器** (`snapshot_generator.rs`) 提供每秒级别的完整市场行情快照：

- **35+ 字段**: OHLC、买卖五档、成交量额、涨跌幅
- **自动统计**: 日内 OHLC、累计成交量/额
- **零拷贝订阅**: 基于 crossbeam channel 的发布-订阅

**文档**: [快照生成器详细文档](./snapshot_generator.md)

```rust
// 快速开始
let market_data_service = MarketDataService::new(matching_engine)
    .with_snapshot_generator(vec!["IF2501".to_string()], 1000);

market_data_service.start_snapshot_generator();

if let Some(snapshot_rx) = market_data_service.subscribe_snapshots() {
    while let Ok(snapshot) = snapshot_rx.recv() {
        println!("快照: {} @ {:.2}", snapshot.instrument_id, snapshot.last_price);
    }
}
```

### 2. 订单簿查询

提供三级缓存架构查询订单簿快照：

```rust
// L1: DashMap 缓存（<10μs）
// L2: WAL 存储恢复（<5ms）
// L3: 实时计算（<50μs）
let snapshot = market_data_service.get_orderbook_snapshot("IF2501", 5)?;
```

**缓存策略**:
- TTL: 100ms（可配置）
- 缓存命中率: >95%（生产环境）
- 缓存未命中自动回源

### 3. 行情广播

**MarketDataBroadcaster** 支持实时推送订单簿变化和成交数据：

```rust
// 订阅市场数据
let receiver = broadcaster.subscribe(
    "session_id".to_string(),
    vec!["IF2501".to_string()],  // 订阅合约
    vec!["orderbook", "tick"],   // 订阅频道
);

// 接收事件
while let Ok(event) = receiver.recv() {
    match event {
        MarketDataEvent::OrderBookSnapshot { bids, asks, .. } => {
            println!("订单簿更新: {} bids, {} asks", bids.len(), asks.len());
        }
        MarketDataEvent::Tick { price, volume, .. } => {
            println!("成交: {} @ {}", volume, price);
        }
        _ => {}
    }
}
```

### 4. 数据恢复

从 WAL 恢复最近 N 分钟的市场数据：

```rust
// 恢复最近 10 分钟数据到缓存
market_data_service.recover_recent_market_data(10)?;
```

**恢复统计**:
```
✅ [Market Data Recovery] Recovered 1234 ticks, 567 orderbooks in 124ms
```

---

## 性能指标

| 指标 | 目标值 | 实际值 | 备注 |
|------|--------|--------|------|
| Tick 查询延迟 (L1) | < 10μs | ~5μs | DashMap 缓存 |
| 订单簿查询延迟 (L1) | < 50μs | ~20μs | DashMap 缓存 |
| WAL 恢复速度 | < 5s | ~0.1s/分钟 | 10分钟数据 < 1s |
| 快照生成延迟 | < 1ms | ~200μs | 5档深度 |
| 缓存命中率 | > 90% | 95%+ | 生产环境 |
| 并发订阅者 | > 1000 | 无限制 | crossbeam |

---

## 数据流

### 行情数据生成流程

```
┌──────────────┐
│ TradeGateway │ (成交事件)
└──────┬───────┘
       │
       ▼
  update_trade_stats()
       │
       ▼
┌────────────────────┐
│ SnapshotGenerator  │ (统计更新)
│  - volume += v     │
│  - turnover += t   │
│  - high = max()    │
│  - low = min()     │
└────────┬───────────┘
         │
         ▼ (每秒触发)
   generate_snapshot()
         │
         ▼
┌────────────────────┐
│  MarketSnapshot    │ (完整快照)
│  - OHLC            │
│  - 买卖5档          │
│  - 成交统计         │
└────────┬───────────┘
         │
         ▼
   broadcast()
         │
    ┌────┴────┐
    ▼         ▼
[订阅者1] [订阅者N]
```

### 查询流程

```
Client Request
    │
    ▼
get_orderbook_snapshot()
    │
    ├─ L1 Cache Hit? ──Yes──> Return (5μs)
    │       │
    │      No
    │       ▼
    ├─ L2 WAL Hit? ──Yes──> Update Cache + Return (5ms)
    │       │
    │      No
    │       ▼
    └─ L3 Compute ────────> Update Cache + Return (50μs)
```

---

## 配置

### MarketDataService 配置

```rust
let market_data_service = MarketDataService::new(matching_engine)
    // 设置存储（用于 L2 恢复）
    .with_storage(market_data_storage)
    // 设置 iceoryx2（零拷贝 IPC）
    .with_iceoryx(iceoryx_manager)
    // 配置快照生成器
    .with_snapshot_generator(
        vec!["IF2501".to_string()],  // 订阅合约
        1000,                         // 1秒间隔
    );
```

### 快照生成器配置

```rust
let config = SnapshotGeneratorConfig {
    interval_ms: 1000,            // 快照间隔（毫秒）
    enable_persistence: false,    // WAL 持久化（待实现）
    instruments: vec![
        "IF2501".to_string(),
        "IC2501".to_string(),
    ],
};
```

---

## API 参考

### MarketDataService

| 方法 | 描述 | 复杂度 |
|------|------|--------|
| `get_orderbook_snapshot(id, depth)` | 查询订单簿快照 | O(depth) |
| `get_tick_data(id)` | 查询 Tick 数据 | O(1) |
| `get_instruments()` | 查询合约列表 | O(n) |
| `get_recent_trades(id, limit)` | 查询成交记录 | O(limit) |
| `subscribe_snapshots()` | 订阅市场快照 | O(1) |
| `update_trade_stats(id, vol, amt)` | 更新成交统计 | O(1) |
| `set_pre_close(id, price)` | 设置昨收盘价 | O(1) |
| `recover_recent_market_data(mins)` | WAL 恢复数据 | O(n) |

### MarketSnapshotGenerator

| 方法 | 描述 | 复杂度 |
|------|------|--------|
| `new(engine, config)` | 创建生成器 | O(1) |
| `start()` | 启动后台线程 | O(1) |
| `subscribe()` | 订阅快照 | O(1) |
| `set_pre_close(id, price)` | 设置昨收盘价 | O(1) |
| `update_trade_stats(id, vol, amt)` | 更新统计 | O(1) |
| `reset_daily_stats()` | 重置统计 | O(n) |
| `get_snapshot_count()` | 获取生成数 | O(1) |

---

## 测试

### 运行测试

```bash
# 单元测试
cargo test --lib market::

# 集成测试
cargo run --example test_snapshot_generator

# 性能测试
cargo run --example test_snapshot_generator --release
```

### 测试覆盖

- ✅ 快照生成正确性
- ✅ 多订阅者并发消费
- ✅ 统计累加正确性
- ✅ 缓存命中/未命中
- ✅ WAL 恢复功能
- ⏳ WebSocket 推送测试
- ⏳ 压力测试（1000+ 订阅者）

---

## 常见问题

### 1. 如何订阅实时行情？

```rust
// 方案1: 订阅快照（每秒级别）
let snapshot_rx = market_data_service.subscribe_snapshots()?;

// 方案2: 订阅广播事件（毫秒级别）
let event_rx = market_broadcaster.subscribe(
    "session_id".to_string(),
    vec!["IF2501".to_string()],
    vec!["orderbook", "tick"],
);
```

### 2. 如何提高查询性能？

- 使用 L1 缓存（默认启用，100ms TTL）
- 批量查询合约列表
- 启用 WAL 恢复预热缓存

### 3. 如何持久化快照数据？

目前快照暂未持久化，计划在 Phase 5 实现：

```rust
// 未来支持
let config = SnapshotGeneratorConfig {
    enable_persistence: true,  // 启用 WAL 持久化
    // ...
};
```

---

## 路线图

- [x] **Phase 1-3**: 基础快照生成器 + MarketDataService 集成
- [ ] **Phase 4**: WebSocket 订阅端点
- [ ] **Phase 5**: WAL 持久化快照
- [ ] **Phase 6**: iceoryx2 零拷贝 IPC
- [ ] **Phase 7**: K线数据生成器
- [ ] **Phase 8**: 实时技术指标计算

---

## 相关文档

- [快照生成器详细文档](./snapshot_generator.md)
- [系统架构](../../02_architecture/system_overview.md)
- [WebSocket API](../../04_api/websocket/protocol.md)
- [性能优化](../../02_architecture/high_performance.md)

---

**@yutiansut @quantaxis** - 2025-01-07
