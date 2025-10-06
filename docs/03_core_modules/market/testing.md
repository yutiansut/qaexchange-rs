# 市场数据模块测试流程

## 测试概览

本文档描述市场数据模块（特别是快照生成器）的完整测试流程，包括单元测试、集成测试和性能测试。

---

## 测试层级

```
┌─────────────────────────────────────────┐
│       单元测试 (Unit Tests)              │
│   - snapshot_generator.rs 内置测试       │
│   - 数据结构验证                          │
│   - 配置默认值测试                        │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│     集成测试 (Integration Tests)         │
│   - test_snapshot_generator.rs          │
│   - 完整生成流程测试                      │
│   - 多订阅者并发测试                      │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│      性能测试 (Performance Tests)        │
│   - 生成延迟测试                          │
│   - 并发订阅者压力测试                     │
│   - 内存占用测试                          │
└─────────────────────────────────────────┘
```

---

## 1. 单元测试

### 运行单元测试

```bash
# 运行快照生成器单元测试
cargo test --lib snapshot_generator

# 带详细输出
cargo test --lib snapshot_generator -- --nocapture

# 运行特定测试
cargo test --lib test_snapshot_default
```

### 测试覆盖

#### 1.1 数据结构测试

**测试文件**: `src/market/snapshot_generator.rs:429-442`

```rust
#[test]
fn test_snapshot_default() {
    let snapshot = MarketSnapshot::default();
    assert_eq!(snapshot.last_price, 0.0);
    assert_eq!(snapshot.volume, 0);
}

#[test]
fn test_config_default() {
    let config = SnapshotGeneratorConfig::default();
    assert_eq!(config.interval_ms, 1000);
    assert!(config.enable_persistence);
}
```

**预期结果**:
```
test market::snapshot_generator::tests::test_snapshot_default ... ok
test market::snapshot_generator::tests::test_config_default ... ok
```

#### 1.2 统计更新测试

```rust
#[test]
fn test_update_trade_stats() {
    let generator = create_test_generator();

    // 第一笔成交
    generator.update_trade_stats("IF2501", 100, 380000.0);

    // 生成快照
    let snapshot = generator.generate_snapshot("IF2501").unwrap();

    assert_eq!(snapshot.volume, 100);
    assert_eq!(snapshot.turnover, 380000.0);

    // 第二笔成交（累加）
    generator.update_trade_stats("IF2501", 50, 190000.0);

    let snapshot = generator.generate_snapshot("IF2501").unwrap();

    assert_eq!(snapshot.volume, 150);
    assert_eq!(snapshot.turnover, 570000.0);
}
```

#### 1.3 OHLC 计算测试

```rust
#[test]
fn test_ohlc_calculation() {
    let generator = create_test_generator();

    generator.set_pre_close("IF2501", 3800.0);

    // 模拟价格变化：3800 → 3850 → 3780 → 3820
    let snapshots = vec![
        generate_with_price(&generator, "IF2501", 3800.0),
        generate_with_price(&generator, "IF2501", 3850.0),
        generate_with_price(&generator, "IF2501", 3780.0),
        generate_with_price(&generator, "IF2501", 3820.0),
    ];

    let final_snapshot = snapshots.last().unwrap();

    assert_eq!(final_snapshot.open, 3800.0);
    assert_eq!(final_snapshot.high, 3850.0);
    assert_eq!(final_snapshot.low, 3780.0);
    assert_eq!(final_snapshot.last_price, 3820.0);
}
```

---

## 2. 集成测试

### 运行集成测试

```bash
# 编译测试示例
cargo build --example test_snapshot_generator

# 运行测试（开发模式）
cargo run --example test_snapshot_generator

# 运行测试（发布模式，性能更好）
cargo run --example test_snapshot_generator --release

# 带日志输出
RUST_LOG=debug cargo run --example test_snapshot_generator
```

### 测试流程

**测试文件**: `examples/test_snapshot_generator.rs`

#### 步骤 1: 初始化环境

```rust
// 1. 创建撮合引擎
let matching_engine = Arc::new(ExchangeMatchingEngine::new());

// 2. 注册合约
matching_engine.register_instrument("IF2501".to_string(), 3800.0)?;

// 3. 创建快照生成器
let config = SnapshotGeneratorConfig {
    interval_ms: 1000,
    enable_persistence: false,
    instruments: vec!["IF2501".to_string()],
};
let generator = Arc::new(MarketSnapshotGenerator::new(
    matching_engine.clone(),
    config,
));

// 4. 设置昨收盘价
generator.set_pre_close("IF2501", 3800.0);
```

#### 步骤 2: 启动生成器

```rust
// 启动后台线程
let _generator_handle = generator.clone().start();
```

#### 步骤 3: 订阅快照

```rust
// 创建3个订阅者
let subscriber1 = generator.subscribe();
let subscriber2 = generator.subscribe();
let subscriber3 = generator.subscribe();
```

#### 步骤 4: 更新统计

```rust
// 模拟成交事件
generator.update_trade_stats("IF2501", 100, 380000.0);
std::thread::sleep(Duration::from_millis(500));
generator.update_trade_stats("IF2501", 50, 190000.0);
```

#### 步骤 5: 消费快照

```rust
// 订阅者1: 打印基础信息
let consumer1 = std::thread::spawn(move || {
    while let Ok(snapshot) = subscriber1.recv_timeout(Duration::from_secs(2)) {
        println!("快照: {} @ {:.2} (涨跌: {:.2}%, 成交量: {})",
            snapshot.instrument_id,
            snapshot.last_price,
            snapshot.change_percent,
            snapshot.volume,
        );
    }
});

// 订阅者2: 打印买卖档位
let consumer2 = std::thread::spawn(move || {
    while let Ok(snapshot) = subscriber2.recv_timeout(Duration::from_secs(2)) {
        println!("买一: {:.2} x {}, 卖一: {:.2} x {}",
            snapshot.bid_price1, snapshot.bid_volume1,
            snapshot.ask_price1, snapshot.ask_volume1,
        );
    }
});

// 订阅者3: 打印OHLC
let consumer3 = std::thread::spawn(move || {
    while let Ok(snapshot) = subscriber3.recv_timeout(Duration::from_secs(2)) {
        println!("OHLC: O={:.2} H={:.2} L={:.2} (成交额: {:.2})",
            snapshot.open, snapshot.high, snapshot.low, snapshot.turnover,
        );
    }
});
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
   ⚠️  简化版测试：跳过订单提交，直接测试快照生成

6️⃣  模拟成交事件...
   ✅ 第1笔成交: volume=100, turnover=380,000
   ✅ 第2笔成交: volume=50, turnover=190,000

7️⃣  订阅者开始消费快照...
   (等待 5 秒，每秒接收一次快照)

   [订阅者1] 收到快照 #1: IF2501 @ 3800.00 (涨跌: 0.00%, 成交量: 150)
   [订阅者2] 买一: 0.00 x 0, 卖一: 0.00 x 0
   [订阅者3] OHLC: O=3800.00 H=3800.00 L=3800.00 (成交额: 570000.00)
   [订阅者1] 收到快照 #2: IF2501 @ 3800.00 (涨跌: 0.00%, 成交量: 150)
   [订阅者2] 买一: 0.00 x 0, 卖一: 0.00 x 0
   [订阅者3] OHLC: O=3800.00 H=3800.00 L=3800.00 (成交额: 570000.00)
   ...

8️⃣  测试统计:
   总快照数: 5
   运行时长: 5.01s
   快照频率: ~1.0/s

✅ 测试完成！
```

### 验收标准

- ✅ 快照生成频率: ~1.0/s (误差 < 5%)
- ✅ 统计累加正确: volume = 150, turnover = 570000
- ✅ 3个订阅者均收到5个快照
- ✅ 无panic或错误日志

---

## 3. 性能测试

### 运行性能测试

```bash
# 发布模式运行（性能最优）
cargo run --example test_snapshot_generator --release

# 使用 criterion 基准测试（如果有）
cargo bench --bench snapshot_bench
```

### 测试指标

#### 3.1 生成延迟

**测试方法**: 测量从订单簿读取到快照生成完成的时间。

```rust
use std::time::Instant;

let start = Instant::now();
let snapshot = generator.generate_snapshot("IF2501")?;
let elapsed = start.elapsed();

println!("生成延迟: {:?}", elapsed);
assert!(elapsed.as_micros() < 1000); // < 1ms
```

**预期结果**:
```
生成延迟: 200μs - 500μs
```

#### 3.2 订阅者吞吐量

**测试方法**: 创建多个订阅者并发消费。

```rust
let num_subscribers = 100;
let mut handles = vec![];

for i in 0..num_subscribers {
    let rx = generator.subscribe();
    handles.push(std::thread::spawn(move || {
        let mut count = 0;
        while count < 10 {
            if rx.recv_timeout(Duration::from_secs(2)).is_ok() {
                count += 1;
            }
        }
    }));
}

// 等待所有订阅者完成
for handle in handles {
    handle.join().unwrap();
}
```

**预期结果**:
```
100 订阅者 @ 1s 间隔: 无延迟
1000 订阅者 @ 1s 间隔: 延迟 < 10ms
```

#### 3.3 内存占用

**测试方法**: 使用 `valgrind` 或 Rust 内存分析工具。

```bash
# 使用 heaptrack (Linux)
heaptrack cargo run --example test_snapshot_generator --release

# 查看内存报告
heaptrack_gui heaptrack.test_snapshot_generator.*
```

**预期结果**:
```
单个快照: ~500 bytes
日内统计: ~100 bytes/合约
100个订阅者: ~10KB（转发线程开销）
```

---

## 4. 压力测试

### 4.1 高频率生成

**测试**: 快照间隔 100ms（10 snapshots/s）

```rust
let config = SnapshotGeneratorConfig {
    interval_ms: 100,  // 100ms
    instruments: vec!["IF2501".to_string()],
    // ...
};
```

**验收标准**:
- CPU 占用 < 10%
- 内存占用稳定（无泄漏）
- 快照延迟 < 1ms

### 4.2 多合约并发

**测试**: 同时生成10个合约的快照

```rust
let instruments = (0..10)
    .map(|i| format!("IF250{}", i))
    .collect::<Vec<_>>();

let config = SnapshotGeneratorConfig {
    interval_ms: 1000,
    instruments,
    // ...
};
```

**验收标准**:
- 总生成延迟 < 5ms
- 无快照丢失
- 内存增长线性（~500 bytes/合约）

### 4.3 大量订阅者

**测试**: 1000+ 订阅者并发

```bash
# 修改测试代码
let num_subscribers = 1000;

# 运行
cargo run --example test_snapshot_generator --release
```

**验收标准**:
- 推送延迟 < 100ms（P99）
- 无订阅者断开连接
- 内存占用 < 50MB

---

## 5. 回归测试

### 测试清单

在每次代码修改后运行以下测试：

```bash
# 1. 单元测试
cargo test --lib snapshot_generator

# 2. 集成测试
cargo run --example test_snapshot_generator

# 3. 完整服务器测试
cargo run --bin qaexchange-server

# 4. WebSocket 订阅测试（手动）
# - 启动服务器
# - 连接 WebSocket
# - 订阅快照频道
# - 验证快照推送
```

### 自动化测试脚本

创建 `scripts/test_snapshot.sh`:

```bash
#!/bin/bash
set -e

echo "=== 快照生成器回归测试 ==="

echo "1. 单元测试..."
cargo test --lib snapshot_generator -- --nocapture

echo "2. 集成测试..."
timeout 15 cargo run --example test_snapshot_generator

echo "3. 编译检查..."
cargo check --all-features

echo "✅ 所有测试通过！"
```

---

## 6. 已知限制

### 当前限制

1. **订单簿数据**: 测试示例未实际插入订单到订单簿（qars API 限制）
   - **解决方案**: 在完整服务器环境下通过 OrderRouter 提交订单

2. **持久化测试**: 快照 WAL 持久化功能未实现
   - **状态**: 待 Phase 5 实现

3. **WebSocket 测试**: 未包含 WebSocket 订阅端点测试
   - **状态**: 待 Phase 4 实现

### 规避方法

```rust
// 完整测试环境（集成 OrderRouter）
let order_router = Arc::new(OrderRouter::new(/* ... */));

// 提交订单
order_router.submit_order(/* ... */)?;

// 等待撮合
std::thread::sleep(Duration::from_millis(100));

// 验证快照
let snapshot = generator.generate_snapshot("IF2501")?;
assert!(snapshot.bid_price1 > 0.0);  // 验证有买盘
```

---

## 7. 故障排查

### 常见问题

#### 7.1 快照中最新价为 0

**症状**: `snapshot.last_price == 0.0`

**原因**:
- 订单簿无成交记录
- 未设置昨收盘价

**解决**:
```rust
// 设置昨收盘价
generator.set_pre_close("IF2501", 3800.0);

// 或提交订单触发成交
order_router.submit_order(/* ... */)?;
```

#### 7.2 订阅者收不到快照

**症状**: `subscriber.recv_timeout()` 超时

**原因**:
- 生成器未启动
- 订阅时机过早（生成器启动前）

**解决**:
```rust
// 先启动生成器
let _handle = generator.clone().start();

// 等待启动完成
std::thread::sleep(Duration::from_millis(100));

// 再订阅
let subscriber = generator.subscribe();
```

#### 7.3 成交统计不更新

**症状**: `snapshot.volume == 0`

**原因**: 未调用 `update_trade_stats()`

**解决**:
```rust
// 确保 TradeGateway 已设置 market_data_service
trade_gateway.set_market_data_service(market_data_service.clone());

// 或手动调用
market_data_service.update_trade_stats("IF2501", 100, 380000.0);
```

---

## 8. 测试报告模板

### 测试结果记录

```markdown
## 快照生成器测试报告

**测试时间**: 2025-01-07 10:00:00
**测试环境**: Ubuntu 20.04, Rust 1.91.0
**版本**: v1.0.0

### 测试结果

| 测试项 | 预期 | 实际 | 状态 |
|--------|------|------|------|
| 单元测试 | 全部通过 | 100% 通过 | ✅ |
| 集成测试 | 快照频率 1/s | 1.02/s | ✅ |
| 生成延迟 | < 1ms | 250μs | ✅ |
| 并发订阅 | 100 订阅者 | 100 订阅者 | ✅ |
| 内存占用 | < 10MB | 5.2MB | ✅ |

### 性能数据

- 生成延迟: P50=200μs, P99=500μs
- 推送延迟: P50=10μs, P99=50μs
- CPU占用: 平均 2.5%
- 内存占用: 峰值 5.2MB

### 问题与建议

- [ ] 添加 WebSocket 订阅端点测试
- [ ] 实现 WAL 持久化测试
- [ ] 增加压力测试（1000+ 订阅者）
```

---

## 参考资料

- [快照生成器文档](./snapshot_generator.md)
- [测试指南](../../06_development/testing.md)
- [性能基准](../../07_reference/benchmarks.md)

---

**@yutiansut @quantaxis** - 2025-01-07
