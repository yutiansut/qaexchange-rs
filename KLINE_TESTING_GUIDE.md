# K线系统测试指南

完整 K线实时推送系统的端到端测试指南

**作者**: @yutiansut @quantaxis
**日期**: 2025-10-07

---

## 1. 系统架构回顾

### 数据流

```
下单 → 撮合引擎 → 成交Tick
                    ↓
              MarketDataBroadcaster (tick频道)
                    ↓
              KLineActor (订阅tick)
                    ↓
          K线聚合 (3s/1min/5min/...)
                    ↓
              MarketDataBroadcaster (kline频道)
                    ↓
        DiffHandler (订阅kline + 转换为DIFF格式)
                    ↓
          SnapshotManager.push_patch()
                    ↓
              DiffWebsocketSession
                    ↓
          客户端 (snapshot.klines 更新)
                    ↓
              HQChart 实时显示
```

---

## 2. 启动系统

### 2.1 启动后端服务

```bash
cd /home/quantaxis/qaexchange-rs
cargo run --bin qaexchange-server
```

**预期输出**：
```
📊 [KLineActor] Starting K-line aggregator...
📊 [KLineActor] WAL recovery completed: 0 K-lines recovered, 0 errors
📊 [KLineActor] Subscribed to tick events (subscriber_id=...)
📊 [KLineActor] Started successfully
[INFO] HTTP Server running at http://0.0.0.0:8094
[INFO] WebSocket Server running at ws://0.0.0.0:8001
```

### 2.2 启动前端服务

```bash
cd /home/quantaxis/qaexchange-rs/web
npm run serve
# 或
./start_dev.sh
```

**访问地址**：
- 主页：http://localhost:8080
- K线页面：http://localhost:8080/chart
- WebSocket测试页：http://localhost:8080/websocket-test

---

## 3. 功能测试

### 3.1 WebSocket 连接测试

**步骤**：
1. 访问 http://localhost:8080/chart
2. 点击"连接"按钮
3. 查看连接状态标签

**预期结果**：
- 标签变为绿色"WebSocket 已连接"
- 浏览器控制台输出：
  ```
  [WebSocketManager] WebSocket connected
  [ChartPage] Subscribing K-line: SHFE.cu2501 period: 5
  ```

### 3.2 K线订阅测试

**步骤**：
1. 连接成功后，在合约下拉框选择 `SHFE.cu2501`
2. 在周期下拉框选择 `5分钟`
3. 观察控制台和图表

**预期结果**：
- 浏览器控制台：
  ```
  [WebSocket] Setting chart: {chart_id: "chart_page", ins_list: "SHFE.cu2501", duration: 300000000000, view_width: 500}
  ```
- 后端日志：
  ```
  📊 [DIFF] User xxx set chart chart_page: instrument=SHFE.cu2501, period=Min5, bars=0
  ```
- K线数量显示：`K线数量: 0 条`（初始无数据）

### 3.3 成交数据生成测试

**方式一：通过前端下单**

1. 访问 http://localhost:8080/websocket-test
2. 订阅合约 `SHFE.cu2501`
3. 在下单面板输入：
   - 合约：SHFE.cu2501
   - 方向：BUY
   - 开平：OPEN
   - 价格：50000
   - 数量：1
4. 点击"提交订单"

**方式二：使用 HTTP API**

```bash
# 下买单
curl -X POST http://localhost:8094/api/order/submit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "test_user",
    "instrument_id": "SHFE.cu2501",
    "direction": "BUY",
    "offset": "OPEN",
    "volume": 1,
    "price_type": "LIMIT",
    "limit_price": 50000
  }'

# 下卖单（触发成交）
curl -X POST http://localhost:8094/api/order/submit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "test_user2",
    "instrument_id": "SHFE.cu2501",
    "direction": "SELL",
    "offset": "OPEN",
    "volume": 1,
    "price_type": "LIMIT",
    "limit_price": 50000
  }'
```

**预期后端日志**：
```
[INFO] Trade executed: SHFE.cu2501 @ 50000.0, volume: 1
📊 [MarketDataBroadcaster] Broadcasting tick: SHFE.cu2501
📊 [KLineActor] Processing tick: SHFE.cu2501 price=50000.0 volume=1
```

### 3.4 K线聚合测试

**触发K线完成**：

为了快速看到K线效果，建议：
1. 修改 `src/market/kline.rs` 中的周期为 **3秒**（Sec3），而不是5分钟
2. 或者连续下单多次，等待5分钟

**预期后端日志**（K线完成时）：
```
📊 [KLineActor] Finished SHFE.cu2501 Min5 K-line: O=50000.00 H=50100.00 L=49900.00 C=50050.00 V=10
📊 [KLineActor] K-line persisted to WAL: SHFE.cu2501 Min5
```

### 3.5 WebSocket 推送测试

**观察前端更新**：

成交后，观察 K线页面：
- **浏览器控制台**：
  ```
  [ChartPage] K-line data updated: 1 bars
  ```
- **页面显示**：K线数量从 0 变为 1
- **HQChart**：显示新的K线柱

**验证DIFF消息格式**（浏览器控制台 → Network → WS）：
```json
{
  "aid": "rtn_data",
  "data": [{
    "klines": {
      "SHFE.cu2501": {
        "300000000000": {
          "data": {
            "123456": {
              "datetime": 1696723200000000000,
              "open": 50000.0,
              "high": 50100.0,
              "low": 49900.0,
              "close": 50050.0,
              "volume": 10,
              "open_oi": 0,
              "close_oi": 0
            }
          }
        }
      }
    }
  }]
}
```

---

## 4. 性能测试

### 4.1 K线聚合性能

**压测脚本**（10,000笔成交/秒）：

```bash
cargo run --example stress_test -- --orders 10000 --instrument SHFE.cu2501
```

**预期指标**：
- K线聚合延迟：P99 < 100μs
- WAL 写入延迟：P99 < 50ms
- 内存使用：< 100MB（10,000根K线）

### 4.2 WebSocket 推送性能

**测试并发连接**（100个客户端）：

```javascript
// browser_stress_test.js
const clients = []
for (let i = 0; i < 100; i++) {
  const ws = new WebSocket('ws://localhost:8001/ws/diff?user_id=user' + i)
  clients.push(ws)
}
```

**预期指标**：
- 推送延迟：< 1ms
- CPU 使用：< 50%
- 内存使用：< 500MB

---

## 5. 故障测试

### 5.1 WAL 恢复测试

**步骤**：
1. 正常运行系统，生成K线数据
2. 停止服务（Ctrl+C）
3. 重新启动服务
4. 检查日志

**预期日志**：
```
📊 [KLineActor] Recovering K-line data from WAL...
📊 [KLineActor] WAL recovery completed: 100 K-lines recovered, 0 errors
```

### 5.2 WebSocket 断线重连测试

**步骤**：
1. 前端连接成功后，停止后端服务
2. 观察前端连接状态
3. 重新启动后端
4. 观察前端自动重连

**预期结果**：
- 断线时：标签变红"WebSocket 未连接"
- 重连成功后：自动恢复K线订阅

---

## 6. 数据验证

### 6.1 K线数据完整性

**检查点**：
1. OHLC 合理性：`Low <= Open, Close <= High`
2. 时间连续性：K线时间戳按周期递增
3. 成交量准确性：`Volume` 应等于该周期内所有成交量之和

**SQL 查询**（未来Parquet存储）：
```sql
SELECT
  instrument_id,
  period,
  COUNT(*) as kline_count,
  MIN(timestamp) as start_time,
  MAX(timestamp) as end_time
FROM klines
GROUP BY instrument_id, period;
```

### 6.2 DIFF 协议合规性

**验证字段**：
- ✅ `datetime` 为纳秒时间戳
- ✅ `open_oi` 和 `close_oi` 存在（期货特有）
- ✅ `volume` 和 `amount` 一致

---

## 7. 常见问题

### Q1: K线不显示

**检查清单**：
1. WebSocket 是否连接？
2. 是否订阅了正确的合约？
3. 后端是否有成交数据？
4. 浏览器控制台是否有错误？

**调试命令**：
```bash
# 检查 MarketDataBroadcaster 订阅者
curl http://localhost:8094/api/admin/market/subscribers

# 检查K线Actor状态
curl http://localhost:8094/api/market/kline/SHFE.cu2501?period=5&count=10
```

### Q2: K线数据不更新

**原因**：
- KLineActor 没有订阅 tick 频道
- MarketDataBroadcaster 没有广播 tick 事件

**验证**：
查看后端日志是否有：
```
📊 [KLineActor] Subscribed to tick events (subscriber_id=...)
```

### Q3: 前端收到数据但不显示

**检查**：
1. `snapshot.klines` 结构是否正确
2. `periodToNs()` 转换是否匹配
3. HQChart 组件是否正常初始化

---

## 8. 下一步优化

**建议**：
1. 添加 Prometheus 指标导出
2. 实现 K线缓存（Redis）
3. 支持更多周期（Week/Month）
4. 实现 K线合并优化（减少 WebSocket 消息量）

---

**测试完成标准**：
- [x] WebSocket 连接成功
- [x] 订阅K线成功
- [x] 成交后K线聚合
- [x] WebSocket 实时推送
- [x] 前端HQChart显示
- [x] WAL 持久化和恢复
- [ ] 压力测试（10K并发）
- [ ] 故障恢复测试

---

**@yutiansut @quantaxis**
