# QAExchange 术语表

本文档提供 QAExchange 系统中使用的所有术语的完整定义和说明。

---

## 📖 目录

- [交易术语](#交易术语)
- [技术术语](#技术术语)
- [架构术语](#架构术语)
- [协议术语](#协议术语)
- [存储术语](#存储术语)
- [性能术语](#性能术语)
- [缩写对照表](#缩写对照表)

---

## 交易术语

### 账户相关

#### Account (账户)
用户在交易所的资金账户，包含资金信息、持仓信息、风险指标等。

**字段结构**:
- `user_id`: 账户ID
- `balance`: 账户权益（静态权益 + 浮动盈亏）
- `available`: 可用资金（权益 - 保证金占用）
- `margin`: 保证金占用
- `risk_ratio`: 风险度（保证金占用 / 账户权益）

**相关代码**: `qars::qaaccount::QA_Account`

#### Static Balance (静态权益)
账户的初始资金加上已实现盈亏，不包含持仓浮动盈亏。

**计算公式**:
```
静态权益 = 上日结算准备金 + 入金 - 出金 + 平仓盈亏 - 手续费
```

#### Float Profit (浮动盈亏)
未平仓持仓的盈亏，随市场价格实时变化。

**计算公式**:
```
多头浮动盈亏 = (当前价 - 开仓价) × 持仓量 × 合约乘数
空头浮动盈亏 = (开仓价 - 当前价) × 持仓量 × 合约乘数
```

#### Balance (账户权益)
账户的总资金量，包含静态权益和浮动盈亏。

**计算公式**:
```
账户权益 = 静态权益 + 浮动盈亏
```

#### Available (可用资金)
账户中可用于开仓的资金量。

**计算公式**:
```
可用资金 = 账户权益 - 保证金占用 - 冻结保证金
```

#### Margin (保证金)
持仓占用的保证金总额。

**计算公式**:
```
保证金 = Σ (持仓量 × 最新价 × 合约乘数 × 保证金率)
```

**QAExchange 保证金率**: 10% (固定)

#### Risk Ratio (风险度)
衡量账户风险水平的指标。

**计算公式**:
```
风险度 = 保证金占用 / 账户权益
```

**风险等级**:
- 风险度 < 80%: 正常
- 80% ≤ 风险度 < 100%: 警告
- 风险度 ≥ 100%: 强制平仓

#### Force Close (强制平仓/强平)
当账户风险度达到或超过 100% 时，系统自动平掉所有持仓以控制风险。

**触发条件**: `risk_ratio >= 1.0`

**执行机制**:
1. 按市价平掉所有持仓
2. 记录强平日志
3. 推送强平通知给用户

---

### 订单相关

#### Order (订单/委托)
用户发起的交易指令。

**核心字段**:
- `order_id`: 订单ID（用户侧，可自定义）
- `exchange_order_id`: 交易所订单号（系统生成，自增i64）
- `instrument_id`: 合约代码（如 `SHFE.cu2501`）
- `direction`: 买卖方向（BUY/SELL）
- `offset`: 开平标志（OPEN/CLOSE）
- `volume_orign`: 原始委托量
- `volume_left`: 剩余未成交量
- `limit_price`: 限价
- `status`: 订单状态

**相关代码**: `qars::qaorder::QAOrder`

#### Order ID (订单ID)
用户侧订单标识，用户可自定义（如 `Strategy1.Order001`）。

**格式**: 任意字符串，建议格式 `{strategy}.{instance}.{seq}`

#### Exchange Order ID (交易所订单号)
交易所内部生成的订单唯一标识。

**特性**:
- 类型: `i64`
- 按 instrument 维度自增
- 保证同一合约的订单严格有序
- 用于交易所内部记录和回报

**生成器**: `ExchangeIdGenerator::next_sequence(instrument_id)`

#### Direction (买卖方向)
订单的交易方向。

**取值**:
- `BUY`: 买入（做多）
- `SELL`: 卖出（做空）

#### Offset (开平标志)
订单是开仓还是平仓。

**取值**:
- `OPEN`: 开仓（建立新持仓）
- `CLOSE`: 平仓（平掉已有持仓）
- `CLOSE_TODAY`: 平今仓（部分品种适用）
- `CLOSE_YESTERDAY`: 平昨仓（部分品种适用）

#### Order Status (订单状态)
订单的当前状态。

**状态枚举**:
- `PENDING`: 等待提交
- `ACCEPTED`: 已接受（进入撮合队列）
- `REJECTED`: 已拒绝（未进入撮合）
- `PARTIAL_FILLED`: 部分成交
- `FILLED`: 完全成交
- `CANCELLING`: 撤单中
- `CANCELLED`: 已撤单
- `CANCEL_REJECTED`: 撤单被拒绝

**状态转换**:
```
PENDING → ACCEPTED → PARTIAL_FILLED → FILLED
                  ↘ CANCELLING → CANCELLED
                  ↘ REJECTED
```

#### Price Type (价格类型)
订单的报价方式。

**取值**:
- `LIMIT`: 限价单（指定价格）
- `MARKET`: 市价单（以对手价成交）
- `ANY`: 任意价（立即成交）

**QAExchange 支持**: LIMIT, MARKET

#### Volume Condition (成交量条件)
订单的成交量要求。

**取值**:
- `ANY`: 任意数量（可部分成交）
- `MIN`: 最小成交量
- `ALL`: 全部成交（FOK - Fill or Kill）

**QAExchange 默认**: ANY

#### Time Condition (时间条件)
订单的有效期。

**取值**:
- `IOC`: Immediate or Cancel（立即成交，否则撤销）
- `GFD`: Good for Day（当日有效）
- `GTC`: Good till Cancel（撤销前有效）
- `GTD`: Good till Date（指定日期前有效）

**QAExchange 默认**: GFD

---

### 成交相关

#### Trade (成交)
订单撮合成功后产生的成交记录。

**核心字段**:
- `trade_id`: 成交ID（自增i64）
- `order_id`: 关联的订单ID
- `exchange_order_id`: 关联的交易所订单号
- `instrument_id`: 合约代码
- `volume`: 成交量
- `price`: 成交价
- `timestamp`: 成交时间

**生成规则**: 一笔订单可能产生多笔成交

#### Trade ID (成交ID)
交易所内部生成的成交唯一标识。

**特性**:
- 类型: `i64`
- 按 instrument 维度自增（与订单号共用序列）
- 保证同一合约的成交严格有序

**生成器**: `ExchangeIdGenerator::next_sequence(instrument_id)`

#### Fill (成交回报)
交易所推送给用户的成交通知。

**回报类型**: `ExchangeResponse::Trade`

**内容**:
- `trade_id`: 成交ID
- `exchange_order_id`: 关联订单号
- `volume`: 成交量
- `price`: 成交价
- `timestamp`: 成交时间

**重要**: 交易所只推送 TRADE 回报，不判断订单是 FILLED 还是 PARTIAL_FILLED，由账户自己根据 `volume_left` 判断。

---

### 持仓相关

#### Position (持仓)
用户在某个合约上的持仓信息。

**核心字段**:
- `instrument_id`: 合约代码
- `volume_long`: 多头持仓量
- `volume_short`: 空头持仓量
- `volume_long_today`: 今日多头持仓
- `volume_short_today`: 今日空头持仓
- `open_price_long`: 多头开仓均价
- `open_price_short`: 空头开仓均价
- `float_profit`: 浮动盈亏
- `margin`: 保证金占用

**相关代码**: `qars::qaposition::QA_Position`

#### Long Position (多头持仓)
买入开仓建立的持仓。

**盈亏计算**:
```
浮动盈亏 = (当前价 - 开仓均价) × 持仓量 × 合约乘数
```

**平仓方式**: 卖出平仓

#### Short Position (空头持仓)
卖出开仓建立的持仓。

**盈亏计算**:
```
浮动盈亏 = (开仓均价 - 当前价) × 持仓量 × 合约乘数
```

**平仓方式**: 买入平仓

#### Today Position (今仓)
当日开仓的持仓。

**特点**:
- 部分交易所今仓手续费较高
- 平今仓需要使用 `CLOSE_TODAY` 标志

#### Yesterday Position (昨仓)
昨日及之前的持仓，经过日终结算转换而来。

**转换规则**: 日终结算时，所有今仓转为昨仓

---

### 合约相关

#### Instrument (合约/品种)
可交易的金融工具。

**核心字段**:
- `instrument_id`: 合约代码（如 `SHFE.cu2501`）
- `exchange_id`: 交易所代码（SHFE/DCE/CZCE/CFFEX/INE）
- `product_id`: 品种代码（cu/ag/rb等）
- `price_tick`: 最小变动价位（如 10元）
- `volume_multiple`: 合约乘数（如 5吨/手）
- `margin_ratio`: 保证金率（如 0.1）
- `commission`: 手续费（如 万分之五）

**示例**:
```rust
Instrument {
    instrument_id: "SHFE.cu2501",
    exchange_id: "SHFE",
    product_id: "cu",
    price_tick: 10.0,
    volume_multiple: 5,
    margin_ratio: 0.1,
    commission: 0.0005,
    ...
}
```

#### Exchange (交易所)
期货交易所。

**支持的交易所**:
- `SHFE`: 上海期货交易所
- `DCE`: 大连商品交易所
- `CZCE`: 郑州商品交易所
- `CFFEX`: 中国金融期货交易所
- `INE`: 上海国际能源交易中心

#### Product (品种)
合约品种（如铜、铝、黄金等）。

**示例**:
- `cu`: 铜
- `ag`: 银
- `au`: 黄金
- `rb`: 螺纹钢
- `IF`: 沪深300指数期货

#### Price Tick (最小变动价位)
价格变动的最小单位。

**示例**:
- 铜 (cu): 10 元/吨
- 螺纹钢 (rb): 1 元/吨
- 黄金 (au): 0.02 元/克

#### Volume Multiple (合约乘数)
一手合约对应的实物数量。

**示例**:
- 铜 (cu): 5 吨/手
- 螺纹钢 (rb): 10 吨/手
- 黄金 (au): 1000 克/手

#### Margin Ratio (保证金率)
开仓需要的保证金比例。

**QAExchange 默认**: 10%

**计算**:
```
保证金 = 价格 × 数量 × 合约乘数 × 保证金率
例: 50000元/吨 × 1手 × 5吨/手 × 10% = 25000元
```

---

### 撮合相关

#### Orderbook (订单簿)
撮合引擎维护的买卖订单队列。

**结构**:
```
卖5: 50100 (10手)
卖4: 50090 (15手)
卖3: 50080 (20手)
卖2: 50070 (25手)
卖1: 50060 (30手)  ← 卖一价
----------------------
买1: 50050 (30手)  ← 买一价
买2: 50040 (25手)
买3: 50030 (20手)
买4: 50020 (15手)
买5: 50010 (10手)
```

**撮合规则**: 价格优先、时间优先

**相关代码**: `qars::qamarket::matchengine::Orderbook`

#### Matching Engine (撮合引擎)
负责订单撮合的核心组件。

**撮合原则**:
1. **价格优先**: 买方出价高的优先，卖方出价低的优先
2. **时间优先**: 同价位先到先得

**撮合流程**:
1. 收到新订单
2. 检查是否可立即成交
3. 如可成交，生成成交记录
4. 如不可成交或部分成交，剩余量挂单
5. 推送成交回报

**性能**:
- 撮合延迟: P99 < 100μs
- 订单吞吐: > 100K/s

#### Bid Price (买入价/出价)
买方愿意买入的价格。

**买一价**: 最高买入价（orderbook 买方队列顶部）

#### Ask Price (卖出价/要价)
卖方愿意卖出的价格。

**卖一价**: 最低卖出价（orderbook 卖方队列顶部）

#### Last Price (最新价)
最近一笔成交的价格。

**用途**:
- 计算浮动盈亏
- 计算保证金
- 显示行情

#### Settlement Price (结算价)
日终结算时使用的参考价格。

**设置方式**:
- 手动设置: `POST /api/admin/settlement/set-price`
- 批量设置: `POST /api/admin/settlement/batch-set-prices`

**用途**:
- 日终结算
- 计算当日盈亏
- 调整保证金

---

### 结算相关

#### Settlement (结算)
每日交易结束后的账户清算过程。

**流程**:
1. 设置结算价
2. 计算持仓盈亏
3. 更新账户权益
4. 检查风险
5. 执行强平（如需要）
6. 今仓转昨仓

**执行时间**: 交易日结束后（通常15:30之后）

#### Daily Settlement (日终结算)
完整的每日结算流程。

**API**: `POST /api/admin/settlement/execute`

**结算公式**:
```
账户权益 = 上日结算准备金 + 持仓盈亏 + 平仓盈亏 - 手续费
可用资金 = 账户权益 - 保证金占用
```

#### Close Profit (平仓盈亏)
平仓实现的盈亏。

**计算**:
```
多头平仓盈亏 = (平仓价 - 开仓价) × 平仓量 × 合约乘数
空头平仓盈亏 = (开仓价 - 平仓价) × 平仓量 × 合约乘数
```

#### Position Profit (持仓盈亏)
日终结算时使用结算价计算的持仓盈亏。

**计算**:
```
持仓盈亏 = (结算价 - 昨结算价) × 持仓量 × 合约乘数
```

---

## 技术术语

### 并发相关

#### DashMap
高性能并发哈希表。

**特性**:
- Lock-free 读取（大部分情况）
- 分片锁（Sharded Lock）
- 支持并发读写

**用途**:
- 账户管理: `DashMap<account_id, Arc<RwLock<QA_Account>>>`
- 订单管理: `DashMap<order_id, QAOrder>`
- 合约管理: `DashMap<instrument_id, Instrument>`

**相关代码**: `dashmap` crate

#### Arc (Atomic Reference Counted)
原子引用计数智能指针。

**用途**: 多线程共享数据

**示例**:
```rust
Arc<RwLock<QA_Account>>  // 可在多线程间共享的账户
Arc<DashMap<String, QAOrder>>  // 可在多线程间共享的订单表
```

#### RwLock (Read-Write Lock)
读写锁。

**特性**:
- 多个读者同时访问
- 写者独占访问

**QAExchange 使用**: `parking_lot::RwLock`（性能优于 std::sync::RwLock）

#### Mutex
互斥锁。

**特性**: 任何时候只允许一个线程访问

**QAExchange 使用**: `parking_lot::Mutex`

#### Atomic
原子类型。

**常用类型**:
- `AtomicI64`: 原子 i64（用于 ExchangeIdGenerator）
- `AtomicBool`: 原子布尔值
- `AtomicUsize`: 原子 usize

**操作顺序**:
- `Ordering::SeqCst`: 顺序一致性（最强保证）
- `Ordering::Acquire`: 获取语义
- `Ordering::Release`: 释放语义
- `Ordering::Relaxed`: 宽松顺序（最弱保证）

---

### 异步相关

#### Tokio
Rust 异步运行时。

**特性**:
- 异步 I/O
- 任务调度
- 定时器

**QAExchange 使用**:
- HTTP/WebSocket 服务
- 异步存储写入
- 后台任务

#### Actix-web
高性能 Web 框架。

**用途**:
- HTTP API (`/api/*`)
- WebSocket 服务 (`/ws`)

**性能**: 50K+ req/s

#### async/await
Rust 异步语法。

**示例**:
```rust
async fn submit_order(order: QAOrder) -> Result<String> {
    // 异步提交订单
    let result = order_router.submit_order(order).await?;
    Ok(result)
}
```

#### Future
异步计算的抽象。

**trait**: `std::future::Future`

#### Task
异步任务。

**创建**: `tokio::spawn(async move { ... })`

---

### 消息传递

#### Channel (通道)
线程间消息传递。

**类型**:
- `crossbeam::channel`: 高性能通道（用于 WebSocket 通知）
- `tokio::mpsc`: 异步多生产者单消费者通道
- `tokio::oneshot`: 一次性通道

**QAExchange 使用**:
```rust
// WebSocket 通知
let (tx, rx) = crossbeam::channel::unbounded();
subscribers.insert(user_id, tx);
```

#### MPSC (Multi-Producer Single-Consumer)
多生产者单消费者通道。

**用途**: 多个线程向一个消费者发送消息

#### Unbounded Channel
无界通道（无容量限制）。

**注意**: 可能导致内存无限增长，需要配合背压控制

**QAExchange 背压阈值**: 500 消息

---

### 序列化

#### Serde
Rust 序列化/反序列化框架。

**支持格式**:
- JSON
- MessagePack
- Bincode
- etc.

**QAExchange 使用**:
```rust
#[derive(Serialize, Deserialize)]
struct QAOrder { ... }
```

#### JSON
JavaScript Object Notation。

**用途**:
- HTTP API 请求/响应
- WebSocket 消息（边界序列化）
- 配置文件

#### rkyv (Zero-Copy Deserialization)
零拷贝序列化库。

**特性**:
- 序列化: ~300 ns/消息（4x vs JSON）
- 反序列化: ~20 ns/消息（125x vs JSON）
- 零内存分配（反序列化时）

**用途**:
- Notification 内部传递
- SSTable OLTP 存储
- 跨进程通信（IPC）

**限制**:
- 不支持 `&'static str`（使用 `String`）
- `Arc<str>` 在归档数据中直接可访问

**相关文档**: `docs/05_integration/serialization.md`

---

## 架构术语

### 系统架构

#### Master-Slave (主从架构)
分布式复制架构。

**角色**:
- **Master**: 主节点，处理所有写请求
- **Slave**: 从节点，复制主节点数据，处理读请求
- **Candidate**: 候选节点，参与选举

**QAExchange 实现**:
- Raft-inspired 选举
- 批量日志复制
- 自动故障转移

**相关文档**: `docs/03_core_modules/storage/replication.md`

#### Broker-Gateway (中转网关架构)
通知系统架构。

**组件**:
- **NotificationBroker**: 通知路由中心
  - 优先级队列（P0-P3）
  - 消息去重
  - 订阅管理
- **NotificationGateway**: 推送网关
  - WebSocket 会话管理
  - 批量推送
  - 背压控制

**优势**:
- 解耦撮合引擎和通知推送
- 支持优先级
- 支持批量优化

**相关文档**: `docs/03_core_modules/notification/architecture.md`

#### LSM-Tree (Log-Structured Merge-Tree)
日志结构合并树存储架构。

**层次**:
1. **WAL** (Write-Ahead Log): 持久化日志
2. **MemTable**: 内存表
3. **SSTable**: 磁盘表

**流程**:
```
写入 → WAL → MemTable → (满) → SSTable → (Compaction)
```

**优势**:
- 写入性能高（顺序写）
- 读取性能可接受（Bloom Filter + mmap）
- 支持高吞吐

**QAExchange 实现**:
- OLTP 路径: rkyv SSTable
- OLAP 路径: Parquet SSTable

#### CQRS (Command Query Responsibility Segregation)
命令查询职责分离。

**QAExchange 实现**:
- **命令路径** (OLTP): SkipMap MemTable → rkyv SSTable
- **查询路径** (OLAP): Arrow2 MemTable → Parquet SSTable

**优势**:
- 写入优化（低延迟）
- 查询优化（分析性能）

---

### 服务架构

#### Service Layer (服务层)
对外提供接口的层。

**组件**:
- HTTP 服务 (`service/http/`)
- WebSocket 服务 (`service/websocket/`)

#### Business Layer (业务层)
核心业务逻辑。

**组件**:
- 账户管理 (`exchange/account_mgr.rs`)
- 订单路由 (`exchange/order_router.rs`)
- 撮合引擎 (`matching/engine.rs`)
- 交易网关 (`exchange/trade_gateway.rs`)

#### Storage Layer (存储层)
数据持久化。

**组件**:
- WAL (`storage/wal/`)
- MemTable (`storage/memtable/`)
- SSTable (`storage/sstable/`)
- 复制系统 (`replication/`)

---

### 设计模式

#### Actor Model (Actor 模型)
并发编程模型。

**QAExchange 使用**: WebSocket Session 是 Actix Actor

**特性**:
- 每个 Actor 有独立的状态
- 通过消息通信
- 无共享内存

#### Repository Pattern (仓储模式)
数据访问抽象。

**示例**: `AccountManager` 管理所有账户数据

#### Observer Pattern (观察者模式)
事件通知机制。

**QAExchange 实现**: 订阅者订阅通知频道，接收推送

---

## 协议术语

### QIFI (QA Interoperable Finance Interface)
QA 可互操作金融接口 - 数据层协议。

**定义位置**: `qars::qaprotocol::qifi`

**核心结构**:
- `Account` (19 字段): 资金账户
- `Position` (28 字段): 持仓数据
- `Order` (14 字段): 委托单
- `BankDetail`: 银行信息

**特性**:
- JSON 序列化
- 字段自恰性（如 `balance = static_balance + float_profit`）
- 向后兼容

**用途**: QAExchange 直接复用 QIFI 数据结构，零修改

### TIFI (Trade Interface for Finance)
金融交易接口 - 传输层协议。

**定义位置**: `qars::qaprotocol::tifi`

**核心消息**:
- `Peek`: 获取数据更新（对应 DIFF `peek_message`）
- `RtnData`: 返回数据（对应 DIFF `rtn_data`）
- `ReqLogin`: 登录请求
- `ReqOrder`: 下单请求
- `ReqCancel`: 撤单请求

**特性**:
- WebSocket 全双工通信
- 异步请求-响应

**与 DIFF 关系**: TIFI 已实现 DIFF 核心传输机制

### DIFF (Differential Information Flow for Finance)
差分信息流金融协议 - 同步层协议。

**核心理念**: 将异步事件回报转为同步数据访问

**机制**:
1. **业务截面** (Business Snapshot): 服务端维护完整业务状态
2. **差分推送** (JSON Merge Patch): 推送增量变化
3. **客户端镜像**: 客户端维护截面镜像

**协议文档**: `docs/05_integration/diff_protocol.md`

**消息类型**:
- `peek_message`: 请求更新
- `rtn_data`: 返回差分数据
- `subscribe_quote`: 订阅行情
- `insert_order`: 下单
- `cancel_order`: 撤单

**数据字段**:
- `quotes`: 行情数据
- `trade.{user_id}.accounts`: 账户数据
- `trade.{user_id}.positions`: 持仓数据
- `trade.{user_id}.orders`: 订单数据
- `trade.{user_id}.trades`: 成交数据
- `notify`: 通知消息

#### JSON Merge Patch (RFC 7386)
JSON 增量更新标准。

**规则**:
1. 对象合并: `{"a": 1} + {"b": 2} = {"a": 1, "b": 2}`
2. 字段覆盖: `{"a": 1} + {"a": 2} = {"a": 2}`
3. 字段删除: `{"a": 1} + {"a": null} = {}`
4. 数组替换: `{"arr": [1,2]} + {"arr": [3]} = {"arr": [3]}`

**QAExchange 实现**: `docs/09_archive/zh_docs/json_merge_patch.md`

---

### WebSocket 协议

#### peek_message
客户端请求数据更新。

**格式**:
```json
{"aid": "peek_message"}
```

**服务端行为**:
- 如有更新，立即返回 `rtn_data`
- 如无更新，等待有更新时再返回（长轮询）

#### rtn_data
服务端推送数据更新。

**格式**:
```json
{
  "aid": "rtn_data",
  "data": [
    {"balance": 10237421.1},
    {"float_profit": 283114.78}
  ]
}
```

**处理**: 依次应用 JSON Merge Patch

#### subscribe_quote
订阅行情。

**格式**:
```json
{
  "aid": "subscribe_quote",
  "ins_list": "SHFE.cu2501,SHFE.ag2506"
}
```

**注意**: 后续订阅会覆盖前一次

#### insert_order
下单。

**格式**:
```json
{
  "aid": "insert_order",
  "user_id": "user1",
  "order_id": "order001",
  "instrument_id": "SHFE.cu2501",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 1,
  "price_type": "LIMIT",
  "limit_price": 50000
}
```

#### cancel_order
撤单。

**格式**:
```json
{
  "aid": "cancel_order",
  "user_id": "user1",
  "order_id": "order001"
}
```

---

## 存储术语

### WAL (Write-Ahead Log)
预写式日志。

**用途**: 崩溃恢复

**特性**:
- 顺序写入（高性能）
- CRC32 校验
- 批量写入优化

**Record 类型**:
- `AccountCreated`: 账户创建
- `OrderInserted`: 订单插入
- `TradeExecuted`: 成交
- `TickData`: Tick 行情
- `OrderBookSnapshot`: 订单簿快照
- etc.

**性能**:
- 写入延迟: P99 < 50ms (HDD)
- 批量吞吐: > 78K entries/s

**相关文档**: `docs/03_core_modules/storage/wal.md`

### MemTable
内存表。

**类型**:
- **OLTP MemTable**: 基于 SkipMap，低延迟写入
- **OLAP MemTable**: 基于 Arrow2，列式存储

**Flush 触发**:
- 大小达到阈值（如 64 MB）
- 定时 Flush（如 5 分钟）
- 手动触发

**性能**:
- 写入延迟: P99 < 10μs

**相关文档**: `docs/03_core_modules/storage/memtable.md`

### SSTable (Sorted String Table)
有序字符串表。

**类型**:
- **OLTP SSTable**: rkyv 格式，零拷贝读取
- **OLAP SSTable**: Parquet 格式，列式存储

**结构**:
```
┌─────────────────┐
│  Bloom Filter   │  (快速判断 key 不存在)
├─────────────────┤
│   Index Block   │  (key → offset 映射)
├─────────────────┤
│   Data Block    │  (实际数据)
├─────────────────┤
│    Metadata     │  (magic number, version, checksum)
└─────────────────┘
```

**访问方式**:
- mmap 零拷贝读取
- Bloom Filter 加速查找

**性能**:
- 读取延迟: P99 < 50μs

**相关文档**: `docs/03_core_modules/storage/sstable.md`

### Bloom Filter (布隆过滤器)
概率性数据结构，用于快速判断元素是否存在。

**特性**:
- False Positive (假阳性): 可能误判存在
- False Negative (假阴性): 不会误判不存在

**QAExchange 配置**:
- 哈希函数数量: 7
- 假阳性率: 1%

**用途**: SSTable 快速判断 key 不存在，避免磁盘读取

### Compaction (压实)
后台合并 SSTable 的过程。

**策略**: Leveled Compaction

**流程**:
1. 选择需要合并的 SSTable
2. 归并排序
3. 删除过期/重复数据
4. 生成新 SSTable
5. 删除旧 SSTable

**触发条件**:
- Level 0 文件数 > 4
- Level N 文件总大小超过阈值

**相关代码**: `src/storage/compaction/`

### mmap (Memory-Mapped File)
内存映射文件。

**特性**:
- 零拷贝读取（直接访问文件内容）
- 操作系统管理缓存
- 适合随机读取

**QAExchange 使用**: SSTable 读取（`src/storage/sstable/mmap_reader.rs`）

### Checkpoint (检查点)
存储系统快照。

**内容**:
- 当前 WAL 位置
- MemTable 快照
- SSTable 列表

**用途**:
- 快速恢复
- 减少 WAL 回放时间

**触发**: 定时创建（如每小时）

---

### 查询引擎

#### Polars
Rust 数据分析库（类似 Pandas）。

**特性**:
- 列式存储
- 懒执行（LazyFrame）
- 查询优化

**QAExchange 使用**: OLAP 查询引擎

**性能**:
- SQL 查询 (100 rows): < 10ms
- Parquet 扫描: > 1GB/s
- 聚合查询: < 50ms

**相关文档**: `docs/03_core_modules/storage/query_engine.md`

#### Arrow2
Apache Arrow 的 Rust 实现。

**用途**:
- 列式内存格式
- Parquet 读写
- 零拷贝数据交换

#### Parquet
列式存储文件格式。

**特性**:
- 高压缩比
- 列裁剪（Column Pruning）
- 谓词下推（Predicate Pushdown）

**QAExchange 使用**: OLAP SSTable 格式

---

## 性能术语

### Latency (延迟)
操作完成所需的时间。

**度量**:
- P50 (中位数): 50% 的请求延迟低于此值
- P95: 95% 的请求延迟低于此值
- P99: 99% 的请求延迟低于此值
- P999: 99.9% 的请求延迟低于此值

**QAExchange 目标**:
- 撮合延迟: P99 < 100μs
- WAL 写入: P99 < 50ms
- MemTable 写入: P99 < 10μs

### Throughput (吞吐量)
单位时间内处理的操作数量。

**单位**:
- ops/s (operations per second)
- req/s (requests per second)
- MB/s (megabytes per second)

**QAExchange 目标**:
- 订单吞吐: > 100K orders/s
- WAL 批量写入: > 78K entries/s
- Parquet 扫描: > 1GB/s

### QPS (Queries Per Second)
每秒查询数。

**HTTP API 性能**: 50K+ QPS

### TPS (Transactions Per Second)
每秒事务数。

### Backpressure (背压)
当生产速度超过消费速度时，限制生产速度的机制。

**QAExchange 实现**:
- WebSocket 通知队列 > 500 时丢弃低优先级消息
- MemTable 满时阻塞写入

### Zero-Copy (零拷贝)
避免内存复制的优化技术。

**QAExchange 使用**:
- rkyv 反序列化（直接访问归档数据）
- mmap SSTable 读取（直接访问文件内容）
- Notification 内部传递（Arc 共享）

**性能提升**:
- rkyv 反序列化: 125x vs JSON
- 减少 CPU 和内存压力

---

## 缩写对照表

### 业务缩写

| 缩写 | 全称 | 中文 |
|------|------|------|
| QIFI | QA Interoperable Finance Interface | QA 可互操作金融接口 |
| TIFI | Trade Interface for Finance | 金融交易接口 |
| DIFF | Differential Information Flow for Finance | 差分信息流金融协议 |
| OLTP | Online Transaction Processing | 在线事务处理 |
| OLAP | Online Analytical Processing | 在线分析处理 |
| WAL | Write-Ahead Log | 预写式日志 |
| SST | Sorted String Table | 有序字符串表 |
| LSM | Log-Structured Merge-Tree | 日志结构合并树 |
| CQRS | Command Query Responsibility Segregation | 命令查询职责分离 |

### 交易所缩写

| 缩写 | 全称 | 中文 |
|------|------|------|
| SHFE | Shanghai Futures Exchange | 上海期货交易所 |
| DCE | Dalian Commodity Exchange | 大连商品交易所 |
| CZCE | Zhengzhou Commodity Exchange | 郑州商品交易所 |
| CFFEX | China Financial Futures Exchange | 中国金融期货交易所 |
| INE | Shanghai International Energy Exchange | 上海国际能源交易中心 |

### 技术缩写

| 缩写 | 全称 | 说明 |
|------|------|------|
| HTTP | Hypertext Transfer Protocol | 超文本传输协议 |
| WS | WebSocket | WebSocket 协议 |
| JSON | JavaScript Object Notation | JavaScript 对象表示法 |
| API | Application Programming Interface | 应用程序接口 |
| REST | Representational State Transfer | 表述性状态转移 |
| CRC | Cyclic Redundancy Check | 循环冗余校验 |
| UUID | Universally Unique Identifier | 通用唯一识别码 |
| JWT | JSON Web Token | JSON 网络令牌 |
| TLS | Transport Layer Security | 传输层安全 |
| gRPC | gRPC Remote Procedure Call | gRPC 远程过程调用 |

### 订单缩写

| 缩写 | 全称 | 中文 |
|------|------|------|
| IOC | Immediate or Cancel | 立即成交或撤销 |
| GFD | Good for Day | 当日有效 |
| GTC | Good till Cancel | 撤销前有效 |
| GTD | Good till Date | 指定日期前有效 |
| FOK | Fill or Kill | 全部成交或撤销 |
| FAK | Fill and Kill | 立即成交剩余撤销 |

### 性能缩写

| 缩写 | 全称 | 说明 |
|------|------|------|
| QPS | Queries Per Second | 每秒查询数 |
| TPS | Transactions Per Second | 每秒事务数 |
| RPS | Requests Per Second | 每秒请求数 |
| P50 | 50th Percentile | 第50百分位（中位数） |
| P95 | 95th Percentile | 第95百分位 |
| P99 | 99th Percentile | 第99百分位 |
| P999 | 99.9th Percentile | 第99.9百分位 |

### Rust 缩写

| 缩写 | 全称 | 说明 |
|------|------|------|
| Arc | Atomic Reference Counted | 原子引用计数 |
| Rc | Reference Counted | 引用计数 |
| Mutex | Mutual Exclusion | 互斥锁 |
| RwLock | Read-Write Lock | 读写锁 |
| MPSC | Multi-Producer Single-Consumer | 多生产者单消费者 |
| SPSC | Single-Producer Single-Consumer | 单生产者单消费者 |

---

## 相关文档

- **[常见问题 FAQ](faq.md)** - 使用和排障
- **[性能基准](benchmarks.md)** - 详细性能测试数据
- **[核心模块文档](../03_core_modules/)** - 详细技术文档
- **[API 文档](../04_api/)** - API 接口文档
- **[集成指南](../05_integration/)** - 协议集成

---

**版本**: v1.0.0
**最后更新**: 2025-10-06
**维护者**: QAExchange Team

---

[返回文档中心](../README.md)
