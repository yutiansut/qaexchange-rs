# 通知系统故障排查指南

> 常见问题诊断和解决方案

**版本**: v1.1.0
**最后更新**: 2025-10-03

---

## 📋 快速诊断

### 检查清单

- [ ] Broker 和 Gateway 是否已启动？
- [ ] Gateway 是否已注册到 Broker？
- [ ] 用户是否已订阅 Gateway？
- [ ] WebSocket 会话是否已注册？
- [ ] 优先级处理器和推送任务是否正在运行？

---

## 常见问题

### 1. 消息未收到

#### 症状
WebSocket 客户端未收到通知消息

#### 诊断步骤

**1️⃣ 检查订阅关系**
```rust
// 确认用户已订阅
broker.subscribe("user_01", "gateway_01");
println!("Stats: {:?}", broker.get_stats());
// 检查 active_users 是否包含该用户
```

**2️⃣ 检查 Gateway 注册**
```rust
let (tx, rx) = mpsc::unbounded_channel();
broker.register_gateway("gateway_01", tx);
println!("Stats: {:?}", broker.get_stats());
// 检查 active_gateways 是否 > 0
```

**3️⃣ 检查会话注册**
```rust
let (session_tx, session_rx) = mpsc::unbounded_channel();
gateway.register_session("session_01", "user_01", session_tx);
println!("Gateway stats: {:?}", gateway.get_stats());
// 检查 active_sessions 是否 > 0
```

**4️⃣ 检查任务启动**
```rust
let _processor = broker.clone().start_priority_processor();
let _pusher = gateway.clone().start_notification_pusher();

// 等待任务启动
tokio::time::sleep(Duration::from_millis(50)).await;
```

**5️⃣ 检查消息发布**
```rust
match broker.publish(notification) {
    Ok(()) => println!("Published successfully"),
    Err(e) => eprintln!("Publish failed: {}", e),
}

let stats = broker.get_stats();
println!("Messages sent: {}", stats.messages_sent);
```

#### 解决方案

如果所有检查都通过但仍未收到消息：

```rust
// 添加调试日志
env_logger::init();  // 启用日志

// 设置环境变量
RUST_LOG=debug cargo run
```

---

### 2. 消息重复

#### 症状
同一消息被接收多次

#### 原因
- ❌ 已修复（v1.0.0）：Broker 中立即路由和 processor 路由导致重复

#### 验证修复
```rust
// 检查版本
cargo tree | grep qaexchange
// 应该显示 v1.0.0 或更高版本
```

#### 如果仍然重复
检查是否有多个 Gateway 订阅了同一用户：

```rust
// 检查用户的 Gateway 列表
let stats = broker.get_stats();
println!("User gateways: {:?}", broker.get_user_gateways("user_01"));
```

**解决方案**：确保每个用户只订阅一个 Gateway

---

### 3. 消息延迟

#### 症状
消息延迟超过预期

#### 诊断

**1️⃣ 检查优先级**
```rust
// 高优先级消息应该立即处理
let notification = Notification::with_priority(
    NotificationType::RiskAlert,
    user_id,
    payload,
    0,  // P0 最高优先级
    "RiskControl",
);
```

**2️⃣ 检查队列积压**
```rust
let stats = broker.get_stats();
println!("Queue sizes: {:?}", stats.queue_sizes);
// [P0, P1, P2, P3]
// 如果某个队列积压，说明处理不过来
```

**3️⃣ 检查批量推送配置**
```rust
// Gateway 默认配置
batch_size: 100,         // 100 条消息触发
batch_interval_ms: 100,  // 100ms 触发
```

#### 解决方案

**调整批量配置**（需要修改源码）：
```rust
// 减少批量大小，提高实时性
let gateway = NotificationGateway::new_with_config(
    gateway_id,
    rx,
    50,   // batch_size
    50,   // batch_interval_ms
);
```

---

### 4. 内存占用过高

#### 症状
长时间运行后内存持续增长

#### 诊断

**检查去重缓存大小**：
```rust
// Broker 去重缓存限制为 10,000 条
// 超过后会清理一半
```

**检查会话泄漏**：
```rust
let stats = gateway.get_stats();
println!("Active sessions: {}", stats.active_sessions);

// 如果持续增长，说明会话未正确清理
```

#### 解决方案

**1️⃣ 确保会话注销**
```rust
impl Drop for WebSocketSession {
    fn drop(&mut self) {
        gateway.unregister_session(&self.session_id);
        broker.unsubscribe(&self.user_id, "gateway_01");
    }
}
```

**2️⃣ 启用心跳检测**
```rust
let _heartbeat = gateway.clone().start_heartbeat_checker();
// 自动清理 5 分钟超时的会话
```

---

### 5. 编译错误

#### Arc<str> 序列化错误

**错误信息**：
```
error[E0597]: `json` does not live long enough
```

**原因**: 使用了旧版本（< v1.0.0）

**解决方案**: 升级到 v1.0.0+
```bash
git pull
cargo update
cargo build --lib
```

#### tokio::sync::Mutex 错误

**错误信息**：
```
error: future is not `Send`
```

**原因**: 使用了 `parking_lot::Mutex` 而非 `tokio::sync::Mutex`

**解决方案**: 检查版本
```bash
grep -r "parking_lot::Mutex" src/notification/
# 应该没有结果
```

---

### 6. 测试失败

#### test_batch_notification 失败

**错误**：
```
assertion failed: left == right
  left: 20
  right: 10
```

**原因**: 消息重复发送（已修复 v1.0.0）

**解决方案**: 升级到最新版本

#### test_publish_notification 超时

**错误**：
```
Timeout waiting for message
```

**原因**: 未启动 `priority_processor`

**解决方案**: 检查测试代码
```rust
let broker = Arc::new(NotificationBroker::new());
let _processor = broker.clone().start_priority_processor();  // ✅ 必须启动
```

---

## 性能问题

### CPU 占用过高

#### 诊断
```bash
# 使用 perf 分析
cargo build --release
perf record -g ./target/release/qaexchange-server
perf report
```

#### 常见原因
1. 优先级处理器循环过快
2. 批量推送频率过高
3. 日志输出过多

#### 解决方案
```rust
// 1. 降低处理器频率
let mut interval = tokio::time::interval(Duration::from_micros(500));  // 从 100 增加到 500

// 2. 减少日志级别
RUST_LOG=info  // 从 debug 改为 info
```

### 网络带宽占用

#### 症状
WebSocket 推送占用大量带宽

#### 诊断
```rust
let stats = gateway.get_stats();
let rate = stats.messages_pushed as f64 / runtime_seconds;
println!("Push rate: {:.2} msg/s", rate);
```

#### 解决方案
```rust
// 使用 rkyv 减少消息体积（v1.1.0+）
let bytes = notification.to_rkyv_bytes()?;
// rkyv 二进制比 JSON 小 30-50%

// 或者使用压缩
use flate2::Compression;
let compressed = compress(json.as_bytes(), Compression::fast());
```

---

## 日志分析

### 关键日志

**Broker 日志**：
```
[INFO] Gateway registered: gateway_01
[INFO] User subscribed: user_01 -> gateway_01
[INFO] Message published: message_id=xxx
[WARN] Priority queue 2 is full, message dropped
```

**Gateway 日志**：
```
[INFO] Session registered: session_01 for user user_01
[INFO] Notification pusher started
[WARN] Session session_01 timeout, removing
[ERROR] Failed to send notification to session xxx
```

### 日志配置

```rust
// 启用详细日志
env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

// 只记录特定模块
RUST_LOG=qaexchange::notification=debug cargo run
```

---

## 监控建议

### 关键指标

| 指标 | 监控方法 | 告警阈值 |
|------|---------|---------|
| 消息延迟 | `timestamp - send_time` | > 100ms |
| 队列积压 | `stats.queue_sizes` | > 1000 |
| 失败率 | `messages_failed / messages_sent` | > 1% |
| 活跃会话数 | `active_sessions` | 异常增长 |
| 内存占用 | 系统监控 | > 500MB |

### Prometheus 集成（TODO）

```rust
// 未来版本将支持
use prometheus::{IntGauge, register_int_gauge};

lazy_static! {
    static ref MESSAGES_SENT: IntGauge = register_int_gauge!(...).unwrap();
}

// 在 publish() 中
MESSAGES_SENT.inc();
```

---

## 紧急故障处理

### 系统无响应

```bash
# 1. 检查进程状态
ps aux | grep qaexchange

# 2. 检查线程状态
pstack <pid>

# 3. 生成 core dump
kill -ABRT <pid>

# 4. 分析 core dump
gdb ./target/release/qaexchange-server core.<pid>
```

### 消息积压

```rust
// 临时解决：清空队列
for priority in 0..4 {
    while let Some(_) = broker.priority_queues[priority].pop() {}
}

// 长期解决：扩容
// - 增加 priority_processor 数量
// - 分片用户到多个 Gateway
```

---

## 联系支持

遇到无法解决的问题？

1. **查看文档**: [README](../README.md) | [API Reference](../02_IMPLEMENTATION/API_REFERENCE.md)
2. **查看历史**: [Iterations](../ITERATIONS.md) - 类似问题的解决方案
3. **提交 Issue**: GitHub Issues
4. **社区支持**: QUANTAXIS 论坛

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
