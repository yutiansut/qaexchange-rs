# 高性能存储架构设计

> 分级存储体系：WAL → MemTable → SSTable

**版本**: v1.0.0
**最后更新**: 2025-10-03

---

## 📋 目录

- [架构概览](#架构概览)
- [WAL设计](#wal设计)
- [MemTable设计](#memtable设计)
- [SSTable设计](#sstable设计)
- [Compaction策略](#compaction策略)
- [性能目标](#性能目标)

---

## 架构概览

### 设计原则

1. **写入优化**：WAL + MemTable 顺序写入，延迟 < 10μs
2. **读取优化**：MemTable + Bloom Filter + Index 快速定位
3. **零拷贝**：rkyv 序列化 + mmap 读取
4. **高可靠**：WAL 持久化 + 主从复制

### 数据流

```
写入路径:
OrderRequest → WAL (fsync) → MemTable (in-memory) → [200ms] → Immutable MemTable → SSTable (disk)
                ↓
             返回确认 (P99 < 10μs)

读取路径:
Query → MemTable → Immutable MemTable → SSTable (mmap + Bloom Filter)
```

### 架构图

```
┌─────────────────────────────────────────────────────────┐
│                    应用层                                │
│  AccountSystem | MatchingEngine | Gateway               │
└──────────────────────┬──────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────────────┐
│                  存储引擎 (Storage Engine)               │
│                                                          │
│  ┌──────────┐  ┌────────────┐  ┌─────────────────┐    │
│  │   WAL    │  │  MemTable  │  │  SSTable Pool   │    │
│  │          │  │            │  │                 │    │
│  │ Sequential│→ │  SkipList  │→ │ [SST1][SST2]... │    │
│  │   Write  │  │  (128MB)   │  │  (immutable)    │    │
│  └──────────┘  └────────────┘  └─────────────────┘    │
│       ↓              ↓                  ↓              │
│  fsync (1ms)    Zero-copy         mmap + rkyv         │
└─────────────────────────────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────────────┐
│                  文件系统                                │
│  /data/wal/  |  /data/memtable/  |  /data/sstable/     │
└─────────────────────────────────────────────────────────┘
```

---

## WAL设计

### 功能定位

- **持久化保证**：每条写入先记录 WAL，确保不丢失
- **崩溃恢复**：服务重启时从 WAL 重放恢复 MemTable
- **主从复制**：WAL 传输到从节点实现数据同步

### 数据结构

```rust
// src/storage/wal/record.rs

use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};

/// WAL 记录类型
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub enum WalRecord {
    /// 订单写入
    OrderInsert {
        order_id: [u8; 40],          // UUID
        user_id: [u8; 32],
        instrument_id: [u8; 16],
        direction: u8,
        offset: u8,
        price: f64,
        volume: f64,
        timestamp: i64,
    },

    /// 成交回报
    TradeExecuted {
        trade_id: [u8; 40],
        order_id: [u8; 40],
        exchange_order_id: [u8; 40],
        price: f64,
        volume: f64,
        timestamp: i64,
    },

    /// 账户更新
    AccountUpdate {
        user_id: [u8; 32],
        balance: f64,
        available: f64,
        frozen: f64,
        margin: f64,
        timestamp: i64,
    },

    /// Checkpoint（标记可以安全截断的位置）
    Checkpoint {
        sequence: u64,
        timestamp: i64,
    },
}

/// WAL 日志条目
#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct WalEntry {
    pub sequence: u64,           // 递增序列号
    pub crc32: u32,              // 数据校验和
    pub timestamp: i64,          // 纳秒时间戳
    pub record: WalRecord,       // 实际数据
}

impl WalEntry {
    /// 序列化为字节流（rkyv）
    pub fn to_bytes(&self) -> Vec<u8> {
        rkyv::to_bytes::<_, 1024>(self).unwrap().to_vec()
    }

    /// 从字节流反序列化（零拷贝）
    pub fn from_bytes(bytes: &[u8]) -> Result<&ArchivedWalEntry, String> {
        rkyv::check_archived_root::<WalEntry>(bytes)
            .map_err(|e| format!("WAL deserialization failed: {}", e))
    }
}
```

### WAL 文件格式

```
文件布局:
┌────────────────────────────────────────────────┐
│ Header (128 bytes)                              │
│  - Magic: [u8; 8] = "QAXWAL01"                 │
│  - Version: u32                                 │
│  - Start Sequence: u64                          │
│  - Timestamp: i64                               │
├────────────────────────────────────────────────┤
│ Entry 1 (variable length)                       │
│  - Length: u32 (4 bytes)                        │
│  - Payload: WalEntry (rkyv serialized)          │
├────────────────────────────────────────────────┤
│ Entry 2                                         │
├────────────────────────────────────────────────┤
│ ...                                             │
└────────────────────────────────────────────────┘

文件命名: wal_{start_sequence:020}.log
示例: wal_00000000000000000001.log
```

### WAL 管理器

```rust
// src/storage/wal/manager.rs

use std::fs::{File, OpenOptions};
use std::io::{Write, BufWriter};
use std::sync::Arc;
use parking_lot::Mutex;

pub struct WalManager {
    current_file: Arc<Mutex<BufWriter<File>>>,
    current_sequence: Arc<AtomicU64>,
    base_path: String,
    max_file_size: u64,  // 单个 WAL 文件最大 1GB
}

impl WalManager {
    pub fn new(base_path: &str) -> Self {
        std::fs::create_dir_all(base_path).unwrap();

        let file_path = format!("{}/wal_{:020}.log", base_path, 1);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .unwrap();

        Self {
            current_file: Arc::new(Mutex::new(BufWriter::new(file))),
            current_sequence: Arc::new(AtomicU64::new(1)),
            base_path: base_path.to_string(),
            max_file_size: 1_000_000_000,  // 1GB
        }
    }

    /// 追加 WAL 记录（同步写入，确保持久化）
    pub fn append(&self, record: WalRecord) -> Result<u64, String> {
        let sequence = self.current_sequence.fetch_add(1, Ordering::SeqCst);

        let entry = WalEntry {
            sequence,
            crc32: 0,  // TODO: 计算 CRC32
            timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            record,
        };

        let bytes = entry.to_bytes();
        let length = bytes.len() as u32;

        let mut file = self.current_file.lock();

        // 写入长度前缀
        file.write_all(&length.to_le_bytes())
            .map_err(|e| format!("WAL write failed: {}", e))?;

        // 写入数据
        file.write_all(&bytes)
            .map_err(|e| format!("WAL write failed: {}", e))?;

        // fsync 确保持久化（P99 < 1ms）
        file.flush()
            .map_err(|e| format!("WAL flush failed: {}", e))?;

        file.get_mut().sync_all()
            .map_err(|e| format!("WAL sync failed: {}", e))?;

        Ok(sequence)
    }

    /// 异步批量追加（高吞吐场景）
    pub fn append_batch(&self, records: Vec<WalRecord>) -> Result<Vec<u64>, String> {
        let mut sequences = Vec::with_capacity(records.len());
        let mut file = self.current_file.lock();

        for record in records {
            let sequence = self.current_sequence.fetch_add(1, Ordering::SeqCst);
            sequences.push(sequence);

            let entry = WalEntry {
                sequence,
                crc32: 0,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                record,
            };

            let bytes = entry.to_bytes();
            let length = bytes.len() as u32;

            file.write_all(&length.to_le_bytes())?;
            file.write_all(&bytes)?;
        }

        // 批量 fsync
        file.flush()?;
        file.get_mut().sync_all()?;

        Ok(sequences)
    }

    /// 回放 WAL（崩溃恢复）
    pub fn replay<F>(&self, mut callback: F) -> Result<(), String>
    where
        F: FnMut(WalEntry) -> Result<(), String>,
    {
        use std::io::Read;

        let files = self.list_wal_files()?;

        for file_path in files {
            let mut file = File::open(&file_path)
                .map_err(|e| format!("Open WAL failed: {}", e))?;

            // Skip header (128 bytes)
            let mut header = vec![0u8; 128];
            file.read_exact(&mut header)?;

            loop {
                // Read length prefix
                let mut len_buf = [0u8; 4];
                match file.read_exact(&mut len_buf) {
                    Ok(_) => {},
                    Err(_) => break,  // EOF
                }

                let length = u32::from_le_bytes(len_buf) as usize;

                // Read entry
                let mut entry_buf = vec![0u8; length];
                file.read_exact(&mut entry_buf)?;

                // Deserialize (zero-copy)
                let archived = WalEntry::from_bytes(&entry_buf)?;

                // Convert to owned
                let entry: WalEntry = archived.deserialize(&mut rkyv::Infallible).unwrap();

                callback(entry)?;
            }
        }

        Ok(())
    }

    /// Checkpoint：截断旧 WAL 文件
    pub fn checkpoint(&self, sequence: u64) -> Result<(), String> {
        let files = self.list_wal_files()?;

        for file_path in files {
            if self.should_truncate(&file_path, sequence) {
                std::fs::remove_file(&file_path)
                    .map_err(|e| format!("Truncate WAL failed: {}", e))?;
            }
        }

        Ok(())
    }

    fn list_wal_files(&self) -> Result<Vec<String>, String> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(&self.base_path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                files.push(path.to_str().unwrap().to_string());
            }
        }

        files.sort();
        Ok(files)
    }

    fn should_truncate(&self, file_path: &str, checkpoint_seq: u64) -> bool {
        // 解析文件名中的起始序列号
        // wal_00000000000000000001.log → 1
        let filename = std::path::Path::new(file_path)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();

        if let Some(seq_str) = filename.strip_prefix("wal_") {
            if let Ok(start_seq) = seq_str.parse::<u64>() {
                return start_seq < checkpoint_seq;
            }
        }

        false
    }
}
```

### 性能优化

1. **批量写入**：`append_batch()` 减少 fsync 次数
2. **Group Commit**：多个线程的写入合并为一次 fsync
3. **预分配空间**：`fallocate()` 避免文件扩展开销
4. **Direct I/O**：`O_DIRECT` 绕过页缓存（可选）

---

## MemTable设计

### 功能定位

- **热数据缓存**：最近写入的数据全部在内存
- **快速查找**：SkipList O(log N) 查找
- **并发安全**：无锁 SkipList（Crossbeam）

### 数据结构

```rust
// src/storage/memtable/mod.rs

use crossbeam_skiplist::SkipMap;
use std::sync::Arc;
use parking_lot::RwLock;

pub struct MemTable {
    data: SkipMap<Vec<u8>, Vec<u8>>,  // Key → rkyv serialized value
    size: Arc<AtomicUsize>,            // 当前大小（字节）
    max_size: usize,                   // 最大 128MB
    created_at: i64,
}

impl MemTable {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: SkipMap::new(),
            size: Arc::new(AtomicUsize::new(0)),
            max_size,
            created_at: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        }
    }

    /// 插入（零拷贝）
    pub fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let entry_size = key.len() + value.len();

        if self.size.fetch_add(entry_size, Ordering::Relaxed) + entry_size > self.max_size {
            return Err("MemTable full".to_string());
        }

        self.data.insert(key, value);
        Ok(())
    }

    /// 查询（零拷贝）
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.data.get(key).map(|entry| entry.value().clone())
    }

    /// 迭代器（用于落盘）
    pub fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        self.data.iter().map(|entry| {
            (entry.key().clone(), entry.value().clone())
        })
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.size.load(Ordering::Relaxed) >= self.max_size
    }

    /// 当前大小
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }
}
```

### MemTable 管理器

```rust
// src/storage/memtable/manager.rs

pub struct MemTableManager {
    active: Arc<RwLock<MemTable>>,           // 当前活跃的 MemTable
    immutable: Arc<RwLock<Vec<Arc<MemTable>>>>,  // 待落盘的只读 MemTable
    max_memtable_size: usize,                // 128MB
}

impl MemTableManager {
    pub fn new(max_memtable_size: usize) -> Self {
        Self {
            active: Arc::new(RwLock::new(MemTable::new(max_memtable_size))),
            immutable: Arc::new(RwLock::new(Vec::new())),
            max_memtable_size,
        }
    }

    /// 插入数据
    pub fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let active = self.active.read();

        if active.is_full() {
            drop(active);
            self.rotate()?;
            return self.insert(key, value);
        }

        active.insert(key, value)
    }

    /// 查询数据（先查 active，再查 immutable）
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        // 1. 查询 active MemTable
        if let Some(value) = self.active.read().get(key) {
            return Some(value);
        }

        // 2. 查询 immutable MemTables（从新到旧）
        let immutable = self.immutable.read();
        for memtable in immutable.iter().rev() {
            if let Some(value) = memtable.get(key) {
                return Some(value);
            }
        }

        None
    }

    /// 切换 MemTable（active → immutable）
    fn rotate(&self) -> Result<(), String> {
        let mut active = self.active.write();
        let mut immutable = self.immutable.write();

        // 将当前 active 移动到 immutable
        let old_active = std::mem::replace(&mut *active, MemTable::new(self.max_memtable_size));
        immutable.push(Arc::new(old_active));

        log::info!("MemTable rotated, immutable count: {}", immutable.len());

        Ok(())
    }

    /// 获取待落盘的 MemTable
    pub fn pop_immutable(&self) -> Option<Arc<MemTable>> {
        self.immutable.write().pop()
    }
}
```

---

## SSTable设计

### 功能定位

- **磁盘持久化**：不可变的排序文件
- **快速查找**：Bloom Filter + Index + mmap
- **压缩存储**：LZ4 压缩（可选）

### 文件格式

```
SSTable 文件布局:
┌────────────────────────────────────────────────┐
│ Header (256 bytes)                              │
│  - Magic: [u8; 8] = "QAXSST01"                 │
│  - Version: u32                                 │
│  - Entry Count: u64                             │
│  - Min Key: [u8; 64]                            │
│  - Max Key: [u8; 64]                            │
│  - Bloom Filter Offset: u64                     │
│  - Index Offset: u64                            │
│  - Data Offset: u64                             │
├────────────────────────────────────────────────┤
│ Data Block (variable length)                    │
│  ┌──────────────────────┐                      │
│  │ Entry 1: Key | Value │                      │
│  │ Entry 2: Key | Value │                      │
│  │ ...                  │                      │
│  └──────────────────────┘                      │
├────────────────────────────────────────────────┤
│ Index Block (sparse index)                      │
│  ┌──────────────────────────────┐              │
│  │ Key1 → Offset1               │              │
│  │ Key2 → Offset2 (every 4KB)   │              │
│  │ ...                          │              │
│  └──────────────────────────────┘              │
├────────────────────────────────────────────────┤
│ Bloom Filter (bit array)                        │
│  - Size: entry_count * 10 bits                 │
│  - False positive rate: 1%                     │
└────────────────────────────────────────────────┘

文件命名: sst_{level}_{sequence:020}.sst
示例: sst_0_00000000000000000001.sst
```

### SSTable 构建器

```rust
// src/storage/sstable/builder.rs

use std::fs::File;
use std::io::{Write, BufWriter};

pub struct SSTableBuilder {
    file: BufWriter<File>,
    index: Vec<(Vec<u8>, u64)>,  // (key, offset)
    bloom: BloomFilter,
    min_key: Option<Vec<u8>>,
    max_key: Option<Vec<u8>>,
    entry_count: u64,
    data_offset: u64,
}

impl SSTableBuilder {
    pub fn new(file_path: &str) -> Self {
        let file = File::create(file_path).unwrap();
        let mut builder = Self {
            file: BufWriter::new(file),
            index: Vec::new(),
            bloom: BloomFilter::new(100_000, 0.01),  // 10万条，1% 误判率
            min_key: None,
            max_key: None,
            entry_count: 0,
            data_offset: 256,  // 跳过 header
        };

        // 预留 header 空间
        builder.file.write_all(&vec![0u8; 256]).unwrap();
        builder
    }

    /// 添加条目（必须按 key 排序）
    pub fn add(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        // 更新 min/max key
        if self.min_key.is_none() {
            self.min_key = Some(key.clone());
        }
        self.max_key = Some(key.clone());

        // 添加到 Bloom Filter
        self.bloom.insert(&key);

        // 每 4KB 添加一个索引条目
        if self.entry_count % 100 == 0 {
            self.index.push((key.clone(), self.data_offset));
        }

        // 写入数据
        let key_len = key.len() as u32;
        let value_len = value.len() as u32;

        self.file.write_all(&key_len.to_le_bytes())?;
        self.file.write_all(&key)?;
        self.file.write_all(&value_len.to_le_bytes())?;
        self.file.write_all(&value)?;

        self.data_offset += 8 + key.len() as u64 + value.len() as u64;
        self.entry_count += 1;

        Ok(())
    }

    /// 完成构建
    pub fn finish(mut self) -> Result<(), String> {
        let index_offset = self.data_offset;

        // 写入索引块
        for (key, offset) in &self.index {
            let key_len = key.len() as u32;
            self.file.write_all(&key_len.to_le_bytes())?;
            self.file.write_all(key)?;
            self.file.write_all(&offset.to_le_bytes())?;
        }

        let bloom_offset = self.data_offset + self.index.len() as u64 * 100;

        // 写入 Bloom Filter
        let bloom_bytes = self.bloom.to_bytes();
        self.file.write_all(&bloom_bytes)?;

        // 写入 Header
        self.file.seek(std::io::SeekFrom::Start(0))?;

        let mut header = vec![0u8; 256];
        header[0..8].copy_from_slice(b"QAXSST01");
        header[8..12].copy_from_slice(&1u32.to_le_bytes());  // version
        header[12..20].copy_from_slice(&self.entry_count.to_le_bytes());

        if let Some(min_key) = &self.min_key {
            let len = min_key.len().min(64);
            header[20..20+len].copy_from_slice(&min_key[..len]);
        }

        if let Some(max_key) = &self.max_key {
            let len = max_key.len().min(64);
            header[84..84+len].copy_from_slice(&max_key[..len]);
        }

        header[148..156].copy_from_slice(&bloom_offset.to_le_bytes());
        header[156..164].copy_from_slice(&index_offset.to_le_bytes());
        header[164..172].copy_from_slice(&256u64.to_le_bytes());  // data offset

        self.file.write_all(&header)?;
        self.file.flush()?;

        Ok(())
    }
}
```

### SSTable 读取器

```rust
// src/storage/sstable/reader.rs

use memmap2::Mmap;
use std::fs::File;

pub struct SSTableReader {
    mmap: Mmap,
    header: SSTableHeader,
    index: Vec<(Vec<u8>, u64)>,
    bloom: BloomFilter,
}

impl SSTableReader {
    pub fn open(file_path: &str) -> Result<Self, String> {
        let file = File::open(file_path)
            .map_err(|e| format!("Open SSTable failed: {}", e))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| format!("mmap failed: {}", e))?;

        // 解析 header
        let header = SSTableHeader::from_bytes(&mmap[0..256])?;

        // 加载索引
        let index = Self::load_index(&mmap, header.index_offset)?;

        // 加载 Bloom Filter
        let bloom = Self::load_bloom(&mmap, header.bloom_offset)?;

        Ok(Self { mmap, header, index, bloom })
    }

    /// 查询（零拷贝）
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        // 1. Bloom Filter 快速排除
        if !self.bloom.contains(key) {
            return None;
        }

        // 2. 二分查找索引
        let offset = self.find_offset(key)?;

        // 3. 顺序扫描数据块
        self.scan_data_block(key, offset)
    }

    fn find_offset(&self, key: &[u8]) -> Option<u64> {
        // 二分查找最接近的索引条目
        let idx = self.index.binary_search_by_key(&key, |(k, _)| k.as_slice())
            .unwrap_or_else(|idx| if idx > 0 { idx - 1 } else { 0 });

        Some(self.index[idx].1)
    }

    fn scan_data_block(&self, key: &[u8], start_offset: u64) -> Option<Vec<u8>> {
        let mut offset = start_offset as usize;

        while offset < self.header.index_offset as usize {
            // 读取 key
            let key_len = u32::from_le_bytes(self.mmap[offset..offset+4].try_into().unwrap()) as usize;
            offset += 4;

            let entry_key = &self.mmap[offset..offset+key_len];
            offset += key_len;

            // 读取 value
            let value_len = u32::from_le_bytes(self.mmap[offset..offset+4].try_into().unwrap()) as usize;
            offset += 4;

            if entry_key == key {
                let value = self.mmap[offset..offset+value_len].to_vec();
                return Some(value);
            }

            offset += value_len;

            // 超过查找范围
            if entry_key > key {
                break;
            }
        }

        None
    }

    fn load_index(mmap: &Mmap, index_offset: u64) -> Result<Vec<(Vec<u8>, u64)>, String> {
        let mut index = Vec::new();
        let mut offset = index_offset as usize;

        // TODO: 解析索引块

        Ok(index)
    }

    fn load_bloom(mmap: &Mmap, bloom_offset: u64) -> Result<BloomFilter, String> {
        // TODO: 解析 Bloom Filter
        Ok(BloomFilter::new(100_000, 0.01))
    }
}

#[derive(Debug)]
struct SSTableHeader {
    version: u32,
    entry_count: u64,
    min_key: Vec<u8>,
    max_key: Vec<u8>,
    bloom_offset: u64,
    index_offset: u64,
    data_offset: u64,
}

impl SSTableHeader {
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if &bytes[0..8] != b"QAXSST01" {
            return Err("Invalid SSTable magic".to_string());
        }

        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let entry_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap());

        // TODO: 解析完整 header

        Ok(Self {
            version,
            entry_count,
            min_key: Vec::new(),
            max_key: Vec::new(),
            bloom_offset: u64::from_le_bytes(bytes[148..156].try_into().unwrap()),
            index_offset: u64::from_le_bytes(bytes[156..164].try_into().unwrap()),
            data_offset: u64::from_le_bytes(bytes[164..172].try_into().unwrap()),
        })
    }
}
```

---

## Compaction策略

### Leveled Compaction（RocksDB 风格）

```
Level 0: 4个 SSTable（10MB each）  ← MemTable flush
Level 1: 40MB (合并 L0)
Level 2: 400MB (合并 L1)
Level 3: 4GB (合并 L2)
...

触发条件:
- L0: 文件数 ≥ 4 → 合并到 L1
- L1+: 层级大小 ≥ 阈值 → 合并到下一层
```

### Compaction 执行器

```rust
// src/storage/compaction/mod.rs

pub struct CompactionExecutor {
    levels: Vec<Vec<Arc<SSTableReader>>>,  // 各层级的 SSTable
    base_path: String,
}

impl CompactionExecutor {
    /// 检查是否需要 Compaction
    pub fn should_compact(&self, level: usize) -> bool {
        match level {
            0 => self.levels[0].len() >= 4,
            1 => self.total_size(1) >= 40 * 1024 * 1024,
            2 => self.total_size(2) >= 400 * 1024 * 1024,
            _ => false,
        }
    }

    /// 执行 Compaction
    pub async fn compact(&mut self, level: usize) -> Result<(), String> {
        log::info!("Starting compaction for level {}", level);

        // 1. 选择需要合并的 SSTable
        let sources = if level == 0 {
            self.levels[0].clone()
        } else {
            self.select_overlapping_sstables(level)
        };

        // 2. 多路归并排序
        let merged = self.merge_sstables(sources).await?;

        // 3. 写入新的 SSTable
        let new_sst = self.write_sstable(level + 1, merged).await?;

        // 4. 更新元数据
        self.levels[level + 1].push(new_sst);

        // 5. 删除旧文件
        self.cleanup_old_sstables(level)?;

        log::info!("Compaction completed for level {}", level);
        Ok(())
    }

    async fn merge_sstables(&self, sstables: Vec<Arc<SSTableReader>>)
        -> Result<Vec<(Vec<u8>, Vec<u8>)>, String>
    {
        // 多路归并排序（K-way merge）
        use std::collections::BinaryHeap;

        let mut heap = BinaryHeap::new();
        let mut result = Vec::new();

        // TODO: 实现 K-way merge

        Ok(result)
    }

    fn total_size(&self, level: usize) -> u64 {
        self.levels[level].iter()
            .map(|sst| sst.header.entry_count * 100)  // 估算
            .sum()
    }
}
```

---

## 性能目标

| 指标 | 目标 | 实现方式 |
|------|------|---------|
| **写入延迟** | P99 < 10μs | WAL 顺序写 + MemTable 内存写 |
| **fsync 延迟** | P99 < 1ms | Group Commit + 批量 fsync |
| **读取延迟** | P99 < 100μs | MemTable → Bloom Filter → mmap |
| **写入吞吐** | > 1M ops/s | 批量写入 + 零拷贝 |
| **恢复时间** | < 10s | WAL 回放（1GB/s）|
| **压缩开销** | < 10% CPU | 后台线程 + 增量压缩 |

---

## 相关链接

- [数据分发系统设计](02_DISTRIBUTION_ARCHITECTURE.md)
- [故障恢复设计](03_RECOVERY_DESIGN.md)
- [实施计划](04_IMPLEMENTATION_PLAN.md)

---

*最后更新: 2025-10-03*
*维护者: @yutiansut*
