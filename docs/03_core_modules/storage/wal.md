# WAL (Write-Ahead Log) 设计

## 📖 概述

Write-Ahead Log (WAL) 是 QAExchange-RS 存储系统的核心组件，提供崩溃恢复和数据持久化保证。

## 🎯 设计目标

- **持久化保证**: 所有交易数据在返回前写入 WAL
- **崩溃恢复**: 系统崩溃后可从 WAL 完整恢复
- **高性能**: P99 < 50ms 写入延迟 (HDD/VM)
- **数据完整性**: CRC32 校验确保数据不损坏

## 🏗️ 架构设计

### 核心组件

```rust
// src/storage/wal/manager.rs
pub struct WalManager {
    /// 当前活跃的 WAL 文件
    active_file: File,

    /// WAL 基础路径
    base_path: PathBuf,

    /// 文件轮转阈值 (默认 1GB)
    rotation_threshold: u64,

    /// 当前文件大小
    current_size: u64,
}
```

### 记录格式

```rust
// src/storage/wal/record.rs
#[derive(Archive, Serialize, Deserialize)]
pub enum WalRecord {
    /// 订单插入
    OrderInsert {
        timestamp: i64,
        order_id: String,
        user_id: String,
        instrument_id: String,
        // ...
    },

    /// 成交记录
    TradeExecuted {
        timestamp: i64,
        trade_id: String,
        order_id: String,
        // ...
    },

    /// 账户更新
    AccountUpdate {
        timestamp: i64,
        account_id: String,
        balance: f64,
        // ...
    },

    /// Tick 数据 (Phase 9)
    TickData {
        timestamp: i64,
        instrument_id: String,
        last_price: f64,
        volume: i64,
        // ...
    },

    /// 订单簿快照 (Phase 9)
    OrderBookSnapshot {
        timestamp: i64,
        instrument_id: String,
        bids: Vec<(f64, i64)>,
        asks: Vec<(f64, i64)>,
    },
}
```

### 文件格式

```
┌─────────────────────────────────────────┐
│  WAL File Header (32 bytes)             │
│  - Magic Number: 0x57414C46             │
│  - Version: u32                          │
│  - Created At: i64                       │
├─────────────────────────────────────────┤
│  Record 1                                │
│  ┌─────────────────────────────────┐   │
│  │ Length: u32 (4 bytes)           │   │
│  │ CRC32: u32 (4 bytes)            │   │
│  │ Data: [u8; length] (rkyv)       │   │
│  └─────────────────────────────────┘   │
├─────────────────────────────────────────┤
│  Record 2                                │
│  ...                                     │
└─────────────────────────────────────────┘
```

## ⚡ 性能特性

### 批量写入

```rust
impl WalManager {
    /// 批量写入多条记录
    pub fn write_batch(&mut self, records: &[WalRecord]) -> Result<()> {
        let mut buffer = Vec::with_capacity(records.len() * 256);

        for record in records {
            // 序列化 (rkyv zero-copy)
            let bytes = rkyv::to_bytes::<_, 256>(record)?;
            let crc = crc32fast::hash(&bytes);

            buffer.write_u32::<LittleEndian>(bytes.len() as u32)?;
            buffer.write_u32::<LittleEndian>(crc)?;
            buffer.write_all(&bytes)?;
        }

        // 一次性写入 + fsync
        self.active_file.write_all(&buffer)?;
        self.active_file.sync_data()?;

        Ok(())
    }
}
```

**性能指标**:
- 批量吞吐: **78,125 entries/sec** (测试结果)
- 单次写入延迟: P99 < 50ms (HDD/VM)
- 批量写入延迟: P99 < 21ms (100条/批)

### 文件轮转

```rust
impl WalManager {
    /// 检查并执行文件轮转
    fn check_rotation(&mut self) -> Result<()> {
        if self.current_size >= self.rotation_threshold {
            self.rotate()?;
        }
        Ok(())
    }

    /// 轮转到新文件
    fn rotate(&mut self) -> Result<()> {
        // 1. 关闭当前文件
        self.active_file.sync_all()?;

        // 2. 创建新文件 (timestamp-based naming)
        let new_file_path = self.generate_new_file_path();
        self.active_file = File::create(&new_file_path)?;
        self.current_size = 0;

        Ok(())
    }
}
```

**轮转策略**:
- 阈值: 1GB (可配置)
- 命名: `wal_{timestamp}.log`
- 自动归档: 旧文件保留 30 天 (可配置)

## 🔄 崩溃恢复

### 恢复流程

```rust
impl WalManager {
    /// 从 WAL 恢复系统状态
    pub fn replay(&self, handler: &mut dyn WalReplayHandler) -> Result<()> {
        // 1. 扫描所有 WAL 文件
        let mut wal_files = self.list_wal_files()?;
        wal_files.sort(); // 按时间戳排序

        // 2. 逐个回放
        for file_path in wal_files {
            self.replay_file(&file_path, handler)?;
        }

        Ok(())
    }

    fn replay_file(&self, path: &Path, handler: &mut dyn WalReplayHandler) -> Result<()> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();

        loop {
            // 读取长度
            let length = match file.read_u32::<LittleEndian>() {
                Ok(len) => len,
                Err(_) => break, // EOF
            };

            // 读取 CRC
            let expected_crc = file.read_u32::<LittleEndian>()?;

            // 读取数据
            buffer.resize(length as usize, 0);
            file.read_exact(&mut buffer)?;

            // 校验 CRC
            let actual_crc = crc32fast::hash(&buffer);
            if actual_crc != expected_crc {
                return Err(WalError::CorruptedRecord);
            }

            // 反序列化并应用
            let record = rkyv::from_bytes::<WalRecord>(&buffer)?;
            handler.apply(record)?;
        }

        Ok(())
    }
}

/// 恢复处理器接口
pub trait WalReplayHandler {
    fn apply(&mut self, record: WalRecord) -> Result<()>;
}
```

### 恢复示例

```rust
// 恢复账户状态
struct AccountRecoveryHandler {
    account_manager: Arc<AccountManager>,
}

impl WalReplayHandler for AccountRecoveryHandler {
    fn apply(&mut self, record: WalRecord) -> Result<()> {
        match record {
            WalRecord::AccountUpdate { account_id, balance, .. } => {
                self.account_manager.update_balance(&account_id, balance)?;
            }
            WalRecord::OrderInsert { order, .. } => {
                self.account_manager.restore_order(order)?;
            }
            _ => {}
        }
        Ok(())
    }
}

// 执行恢复
let mut handler = AccountRecoveryHandler { account_manager };
wal_manager.replay(&mut handler)?;
```

## 📊 按品种隔离

### 目录结构

```
/data/storage/
├── IF2501/
│   ├── wal/
│   │   ├── wal_1696234567890.log
│   │   ├── wal_1696320967890.log
│   │   └── ...
│   ├── memtable/
│   └── sstables/
├── IC2501/
│   ├── wal/
│   └── ...
└── ...
```

### 优势

1. **并行写入**: 不同品种可并行持久化
2. **隔离故障**: 单个品种损坏不影响其他
3. **按需恢复**: 只恢复需要的品种
4. **水平扩展**: 可按品种分片到不同节点

## 🛠️ 配置示例

```toml
# config/storage.toml
[wal]
base_path = "/data/storage"
rotation_threshold_mb = 1024  # 1GB
retention_days = 30
enable_compression = false     # 暂不支持
fsync_on_write = true          # 生产环境必须开启
```

## 🔍 监控指标

```rust
pub struct WalMetrics {
    /// 总写入记录数
    pub total_records: u64,

    /// 总写入字节数
    pub total_bytes: u64,

    /// 当前活跃文件数
    pub active_files: usize,

    /// 最后写入延迟 (ms)
    pub last_write_latency_ms: f64,

    /// P99 写入延迟 (ms)
    pub p99_write_latency_ms: f64,
}
```

## 📚 相关文档

- [MemTable 实现](memtable.md) - WAL 数据如何进入内存
- [SSTable 格式](sstable.md) - MemTable 如何持久化
- [崩溃恢复设计](../../02_architecture/decoupled_storage.md) - 完整恢复流程
- [存储系统详细设计](../../storage/01_STORAGE_ARCHITECTURE.md) - 架构细节

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
