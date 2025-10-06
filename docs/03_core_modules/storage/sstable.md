# SSTable (Sorted String Table) 格式

## 📖 概述

SSTable (Sorted String Table) 是 QAExchange-RS 存储系统中 MemTable 的持久化格式。当 MemTable 达到大小阈值时，数据会被 flush 到磁盘上的 SSTable 文件中，提供高效的磁盘存储和零拷贝读取能力。

## 🎯 设计目标

- **持久化**: MemTable 数据的永久存储
- **零拷贝读取**: 使用 mmap 避免数据拷贝 (OLTP)
- **高压缩率**: 列式存储减少磁盘占用 (OLAP)
- **快速查找**: Bloom Filter + 索引加速
- **顺序写入**: LSM-Tree 架构，写入性能优秀

## 🏗️ 双格式架构

QAExchange-RS 实现了 **OLTP** 和 **OLAP** 双 SSTable 体系：

### 1. OLTP SSTable (rkyv 格式)

#### 设计理念

- **目标场景**: 低延迟点查询、小范围扫描
- **序列化格式**: rkyv (zero-copy)
- **读取方式**: mmap 内存映射
- **典型延迟**: P99 < 20μs

#### 文件格式

```
┌─────────────────────────────────────────────────────────────┐
│  Header (32 bytes)                                           │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Magic Number: 0x53535442 ("SSTB")                     │  │
│  │ Version: u32                                           │  │
│  │ Created At: i64 (timestamp)                            │  │
│  │ Number of Entries: u64                                 │  │
│  │ Bloom Filter Offset: u64                               │  │
│  │ Index Offset: u64                                      │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Bloom Filter (可选, ~1KB - 10KB)                           │
│  - Bit array size: computed from entry count                │
│  - Number of hash functions: 7 (optimal for 1% FP rate)    │
├─────────────────────────────────────────────────────────────┤
│  Data Blocks (multiple, 4KB - 64KB each)                    │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Block 1:                                               │  │
│  │   Entry 1: [Key Length: u32] [Key: bytes]             │  │
│  │            [Value Length: u32] [Value: rkyv bytes]    │  │
│  │   Entry 2: ...                                         │  │
│  │   ...                                                  │  │
│  │ Block 2:                                               │  │
│  │   Entry N: ...                                         │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Index Block                                                 │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Sparse Index (每个 Block 一条索引)                     │  │
│  │   [First Key: bytes] → [Block Offset: u64]            │  │
│  │   [First Key: bytes] → [Block Offset: u64]            │  │
│  │   ...                                                  │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Footer (64 bytes)                                           │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Index CRC32: u32                                       │  │
│  │ Data CRC32: u32                                        │  │
│  │ Total File Size: u64                                   │  │
│  │ Padding: [u8; 48]                                      │  │
│  │ Magic Number: 0x53535442 (validation)                 │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### 核心实现

```rust
// src/storage/sstable/oltp_rkyv.rs

use rkyv::{Archive, Serialize, Deserialize};
use memmap2::Mmap;

/// OLTP SSTable 写入器
pub struct OltpSstableWriter {
    /// 输出文件
    file: File,

    /// 当前偏移量
    current_offset: u64,

    /// 数据块缓冲
    block_buffer: Vec<u8>,

    /// 块大小阈值 (默认 64KB)
    block_size_threshold: usize,

    /// 稀疏索引 (每个块的第一个 key)
    sparse_index: BTreeMap<Vec<u8>, u64>,

    /// Bloom Filter 构建器
    bloom_builder: Option<BloomFilterBuilder>,

    /// 配置
    config: SstableConfig,
}

impl OltpSstableWriter {
    /// 创建新的 SSTable 写入器
    pub fn new(path: PathBuf, config: SstableConfig) -> Result<Self> {
        let mut file = File::create(&path)?;

        // 预留 Header 空间
        file.write_all(&[0u8; 32])?;

        Ok(Self {
            file,
            current_offset: 32,
            block_buffer: Vec::with_capacity(config.block_size),
            block_size_threshold: config.block_size,
            sparse_index: BTreeMap::new(),
            bloom_builder: if config.enable_bloom_filter {
                Some(BloomFilterBuilder::new(config.expected_entries))
            } else {
                None
            },
            config,
        })
    }

    /// 写入键值对
    pub fn write(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        // 记录块的第一个 key
        if self.block_buffer.is_empty() {
            self.sparse_index.insert(key.to_vec(), self.current_offset);
        }

        // 添加到 Bloom Filter
        if let Some(ref mut bloom) = self.bloom_builder {
            bloom.insert(key);
        }

        // 写入到块缓冲
        self.block_buffer.write_u32::<LittleEndian>(key.len() as u32)?;
        self.block_buffer.write_all(key)?;
        self.block_buffer.write_u32::<LittleEndian>(value.len() as u32)?;
        self.block_buffer.write_all(value)?;

        // 检查是否需要 flush 块
        if self.block_buffer.len() >= self.block_size_threshold {
            self.flush_block()?;
        }

        Ok(())
    }

    /// Flush 当前数据块到文件
    fn flush_block(&mut self) -> Result<()> {
        if self.block_buffer.is_empty() {
            return Ok(());
        }

        // 写入块数据
        self.file.write_all(&self.block_buffer)?;
        self.current_offset += self.block_buffer.len() as u64;

        // 清空缓冲
        self.block_buffer.clear();

        Ok(())
    }

    /// 完成写入，写入 Bloom Filter、索引和 Footer
    pub fn finish(mut self) -> Result<SstableMetadata> {
        // 1. Flush 最后一个块
        self.flush_block()?;

        let bloom_offset = self.current_offset;

        // 2. 写入 Bloom Filter
        let bloom_size = if let Some(bloom) = self.bloom_builder {
            let bloom_bytes = bloom.build().to_bytes();
            self.file.write_all(&bloom_bytes)?;
            bloom_bytes.len() as u64
        } else {
            0
        };

        self.current_offset += bloom_size;
        let index_offset = self.current_offset;

        // 3. 写入稀疏索引
        let index_bytes = rkyv::to_bytes::<_, 256>(&self.sparse_index)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        self.file.write_all(&index_bytes)?;
        self.current_offset += index_bytes.len() as u64;

        // 4. 计算 CRC
        let data_crc = self.compute_data_crc()?;
        let index_crc = crc32fast::hash(&index_bytes);

        // 5. 写入 Footer
        self.file.write_u32::<LittleEndian>(index_crc)?;
        self.file.write_u32::<LittleEndian>(data_crc)?;
        self.file.write_u64::<LittleEndian>(self.current_offset + 64)?;
        self.file.write_all(&[0u8; 48])?; // padding
        self.file.write_u32::<LittleEndian>(0x53535442)?; // magic

        // 6. 更新 Header
        self.file.seek(SeekFrom::Start(0))?;
        self.write_header(bloom_offset, index_offset)?;

        // 7. Sync to disk
        self.file.sync_all()?;

        Ok(SstableMetadata {
            num_entries: self.sparse_index.len() as u64,
            file_size: self.current_offset + 64,
            bloom_filter_size: bloom_size,
            index_size: index_bytes.len() as u64,
        })
    }

    fn write_header(&mut self, bloom_offset: u64, index_offset: u64) -> Result<()> {
        self.file.write_u32::<LittleEndian>(0x53535442)?; // magic
        self.file.write_u32::<LittleEndian>(1)?; // version
        self.file.write_i64::<LittleEndian>(chrono::Utc::now().timestamp())?;
        self.file.write_u64::<LittleEndian>(self.sparse_index.len() as u64)?;
        self.file.write_u64::<LittleEndian>(bloom_offset)?;
        self.file.write_u64::<LittleEndian>(index_offset)?;
        Ok(())
    }

    fn compute_data_crc(&mut self) -> Result<u32> {
        self.file.seek(SeekFrom::Start(32))?;
        let mut hasher = crc32fast::Hasher::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let n = self.file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(hasher.finalize())
    }
}

/// OLTP SSTable 读取器 (mmap 零拷贝)
pub struct OltpSstableReader {
    /// 内存映射文件
    mmap: Mmap,

    /// 稀疏索引 (反序列化后的)
    sparse_index: BTreeMap<Vec<u8>, u64>,

    /// Bloom Filter
    bloom_filter: Option<BloomFilter>,

    /// Header 信息
    header: SstableHeader,
}

impl OltpSstableReader {
    /// 打开 SSTable 文件
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // 读取并验证 Header
        let header = Self::read_header(&mmap)?;

        // 读取 Bloom Filter
        let bloom_filter = if header.bloom_offset > 0 {
            let bloom_bytes = &mmap[header.bloom_offset as usize..header.index_offset as usize];
            Some(BloomFilter::from_bytes(bloom_bytes)?)
        } else {
            None
        };

        // 读取稀疏索引
        let index_bytes = &mmap[header.index_offset as usize..];
        let sparse_index = rkyv::from_bytes::<BTreeMap<Vec<u8>, u64>>(index_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Self {
            mmap,
            sparse_index,
            bloom_filter,
            header,
        })
    }

    /// 点查询 (零拷贝)
    pub fn get(&self, key: &[u8]) -> Result<Option<&[u8]>> {
        // 1. Bloom Filter 快速过滤
        if let Some(ref bloom) = self.bloom_filter {
            if !bloom.contains(key) {
                return Ok(None); // 一定不存在
            }
        }

        // 2. 定位数据块
        let block_offset = self.find_block(key)?;
        if block_offset.is_none() {
            return Ok(None);
        }

        let block_start = block_offset.unwrap() as usize;

        // 3. 在块内二分查找
        self.search_in_block(block_start, key)
    }

    /// 范围扫描
    pub fn scan(&self, start: &[u8], end: &[u8]) -> Result<Vec<(&[u8], &[u8])>> {
        let mut results = Vec::new();

        // 定位起始块
        let start_block = self.find_block(start)?.unwrap_or(32);

        // 遍历所有可能的块
        for (block_key, block_offset) in self.sparse_index.range(start.to_vec()..) {
            if block_key.as_slice() >= end {
                break;
            }

            // 扫描块内数据
            let block_results = self.scan_block(*block_offset as usize, start, end)?;
            results.extend(block_results);
        }

        Ok(results)
    }

    fn find_block(&self, key: &[u8]) -> Result<Option<u64>> {
        // 使用稀疏索引找到包含 key 的块
        let mut iter = self.sparse_index.range(..=key.to_vec());
        Ok(iter.next_back().map(|(_, offset)| *offset))
    }

    fn search_in_block(&self, block_start: usize, target_key: &[u8]) -> Result<Option<&[u8]>> {
        let mut cursor = block_start;

        loop {
            // 检查是否超出块边界
            if cursor >= self.mmap.len() {
                return Ok(None);
            }

            // 读取 key
            let key_len = u32::from_le_bytes([
                self.mmap[cursor],
                self.mmap[cursor + 1],
                self.mmap[cursor + 2],
                self.mmap[cursor + 3],
            ]) as usize;
            cursor += 4;

            let key = &self.mmap[cursor..cursor + key_len];
            cursor += key_len;

            // 读取 value
            let value_len = u32::from_le_bytes([
                self.mmap[cursor],
                self.mmap[cursor + 1],
                self.mmap[cursor + 2],
                self.mmap[cursor + 3],
            ]) as usize;
            cursor += 4;

            let value = &self.mmap[cursor..cursor + value_len];
            cursor += value_len;

            // 比较 key
            match key.cmp(target_key) {
                Ordering::Equal => return Ok(Some(value)), // 找到！零拷贝返回
                Ordering::Greater => return Ok(None),      // 已超过，不存在
                Ordering::Less => continue,                // 继续查找
            }
        }
    }

    fn scan_block(&self, block_start: usize, start: &[u8], end: &[u8])
        -> Result<Vec<(&[u8], &[u8])>>
    {
        let mut results = Vec::new();
        let mut cursor = block_start;

        loop {
            if cursor >= self.mmap.len() {
                break;
            }

            // 读取 entry
            let key_len = u32::from_le_bytes([
                self.mmap[cursor],
                self.mmap[cursor + 1],
                self.mmap[cursor + 2],
                self.mmap[cursor + 3],
            ]) as usize;
            cursor += 4;

            let key = &self.mmap[cursor..cursor + key_len];
            cursor += key_len;

            let value_len = u32::from_le_bytes([
                self.mmap[cursor],
                self.mmap[cursor + 1],
                self.mmap[cursor + 2],
                self.mmap[cursor + 3],
            ]) as usize;
            cursor += 4;

            let value = &self.mmap[cursor..cursor + value_len];
            cursor += value_len;

            // 检查范围
            if key >= start && key < end {
                results.push((key, value));
            } else if key >= end {
                break;
            }
        }

        Ok(results)
    }

    fn read_header(mmap: &Mmap) -> Result<SstableHeader> {
        let mut cursor = 0;

        let magic = u32::from_le_bytes([mmap[0], mmap[1], mmap[2], mmap[3]]);
        if magic != 0x53535442 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic number"));
        }
        cursor += 4;

        let version = u32::from_le_bytes([mmap[4], mmap[5], mmap[6], mmap[7]]);
        cursor += 4;

        let created_at = i64::from_le_bytes([
            mmap[8], mmap[9], mmap[10], mmap[11],
            mmap[12], mmap[13], mmap[14], mmap[15],
        ]);
        cursor += 8;

        let num_entries = u64::from_le_bytes([
            mmap[16], mmap[17], mmap[18], mmap[19],
            mmap[20], mmap[21], mmap[22], mmap[23],
        ]);
        cursor += 8;

        let bloom_offset = u64::from_le_bytes([
            mmap[24], mmap[25], mmap[26], mmap[27],
            mmap[28], mmap[29], mmap[30], mmap[31],
        ]);
        cursor += 8;

        let index_offset = u64::from_le_bytes([
            mmap[32], mmap[33], mmap[34], mmap[35],
            mmap[36], mmap[37], mmap[38], mmap[39],
        ]);

        Ok(SstableHeader {
            magic,
            version,
            created_at,
            num_entries,
            bloom_offset,
            index_offset,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SstableHeader {
    pub magic: u32,
    pub version: u32,
    pub created_at: i64,
    pub num_entries: u64,
    pub bloom_offset: u64,
    pub index_offset: u64,
}

#[derive(Debug, Clone)]
pub struct SstableMetadata {
    pub num_entries: u64,
    pub file_size: u64,
    pub bloom_filter_size: u64,
    pub index_size: u64,
}

#[derive(Debug, Clone)]
pub struct SstableConfig {
    /// 数据块大小 (默认 64KB)
    pub block_size: usize,

    /// 是否启用 Bloom Filter
    pub enable_bloom_filter: bool,

    /// 预期条目数 (用于 Bloom Filter 大小计算)
    pub expected_entries: usize,
}

impl Default for SstableConfig {
    fn default() -> Self {
        Self {
            block_size: 64 * 1024,
            enable_bloom_filter: true,
            expected_entries: 10000,
        }
    }
}
```

#### 性能特性

**写入性能**:
- 批量写入: > 100K entries/sec
- 块缓冲: 减少系统调用
- 顺序写入: SSD/HDD 友好

**读取性能** (Phase 7 优化后):
- 点查询: **P99 < 20μs** (mmap)
- Bloom Filter: ~100ns 过滤
- 零拷贝: 直接返回 mmap 切片

### 2. OLAP SSTable (Parquet 格式)

#### 设计理念

- **目标场景**: 批量扫描、聚合分析、BI 报表
- **文件格式**: Apache Parquet
- **压缩算法**: Snappy / Zstd
- **典型吞吐**: > 1.5 GB/s

#### 核心实现

```rust
// src/storage/sstable/olap_parquet.rs

use arrow2::array::*;
use arrow2::chunk::Chunk;
use arrow2::datatypes::*;
use arrow2::io::parquet::write::*;

/// OLAP SSTable 写入器
pub struct OlapSstableWriter {
    /// 输出路径
    path: PathBuf,

    /// Arrow Schema
    schema: Schema,

    /// 列数据缓冲
    columns: Vec<Vec<Box<dyn Array>>>,

    /// 当前行数
    row_count: usize,

    /// Row Group 大小 (默认 100K 行)
    row_group_size: usize,
}

impl OlapSstableWriter {
    pub fn new(path: PathBuf, schema: Schema) -> Result<Self> {
        Ok(Self {
            path,
            schema,
            columns: vec![Vec::new(); schema.fields.len()],
            row_count: 0,
            row_group_size: 100_000,
        })
    }

    /// 写入 RecordBatch
    pub fn write_batch(&mut self, batch: Chunk<Box<dyn Array>>) -> Result<()> {
        for (i, column) in batch.columns().iter().enumerate() {
            self.columns[i].push(column.clone());
        }

        self.row_count += batch.len();
        Ok(())
    }

    /// 完成写入
    pub fn finish(self) -> Result<()> {
        let file = File::create(&self.path)?;

        // Parquet 写入配置
        let options = WriteOptions {
            write_statistics: true,
            compression: CompressionOptions::Snappy, // 或 Zstd
            version: Version::V2,
            data_pagesize_limit: Some(64 * 1024), // 64KB
        };

        // 构建 Row Groups
        let row_groups = self.build_row_groups()?;

        // 写入 Parquet
        let mut writer = FileWriter::try_new(file, self.schema, options)?;

        for row_group in row_groups {
            writer.write(row_group)?;
        }

        writer.end(None)?;

        Ok(())
    }

    fn build_row_groups(&self) -> Result<Vec<RowGroup>> {
        // 将列数据切分为多个 Row Group
        let num_row_groups = (self.row_count + self.row_group_size - 1) / self.row_group_size;
        let mut row_groups = Vec::with_capacity(num_row_groups);

        for i in 0..num_row_groups {
            let start_row = i * self.row_group_size;
            let end_row = ((i + 1) * self.row_group_size).min(self.row_count);

            // 切片列数据
            let mut row_group_columns = Vec::new();
            for col_arrays in &self.columns {
                let sliced = self.slice_arrays(col_arrays, start_row, end_row)?;
                row_group_columns.push(sliced);
            }

            row_groups.push(RowGroup {
                columns: row_group_columns,
                num_rows: end_row - start_row,
            });
        }

        Ok(row_groups)
    }

    fn slice_arrays(&self, arrays: &[Box<dyn Array>], start: usize, end: usize)
        -> Result<Box<dyn Array>>
    {
        // 合并并切片数组
        let concatenated = concatenate(arrays)?;
        Ok(concatenated.sliced(start, end - start))
    }
}

/// OLAP SSTable 读取器
pub struct OlapSstableReader {
    path: PathBuf,
    schema: Schema,
    metadata: FileMetadata,
}

impl OlapSstableReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let metadata = read_metadata(&mut BufReader::new(&file))?;
        let schema = infer_schema(&metadata)?;

        Ok(Self {
            path: path.to_path_buf(),
            schema,
            metadata,
        })
    }

    /// 读取所有数据
    pub fn read_all(&self) -> Result<Chunk<Box<dyn Array>>> {
        let file = File::open(&self.path)?;
        let reader = FileReader::new(file, self.metadata.row_groups.clone(), self.schema.clone(), None, None, None);

        let mut chunks = Vec::new();
        for maybe_chunk in reader {
            chunks.push(maybe_chunk?);
        }

        // 合并所有 chunks
        concatenate_chunks(&chunks)
    }

    /// 读取指定列
    pub fn read_columns(&self, column_names: &[&str]) -> Result<Chunk<Box<dyn Array>>> {
        let column_indices: Vec<_> = column_names
            .iter()
            .map(|name| self.schema.fields.iter().position(|f| f.name == *name))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Column not found"))?;

        let file = File::open(&self.path)?;
        let reader = FileReader::new(
            file,
            self.metadata.row_groups.clone(),
            self.schema.clone(),
            Some(column_indices),
            None,
            None
        );

        let mut chunks = Vec::new();
        for maybe_chunk in reader {
            chunks.push(maybe_chunk?);
        }

        concatenate_chunks(&chunks)
    }

    /// 带谓词下推的读取
    pub fn read_with_predicate<F>(&self, predicate: F) -> Result<Chunk<Box<dyn Array>>>
    where
        F: Fn(&Chunk<Box<dyn Array>>) -> Result<BooleanArray>,
    {
        let file = File::open(&self.path)?;
        let reader = FileReader::new(file, self.metadata.row_groups.clone(), self.schema.clone(), None, None, None);

        let mut filtered_chunks = Vec::new();

        for maybe_chunk in reader {
            let chunk = maybe_chunk?;
            let mask = predicate(&chunk)?;
            let filtered = filter_chunk(&chunk, &mask)?;
            filtered_chunks.push(filtered);
        }

        concatenate_chunks(&filtered_chunks)
    }
}

fn concatenate_chunks(chunks: &[Chunk<Box<dyn Array>>]) -> Result<Chunk<Box<dyn Array>>> {
    if chunks.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No chunks to concatenate"));
    }

    let num_columns = chunks[0].columns().len();
    let mut result_columns = Vec::with_capacity(num_columns);

    for col_idx in 0..num_columns {
        let column_arrays: Vec<_> = chunks.iter()
            .map(|chunk| chunk.columns()[col_idx].as_ref())
            .collect();

        let concatenated = concatenate(&column_arrays)?;
        result_columns.push(concatenated);
    }

    Ok(Chunk::new(result_columns))
}

fn filter_chunk(chunk: &Chunk<Box<dyn Array>>, mask: &BooleanArray) -> Result<Chunk<Box<dyn Array>>> {
    let filtered_columns: Vec<_> = chunk.columns()
        .iter()
        .map(|col| filter(col.as_ref(), mask))
        .collect::<Result<_, _>>()?;

    Ok(Chunk::new(filtered_columns))
}
```

#### 性能特性

**压缩效果**:
- Snappy: 2-4x 压缩率, 低 CPU 开销
- Zstd: 5-10x 压缩率, 高 CPU 开销

**扫描性能**:
- 列式扫描: > 10M rows/sec
- 全表扫描: > 1.5 GB/s
- 谓词下推: 跳过不匹配的 Row Group

## 🌸 Bloom Filter

### 设计

```rust
// src/storage/sstable/bloom.rs

use bit_vec::BitVec;

pub struct BloomFilter {
    /// 位数组
    bits: BitVec,

    /// 哈希函数数量
    num_hashes: usize,

    /// 位数组大小
    num_bits: usize,
}

impl BloomFilter {
    /// 创建 Bloom Filter
    /// - `expected_items`: 预期元素数量
    /// - `false_positive_rate`: 假阳率 (默认 0.01 = 1%)
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // 计算最优参数
        let num_bits = Self::optimal_num_bits(expected_items, false_positive_rate);
        let num_hashes = Self::optimal_num_hashes(num_bits, expected_items);

        Self {
            bits: BitVec::from_elem(num_bits, false),
            num_hashes,
            num_bits,
        }
    }

    /// 插入元素
    pub fn insert(&mut self, key: &[u8]) {
        for i in 0..self.num_hashes {
            let hash = self.hash(key, i);
            let bit_index = (hash % self.num_bits as u64) as usize;
            self.bits.set(bit_index, true);
        }
    }

    /// 检查元素是否可能存在
    pub fn contains(&self, key: &[u8]) -> bool {
        for i in 0..self.num_hashes {
            let hash = self.hash(key, i);
            let bit_index = (hash % self.num_bits as u64) as usize;
            if !self.bits.get(bit_index).unwrap_or(false) {
                return false; // 一定不存在
            }
        }
        true // 可能存在
    }

    /// 哈希函数 (double hashing)
    fn hash(&self, key: &[u8], i: usize) -> u64 {
        let hash1 = seahash::hash(key);
        let hash2 = seahash::hash(&hash1.to_le_bytes());
        hash1.wrapping_add(i as u64 * hash2)
    }

    /// 计算最优位数量
    fn optimal_num_bits(n: usize, p: f64) -> usize {
        let ln2 = std::f64::consts::LN_2;
        (-(n as f64) * p.ln() / (ln2 * ln2)).ceil() as usize
    }

    /// 计算最优哈希函数数量
    fn optimal_num_hashes(m: usize, n: usize) -> usize {
        ((m as f64 / n as f64) * std::f64::consts::LN_2).ceil() as usize
    }

    /// 序列化
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_u64::<LittleEndian>(self.num_bits as u64).unwrap();
        bytes.write_u64::<LittleEndian>(self.num_hashes as u64).unwrap();
        bytes.extend_from_slice(&self.bits.to_bytes());
        bytes
    }

    /// 反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        let num_bits = cursor.read_u64::<LittleEndian>()? as usize;
        let num_hashes = cursor.read_u64::<LittleEndian>()? as usize;

        let mut bit_bytes = Vec::new();
        cursor.read_to_end(&mut bit_bytes)?;
        let bits = BitVec::from_bytes(&bit_bytes);

        Ok(Self {
            bits,
            num_hashes,
            num_bits,
        })
    }
}
```

### 性能分析

**查找延迟**: ~100ns (7 次哈希)

**假阳率**: 1% (可配置)
- 10,000 条目: ~12 KB
- 100,000 条目: ~120 KB
- 1,000,000 条目: ~1.2 MB

**收益**:
- 避免无效磁盘 I/O
- 加速 99% 的负查询

## 📊 Compaction

### Leveled Compaction 策略

```
Level 0:  [SST1] [SST2] [SST3] [SST4]  ← 可能有重叠
            ↓ Compaction
Level 1:  [SST5──────SST6──────SST7]   ← 无重叠, 10 MB/file
            ↓ Compaction
Level 2:  [SST8──────SST9──────SST10─────SST11]  ← 100 MB/file
            ↓ Compaction
Level 3:  [SST12──────────────SST13──────────────SST14]  ← 1 GB/file
```

### 触发条件

```rust
pub struct CompactionTrigger {
    /// Level 0 文件数阈值
    l0_file_threshold: usize,  // 默认 4

    /// 各层大小阈值
    level_size_multiplier: usize,  // 默认 10
}

impl CompactionTrigger {
    pub fn should_compact(&self, level: usize, file_count: usize, total_size: u64) -> bool {
        match level {
            0 => file_count >= self.l0_file_threshold,
            n => total_size >= self.level_target_size(n),
        }
    }

    fn level_target_size(&self, level: usize) -> u64 {
        10 * 1024 * 1024 * (self.level_size_multiplier.pow(level as u32 - 1)) as u64
    }
}
```

## 🛠️ 配置示例

```toml
# config/storage.toml
[sstable.oltp]
block_size_kb = 64
enable_bloom_filter = true
expected_entries_per_file = 100000
bloom_false_positive_rate = 0.01

[sstable.olap]
row_group_size = 100000
compression = "snappy"  # or "zstd", "none"
data_page_size_kb = 64

[compaction]
l0_file_threshold = 4
level_size_multiplier = 10
max_background_compactions = 2
```

## 📈 性能基准

### OLTP SSTable

| 操作 | 无 Bloom Filter | 有 Bloom Filter | 优化 (%) |
|------|----------------|----------------|---------|
| 点查询 (存在) | 45 μs | 22 μs | +52% |
| 点查询 (不存在) | 42 μs | 0.1 μs | +99.8% |
| 范围扫描 (1K) | 850 μs | 850 μs | 0% |

### OLAP SSTable

| 操作 | Snappy | Zstd | 无压缩 |
|------|--------|------|--------|
| 写入速度 | 1.2 GB/s | 800 MB/s | 2 GB/s |
| 读取速度 | 1.5 GB/s | 1.3 GB/s | 3 GB/s |
| 压缩率 | 3.5x | 8x | 1x |
| 磁盘占用 | 286 MB | 125 MB | 1 GB |

## 💡 最佳实践

### 1. 选择合适的 SSTable 类型

```rust
// OLTP: 需要低延迟点查询
if use_case.requires_low_latency() {
    use_oltp_sstable();
}

// OLAP: 需要批量扫描和分析
if use_case.is_analytical() {
    use_olap_sstable();
}
```

### 2. Bloom Filter 参数调优

```rust
// 高假阳率 → 小内存占用,但更多磁盘 I/O
let bloom = BloomFilter::new(entries, 0.05); // 5% FP rate

// 低假阳率 → 大内存占用,但少磁盘 I/O
let bloom = BloomFilter::new(entries, 0.001); // 0.1% FP rate
```

### 3. 压缩算法选择

```rust
// Snappy: 平衡性能和压缩率
config.compression = CompressionOptions::Snappy;

// Zstd: 高压缩率,适合冷数据
config.compression = CompressionOptions::Zstd;

// 无压缩: 最高性能,但磁盘占用大
config.compression = CompressionOptions::None;
```

## 🔍 故障排查

### 问题 1: SSTable 读取缓慢

**症状**: P99 延迟 > 100μs

**排查步骤**:
1. 检查 Bloom Filter 是否启用
2. 检查 mmap 是否生效
3. 检查稀疏索引是否过大

**解决方案**:
```rust
// 启用 Bloom Filter
config.enable_bloom_filter = true;

// 减小块大小 (增加索引密度)
config.block_size = 32 * 1024; // 32KB
```

### 问题 2: Compaction 阻塞写入

**症状**: 写入延迟突然升高

**排查步骤**:
1. 检查 L0 文件数
2. 检查 Compaction 线程是否繁忙

**解决方案**:
```rust
// 增加 L0 文件阈值
config.l0_file_threshold = 8;

// 增加后台 Compaction 线程
config.max_background_compactions = 4;
```

## 📚 相关文档

- [WAL 设计](wal.md) - SSTable 的数据来源
- [MemTable 实现](memtable.md) - flush 到 SSTable
- [查询引擎](query_engine.md) - 如何查询 SSTable
- [Compaction 详细设计](../../storage/01_STORAGE_ARCHITECTURE.md#compaction) - 压缩策略
- [Bloom Filter 论文](https://en.wikipedia.org/wiki/Bloom_filter) - 原理详解

---

[返回核心模块](../README.md) | [返回文档中心](../../README.md)
