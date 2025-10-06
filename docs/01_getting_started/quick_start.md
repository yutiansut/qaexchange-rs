# QAExchange 快速开始指南

## 🚀 快速启动

### 1. 编译项目

```bash
# 编译所有组件
cargo build --release

# 或者仅编译服务器
cargo build --release --bin qaexchange-server
```

### 2. 启动服务器

```bash
# 使用默认配置启动
cargo run --bin qaexchange-server

# 使用自定义配置
cargo run --bin qaexchange-server -- --config config/exchange.toml

# 指定端口
cargo run --bin qaexchange-server -- --http 127.0.0.1:8080 --ws 127.0.0.1:8081

# 禁用存储（仅内存模式）
cargo run --bin qaexchange-server -- --no-storage
```

服务器启动后会显示：

```
╔═══════════════════════════════════════════════════════════════════════╗
║                    🚀 QAExchange Server Started                       ║
╚═══════════════════════════════════════════════════════════════════════╝

📡 Service Endpoints:
   • HTTP API:    http://127.0.0.1:8080
   • WebSocket:   ws://127.0.0.1:8081/ws
   • Health:      http://127.0.0.1:8080/health

💾 Storage:
   • Status:      Enabled ✓
   • Path:        /tmp/qaexchange/storage
   • Mode:        Async batch write (100 records / 10ms)
```

### 3. 运行完整演示

在另一个终端运行客户端演示：

```bash
cargo run --example full_trading_demo
```

这会演示：
- HTTP API 开户
- HTTP API 提交订单
- WebSocket 连接和认证
- WebSocket 实时交易
- 实时通知推送

## 📋 配置文件

配置文件位置：`config/exchange.toml`

主要配置项：

```toml
# 服务器配置
[server]
name = "QAExchange"
environment = "development"  # development | production | testing
log_level = "info"           # trace | debug | info | warn | error

# HTTP API
[http]
host = "127.0.0.1"
port = 8080

# WebSocket
[websocket]
host = "127.0.0.1"
port = 8081

# 存储配置
[storage]
enabled = true
base_path = "/tmp/qaexchange/storage"

[storage.subscriber]
batch_size = 100            # 批量写入大小
batch_timeout_ms = 10       # 批量超时
buffer_size = 10000         # 缓冲区大小

# 合约配置
[[instruments]]
instrument_id = "IF2501"
name = "沪深300股指期货2501"
init_price = 3800.0
is_trading = true
```

## 🔌 API 使用示例

### HTTP REST API

#### 1. 健康检查

```bash
curl http://127.0.0.1:8080/health
```

#### 2. 开户

```bash
curl -X POST http://127.0.0.1:8080/api/account/open \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "demo_user",
    "user_name": "Demo User",
    "init_cash": 1000000,
    "account_type": "individual",
    "password": "demo123"
  }'
```

响应：

```json
{
  "success": true,
  "data": {
    "account_id": "demo_user"
  }
}
```

#### 3. 查询账户

```bash
curl http://127.0.0.1:8080/api/account/demo_user
```

响应：

```json
{
  "success": true,
  "data": {
    "user_id": "demo_user",
    "balance": 1000000.0,
    "available": 1000000.0,
    "margin": 0.0,
    "profit": 0.0,
    "risk_ratio": 0.0
  }
}
```

#### 4. 提交订单

```bash
curl -X POST http://127.0.0.1:8080/api/order/submit \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "demo_user",
    "instrument_id": "IF2501",
    "direction": "BUY",
    "offset": "OPEN",
    "volume": 1,
    "price": 3800,
    "order_type": "LIMIT"
  }'
```

响应：

```json
{
  "success": true,
  "data": {
    "order_id": "O1735123456001",
    "status": "submitted"
  }
}
```

#### 5. 查询订单

```bash
curl http://127.0.0.1:8080/api/order/O1735123456001
```

#### 6. 查询用户所有订单

```bash
curl http://127.0.0.1:8080/api/order/user/demo_user
```

### WebSocket API

#### 连接

```javascript
const ws = new WebSocket('ws://127.0.0.1:8081/ws?user_id=demo_user');
```

#### 1. 认证

发送：

```json
{
  "type": "auth",
  "user_id": "demo_user",
  "token": "demo_token"
}
```

接收：

```json
{
  "type": "auth_response",
  "success": true,
  "user_id": "demo_user",
  "message": "Authentication successful"
}
```

#### 2. 提交订单

发送：

```json
{
  "type": "submit_order",
  "instrument_id": "IF2501",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 1,
  "price": 3800,
  "order_type": "LIMIT"
}
```

接收（订单响应）：

```json
{
  "type": "order_response",
  "success": true,
  "order_id": "O1735123456002",
  "message": "Order submitted"
}
```

#### 3. 实时通知

成交通知：

```json
{
  "type": "trade",
  "trade_id": "T1735123456001",
  "order_id": "O1735123456002",
  "instrument_id": "IF2501",
  "price": 3800.0,
  "volume": 1.0,
  "timestamp": 1735123456000000000
}
```

账户更新：

```json
{
  "type": "account_update",
  "user_id": "demo_user",
  "balance": 999450.0,
  "available": 885450.0,
  "margin": 114000.0
}
```

#### 4. 查询账户

发送：

```json
{
  "type": "query_account"
}
```

#### 5. 心跳

发送：

```json
{
  "type": "ping"
}
```

接收：

```json
{
  "type": "pong"
}
```

## 📊 合约列表

| 合约代码 | 名称 | 交易所 | 初始价格 | 合约乘数 |
|---------|------|--------|----------|----------|
| IF2501  | 沪深300股指期货2501 | CFFEX | 3800.0 | 300 |
| IC2501  | 中证500股指期货2501 | CFFEX | 5600.0 | 200 |
| IH2501  | 上证50股指期货2501  | CFFEX | 2800.0 | 300 |

## 🛠️ 示例程序

### 1. 完整交易演示

```bash
cargo run --example full_trading_demo
```

演示 HTTP + WebSocket 完整交易流程。

### 2. 解耦存储演示

```bash
cargo run --example decoupled_storage_demo
```

演示异步存储架构。

### 3. 压力测试

```bash
cargo run --example stress_test
```

测试系统性能。

## 💾 数据持久化

### 存储目录结构

```
/tmp/qaexchange/storage/
├── IF2501/
│   ├── wal/
│   │   ├── 00000001.wal
│   │   └── 00000002.wal
│   └── sstables/
│       ├── 00000001.sst
│       └── 00000002.sst
├── IC2501/
└── IH2501/
```

### 查看存储数据

```bash
# 查看存储文件
ls -lh /tmp/qaexchange/storage/IF2501/

# 查看 WAL 文件数量
find /tmp/qaexchange/storage -name "*.wal" | wc -l

# 查看总存储大小
du -sh /tmp/qaexchange/storage/
```

### 清空数据

```bash
# 清空所有数据
rm -rf /tmp/qaexchange/storage/

# 清空特定合约
rm -rf /tmp/qaexchange/storage/IF2501/
```

## 🐛 故障排除

### 1. 端口被占用

```bash
# 查找占用端口的进程
lsof -i :8080
lsof -i :8081

# 杀死进程
kill -9 <PID>

# 或者使用不同端口启动
cargo run --bin qaexchange-server -- --http 127.0.0.1:9090 --ws 127.0.0.1:9091
```

### 2. 存储目录权限问题

```bash
# 创建目录并设置权限
mkdir -p /tmp/qaexchange/storage
chmod 755 /tmp/qaexchange/storage
```

### 3. 配置文件问题

```bash
# 验证配置文件语法
cat config/exchange.toml | grep -v "^#" | grep -v "^$"

# 使用默认配置
cargo run --bin qaexchange-server -- --no-config
```

### 4. WebSocket 连接失败

- 确保服务器已启动
- 检查防火墙设置
- 使用正确的 URL 格式：`ws://127.0.0.1:8081/ws?user_id=<USER_ID>`

## 📚 更多文档

- [架构文档](docs/DECOUPLED_STORAGE_ARCHITECTURE.md) - 解耦存储架构详解
- [性能文档](docs/PERFORMANCE.md) - 性能测试和优化
- [CLAUDE.md](CLAUDE.md) - 项目开发指南

## 🎯 下一步

1. **生产部署**：
   - 修改配置文件中的存储路径
   - 启用监控和指标收集
   - 配置日志级别为 `warn` 或 `error`

2. **性能测试**：
   ```bash
   cargo run --release --example stress_test
   ```

3. **开发自定义功能**：
   - 参考 [CLAUDE.md](CLAUDE.md) 开发指南
   - 优先复用现有组件
   - 遵循解耦架构原则

## ⚠️ 注意事项

1. **生产环境**：
   - 不要使用默认存储路径 `/tmp`
   - 启用安全认证
   - 配置 HTTPS/WSS
   - 启用监控和日志

2. **数据安全**：
   - 定期备份 WAL 和 SSTable
   - 测试崩溃恢复机制
   - 监控磁盘空间

3. **性能调优**：
   - 根据硬件调整 `batch_size`
   - SSD 使用 `sync_mode = "async"`
   - HDD 增大 `batch_timeout_ms`

---

**Have fun trading! 🚀**
