# QAExchange 常见问题 (FAQ)

本文档收集整理了 QAExchange 系统使用过程中的常见问题及解决方案。

---

## 📖 目录

- [安装和编译](#安装和编译)
- [配置问题](#配置问题)
- [运行问题](#运行问题)
- [交易问题](#交易问题)
- [WebSocket 连接](#websocket-连接)
- [性能问题](#性能问题)
- [数据和存储](#数据和存储)
- [故障排查](#故障排查)
- [开发问题](#开发问题)

---

## 安装和编译

### Q1: 编译时报错 "failed to compile qaexchange"

**症状**:
```
error: failed to run custom build command for `qaexchange`
```

**原因**:
1. Rust 版本过低
2. qars 依赖未找到
3. 系统库缺失

**解决方案**:

**检查 Rust 版本**:
```bash
rustc --version
# 需要 1.91.0-nightly 或更高版本
rustup update nightly
rustup default nightly
```

**检查 qars 依赖**:
```bash
# 确保 qars2 在正确位置
ls ../qars2/
# 应该看到 Cargo.toml 和 src/ 目录

# 如果没有，clone qars
git clone https://github.com/QUANTAXIS/qars ../qars2
```

**安装系统依赖 (Ubuntu/Debian)**:
```bash
sudo apt-get update
sudo apt-get install build-essential pkg-config libssl-dev
```

**安装系统依赖 (macOS)**:
```bash
brew install openssl pkg-config
```

---

### Q2: 编译时提示 "can't find crate for qars"

**症状**:
```
error[E0463]: can't find crate for `qars`
```

**原因**: `qars` 依赖路径配置错误

**解决方案**:

检查 `Cargo.toml` 中的路径:
```toml
[dependencies]
qars = { path = "../qars2" }
```

确保路径正确:
```bash
# 从 qaexchange-rs 目录执行
cd ..
ls -d qars2
cd qaexchange-rs
```

如果路径不对，修改 `Cargo.toml`:
```toml
qars = { path = "/absolute/path/to/qars2" }
```

---

### Q3: 编译警告 "unused variable" 或 "dead code"

**症状**:
```
warning: unused variable: `xxx`
warning: function is never used: `yyy`
```

**原因**: 开发过程中的正常警告

**解决方案**:

**忽略警告编译**:
```bash
cargo build --lib 2>&1 | grep -v warning
```

**消除特定警告**:
```rust
#[allow(dead_code)]
fn unused_function() { ... }

#[allow(unused_variables)]
let unused_var = 42;
```

---

### Q4: iceoryx2 编译失败

**症状**:
```
error: failed to compile `iceoryx2`
```

**原因**: iceoryx2 是可选功能，可能环境不支持

**解决方案**:

**禁用 iceoryx2**:
```bash
# 编译时禁用 iceoryx2 feature
cargo build --lib --no-default-features
```

**修改 Cargo.toml**:
```toml
[features]
default = []  # 移除 "iceoryx2"
iceoryx2 = ["dep:iceoryx2"]
```

---

## 配置问题

### Q5: 启动时报错 "config file not found"

**症状**:
```
Error: Config file not found: config/exchange.toml
```

**原因**: 配置文件缺失或路径错误

**解决方案**:

**检查配置文件**:
```bash
ls config/
# 应该看到 exchange.toml 和 instruments.toml
```

**如果缺失，创建默认配置**:

`config/exchange.toml`:
```toml
[exchange]
name = "QAExchange"
trading_hours = "09:00-15:00"
settlement_time = "15:30"

[risk]
margin_ratio = 0.1
force_close_threshold = 1.0

[server]
http_host = "127.0.0.1"
http_port = 8000
ws_host = "127.0.0.1"
ws_port = 8001
```

`config/instruments.toml`:
```toml
[[instruments]]
instrument_id = "SHFE.cu2501"
exchange_id = "SHFE"
product_id = "cu"
price_tick = 10.0
volume_multiple = 5
margin_ratio = 0.1
commission = 0.0005
```

---

### Q6: 合约配置无效

**症状**: 下单时提示 "instrument not found"

**原因**: 合约未注册或配置格式错误

**解决方案**:

**检查 instruments.toml 格式**:
```toml
[[instruments]]  # 注意双中括号
instrument_id = "SHFE.cu2501"  # 必须包含交易所前缀
exchange_id = "SHFE"  # 大写
product_id = "cu"  # 小写
price_tick = 10.0  # 浮点数
volume_multiple = 5  # 整数
```

**运行时注册合约**:
```bash
curl -X POST http://localhost:8000/api/admin/instrument/create \
  -H "Content-Type: application/json" \
  -d '{
    "instrument_id": "SHFE.cu2501",
    "exchange_id": "SHFE",
    "product_id": "cu",
    "price_tick": 10.0,
    "volume_multiple": 5,
    "margin_ratio": 0.1,
    "commission": 0.0005
  }'
```

**查询已注册合约**:
```bash
curl http://localhost:8000/api/admin/instruments
```

---

### Q7: 日志级别设置无效

**症状**: 设置 `RUST_LOG=debug` 后仍然只看到 INFO 日志

**原因**: 环境变量设置方式错误

**解决方案**:

**正确设置日志级别**:
```bash
# Linux/macOS
export RUST_LOG=qaexchange=debug
cargo run --bin qaexchange-server

# 或者临时设置
RUST_LOG=qaexchange=debug cargo run --bin qaexchange-server

# Windows (PowerShell)
$env:RUST_LOG="qaexchange=debug"
cargo run --bin qaexchange-server
```

**按模块设置日志**:
```bash
# 只显示 matching 模块的 DEBUG 日志
RUST_LOG=qaexchange::matching=debug

# 多模块设置
RUST_LOG=qaexchange::matching=debug,qaexchange::storage=info
```

**在代码中设置**:
```rust
// src/main.rs
env_logger::Builder::from_default_env()
    .filter_level(log::LevelFilter::Debug)
    .init();
```

---

## 运行问题

### Q8: 启动时报错 "Address already in use"

**症状**:
```
Error: Address already in use (os error 98)
```

**原因**: 端口已被占用

**解决方案**:

**查找占用端口的进程**:
```bash
# Linux/macOS
lsof -i :8000
lsof -i :8001

# 杀死进程
kill -9 <PID>
```

**修改端口配置**:

`config/exchange.toml`:
```toml
[server]
http_port = 8002  # 改为其他端口
ws_port = 8003
```

**或者使用环境变量**:
```bash
HTTP_PORT=8002 WS_PORT=8003 cargo run --bin qaexchange-server
```

---

### Q9: 启动后无法访问 HTTP API

**症状**: `curl http://localhost:8000/health` 返回 "Connection refused"

**原因**:
1. 服务未启动成功
2. 端口配置错误
3. 防火墙拦截

**解决方案**:

**检查服务是否运行**:
```bash
# 查看进程
ps aux | grep qaexchange

# 查看日志
tail -f logs/qaexchange.log
```

**检查端口监听**:
```bash
# Linux/macOS
netstat -an | grep 8000

# 应该看到:
# tcp  0  0  127.0.0.1:8000  0.0.0.0:*  LISTEN
```

**检查防火墙**:
```bash
# Ubuntu
sudo ufw status
sudo ufw allow 8000

# CentOS
sudo firewall-cmd --add-port=8000/tcp --permanent
sudo firewall-cmd --reload
```

**使用 0.0.0.0 监听所有接口**:
```toml
[server]
http_host = "0.0.0.0"  # 允许外部访问
http_port = 8000
```

---

### Q10: 运行一段时间后崩溃

**症状**: 服务运行几小时后自动退出

**原因**:
1. 内存溢出 (OOM)
2. Panic 未捕获
3. 磁盘空间不足

**解决方案**:

**检查内存使用**:
```bash
# 监控内存
top -p $(pgrep qaexchange)

# 查看 OOM 日志
dmesg | grep -i "out of memory"
sudo grep -i "killed process" /var/log/syslog
```

**启用崩溃日志**:
```rust
// src/main.rs
use std::panic;

fn main() {
    panic::set_hook(Box::new(|panic_info| {
        log::error!("Panic occurred: {:?}", panic_info);
    }));

    // ... your code
}
```

**检查磁盘空间**:
```bash
df -h
# 确保有足够空间用于 WAL 和 SSTable
```

**限制 WAL 和 SSTable 大小**:

参见 [Q26: WAL 文件过大](#q26-wal-文件过大)

---

### Q11: 如何后台运行服务

**问题**: 关闭终端后服务停止

**解决方案**:

**使用 nohup**:
```bash
nohup cargo run --bin qaexchange-server > logs/server.log 2>&1 &
```

**使用 systemd (推荐生产环境)**:

创建 `/etc/systemd/system/qaexchange.service`:
```ini
[Unit]
Description=QAExchange Trading System
After=network.target

[Service]
Type=simple
User=quantaxis
WorkingDirectory=/home/quantaxis/qaexchange-rs
ExecStart=/home/quantaxis/.cargo/bin/cargo run --bin qaexchange-server --release
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

启用服务:
```bash
sudo systemctl daemon-reload
sudo systemctl enable qaexchange
sudo systemctl start qaexchange
sudo systemctl status qaexchange
```

查看日志:
```bash
sudo journalctl -u qaexchange -f
```

---

## 交易问题

### Q12: 下单失败 "insufficient funds"

**症状**: 下单返回错误 "Insufficient funds"

**原因**:
1. 账户可用资金不足
2. 保证金计算错误
3. 账户未入金

**解决方案**:

**查询账户信息**:
```bash
curl http://localhost:8000/api/account/user123
```

检查返回的 `available` 字段:
```json
{
  "user_id": "user123",
  "balance": 100000,
  "available": 50000,  # 可用资金
  "margin": 50000
}
```

**计算所需保证金**:
```
保证金 = 价格 × 数量 × 合约乘数 × 保证金率
例: 50000 × 1 × 5 × 0.1 = 25000 元
```

**入金**:
```bash
curl -X POST http://localhost:8000/api/management/deposit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user123",
    "amount": 100000
  }'
```

---

### Q13: 订单状态一直是 PENDING

**症状**: 下单后订单状态不更新

**原因**:
1. 撮合引擎未运行
2. 合约未注册
3. 价格超出涨跌停板

**解决方案**:

**检查撮合引擎**:
```bash
# 查看日志
grep "matching engine" logs/qaexchange.log
```

**检查订单详情**:
```bash
curl http://localhost:8000/api/order/order123
```

**检查合约状态**:
```bash
curl http://localhost:8000/api/admin/instruments | grep "SHFE.cu2501"
```

确保合约状态为 `TRADING`:
```json
{
  "instrument_id": "SHFE.cu2501",
  "status": "TRADING"  # 不是 SUSPENDED
}
```

**手动触发撮合**:

如果是测试环境，可以提交反向订单:
```bash
# 原订单: BUY 1手 @ 50000
# 提交: SELL 1手 @ 50000
curl -X POST http://localhost:8000/api/order/submit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user456",
    "order_id": "order456",
    "instrument_id": "SHFE.cu2501",
    "direction": "SELL",
    "offset": "OPEN",
    "volume": 1,
    "price_type": "LIMIT",
    "limit_price": 50000
  }'
```

---

### Q14: 撤单失败 "order not found"

**症状**: 撤单时提示订单不存在

**原因**:
1. 订单ID错误
2. 订单已成交
3. 订单已撤销

**解决方案**:

**查询订单状态**:
```bash
curl http://localhost:8000/api/order/order123
```

检查订单状态:
```json
{
  "order_id": "order123",
  "status": "FILLED"  # 已成交，无法撤单
}
```

**可撤单状态**:
- `ACCEPTED`: 已接受，未成交
- `PARTIAL_FILLED`: 部分成交

**不可撤单状态**:
- `FILLED`: 已完全成交
- `CANCELLED`: 已撤销
- `REJECTED`: 已拒绝

**正确撤单**:
```bash
curl -X POST http://localhost:8000/api/order/cancel \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user123",
    "order_id": "order123"  # 确保 order_id 正确
  }'
```

---

### Q15: 持仓浮动盈亏计算不对

**症状**: 查询持仓时 `float_profit` 值不正确

**原因**:
1. 最新价未更新
2. 开仓价计算错误
3. 合约乘数配置错误

**解决方案**:

**手动计算验证**:
```
多头浮动盈亏 = (最新价 - 开仓价) × 持仓量 × 合约乘数
例: (50500 - 50000) × 2 × 5 = 5000 元

空头浮动盈亏 = (开仓价 - 最新价) × 持仓量 × 合约乘数
例: (50000 - 49500) × 2 × 5 = 5000 元
```

**查询持仓详情**:
```bash
curl http://localhost:8000/api/position/user123
```

检查关键字段:
```json
{
  "instrument_id": "SHFE.cu2501",
  "volume_long": 2,
  "open_price_long": 50000,
  "last_price": 50500,  # 最新价
  "float_profit": 5000,  # 应该等于 (50500-50000)*2*5
  "volume_multiple": 5
}
```

**触发价格更新**:

提交成交单更新 `last_price`:
```bash
# 任意成交都会更新 last_price
```

---

### Q16: 强制平仓没有触发

**症状**: 账户风险度 > 100% 但未强平

**原因**:
1. 日终结算未执行
2. 强平阈值配置过高
3. 强平逻辑未实现

**解决方案**:

**检查风险度**:
```bash
curl http://localhost:8000/api/account/user123
```

```json
{
  "balance": 20000,
  "margin": 25000,
  "risk_ratio": 1.25  # 125% > 100%
}
```

**手动触发结算**:
```bash
# 先设置结算价
curl -X POST http://localhost:8000/api/admin/settlement/set-price \
  -H "Content-Type: application/json" \
  -d '{
    "instrument_id": "SHFE.cu2501",
    "settlement_price": 50000
  }'

# 执行日终结算
curl -X POST http://localhost:8000/api/admin/settlement/execute
```

**检查强平配置**:

`config/exchange.toml`:
```toml
[risk]
force_close_threshold = 1.0  # 100%
```

**查看强平日志**:
```bash
grep "Force closing" logs/qaexchange.log
```

---

## WebSocket 连接

### Q17: WebSocket 连接失败

**症状**: 前端无法连接 WebSocket

**原因**:
1. WebSocket 服务未启动
2. 端口配置错误
3. CORS 问题

**解决方案**:

**测试 WebSocket 连接**:

使用 `websocat` (推荐):
```bash
# 安装
cargo install websocat

# 连接
websocat ws://localhost:8001/ws?user_id=user123
```

或使用 JavaScript 测试:
```javascript
const ws = new WebSocket('ws://localhost:8001/ws?user_id=user123');

ws.onopen = () => {
  console.log('Connected');
  ws.send(JSON.stringify({aid: 'peek_message'}));
};

ws.onmessage = (event) => {
  console.log('Received:', event.data);
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};
```

**检查 WebSocket 服务**:
```bash
netstat -an | grep 8001
```

**配置 CORS**:

如果跨域访问，需要配置 CORS:
```rust
// src/service/websocket/mod.rs
use actix_cors::Cors;

HttpServer::new(|| {
    App::new()
        .wrap(
            Cors::default()
                .allow_any_origin()
                .allow_any_method()
                .allow_any_header()
        )
        .route("/ws", web::get().to(ws_handler))
})
```

---

### Q18: WebSocket 连接频繁断开

**症状**: WebSocket 每隔 10 秒断开

**原因**: 心跳超时

**解决方案**:

**客户端实现心跳**:
```javascript
const ws = new WebSocket('ws://localhost:8001/ws?user_id=user123');

// 每 5 秒发送 ping
setInterval(() => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({aid: 'ping'}));
  }
}, 5000);

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.aid === 'pong') {
    console.log('Heartbeat OK');
  }
};
```

**自动重连**:
```javascript
function connectWebSocket() {
  const ws = new WebSocket('ws://localhost:8001/ws?user_id=user123');

  ws.onclose = () => {
    console.log('Connection closed, reconnecting...');
    setTimeout(connectWebSocket, 1000);
  };

  return ws;
}

const ws = connectWebSocket();
```

---

### Q19: WebSocket 消息延迟高

**症状**: 成交后 1-2 秒才收到通知

**原因**:
1. 未发送 `peek_message`
2. 通知队列积压
3. 网络延迟

**解决方案**:

**正确实现 peek_message 机制**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.aid === 'rtn_data') {
    // 处理数据更新
    processUpdate(msg.data);

    // 立即发送下一个 peek_message
    ws.send(JSON.stringify({aid: 'peek_message'}));
  }
};

// 连接后立即发送第一个 peek_message
ws.onopen = () => {
  ws.send(JSON.stringify({aid: 'peek_message'}));
};
```

**检查通知队列**:
```bash
# 查看日志
grep "notification queue" logs/qaexchange.log

# 应该看到类似:
# [INFO] notification queue size: 5 (< 500 threshold)
```

**监控延迟**:
```javascript
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  const now = Date.now();
  const latency = now - msg.timestamp;
  console.log('Notification latency:', latency, 'ms');
};
```

---

### Q20: WebSocket 收不到成交通知

**症状**: 订单成交但 WebSocket 未收到 Trade 通知

**原因**:
1. 未订阅通知频道
2. user_id 不匹配
3. NotificationGateway 未连接

**解决方案**:

**检查 WebSocket 连接参数**:
```javascript
// 确保 user_id 与下单时的 user_id 一致
const ws = new WebSocket('ws://localhost:8001/ws?user_id=user123');
```

**订阅交易频道**:
```javascript
ws.onopen = () => {
  // 订阅全部频道
  ws.send(JSON.stringify({
    aid: 'subscribe',
    channels: ['trade', 'account', 'position', 'order']
  }));

  ws.send(JSON.stringify({aid: 'peek_message'}));
};
```

**检查通知是否发送**:
```bash
# 查看日志
grep "Sending Trade notification" logs/qaexchange.log
```

**手动查询成交记录**:
```bash
curl http://localhost:8000/api/order/user/user123
```

---

## 性能问题

### Q21: 订单吞吐量低于预期

**症状**: 只能达到 10K orders/s，远低于 100K 目标

**原因**:
1. 单线程提交
2. HTTP 连接复用不足
3. 预交易检查耗时过多

**解决方案**:

**并发提交订单**:
```rust
use tokio::task;

let mut handles = vec![];
for i in 0..1000 {
    let handle = task::spawn(async move {
        submit_order(order).await
    });
    handles.push(handle);
}

for handle in handles {
    handle.await.unwrap();
}
```

**使用连接池**:
```rust
use reqwest::Client;

let client = Client::builder()
    .pool_max_idle_per_host(100)  // 连接池大小
    .build()?;
```

**禁用预交易检查（测试环境）**:
```rust
// src/exchange/order_router.rs
pub async fn submit_order_fast(&self, order: QAOrder) -> Result<String> {
    // 跳过 pre_trade_check
    self.matching_engine.submit_order(order).await
}
```

**压测示例**:
```bash
# 使用 Apache Bench
ab -n 100000 -c 100 -p order.json -T application/json \
   http://localhost:8000/api/order/submit
```

---

### Q22: 撮合延迟过高

**症状**: P99 延迟 > 1ms，远高于目标 100μs

**原因**:
1. 锁竞争
2. Orderbook 实现效率低
3. Debug 模式运行

**解决方案**:

**使用 Release 模式**:
```bash
cargo build --release
cargo run --release --bin qaexchange-server

# 性能提升 10-100x
```

**检查是否使用 qars Orderbook**:
```rust
// src/matching/engine.rs
use qars::qamarket::matchengine::Orderbook;  // ✓ 正确

// 不要自己实现 Orderbook
```

**减少锁粒度**:
```rust
// 不好: 长时间持有锁
let mut orderbook = self.orderbook.write();
orderbook.submit_order(order);
orderbook.process();
drop(orderbook);

// 好: 尽快释放锁
{
    let mut orderbook = self.orderbook.write();
    orderbook.submit_order(order);
}  // 锁在此处自动释放

self.process_trades();
```

**性能分析**:
```bash
# 使用 flamegraph
cargo install flamegraph
sudo flamegraph target/release/qaexchange-server

# 查看 flamegraph.svg 找出热点
```

---

### Q23: 内存占用过高

**症状**: 运行一段时间后内存占用超过 10GB

**原因**:
1. MemTable 未及时 Flush
2. 通知队列积压
3. 订单/成交记录未清理

**解决方案**:

**配置 MemTable 自动 Flush**:
```rust
// src/storage/memtable/oltp.rs
pub const MEMTABLE_FLUSH_SIZE: usize = 64 * 1024 * 1024;  // 64 MB
pub const MEMTABLE_FLUSH_INTERVAL: Duration = Duration::from_secs(300);  // 5 分钟
```

**清理历史订单**:
```rust
// 定期清理已完成订单
pub fn cleanup_old_orders(&self, before: i64) {
    self.orders.retain(|_, order| {
        order.timestamp > before
    });
}
```

**监控内存**:
```bash
# 实时监控
watch -n 1 'ps aux | grep qaexchange | grep -v grep'

# 内存分析
cargo install valgrind
valgrind --tool=massif target/release/qaexchange-server
```

**限制通知队列大小**:

已实现背压控制，参见 `NotificationGateway::BACKPRESSURE_THRESHOLD = 500`

---

### Q24: CPU 使用率过高

**症状**: CPU 占用持续 100%

**原因**:
1. 忙等待循环
2. 无限重试
3. 日志输出过多

**解决方案**:

**避免忙等待**:
```rust
// 不好: 忙等待
loop {
    if condition {
        break;
    }
}

// 好: 使用 sleep
loop {
    if condition {
        break;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
}
```

**减少日志输出**:
```bash
# 只记录 WARN 及以上级别
RUST_LOG=qaexchange=warn cargo run --release
```

**检查无限循环**:
```bash
# 使用 perf 分析 CPU 热点
sudo perf record -g target/release/qaexchange-server
sudo perf report
```

---

## 数据和存储

### Q25: 数据恢复失败

**症状**: 重启后账户数据丢失

**原因**:
1. WAL 文件损坏
2. WAL 回放失败
3. 未调用 Checkpoint

**解决方案**:

**检查 WAL 文件**:
```bash
ls -lh data/wal/
# 确保有 .wal 文件
```

**手动回放 WAL**:
```bash
# 启用详细日志
RUST_LOG=qaexchange::storage=debug cargo run --bin qaexchange-server

# 查看回放过程
grep "Replaying WAL" logs/qaexchange.log
```

**验证 WAL 完整性**:
```rust
// 检查 CRC32 校验
pub fn verify_wal(&self, file_path: &str) -> Result<bool> {
    // ... CRC 验证逻辑
}
```

**定期 Checkpoint**:
```rust
// 每小时创建一次 Checkpoint
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        checkpoint_manager.create_checkpoint().await;
    }
});
```

---

### Q26: WAL 文件过大

**症状**: `data/wal/` 目录占用超过 100GB

**原因**:
1. 未清理旧 WAL
2. Checkpoint 未及时创建
3. 高频交易写入

**解决方案**:

**配置 WAL 清理策略**:
```rust
// src/storage/wal/manager.rs
pub const WAL_RETENTION_DAYS: u64 = 7;  // 保留 7 天

pub fn cleanup_old_wal(&self) {
    let cutoff = Utc::now() - Duration::days(WAL_RETENTION_DAYS);
    // ... 删除旧文件
}
```

**手动清理**:
```bash
# 查看 WAL 文件大小
du -sh data/wal/

# 删除 7 天前的 WAL
find data/wal/ -name "*.wal" -mtime +7 -delete
```

**WAL 压缩**:
```bash
# 压缩旧 WAL
gzip data/wal/*.wal.old
```

**创建 Checkpoint 后清理**:
```rust
pub fn create_checkpoint(&self) -> Result<()> {
    // 1. 创建 Checkpoint
    self.create_snapshot()?;

    // 2. 清理已 Checkpoint 的 WAL
    self.cleanup_wal_before(checkpoint_lsn)?;

    Ok(())
}
```

---

### Q27: SSTable 查询慢

**症状**: 查询历史数据耗时 > 1 秒

**原因**:
1. 未使用 Bloom Filter
2. SSTable 文件过多
3. 未使用 mmap

**解决方案**:

**启用 Bloom Filter**:
```rust
// src/storage/sstable/oltp_rkyv.rs
pub fn build_with_bloom_filter(&self) -> Result<()> {
    let bloom = BloomFilter::new(10000, 0.01);  // 1% FP rate
    // ... 构建 Bloom Filter
}
```

**触发 Compaction**:
```bash
# 查看 SSTable 文件数
ls data/sstable/ | wc -l

# 如果 > 100，手动触发 Compaction
curl -X POST http://localhost:8000/api/admin/compaction/trigger
```

**启用 mmap**:
```rust
// src/storage/sstable/mmap_reader.rs
pub fn open_with_mmap(path: &Path) -> Result<Self> {
    let file = File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    // ... 零拷贝读取
}
```

**查询优化**:
```rust
// 使用 Polars LazyFrame
let df = LazyFrame::scan_parquet(path)?
    .filter(col("timestamp").gt(start_time))
    .select(&[col("order_id"), col("volume")])
    .limit(100)
    .collect()?;
```

---

### Q28: Parquet 文件损坏

**症状**: 读取 OLAP 数据时报错 "Invalid Parquet file"

**原因**:
1. 写入中途崩溃
2. 磁盘错误
3. 格式不兼容

**解决方案**:

**检查文件完整性**:
```bash
# 使用 parquet-tools
pip install parquet-tools
parquet-tools inspect data/sstable/olap/xxx.parquet
```

**删除损坏文件**:
```bash
# 备份
cp data/sstable/olap/xxx.parquet data/backup/

# 删除
rm data/sstable/olap/xxx.parquet

# 从 WAL 重建
cargo run --bin recover-from-wal
```

**启用写入校验**:
```rust
use parquet::file::properties::WriterProperties;

let props = WriterProperties::builder()
    .set_compression(Compression::SNAPPY)
    .set_write_batch_size(1024)
    .build();
```

---

## 故障排查

### Q29: 如何查看系统运行状态

**解决方案**:

**健康检查**:
```bash
curl http://localhost:8000/health
```

**系统监控**:
```bash
curl http://localhost:8000/api/monitoring/system
```

返回:
```json
{
  "accounts_count": 100,
  "orders_count": 1500,
  "trades_count": 500,
  "ws_connections": 50,
  "memory_usage_mb": 512,
  "cpu_usage_percent": 25.5,
  "uptime_seconds": 3600
}
```

**存储监控**:
```bash
curl http://localhost:8000/api/monitoring/storage
```

返回:
```json
{
  "wal_size_mb": 128,
  "memtable_size_mb": 32,
  "sstable_count": 15,
  "sstable_total_size_mb": 1024
}
```

---

### Q30: 如何开启调试日志

**解决方案**:

**环境变量**:
```bash
# 全局 DEBUG
RUST_LOG=debug cargo run

# 仅特定模块
RUST_LOG=qaexchange::matching=debug,qaexchange::storage=trace

# 包含依赖库
RUST_LOG=debug,actix_web=info
```

**代码中设置**:
```rust
// src/main.rs
env_logger::Builder::from_default_env()
    .filter_module("qaexchange::matching", log::LevelFilter::Trace)
    .filter_module("qaexchange::storage", log::LevelFilter::Debug)
    .init();
```

**日志格式**:
```rust
env_logger::Builder::from_default_env()
    .format(|buf, record| {
        writeln!(
            buf,
            "[{} {} {}:{}] {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.args()
        )
    })
    .init();
```

---

### Q31: 如何抓取 WebSocket 通信内容

**解决方案**:

**浏览器开发者工具**:
1. 打开 Chrome DevTools (F12)
2. 切换到 Network 标签
3. 筛选 WS (WebSocket)
4. 查看 Messages 标签

**使用 Wireshark**:
```bash
# 安装 Wireshark
sudo apt-get install wireshark

# 捕获本地回环
sudo wireshark -i lo -f "tcp port 8001"
```

**代码中记录**:
```rust
// src/service/websocket/session.rs
fn handle_text(&mut self, text: &str, ctx: &mut Self::Context) {
    log::debug!("WS Received: {}", text);  // 记录接收

    let response = process_message(text);

    log::debug!("WS Sending: {}", response);  // 记录发送
    ctx.text(response);
}
```

---

### Q32: 性能分析工具推荐

**解决方案**:

**CPU 分析**:
```bash
# flamegraph
cargo install flamegraph
sudo flamegraph --bin qaexchange-server

# perf (Linux)
sudo perf record -g target/release/qaexchange-server
sudo perf report
```

**内存分析**:
```bash
# valgrind
valgrind --tool=massif target/release/qaexchange-server
ms_print massif.out.<pid>

# heaptrack (更快)
heaptrack target/release/qaexchange-server
heaptrack_gui heaptrack.qaexchange-server.<pid>.gz
```

**异步任务分析**:
```bash
# tokio-console
cargo install tokio-console

# 代码中启用
#[tokio::main]
async fn main() {
    console_subscriber::init();
    // ...
}

# 运行 console
tokio-console
```

**网络分析**:
```bash
# tcpdump
sudo tcpdump -i lo port 8000 -w capture.pcap

# 分析
wireshark capture.pcap
```

---

## 开发问题

### Q33: 如何运行单元测试

**解决方案**:

**运行所有测试**:
```bash
cargo test
```

**运行特定模块测试**:
```bash
cargo test --lib matching
cargo test --lib storage
```

**运行单个测试**:
```bash
cargo test test_submit_order
```

**显示测试输出**:
```bash
cargo test -- --nocapture
```

**并行测试**:
```bash
# 默认并行
cargo test

# 单线程运行（避免资源竞争）
cargo test -- --test-threads=1
```

**测试覆盖率**:
```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --out Html
```

---

### Q34: 如何添加新的 HTTP 端点

**解决方案**:

**1. 定义请求/响应模型**:

`src/service/http/models.rs`:
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct MyRequest {
    pub user_id: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MyResponse {
    pub result: String,
}
```

**2. 实现处理函数**:

`src/service/http/handlers.rs`:
```rust
pub async fn my_handler(
    req: web::Json<MyRequest>,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    // 业务逻辑
    let result = process_request(&req.user_id, &req.data)?;

    Ok(HttpResponse::Ok().json(MyResponse {
        result: result.to_string(),
    }))
}
```

**3. 注册路由**:

`src/service/http/routes.rs`:
```rust
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/api/my-endpoint")
            .route(web::post().to(my_handler))
    );
}
```

**4. 测试**:
```bash
curl -X POST http://localhost:8000/api/my-endpoint \
  -H "Content-Type: application/json" \
  -d '{"user_id": "user123", "data": "test"}'
```

---

### Q35: 如何扩展 WebSocket 消息类型

**解决方案**:

**1. 定义新消息类型**:

`src/service/websocket/messages.rs`:
```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "aid")]
pub enum ClientMessage {
    #[serde(rename = "peek_message")]
    PeekMessage,

    #[serde(rename = "my_new_message")]
    MyNewMessage {
        param1: String,
        param2: i64,
    },
}
```

**2. 实现处理逻辑**:

`src/service/websocket/session.rs`:
```rust
fn handle_client_message(&mut self, msg: ClientMessage, ctx: &mut Self::Context) {
    match msg {
        ClientMessage::PeekMessage => {
            // 现有逻辑
        }
        ClientMessage::MyNewMessage { param1, param2 } => {
            // 新消息处理
            let result = self.process_new_message(param1, param2);
            ctx.text(serde_json::to_string(&result).unwrap());
        }
    }
}
```

**3. 客户端发送**:
```javascript
ws.send(JSON.stringify({
  aid: 'my_new_message',
  param1: 'test',
  param2: 123
}));
```

---

### Q36: 如何调试 qars 依赖问题

**症状**: qars 行为不符合预期

**解决方案**:

**查看 qars 源码**:
```bash
cd ../qars2
code src/qaaccount/account.rs
```

**修改 qars 并测试**:
```bash
cd ../qars2
# 修改代码
vim src/qaaccount/account.rs

# 在 qaexchange 中测试
cd ../qaexchange-rs
cargo build --lib
```

**使用 qars 的测试**:
```bash
cd ../qars2
cargo test qa_account
```

**查看 qars 版本**:
```bash
grep "qars" Cargo.toml
# qars = { path = "../qars2" }

cd ../qars2
git log -1
```

**临时使用其他 qars 版本**:
```toml
[dependencies]
# qars = { path = "../qars2" }
qars = { git = "https://github.com/QUANTAXIS/qars", branch = "dev" }
```

---

## 常用命令速查

### 编译和运行
```bash
# 编译库
cargo build --lib

# 编译服务器
cargo build --bin qaexchange-server

# 运行（debug）
cargo run --bin qaexchange-server

# 运行（release）
cargo run --release --bin qaexchange-server

# 运行示例
cargo run --example client_demo
```

### 测试
```bash
# 所有测试
cargo test

# 特定测试
cargo test test_name

# 显示输出
cargo test -- --nocapture

# 测试覆盖率
cargo tarpaulin
```

### API 测试
```bash
# 健康检查
curl http://localhost:8000/health

# 开户
curl -X POST http://localhost:8000/api/account/open \
  -H "Content-Type: application/json" \
  -d '{"user_id": "user123", "initial_balance": 100000}'

# 下单
curl -X POST http://localhost:8000/api/order/submit \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user123",
    "order_id": "order001",
    "instrument_id": "SHFE.cu2501",
    "direction": "BUY",
    "offset": "OPEN",
    "volume": 1,
    "price_type": "LIMIT",
    "limit_price": 50000
  }'

# 查询账户
curl http://localhost:8000/api/account/user123

# 查询持仓
curl http://localhost:8000/api/position/user123
```

### 日志和监控
```bash
# 查看日志
tail -f logs/qaexchange.log

# 开启 DEBUG 日志
RUST_LOG=debug cargo run

# 系统监控
curl http://localhost:8000/api/monitoring/system

# 存储监控
curl http://localhost:8000/api/monitoring/storage
```

---

## 获取帮助

如果以上方案无法解决您的问题，请：

1. **查看详细文档**: `docs/03_core_modules/`
2. **查看示例代码**: `examples/`
3. **提交 Issue**: [GitHub Issues](https://github.com/QUANTAXIS/qaexchange-rs/issues)
4. **加入社区**: QQ群 或 Discord

---

**版本**: v1.0.0
**最后更新**: 2025-10-06
**维护者**: QAExchange Team

---

[返回文档中心](../README.md) | [术语表](glossary.md) | [性能基准](benchmarks.md)
