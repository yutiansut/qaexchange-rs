# 因子计算系统

**版本**: v1.1.0
**作者**: @yutiansut @quantaxis
**最后更新**: 2025-12-10

---

## 概述

因子计算系统是 QAExchange-RS 的核心量化分析模块，提供**流批一体化**的因子计算能力。系统同时支持：
- **流式计算 (Stream)**: O(1) 增量更新，适用于实时行情
- **批量计算 (Batch)**: Polars 向量化，适用于历史回测

```
┌─────────────────────────────────────────────────────────────┐
│                    因子计算系统架构                           │
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │   DSL 层    │    │  DAG 管理   │    │  状态存储   │     │
│  │ (语法解析)  │───▶│ (依赖拓扑)  │───▶│ (物化视图)  │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                  │                  │             │
│         ▼                  ▼                  ▼             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              流批一体执行引擎                         │   │
│  │  ┌──────────────┐        ┌──────────────┐          │   │
│  │  │  Stream Engine│        │  Batch Engine │          │   │
│  │  │  (增量算子)   │        │  (Polars SQL) │          │   │
│  │  └──────────────┘        └──────────────┘          │   │
│  └─────────────────────────────────────────────────────┘   │
│         │                                    │             │
│         ▼                                    ▼             │
│  ┌─────────────┐                    ┌─────────────┐       │
│  │ 增量算子库  │                    │   Polars    │       │
│  │ Ring Buffer │                    │  LazyFrame  │       │
│  │   Welford   │                    │             │       │
│  └─────────────┘                    └─────────────┘       │
└─────────────────────────────────────────────────────────────┘
```

---

## 核心模块

### 1. 增量算子库 (`operators/`)

高性能 O(1) 增量计算算子，适用于实时流处理：

| 算子 | 功能 | 时间复杂度 | 空间复杂度 |
|------|------|-----------|-----------|
| `RollingMean` | 滚动均值 | O(1) | O(n) |
| `RollingStd` | 滚动标准差 (Welford) | O(1) | O(n) |
| `RollingCorr` | 滚动相关系数 | O(1) | O(n) |
| `EMA` | 指数移动平均 | O(1) | O(1) |
| `DEMA` | 双指数移动平均 | O(1) | O(1) |
| `RSI` | 相对强弱指数 | O(1) | O(1) |
| `MACD` | 指数平滑异同线 | O(1) | O(1) |
| `BollingerBands` | 布林带 | O(1) | O(n) |
| `ATR` | 平均真实范围 | O(1) | O(1) |

#### 示例：RollingMean

```rust
use qaexchange::factor::operators::rolling::RollingMean;

let mut ma = RollingMean::new(5); // 5日均线

// 流式更新
for price in [10.0, 11.0, 12.0, 13.0, 14.0, 15.0] {
    ma.update(price);
    println!("MA5: {}", ma.value());
}
// 输出: MA5: 13.0 (最后5个值的平均)
```

#### 示例：RSI

```rust
use qaexchange::factor::operators::rolling::RSI;

let mut rsi = RSI::new(14); // 14日RSI

// 模拟上涨趋势
for i in 0..20 {
    rsi.update(100.0 + i as f64);
}

if let Some(value) = rsi.value() {
    println!("RSI: {:.2}", value); // RSI > 50 (上涨趋势)
}
```

---

### 2. 环形缓冲区 (`ring_buffer.rs`)

滑动窗口的高效实现，支持 O(1) 的增量统计：

```rust
use qaexchange::factor::operators::ring_buffer::NumericRingBuffer;

let mut buffer = NumericRingBuffer::new(100);

// 推入数据
buffer.push(1.0);
buffer.push(2.0);
buffer.push(3.0);

// O(1) 统计
println!("Sum: {}", buffer.sum());      // 增量累加
println!("Mean: {}", buffer.mean());    // sum / count
println!("Min: {}", buffer.min());      // 滑动最小值
println!("Max: {}", buffer.max());      // 滑动最大值
```

**特性**:
- 预分配内存，无运行时分配
- 支持 `PairedRingBuffer` 用于协方差/相关系数计算
- 线程安全版本 `SyncRingBuffer` 可选

---

### 3. Welford 算法 (`welford.rs`)

数值稳定的在线统计算法，避免浮点溢出：

```rust
use qaexchange::factor::operators::welford::WelfordState;

let mut state = WelfordState::new();

// 在线更新
for value in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
    state.update(value);
}

println!("Mean: {}", state.mean());         // 5.0
println!("Variance: {}", state.variance()); // 4.0
println!("Std: {}", state.std());           // 2.0
println!("Skewness: {}", state.skewness()); // 偏度
println!("Kurtosis: {}", state.kurtosis()); // 峰度
```

**优势**:
- 数值稳定：避免大数相减导致的精度损失
- 单遍扫描：只需遍历数据一次
- 支持窗口化：`WindowedWelfordState` 支持滑动窗口

---

### 4. 因子 DAG 管理器 (`dag.rs`)

管理因子之间的依赖关系，支持并行计算：

```rust
use qaexchange::factor::dag::{FactorDag, FactorNode, FactorType};

let dag = FactorDag::new();

// 添加因子节点
dag.add_node(FactorNode {
    id: "close".to_string(),
    factor_type: FactorType::Source,
    dependencies: vec![],
    depth: 0,
});

dag.add_node(FactorNode {
    id: "ma5".to_string(),
    factor_type: FactorType::Rolling { window: 5, func: "mean".to_string() },
    dependencies: vec!["close".to_string()],
    depth: 1,
});

dag.add_node(FactorNode {
    id: "ma_diff".to_string(),
    factor_type: FactorType::BinaryOp { op: "sub".to_string() },
    dependencies: vec!["ma5".to_string(), "ma10".to_string()],
    depth: 2,
});

// 拓扑排序
let order = dag.topological_sort();
// ["close", "ma5", "ma10", "ma_diff"]

// 获取并行层级
let levels = dag.get_parallel_levels();
// [[close], [ma5, ma10], [ma_diff]]
```

---

### 5. 流批一体引擎 (`engine.rs`)

统一的因子计算接口，自动选择最优执行路径：

```rust
use qaexchange::factor::engine::{FactorEngine, FactorDef, RollingFunc};

let mut engine = FactorEngine::new();

// 注册因子
engine.register(
    "volatility",
    "20日波动率",
    FactorDef::Rolling {
        source: "close".to_string(),
        window: 20,
        func: RollingFunc::Std,
    },
    "20日收盘价标准差",
);

// 流式计算 (实时)
engine.init_stream_factor("volatility").unwrap();
for price in prices {
    let vol = engine.stream_update("volatility", price)?;
    println!("实时波动率: {}", vol);
}

// 批量计算 (历史)
let df = polars::prelude::df!{
    "close" => &[10.0, 11.0, 12.0, 13.0, 14.0]
}?.lazy();

let result = engine.batch_compute(df, &["volatility"])?;
```

---

### 6. 物化视图状态管理 (`state.rs` & `view.rs`)

因子计算状态的持久化和快照管理：

```rust
use qaexchange::factor::state::{GlobalStateSnapshot, CheckpointStore, CheckpointConfig};
use qaexchange::factor::view::{MaterializedView, ViewConfig};

// 创建物化视图
let view = MaterializedView::new(ViewConfig {
    ttl: Duration::from_secs(300), // 5分钟过期
    auto_init: true,
});

// 更新 Tick 数据
view.update_tick("SHFE.cu2501", TickData {
    last_price: 85000.0,
    volume: 1000,
    timestamp: 1732456789000,
});

// 获取因子状态
if let Some(state) = view.get("SHFE.cu2501") {
    println!("MA5: {}", state.ma_5.value());
    println!("RSI: {:?}", state.rsi_14.value());
}

// 检查点存储 (✨ ZSTD 压缩支持 @yutiansut @quantaxis)
let store = CheckpointStore::new(CheckpointConfig {
    base_path: PathBuf::from("/data/checkpoints"),
    max_checkpoints: 10,
    compress: true,  // 启用 ZSTD Level 3 压缩
    checkpoint_interval: Duration::from_secs(60),
});

// 保存检查点 (自动 ZSTD 压缩，典型压缩比 30-50%)
let snapshot = view.create_snapshot();
let checkpoint_id = store.save_checkpoint(&snapshot)?;
// 日志输出: "Checkpoint 1 compressed: 1024000 -> 358400 bytes (35.0% ratio)"

// 恢复检查点 (自动 ZSTD 解压)
let restored = store.load_checkpoint(checkpoint_id)?;
view.restore_from_snapshot(&restored);
```

---

## DSL 语法 (可选)

因子定义的 DSL 语法，便于配置化管理：

```pest
// grammar.pest
ma5 = rolling_mean(close, 5)
ma20 = rolling_mean(close, 20)
ma_diff = ma5 - ma20
volatility = rolling_std(close, 20)
rsi = RSI(close, 14)
macd = MACD(close, 12, 26, 9)
signal = if rsi > 70 then -1 else if rsi < 30 then 1 else 0
```

```rust
use qaexchange::dsl::parser::AstBuilder;

let program = AstBuilder::parse(r#"
    ma5 = rolling_mean(close, 5)
    signal = if ma5 > ma20 then 1 else -1
"#)?;

for stmt in program.statements {
    println!("{:?}", stmt);
}
```

---

## 性能指标

| 操作 | 延迟 | 吞吐量 |
|------|------|--------|
| RollingMean.update() | ~15 ns | 66M ops/s |
| RollingStd.update() (Welford) | ~25 ns | 40M ops/s |
| RSI.update() | ~20 ns | 50M ops/s |
| MACD.update() | ~30 ns | 33M ops/s |
| 环形缓冲区 push | ~5 ns | 200M ops/s |
| Polars 批量计算 (1M rows) | ~50 ms | 20M rows/s |
| ZSTD 压缩 (Level 3) ✨ | ~4 ms/MB | 250 MB/s |
| ZSTD 解压 ✨ | ~1 ms/MB | 1000 MB/s |
| 检查点保存 (100KB 状态) ✨ | ~5 ms | 包含压缩 |
| 检查点恢复 ✨ | ~2 ms | 包含解压 |

---

## 相关文档

- [增量算子详解](./incremental_operators.md) - 算子实现原理
- [流批一体引擎](./stream_batch_engine.md) - 引擎架构设计
- [因子 WAL 集成](./wal_persister.md) - 异步持久化与恢复 ✨ **新增**
- [查询引擎](../storage/query_engine.md) - Polars SQL 集成
- [压缩策略](../storage/compression.md) - 因子数据压缩配置
- [二级索引](../storage/index.md) - 因子查询索引
- [系统架构](../../02_architecture/system_overview.md) - 整体架构

---

## 文件结构

```
src/factor/
├── mod.rs              # 模块导出
├── engine.rs           # 流批一体引擎
├── dag.rs              # 因子 DAG 管理
├── state.rs            # 状态存储与检查点
├── view.rs             # 物化视图管理
├── wal_persister.rs    # WAL 异步持久化 ✨ 新增
└── operators/
    ├── mod.rs          # 算子模块导出
    ├── basic.rs        # 基础算子
    ├── ring_buffer.rs  # 环形缓冲区
    ├── rolling.rs      # 滚动窗口算子
    └── welford.rs      # Welford 统计算法
```
