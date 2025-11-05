// OLAP SSTable - Parquet 列式存储
//
// 设计理念:
// - 列式压缩存储，高压缩率
// - 支持谓词下推（predicate pushdown）
// - 适合大规模扫描和聚合查询
// - 不可变文件

use arrow2::array::{Array, BooleanArray, MutableBooleanArray, MutableArray, MutableFixedSizeBinaryArray};
use arrow2::chunk::Chunk;
use arrow2::datatypes::Schema;
use arrow2::io::parquet::read::{FileReader, read_metadata, infer_schema};
use arrow2::io::parquet::write::{
    CompressionOptions, Encoding, FileWriter, Version, WriteOptions,
    RowGroupIterator, DynIter, DynStreamingIterator,
};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::types::SSTableMetadata;

/// Parquet SSTable Writer
///
/// 将 Arrow2 Chunk 批量写入 Parquet 文件
pub struct ParquetSSTableWriter {
    file_path: PathBuf,
    schema: Arc<Schema>,
    writer: Option<FileWriter<File>>,
    entry_count: u64,
    min_timestamp: Option<i64>,
    max_timestamp: Option<i64>,
}

impl ParquetSSTableWriter {
    /// 创建新的 Parquet Writer
    ///
    /// # Arguments
    /// * `file_path` - 输出文件路径
    /// * `schema` - Arrow2 Schema
    pub fn create<P: AsRef<Path>>(file_path: P, schema: Arc<Schema>) -> Result<Self, String> {
        let file_path = file_path.as_ref().to_path_buf();

        // 确保目录存在
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Create dir failed: {}", e))?;
        }

        // 创建文件
        let file = File::create(&file_path)
            .map_err(|e| format!("Create parquet file failed: {}", e))?;

        // Parquet 写入选项（简化版本，使用 Snappy 压缩）
        let options = WriteOptions {
            write_statistics: true,    // 写入统计信息（min/max 等）
            compression: CompressionOptions::Snappy, // Snappy 压缩
            version: Version::V2,      // Parquet 2.0
            data_pagesize_limit: None,
        };

        // 创建 Parquet Writer
        // Arrow2 FileWriter 接受 schema 作为 &Schema，不是 Arc<Schema>
        let _encodings: Vec<Vec<Encoding>> = schema.fields.iter().map(|_| vec![Encoding::Plain]).collect();

        let writer = FileWriter::try_new(file, (*schema).clone(), options)
            .map_err(|e| format!("Create parquet writer failed: {}", e))?;

        Ok(Self {
            file_path,
            schema,
            writer: Some(writer),
            entry_count: 0,
            min_timestamp: None,
            max_timestamp: None,
        })
    }

    /// 写入一个 Chunk
    ///
    /// # Arguments
    /// * `chunk` - Arrow2 Chunk
    pub fn write_chunk(&mut self, chunk: &Chunk<Box<dyn Array>>) -> Result<(), String> {
        if chunk.is_empty() {
            return Ok(());
        }

        // 提取时间戳统计
        if let Some(timestamp_array) = chunk.arrays()[0]
            .as_any()
            .downcast_ref::<arrow2::array::PrimitiveArray<i64>>()
        {
            if let Some(&first_ts) = timestamp_array.iter().next().and_then(|v| v) {
                self.min_timestamp = Some(self.min_timestamp.unwrap_or(first_ts).min(first_ts));
            }
            if let Some(&last_ts) = timestamp_array.iter().last().and_then(|v| v) {
                self.max_timestamp = Some(self.max_timestamp.unwrap_or(last_ts).max(last_ts));
            }
        }

        self.entry_count += chunk.len() as u64;

        // 写入 Chunk
        let writer = self.writer.as_mut().ok_or("Writer already finished")?;

        // Arrow2 需要 RowGroupIterator，这里创建一个单行组
        let iter = vec![Ok(chunk.clone())];
        let row_groups = RowGroupIterator::try_new(
            iter.into_iter(),
            &self.schema,
            WriteOptions {
                write_statistics: true,
                compression: CompressionOptions::Snappy,
                version: Version::V2,
                data_pagesize_limit: None,
            },
            vec![vec![Encoding::Plain]; self.schema.fields.len()],
        ).map_err(|e| format!("Create row group iterator failed: {}", e))?;

        for group in row_groups {
            writer.write(group.map_err(|e| format!("Write row group failed: {}", e))?)
                .map_err(|e| format!("Write failed: {}", e))?;
        }

        Ok(())
    }

    /// 完成写入并关闭文件
    ///
    /// 返回 SSTable 元数据
    pub fn finish(mut self) -> Result<SSTableMetadata, String> {
        let mut writer = self.writer.take().ok_or("Writer already finished")?;

        // 结束写入
        let _size = writer
            .end(None)
            .map_err(|e| format!("Finish parquet writer failed: {}", e))?;

        let file_size = std::fs::metadata(&self.file_path)
            .map_err(|e| format!("Get file size failed: {}", e))?
            .len();

        Ok(SSTableMetadata {
            version: 2, // Parquet format
            entry_count: self.entry_count,
            min_timestamp: self.min_timestamp.unwrap_or(0),
            max_timestamp: self.max_timestamp.unwrap_or(0),
            min_key: Vec::new(), // OLAP不需要key range（只用于OLTP compaction）
            max_key: Vec::new(),
            file_size,
            block_offsets: Vec::new(), // Parquet 有自己的 Row Group 管理
            bloom_filter: None,
            created_at: chrono::Utc::now().timestamp(),
        })
    }
}

/// Parquet SSTable Reader
///
/// 从 Parquet 文件读取数据
pub struct ParquetSSTable {
    file_path: PathBuf,
    metadata: SSTableMetadata,
    schema: Arc<Schema>,
}

impl ParquetSSTable {
    /// 打开 Parquet SSTable
    pub fn open<P: AsRef<Path>>(file_path: P) -> Result<Self, String> {
        let file_path = file_path.as_ref().to_path_buf();

        // 读取 Parquet 元数据
        let mut file = File::open(&file_path)
            .map_err(|e| format!("Open parquet file failed: {}", e))?;

        let parquet_metadata = read_metadata(&mut file)
            .map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        let schema = infer_schema(&parquet_metadata)
            .map_err(|e| format!("Infer schema failed: {}", e))?;

        // 提取统计信息
        let mut entry_count = 0u64;
        let mut min_timestamp = i64::MAX;
        let mut max_timestamp = i64::MIN;

        // 遍历所有row groups，提取时间戳统计信息
        for row_group in parquet_metadata.row_groups.iter() {
            entry_count += row_group.num_rows() as u64;

            // 查找timestamp列（通常是第一列或名为"timestamp"的列）
            for (col_idx, column) in row_group.columns().iter().enumerate() {
                // 获取列名
                let col_name = &parquet_metadata.schema().fields()[col_idx].name;

                // 如果是timestamp列，提取统计信息
                if col_name == "timestamp" || col_name == "time" || col_idx == 0 {
                    if let Some(stats) = column.metadata().statistics() {
                        // 尝试提取i64类型的统计信息
                        if let Some(min_val) = stats.min_value.as_ref() {
                            if min_val.len() >= 8 {
                                let val = i64::from_le_bytes(min_val[0..8].try_into().unwrap_or([0; 8]));
                                min_timestamp = min_timestamp.min(val);
                            }
                        }
                        if let Some(max_val) = stats.max_value.as_ref() {
                            if max_val.len() >= 8 {
                                let val = i64::from_le_bytes(max_val[0..8].try_into().unwrap_or([0; 8]));
                                max_timestamp = max_timestamp.max(val);
                            }
                        }
                    }
                    break; // 找到timestamp列后退出
                }
            }
        }

        // 如果没有找到有效的时间戳，使用默认值
        if min_timestamp == i64::MAX {
            min_timestamp = 0;
        }
        if max_timestamp == i64::MIN {
            max_timestamp = 0;
        }

        let file_size = std::fs::metadata(&file_path)
            .map_err(|e| format!("Get file size failed: {}", e))?
            .len();

        let metadata = SSTableMetadata {
            version: 2,
            entry_count,
            min_timestamp,
            max_timestamp,
            min_key: Vec::new(), // OLAP不需要key range（只用于OLTP compaction）
            max_key: Vec::new(),
            file_size,
            block_offsets: Vec::new(),
            bloom_filter: None,
            created_at: chrono::Utc::now().timestamp(),
        };

        Ok(Self {
            file_path,
            metadata,
            schema: Arc::new(schema),
        })
    }

    /// 获取元数据
    pub fn metadata(&self) -> &SSTableMetadata {
        &self.metadata
    }

    /// 获取 Schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// 范围查询
    ///
    /// 返回时间戳范围内的所有 Chunk
    ///
    /// # Arguments
    /// * `start_ts` - 起始时间戳（纳秒）
    /// * `end_ts` - 结束时间戳（纳秒）
    pub fn range_query(&self, start_ts: i64, end_ts: i64) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        // 快速路径：时间范围不重叠
        if self.metadata.max_timestamp != 0 &&
           (end_ts < self.metadata.min_timestamp || start_ts > self.metadata.max_timestamp) {
            return Ok(Vec::new());
        }

        let mut file = File::open(&self.file_path)
            .map_err(|e| format!("Open parquet file for query failed: {}", e))?;

        let parquet_metadata = read_metadata(&mut file)
            .map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        let reader = FileReader::new(file, parquet_metadata.row_groups, (*self.schema).clone(), None, None, None);

        let mut chunks = Vec::new();

        // 读取所有 Row Groups
        for chunk_result in reader {
            let chunk = chunk_result.map_err(|e| format!("Read chunk failed: {}", e))?;

            // 过滤：只保留时间戳在范围内的行
            let filtered_chunk = filter_chunk_by_timestamp(&chunk, start_ts, end_ts)?;

            if !filtered_chunk.is_empty() {
                chunks.push(filtered_chunk);
            }
        }

        Ok(chunks)
    }

    /// 扫描整个文件（用于全量查询）
    pub fn scan(&self) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        let mut file = File::open(&self.file_path)
            .map_err(|e| format!("Open parquet file for scan failed: {}", e))?;

        let parquet_metadata = read_metadata(&mut file)
            .map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        let reader = FileReader::new(file, parquet_metadata.row_groups, (*self.schema).clone(), None, None, None);

        let chunks: Result<Vec<_>, _> = reader.collect();
        chunks.map_err(|e| format!("Read chunks failed: {}", e))
    }
}

/// 按时间戳过滤 Chunk
fn filter_chunk_by_timestamp(
    chunk: &Chunk<Box<dyn Array>>,
    start_ts: i64,
    end_ts: i64,
) -> Result<Chunk<Box<dyn Array>>, String> {
    if chunk.is_empty() {
        return Ok(chunk.clone());
    }

    // 获取时间戳列
    let timestamp_array = chunk.arrays()[0]
        .as_any()
        .downcast_ref::<arrow2::array::PrimitiveArray<i64>>()
        .ok_or("Timestamp column not found")?;

    // 构建 boolean mask
    let mut mask_builder = MutableBooleanArray::new();
    for ts_opt in timestamp_array.iter() {
        let is_in_range = ts_opt.map(|&ts| ts >= start_ts && ts <= end_ts).unwrap_or(false);
        mask_builder.push(Some(is_in_range));
    }
    let mask: BooleanArray = mask_builder.into();

    // 应用 filter 到所有列
    // 手动实现 filter：根据 mask 选择行
    let true_indices: Vec<usize> = mask
        .values_iter()
        .enumerate()
        .filter_map(|(i, v)| if v { Some(i) } else { None })
        .collect();

    if true_indices.is_empty() {
        // 没有匹配的行，返回空 chunk
        return Ok(Chunk::new(vec![]));
    }

    // 对每个数组应用索引过滤
    use arrow2::array::{PrimitiveArray, FixedSizeBinaryArray};
    let filtered_arrays: Vec<Box<dyn Array>> = chunk
        .arrays()
        .iter()
        .map(|array| {
            // 根据类型处理不同的数组
            if let Some(prim_i64) = array.as_any().downcast_ref::<PrimitiveArray<i64>>() {
                let filtered: PrimitiveArray<i64> = true_indices
                    .iter()
                    .map(|&idx| prim_i64.get(idx))
                    .collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_i32) = array.as_any().downcast_ref::<PrimitiveArray<i32>>() {
                let filtered: PrimitiveArray<i32> = true_indices
                    .iter()
                    .map(|&idx| prim_i32.get(idx))
                    .collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_u64) = array.as_any().downcast_ref::<PrimitiveArray<u64>>() {
                let filtered: PrimitiveArray<u64> = true_indices
                    .iter()
                    .map(|&idx| prim_u64.get(idx))
                    .collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_u8) = array.as_any().downcast_ref::<PrimitiveArray<u8>>() {
                let filtered: PrimitiveArray<u8> = true_indices
                    .iter()
                    .map(|&idx| prim_u8.get(idx))
                    .collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_f64) = array.as_any().downcast_ref::<PrimitiveArray<f64>>() {
                let filtered: PrimitiveArray<f64> = true_indices
                    .iter()
                    .map(|&idx| prim_f64.get(idx))
                    .collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(fixed_bin) = array.as_any().downcast_ref::<FixedSizeBinaryArray>() {
                let size = fixed_bin.size();
                let mut builder = MutableFixedSizeBinaryArray::with_capacity(size, true_indices.len());
                for &idx in &true_indices {
                    builder.push(fixed_bin.get(idx));
                }
                let filtered: FixedSizeBinaryArray = builder.into();
                Box::new(filtered) as Box<dyn Array>
            } else {
                // 未知类型，panic以便调试
                panic!("Unsupported array type in filter_chunk_by_timestamp: {:?}", array.data_type())
            }
        })
        .collect();

    Ok(Chunk::new(filtered_arrays))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memtable::olap::{create_olap_schema, OlapMemTable};
    use crate::storage::memtable::types::MemTableKey;
    use crate::storage::wal::record::WalRecord;

    fn create_test_records(count: usize) -> Vec<(MemTableKey, WalRecord)> {
        (0..count)
            .map(|i| {
                let key = MemTableKey {
                    timestamp: 1000 + i as i64,
                    sequence: i as u64,
                };

                let record = WalRecord::OrderInsert {
                    order_id: i as u64,
                    user_id: [1u8; 32],
                    instrument_id: [2u8; 16],
                    direction: 0,
                    offset: 0,
                    price: 100.0 + i as f64,
                    volume: 10.0,
                    timestamp: key.timestamp,
                };

                (key, record)
            })
            .collect()
    }

    #[test]
    fn test_write_and_read() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = tmp_dir.path().join("test.parquet");

        // 构建数据
        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        // 写入 Parquet
        {
            let mut writer = ParquetSSTableWriter::create(
                &file_path,
                Arc::new(create_olap_schema()),
            )
            .unwrap();

            writer.write_chunk(memtable.chunk()).unwrap();

            let metadata = writer.finish().unwrap();
            assert_eq!(metadata.entry_count, 100);
            assert_eq!(metadata.min_timestamp, 1000);
            assert_eq!(metadata.max_timestamp, 1099);
        }

        // 读取 Parquet
        {
            let sstable = ParquetSSTable::open(&file_path).unwrap();
            assert_eq!(sstable.metadata().entry_count, 100);

            let chunks = sstable.range_query(1010, 1020).unwrap();
            assert!(!chunks.is_empty());

            // 验证返回的数据量
            let total_rows: usize = chunks.iter().map(|c| c.len()).sum();
            assert_eq!(total_rows, 11); // 1010-1020 inclusive
        }
    }

    #[test]
    fn test_range_query_no_overlap() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = tmp_dir.path().join("test.parquet");

        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        let mut writer = ParquetSSTableWriter::create(
            &file_path,
            Arc::new(create_olap_schema()),
        )
        .unwrap();

        writer.write_chunk(memtable.chunk()).unwrap();
        writer.finish().unwrap();

        let sstable = ParquetSSTable::open(&file_path).unwrap();
        let chunks = sstable.range_query(2000, 3000).unwrap();

        let total_rows: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total_rows, 0);
    }

    #[test]
    fn test_scan_all() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = tmp_dir.path().join("test.parquet");

        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        let mut writer = ParquetSSTableWriter::create(
            &file_path,
            Arc::new(create_olap_schema()),
        )
        .unwrap();

        writer.write_chunk(memtable.chunk()).unwrap();
        writer.finish().unwrap();

        let sstable = ParquetSSTable::open(&file_path).unwrap();
        let chunks = sstable.scan().unwrap();

        let total_rows: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total_rows, 100);
    }
}
