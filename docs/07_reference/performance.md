# 性能优化指南

**版本**: v0.1.0
**更新日期**: 2025-10-03
**开发团队**: @yutiansut

---

## 📋 目录

1. [性能目标](#性能目标)
2. [性能分析](#性能分析)
3. [编译优化](#编译优化)
4. [并发优化](#并发优化)
5. [内存优化](#内存优化)
6. [网络优化](#网络优化)
7. [数据库优化](#数据库优化)
8. [监控与调优](#监控与调优)

---

## 性能目标

### 目标指标

| 指标 | 目标值 | 当前状态 |
|------|--------|---------|
| **订单吞吐量** | > 100K orders/sec | ✅ 架构支持 |
| **撮合延迟 (P50)** | < 50μs | ✅ 基于 qars |
| **撮合延迟 (P99)** | < 100μs | ✅ 基于 qars |
| **HTTP API QPS** | > 10K req/s | ✅ 架构支持 |
| **WebSocket 并发** | > 10K connections | ✅ 架构支持 |
| **日终结算速度** | > 1000 accounts/sec | 🔄 待测试 |
| **内存占用** | < 2GB (10K accounts) | 🔄 待测试 |
| **CPU 使用率** | < 50% (常态) | 🔄 待测试 |

### 延迟分解

**完整订单流程延迟**:
```
客户端 → HTTP Server → OrderRouter → PreTradeCheck → MatchingEngine → TradeGateway → WebSocket → 客户端
  |          |              |              |               |                |            |
 RTT       < 1ms         < 10μs        < 10μs          < 50μs           < 1ms        < 5ms      RTT

总延迟 (P99): < 100ms (包含网络)
核心处理延迟: < 100μs (服务器端)
```

---

## 性能分析

### 1. CPU 性能分析

**使用 perf**:
```bash
# 安装 perf
sudo apt install linux-tools-common linux-tools-generic

# 录制性能数据
sudo perf record -F 99 -g ./target/release/qaexchange-rs

# 查看报告
sudo perf report

# 生成火焰图
sudo perf script | stackcollapse-perf.pl | flamegraph.pl > flame.svg
```

**使用 cargo-flamegraph**:
```bash
# 安装
cargo install flamegraph

# 生成火焰图
cargo flamegraph --bin qaexchange-rs

# 查看
open flamegraph.svg
```

### 2. 内存分析

**使用 Valgrind**:
```bash
# 内存泄漏检查
valgrind --leak-check=full ./target/debug/qaexchange-rs

# 缓存分析
valgrind --tool=cachegrind ./target/release/qaexchange-rs

# 查看缓存报告
cg_annotate cachegrind.out.<pid>
```

**使用 heaptrack**:
```bash
# 安装
sudo apt install heaptrack

# 运行
heaptrack ./target/release/qaexchange-rs

# 查看
heaptrack_gui heaptrack.qaexchange-rs.<pid>.gz
```

### 3. 延迟分析

**使用 tracing**:
```rust
use tracing::{info, instrument};

#[instrument]
pub fn submit_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
    let start = std::time::Instant::now();

    // 处理逻辑...

    let duration = start.elapsed();
    info!("submit_order took {:?}", duration);

    response
}
```

---

## 编译优化

### 1. Release 构建优化

**Cargo.toml**:
```toml
[profile.release]
opt-level = 3              # 最高优化级别
lto = "fat"                # 链接时优化
codegen-units = 1          # 单编译单元 (更好的优化，但编译慢)
panic = "abort"            # Panic 时直接退出 (减少二进制大小)
strip = true               # 移除符号 (减少二进制大小)

[profile.release.build-override]
opt-level = 3
```

### 2. CPU 特定优化

```bash
# 编译时启用 CPU 特定指令 (AVX, SSE 等)
RUSTFLAGS="-C target-cpu=native" cargo build --release

# 或在 .cargo/config.toml 中配置
[build]
rustflags = ["-C", "target-cpu=native"]
```

### 3. PGO (Profile-Guided Optimization)

```bash
# 1. 构建带 profile 的版本
RUSTFLAGS="-Cprofile-generate=/home/quantaxis/qaexchange-rs/output//pgo-data" cargo build --release

# 2. 运行负载测试，生成 profile 数据
./target/release/qaexchange-rs

# 3. 使用 profile 重新编译
llvm-profdata merge -o /home/quantaxis/qaexchange-rs/output//pgo-data/merged.profdata /home/quantaxis/qaexchange-rs/output//pgo-data/*.profraw
RUSTFLAGS="-Cprofile-use=/home/quantaxis/qaexchange-rs/output//pgo-data/merged.profdata" cargo build --release
```

### 4. 并行编译

```bash
# 使用多核编译
cargo build --release -j 8

# 或在配置文件中设置
# .cargo/config.toml
[build]
jobs = 8
```

---

## 并发优化

### 1. 无锁数据结构

**使用 DashMap**:
```rust
use dashmap::DashMap;

// ✅ 无锁并发 HashMap
pub struct OrderRouter {
    orders: Arc<DashMap<String, OrderInfo>>,  // 并发安全
}

// 并发读写无需手动加锁
self.orders.insert(order_id.clone(), order_info);
let order = self.orders.get(&order_id);
```

**vs RwLock<HashMap>**:
```rust
// ❌ 需要手动加锁
pub struct OrderRouter {
    orders: Arc<RwLock<HashMap<String, OrderInfo>>>,
}

// 性能较差，需要锁竞争
let mut orders = self.orders.write();
orders.insert(order_id.clone(), order_info);
```

**性能对比**:
| 场景 | DashMap | RwLock<HashMap> | 性能提升 |
|------|---------|----------------|---------|
| 并发读 | 100% | 95% | +5% |
| 并发写 | 100% | 60% | **+67%** |
| 读写混合 | 100% | 70% | **+43%** |

### 2. 原子操作

```rust
use std::sync::atomic::{AtomicU64, Ordering};

// ✅ 无锁计数器
pub struct OrderRouter {
    order_seq: AtomicU64,
}

impl OrderRouter {
    fn generate_order_id(&self) -> String {
        let seq = self.order_seq.fetch_add(1, Ordering::SeqCst);
        format!("O{}{:016}", timestamp, seq)
    }
}
```

### 3. Channel 优化

**使用 crossbeam unbounded channel**:
```rust
use crossbeam::channel;

// ✅ 高性能无界通道
let (tx, rx) = channel::unbounded();

// 非阻塞发送
tx.send(notification).unwrap();

// 非阻塞接收
while let Ok(msg) = rx.try_recv() {
    process(msg);
}
```

**vs std::sync::mpsc**:
| 特性 | crossbeam | std::mpsc | 优势 |
|------|-----------|-----------|------|
| 性能 | 100% | 80% | **+25%** |
| 多生产者 | ✅ | ✅ | - |
| 多消费者 | ✅ | ❌ | **更灵活** |
| try_recv | ✅ | ✅ | - |

### 4. 线程池

**使用 Rayon 并行处理**:
```rust
use rayon::prelude::*;

// ✅ 并行结算多个账户
pub fn daily_settlement(&self, accounts: &[String]) -> Result<SettlementResult> {
    let results: Vec<_> = accounts
        .par_iter()  // 并行迭代
        .map(|user_id| self.settle_account(user_id))
        .collect();

    // 汇总结果
    aggregate(results)
}
```

---

## 内存优化

### 1. 避免频繁分配

**对象池**:
```rust
use object_pool::Pool;

lazy_static! {
    static ref ORDER_POOL: Pool<Order> = Pool::new(1000, || Order::default());
}

// ✅ 复用对象
let mut order = ORDER_POOL.try_pull().unwrap();
order.set_data(...);
// 使用完后自动归还池中
```

**预分配容量**:
```rust
// ✅ 预分配避免多次重新分配
let mut orders = Vec::with_capacity(10000);
for _ in 0..10000 {
    orders.push(order);
}

// ❌ 频繁重新分配
let mut orders = Vec::new();  // 初始容量 0
for _ in 0..10000 {
    orders.push(order);  // 多次 realloc
}
```

### 2. 使用 SmallVec

```rust
use smallvec::{SmallVec, smallvec};

// ✅ 小数组在栈上，大数组在堆上
type Orders = SmallVec<[Order; 16]>;  // <= 16 在栈上

let orders: Orders = smallvec![];
```

### 3. 零拷贝

**使用 Arc 共享所有权**:
```rust
// ✅ 共享所有权，无拷贝
pub struct TradeGateway {
    account_mgr: Arc<AccountManager>,  // 共享引用
}

// 克隆 Arc 只增加引用计数，不拷贝数据
let account_mgr_clone = self.account_mgr.clone();
```

**使用 Cow (Clone-on-Write)**:
```rust
use std::borrow::Cow;

fn process(data: Cow<str>) {
    // 只读时不拷贝
    println!("{}", data);

    // 需要修改时才拷贝
    let owned = data.into_owned();
}
```

### 4. 内存布局优化

```rust
// ❌ 内存对齐浪费
#[derive(Debug)]
struct Order {
    id: String,       // 24 bytes
    price: f64,       // 8 bytes
    flag: bool,       // 1 byte + 7 bytes padding
    volume: f64,      // 8 bytes
}
// 总大小: 48 bytes

// ✅ 优化布局
#[derive(Debug)]
struct Order {
    id: String,       // 24 bytes
    price: f64,       // 8 bytes
    volume: f64,      // 8 bytes
    flag: bool,       // 1 byte + 0 bytes padding
}
// 总大小: 41 bytes (节省 15%)
```

---

## 网络优化

### 1. TCP 参数调优

**系统配置**:
```bash
# /etc/sysctl.conf

# 增加 TCP 缓冲区
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216

# 增加连接队列
net.core.somaxconn = 4096
net.ipv4.tcp_max_syn_backlog = 8192

# TIME_WAIT 优化
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 30

# 生效
sudo sysctl -p
```

### 2. Actix-web 配置

```rust
use actix_web::{HttpServer, App};

HttpServer::new(|| {
    App::new()
        .app_data(web::Data::new(app_state))
})
.workers(num_cpus::get())        // 使用所有 CPU 核心
.backlog(8192)                   // 增加 backlog
.max_connections(10000)          // 最大连接数
.keep_alive(Duration::from_secs(75))  // Keep-Alive 时间
.client_request_timeout(Duration::from_secs(30))  // 请求超时
.bind("0.0.0.0:8080")?
.run()
.await
```

### 3. WebSocket 优化

```rust
impl WsSession {
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            // 检查心跳超时
            if Instant::now().duration_since(act.heartbeat) > Duration::from_secs(10) {
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });

        // 降低轮询频率 (10ms → 50ms)
        ctx.run_interval(Duration::from_millis(50), |act, ctx| {
            // 批量接收通知
            let mut batch = Vec::with_capacity(10);
            while let Ok(notification) = act.notification_receiver.try_recv() {
                batch.push(notification);
                if batch.len() >= 10 {
                    break;
                }
            }

            // 批量发送
            for notification in batch {
                ctx.text(serde_json::to_string(&notification).unwrap());
            }
        });
    }
}
```

### 4. 批量处理

```rust
// ✅ 批量处理订单
pub fn submit_orders_batch(&self, requests: Vec<SubmitOrderRequest>) -> Vec<SubmitOrderResponse> {
    requests
        .into_iter()
        .map(|req| self.submit_order(req))
        .collect()
}
```

---

## 数据库优化

### 1. MongoDB 优化

**索引优化**:
```javascript
// 为常用查询创建索引
db.accounts.createIndex({ "user_id": 1 }, { unique: true })
db.orders.createIndex({ "user_id": 1, "created_at": -1 })
db.trades.createIndex({ "instrument_id": 1, "timestamp": -1 })

// 复合索引
db.orders.createIndex({ "user_id": 1, "status": 1, "created_at": -1 })
```

**批量写入**:
```rust
use mongodb::options::InsertManyOptions;

// ✅ 批量插入
let orders: Vec<Document> = /* ... */;
collection.insert_many(orders, None).await?;

// ❌ 逐条插入
for order in orders {
    collection.insert_one(order, None).await?;  // 慢
}
```

**连接池**:
```rust
use mongodb::{Client, options::ClientOptions};

let mut options = ClientOptions::parse("mongodb://localhost:27017").await?;
options.max_pool_size = Some(100);  // 连接池大小
options.min_pool_size = Some(10);

let client = Client::with_options(options)?;
```

### 2. ClickHouse 优化

**批量写入**:
```rust
// ✅ 批量插入 (10K 条/批)
let batch: Vec<Trade> = /* ... */;
clickhouse_client.insert_batch("trades", batch).await?;
```

**分区表**:
```sql
-- 按日期分区
CREATE TABLE trades (
    trade_id String,
    timestamp DateTime,
    ...
) ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (timestamp, trade_id);
```

### 3. Redis 缓存

**缓存热点数据**:
```rust
use redis::AsyncCommands;

// 缓存账户信息 (5 分钟)
async fn get_account_cached(&self, user_id: &str) -> Result<Account> {
    let key = format!("account:{}", user_id);

    // 先查缓存
    if let Ok(cached) = self.redis.get::<_, String>(&key).await {
        return Ok(serde_json::from_str(&cached)?);
    }

    // 缓存未命中，查数据库
    let account = self.db.get_account(user_id).await?;

    // 写入缓存
    let _: () = self.redis.set_ex(&key, serde_json::to_string(&account)?, 300).await?;

    Ok(account)
}
```

---

## 监控与调优

### 1. 性能指标收集

**Prometheus 集成**:
```rust
use prometheus::{Counter, Histogram, Registry};

lazy_static! {
    static ref ORDER_COUNTER: Counter = Counter::new("orders_total", "Total orders").unwrap();
    static ref ORDER_LATENCY: Histogram = Histogram::new("order_latency_seconds", "Order latency").unwrap();
}

pub fn submit_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
    let start = std::time::Instant::now();

    // 处理订单...

    ORDER_COUNTER.inc();
    ORDER_LATENCY.observe(start.elapsed().as_secs_f64());

    response
}

// 暴露 /metrics 端点
#[get("/metrics")]
async fn metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode_to_string(&metric_families).unwrap()
}
```

### 2. 系统监控

**监控脚本**:
```bash
#!/bin/bash
# monitor.sh

while true; do
    echo "=== $(date) ==="

    # CPU 使用率
    echo "CPU:"
    mpstat 1 1 | tail -1

    # 内存使用
    echo "Memory:"
    free -m | grep Mem

    # 网络连接数
    echo "Connections:"
    ss -s | grep TCP

    # 进程状态
    echo "Process:"
    ps aux | grep qaexchange-rs | grep -v grep

    sleep 10
done
```

### 3. 性能基准

**基准测试脚本**:
```bash
#!/bin/bash
# benchmark.sh

# HTTP API 压测
echo "HTTP API Benchmark:"
ab -n 100000 -c 100 http://localhost:8080/health

# WebSocket 压测
echo "WebSocket Benchmark:"
wscat -c ws://localhost:8081/ws?user_id=test_user
```

### 4. 调优检查清单

**编译优化**:
- [ ] 使用 `--release` 构建
- [ ] 启用 LTO
- [ ] 使用 `target-cpu=native`
- [ ] 考虑 PGO

**并发优化**:
- [ ] 使用无锁数据结构 (DashMap)
- [ ] 使用原子操作
- [ ] 使用 crossbeam channel
- [ ] 使用 Rayon 并行处理

**内存优化**:
- [ ] 预分配容量
- [ ] 使用对象池
- [ ] 使用 Arc 共享所有权
- [ ] 优化数据结构布局

**网络优化**:
- [ ] 调整 TCP 参数
- [ ] 配置 Actix-web workers
- [ ] 批量处理请求
- [ ] 降低心跳频率

**数据库优化**:
- [ ] 创建索引
- [ ] 批量写入
- [ ] 使用连接池
- [ ] 缓存热点数据

---

## 性能测试报告模板

```markdown
# 性能测试报告

**测试日期**: 2025-10-03
**版本**: v0.1.0
**测试环境**: 8 核 16GB

## 测试场景 1: 订单提交吞吐量

**配置**:
- 并发用户: 1000
- 测试时长: 60s
- 订单类型: 限价单

**结果**:
| 指标 | 值 |
|------|---|
| 总订单数 | 6,000,000 |
| 吞吐量 | 100,000 orders/sec |
| 平均延迟 | 8 ms |
| P95 延迟 | 15 ms |
| P99 延迟 | 25 ms |
| 成功率 | 100% |

**资源使用**:
- CPU: 45%
- 内存: 1.2GB
- 网络: 800 Mbps

## 测试场景 2: WebSocket 并发连接

**配置**:
- 并发连接数: 10,000
- 消息频率: 10 msg/sec per connection

**结果**:
| 指标 | 值 |
|------|---|
| 建立连接时间 | 5s |
| 消息延迟 (P99) | 50 ms |
| 连接成功率 | 99.8% |
| CPU 使用率 | 60% |
| 内存使用 | 2.5GB |

## 优化建议

1. 降低 WebSocket 轮询频率 (10ms → 50ms)
2. 增加服务器内存到 32GB
3. 启用 PGO 优化编译
```

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
