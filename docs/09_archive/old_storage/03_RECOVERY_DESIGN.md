# 故障恢复和高可用设计

> 数据恢复、故障转移、主从复制

**版本**: v1.0.0
**最后更新**: 2025-10-03

---

## 📋 目录

- [恢复策略](#恢复策略)
- [主从复制](#主从复制)
- [故障检测和转移](#故障检测和转移)
- [数据一致性](#数据一致性)
- [灾难恢复](#灾难恢复)

---

## 恢复策略

### 1. WAL 回放恢复

**场景**：服务崩溃重启

**流程**：

```
1. 启动服务
2. 加载 Checkpoint → 找到最后一个 SSTable 的 sequence
3. 从 Checkpoint 之后的 WAL 开始回放
4. 重建 MemTable
5. 恢复完成，开始提供服务
```

**实现**：

```rust
// src/storage/recovery/wal_recovery.rs

use std::sync::Arc;
use parking_lot::RwLock;

pub struct WalRecovery {
    wal_manager: Arc<WalManager>,
    memtable_manager: Arc<MemTableManager>,
    checkpoint_manager: Arc<CheckpointManager>,
}

impl WalRecovery {
    pub fn new(
        wal_path: &str,
        checkpoint_path: &str,
        memtable_manager: Arc<MemTableManager>,
    ) -> Self {
        Self {
            wal_manager: Arc::new(WalManager::new(wal_path)),
            memtable_manager,
            checkpoint_manager: Arc::new(CheckpointManager::new(checkpoint_path)),
        }
    }

    /// 执行恢复
    pub async fn recover(&self) -> Result<RecoveryStats, String> {
        log::info!("Starting WAL recovery...");

        let start = Instant::now();
        let mut stats = RecoveryStats::default();

        // 1. 加载 Checkpoint
        let checkpoint_seq = self.checkpoint_manager.load().unwrap_or(0);
        log::info!("Loaded checkpoint: sequence={}", checkpoint_seq);

        // 2. 回放 WAL
        self.wal_manager.replay(|entry| {
            // 跳过已经持久化的条目
            if entry.sequence <= checkpoint_seq {
                stats.skipped += 1;
                return Ok(());
            }

            // 重建 MemTable
            self.apply_wal_entry(&entry)?;
            stats.replayed += 1;

            if stats.replayed % 10000 == 0 {
                log::info!("Replayed {} entries...", stats.replayed);
            }

            Ok(())
        })?;

        stats.duration = start.elapsed();

        log::info!("WAL recovery completed: replayed={}, skipped={}, duration={:?}",
            stats.replayed, stats.skipped, stats.duration);

        Ok(stats)
    }

    /// 应用 WAL 条目到 MemTable
    fn apply_wal_entry(&self, entry: &WalEntry) -> Result<(), String> {
        match &entry.record {
            WalRecord::OrderInsert { order_id, user_id, instrument_id, price, volume, .. } => {
                // 构造 key-value
                let key = Self::order_key(order_id);
                let value = Self::order_value(user_id, instrument_id, *price, *volume);

                self.memtable_manager.insert(key, value)?;
            }

            WalRecord::TradeExecuted { trade_id, order_id, price, volume, .. } => {
                let key = Self::trade_key(trade_id);
                let value = Self::trade_value(order_id, *price, *volume);

                self.memtable_manager.insert(key, value)?;
            }

            WalRecord::AccountUpdate { user_id, balance, available, frozen, margin, .. } => {
                let key = Self::account_key(user_id);
                let value = Self::account_value(*balance, *available, *frozen, *margin);

                self.memtable_manager.insert(key, value)?;
            }

            WalRecord::Checkpoint { .. } => {
                // Checkpoint 记录不需要重放
            }
        }

        Ok(())
    }

    fn order_key(order_id: &[u8; 40]) -> Vec<u8> {
        let mut key = Vec::with_capacity(41);
        key.push(b'O');  // 'O' for Order
        key.extend_from_slice(order_id);
        key
    }

    fn order_value(user_id: &[u8; 32], instrument_id: &[u8; 16], price: f64, volume: f64) -> Vec<u8> {
        // rkyv 序列化
        let data = OrderData {
            user_id: *user_id,
            instrument_id: *instrument_id,
            price,
            volume,
        };

        rkyv::to_bytes::<_, 256>(&data).unwrap().to_vec()
    }

    fn trade_key(trade_id: &[u8; 40]) -> Vec<u8> {
        let mut key = Vec::with_capacity(41);
        key.push(b'T');  // 'T' for Trade
        key.extend_from_slice(trade_id);
        key
    }

    fn trade_value(order_id: &[u8; 40], price: f64, volume: f64) -> Vec<u8> {
        let data = TradeData {
            order_id: *order_id,
            price,
            volume,
        };

        rkyv::to_bytes::<_, 256>(&data).unwrap().to_vec()
    }

    fn account_key(user_id: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(33);
        key.push(b'A');  // 'A' for Account
        key.extend_from_slice(user_id);
        key
    }

    fn account_value(balance: f64, available: f64, frozen: f64, margin: f64) -> Vec<u8> {
        let data = AccountData {
            balance,
            available,
            frozen,
            margin,
        };

        rkyv::to_bytes::<_, 256>(&data).unwrap().to_vec()
    }
}

#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub replayed: usize,
    pub skipped: usize,
    pub duration: Duration,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
struct OrderData {
    user_id: [u8; 32],
    instrument_id: [u8; 16],
    price: f64,
    volume: f64,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
struct TradeData {
    order_id: [u8; 40],
    price: f64,
    volume: f64,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
struct AccountData {
    balance: f64,
    available: f64,
    frozen: f64,
    margin: f64,
}
```

### 2. SSTable 恢复

**场景**：MemTable 丢失，但 SSTable 完整

**流程**：

```
1. 扫描 SSTable 目录，加载所有 SSTable
2. 构建 LSM-Tree 结构
3. 无需回放 WAL（SSTable 已持久化）
4. 开始提供服务
```

**实现**：

```rust
// src/storage/recovery/sstable_recovery.rs

pub struct SSTableRecovery {
    sstable_dir: String,
    levels: Vec<Vec<Arc<SSTableReader>>>,
}

impl SSTableRecovery {
    pub fn new(sstable_dir: &str) -> Self {
        Self {
            sstable_dir: sstable_dir.to_string(),
            levels: vec![Vec::new(); 7],  // 7 级
        }
    }

    /// 扫描并加载 SSTable
    pub fn recover(&mut self) -> Result<(), String> {
        log::info!("Starting SSTable recovery...");

        // 扫描目录
        let files = self.scan_sstable_files()?;

        for file_path in files {
            // 解析文件名: sst_{level}_{sequence}.sst
            let (level, sequence) = self.parse_filename(&file_path)?;

            // 打开 SSTable
            let reader = Arc::new(SSTableReader::open(&file_path)?);

            self.levels[level].push(reader);
        }

        // 按 sequence 排序
        for level in &mut self.levels {
            level.sort_by_key(|sst| sst.min_sequence());
        }

        log::info!("SSTable recovery completed: {} levels loaded", self.levels.len());

        Ok(())
    }

    fn scan_sstable_files(&self) -> Result<Vec<String>, String> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(&self.sstable_dir)
            .map_err(|e| format!("Read dir failed: {}", e))?
        {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("sst") {
                files.push(path.to_str().unwrap().to_string());
            }
        }

        files.sort();
        Ok(files)
    }

    fn parse_filename(&self, file_path: &str) -> Result<(usize, u64), String> {
        // sst_0_00000000000000000001.sst → (0, 1)
        let filename = std::path::Path::new(file_path)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();

        let parts: Vec<&str> = filename.split('_').collect();

        if parts.len() != 3 {
            return Err(format!("Invalid SSTable filename: {}", filename));
        }

        let level = parts[1].parse::<usize>()
            .map_err(|e| format!("Parse level failed: {}", e))?;

        let sequence = parts[2].parse::<u64>()
            .map_err(|e| format!("Parse sequence failed: {}", e))?;

        Ok((level, sequence))
    }
}
```

### 3. 快速恢复（Snapshot）

**场景**：加速大规模数据恢复

**流程**：

```
1. 定期生成 Snapshot（MemTable + SSTable 的完整快照）
2. 崩溃恢复时：
   - 加载最新 Snapshot
   - 只回放 Snapshot 之后的 WAL
3. 恢复时间：10GB 数据 < 10s
```

**实现**：

```rust
// src/storage/recovery/snapshot.rs

pub struct SnapshotManager {
    snapshot_dir: String,
    interval: Duration,
}

impl SnapshotManager {
    pub fn new(snapshot_dir: &str, interval: Duration) -> Self {
        std::fs::create_dir_all(snapshot_dir).unwrap();

        Self {
            snapshot_dir: snapshot_dir.to_string(),
            interval,
        }
    }

    /// 生成快照
    pub async fn create_snapshot(
        &self,
        memtable: &MemTableManager,
        sstables: &Vec<Vec<Arc<SSTableReader>>>,
        sequence: u64,
    ) -> Result<(), String> {
        log::info!("Creating snapshot at sequence {}", sequence);

        let snapshot_file = format!("{}/snapshot_{:020}.snap", self.snapshot_dir, sequence);
        let mut file = File::create(&snapshot_file)
            .map_err(|e| format!("Create snapshot failed: {}", e))?;

        // 写入 Header
        let header = SnapshotHeader {
            version: 1,
            sequence,
            timestamp: chrono::Utc::now().timestamp(),
            memtable_size: memtable.size(),
            sstable_count: sstables.iter().map(|level| level.len()).sum(),
        };

        let header_bytes = rkyv::to_bytes::<_, 256>(&header).unwrap();
        file.write_all(&header_bytes.len().to_le_bytes())?;
        file.write_all(&header_bytes)?;

        // 写入 MemTable 数据
        for (key, value) in memtable.iter() {
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(&key)?;
            file.write_all(&(value.len() as u32).to_le_bytes())?;
            file.write_all(&value)?;
        }

        // 写入 SSTable 元数据
        for (level, sstable_list) in sstables.iter().enumerate() {
            for sst in sstable_list {
                let meta = SSTableMeta {
                    level: level as u8,
                    file_path: sst.file_path.clone(),
                    min_key: sst.min_key.clone(),
                    max_key: sst.max_key.clone(),
                    entry_count: sst.entry_count,
                };

                let meta_bytes = rkyv::to_bytes::<_, 512>(&meta).unwrap();
                file.write_all(&meta_bytes.len().to_le_bytes())?;
                file.write_all(&meta_bytes)?;
            }
        }

        file.sync_all()?;

        log::info!("Snapshot created: {}", snapshot_file);
        Ok(())
    }

    /// 加载快照
    pub async fn load_snapshot(&self) -> Result<SnapshotData, String> {
        // 找到最新的快照
        let snapshot_file = self.find_latest_snapshot()?;

        log::info!("Loading snapshot: {}", snapshot_file);

        let mut file = File::open(&snapshot_file)
            .map_err(|e| format!("Open snapshot failed: {}", e))?;

        // 读取 Header
        let mut header_len_buf = [0u8; 8];
        file.read_exact(&mut header_len_buf)?;
        let header_len = usize::from_le_bytes(header_len_buf);

        let mut header_buf = vec![0u8; header_len];
        file.read_exact(&mut header_buf)?;

        let archived_header = rkyv::check_archived_root::<SnapshotHeader>(&header_buf)
            .map_err(|e| format!("Deserialize header failed: {}", e))?;

        let header: SnapshotHeader = archived_header.deserialize(&mut rkyv::Infallible).unwrap();

        // 读取 MemTable 数据
        let mut memtable_data = Vec::new();

        for _ in 0..header.memtable_size {
            let mut key_len_buf = [0u8; 4];
            file.read_exact(&mut key_len_buf)?;
            let key_len = u32::from_le_bytes(key_len_buf) as usize;

            let mut key = vec![0u8; key_len];
            file.read_exact(&mut key)?;

            let mut value_len_buf = [0u8; 4];
            file.read_exact(&mut value_len_buf)?;
            let value_len = u32::from_le_bytes(value_len_buf) as usize;

            let mut value = vec![0u8; value_len];
            file.read_exact(&mut value)?;

            memtable_data.push((key, value));
        }

        // 读取 SSTable 元数据
        let mut sstable_metas = Vec::new();

        for _ in 0..header.sstable_count {
            let mut meta_len_buf = [0u8; 8];
            file.read_exact(&mut meta_len_buf)?;
            let meta_len = usize::from_le_bytes(meta_len_buf);

            let mut meta_buf = vec![0u8; meta_len];
            file.read_exact(&mut meta_buf)?;

            let archived_meta = rkyv::check_archived_root::<SSTableMeta>(&meta_buf)?;
            let meta: SSTableMeta = archived_meta.deserialize(&mut rkyv::Infallible).unwrap();

            sstable_metas.push(meta);
        }

        log::info!("Snapshot loaded: sequence={}, memtable_size={}, sstable_count={}",
            header.sequence, header.memtable_size, header.sstable_count);

        Ok(SnapshotData {
            header,
            memtable_data,
            sstable_metas,
        })
    }

    fn find_latest_snapshot(&self) -> Result<String, String> {
        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(&self.snapshot_dir)
            .map_err(|e| format!("Read dir failed: {}", e))?
        {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("snap") {
                snapshots.push(path.to_str().unwrap().to_string());
            }
        }

        snapshots.sort();
        snapshots.last()
            .cloned()
            .ok_or_else(|| "No snapshot found".to_string())
    }

    /// 启动自动快照
    pub fn start_auto_snapshot(
        self: Arc<Self>,
        memtable: Arc<MemTableManager>,
        sstables: Arc<RwLock<Vec<Vec<Arc<SSTableReader>>>>>,
        sequence: Arc<AtomicU64>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(self.interval);

            loop {
                ticker.tick().await;

                let seq = sequence.load(Ordering::Relaxed);
                let sstables_clone = sstables.read().clone();

                if let Err(e) = self.create_snapshot(&memtable, &sstables_clone, seq).await {
                    log::error!("Create snapshot failed: {}", e);
                }
            }
        })
    }
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
struct SnapshotHeader {
    version: u32,
    sequence: u64,
    timestamp: i64,
    memtable_size: usize,
    sstable_count: usize,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
struct SSTableMeta {
    level: u8,
    file_path: String,
    min_key: Vec<u8>,
    max_key: Vec<u8>,
    entry_count: u64,
}

pub struct SnapshotData {
    pub header: SnapshotHeader,
    pub memtable_data: Vec<(Vec<u8>, Vec<u8>)>,
    pub sstable_metas: Vec<SSTableMeta>,
}
```

---

## 主从复制

### 架构

```
┌─────────────┐     WAL Stream      ┌─────────────┐
│   Master    │ ─────────────────→  │   Slave 1   │
│             │                      │             │
│  - 接受写入  │                      │  - 只读查询  │
│  - 生成 WAL │                      │  - WAL Replay│
└─────────────┘                      └─────────────┘
       │                                    ↑
       │         WAL Stream                 │
       └────────────────────────────────────┘
                                      ┌─────────────┐
                                      │   Slave 2   │
                                      │             │
                                      │  - 只读查询  │
                                      │  - WAL Replay│
                                      └─────────────┘
```

### 实现

```rust
// src/storage/replication/master.rs

pub struct ReplicationMaster {
    wal_manager: Arc<WalManager>,
    slaves: Arc<DashMap<String, SlaveConnection>>,
    replication_lag: Arc<DashMap<String, Duration>>,
}

struct SlaveConnection {
    id: String,
    tx: mpsc::UnboundedSender<WalEntry>,
    last_ack_sequence: Arc<AtomicU64>,
    last_ack_time: Arc<RwLock<Instant>>,
}

impl ReplicationMaster {
    pub fn new(wal_manager: Arc<WalManager>) -> Self {
        Self {
            wal_manager,
            slaves: Arc::new(DashMap::new()),
            replication_lag: Arc::new(DashMap::new()),
        }
    }

    /// 注册 Slave
    pub fn register_slave(&self, slave_id: &str, tx: mpsc::UnboundedSender<WalEntry>) {
        self.slaves.insert(slave_id.to_string(), SlaveConnection {
            id: slave_id.to_string(),
            tx,
            last_ack_sequence: Arc::new(AtomicU64::new(0)),
            last_ack_time: Arc::new(RwLock::new(Instant::now())),
        });

        log::info!("Slave registered: {}", slave_id);
    }

    /// 复制 WAL 到所有 Slave
    pub fn replicate(&self, entry: WalEntry) -> Result<(), String> {
        for slave in self.slaves.iter() {
            slave.tx.send(entry.clone())
                .map_err(|e| format!("Send to slave {} failed: {}", slave.id, e))?;
        }

        Ok(())
    }

    /// 收到 Slave ACK
    pub fn on_slave_ack(&self, slave_id: &str, sequence: u64) {
        if let Some(slave) = self.slaves.get(slave_id) {
            slave.last_ack_sequence.store(sequence, Ordering::Relaxed);
            *slave.last_ack_time.write() = Instant::now();

            // 计算复制延迟
            let lag = Instant::now().duration_since(*slave.last_ack_time.read());
            self.replication_lag.insert(slave_id.to_string(), lag);
        }
    }

    /// 获取所有 Slave 的复制进度
    pub fn get_replication_status(&self) -> Vec<(String, u64, Duration)> {
        self.slaves.iter()
            .map(|entry| {
                let slave = entry.value();
                let sequence = slave.last_ack_sequence.load(Ordering::Relaxed);
                let lag = self.replication_lag.get(&slave.id)
                    .map(|l| *l.value())
                    .unwrap_or(Duration::from_secs(0));

                (slave.id.clone(), sequence, lag)
            })
            .collect()
    }

    /// 等待所有 Slave 复制完成（同步复制）
    pub async fn wait_for_replication(&self, sequence: u64, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();

        loop {
            let all_synced = self.slaves.iter()
                .all(|entry| {
                    entry.last_ack_sequence.load(Ordering::Relaxed) >= sequence
                });

            if all_synced {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err("Replication timeout".to_string());
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

// src/storage/replication/slave.rs

pub struct ReplicationSlave {
    master_addr: String,
    wal_manager: Arc<WalManager>,
    memtable_manager: Arc<MemTableManager>,
    last_applied_sequence: Arc<AtomicU64>,
}

impl ReplicationSlave {
    pub fn new(
        master_addr: &str,
        wal_manager: Arc<WalManager>,
        memtable_manager: Arc<MemTableManager>,
    ) -> Self {
        Self {
            master_addr: master_addr.to_string(),
            wal_manager,
            memtable_manager,
            last_applied_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 启动复制
    pub async fn start(self: Arc<Self>) -> Result<(), String> {
        log::info!("Connecting to master: {}", self.master_addr);

        // 连接到 Master
        let (tx, mut rx) = mpsc::unbounded_channel::<WalEntry>();

        // TODO: 建立到 Master 的连接，注册自己

        // 接收 WAL 流
        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                if let Err(e) = self.apply_wal_entry(&entry).await {
                    log::error!("Apply WAL failed: {}", e);
                    continue;
                }

                self.last_applied_sequence.store(entry.sequence, Ordering::Relaxed);

                // 发送 ACK
                self.send_ack(entry.sequence).await.ok();
            }
        });

        Ok(())
    }

    async fn apply_wal_entry(&self, entry: &WalEntry) -> Result<(), String> {
        // 1. 写入本地 WAL
        self.wal_manager.append(entry.record.clone())?;

        // 2. 应用到 MemTable
        self.replay_to_memtable(entry)?;

        Ok(())
    }

    fn replay_to_memtable(&self, entry: &WalEntry) -> Result<(), String> {
        // 与 WalRecovery::apply_wal_entry 相同的逻辑
        match &entry.record {
            WalRecord::OrderInsert { .. } => {
                // TODO: 应用到 MemTable
            }
            _ => {}
        }

        Ok(())
    }

    async fn send_ack(&self, sequence: u64) -> Result<(), String> {
        // TODO: 发送 ACK 到 Master
        Ok(())
    }
}
```

---

## 故障检测和转移

### 1. 故障检测

```rust
// src/storage/failover/detector.rs

pub struct FailureDetector {
    nodes: Arc<DashMap<String, NodeHealth>>,
    heartbeat_interval: Duration,
    failure_threshold: Duration,
}

struct NodeHealth {
    role: NodeRole,
    last_heartbeat: Instant,
    status: NodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NodeRole {
    Master,
    Slave,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NodeStatus {
    Healthy,
    Degraded,
    Failed,
}

impl FailureDetector {
    pub fn new(heartbeat_interval: Duration, failure_threshold: Duration) -> Self {
        Self {
            nodes: Arc::new(DashMap::new()),
            heartbeat_interval,
            failure_threshold,
        }
    }

    /// 启动故障检测
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(self.heartbeat_interval);

            loop {
                ticker.tick().await;

                for mut entry in self.nodes.iter_mut() {
                    let elapsed = entry.last_heartbeat.elapsed();

                    if elapsed > self.failure_threshold {
                        if entry.status != NodeStatus::Failed {
                            entry.status = NodeStatus::Failed;
                            log::error!("Node {} failed (no heartbeat for {:?})",
                                entry.key(), elapsed);

                            // 触发故障转移
                            if entry.role == NodeRole::Master {
                                self.trigger_failover(entry.key()).await;
                            }
                        }
                    } else if elapsed > self.failure_threshold / 2 {
                        if entry.status != NodeStatus::Degraded {
                            entry.status = NodeStatus::Degraded;
                            log::warn!("Node {} degraded (heartbeat delayed {:?})",
                                entry.key(), elapsed);
                        }
                    } else {
                        entry.status = NodeStatus::Healthy;
                    }
                }
            }
        })
    }

    /// 收到心跳
    pub fn on_heartbeat(&self, node_id: &str, role: NodeRole) {
        self.nodes.entry(node_id.to_string())
            .and_modify(|health| {
                health.last_heartbeat = Instant::now();
                health.role = role;
                health.status = NodeStatus::Healthy;
            })
            .or_insert(NodeHealth {
                role,
                last_heartbeat: Instant::now(),
                status: NodeStatus::Healthy,
            });
    }

    async fn trigger_failover(&self, failed_master_id: &str) {
        log::info!("Triggering failover for failed master: {}", failed_master_id);

        // 选举新 Master
        let new_master = self.elect_new_master();

        if let Some(master_id) = new_master {
            log::info!("Elected new master: {}", master_id);

            // TODO: 执行故障转移
        } else {
            log::error!("No available slave for failover");
        }
    }

    fn elect_new_master(&self) -> Option<String> {
        // 选择最近的 Slave（复制进度最快）
        self.nodes.iter()
            .filter(|entry| entry.role == NodeRole::Slave && entry.status == NodeStatus::Healthy)
            .min_by_key(|entry| entry.last_heartbeat.elapsed())
            .map(|entry| entry.key().clone())
    }
}
```

### 2. 自动故障转移

```rust
// src/storage/failover/coordinator.rs

pub struct FailoverCoordinator {
    detector: Arc<FailureDetector>,
    replication_master: Arc<ReplicationMaster>,
    current_master: Arc<RwLock<Option<String>>>,
}

impl FailoverCoordinator {
    pub fn new(
        detector: Arc<FailureDetector>,
        replication_master: Arc<ReplicationMaster>,
    ) -> Self {
        Self {
            detector,
            replication_master,
            current_master: Arc::new(RwLock::new(None)),
        }
    }

    /// 执行故障转移
    pub async fn failover(&self, new_master_id: &str) -> Result<(), String> {
        log::info!("Starting failover to new master: {}", new_master_id);

        // 1. 停止旧 Master 的写入
        // 2. 等待所有 Slave 同步
        // 3. 提升新 Master
        // 4. 重新配置 Slave

        *self.current_master.write() = Some(new_master_id.to_string());

        log::info!("Failover completed: new master={}", new_master_id);
        Ok(())
    }

    /// 获取当前 Master
    pub fn get_current_master(&self) -> Option<String> {
        self.current_master.read().clone()
    }
}
```

---

## 数据一致性

### 1. 强一致性（同步复制）

```rust
// Master 写入时等待所有 Slave 确认
pub async fn write_with_sync_replication(
    &self,
    wal_entry: WalEntry,
) -> Result<(), String> {
    // 1. 写入 Master WAL
    let sequence = self.wal_manager.append(wal_entry.record.clone())?;

    // 2. 复制到 Slave
    self.replication_master.replicate(wal_entry)?;

    // 3. 等待所有 Slave 确认
    self.replication_master.wait_for_replication(sequence, Duration::from_secs(5)).await?;

    Ok(())
}
```

### 2. 最终一致性（异步复制）

```rust
// Master 写入后立即返回，Slave 异步同步
pub fn write_with_async_replication(
    &self,
    wal_entry: WalEntry,
) -> Result<u64, String> {
    // 1. 写入 Master WAL
    let sequence = self.wal_manager.append(wal_entry.record.clone())?;

    // 2. 异步复制到 Slave（不等待）
    self.replication_master.replicate(wal_entry)?;

    Ok(sequence)
}
```

---

## 灾难恢复

### 备份策略

| 类型 | 频率 | 保留时间 | 用途 |
|------|------|---------|------|
| **全量备份** | 每周 | 4 周 | 完整数据恢复 |
| **增量备份** | 每天 | 7 天 | 快速恢复 |
| **WAL 备份** | 实时 | 30 天 | 点对点恢复 |

### 全量备份

```rust
// src/storage/backup/full_backup.rs

pub struct FullBackup {
    source_dir: String,
    backup_dir: String,
}

impl FullBackup {
    /// 执行全量备份
    pub async fn backup(&self) -> Result<String, String> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_path = format!("{}/full_{}", self.backup_dir, timestamp);

        std::fs::create_dir_all(&backup_path)?;

        // 1. 备份 SSTable
        self.backup_sstables(&backup_path).await?;

        // 2. 备份 WAL
        self.backup_wal(&backup_path).await?;

        // 3. 创建 Snapshot
        self.create_snapshot(&backup_path).await?;

        log::info!("Full backup completed: {}", backup_path);
        Ok(backup_path)
    }

    async fn backup_sstables(&self, backup_path: &str) -> Result<(), String> {
        // 复制所有 SSTable 文件
        let sstable_dir = format!("{}/sstable", self.source_dir);
        let backup_sstable_dir = format!("{}/sstable", backup_path);

        std::fs::create_dir_all(&backup_sstable_dir)?;

        for entry in std::fs::read_dir(&sstable_dir)? {
            let entry = entry?;
            let src = entry.path();
            let dst = format!("{}/{}", backup_sstable_dir, entry.file_name().to_str().unwrap());

            std::fs::copy(&src, &dst)?;
        }

        Ok(())
    }

    async fn backup_wal(&self, backup_path: &str) -> Result<(), String> {
        // 复制所有 WAL 文件
        // ...
        Ok(())
    }

    async fn create_snapshot(&self, backup_path: &str) -> Result<(), String> {
        // 创建备份元数据
        // ...
        Ok(())
    }

    /// 从备份恢复
    pub async fn restore(&self, backup_path: &str) -> Result<(), String> {
        log::info!("Restoring from backup: {}", backup_path);

        // 1. 恢复 SSTable
        // 2. 恢复 WAL
        // 3. 加载 Snapshot

        Ok(())
    }
}
```

---

## 性能目标

| 指标 | 目标 | 实现方式 |
|------|------|---------|
| **恢复时间** | < 10s | Snapshot + WAL Replay |
| **复制延迟** | < 100ms | 异步复制 + 批量传输 |
| **故障检测** | < 5s | 心跳检测 (1s 间隔) |
| **故障转移** | < 30s | 自动选举 + 数据同步 |
| **备份速度** | > 500MB/s | 并行复制 + 零拷贝 |

---

## 相关链接

- [存储架构设计](01_STORAGE_ARCHITECTURE.md)
- [数据分发架构](02_DISTRIBUTION_ARCHITECTURE.md)
- [实施计划](04_IMPLEMENTATION_PLAN.md)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
