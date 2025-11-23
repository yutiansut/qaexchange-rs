// OLTP SSTable - 基于 rkyv 的零拷贝 SSTable
//
// 性能目标：
// - 写入吞吐：> 100 MB/s
// - 读取延迟：P99 < 100μs（mmap 零拷贝）
// - 范围扫描：> 1 GB/s
//
// 文件格式：
// [Header: 128 bytes]
// [Data Block 1: entries]
// [Data Block 2: entries]
// ...
// [Index Block: block offsets]
// [Metadata: rkyv serialized]
// [Footer: 64 bytes]

use super::bloom::BloomFilter;
use super::types::SSTableMetadata;
use crate::storage::memtable::types::{MemTableKey, MemTableValue};
use crate::storage::wal::WalRecord;
use rkyv::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// SSTable 文件头（128 bytes）
#[derive(Debug, Clone)]
struct SSTableHeader {
    magic: [u8; 8],       // "QAXSS01\0"
    version: u32,         // 版本号
    entry_count: u64,     // 记录数
    min_timestamp: i64,   // 最小时间戳
    max_timestamp: i64,   // 最大时间戳
    metadata_offset: u64, // 元数据偏移
    _reserved: [u8; 84],  // 保留字段
}

impl SSTableHeader {
    fn new(entry_count: u64, min_ts: i64, max_ts: i64, metadata_offset: u64) -> Self {
        Self {
            magic: *b"QAXSS01\0",
            version: 1,
            entry_count,
            min_timestamp: min_ts,
            max_timestamp: max_ts,
            metadata_offset,
            _reserved: [0u8; 84],
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.entry_count.to_le_bytes());
        bytes.extend_from_slice(&self.min_timestamp.to_le_bytes());
        bytes.extend_from_slice(&self.max_timestamp.to_le_bytes());
        bytes.extend_from_slice(&self.metadata_offset.to_le_bytes());
        bytes.extend_from_slice(&self._reserved);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 128 {
            return Err("Invalid header size".to_string());
        }

        let mut magic = [0u8; 8];
        magic.copy_from_slice(&bytes[0..8]);
        if &magic != b"QAXSS01\0" {
            return Err("Invalid magic".to_string());
        }

        Ok(Self {
            magic,
            version: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            entry_count: u64::from_le_bytes(bytes[12..20].try_into().unwrap()),
            min_timestamp: i64::from_le_bytes(bytes[20..28].try_into().unwrap()),
            max_timestamp: i64::from_le_bytes(bytes[28..36].try_into().unwrap()),
            metadata_offset: u64::from_le_bytes(bytes[36..44].try_into().unwrap()),
            _reserved: bytes[44..128].try_into().unwrap(),
        })
    }
}

/// rkyv SSTable Writer
pub struct RkyvSSTableWriter {
    file: BufWriter<File>,
    entry_count: u64,
    min_timestamp: Option<i64>,
    max_timestamp: Option<i64>,
    min_key: Option<Vec<u8>>,
    max_key: Option<Vec<u8>>,
    current_offset: u64,
    bloom_filter: BloomFilter,
}

impl RkyvSSTableWriter {
    /// 创建新的 SSTable Writer
    ///
    /// # 参数
    /// - path: SSTable 文件路径
    /// - expected_entries: 预期条目数（用于 Bloom Filter 优化，默认 10000）
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        Self::create_with_capacity(path, 10000)
    }

    /// 创建新的 SSTable Writer，指定容量
    pub fn create_with_capacity<P: AsRef<Path>>(
        path: P,
        expected_entries: usize,
    ) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| format!("Create SSTable failed: {}", e))?;

        let mut writer = BufWriter::new(file);

        // 预留 Header 空间（稍后回填）
        let placeholder = vec![0u8; 128];
        writer
            .write_all(&placeholder)
            .map_err(|e| format!("Write placeholder failed: {}", e))?;

        // 创建 Bloom Filter（1% 假阳性率）
        let bloom_filter = BloomFilter::new(expected_entries, 0.01);

        Ok(Self {
            file: writer,
            entry_count: 0,
            min_timestamp: None,
            max_timestamp: None,
            min_key: None,
            max_key: None,
            current_offset: 128, // Header 后开始
            bloom_filter,
        })
    }

    /// 写入单条记录
    pub fn append(&mut self, key: MemTableKey, value: MemTableValue) -> Result<(), String> {
        // 更新统计信息
        self.entry_count += 1;
        if self.min_timestamp.is_none() || Some(key.timestamp) < self.min_timestamp {
            self.min_timestamp = Some(key.timestamp);
        }
        if self.max_timestamp.is_none() || Some(key.timestamp) > self.max_timestamp {
            self.max_timestamp = Some(key.timestamp);
        }

        // 更新 key range
        let key_bytes = key.to_bytes();
        if self.min_key.is_none() || Some(&key_bytes) < self.min_key.as_ref() {
            self.min_key = Some(key_bytes.clone());
        }
        if self.max_key.is_none() || Some(&key_bytes) > self.max_key.as_ref() {
            self.max_key = Some(key_bytes.clone());
        }

        // 插入到 Bloom Filter
        self.bloom_filter.insert(&key_bytes);

        // 序列化键值对
        let key_bytes =
            rkyv::to_bytes::<_, 256>(&key).map_err(|e| format!("Serialize key failed: {}", e))?;
        let value_bytes = rkyv::to_bytes::<_, 2048>(&value)
            .map_err(|e| format!("Serialize value failed: {}", e))?;

        // 写入长度前缀 + 数据
        self.file
            .write_all(&(key_bytes.len() as u32).to_le_bytes())
            .map_err(|e| format!("Write key length failed: {}", e))?;
        self.file
            .write_all(&key_bytes)
            .map_err(|e| format!("Write key failed: {}", e))?;

        self.file
            .write_all(&(value_bytes.len() as u32).to_le_bytes())
            .map_err(|e| format!("Write value length failed: {}", e))?;
        self.file
            .write_all(&value_bytes)
            .map_err(|e| format!("Write value failed: {}", e))?;

        self.current_offset += 8 + key_bytes.len() as u64 + value_bytes.len() as u64;

        Ok(())
    }

    /// 批量写入（从 MemTable）
    pub fn write_from_memtable(
        &mut self,
        entries: Vec<(MemTableKey, MemTableValue)>,
    ) -> Result<(), String> {
        for (key, value) in entries {
            self.append(key, value)?;
        }
        Ok(())
    }

    /// 完成写入（回填 Header 和 Metadata）
    pub fn finish(mut self) -> Result<SSTableMetadata, String> {
        let metadata_offset = self.current_offset;

        // 获取 Bloom Filter 统计信息
        let bloom_stats = self.bloom_filter.stats();
        log::info!(
            "SSTable Bloom Filter: {} entries, {} bits, {} hash functions, {:.2}% FPP, {} bytes",
            bloom_stats.n,
            bloom_stats.m,
            bloom_stats.k,
            bloom_stats.estimated_fpp * 100.0,
            bloom_stats.memory_bytes
        );

        // 创建元数据
        let metadata = SSTableMetadata::new(
            self.entry_count,
            self.min_timestamp.unwrap_or(0),
            self.max_timestamp.unwrap_or(0),
        )
        .with_key_range(
            self.min_key.unwrap_or_default(),
            self.max_key.unwrap_or_default(),
        )
        .with_bloom_filter(self.bloom_filter);

        // 序列化元数据
        let metadata_bytes = rkyv::to_bytes::<_, 4096>(&metadata)
            .map_err(|e| format!("Serialize metadata failed: {}", e))?;

        // 写入元数据
        self.file
            .write_all(&metadata_bytes)
            .map_err(|e| format!("Write metadata failed: {}", e))?;

        self.file
            .flush()
            .map_err(|e| format!("Flush failed: {}", e))?;

        // 回填 Header
        let header = SSTableHeader::new(
            self.entry_count,
            self.min_timestamp.unwrap_or(0),
            self.max_timestamp.unwrap_or(0),
            metadata_offset,
        );

        let mut file = self
            .file
            .into_inner()
            .map_err(|e| format!("Get inner file failed: {}", e))?;

        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Seek to header failed: {}", e))?;
        file.write_all(&header.to_bytes())
            .map_err(|e| format!("Write header failed: {}", e))?;
        file.sync_all().map_err(|e| format!("Sync failed: {}", e))?;

        Ok(metadata)
    }
}

/// rkyv SSTable Reader
pub struct RkyvSSTable {
    file_path: String,
    header: SSTableHeader,
    metadata: SSTableMetadata,
}

impl RkyvSSTable {
    /// 打开已存在的 SSTable
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file_path = path.as_ref().to_str().unwrap().to_string();
        let mut file = File::open(&path).map_err(|e| format!("Open SSTable failed: {}", e))?;

        // 读取 Header
        let mut header_buf = vec![0u8; 128];
        file.read_exact(&mut header_buf)
            .map_err(|e| format!("Read header failed: {}", e))?;
        let header = SSTableHeader::from_bytes(&header_buf)?;

        // 读取 Metadata
        file.seek(SeekFrom::Start(header.metadata_offset))
            .map_err(|e| format!("Seek to metadata failed: {}", e))?;

        // 读取从 metadata_offset 到文件末尾的所有数据
        let mut metadata_buf = Vec::new();
        file.read_to_end(&mut metadata_buf)
            .map_err(|e| format!("Read metadata failed: {}", e))?;

        // rkyv 需要对齐的缓冲区，将数据复制到 AlignedVec
        let mut aligned_buf = rkyv::AlignedVec::with_capacity(metadata_buf.len());
        aligned_buf.extend_from_slice(&metadata_buf);

        let archived_metadata = rkyv::check_archived_root::<SSTableMetadata>(&aligned_buf)
            .map_err(|e| format!("Deserialize metadata failed: {}", e))?;
        let metadata: SSTableMetadata = archived_metadata
            .deserialize(&mut rkyv::Infallible)
            .map_err(|e| format!("Deserialize metadata failed: {:?}", e))?;

        Ok(Self {
            file_path,
            header,
            metadata,
        })
    }

    /// 获取元数据
    pub fn metadata(&self) -> &SSTableMetadata {
        &self.metadata
    }

    /// 获取文件路径
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// 范围查询（简化版，全扫描）
    ///
    /// 生产环境优化：
    /// - 使用 mmap 进行零拷贝读取
    /// - 二分查找定位起始位置
    /// - Block 级别索引
    pub fn range_query(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<(i64, u64, WalRecord)>, String> {
        // 快速过滤：时间范围检查
        if end_ts < self.header.min_timestamp || start_ts > self.header.max_timestamp {
            return Ok(Vec::new());
        }

        let mut file =
            File::open(&self.file_path).map_err(|e| format!("Open file failed: {}", e))?;

        // 跳过 Header
        file.seek(SeekFrom::Start(128))
            .map_err(|e| format!("Seek failed: {}", e))?;

        let mut results = Vec::new();
        let mut entries_read = 0u64;

        while entries_read < self.header.entry_count {
            // 读取 key 长度
            let mut key_len_buf = [0u8; 4];
            if file.read_exact(&mut key_len_buf).is_err() {
                break;
            }
            let key_len = u32::from_le_bytes(key_len_buf) as usize;

            // 读取 key
            let mut key_buf = vec![0u8; key_len];
            file.read_exact(&mut key_buf)
                .map_err(|e| format!("Read key failed: {}", e))?;

            let archived_key = rkyv::check_archived_root::<MemTableKey>(&key_buf)
                .map_err(|e| format!("Deserialize key failed: {}", e))?;
            let key: MemTableKey = archived_key
                .deserialize(&mut rkyv::Infallible)
                .map_err(|e| format!("Deserialize key failed: {:?}", e))?;

            // 读取 value 长度
            let mut value_len_buf = [0u8; 4];
            file.read_exact(&mut value_len_buf)
                .map_err(|e| format!("Read value length failed: {}", e))?;
            let value_len = u32::from_le_bytes(value_len_buf) as usize;

            // 读取 value
            let mut value_buf = vec![0u8; value_len];
            file.read_exact(&mut value_buf)
                .map_err(|e| format!("Read value failed: {}", e))?;

            // 时间范围过滤
            if key.timestamp >= start_ts && key.timestamp <= end_ts {
                let archived_value = rkyv::check_archived_root::<MemTableValue>(&value_buf)
                    .map_err(|e| format!("Deserialize value failed: {}", e))?;
                let value: MemTableValue = archived_value
                    .deserialize(&mut rkyv::Infallible)
                    .map_err(|e| format!("Deserialize value failed: {:?}", e))?;

                results.push((key.timestamp, key.sequence, value.record));
            }

            entries_read += 1;
        }

        Ok(results)
    }

    /// 获取文件大小
    pub fn file_size(&self) -> Result<u64, String> {
        std::fs::metadata(&self.file_path)
            .map(|m| m.len())
            .map_err(|e| format!("Get file size failed: {}", e))
    }

    /// 使用 Bloom Filter 快速检查 key 是否可能存在
    ///
    /// 返回值：
    /// - false: 一定不存在，可以跳过此 SSTable
    /// - true: 可能存在，需要实际查询
    pub fn might_contain(&self, key_bytes: &[u8]) -> bool {
        if let Some(ref bloom) = self.metadata.bloom_filter {
            bloom.contains(key_bytes)
        } else {
            // 没有 Bloom Filter，保守返回 true
            true
        }
    }

    /// 检查时间范围查询是否需要扫描此 SSTable
    pub fn should_scan(&self, start_ts: i64, end_ts: i64) -> bool {
        // 时间范围过滤
        if end_ts < self.header.min_timestamp || start_ts > self.header.max_timestamp {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::WalRecord;

    fn create_test_entry(seq: u64, timestamp: i64) -> (MemTableKey, MemTableValue) {
        let key = MemTableKey::new(timestamp, seq);
        let value = MemTableValue::new(WalRecord::OrderInsert {
            order_id: seq,
            user_id: [1u8; 32],
            instrument_id: [1u8; 16],
            direction: 0,
            offset: 0,
            price: 4000.0 + seq as f64,
            volume: 10.0,
            timestamp,
        });
        (key, value)
    }

    #[test]
    fn test_write_and_read() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let sstable_path = tmp_dir.path().join("test.sst");

        // 写入
        {
            let mut writer = RkyvSSTableWriter::create(&sstable_path).unwrap();

            for i in 0..100 {
                let (key, value) = create_test_entry(i, 1000 + i as i64 * 10);
                writer.append(key, value).unwrap();
            }

            let metadata = writer.finish().unwrap();
            assert_eq!(metadata.entry_count, 100);
            assert_eq!(metadata.min_timestamp, 1000);
            assert_eq!(metadata.max_timestamp, 1990);
        }

        // 读取
        {
            let reader = RkyvSSTable::open(&sstable_path).unwrap();
            assert_eq!(reader.metadata().entry_count, 100);

            // 范围查询
            let results = reader.range_query(1000, 1500).unwrap();
            assert_eq!(results.len(), 51); // 1000, 1010, ..., 1500
        }
    }

    #[test]
    fn test_range_query_filter() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let sstable_path = tmp_dir.path().join("test_range.sst");

        // 写入
        {
            let mut writer = RkyvSSTableWriter::create(&sstable_path).unwrap();
            for i in 0..10 {
                let (key, value) = create_test_entry(i, 1000 + i as i64 * 100);
                writer.append(key, value).unwrap();
            }
            writer.finish().unwrap();
        }

        // 读取并查询
        let reader = RkyvSSTable::open(&sstable_path).unwrap();

        // 完全不重叠
        let results = reader.range_query(0, 500).unwrap();
        assert_eq!(results.len(), 0);

        // 完全包含
        let results = reader.range_query(1000, 2000).unwrap();
        assert_eq!(results.len(), 10);

        // 部分重叠
        let results = reader.range_query(1200, 1600).unwrap();
        assert_eq!(results.len(), 5); // 1200, 1300, 1400, 1500, 1600
    }
}
