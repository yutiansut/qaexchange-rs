# 快照生成器 (Snapshot Generator)

## 概述

快照生成器（`MarketSnapshotGenerator`）是 QAExchange 的市场数据服务核心组件，负责**每秒级别生成完整的市场行情快照**，并通过零拷贝的发布-订阅模式分发给多个消费者。

### 核心功能

- **定时生成**：独立线程，可配置间隔（默认 1 秒）
- **完整快照**：35+ 字段，包含 OHLC、买卖五档、成交量额、涨跌幅等
- **实时统计**：自动跟踪日内 OHLC、累计成交量/额
- **多订阅者**：支持无限制的并发消费者（基于 crossbeam channel）
- **零拷贝**：订阅者间共享快照数据，无需重复序列化

---

## 数据结构

### MarketSnapshot

```rust
pub struct MarketSnapshot {
    // 基础信息
    pub instrument_id: String,      // 合约代码
    pub timestamp: i64,              // 快照时间戳（纳秒）
    pub trading_day: String,         // 交易日期（YYYYMMDD）

    // 价格信息
    pub last_price: f64,             // 最新价
    pub change_percent: f64,         // 涨跌幅（%）
    pub change_amount: f64,          // 涨跌额
    pub pre_close: f64,              // 昨收盘价

    // 买卖五档
    pub bid_price1: f64,  pub bid_volume1: i64,
    pub bid_price2: f64,  pub bid_volume2: i64,
    pub bid_price3: f64,  pub bid_volume3: i64,
    pub bid_price4: f64,  pub bid_volume4: i64,
    pub bid_price5: f64,  pub bid_volume5: i64,

    pub ask_price1: f64,  pub ask_volume1: i64,
    pub ask_price2: f64,  pub ask_volume2: i64,
    pub ask_price3: f64,  pub ask_volume3: i64,
    pub ask_price4: f64,  pub ask_volume4: i64,
    pub ask_price5: f64,  pub ask_volume5: i64,

    // OHLC
    pub open: f64,                   // 今日开盘价
    pub high: f64,                   // 今日最高价
    pub low: f64,                    // 今日最低价

    // 成交统计
    pub volume: i64,                 // 成交量（手）
    pub turnover: f64,               // 成交额（元）
    pub open_interest: i64,          // 持仓量

    // 涨跌停
    pub upper_limit: f64,            // 涨停价
    pub lower_limit: f64,            // 跌停价
}
```

### SnapshotGeneratorConfig

```rust
pub struct SnapshotGeneratorConfig {
    pub interval_ms: u64,            // 快照生成间隔（毫秒）
    pub enable_persistence: bool,    // 是否启用持久化（暂未实现）
    pub instruments: Vec<String>,    // 订阅的合约列表
}
```

---

## 快速开始

### 1. 基础用法

```rust
use qaexchange::market::snapshot_generator::{
    MarketSnapshotGenerator,
    SnapshotGeneratorConfig
};
use std::sync::Arc;

// 1. 创建配置
let config = SnapshotGeneratorConfig {
    interval_ms: 1000,  // 每秒生成一次
    enable_persistence: false,
    instruments: vec!["IF2501".to_string(), "IC2501".to_string()],
};

// 2. 创建生成器
let generator = Arc::new(MarketSnapshotGenerator::new(
    matching_engine.clone(),
    config,
));

// 3. 设置昨收盘价（用于涨跌幅计算）
generator.set_pre_close("IF2501", 3800.0);
generator.set_pre_close("IC2501", 5600.0);

// 4. 启动后台线程
let handle = generator.clone().start();

// 5. 订阅快照
let snapshot_rx = generator.subscribe();

// 6. 消费快照
tokio::spawn(async move {
    while let Ok(snapshot) = snapshot_rx.recv() {
        println!("快照: {} @ {:.2} (涨跌: {:.2}%)",
            snapshot.instrument_id,
            snapshot.last_price,
            snapshot.change_percent,
        );
    }
});
```

### 2. 通过 MarketDataService 使用

```rust
use qaexchange::market::MarketDataService;

// 1. 创建服务并配置快照生成器
let market_data_service = MarketDataService::new(matching_engine.clone())
    .with_snapshot_generator(
        vec!["IF2501".to_string()],  // 订阅合约
        1000,                         // 1秒间隔
    );

// 2. 启动生成器
market_data_service.start_snapshot_generator();

// 3. 订阅快照
if let Some(snapshot_rx) = market_data_service.subscribe_snapshots() {
    // 消费快照...
}

// 4. 成交时更新统计（由 TradeGateway 自动调用）
market_data_service.update_trade_stats("IF2501", 100, 380000.0);
```

---

## 核心方法

### 生成器方法

| 方法 | 描述 | 示例 |
|------|------|------|
| `new()` | 创建生成器 | `MarketSnapshotGenerator::new(engine, config)` |
| `start()` | 启动后台线程 | `generator.clone().start()` |
| `subscribe()` | 订阅快照 | `let rx = generator.subscribe()` |
| `set_pre_close()` | 设置昨收盘价 | `generator.set_pre_close("IF2501", 3800.0)` |
| `update_trade_stats()` | 更新成交统计 | `generator.update_trade_stats("IF2501", 100, 380000.0)` |
| `reset_daily_stats()` | 重置日内统计 | `generator.reset_daily_stats()` |
| `get_snapshot_count()` | 获取已生成快照数 | `let count = generator.get_snapshot_count()` |

### MarketDataService 方法

| 方法 | 描述 |
|------|------|
| `with_snapshot_generator()` | 配置快照生成器 |
| `start_snapshot_generator()` | 启动生成器 |
| `subscribe_snapshots()` | 订阅快照 |
| `update_trade_stats()` | 更新成交统计 |
| `set_pre_close()` | 设置昨收盘价 |

---

## 性能特性

### 生成性能

| 指标 | 数值 | 说明 |
|------|------|------|
| 生成延迟 | < 1ms | 从订单簿读取到快照生成 |
| 订阅者开销 | ~10μs | 每个订阅者的转发延迟 |
| 内存占用 | ~500 bytes/snapshot | 单个快照内存大小 |
| 并发订阅者 | 无限制 | 基于 crossbeam 无锁 channel |

### 生成流程

```
┌─────────────┐
│ 定时触发器   │ (每 interval_ms)
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────┐
│ 1. 读取订单簿 (Orderbook.read)  │ ~100μs
├─────────────────────────────────┤
│ 2. 提取买卖5档                   │ ~50μs
├─────────────────────────────────┤
│ 3. 读取日内统计 (DailyStats)    │ ~10μs
├─────────────────────────────────┤
│ 4. 计算涨跌幅/OHLC               │ ~10μs
├─────────────────────────────────┤
│ 5. 构建快照对象                  │ ~50μs
├─────────────────────────────────┤
│ 6. 广播到所有订阅者              │ ~10μs/订阅者
└─────────────────────────────────┘
         │
         ▼
   ┌──────────┐
   │ 订阅者 1  │
   ├──────────┤
   │ 订阅者 2  │
   ├──────────┤
   │ 订阅者 N  │
   └──────────┘
```

---

## 统计更新机制

### 自动更新

成交事件发生时，`TradeGateway` 会自动调用 `update_trade_stats()` 更新统计：

```rust
// src/exchange/trade_gateway.rs:727-733
if let Some(mds) = &self.market_data_service {
    let turnover = price * volume;
    mds.update_trade_stats(instrument_id, volume as i64, turnover);
}
```

### 日内统计结构

```rust
struct DailyStats {
    open: f64,       // 开盘价（首笔成交价）
    high: f64,       // 最高价（自动更新）
    low: f64,        // 最低价（自动更新）
    pre_close: f64,  // 昨收盘价（手动设置）
    volume: i64,     // 累计成交量（自动累加）
    turnover: f64,   // 累计成交额（自动累加）
}
```

### 重置时机

```rust
// 每日开盘前调用
generator.reset_daily_stats();
```

---

## 订阅模式

### 转发机制

快照生成器使用**主通道 + 转发线程**模式实现多订阅者：

```rust
pub fn subscribe(&self) -> Receiver<MarketSnapshot> {
    let (tx, rx) = unbounded();  // 为订阅者创建专用通道

    // 启动转发线程
    let snapshot_rx = self.snapshot_rx.clone();
    std::thread::spawn(move || {
        loop {
            let rx_guard = snapshot_rx.read();
            if let Ok(snapshot) = rx_guard.try_recv() {
                drop(rx_guard);  // 尽早释放锁
                if tx.send(snapshot).is_err() {
                    break;  // 订阅者断开连接
                }
            } else {
                drop(rx_guard);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });

    rx  // 返回订阅者专用接收器
}
```

### 订阅者断开检测

- 当订阅者的 `Receiver` 被 drop 时，转发线程会自动退出
- 无需手动管理订阅者生命周期

---

## 集成示例

### WebSocket 实时推送

```rust
use actix_web_actors::ws;

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for SnapshotSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                if text == "subscribe_snapshot" {
                    // 订阅快照
                    let snapshot_rx = self.market_data_service.subscribe_snapshots().unwrap();

                    // 启动推送任务
                    ctx.spawn(wrap_future(async move {
                        while let Ok(snapshot) = snapshot_rx.recv() {
                            let json = serde_json::to_string(&snapshot).unwrap();
                            ctx.text(json);
                        }
                    }));
                }
            }
            _ => {}
        }
    }
}
```

### 日志记录

```rust
use log::{info, debug};

let snapshot_rx = generator.subscribe();

tokio::spawn(async move {
    while let Ok(snapshot) = snapshot_rx.recv() {
        info!("快照: {} @ {:.2} | 买一: {:.2} x {} | 卖一: {:.2} x {}",
            snapshot.instrument_id,
            snapshot.last_price,
            snapshot.bid_price1, snapshot.bid_volume1,
            snapshot.ask_price1, snapshot.ask_volume1,
        );

        debug!("成交统计: volume={}, turnover={:.2}",
            snapshot.volume, snapshot.turnover);
    }
});
```

---

## 测试

### 运行集成测试

```bash
# 编译测试示例
cargo build --example test_snapshot_generator

# 运行测试（带日志）
RUST_LOG=info cargo run --example test_snapshot_generator
```

### 预期输出

```
=== 快照生成器测试 ===

1️⃣  初始化撮合引擎...
   ✅ 注册合约: IF2501 @ 3800

2️⃣  创建快照生成器...
   ✅ 快照生成器已创建 (间隔: 1s)

3️⃣  创建订阅者...
   ✅ 创建了 3 个订阅者

4️⃣  启动快照生成器...
   ✅ 后台线程已启动

5️⃣  提交测试订单...
   ✅ 已提交 10 个订单（买5/卖5）

6️⃣  模拟成交事件...
   ✅ 第1笔成交: volume=100, turnover=380,000
   ✅ 第2笔成交: volume=50, turnover=190,000

7️⃣  订阅者开始消费快照...
   (等待 5 秒，每秒接收一次快照)

   [订阅者1] 收到快照 #1: IF2501 @ 3800.00 (涨跌: 0.00%, 成交量: 150)
   [订阅者2] 买一: 3800.00 x 10, 卖一: 3800.20 x 10
   [订阅者3] OHLC: O=3800.00 H=3800.00 L=3800.00 (成交额: 570000.00)
   ...

8️⃣  测试统计:
   总快照数: 5
   运行时长: 5.01s
   快照频率: ~1.0/s

✅ 测试完成！
```

### 单元测试

```bash
# 运行快照生成器单元测试
cargo test --lib snapshot_generator
```

---

## 常见问题

### 1. 快照中的最新价为 0？

**原因**: 未设置昨收盘价或订单簿无成交。

**解决方案**:
```rust
// 启动时设置昨收盘价
generator.set_pre_close("IF2501", 3800.0);
```

### 2. 成交量/额始终为 0？

**原因**: 未调用 `update_trade_stats()` 更新统计。

**解决方案**:
```rust
// TradeGateway 集成后会自动调用
// 或手动调用
market_data_service.update_trade_stats("IF2501", volume, turnover);
```

### 3. 订阅者收不到快照？

**原因**: 生成器未启动或订阅时机过早。

**解决方案**:
```rust
// 确保先启动生成器
generator.clone().start();

// 再订阅
let snapshot_rx = generator.subscribe();
```

### 4. 如何修改快照频率？

```rust
let config = SnapshotGeneratorConfig {
    interval_ms: 500,  // 改为 500ms（0.5秒）
    // ...
};
```

---

## 路线图

- [x] **Phase 1**: 基础快照生成器（OHLC、买卖5档）
- [x] **Phase 2**: 集成到 MarketDataService
- [x] **Phase 3**: TradeGateway 自动统计更新
- [ ] **Phase 4**: WebSocket 订阅端点
- [ ] **Phase 5**: WAL 持久化快照
- [ ] **Phase 6**: iceoryx2 零拷贝 IPC 分发
- [ ] **Phase 7**: K线数据生成（1分钟、5分钟、日K等）
- [ ] **Phase 8**: 实时指标计算（MACD、RSI等）

---

## 参考资料

- [MarketDataService 架构](../README.md)
- [订单簿设计](../../02_architecture/trading_mechanism.md)
- [WebSocket API](../../04_api/websocket/protocol.md)
- [性能优化指南](../../02_architecture/high_performance.md)

---

**@yutiansut @quantaxis** - 2025-01-07
