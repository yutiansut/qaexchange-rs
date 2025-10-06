# 行情推送系统完善方案

## 当前架构问题总结

### 1. 行情数据未持久化
- WAL 不存储 Tick 和 OrderBook 数据
- 系统崩溃后无法恢复行情快照
- 无法回放历史行情

### 2. 无分级缓存
- 所有行情查询都直接访问 Orderbook (读锁)
- 高并发场景下性能瓶颈
- 无 L1/L2/L3 缓存层

### 3. 行情分发性能待优化
- WebSocket 每 10ms 轮询 (可能丢失高频行情)
- crossbeam::channel 无背压控制
- iceoryx2 未启用 (零拷贝优势未发挥)

---

## 完善方案

### 方案 1: 行情数据持久化 (扩展 WAL)

#### 1.1 新增 WAL 记录类型

```rust
// src/storage/wal/record.rs

#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub enum WalRecord {
    // 现有类型...
    AccountOpen { ... },
    OrderInsert { ... },
    TradeExecuted { ... },
    AccountUpdate { ... },
    Checkpoint { ... },

    // 新增行情类型
    /// Tick 行情
    TickData {
        instrument_id: [u8; 16],
        last_price: f64,
        bid_price: f64,
        ask_price: f64,
        volume: i64,
        timestamp: i64,
    },

    /// 订单簿快照 (Level2, 10档)
    OrderBookSnapshot {
        instrument_id: [u8; 16],
        bids: [(f64, i64); 10],  // 固定数组避免动态分配
        asks: [(f64, i64); 10],
        timestamp: i64,
    },

    /// 订单簿增量更新 (Level1)
    OrderBookDelta {
        instrument_id: [u8; 16],
        side: u8,  // 0=bid, 1=ask
        price: f64,
        volume: i64,  // 0 表示删除
        timestamp: i64,
    },
}
```

#### 1.2 行情写入策略

**Tick 数据**: 每笔成交立即写入
- 触发点: `OrderRouter::handle_success_result()` 成交后
- 频率: 高频 (可能 1000+ TPS)

**订单簿快照**: 定期写入 + 变化阈值触发
- 定期: 每 1 秒写入完整快照 (可配置)
- 阈值: 订单簿变化超过 5% 时立即快照

**订单簿增量**: 每次 Level1 变化写入
- 触发点: 订单簿顶部档位变化时

#### 1.3 实现代码框架

```rust
// src/exchange/order_router.rs

impl OrderRouter {
    fn handle_success_result(&self, ...) -> Result<()> {
        // 现有逻辑: 更新订单状态、记录成交

        // 新增: 写入 Tick 到 WAL
        if let Some(ref storage) = self.storage {
            let tick_record = WalRecord::TickData {
                instrument_id: to_fixed_array(&instrument_id),
                last_price: price,
                bid_price: self.get_best_bid(instrument_id)?,
                ask_price: self.get_best_ask(instrument_id)?,
                volume: filled_volume as i64,
                timestamp: chrono::Utc::now().timestamp_nanos(),
            };

            storage.append(WalEntry::new(seq, tick_record))?;
        }

        // 广播行情
        if let Some(ref broadcaster) = self.market_broadcaster {
            broadcaster.broadcast_tick(...);
        }

        Ok(())
    }
}
```

---

### 方案 2: 分级行情缓存 (L1/L2/L3)

#### 2.1 三级缓存架构

```
L1 Cache (内存 - Arc<DashMap>)
    ↓ Miss
L2 Cache (MemTable - SkipMap)
    ↓ Miss
L3 Storage (SSTable - mmap)
    ↓ Miss
Orderbook (实时计算)
```

#### 2.2 缓存实现

```rust
// src/market/cache.rs (新文件)

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// L1 行情缓存 (热数据)
pub struct MarketDataCache {
    /// Tick 缓存 (instrument_id -> TickData)
    tick_cache: Arc<DashMap<String, CachedTick>>,

    /// 订单簿缓存 (instrument_id -> OrderBookSnapshot)
    orderbook_cache: Arc<DashMap<String, CachedOrderBook>>,

    /// 缓存 TTL
    ttl: Duration,
}

#[derive(Clone)]
struct CachedTick {
    data: TickData,
    cached_at: Instant,
}

impl MarketDataCache {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            tick_cache: Arc::new(DashMap::new()),
            orderbook_cache: Arc::new(DashMap::new()),
            ttl: Duration::from_millis(ttl_ms),
        }
    }

    /// 获取 Tick (带缓存)
    pub fn get_tick(&self, instrument_id: &str) -> Option<TickData> {
        if let Some(cached) = self.tick_cache.get(instrument_id) {
            if cached.cached_at.elapsed() < self.ttl {
                return Some(cached.data.clone());
            }
            // 过期，删除
            drop(cached);
            self.tick_cache.remove(instrument_id);
        }
        None
    }

    /// 更新缓存 (在成交时调用)
    pub fn update_tick(&self, instrument_id: String, tick: TickData) {
        self.tick_cache.insert(instrument_id, CachedTick {
            data: tick,
            cached_at: Instant::now(),
        });
    }

    /// 获取订单簿 (带缓存)
    pub fn get_orderbook(&self, instrument_id: &str) -> Option<OrderBookSnapshot> {
        if let Some(cached) = self.orderbook_cache.get(instrument_id) {
            if cached.cached_at.elapsed() < self.ttl {
                return Some(cached.data.clone());
            }
            drop(cached);
            self.orderbook_cache.remove(instrument_id);
        }
        None
    }
}
```

#### 2.3 集成到 MarketDataService

```rust
// src/market/mod.rs

pub struct MarketDataService {
    matching_engine: Arc<ExchangeMatchingEngine>,
    cache: Arc<MarketDataCache>,  // 新增缓存层
}

impl MarketDataService {
    pub fn get_tick_data(&self, instrument_id: &str) -> Result<TickData> {
        // L1 缓存查询
        if let Some(tick) = self.cache.get_tick(instrument_id) {
            return Ok(tick);
        }

        // L2/L3 缓存查询 (从 MemTable/SSTable 读取)
        // TODO: 实现 L2/L3 查询

        // 缓存未命中，从 Orderbook 实时计算
        let engine = &self.matching_engine;
        let orderbook = engine.get_orderbook(instrument_id)
            .ok_or_else(|| ExchangeError::MatchingError(...))?;

        let ob = orderbook.read();
        let tick = TickData {
            instrument_id: instrument_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            last_price: ob.lastprice,
            bid_price: ob.bid_queue.get_sorted_orders()
                .and_then(|orders| orders.first().map(|o| o.price)),
            ask_price: ob.ask_queue.get_sorted_orders()
                .and_then(|orders| orders.first().map(|o| o.price)),
            volume: 0,  // TODO: 从成交记录获取
        };

        // 更新 L1 缓存
        self.cache.update_tick(instrument_id.to_string(), tick.clone());

        Ok(tick)
    }
}
```

---

### 方案 3: 优化行情分发性能

#### 3.1 启用 iceoryx2 零拷贝分发

```toml
# Cargo.toml
[features]
default = []
iceoryx2 = ["dep:iceoryx2"]

[dependencies]
iceoryx2 = { version = "0.4", optional = true }
```

```bash
# 编译时启用 iceoryx2
cargo build --release --features iceoryx2
```

#### 3.2 混合分发策略

```rust
// src/market/hybrid_broadcaster.rs (新文件)

pub struct HybridMarketBroadcaster {
    /// 内部订阅 (同进程): crossbeam::channel
    internal_broadcaster: Arc<MarketDataBroadcaster>,

    /// 外部订阅 (跨进程): iceoryx2 (可选)
    #[cfg(feature = "iceoryx2")]
    external_publisher: Arc<IceoryxPublisher>,
}

impl HybridMarketBroadcaster {
    pub fn broadcast_tick(&self, tick: TickData) {
        // 内部分发 (WebSocket 等)
        self.internal_broadcaster.broadcast_tick(...);

        // 外部分发 (策略引擎、风控服务等)
        #[cfg(feature = "iceoryx2")]
        {
            if let Err(e) = self.external_publisher.publish(&tick) {
                log::warn!("iceoryx2 publish failed: {}", e);
            }
        }
    }
}
```

#### 3.3 WebSocket 背压控制

```rust
// src/service/websocket/session.rs

fn start_market_data_listener(&self, ctx: &mut ws::WebsocketContext<Self>) {
    if let Some(ref receiver) = self.market_data_receiver {
        let receiver = receiver.clone();
        let mut dropped_count = 0;

        ctx.run_interval(Duration::from_millis(10), move |_act, ctx| {
            let mut events = Vec::new();

            // 批量接收，最多 100 条
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
                if events.len() >= 100 {
                    // 检查是否还有更多事件待处理
                    if receiver.len() > 100 {
                        dropped_count += receiver.len() - 100;
                        log::warn!("WebSocket backpressure: dropped {} events", dropped_count);
                    }
                    break;
                }
            }

            // 发送事件 (批量合并)
            if !events.is_empty() {
                let batch_json = serde_json::to_string(&events).unwrap_or_default();
                ctx.text(batch_json);
            }
        });
    }
}
```

---

### 方案 4: 行情恢复机制

#### 4.1 快照恢复流程

```rust
// src/market/recovery.rs (新文件)

pub struct MarketDataRecovery {
    storage: Arc<HybridStorage>,
}

impl MarketDataRecovery {
    /// 从 WAL 恢复行情快照
    pub async fn recover_market_data(&self, instrument_id: &str) -> Result<RecoveredMarketData> {
        let mut ticks = Vec::new();
        let mut latest_orderbook = None;

        // 扫描 WAL，提取行情记录
        let entries = self.storage.scan_wal()?;

        for entry in entries {
            match entry.record {
                WalRecord::TickData { instrument_id: inst, .. }
                    if inst == instrument_id => {
                    ticks.push(/* 解析 Tick */);
                }
                WalRecord::OrderBookSnapshot { instrument_id: inst, .. }
                    if inst == instrument_id => {
                    latest_orderbook = Some(/* 解析快照 */);
                }
                _ => {}
            }
        }

        Ok(RecoveredMarketData {
            ticks,
            orderbook_snapshot: latest_orderbook,
        })
    }
}
```

#### 4.2 崩溃恢复集成

```rust
// src/main.rs

async fn main() -> Result<()> {
    // 初始化存储
    let storage = HybridStorage::new(...)?;

    // 行情恢复
    let recovery = MarketDataRecovery::new(storage.clone());
    for instrument_id in instruments {
        match recovery.recover_market_data(&instrument_id).await {
            Ok(data) => {
                log::info!("Recovered {} ticks for {}", data.ticks.len(), instrument_id);
                // 恢复到缓存
                cache.restore_from_recovery(data);
            }
            Err(e) => {
                log::error!("Failed to recover market data for {}: {}", instrument_id, e);
            }
        }
    }

    // 启动服务...
}
```

---

## 性能优化目标

| 指标 | 当前 | 优化后 | 方案 |
|------|------|--------|------|
| Tick 查询延迟 | ~100μs (Orderbook 读锁) | **< 10μs** | L1 缓存 |
| 订单簿查询延迟 | ~200μs (聚合计算) | **< 50μs** | L1 缓存 + 快照 |
| WebSocket 推送延迟 | 10ms (轮询间隔) | **< 1ms** | 批量发送 + 背压控制 |
| 跨进程分发延迟 | N/A | **< 1μs** | iceoryx2 零拷贝 |
| 行情恢复时间 | N/A (无持久化) | **< 5s** | WAL 快照恢复 |

---

## 实施优先级

### P0 (立即实施)
1. ✅ **修复 lastprice 初始化 bug** (已完成)
2. ✅ **实现 get_recent_trades()** (已完成)
3. 🔧 **新增 WAL 行情记录类型** (TickData, OrderBookSnapshot)
4. 🔧 **实现 L1 缓存 (DashMap)**

### P1 (本周完成)
5. 📊 **集成 WAL 行情写入到 OrderRouter**
6. 🚀 **优化 WebSocket 批量推送和背压控制**
7. 💾 **实现行情快照恢复机制**

### P2 (下周完成)
8. 🔄 **实现 L2/L3 缓存 (MemTable/SSTable)**
9. 🌐 **启用 iceoryx2 跨进程分发** (可选)
10. 📈 **性能测试和调优**

---

## 实施检查清单

- [ ] 新增 `WalRecord::TickData` 和 `WalRecord::OrderBookSnapshot`
- [ ] 实现 `MarketDataCache` (L1 缓存)
- [ ] 修改 `OrderRouter` 在成交时写入 Tick 到 WAL
- [ ] 修改 `MarketDataService` 集成缓存查询
- [ ] 实现 `MarketDataRecovery` 行情恢复
- [ ] 优化 WebSocket 批量推送逻辑
- [ ] 编写性能测试用例
- [ ] 文档更新 (架构图、API 说明)

---

## 参考资料

- CLAUDE.md: qaexchange-rs 架构说明
- qars 文档: Orderbook 和 broadcast_hub 实现
- iceoryx2 文档: https://iceoryx.io/v2.0.0/
- WAL 设计: `src/storage/wal/record.rs`
