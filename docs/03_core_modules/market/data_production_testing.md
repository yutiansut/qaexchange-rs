# 数据生产测试指南

**版本**: v0.1.0
**更新日期**: 2025-12-17
**开发团队**: @yutiansut @quantaxis

---

## 📋 目录

1. [测试概述](#测试概述)
2. [逐笔委托测试](#逐笔委托测试)
3. [逐笔成交测试](#逐笔成交测试)
4. [Tick数据测试](#tick数据测试)
5. [快照数据测试](#快照数据测试)
6. [K线数据测试](#k线数据测试)
7. [端到端数据流测试](#端到端数据流测试)
8. [测试执行指南](#测试执行指南)

---

## 测试概述

### 数据生产测试目标

| 测试维度 | 测试数量 | 验证内容 |
|----------|----------|----------|
| 逐笔委托 | 4个 | 订单录入、买卖双边、批量提交 |
| 逐笔成交 | 4个 | 成交生成、价格时间优先、部分成交 |
| Tick数据 | 4个 | 实时行情、价格更新、成交量统计 |
| 快照数据 | 4个 | 盘口深度、限制档位、动态更新 |
| K线数据 | 4个 | 多周期聚合、OHLCV计算、跨周期传播 |
| 端到端 | 2个 | 完整数据流、多标的并发 |

### 测试架构

```
撮合引擎 (ExchangeMatchingEngine)
    │
    ├── 订单处理 (process_order)
    │       │
    │       ├── 逐笔委托 → OrderProcessingResult::Accepted
    │       └── 逐笔成交 → OrderProcessingResult::Filled/PartiallyFilled
    │
    └── 行情服务 (MarketDataService)
            │
            ├── on_trade() → Tick数据生成
            ├── get_orderbook_snapshot() → 快照生成
            └── get_klines() → K线聚合
```

---

## 逐笔委托测试

### 1.1 单边委托录入

**测试函数**: `test_order_record_single_order`

**场景描述**:
```
提交单个限价买单:
  - 合约: TEST001
  - 方向: BUY
  - 价格: 100.0
  - 数量: 10

验证: 订单被正确接受 (Accepted)
```

**运行命令**:
```bash
cargo test test_order_record_single_order --lib
```

### 1.2 双边委托录入

**测试函数**: `test_order_record_both_sides`

**场景描述**:
```
同一合约提交买卖双边委托:
  - 买单: @100.0 × 10
  - 卖单: @110.0 × 5
  - 两单不交叉，均为挂单
```

**验证点**:
- [x] 买单状态: Accepted
- [x] 卖单状态: Accepted
- [x] 无成交

### 1.3 批量委托

**测试函数**: `test_order_record_batch_orders`

**场景描述**:
```
批量提交100个买单:
  - 价格区间: 90.0 ~ 99.0 (随机分布)
  - 验证所有订单被接受
```

### 1.4 序列号递增

**测试函数**: `test_order_record_sequence_numbers`

**场景描述**:
```
验证订单序列号连续递增:
  - 提交10个订单
  - 检查seq_num: 1, 2, 3, ...
```

---

## 逐笔成交测试

### 2.1 单笔完全成交

**测试函数**: `test_trade_record_single_fill`

**场景描述**:
```
1. 挂卖单: @100.0 × 10
2. 提交买单: @100.0 × 10

预期: 买单完全成交 (Filled)
```

**验证点**:
- [x] 成交数量: 10
- [x] 成交价格: 100.0
- [x] 卖单状态: Filled
- [x] 买单状态: Filled

### 2.2 价格时间优先

**测试函数**: `test_trade_record_price_time_priority`

**场景描述**:
```
1. 挂卖单A: @100.0 × 5 (先入)
2. 挂卖单B: @100.0 × 5 (后入)
3. 提交大买单: @100.0 × 10

验证: 先与卖单A成交，再与卖单B成交
```

### 2.3 部分成交

**测试函数**: `test_trade_record_partial_fill`

**场景描述**:
```
1. 挂卖单: @100.0 × 5
2. 提交买单: @100.0 × 10

预期: 买单部分成交 (PartiallyFilled)
      成交量=5, 剩余量=5
```

### 2.4 多笔连续成交

**测试函数**: `test_trade_record_multiple_fills`

**场景描述**:
```
1. 挂多个卖单:
   - @100.0 × 5
   - @101.0 × 5
   - @102.0 × 5
2. 提交大买单: @105.0 × 12

验证: 与3个卖单依次成交
      总成交量: 12
```

---

## Tick数据测试

### 3.1 基础Tick生成

**测试函数**: `test_tick_generation_basic`

**场景描述**:
```
通过成交触发Tick:
1. 撮合一笔 @100.0 × 10 的交易
2. 调用 market_service.on_trade()
3. 验证Tick数据已生成
```

### 3.2 价格更新

**测试函数**: `test_tick_price_update`

**场景描述**:
```
多笔成交后验证最新价:
1. 成交 @100.0
2. 成交 @101.0
3. 成交 @99.5

验证: last_price = 99.5
```

### 3.3 累计成交量

**测试函数**: `test_tick_volume_accumulation`

**场景描述**:
```
验证成交量正确累加:
1. 成交 10手
2. 成交 20手
3. 成交 15手

验证: total_volume = 45
```

### 3.4 多合约Tick隔离

**测试函数**: `test_tick_multi_instrument`

**场景描述**:
```
验证不同合约Tick数据隔离:
- TEST001: 成交@100.0
- TEST002: 成交@200.0

验证: 两合约Tick数据互不影响
```

---

## 快照数据测试

### 4.1 基础快照生成

**测试函数**: `test_snapshot_basic_generation`

**场景描述**:
```
构建简单订单簿:
- 买盘: @99.0 × 10, @98.0 × 20
- 卖盘: @101.0 × 15, @102.0 × 25

获取快照验证盘口数据
```

### 4.2 深度限制

**测试函数**: `test_snapshot_depth_limit`

**场景描述**:
```
构建深度订单簿 (20档):
- 买盘: 20档
- 卖盘: 20档

验证: 快照depth参数限制有效
```

**注意**: 实际深度可能受实现限制

### 4.3 快照更新

**测试函数**: `test_snapshot_update_after_order`

**场景描述**:
```
1. 获取初始快照
2. 新增订单
3. 获取更新后快照

验证: 盘口数据正确更新
```

### 4.4 成交后快照

**测试函数**: `test_snapshot_after_trade`

**场景描述**:
```
1. 构建订单簿
2. 执行撮合成交
3. 验证快照反映成交后状态
```

---

## K线数据测试

### 5.1 基础K线生成

**测试函数**: `test_kline_basic_generation`

**场景描述**:
```
通过Tick数据触发K线生成:
1. 推送多个不同价格的Tick
2. 验证K线OHLCV数据正确
```

**K线字段验证**:
- Open: 第一个成交价
- High: 最高成交价
- Low: 最低成交价
- Close: 最后成交价
- Volume: 累计成交量

### 5.2 多周期K线

**测试函数**: `test_kline_multiple_periods`

**场景描述**:
```
验证多周期K线同步生成:
- Sec3 (3秒)
- Min1 (1分钟)
- Min5 (5分钟)
- Min15 (15分钟)
- etc.
```

### 5.3 K线聚合器

**测试函数**: `test_kline_aggregator_ohlc`

**场景描述**:
```
验证KLineAggregator OHLC计算:
1. 更新价格序列: 100, 105, 95, 102
2. 验证: O=100, H=105, L=95, C=102
```

### 5.4 跨周期K线传播

**测试函数**: `test_kline_cross_period_propagation`

**场景描述**:
```
验证KLineManager正确传播:
- 成交数据 → 所有订阅周期
- 各周期K线独立聚合
```

---

## 端到端数据流测试

### 6.1 完整数据流

**测试函数**: `test_end_to_end_data_flow`

**场景描述**:
```
模拟完整交易流程:
1. 注册合约
2. 提交买卖订单
3. 撮合成交
4. 验证:
   - 订单记录
   - 成交记录
   - Tick生成
   - 快照更新
   - K线聚合
```

**架构图**:
```
Order → Orderbook → Trade → MarketDataService
                              ├── Tick
                              ├── Snapshot
                              └── KLine
```

### 6.2 多标的并发数据流

**测试函数**: `test_end_to_end_multi_instrument`

**场景描述**:
```
并发处理5个合约:
- 每合约: 10买单 + 10卖单
- 验证各合约数据独立且正确
```

---

## 测试执行指南

### 运行全部数据生产测试

```bash
# Debug模式
cargo test market::data_production_tests --lib

# Release模式 (性能测试)
cargo test market::data_production_tests --lib --release

# 显示详细输出
cargo test market::data_production_tests --lib -- --nocapture
```

### 运行特定类别测试

```bash
# 逐笔委托测试
cargo test test_order_record --lib

# 逐笔成交测试
cargo test test_trade_record --lib

# Tick数据测试
cargo test test_tick --lib

# 快照测试
cargo test test_snapshot --lib

# K线测试
cargo test test_kline --lib

# 端到端测试
cargo test test_end_to_end --lib
```

### 测试统计

| 类别 | 通过 | 失败 | 总计 |
|------|------|------|------|
| 逐笔委托 | 4 | 0 | 4 |
| 逐笔成交 | 4 | 0 | 4 |
| Tick数据 | 4 | 0 | 4 |
| 快照数据 | 4 | 0 | 4 |
| K线数据 | 4 | 0 | 4 |
| 端到端 | 2 | 0 | 2 |
| **总计** | **22** | **0** | **22** |

---

## 相关文档

- [市场数据模块](./README.md)
- [快照生成器](./snapshot_generator.md)
- [K线实时推送系统](../../06_development/KLINE_IMPLEMENTATION_SUMMARY.md)
- [撮合引擎测试指南](../matching/testing.md)
