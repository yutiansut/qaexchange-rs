# 系统架构设计

**版本**: v0.5.0
**更新日期**: 2025-12-18 (新增可观测性层、安全层、Phase 12-13 特性)
**开发团队**: @yutiansut @quantaxis

---

## 📋 目录

1. [架构概览](#架构概览)
2. [核心设计原则](#核心设计原则)
3. [分层架构](#分层架构)
4. [可观测性层](#可观测性层) ✨ **NEW**
5. [安全层](#安全层) ✨ **NEW**
6. [管理端架构](#管理端架构)
7. [数据流设计](#数据流设计)
8. [并发模型](#并发模型)
9. [性能优化](#性能优化)
10. [扩展性设计](#扩展性设计)
11. [安全设计](#安全设计)
12. [数据协议](#数据协议)

---

## 架构概览

### 系统架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        客户端层 (Client Layer)                    │
│                                                                   │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│   │  Web 前端     │  │  移动端 App   │  │  交易终端     │          │
│   │  (React/Vue) │  │  (Flutter)   │  │  (Desktop)   │          │
│   └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
│          │                 │                 │                   │
└──────────┼─────────────────┼─────────────────┼───────────────────┘
           │                 │                 │
       HTTP REST         WebSocket         WebSocket
           │                 │                 │
┌──────────▼─────────────────▼─────────────────▼───────────────────┐
│                      服务层 (Service Layer)                        │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │  HTTP Server (Actix-web)        Port: 8080              │    │
│   │  ├── CORS Middleware                                    │    │
│   │  ├── Logger Middleware                                  │    │
│   │  ├── Compression (Gzip)                                 │    │
│   │  └── Routes:                                            │    │
│   │      ├── /api/account/* (账户管理)                       │    │
│   │      ├── /api/order/* (订单管理)                         │    │
│   │      ├── /api/position/* (持仓查询)                      │    │
│   │      ├── /api/market/* (市场数据)                        │    │
│   │      ├── /admin/instruments/* (合约管理) 🔧              │    │
│   │      ├── /admin/settlement/* (结算管理) 🔧               │    │
│   │      ├── /admin/risk/* (风控管理) 🔧                     │    │
│   │      └── /monitoring/* (系统监控) 🔧                     │    │
│   └─────────────────────────────────────────────────────────┘    │
│                                                                   │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │  WebSocket Server (Actix-web-actors)  Port: 8081        │    │
│   │  ├── Session Management (Actor Model)                   │    │
│   │  ├── Authentication & Authorization                     │    │
│   │  ├── Heartbeat (5s interval, 10s timeout)              │    │
│   │  └── Message Routing:                                   │    │
│   │      ├── Trading Messages (submit/cancel/query)         │    │
│   │      └── Subscription (trade/orderbook/account)         │    │
│   └─────────────────────────────────────────────────────────┘    │
└───────────────────────────┬───────────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────────┐
│                      业务层 (Business Layer)                       │
│                                                                   │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│   │  OrderRouter     │  │  TradeGateway    │  │  Settlement  │  │
│   │  (订单路由)       │  │  (成交网关)       │  │  (结算引擎)   │  │
│   │                  │  │                  │  │              │  │
│   │ • 订单接收        │  │ • 成交通知        │  │ • 日终结算   │  │
│   │ • 风控检查        │  │ • 账户更新        │  │ • 盯市盈亏   │  │
│   │ • 路由撮合        │  │ • 推送订阅者      │  │ • 强平处理   │  │
│   │ • 状态管理        │  │ • Pub/Sub        │  │ • 结算历史   │  │
│   └────────┬─────────┘  └────────┬─────────┘  └──────────────┘  │
│            │                     │                               │
│            │    ┌────────────────▼─────────┐                     │
│            │    │  PreTradeCheck (风控)     │                     │
│            │    │  • 资金检查               │                     │
│            │    │  • 持仓限额               │                     │
│            │    │  • 风险度检查             │                     │
│            │    │  • 自成交防范             │                     │
│            │    └──────────────────────────┘                     │
└────────────┼──────────────────────────────────────────────────────┘
             │
┌────────────▼──────────────────────────────────────────────────────┐
│                      核心层 (Core Layer)                           │
│                                                                   │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│   │ AccountManager   │  │ MatchingEngine   │  │ Instrument   │  │
│   │ (账户管理)        │  │ (撮合引擎)        │  │ Registry     │  │
│   │                  │  │                  │  │ (合约注册) 🔧│  │
│   │ • 开户/销户       │  │ • Orderbook      │  │ • 合约信息   │  │
│   │ • 入金/出金       │  │ • 价格时间优先    │  │ • 生命周期   │  │
│   │ • 账户查询        │  │ • 撮合算法        │  │ • 参数配置   │  │
│   │ • 权限管理        │  │ • 成交生成        │  │ • 状态管理   │  │
│   └──────────────────┘  └──────────────────┘  └──────────────┘  │
│                                                                   │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│   │ SettlementEngine │  │ RiskMonitor      │  │ SystemMonitor│  │
│   │ (结算引擎) 🔧    │  │ (风控监控) 🔧    │  │ (系统监控)🔧 │  │
│   │                  │  │                  │  │              │  │
│   │ • 设置结算价      │  │ • 风险账户       │  │ • CPU/内存   │  │
│   │ • 日终结算        │  │ • 保证金监控     │  │ • 存储监控   │  │
│   │ • 盈亏计算        │  │ • 强平检测       │  │ • 账户统计   │  │
│   │ • 强平处理        │  │ • 强平记录       │  │ • 性能指标   │  │
│   └──────────────────┘  └──────────────────┘  └──────────────┘  │
│                                                                   │
└───────────────────────────┬───────────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────────┐
│                      数据层 (Data Layer - 复用 qars)               │
│                                                                   │
│   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│   │  QA_Account      │  │  QA_Position     │  │  QA_Order    │  │
│   │  (账户结构)       │  │  (持仓结构)       │  │  (订单结构)   │  │
│   │                  │  │                  │  │              │  │
│   │ • QIFI 协议      │  │ • 多空持仓        │  │ • 订单状态   │  │
│   │ • 资金管理        │  │ • 今昨仓分离      │  │ • 成交记录   │  │
│   │ • 盈亏计算        │  │ • 成本计算        │  │ • 订单簿    │  │
│   └──────────────────┘  └──────────────────┘  └──────────────┘  │
│                                                                   │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │  数据持久化 (可选)                                          │  │
│   │  ├── MongoDB (账户快照、订单记录)                           │  │
│   │  ├── ClickHouse (成交记录、行情数据)                        │  │
│   │  └── Redis (缓存、会话)                                    │  │
│   └──────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

---

## 核心设计原则

### 1. 高性能 (High Performance)

**目标**:
- 订单吞吐量: > 100K orders/sec
- 撮合延迟: P99 < 100μs
- WebSocket 并发: > 10K connections

**实现**:
- **异步非阻塞**: Tokio 异步运行时
- **零拷贝通信**: crossbeam unbounded channel
- **无锁并发**: DashMap, parking_lot RwLock
- **内存池**: 预分配对象池，减少 GC 压力

### 2. 类型安全 (Type Safety)

**优势**:
- **编译时保证**: Rust 类型系统在编译时发现错误
- **无运行时异常**: 无空指针、无数据竞争
- **强类型协议**: QIFI/TIFI/MIFI 协议定义

**示例**:
```rust
// 编译时类型检查
pub enum OrderDirection {
    BUY,
    SELL,
}

pub enum OrderOffset {
    OPEN,
    CLOSE,
    CLOSETODAY,
    CLOSEYESTERDAY,
}

// 编译器确保只能使用合法值
fn submit_order(direction: OrderDirection, offset: OrderOffset) {
    // ...
}
```

### 3. 模块化 (Modularity)

**分层解耦**:
- **服务层**: 不依赖具体业务实现
- **业务层**: 可独立测试和替换
- **核心层**: 纯粹的领域逻辑
- **数据层**: 复用 qars 成熟组件

### 4. 可观测性 (Observability)

**日志分级**:
```rust
log::trace!("订单簿快照: {:?}", orderbook);
log::debug!("订单 {} 提交成功", order_id);
log::info!("账户 {} 开户成功", user_id);
log::warn!("风险度 {:.2}% 接近阈值", risk_ratio * 100.0);
log::error!("撮合引擎异常: {}", error);
```

**性能指标** (预留):
- 订单提交延迟分布
- 撮合引擎 TPS
- WebSocket 连接数
- 内存/CPU 使用率

---

## 分层架构

### 服务层 (Service Layer)

**职责**: 对外提供 HTTP 和 WebSocket 接口

**HTTP Server**:
```rust
pub struct HttpServer {
    app_state: Arc<AppState>,
    bind_address: String,
}

pub struct AppState {
    order_router: Arc<OrderRouter>,
    account_mgr: Arc<AccountManager>,
}
```

**WebSocket Server**:
```rust
pub struct WsSession {
    id: String,
    state: SessionState,  // Unauthenticated | Authenticated
    heartbeat: Instant,
    subscribed_channels: Vec<String>,
    notification_receiver: Option<Receiver<Notification>>,
}
```

### 业务层 (Business Layer)

**OrderRouter (订单路由器)**:
```rust
pub struct OrderRouter {
    account_mgr: Arc<AccountManager>,
    risk_checker: Arc<PreTradeCheck>,
    matching_engines: Arc<DashMap<String, Arc<RwLock<ExchangeMatchingEngine>>>>,
    orders: Arc<DashMap<String, OrderInfo>>,
    trade_gateway: Arc<TradeGateway>,
    order_seq: AtomicU64,
}
```

**完整订单流程**:
1. 接收订单请求
2. 风控检查 (PreTradeCheck)
3. 路由到撮合引擎
4. 处理撮合结果
5. 更新账户状态 (TradeGateway)
6. 推送通知给订阅者

**TradeGateway (成交网关)**:
```rust
pub struct TradeGateway {
    account_mgr: Arc<AccountManager>,
    user_subscribers: Arc<DashMap<String, Vec<Sender<Notification>>>>,
    global_subscribers: Arc<DashMap<String, Sender<Notification>>>,
    trade_seq: AtomicU64,
}
```

**Pub/Sub 模式**:
- 用户订阅: 接收自己的成交/订单状态/账户更新
- 全局订阅: 接收指定合约的所有成交

### 核心层 (Core Layer)

**AccountManager (账户管理器)**:
```rust
pub struct AccountManager {
    accounts: Arc<DashMap<String, Arc<RwLock<QA_Account>>>>,
}

impl AccountManager {
    pub fn open_account(&self, req: OpenAccountRequest) -> Result<String, ExchangeError>;
    pub fn get_account(&self, user_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError>;
    pub fn deposit(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError>;
    pub fn withdraw(&self, user_id: &str, amount: f64) -> Result<(), ExchangeError>;
}
```

**MatchingEngine (撮合引擎)**:
- 复用 qars `ExchangeMatchingEngine`
- 价格-时间优先撮合算法
- 支持限价单、市价单
- 集合竞价支持 (预留)

### 数据层 (Data Layer)

**复用 qars 核心数据结构**:

**QA_Account (账户)**:
```rust
pub struct QA_Account {
    pub user_id: String,
    pub accounts: Account,      // 资金账户
    pub hold: HashMap<String, QA_Position>,  // 持仓
    pub trades: Vec<QA_Trade>,  // 成交记录
    pub orders: BTreeMap<String, Order>,     // 订单
}
```

**Account (资金结构)**:
```rust
pub struct Account {
    pub balance: f64,        // 总权益
    pub available: f64,      // 可用资金
    pub margin: f64,         // 占用保证金
    pub close_profit: f64,   // 平仓盈亏
    pub risk_ratio: f64,     // 风险度
}
```

**QA_Position (持仓)**:
```rust
pub struct QA_Position {
    pub volume_long_today: f64,      // 多头今仓
    pub volume_long_his: f64,        // 多头昨仓
    pub volume_short_today: f64,     // 空头今仓
    pub volume_short_his: f64,       // 空头昨仓
    pub open_price_long: f64,        // 多头开仓均价
    pub open_price_short: f64,       // 空头开仓均价
}
```

### 存储层 (Storage Layer)

**混合存储架构 (Hybrid OLTP/OLAP)**:

```
┌──────────────────────────────────────────────────────────────┐
│                        Storage Layer                         │
│                                                              │
│   OLTP Path (Low Latency)      OLAP Path (Analytics)       │
│   ↓                             ↓                            │
│   WAL                           WAL                          │
│   ↓                             ↓                            │
│   MemTable (SkipMap)            MemTable (Arrow2 Columnar)  │
│   ↓                             ↓                            │
│   SSTable (rkyv + mmap)         SSTable (Parquet)           │
│   ↓                             ↓                            │
│   Compaction + Bloom Filter     Query Engine (Polars)       │
└──────────────────────────────────────────────────────────────┘
```

**WAL (Write-Ahead Log)** - `src/storage/wal/`:
```rust
pub struct WalManager {
    dir: PathBuf,
    current_file: File,
    sequence: AtomicU64,
}
```
- CRC32 数据完整性校验
- 批量写入优化 (> 78K entries/sec)
- 崩溃恢复机制
- P99 < 50ms 写入延迟

**MemTable** - `src/storage/memtable/`:
- **OLTP MemTable** (`oltp.rs`): SkipMap, P99 < 10μs
- **OLAP MemTable** (`olap.rs`): Arrow2 columnar format

**SSTable** - `src/storage/sstable/`:
- **OLTP SSTable** (`oltp_rkyv.rs`): rkyv 零拷贝, P99 < 50μs
- **OLAP SSTable** (`olap_parquet.rs`): Parquet 列式存储
- **Bloom Filter** (`bloom.rs`): 1% FP rate, ~100ns lookup
- **mmap Reader** (`mmap_reader.rs`): 零拷贝文件映射

**Compaction** - `src/storage/compaction/`:
- Leveled compaction 策略
- 后台压缩线程
- 自动触发和手动触发

**Checkpoint** - `src/storage/checkpoint/`:
- 快照创建
- 恢复管理
- 增量备份

**Hybrid Storage** - `src/storage/hybrid/`:
```rust
pub struct OltpHybridStorage {
    wal: Arc<WalManager>,
    memtable: Arc<RwLock<OltpMemTable>>,
    sstable_dir: PathBuf,
}
```

### 查询层 (Query Layer) ✨ Phase 8

**查询引擎架构** - `src/query/`:

```
┌──────────────────────────────────────────────────────────────┐
│                      Query Engine (Polars)                   │
│                                                              │
│   QueryRequest ─→ SSTableScanner ─→ LazyFrame ─→ DataFrame   │
│        │              │                  │            │       │
│        │              │                  │            │       │
│   SQL/Struct/    OLTP + OLAP      Predicate       Column     │
│   TimeSeries      SSTables         Pushdown       Pruning    │
└──────────────────────────────────────────────────────────────┘
```

**QueryEngine** - `src/query/engine.rs`:
```rust
pub struct QueryEngine {
    scanner: SSTableScanner,
}

impl QueryEngine {
    pub fn execute(&self, request: QueryRequest)
        -> Result<QueryResponse, String>;
}
```

**查询类型**:
1. **SQL Query**: 标准 SQL via Polars SQLContext
```rust
QueryType::Sql {
    query: "SELECT * FROM data WHERE price > 100 LIMIT 10"
}
```

2. **Structured Query**: 程序化 API
```rust
QueryType::Structured {
    select: vec!["timestamp", "price", "volume"],
    from: "data",
}
// + filters, aggregations, order_by, limit
```

3. **Time-Series Query**: 时间序列聚合
```rust
QueryType::TimeSeries {
    metrics: vec!["price", "volume"],
    dimensions: vec!["instrument_id"],
    granularity: Some(60), // 60秒粒度
}
```

**性能优化**:
- LazyFrame 延迟执行
- Predicate pushdown (谓词下推)
- Column pruning (列裁剪)
- Multi-file parallel scanning
- Parquet scan throughput: > 1GB/s

### 复制层 (Replication Layer) - Phase 6

**Master-Slave Replication** - `src/replication/`:

```rust
pub struct LogReplicator {
    role: Arc<RwLock<NodeRole>>,  // Master/Slave/Candidate
    log_buffer: Arc<RwLock<Vec<ReplicationLogEntry>>>,
}
```

**核心能力**:
- 批量日志复制 (< 10ms 延迟)
- 心跳检测 (100ms 间隔)
- 自动故障转移 (< 500ms)
- Raft-inspired 选主算法

**角色管理**:
- **Master**: 接收写入, 复制日志到 Slave
- **Slave**: 只读, 接收日志并应用
- **Candidate**: 参与选主投票

### 因子计算层 (Factor Layer) ✨ NEW

**流批一体因子引擎** - `src/factor/`:

```
┌──────────────────────────────────────────────────────────────┐
│                    因子计算系统架构                            │
│                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│   │   DSL 层    │    │  DAG 管理   │    │  状态存储   │     │
│   │ (语法解析)  │───▶│ (依赖拓扑)  │───▶│ (物化视图)  │     │
│   └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                  │                  │             │
│         ▼                  ▼                  ▼             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              流批一体执行引擎                         │   │
│   │  ┌──────────────┐        ┌──────────────┐          │   │
│   │  │  Stream Engine│        │  Batch Engine │          │   │
│   │  │  (增量算子)   │        │  (Polars SQL) │          │   │
│   │  └──────────────┘        └──────────────┘          │   │
│   └─────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

**核心组件**:
- **增量算子** (`operators/`): RollingMean, EMA, RSI, MACD 等 O(1) 算子
- **环形缓冲区** (`ring_buffer.rs`): 滑动窗口高效实现
- **Welford 算法** (`welford.rs`): 数值稳定的在线统计
- **因子 DAG** (`dag.rs`): 依赖管理与并行计算
- **物化视图** (`view.rs`): 因子状态持久化

**性能指标**:
- RollingMean.update(): ~15 ns (66M ops/s)
- RollingStd.update(): ~25 ns (Welford)
- 环形缓冲区 push: ~5 ns (200M ops/s)

### 集群管理层 (Cluster Layer) ✨ NEW

**一致性哈希分片** - `src/cluster/`:

```
┌──────────────────────────────────────────────────────────────┐
│                    集群架构                                   │
│                                                              │
│    ┌─────────────┐                                          │
│    │ ShardRouter │ ◀─── 路由缓存                             │
│    └─────────────┘                                          │
│          │                                                   │
│          ▼                                                   │
│    ┌─────────────────────────────┐                          │
│    │   ConsistentHashRing        │                          │
│    │  0 ────────────── 2^64      │                          │
│    │  │  N1  N2  N3  N1  N2  N3  │ ◀─── 虚拟节点             │
│    └─────────────────────────────┘                          │
│          │                                                   │
│          ▼                                                   │
│    ┌─────────┐  ┌─────────┐  ┌─────────┐                    │
│    │  Node1  │  │  Node2  │  │  Node3  │                    │
│    └─────────┘  └─────────┘  └─────────┘                    │
└──────────────────────────────────────────────────────────────┘
```

**核心能力**:
- **一致性哈希**: 节点变更时最小化数据迁移
- **虚拟节点**: 150 个虚拟节点/物理节点
- **带权重节点**: 高性能节点承担更多负载
- **副本分布**: 多副本高可靠性
- **分片路由**: 按账户ID/合约ID/订单ID分片

**性能指标**:
- `get_node()`: ~100 ns
- `route()` 缓存命中: ~20 ns
- `add_node()`: ~1 ms

### DSL 层 (DSL Layer) ✨ NEW

**因子定义语言** - `src/dsl/`:

```rust
// DSL 示例
ma5 = rolling_mean(close, 5)
ma20 = rolling_mean(close, 20)
ma_diff = ma5 - ma20
signal = if rsi > 70 then -1 else if rsi < 30 then 1 else 0
```

**核心组件**:
- **词法分析器** (`lexer.rs`): Token 生成
- **语法解析器** (`parser.rs`): AST 构建 (pest)
- **语法定义** (`grammar.pest`): PEG 语法规则

---

## 可观测性层 (Observability Layer) ✨ Phase 12

**分布式追踪与监控** - `src/observability/`:

```
┌──────────────────────────────────────────────────────────────┐
│                    Observability Stack                        │
│                                                              │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│   │   Tracing   │    │   Metrics   │    │   Logging   │     │
│   │             │    │             │    │             │     │
│   │ OpenTelemetry│    │ Prometheus │    │  tracing-   │     │
│   │   + OTLP    │    │  Exporter  │    │  subscriber │     │
│   └──────┬──────┘    └──────┬──────┘    └──────┬──────┘     │
│          │                  │                  │             │
│          └──────────────────┴──────────────────┘             │
│                            │                                  │
│                    ┌───────▼───────┐                         │
│                    │    Grafana    │                         │
│                    │   Dashboard   │                         │
│                    └───────────────┘                         │
└──────────────────────────────────────────────────────────────┘
```

### OpenTelemetry 追踪

**配置结构** - `src/observability/tracing.rs`:
```rust
pub struct TracingConfig {
    pub enabled: bool,
    pub service_name: String,
    pub service_version: String,
    pub environment: String,        // dev/staging/prod
    pub exporter: ExporterType,     // Otlp/Console/None
    pub endpoint: String,           // OTLP endpoint
    pub sampling_rate: f64,         // 0.0 - 1.0
    pub batch_config: BatchExportConfig,
    pub log_filter: String,
    pub console_export: bool,
}
```

**预置配置**:
- `TracingConfig::development()`: 100% 采样，控制台输出
- `TracingConfig::production(endpoint)`: 10% 采样，OTLP 导出
- `TracingConfig::test()`: 100% 采样，控制台输出

**Span 宏**:
```rust
use qaexchange::{trace_span, trace_operation};

// 自动计时操作
let result = trace_operation!("match_order", {
    matching_engine.process_order(order)?
});
// 输出: elapsed_us = xxx, "operation completed"
```

### Prometheus 指标

**核心指标**:
| 指标名 | 类型 | 说明 |
|--------|------|------|
| `qaexchange_orders_total` | Counter | 订单总数 |
| `qaexchange_trades_total` | Counter | 成交总数 |
| `qaexchange_order_latency_seconds` | Histogram | 订单延迟 |
| `qaexchange_active_connections` | Gauge | 活跃连接数 |
| `qaexchange_memtable_size_bytes` | Gauge | MemTable 大小 |

### Grafana 大盘

**预置面板** - `config/grafana/dashboards/`:
1. **交易概览**: 订单量、成交量、撮合延迟
2. **存储状态**: WAL 大小、MemTable 占用、SSTable 数量
3. **系统健康**: CPU、内存、磁盘 IO
4. **复制状态**: 主从延迟、心跳状态

**性能指标**:
- Span 创建开销: < 100ns
- 批量导出: 异步非阻塞
- 采样率: 可配置（生产环境建议 1-10%）

---

## 安全层 (Security Layer) ✨ Phase 13

**TLS/mTLS 加密通信** - `src/replication/tls.rs`:

```
┌──────────────────────────────────────────────────────────────┐
│                    TLS Security Architecture                  │
│                                                              │
│   ┌──────────────────────────────────────────────────────┐   │
│   │                 Certificate Chain                      │   │
│   │                                                        │   │
│   │    ┌────────┐                                         │   │
│   │    │ Root CA │  (10 年有效期, 离线存储)                │   │
│   │    └────┬───┘                                         │   │
│   │         │ signs                                        │   │
│   │         ▼                                              │   │
│   │    ┌────────────┐     ┌────────────┐                  │   │
│   │    │ Server Cert │     │ Client Cert │  (mTLS)         │   │
│   │    │ (1 年)      │     │ (1 年)      │                 │   │
│   │    └────────────┘     └────────────┘                  │   │
│   └──────────────────────────────────────────────────────┘   │
│                                                              │
│   加密算法:                                                   │
│   - TLS 1.3                                                  │
│   - AES-256-GCM / ChaCha20-Poly1305                         │
│   - RSA 2048 / ECDSA P-256                                  │
└──────────────────────────────────────────────────────────────┘
```

### CertificateGenerator API

```rust
use qaexchange::replication::tls::CertificateGenerator;

// 生成自签名 CA
let ca = CertificateGenerator::generate_ca_certificate(
    "QAExchange CA", 365 * 10
)?;

// 生成服务器证书
let server = CertificateGenerator::generate_server_certificate(
    &ca, "server", &["localhost", "192.168.1.100"], 365
)?;

// 生成客户端证书 (mTLS)
let client = CertificateGenerator::generate_client_certificate(
    &ca, "trader-001", 365
)?;
```

### TLS 配置构建器

```rust
use qaexchange::replication::tls::TlsConfigBuilder;

// 服务端 mTLS 配置
let server_config = TlsConfigBuilder::new()
    .with_certificate_paths(&server_paths)?
    .require_client_auth(&ca_paths)?  // mTLS
    .build_server_config()?;

// 客户端配置
let client_config = TlsConfigBuilder::new()
    .with_certificate_paths(&client_paths)?
    .with_ca_certificate(&ca_paths)?
    .build_client_config()?;
```

### SIMD 优化

**运行时检测** - `src/ipc/simd.rs`:
```rust
pub struct SimdCapabilities {
    pub avx2: bool,      // x86_64 AVX2 (256-bit)
    pub avx512: bool,    // x86_64 AVX-512 (512-bit)
    pub sse42: bool,     // x86_64 SSE4.2
    pub neon: bool,      // ARM NEON
}

// 自动选择最优实现
let caps = SimdCapabilities::detect();
```

**优化函数**:
| 函数 | Scalar | SIMD | 加速比 |
|------|--------|------|--------|
| `find_best_price(1K)` | 800ns | 150ns | 5.3x |
| `sum_volumes(1K)` | 500ns | 100ns | 5x |
| `crc32(4KB)` | 2μs | 400ns | 5x |

### Block Index

**块级索引** - `src/storage/sstable/block_index.rs`:

```
┌──────────────────────────────────────────────────────────────┐
│                    SSTable with Block Index                   │
│                                                              │
│   Block Index (Binary Search O(log n))                       │
│   ┌──────────────────────────────────────────────────────┐   │
│   │ Block 0: offset=0, ts=1000-1099, count=100           │   │
│   │ Block 1: offset=4096, ts=1100-1199, count=100        │   │
│   │ Block 2: offset=8192, ts=1200-1299, count=100        │   │
│   └──────────────────────────────────────────────────────┘   │
│                            │                                  │
│                            ▼                                  │
│   Data Blocks                                                │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐                     │
│   │ Block 0 │  │ Block 1 │  │ Block 2 │  ...                │
│   │  4KB    │  │  4KB    │  │  4KB    │                     │
│   └─────────┘  └─────────┘  └─────────┘                     │
└──────────────────────────────────────────────────────────────┘
```

**性能指标**:
- 索引查找: O(log n)
- 范围查询 (1M blocks): ~500ns
- 内存开销/entry: 48 bytes

---

## 管理端架构

### 概述

管理端提供合约管理、结算管理、风控监控和系统监控等管理员功能，确保交易所的稳定运行和合规操作。

### 管理端数据流

```
┌──────────────────────────────────────────────────────────────┐
│                      管理端数据流                              │
│                                                                │
│  管理员 → 前端界面 → HTTP API → 管理端模块 → 核心引擎 → 存储   │
│                                                                │
│  ┌─────────┐   ┌──────────┐   ┌─────────────┐   ┌─────────┐ │
│  │ 管理员  │ → │ Admin UI │ → │ Admin API   │ → │  Core   │ │
│  │         │   │          │   │             │   │ Engines │ │
│  │ • 合约  │   │ • Vue.js │   │ • Actix-web │   │         │ │
│  │ • 结算  │   │ • Element│   │ • REST      │   │ • 账户  │ │
│  │ • 风控  │   │   UI     │   │ • JSON      │   │ • 撮合  │ │
│  │ • 监控  │   │ • ECharts│   │             │   │ • 存储  │ │
│  └─────────┘   └──────────┘   └─────────────┘   └─────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### 核心模块

#### 1. InstrumentRegistry (合约注册表)

**职责**: 管理合约的全生命周期

**核心功能**:
```rust
pub struct InstrumentRegistry {
    instruments: Arc<DashMap<String, InstrumentInfo>>,
}

impl InstrumentRegistry {
    // 合约注册与管理
    pub fn register(&self, instrument: InstrumentInfo) -> Result<()>
    pub fn update(&self, id: &str, updater: impl FnOnce(&mut InstrumentInfo)) -> Result<()>

    // 状态管理
    pub fn suspend(&self, id: &str) -> Result<()>  // 暂停交易
    pub fn resume(&self, id: &str) -> Result<()>   // 恢复交易
    pub fn delist(&self, id: &str) -> Result<()>   // 下市合约

    // 查询
    pub fn get(&self, id: &str) -> Option<InstrumentInfo>
    pub fn list_all(&self) -> Vec<InstrumentInfo>
}
```

**合约生命周期**:
```
创建 → 上市 → 交易中 ⇄ 暂停 → 下市
     (register) (Trading) (Suspended) (Delisted)
```

**下市安全检查**:
- 遍历所有账户
- 检查是否有未平仓持仓
- 返回详细错误信息（包含持仓账户列表）
- 防止数据不一致

---

#### 2. SettlementEngine (结算引擎)

**职责**: 执行日终结算和强平处理

**核心功能**:
```rust
pub struct SettlementEngine {
    account_mgr: Arc<AccountManager>,
    settlement_prices: Arc<DashMap<String, f64>>,
    settlement_history: Arc<DashMap<String, SettlementResult>>,
    force_close_threshold: f64,  // 强平阈值（默认1.0 = 100%）
}

impl SettlementEngine {
    // 结算价管理
    pub fn set_settlement_price(&self, instrument_id: String, price: f64)
    pub fn set_settlement_prices(&self, prices: HashMap<String, f64>)

    // 日终(生产入口:定时任务 + 手动接口都走这里)—— 闭市 → 撤活单 → 归档 → 结算 → 开市
    pub fn run_end_of_day(&self, trading_day: &str) -> Result<EndOfDayReport>
    pub fn set_snapshot_dir(&self, dir: String)   // 启动时注入,用于归档 / 结算后落快照

    // 只做账户结算本身(调用前撮合簿必须没有挂单,见下方「日终流程」)
    pub fn daily_settlement(&self) -> Result<SettlementResult>
    pub fn daily_settlement_for(&self, settlement_date: &str) -> Result<SettlementResult>

    // 单个账户结算
    fn settle_account(&self, user_id: &str, date: &str) -> Result<AccountSettlement>

    // 强平处理
    pub fn force_close_account(&self, user_id: &str) -> Result<()>

    // 查询
    pub fn get_settlement_history(&self) -> Vec<SettlementResult>
    pub fn get_settlement_detail(&self, date: &str) -> Option<SettlementResult>
}
```

**日终流程**(`run_end_of_day`,2026-11-08 起每自然日北京 00:00 自动执行,`config/exchange.toml` 的 `[settlement]`):
```
1. 闭市:OrderRouter 日终闸门置位 —— 新单拒 4100(→ CTP 91「日终结算中,暂停接收报单」),撤单照常;
   等在途报单计数归零
   ↓
2. 撤全部活单(GFD 日终失效):走与客户端撤单同一条路径(释放冻结、推 DIFF「已撤单」);
   逐簿复核,路由不认得的残单直接摘掉;簿上仍有挂单 → 本轮不结算,直接开市
   ↓
3. 归档:落一轮最新快照,硬链接进 snapshots/history/<交易日>/(同日再结算 → -2、-3)
   ↓
4. 结算价:手工设置优先,否则撮合簿最新价;结算前写进每个持仓,结算后清空
   ↓
5. 账户结算(下方「结算流程」;qars settle:今仓→昨仓、清当日委托/成交、退冻结)
   ↓
6. 新交易日:客户端编号(CTP 报单引用)查重表清空,client_order_ids.jsonl 归档
   ↓
7. 开市 → 落结算后快照 → 按 DIFF 推一次各账户 accounts / positions
```

> ⚠️ 为什么不能直接调 `daily_settlement`:qars `settle()` 清空当日委托,而挂单还在撮合簿里 ——
> 之后撤单(qars `cancel_order` expect)或成交(`receive_deal_sim` unwrap)会 panic,release 是 panic=abort,
> 整个进程退出。网关在调 qars 前另有守卫,但那只是纵深防御。
>
> 手动触发:`POST /api/admin/settlement/execute`(结算今天,北京时间),响应顶层仍是 `SettlementResult` 字段,
> 另带 `sweep`(撤单 / 失败 / 摘残单 / 剩余)、`archived_snapshots`、`archive_dir`、`settlement_prices`、`halted_ms`。

**结算流程**:
```
1. 设置结算价
   ↓
2. 遍历所有账户
   ↓
3. 计算持仓盈亏（盯市）
   • 多头盈亏 = (结算价 - 开仓价) × 多头数量 × 合约乘数
   • 空头盈亏 = (开仓价 - 结算价) × 空头数量 × 合约乘数
   ↓
4. 计算累计手续费
   • 从账户累计值获取
   ↓
5. 更新账户权益
   • 新权益 = 原权益 + 持仓盈亏 + 平仓盈亏 - 手续费
   ↓
6. 计算风险度
   • 风险度 = 占用保证金 / 账户权益
   ↓
7. 检查强平
   • 如果风险度 >= 100%，记录强平账户
   ↓
8. 保存结算结果
   • 总账户数、成功数、失败数
   • 强平账户列表
   • 总手续费、总盈亏
```

**强平处理**:
- 自动识别风险度 >= 100% 的账户
- 记录到 `force_closed_accounts` 列表
- 可配置强平阈值

---

#### 3. RiskMonitor (风控监控) ⚠️ 部分实现

**职责**: 实时监控账户风险

**规划功能**:
```rust
pub struct RiskMonitor {
    account_mgr: Arc<AccountManager>,
    risk_threshold: f64,
}

impl RiskMonitor {
    // 风险账户筛选
    pub fn get_risk_accounts(&self, threshold: f64) -> Vec<RiskAccount>

    // 保证金监控汇总
    pub fn get_margin_summary(&self) -> MarginSummary

    // 强平记录
    pub fn get_liquidation_records(&self, start_date: &str, end_date: &str)
        -> Vec<LiquidationRecord>
}
```

**风险等级**:
- **正常**: 风险度 < 50%
- **警告**: 50% ≤ 风险度 < 80%
- **高风险**: 80% ≤ 风险度 < 100%
- **强平**: 风险度 >= 100%

**状态**: 前端已实现，后端API待开发

---

#### 4. SystemMonitor (系统监控)

**职责**: 监控系统运行状态

**核心功能**:
```rust
pub struct SystemMonitor {
    account_mgr: Arc<AccountManager>,
    storage_subscriber: Arc<StorageSubscriber>,
}

impl SystemMonitor {
    // 系统状态
    pub fn get_system_status(&self) -> SystemStatus {
        SystemStatus {
            cpu_usage: get_cpu_usage(),
            memory_usage: get_memory_usage(),
            disk_usage: get_disk_usage(),
            uptime: get_uptime(),
            process_count: get_process_count(),
        }
    }

    // 存储监控
    pub fn get_storage_status(&self) -> StorageStatus {
        let stats = self.storage_subscriber.get_stats();
        StorageStatus {
            wal_size: stats.wal_size,
            wal_files: stats.wal_files,
            memtable_size: stats.memtable_size,
            memtable_entries: stats.memtable_entries,
            sstable_count: stats.sstable_count,
            sstable_size: stats.sstable_size,
        }
    }

    // 账户监控
    pub fn get_account_stats(&self) -> AccountStats

    // 订单监控
    pub fn get_order_stats(&self) -> OrderStats

    // 成交监控
    pub fn get_trade_stats(&self) -> TradeStats

    // 生成报告
    pub fn generate_report(&self) -> MonitoringReport
}
```

**监控指标**:
```
系统层:
• CPU使用率
• 内存使用率
• 磁盘使用率
• 运行时间
• 进程数

存储层:
• WAL大小和文件数
• MemTable大小和条目数
• SSTable数量和总大小

业务层:
• 账户总数/活跃数
• 总资金/总保证金
• 订单总数/待处理数
• 成交总数/总成交额
```

---

### 管理端API路由

| 功能模块 | API路由 | 数量 | 状态 |
|---------|---------|------|------|
| 合约管理 | `/admin/instruments/*` | 6个 | ✅ |
| 结算管理 | `/admin/settlement/*` | 5个 | ✅ |
| 风控管理 | `/admin/risk/*` | 3个 | ⚠️ |
| 系统监控 | `/monitoring/*` | 6个 | ✅ |

**权限控制**:
- 管理端API需要管理员权限
- Token验证 (JWT)
- 操作审计日志

---

### 管理端前端页面

| 页面 | 路由 | 功能 | 状态 |
|-----|------|------|------|
| 合约管理 | `/admin-instruments` | 合约CRUD、状态管理 | ✅ |
| 结算管理 | `/admin-settlement` | 设置结算价、执行结算、查询历史 | ✅ |
| 风控监控 | `/admin-risk` | 风险账户、保证金监控、强平记录 | ✅ |
| 账户管理 | `/admin-accounts` | 账户列表、详情、审核 | ✅ |
| 交易管理 | `/admin-transactions` | 全市场成交、订单统计 | ✅ |
| 系统监控 | `/monitoring` | 系统/存储/业务监控 | ✅ |

**技术栈**:
- Vue 2.6.11
- Element UI
- vxe-table
- ECharts

---

### 数据流示例：日终结算

```
1. 管理员设置结算价
   POST /admin/settlement/batch-set-prices
   {
     "prices": [
       { "instrument_id": "IF2501", "settlement_price": 3850.0 },
       { "instrument_id": "IH2501", "settlement_price": 2650.0 }
     ]
   }
   ↓
2. SettlementEngine 存储结算价
   settlement_prices.insert("IF2501", 3850.0)
   settlement_prices.insert("IH2501", 2650.0)
   ↓
3. 管理员执行结算
   POST /admin/settlement/execute
   ↓
4. SettlementEngine 遍历账户
   accounts = account_mgr.get_all_accounts()
   for account in accounts {
       settle_account(account)
   }
   ↓
5. 计算盈亏并更新账户
   • 持仓盈亏 = f(结算价, 开仓价, 数量)
   • 新权益 = 原权益 + 盈亏 - 手续费
   • 风险度 = 保证金 / 权益
   ↓
6. 识别强平账户
   if risk_ratio >= 100% {
       force_closed_accounts.push(user_id)
   }
   ↓
7. 返回结算结果
   {
     "settlement_date": "2025-10-05",
     "total_accounts": 1250,
     "settled_accounts": 1247,
     "failed_accounts": 3,
     "force_closed_accounts": ["user123", "user456"],
     "total_commission": 152340.50,
     "total_profit": -234560.75
   }
```

---

## 数据流设计

### 订单提交流程

```
┌─────────┐
│ 客户端   │
└────┬────┘
     │ 1. POST /api/order/submit
     ▼
┌─────────────────────────┐
│  HTTP Handler           │
│  (handlers::submit_order)│
└────┬────────────────────┘
     │ 2. SubmitOrderRequest
     ▼
┌─────────────────────────┐
│  OrderRouter            │
│  .submit_order()        │
└────┬────────────────────┘
     │ 3. 生成订单ID (原子递增)
     │
     ├─────────────────────┐
     │                     ▼
     │              ┌──────────────┐
     │              │ PreTradeCheck│
     │              │ .check()     │
     │              └──────┬───────┘
     │                     │
     │ 4. RiskCheckResult  │
     │◄────────────────────┘
     │
     │ 5. 通过 → 创建订单
     │    拒绝 → 返回错误
     │
     ├─────────────────────┐
     │                     ▼
     │         ┌───────────────────┐
     │         │ MatchingEngine    │
     │         │ .process_order()  │
     │         └───────┬───────────┘
     │                 │
     │ 6. Vec<Result>  │
     │◄────────────────┘
     │
     ├─────────────────────┐
     │                     ▼
     │         ┌───────────────────┐
     │         │ TradeGateway      │
     │         │ .handle_filled()  │
     │         └───────┬───────────┘
     │                 │
     │                 ├─ 7a. 更新账户
     │                 ├─ 7b. 推送成交通知
     │                 └─ 7c. 推送账户更新
     │
     ▼
┌─────────────────────────┐
│  WebSocket Subscribers  │
│  (实时接收通知)          │
└─────────────────────────┘
```

### WebSocket 实时推送流程

```
┌──────────┐
│ 客户端    │
└────┬─────┘
     │ 1. WebSocket 连接
     │    ws://localhost:8081/ws?user_id=user001
     ▼
┌─────────────────────────┐
│  WsSession (Actor)      │
│  .started()             │
└────┬────────────────────┘
     │ 2. 注册到 WebSocketServer
     │
     ├─────────────────────┐
     │                     ▼
     │         ┌───────────────────┐
     │         │ TradeGateway      │
     │         │ .subscribe()      │
     │         └───────────────────┘
     │
     │ 3. 客户端发送认证
     │    { "type": "auth", "user_id": "...", "token": "..." }
     ▼
┌─────────────────────────┐
│  WsMessageHandler       │
│  .handle_message()      │
└────┬────────────────────┘
     │ 4. 验证 Token
     │    SessionState → Authenticated
     │
     │ 5. 客户端订阅
     │    { "type": "subscribe", "channels": ["trade", "account_update"] }
     │
     │ 6. 当有成交时
     │    TradeGateway.send_notification()
     │
     ├──────────────────────────┐
     │                          ▼
     │              ┌──────────────────────┐
     │              │ crossbeam::channel   │
     │              │ (Sender → Receiver)  │
     │              └──────────┬───────────┘
     │                         │
     │ 7. WsSession.started()  │
     │    10ms 轮询接收         │
     │◄────────────────────────┘
     │
     ▼
┌─────────────────────────┐
│  客户端接收消息          │
│  ws.onmessage()         │
└─────────────────────────┘
```

---

## 并发模型

### 1. Actor 模型 (WebSocket 会话)

**Actix Actor**:
- 每个 WebSocket 连接 = 1 个 Actor
- Actor 之间通过消息传递通信
- 自动处理并发，无需手动加锁

```rust
impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // 启动心跳
        self.start_heartbeat(ctx);

        // 轮询通知
        ctx.run_interval(Duration::from_millis(10), |act, ctx| {
            // 非阻塞接收
            while let Ok(notification) = act.notification_receiver.try_recv() {
                // 发送给客户端
            }
        });
    }
}
```

### 2. 无锁并发 (DashMap)

**DashMap 优势**:
- 分段锁设计，并发读写性能优异
- 无需手动加锁，API 自动处理
- 适合高并发场景

```rust
// 订单存储
orders: Arc<DashMap<String, OrderInfo>>

// 并发读写无需锁
self.orders.insert(order_id.clone(), order_info);
let order = self.orders.get(&order_id);
```

### 3. 读写锁 (RwLock)

**parking_lot RwLock**:
- 比标准库 RwLock 性能更好
- 读锁可并发，写锁独占

```rust
// 账户存储
accounts: Arc<DashMap<String, Arc<RwLock<QA_Account>>>>

// 读操作（并发）
let account = self.account_mgr.get_account(user_id)?;
let acc = account.read();  // 多个线程可同时读
let balance = acc.accounts.balance;

// 写操作（独占）
let mut acc = account.write();  // 独占锁
acc.accounts.balance += 10000.0;
drop(acc);  // 尽早释放锁
```

### 4. 原子操作 (AtomicU64)

**无锁递增**:
```rust
order_seq: AtomicU64

// 原子递增生成订单ID
let seq = self.order_seq.fetch_add(1, Ordering::SeqCst);
let order_id = format!("O{}{:016}", timestamp, seq);
```

---

## 性能优化

### 1. 内存管理

**避免频繁分配**:
```rust
// 预分配容量
let mut orders = Vec::with_capacity(1000);

// 对象池复用
let order = order_pool.acquire();
```

**尽早释放锁**:
```rust
{
    let acc = account.read();
    let balance = acc.accounts.balance;  // 获取数据
}  // acc 被 drop，锁被释放

process_balance(balance);  // 无锁操作
```

### 2. 异步非阻塞

**Tokio 异步运行时**:
```rust
#[tokio::main]
async fn main() {
    // HTTP 服务器异步运行
    let http_server = HttpServer::new(...);
    tokio::spawn(async move {
        http_server.run().await.unwrap();
    });

    // WebSocket 服务器异步运行
    let ws_server = WebSocketServer::new(...);
    tokio::spawn(async move {
        ws_server.run().await.unwrap();
    });

    // 等待所有服务
    tokio::signal::ctrl_c().await.unwrap();
}
```

### 3. 零拷贝通信

**crossbeam unbounded channel**:
```rust
// 发送端
let (tx, rx) = crossbeam::channel::unbounded();
tx.send(notification).unwrap();  // 非阻塞

// 接收端
while let Ok(msg) = rx.try_recv() {  // 非阻塞
    process(msg);
}
```

### 4. 批量处理

**批量撮合**:
```rust
// 收集一批订单
let mut batch = Vec::with_capacity(100);
while batch.len() < 100 {
    if let Ok(order) = order_rx.try_recv() {
        batch.push(order);
    } else {
        break;
    }
}

// 批量处理
for order in batch {
    matching_engine.process_order(order);
}
```

---

## 扩展性设计

### 1. 水平扩展

**多实例部署**:
```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│  Instance 1 │      │  Instance 2 │      │  Instance 3 │
│  HTTP:8080  │      │  HTTP:8080  │      │  HTTP:8080  │
│  WS:8081    │      │  WS:8081    │      │  WS:8081    │
└──────┬──────┘      └──────┬──────┘      └──────┬──────┘
       │                    │                    │
       └────────────────────┴────────────────────┘
                            │
                  ┌─────────▼─────────┐
                  │   Load Balancer   │
                  │   (Nginx/HAProxy) │
                  └───────────────────┘
```

**注意事项**:
- WebSocket 需要 sticky session
- 订单路由需要确保同一用户请求到同一实例
- 共享状态通过 Redis 同步

### 2. 垂直扩展

**模块拆分**:
```
┌────────────────┐
│  Gateway       │  (API 网关)
└───────┬────────┘
        │
┌───────┴────────┐
│                │
▼                ▼
┌──────────┐  ┌──────────┐
│ Trading  │  │ Market   │  (交易服务 | 行情服务)
│ Service  │  │ Service  │
└──────────┘  └──────────┘
```

### 3. 插件化

**撮合引擎可替换**:
```rust
pub trait MatchingEngine {
    fn process_order(&mut self, order: Order) -> Vec<Result<Success, Failed>>;
}

// 默认实现
struct DefaultMatchingEngine { ... }

// 自定义实现
struct CustomMatchingEngine { ... }
```

**风控策略可配置**:
```rust
pub struct RiskConfig {
    pub max_position_ratio: f64,
    pub risk_ratio_reject: f64,
    // ... 可通过配置文件动态加载
}
```

---

## 安全设计

### 1. 认证授权

**Token 认证**:
```rust
// WebSocket 认证
ClientMessage::Auth { user_id, token } => {
    if verify_token(&user_id, &token) {
        session.state = SessionState::Authenticated { user_id };
    }
}
```

### 2. 风控保护

**多层风控**:
1. **盘前风控**: PreTradeCheck 检查资金、持仓、风险度
2. **盘中监控**: 实时计算风险度，接近阈值预警
3. **强平机制**: 风险度超限自动强平

### 3. 参数校验

**严格校验**:
```rust
fn check_order_params(req: &OrderCheckRequest) -> Result<(), ExchangeError> {
    if req.volume < config.min_order_volume {
        return Err(ExchangeError::InvalidOrderParams("数量过小".into()));
    }
    if req.price <= 0.0 {
        return Err(ExchangeError::InvalidOrderParams("价格非法".into()));
    }
    Ok(())
}
```

---

## 数据协议

### QIFI (Quantitative Investment Format Interface)

**账户标准格式**:
```json
{
  "account_cookie": "user001",
  "portfolio": "default",
  "account_type": "individual",
  "money": 1000000.0,
  "available": 950000.0,
  "margin": 50000.0,
  "positions": [
    {
      "instrument_id": "IX2301",
      "volume_long": 10,
      "volume_short": 0,
      "open_price_long": 120.0,
      "profit": 500.0
    }
  ],
  "trades": [ ... ],
  "orders": [ ... ]
}
```

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
