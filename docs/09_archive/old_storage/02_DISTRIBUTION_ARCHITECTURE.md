# 零拷贝数据分发架构

> 高性能、高可靠的实时数据分发系统

**版本**: v1.0.0
**最后更新**: 2025-10-03

---

## 📋 目录

- [架构概览](#架构概览)
- [零拷贝分发设计](#零拷贝分发设计)
- [多级订阅系统](#多级订阅系统)
- [可靠性保证](#可靠性保证)
- [性能优化](#性能优化)

---

## 架构概览

### 设计目标

1. **超低延迟**：P99 < 10μs（零拷贝 + 共享内存）
2. **高吞吐**：> 10M msg/s（iceoryx2 零拷贝）
3. **高可靠**：确认机制 + 断点续传
4. **水平扩展**：多 Publisher + 多 Subscriber

### 数据流

```
数据源 (MatchingEngine/AccountSystem)
    ↓
Publisher (rkyv 序列化)
    ↓
iceoryx2 共享内存 (零拷贝)
    ↓
    ├─→ Real-time Subscriber (WebSocket) - P99 < 10μs
    ├─→ Delayed Subscriber (Batch) - 100ms 延迟
    └─→ Historical Subscriber (WAL Replay) - 历史数据
```

### 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                      数据源层                                │
│  MatchingEngine | AccountSystem | RiskControl               │
└────────────────────────┬────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                  Publisher 层                                │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Trade        │  │ Account      │  │ Market       │      │
│  │ Publisher    │  │ Publisher    │  │ Publisher    │      │
│  └──────┬───────┘  └──────┬───────┘  │              │      │
│         │                 │          └──────┬───────┘      │
│         └─────────────────┴─────────────────┘              │
│                          ↓                                  │
│              ┌─────────────────────┐                        │
│              │  rkyv Serialization │                        │
│              └─────────────────────┘                        │
└─────────────────────────┬───────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│            iceoryx2 共享内存总线 (零拷贝)                     │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Topic: trade_events   | 10MB Ring Buffer            │   │
│  │  Topic: account_events | 10MB Ring Buffer            │   │
│  │  Topic: market_l2      | 50MB Ring Buffer            │   │
│  └─────────────────────────────────────────────────────┘   │
└────────────┬────────────┬────────────┬─────────────────────┘
             ↓            ↓            ↓
┌─────────────────────────────────────────────────────────────┐
│                  Subscriber 层                               │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Real-time    │  │ Delayed      │  │ Historical   │      │
│  │ (WebSocket)  │  │ (Batch)      │  │ (WAL Replay) │      │
│  │              │  │              │  │              │      │
│  │ P99 < 10μs   │  │ 100ms batch  │  │ Full history │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

---

## 零拷贝分发设计

### 1. 基于 iceoryx2 的共享内存

**优势**：
- 零拷贝：数据直接写入共享内存
- 低延迟：P99 < 10μs（无序列化开销）
- 高吞吐：> 10M msg/s

**复用 qars broadcast_hub**：

```rust
// qars/libs/qadata/src/broadcast_hub.rs 已实现
use qars::qadata::broadcast_hub::{BroadcastHub, Topic};

// 我们需要扩展以支持 rkyv
```

### 2. rkyv 零拷贝序列化

```rust
// src/distribution/message.rs

use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};

/// 分发消息类型
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub enum DistributionMessage {
    /// 成交事件
    TradeEvent {
        trade_id: [u8; 40],
        order_id: [u8; 40],
        instrument_id: [u8; 16],
        price: f64,
        volume: f64,
        direction: u8,
        timestamp: i64,
    },

    /// 账户更新
    AccountUpdate {
        user_id: [u8; 32],
        balance: f64,
        available: f64,
        margin: f64,
        timestamp: i64,
    },

    /// Level2 行情
    MarketL2 {
        instrument_id: [u8; 16],
        bids: [(f64, f64); 10],  // (price, volume)
        asks: [(f64, f64); 10],
        timestamp: i64,
    },

    /// 心跳
    Heartbeat {
        publisher_id: [u8; 16],
        sequence: u64,
        timestamp: i64,
    },
}

impl DistributionMessage {
    /// 序列化为 rkyv 字节流
    pub fn to_rkyv_bytes(&self) -> Vec<u8> {
        rkyv::to_bytes::<_, 2048>(self).unwrap().to_vec()
    }

    /// 零拷贝反序列化
    pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedDistributionMessage, String> {
        rkyv::check_archived_root::<DistributionMessage>(bytes)
            .map_err(|e| format!("Deserialization failed: {}", e))
    }
}
```

### 3. Publisher 实现

```rust
// src/distribution/publisher.rs

use iceoryx2::prelude::*;
use std::sync::Arc;
use parking_lot::Mutex;

pub struct DistributionPublisher {
    service: Arc<Mutex<ipc::Service<DistributionMessage>>>,
    publisher_id: String,
    sequence: Arc<AtomicU64>,
}

impl DistributionPublisher {
    pub fn new(topic: &str, publisher_id: &str) -> Result<Self, String> {
        let service_name = ServiceName::new(topic)
            .map_err(|e| format!("Invalid topic: {}", e))?;

        let service = zero_copy::Service::new(&service_name)
            .publish_subscribe()
            .max_publishers(10)
            .max_subscribers(1000)
            .subscriber_max_buffer_size(1000)
            .enable_safe_overflow(true)  // 覆盖旧数据，不阻塞
            .create::<[u8]>()
            .map_err(|e| format!("Create service failed: {}", e))?;

        let publisher = service.publisher()
            .max_slice_len(2048)
            .create()
            .map_err(|e| format!("Create publisher failed: {}", e))?;

        Ok(Self {
            service: Arc::new(Mutex::new(service)),
            publisher_id: publisher_id.to_string(),
            sequence: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 发布消息（零拷贝）
    pub fn publish(&self, msg: DistributionMessage) -> Result<(), String> {
        let bytes = msg.to_rkyv_bytes();

        let service = self.service.lock();
        let publisher = service.publisher()
            .max_slice_len(bytes.len())
            .create()
            .map_err(|e| format!("Get publisher failed: {}", e))?;

        // 零拷贝写入共享内存
        let mut sample = publisher.loan_slice_uninit(bytes.len())
            .map_err(|e| format!("Loan failed: {}", e))?;

        sample.copy_from_slice(&bytes);

        sample.send()
            .map_err(|e| format!("Send failed: {}", e))?;

        self.sequence.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 批量发布（减少锁竞争）
    pub fn publish_batch(&self, messages: Vec<DistributionMessage>) -> Result<(), String> {
        let service = self.service.lock();
        let publisher = service.publisher()
            .max_slice_len(2048)
            .create()?;

        for msg in messages {
            let bytes = msg.to_rkyv_bytes();

            let mut sample = publisher.loan_slice_uninit(bytes.len())?;
            sample.copy_from_slice(&bytes);
            sample.send()?;
        }

        Ok(())
    }

    /// 发送心跳
    pub fn send_heartbeat(&self) -> Result<(), String> {
        let sequence = self.sequence.load(Ordering::Relaxed);

        let heartbeat = DistributionMessage::Heartbeat {
            publisher_id: {
                let mut id = [0u8; 16];
                let bytes = self.publisher_id.as_bytes();
                let len = bytes.len().min(16);
                id[..len].copy_from_slice(&bytes[..len]);
                id
            },
            sequence,
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        self.publish(heartbeat)
    }
}
```

### 4. Subscriber 实现

```rust
// src/distribution/subscriber.rs

pub struct DistributionSubscriber {
    service: Arc<ipc::Service<[u8]>>,
    subscriber: ipc::Subscriber<[u8]>,
    subscriber_id: String,
    callback: Arc<dyn Fn(DistributionMessage) + Send + Sync>,
}

impl DistributionSubscriber {
    pub fn new<F>(topic: &str, subscriber_id: &str, callback: F) -> Result<Self, String>
    where
        F: Fn(DistributionMessage) + Send + Sync + 'static,
    {
        let service_name = ServiceName::new(topic)?;

        let service = zero_copy::Service::new(&service_name)
            .publish_subscribe()
            .open_or_create::<[u8]>()?;

        let subscriber = service.subscriber()
            .create()?;

        Ok(Self {
            service: Arc::new(service),
            subscriber,
            subscriber_id: subscriber_id.to_string(),
            callback: Arc::new(callback),
        })
    }

    /// 启动接收循环
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                // 零拷贝接收
                while let Some(sample) = self.subscriber.receive().unwrap() {
                    let bytes = &*sample;

                    // 零拷贝反序列化
                    match DistributionMessage::from_rkyv_bytes(bytes) {
                        Ok(archived_msg) => {
                            // 转换为 owned
                            let msg: DistributionMessage = archived_msg
                                .deserialize(&mut rkyv::Infallible)
                                .unwrap();

                            (self.callback)(msg);
                        }
                        Err(e) => {
                            log::error!("Deserialize failed: {}", e);
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
            }
        })
    }

    /// 启动接收循环（阻塞版本）
    pub fn run(self) {
        loop {
            while let Some(sample) = self.subscriber.receive().unwrap() {
                let bytes = &*sample;

                match DistributionMessage::from_rkyv_bytes(bytes) {
                    Ok(archived_msg) => {
                        let msg: DistributionMessage = archived_msg
                            .deserialize(&mut rkyv::Infallible)
                            .unwrap();

                        (self.callback)(msg);
                    }
                    Err(e) => {
                        log::error!("Deserialize failed: {}", e);
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_micros(10));
        }
    }
}
```

---

## 多级订阅系统

### 订阅级别

| 级别 | 延迟 | 用途 | 实现方式 |
|------|------|------|---------|
| **Real-time** | P99 < 10μs | WebSocket 实时推送 | iceoryx2 零拷贝 |
| **Delayed** | ~100ms | 批量处理、风控 | 批量聚合后推送 |
| **Historical** | 秒级 | 历史查询、回测 | WAL Replay |

### Real-time 订阅

```rust
// src/distribution/realtime_subscriber.rs

pub struct RealtimeSubscriber {
    inner: DistributionSubscriber,
    websocket_sessions: Arc<DashMap<String, WebSocketSession>>,
}

impl RealtimeSubscriber {
    pub fn new(topic: &str) -> Self {
        let sessions = Arc::new(DashMap::new());
        let sessions_clone = sessions.clone();

        let subscriber = DistributionSubscriber::new(
            topic,
            "realtime",
            move |msg| {
                // 立即推送到所有 WebSocket 会话
                for session in sessions_clone.iter() {
                    session.send(msg.clone()).ok();
                }
            }
        ).unwrap();

        Self {
            inner: subscriber,
            websocket_sessions: sessions,
        }
    }

    pub fn register_session(&self, session_id: String, session: WebSocketSession) {
        self.websocket_sessions.insert(session_id, session);
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        self.inner.start()
    }
}
```

### Delayed 订阅（批量处理）

```rust
// src/distribution/delayed_subscriber.rs

pub struct DelayedSubscriber {
    inner: DistributionSubscriber,
    batch_buffer: Arc<Mutex<Vec<DistributionMessage>>>,
    batch_size: usize,
    batch_interval: Duration,
}

impl DelayedSubscriber {
    pub fn new(topic: &str, batch_size: usize, batch_interval: Duration) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(batch_size)));
        let buffer_clone = buffer.clone();

        let subscriber = DistributionSubscriber::new(
            topic,
            "delayed",
            move |msg| {
                let mut buf = buffer_clone.lock();
                buf.push(msg);
            }
        ).unwrap();

        Self {
            inner: subscriber,
            batch_buffer: buffer,
            batch_size,
            batch_interval,
        }
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        // 启动接收循环
        let recv_handle = self.inner.start();

        // 启动批量处理循环
        let buffer = self.batch_buffer.clone();
        let batch_size = self.batch_size;
        let interval = self.batch_interval;

        let batch_handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                let mut buf = buffer.lock();

                if !buf.is_empty() {
                    let batch: Vec<_> = buf.drain(..).collect();

                    // 处理批量数据
                    Self::process_batch(batch).await;
                }
            }
        });

        recv_handle
    }

    async fn process_batch(batch: Vec<DistributionMessage>) {
        log::info!("Processing batch of {} messages", batch.len());

        // 批量写入数据库、风控检查等
        for msg in batch {
            // TODO: 批量处理逻辑
        }
    }
}
```

### Historical 订阅（WAL Replay）

```rust
// src/distribution/historical_subscriber.rs

pub struct HistoricalSubscriber {
    wal_manager: Arc<WalManager>,
}

impl HistoricalSubscriber {
    pub fn new(wal_path: &str) -> Self {
        Self {
            wal_manager: Arc::new(WalManager::new(wal_path)),
        }
    }

    /// 回放历史数据
    pub async fn replay<F>(
        &self,
        start_sequence: u64,
        end_sequence: Option<u64>,
        callback: F,
    ) -> Result<(), String>
    where
        F: Fn(DistributionMessage) + Send + Sync,
    {
        self.wal_manager.replay(|entry| {
            if entry.sequence < start_sequence {
                return Ok(());
            }

            if let Some(end_seq) = end_sequence {
                if entry.sequence > end_seq {
                    return Err("Reached end sequence".to_string());
                }
            }

            // 转换 WalRecord → DistributionMessage
            let msg = Self::wal_to_distribution_msg(&entry.record)?;

            callback(msg);

            Ok(())
        })
    }

    fn wal_to_distribution_msg(record: &WalRecord) -> Result<DistributionMessage, String> {
        match record {
            WalRecord::TradeExecuted { trade_id, order_id, price, volume, timestamp, .. } => {
                Ok(DistributionMessage::TradeEvent {
                    trade_id: *trade_id,
                    order_id: *order_id,
                    instrument_id: [0u8; 16],  // TODO: 从 WAL 中提取
                    price: *price,
                    volume: *volume,
                    direction: 0,
                    timestamp: *timestamp,
                })
            }
            _ => Err("Unsupported WAL record type".to_string()),
        }
    }
}
```

---

## 可靠性保证

### 1. 确认机制（ACK）

```rust
// src/distribution/reliable_publisher.rs

pub struct ReliablePublisher {
    inner: DistributionPublisher,
    pending_acks: Arc<DashMap<u64, PendingMessage>>,  // sequence → message
    retry_timeout: Duration,
}

struct PendingMessage {
    message: DistributionMessage,
    sent_at: Instant,
    retry_count: u32,
}

impl ReliablePublisher {
    /// 发布消息（等待确认）
    pub async fn publish_reliable(&self, msg: DistributionMessage) -> Result<u64, String> {
        let sequence = self.inner.sequence.fetch_add(1, Ordering::SeqCst);

        // 记录待确认
        self.pending_acks.insert(sequence, PendingMessage {
            message: msg.clone(),
            sent_at: Instant::now(),
            retry_count: 0,
        });

        // 发布
        self.inner.publish(msg)?;

        // 等待确认（超时重发）
        self.wait_for_ack(sequence).await
    }

    async fn wait_for_ack(&self, sequence: u64) -> Result<u64, String> {
        let timeout = tokio::time::sleep(self.retry_timeout);
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    // 超时，重发
                    if let Some(mut pending) = self.pending_acks.get_mut(&sequence) {
                        pending.retry_count += 1;

                        if pending.retry_count > 3 {
                            return Err("Max retries exceeded".to_string());
                        }

                        log::warn!("Retry sequence {}, count: {}", sequence, pending.retry_count);
                        self.inner.publish(pending.message.clone())?;
                    }
                }
            }

            // 检查是否已确认
            if !self.pending_acks.contains_key(&sequence) {
                return Ok(sequence);
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// 收到确认
    pub fn on_ack(&self, sequence: u64) {
        self.pending_acks.remove(&sequence);
    }
}

// Subscriber 发送 ACK
impl DistributionSubscriber {
    pub fn send_ack(&self, sequence: u64) -> Result<(), String> {
        let ack = DistributionMessage::Ack {
            sequence,
            subscriber_id: {
                let mut id = [0u8; 16];
                let bytes = self.subscriber_id.as_bytes();
                let len = bytes.len().min(16);
                id[..len].copy_from_slice(&bytes[..len]);
                id
            },
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        // 通过反向通道发送 ACK
        // TODO: 实现 ACK 通道
        Ok(())
    }
}
```

### 2. 断点续传

```rust
// src/distribution/resumable_subscriber.rs

pub struct ResumableSubscriber {
    inner: DistributionSubscriber,
    checkpoint_manager: Arc<CheckpointManager>,
    last_sequence: Arc<AtomicU64>,
}

impl ResumableSubscriber {
    pub fn new(topic: &str, checkpoint_path: &str) -> Self {
        let checkpoint_mgr = Arc::new(CheckpointManager::new(checkpoint_path));

        // 恢复上次的 sequence
        let last_seq = checkpoint_mgr.load().unwrap_or(0);

        let last_sequence = Arc::new(AtomicU64::new(last_seq));
        let last_seq_clone = last_sequence.clone();
        let checkpoint_mgr_clone = checkpoint_mgr.clone();

        let subscriber = DistributionSubscriber::new(
            topic,
            "resumable",
            move |msg| {
                // 更新 sequence
                if let Some(seq) = Self::extract_sequence(&msg) {
                    last_seq_clone.store(seq, Ordering::Relaxed);

                    // 定期保存 checkpoint
                    if seq % 1000 == 0 {
                        checkpoint_mgr_clone.save(seq).ok();
                    }
                }

                // 处理消息
                Self::process_message(msg);
            }
        ).unwrap();

        Self {
            inner: subscriber,
            checkpoint_manager: checkpoint_mgr,
            last_sequence,
        }
    }

    /// 从断点恢复
    pub async fn resume(&self) -> Result<(), String> {
        let last_seq = self.last_sequence.load(Ordering::Relaxed);

        if last_seq > 0 {
            log::info!("Resuming from sequence {}", last_seq);

            // 从 WAL 重放缺失的消息
            let historical = HistoricalSubscriber::new("/data/wal");
            historical.replay(last_seq + 1, None, |msg| {
                Self::process_message(msg);
            }).await?;
        }

        Ok(())
    }

    fn extract_sequence(msg: &DistributionMessage) -> Option<u64> {
        // TODO: 从消息中提取 sequence
        None
    }

    fn process_message(msg: DistributionMessage) {
        // TODO: 处理逻辑
    }
}

struct CheckpointManager {
    path: String,
}

impl CheckpointManager {
    fn new(path: &str) -> Self {
        std::fs::create_dir_all(path).unwrap();
        Self { path: path.to_string() }
    }

    fn save(&self, sequence: u64) -> Result<(), String> {
        let checkpoint_file = format!("{}/checkpoint", self.path);
        std::fs::write(&checkpoint_file, sequence.to_string())
            .map_err(|e| format!("Save checkpoint failed: {}", e))
    }

    fn load(&self) -> Result<u64, String> {
        let checkpoint_file = format!("{}/checkpoint", self.path);

        match std::fs::read_to_string(&checkpoint_file) {
            Ok(content) => content.parse::<u64>()
                .map_err(|e| format!("Parse checkpoint failed: {}", e)),
            Err(_) => Ok(0),
        }
    }
}
```

### 3. 故障检测

```rust
// src/distribution/health_monitor.rs

pub struct HealthMonitor {
    publishers: Arc<DashMap<String, PublisherHealth>>,
    heartbeat_timeout: Duration,
}

struct PublisherHealth {
    last_heartbeat: Instant,
    sequence: u64,
    status: PublisherStatus,
}

#[derive(Debug, Clone, Copy)]
enum PublisherStatus {
    Healthy,
    Degraded,   // 心跳延迟
    Failed,     // 超时
}

impl HealthMonitor {
    pub fn new(heartbeat_timeout: Duration) -> Self {
        Self {
            publishers: Arc::new(DashMap::new()),
            heartbeat_timeout,
        }
    }

    /// 处理心跳
    pub fn on_heartbeat(&self, publisher_id: &str, sequence: u64) {
        self.publishers.entry(publisher_id.to_string())
            .and_modify(|health| {
                health.last_heartbeat = Instant::now();
                health.sequence = sequence;
                health.status = PublisherStatus::Healthy;
            })
            .or_insert(PublisherHealth {
                last_heartbeat: Instant::now(),
                sequence,
                status: PublisherStatus::Healthy,
            });
    }

    /// 启动健康检查
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));

            loop {
                ticker.tick().await;

                for mut entry in self.publishers.iter_mut() {
                    let elapsed = entry.last_heartbeat.elapsed();

                    if elapsed > self.heartbeat_timeout * 2 {
                        entry.status = PublisherStatus::Failed;
                        log::error!("Publisher {} failed (no heartbeat for {:?})",
                            entry.key(), elapsed);
                    } else if elapsed > self.heartbeat_timeout {
                        entry.status = PublisherStatus::Degraded;
                        log::warn!("Publisher {} degraded (heartbeat delayed {:?})",
                            entry.key(), elapsed);
                    }
                }
            }
        })
    }

    /// 获取所有 Publisher 状态
    pub fn get_status(&self) -> Vec<(String, PublisherStatus, u64)> {
        self.publishers.iter()
            .map(|entry| {
                (entry.key().clone(), entry.status, entry.sequence)
            })
            .collect()
    }
}
```

---

## 性能优化

### 1. 批量处理

```rust
// 批量发布（减少系统调用）
publisher.publish_batch(vec![msg1, msg2, msg3])?;

// 批量接收（减少上下文切换）
while let Some(samples) = subscriber.receive_batch(100).unwrap() {
    for sample in samples {
        process(sample);
    }
}
```

### 2. CPU 亲和性

```rust
// 绑定 Publisher 线程到特定 CPU
use core_affinity;

let core_ids = core_affinity::get_core_ids().unwrap();

thread::Builder::new()
    .name("Publisher".to_string())
    .spawn(move || {
        // 绑定到 CPU 0
        core_affinity::set_for_current(core_ids[0]);

        publisher.run();
    })
    .unwrap();
```

### 3. 预分配内存

```rust
// 预分配 Ring Buffer
let service = zero_copy::Service::new(&service_name)
    .publish_subscribe()
    .subscriber_max_buffer_size(10000)  // 10000 个消息
    .enable_safe_overflow(true)         // 覆盖旧数据
    .create::<[u8]>()?;
```

### 4. Lock-Free 数据结构

```rust
// 使用 DashMap 替代 Mutex<HashMap>
use dashmap::DashMap;

let sessions: DashMap<String, WebSocketSession> = DashMap::new();

// 无锁并发访问
sessions.insert(id, session);
sessions.get(&id);
```

---

## 性能目标

| 指标 | 目标 | 实现方式 |
|------|------|---------|
| **分发延迟** | P99 < 10μs | iceoryx2 零拷贝 |
| **吞吐量** | > 10M msg/s | 共享内存 + 批量处理 |
| **可靠性** | 99.99% | ACK 确认 + 断点续传 |
| **故障恢复** | < 5s | WAL Replay |
| **内存占用** | < 500MB | Ring Buffer 覆盖 |

---

## 相关链接

- [存储架构设计](01_STORAGE_ARCHITECTURE.md)
- [故障恢复设计](03_RECOVERY_DESIGN.md)
- [实施计划](04_IMPLEMENTATION_PLAN.md)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
