# 通知消息系统测试流程

## 测试目标

验证通知消息系统的功能正确性、性能指标和稳定性。

---

## 测试环境准备

### 1. 依赖检查

```bash
cd /home/quantaxis/qaexchange-rs

# 检查依赖
cargo check --lib

# 编译通知模块
cargo build --lib
```

### 2. 环境变量

```bash
# 设置日志级别
export RUST_LOG=info

# 性能测试时设置
export RUST_LOG=warn  # 减少日志开销
```

---

## 单元测试

### 1. 消息结构测试

```bash
# 测试消息结构的创建和序列化
cargo test --lib notification::message::tests
```

**验证点：**
- ✅ Notification 创建
- ✅ NotificationType 优先级设置
- ✅ 消息序列化/反序列化
- ✅ 类型名称转换

**预期结果：**
```
test notification::message::tests::test_notification_creation ... ok
test notification::message::tests::test_notification_priority ... ok
test notification::message::tests::test_notification_type_str ... ok
test notification::message::tests::test_serialization ... ok
```

### 2. Broker测试

```bash
# 测试NotificationBroker的路由和去重功能
cargo test --lib notification::broker::tests
```

**验证点：**
- ✅ Broker 创建
- ✅ Gateway 注册/注销
- ✅ 用户订阅/取消订阅
- ✅ 消息发布和路由
- ✅ 消息去重
- ✅ 优先级队列

**预期结果：**
```
test notification::broker::tests::test_broker_creation ... ok
test notification::broker::tests::test_gateway_registration ... ok
test notification::broker::tests::test_user_subscription ... ok
test notification::broker::tests::test_publish_notification ... ok
test notification::broker::tests::test_message_deduplication ... ok
test notification::broker::tests::test_priority_queue ... ok
```

### 3. Gateway测试

```bash
# 测试NotificationGateway的推送功能
cargo test --lib notification::gateway::tests
```

**验证点：**
- ✅ Gateway 创建
- ✅ 会话注册/注销
- ✅ 消息推送
- ✅ 批量推送

**预期结果：**
```
test notification::gateway::tests::test_gateway_creation ... ok
test notification::gateway::tests::test_session_registration ... ok
test notification::gateway::tests::test_notification_push ... ok
test notification::gateway::tests::test_batch_push ... ok
```

### 运行所有单元测试

```bash
cargo test --lib notification
```

---

## 集成测试

### 1. 端到端测试

创建 `tests/notification_integration_test.rs`:

```rust
//! 通知系统集成测试

use qaexchange::notification::*;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::test]
async fn test_end_to_end_notification_flow() {
    // 1. 创建系统
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gw1", rx));

    // 2. 连接系统
    broker.register_gateway("gw1", tx);
    broker.subscribe("user_01", "gw1");

    // 3. 注册会话
    let (session_tx, mut session_rx) = mpsc::unbounded_channel();
    gateway.register_session("s1", "user_01", session_tx);

    // 4. 启动任务
    let _p1 = gateway.clone().start_notification_pusher();
    let _p2 = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5. 发送通知
    let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    let notification = Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        payload,
        "Test",
    );

    broker.publish(notification).unwrap();

    // 6. 验证接收
    let received = tokio::time::timeout(
        Duration::from_secs(1),
        session_rx.recv()
    ).await.expect("Timeout").expect("No message");

    assert!(received.contains("account_update"));
    assert!(received.contains("user_01"));
}

#[tokio::test]
async fn test_multi_user_notification() {
    // 测试多用户消息隔离
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gw1", rx));

    broker.register_gateway("gw1", tx);
    broker.subscribe("user_01", "gw1");
    broker.subscribe("user_02", "gw1");

    let (s1_tx, mut s1_rx) = mpsc::unbounded_channel();
    let (s2_tx, mut s2_rx) = mpsc::unbounded_channel();

    gateway.register_session("s1", "user_01", s1_tx);
    gateway.register_session("s2", "user_02", s2_tx);

    let _p1 = gateway.clone().start_notification_pusher();
    let _p2 = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发送给user_01
    let payload1 = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    });

    broker.publish(Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_01"),
        payload1,
        "Test",
    )).unwrap();

    // user_01应该收到，user_02不应该收到
    let msg1 = tokio::time::timeout(Duration::from_millis(200), s1_rx.recv())
        .await.expect("Timeout").expect("No message");
    assert!(msg1.contains("user_01"));

    let msg2 = tokio::time::timeout(Duration::from_millis(200), s2_rx.recv()).await;
    assert!(msg2.is_err(), "user_02 should not receive message");
}
```

运行集成测试：

```bash
cargo test --test notification_integration_test
```

---

## 功能测试

### 1. 运行示例程序

```bash
# 运行完整示例
cargo run --example notification_demo

# 观察输出，验证：
# - Broker和Gateway创建成功
# - 会话注册成功
# - 各类通知消息正确推送
# - 统计信息正确
```

**预期输出：**

```
=== 通知消息系统示例 ===

1. 创建 NotificationBroker
2. 创建 NotificationGateway
3. 注册 Gateway 到 Broker
4. 订阅用户消息
5. 注册 WebSocket 会话
6. 启动推送任务
7. 启动消息接收任务

8. 发送各类通知消息

8.1 发送订单确认通知（用户1）
[Session 1] Received message 1:
{
  "message_id": "...",
  "message_type": "order_accepted",
  "user_id": "user_01",
  "priority": 1,
  "payload": {
    "type": "order_accepted",
    "order_id": "a1b2c3d4-e5f6-7890-abcd-1234567890ab",
    "exchange_order_id": "EX_1728123456789_IX2401_B",
    ...
  },
  ...
}

...

10. 统计信息
Broker统计:
  - 已发送消息: 12
  - 已去重消息: 0
  - 已丢弃消息: 0
  - 活跃用户数: 2
  - 活跃Gateway数: 1
  - 队列大小: P0=0, P1=0, P2=0, P3=0

Gateway统计:
  - Gateway ID: gateway_01
  - 已推送消息: 12
  - 推送失败数: 0
  - 活跃会话数: 2

=== 示例完成 ===
```

### 2. 验证消息去重

```bash
# 修改示例代码，发送相同message_id的消息两次
# 验证只接收一次
```

### 3. 验证优先级处理

```bash
# 发送不同优先级的消息
# 验证P0消息优先被处理
```

---

## 性能测试

### 1. 吞吐量测试

创建 `examples/notification_benchmark.rs`:

```rust
//! 通知系统性能测试

use qaexchange::notification::*;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("=== 通知系统性能测试 ===\n");

    // 测试配置
    let num_users = 1000;
    let num_messages_per_user = 100;
    let total_messages = num_users * num_messages_per_user;

    println!("配置:");
    println!("  - 用户数: {}", num_users);
    println!("  - 每用户消息数: {}", num_messages_per_user);
    println!("  - 总消息数: {}\n", total_messages);

    // 创建系统
    let broker = Arc::new(NotificationBroker::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let gateway = Arc::new(NotificationGateway::new("gw1", rx));

    broker.register_gateway("gw1", tx);

    // 注册用户和会话
    let mut receivers = Vec::new();
    for i in 0..num_users {
        let user_id = format!("user_{:04}", i);
        broker.subscribe(&user_id, "gw1");

        let (s_tx, s_rx) = mpsc::unbounded_channel();
        gateway.register_session(&format!("s_{:04}", i), &user_id, s_tx);
        receivers.push(s_rx);
    }

    // 启动任务
    let _p1 = gateway.clone().start_notification_pusher();
    let _p2 = broker.clone().start_priority_processor();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 发送消息
    println!("开始发送消息...");
    let start = Instant::now();

    for i in 0..num_users {
        let user_id = format!("user_{:04}", i);

        for j in 0..num_messages_per_user {
            let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
                user_id: user_id.clone(),
                balance: 1000000.0 + j as f64,
                available: 980000.0,
                frozen: 0.0,
                margin: 20000.0,
                position_profit: j as f64,
                close_profit: 0.0,
                risk_ratio: 0.02,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            });

            broker.publish(Notification::new(
                NotificationType::AccountUpdate,
                Arc::from(user_id.clone()),
                payload,
                "Benchmark",
            )).unwrap();
        }
    }

    let publish_duration = start.elapsed();
    println!("消息发送完成，耗时: {:?}", publish_duration);
    println!("发送吞吐量: {:.0} msg/s\n",
             total_messages as f64 / publish_duration.as_secs_f64());

    // 等待所有消息被推送
    println!("等待消息推送...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 统计信息
    let broker_stats = broker.get_stats();
    let gateway_stats = gateway.get_stats();

    println!("\n性能统计:");
    println!("  - 已发送消息: {}", broker_stats.messages_sent);
    println!("  - 已推送消息: {}", gateway_stats.messages_pushed);
    println!("  - 推送成功率: {:.2}%",
             100.0 * gateway_stats.messages_pushed as f64 / broker_stats.messages_sent as f64);
    println!("  - 平均延迟: {:.2} ms",
             publish_duration.as_millis() as f64 / total_messages as f64);
}
```

运行性能测试：

```bash
cargo run --release --example notification_benchmark
```

**性能指标验收：**
- ✅ 消息吞吐量 > 100,000 msg/s
- ✅ 平均延迟 < 1ms
- ✅ 推送成功率 > 99.9%

### 2. 并发测试

```bash
# 同时运行多个客户端
for i in {1..10}; do
  cargo run --example notification_demo &
done

wait
```

### 3. 内存测试

```bash
# 使用 valgrind 或 heaptrack 检测内存泄漏
cargo build --release --example notification_demo
valgrind --leak-check=full ./target/release/examples/notification_demo
```

---

## 稳定性测试

### 1. 长时间运行测试

```bash
# 运行24小时测试
timeout 86400 cargo run --release --example notification_benchmark
```

**验证点：**
- ✅ 无内存泄漏
- ✅ 无崩溃
- ✅ 性能稳定

### 2. 异常情况测试

#### 测试1：Gateway断开

```rust
// 模拟Gateway断开
gateway.unregister_session("session_01");

// 验证：消息发送不会崩溃，会记录错误日志
```

#### 测试2：消息队列满

```rust
// 快速发送大量消息，填满队列
// 验证：多余消息被丢弃，不会崩溃
```

#### 测试3：消息去重

```rust
// 发送相同message_id的消息
// 验证：只有第一条被处理
```

---

## 测试检查清单

### 功能测试
- [ ] ✅ 消息创建和序列化
- [ ] ✅ Broker路由功能
- [ ] ✅ Gateway推送功能
- [ ] ✅ 消息去重
- [ ] ✅ 优先级队列
- [ ] ✅ 多用户隔离
- [ ] ✅ 会话管理

### 性能测试
- [ ] ✅ 吞吐量 > 100,000 msg/s
- [ ] ✅ 延迟 < 10ms (P99)
- [ ] ✅ 支持 10,000+ 并发会话
- [ ] ✅ 内存占用 < 100MB (10,000会话)

### 稳定性测试
- [ ] ✅ 24小时稳定运行
- [ ] ✅ 无内存泄漏
- [ ] ✅ 异常恢复（Gateway断开）
- [ ] ✅ 消息不丢失（断线重连）

---

## 问题排查

### 问题1：消息未接收

**检查步骤：**
1. 验证 Gateway 已注册：`broker.get_stats().active_gateways`
2. 验证用户已订阅：检查 `user_gateways`
3. 验证会话已注册：`gateway.get_stats().active_sessions`
4. 检查推送任务是否启动：`start_notification_pusher()`
5. 检查日志：`RUST_LOG=debug`

### 问题2：性能不达标

**优化方向：**
1. 增加批量大小：`batch_size = 500`
2. 减少日志输出：`RUST_LOG=warn`
3. 使用 release 模式：`cargo build --release`
4. 调整优先级队列大小

### 问题3：内存泄漏

**排查工具：**
```bash
# 使用 heaptrack
heaptrack ./target/release/examples/notification_demo

# 使用 valgrind
valgrind --leak-check=full ./target/release/examples/notification_demo
```

---

## 持续集成（CI）

### GitHub Actions 配置

```yaml
name: Notification System Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run tests
        run: cargo test --lib notification
      - name: Run integration tests
        run: cargo test --test notification_integration_test
      - name: Run example
        run: timeout 30 cargo run --example notification_demo
```

---

## 总结

完整的测试流程覆盖了：
1. ✅ **单元测试** - 各模块独立功能
2. ✅ **集成测试** - 端到端流程
3. ✅ **功能测试** - 实际使用场景
4. ✅ **性能测试** - 吞吐量和延迟
5. ✅ **稳定性测试** - 长时间运行和异常恢复

通过所有测试后，通知消息系统即可投入生产使用。
