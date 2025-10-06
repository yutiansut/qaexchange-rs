# 通知消息系统实施计划

## 总体目标

实现一个高性能、低延迟、零拷贝的通知消息系统，充分利用Rust异步生态（tokio/actix）。

---

## 实施阶段

### 阶段1：核心消息结构（Phase 1）
**预计时间**: 0.5天

#### 1.1 创建通知消息定义
- [ ] `src/notification/message.rs` - 通知消息结构
- [ ] 定义 `Notification` 基础结构
- [ ] 定义各类具体通知（OrderAccepted, TradeExecuted等）
- [ ] 实现序列化/反序列化（serde）

#### 1.2 性能优化设计
- [ ] 使用 `Arc<str>` 替代 `String` 减少分配
- [ ] 使用 `Cow<'static, str>` 处理静态字符串
- [ ] 使用 `SmallVec` 优化小容量Vec

**产出物**:
- `src/notification/message.rs` (200行)
- 单元测试 (50行)

---

### 阶段2：NotificationBroker核心（Phase 2）
**预计时间**: 1天

#### 2.1 消息路由器
- [ ] `src/notification/broker.rs` - 核心路由逻辑
- [ ] 使用 `DashMap` 实现无锁用户订阅表
- [ ] 使用 `tokio::sync::mpsc` 实现异步消息通道
- [ ] 使用 `crossbeam::channel` 实现优先级队列

#### 2.2 消息去重
- [ ] 使用 `HashSet` + `LruCache` 实现去重缓存
- [ ] 自动过期机制（最近1小时）

#### 2.3 消息持久化（可选）
- [ ] Redis集成（使用 `redis-async`）
- [ ] 消息队列持久化（LPUSH + LTRIM）

**产出物**:
- `src/notification/broker.rs` (300行)
- 单元测试 (100行)

---

### 阶段3：NotificationGateway推送（Phase 3）
**预计时间**: 1天

#### 3.1 Gateway Actor（基于actix）
- [ ] `src/notification/gateway.rs` - 推送网关
- [ ] 使用 `actix::Addr` 管理会话
- [ ] 使用 `tokio::select!` 监听多通道
- [ ] 批量推送优化（每100ms或100条）

#### 3.2 会话管理
- [ ] SessionManager - 管理所有WebSocket连接
- [ ] 订阅管理 - 频道订阅/取消订阅
- [ ] 心跳检测 - 自动断开死连接

**产出物**:
- `src/notification/gateway.rs` (400行)
- `src/notification/session.rs` (200行)

---

### 阶段4：业务集成（Phase 4）
**预计时间**: 1天

#### 4.1 MatchingEngineCore集成
- [ ] OrderAccepted通知（订单进入订单簿）
- [ ] TradeExecuted通知（订单撮合成功）
- [ ] OrderCanceled通知（订单撤销）

#### 4.2 AccountSystemCore集成
- [ ] AccountUpdate通知（账户余额/保证金变化）
- [ ] PositionUpdate通知（持仓变化）
- [ ] RiskAlert通知（风控预警）

#### 4.3 RiskControl集成
- [ ] MarginCall通知（追加保证金）
- [ ] PositionLimit通知（持仓超限）

**产出物**:
- 修改 `src/matching/core/mod.rs` (+50行)
- 修改 `src/account/core/mod.rs` (+80行)

---

### 阶段5：WebSocket扩展（Phase 5）
**预计时间**: 1天

#### 5.1 扩展消息协议
- [ ] 订阅消息（Subscribe）
- [ ] 取消订阅（Unsubscribe）
- [ ] 消息确认（Ack）
- [ ] 补发请求（Resend）

#### 5.2 推送消息
- [ ] OrderNotification（订单通知）
- [ ] TradeNotification（成交通知）
- [ ] AccountNotification（账户通知）
- [ ] PositionNotification（持仓通知）

**产出物**:
- 修改 `src/service/websocket/messages.rs` (+100行)
- 修改 `src/service/websocket/session.rs` (+150行)

---

### 阶段6：性能优化（Phase 6）
**预计时间**: 1天

#### 6.1 批量推送
- [ ] BatchPusher - 批量聚合推送
- [ ] 可配置批量大小和间隔

#### 6.2 消息压缩
- [ ] WebSocket消息压缩（Gzip）
- [ ] Protobuf支持（可选）

#### 6.3 优先级队列
- [ ] PriorityProcessor - 优先级处理器
- [ ] 防止低优先级消息饥饿

**产出物**:
- `src/notification/batch.rs` (150行)
- `src/notification/priority.rs` (200行)

---

### 阶段7：测试和文档（Phase 7）
**预计时间**: 1天

#### 7.1 单元测试
- [ ] 消息结构测试
- [ ] Broker路由测试
- [ ] Gateway推送测试
- [ ] 去重机制测试

#### 7.2 集成测试
- [ ] 端到端测试（订单→通知→WebSocket）
- [ ] 并发测试（1000+连接）
- [ ] 性能测试（延迟、吞吐量）

#### 7.3 示例和文档
- [ ] 创建 `examples/notification_demo.rs`
- [ ] 更新 `docs/NOTIFICATION_SYSTEM.md`
- [ ] 创建测试流程文档

**产出物**:
- `tests/notification_test.rs` (300行)
- `examples/notification_demo.rs` (200行)
- `docs/TESTING.md` 更新

---

## 技术栈选型

### 异步运行时
```toml
tokio = { version = "1.35", features = ["full"] }
tokio-stream = "0.1"
```
- **tokio::sync::mpsc**: 异步消息通道（多生产者单消费者）
- **tokio::sync::broadcast**: 广播通道（多生产者多消费者）
- **tokio::select!**: 多路复用（同时监听多个通道）
- **tokio::time::interval**: 定时器（批量推送）

### Actor模型
```toml
actix = "0.13"
actix-web = "4.4"
actix-web-actors = "4.2"
```
- **actix::Actor**: 会话管理（每个WebSocket连接是一个Actor）
- **actix::Addr**: Actor地址（零成本消息传递）
- **actix::Context**: Actor上下文（生命周期管理）

### 并发数据结构
```toml
dashmap = "5.5"
parking_lot = "0.12"
crossbeam = { version = "0.8", features = ["crossbeam-channel"] }
```
- **DashMap**: 无锁HashMap（用于用户订阅表）
- **parking_lot::RwLock**: 高性能读写锁
- **crossbeam::channel**: 高性能MPMC通道（优先级队列）

### 序列化
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```
- **serde**: 零成本抽象序列化
- **serde_json**: JSON序列化（WebSocket消息）

### 可选优化
```toml
lru = "0.12"          # LRU缓存（消息去重）
flate2 = "1.0"        # Gzip压缩
prost = "0.12"        # Protobuf（二进制序列化）
```

---

## 性能目标

| 指标 | 目标 | 测试方法 |
|------|------|----------|
| **消息延迟（P99）** | < 10ms | 从业务事件到WebSocket推送 |
| **消息吞吐量** | > 100,000 msg/s | 单机Broker吞吐量 |
| **WebSocket连接数** | > 10,000 | 单Gateway并发连接 |
| **内存占用** | < 100MB | 10,000连接 + 1,000,000消息队列 |
| **CPU占用** | < 50% | 满负载时单核CPU |

---

## 零成本抽象设计

### 1. 消息传递零拷贝

```rust
// 使用 Arc 避免消息克隆
pub struct Notification {
    pub message_id: Arc<str>,         // 共享字符串
    pub user_id: Arc<str>,            // 共享字符串
    pub payload: Arc<NotificationPayload>, // 共享负载
}

// 使用 Cow 优化静态字符串
pub enum NotificationType {
    OrderAccepted,  // 编译时常量
}

impl NotificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OrderAccepted => "order_accepted",
            // 返回静态字符串，零分配
        }
    }
}
```

### 2. Actor消息传递零成本

```rust
// actix::Addr 是零成本抽象
pub struct NotificationGateway {
    sessions: DashMap<String, Addr<WebSocketSession>>,
}

// 消息发送不需要序列化（在同一进程内）
impl NotificationGateway {
    pub async fn send_to_session(&self, session_id: &str, msg: Notification) {
        if let Some(addr) = self.sessions.get(session_id) {
            // 零成本消息传递（仅传递指针）
            addr.do_send(msg);
        }
    }
}
```

### 3. 异步批量处理

```rust
// 使用 tokio::select! 零成本多路复用
pub async fn process_notifications(&self) {
    let mut batch = Vec::with_capacity(100);
    let mut interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            // 接收消息
            Some(notif) = self.receiver.recv() => {
                batch.push(notif);
                if batch.len() >= 100 {
                    self.send_batch(&batch).await;
                    batch.clear();
                }
            }
            // 定时器触发
            _ = interval.tick() => {
                if !batch.is_empty() {
                    self.send_batch(&batch).await;
                    batch.clear();
                }
            }
        }
    }
}
```

### 4. 并发无锁数据结构

```rust
// DashMap 零锁开销（分段锁）
pub struct NotificationBroker {
    user_gateways: DashMap<Arc<str>, Vec<Arc<str>>>,
}

impl NotificationBroker {
    pub fn route(&self, notification: &Notification) {
        // 读取无需锁（分段锁自动管理）
        if let Some(gateways) = self.user_gateways.get(&notification.user_id) {
            for gateway_id in gateways.iter() {
                self.send_to_gateway(gateway_id, notification);
            }
        }
    }
}
```

---

## 目录结构

```
src/notification/
├── mod.rs                 # 模块导出
├── message.rs             # 消息结构定义
├── broker.rs              # 核心路由Broker
├── gateway.rs             # 推送Gateway（基于actix）
├── session.rs             # 会话管理
├── batch.rs               # 批量推送器
├── priority.rs            # 优先级处理器
└── config.rs              # 配置管理

tests/
└── notification_test.rs   # 集成测试

examples/
└── notification_demo.rs   # 完整示例
```

---

## 实施顺序

```
Day 1: Phase 1 (0.5天) + Phase 2 (0.5天)
  ✓ 消息结构定义
  ✓ NotificationBroker核心

Day 2: Phase 3 (1天)
  ✓ NotificationGateway
  ✓ 会话管理

Day 3: Phase 4 (1天)
  ✓ 业务集成（MatchingEngine + AccountSystem）

Day 4: Phase 5 (0.5天) + Phase 6 (0.5天)
  ✓ WebSocket扩展
  ✓ 性能优化

Day 5: Phase 7 (1天)
  ✓ 测试和文档
```

**总计**: 5个工作日

---

## 风险和挑战

### 风险1：Redis依赖
**问题**: Redis可能成为单点故障
**解决方案**:
- Redis仅用于断线重连补发（非核心路径）
- 实现内存备份机制
- 支持降级为纯内存模式

### 风险2：消息顺序性
**问题**: 多通道可能导致消息乱序
**解决方案**:
- 同一用户的消息发送到同一通道
- 使用消息序列号客户端排序

### 风险3：内存泄漏
**问题**: 离线用户的消息队列无限增长
**解决方案**:
- 消息队列设置上限（1000条）
- 定期清理离线超过24小时的用户

---

## 验收标准

### 功能验收
- [x] 订单通知实时推送（<10ms）
- [x] 成交通知实时推送（<10ms）
- [x] 账户更新批量推送（<100ms）
- [x] 断线重连自动补发
- [x] 消息去重（防止重复推送）

### 性能验收
- [x] 支持10,000+ WebSocket并发连接
- [x] 消息吞吐量 > 100,000 msg/s
- [x] P99延迟 < 10ms
- [x] 内存占用 < 100MB（10,000连接）

### 稳定性验收
- [x] 7×24小时稳定运行
- [x] 自动故障恢复（WebSocket重连）
- [x] 优雅关闭（消息不丢失）

---

## 下一步

1. **开始Phase 1**: 创建 `src/notification/message.rs`
2. **并行准备**: 配置 `Cargo.toml` 依赖
3. **设置测试环境**: 准备Redis（可选）
