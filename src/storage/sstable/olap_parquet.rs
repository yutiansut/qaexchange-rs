// OLAP SSTable - Parquet 列式存储
//
// 设计理念:
// - 列式压缩存储，高压缩率
// - 支持谓词下推（predicate pushdown）
// - 适合大规模扫描和聚合查询
// - 不可变文件
//
// 性能优化 (Phase 11):
// - Row Group 级别时间戳统计提取
// - 谓词下推跳过不相关 Row Groups
// - 零拷贝读取路径
//
// @yutiansut @quantaxis

use arrow2::array::{
    Array, BooleanArray, MutableArray, MutableBooleanArray, MutableFixedSizeBinaryArray,
};
use arrow2::chunk::Chunk;
use arrow2::datatypes::Schema;
use arrow2::io::parquet::read::{
    infer_schema, read_metadata, FileReader, RowGroupMetaData,
};
use arrow2::io::parquet::write::{
    CompressionOptions, Encoding, FileWriter, RowGroupIterator,
    Version, WriteOptions,
};
// parquet2 types for statistics extraction
use parquet2::schema::types::PhysicalType as ParquetPhysicalType;

use super::compression::{CompressionAlgorithm, CompressionStrategy};
use crate::storage::hybrid::query_filter::RecordCategory;
use parquet2::statistics::PrimitiveStatistics;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::types::SSTableMetadata;

/// Row Group 时间戳范围（用于谓词下推）
#[derive(Debug, Clone)]
pub struct RowGroupTimeRange {
    pub index: usize,
    pub min_timestamp: i64,
    pub max_timestamp: i64,
    pub row_count: usize,
}

/// Parquet SSTable Writer
///
/// 将 Arrow2 Chunk 批量写入 Parquet 文件
///
/// 支持按数据类型自动选择最优压缩算法
pub struct ParquetSSTableWriter {
    file_path: PathBuf,
    schema: Arc<Schema>,
    writer: Option<FileWriter<File>>,
    entry_count: u64,
    min_timestamp: Option<i64>,
    max_timestamp: Option<i64>,
    /// 压缩选项（根据数据类型动态选择）
    compression: CompressionOptions,
    /// 压缩策略（用于选择压缩算法）
    compression_strategy: CompressionStrategy,
}

impl ParquetSSTableWriter {
    /// 创建新的 Parquet Writer（使用默认平衡压缩策略）
    ///
    /// # Arguments
    /// * `file_path` - 输出文件路径
    /// * `schema` - Arrow2 Schema
    pub fn create<P: AsRef<Path>>(file_path: P, schema: Arc<Schema>) -> Result<Self, String> {
        Self::create_with_compression(file_path, schema, CompressionStrategy::balanced(), None)
    }

    /// 创建新的 Parquet Writer（指定压缩策略和数据类别）
    ///
    /// # Arguments
    /// * `file_path` - 输出文件路径
    /// * `schema` - Arrow2 Schema
    /// * `strategy` - 压缩策略配置
    /// * `category` - 数据类别（用于自动选择压缩算法）
    pub fn create_with_compression<P: AsRef<Path>>(
        file_path: P,
        schema: Arc<Schema>,
        strategy: CompressionStrategy,
        category: Option<RecordCategory>,
    ) -> Result<Self, String> {
        let file_path = file_path.as_ref().to_path_buf();

        // 确保目录存在
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Create dir failed: {}", e))?;
        }

        // 创建文件
        let file =
            File::create(&file_path).map_err(|e| format!("Create parquet file failed: {}", e))?;

        // 根据数据类别选择压缩算法
        let compression_alg = match category {
            Some(cat) => strategy.get_for_category(cat),
            None => strategy.default, // 使用默认压缩
        };
        let compression = compression_alg.to_parquet_options();

        log::debug!(
            "Creating ParquetSSTable {:?} with compression: {} (category: {:?})",
            file_path,
            compression_alg.name(),
            category
        );

        // Parquet 写入选项（根据数据类型动态选择压缩）
        let options = WriteOptions {
            write_statistics: true, // 写入统计信息（min/max 等）
            compression,            // 动态压缩
            version: Version::V2,   // Parquet 2.0
            data_pagesize_limit: None,
        };

        // 创建 Parquet Writer
        // Arrow2 FileWriter 接受 schema 作为 &Schema，不是 Arc<Schema>
        let _encodings: Vec<Vec<Encoding>> = schema
            .fields
            .iter()
            .map(|_| vec![Encoding::Plain])
            .collect();

        let writer = FileWriter::try_new(file, (*schema).clone(), options)
            .map_err(|e| format!("Create parquet writer failed: {}", e))?;

        Ok(Self {
            file_path,
            schema,
            writer: Some(writer),
            entry_count: 0,
            min_timestamp: None,
            max_timestamp: None,
            compression,
            compression_strategy: strategy,
        })
    }

    /// 创建低延迟 Parquet Writer（使用 LZ4 压缩）
    pub fn create_low_latency<P: AsRef<Path>>(file_path: P, schema: Arc<Schema>) -> Result<Self, String> {
        Self::create_with_compression(file_path, schema, CompressionStrategy::low_latency(), None)
    }

    /// 创建高压缩 Parquet Writer（使用 ZSTD 压缩）
    pub fn create_high_compression<P: AsRef<Path>>(file_path: P, schema: Arc<Schema>) -> Result<Self, String> {
        Self::create_with_compression(file_path, schema, CompressionStrategy::high_compression(), None)
    }

    /// 创建归档 Parquet Writer（最高压缩比）
    pub fn create_archive<P: AsRef<Path>>(file_path: P, schema: Arc<Schema>) -> Result<Self, String> {
        Self::create_with_compression(file_path, schema, CompressionStrategy::archive(), None)
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
            if let Some(&last_ts) = timestamp_array.iter().next_back().and_then(|v| v) {
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
                compression: self.compression, // 使用动态压缩配置
                version: Version::V2,
                data_pagesize_limit: None,
            },
            vec![vec![Encoding::Plain]; self.schema.fields.len()],
        )
        .map_err(|e| format!("Create row group iterator failed: {}", e))?;

        for group in row_groups {
            writer
                .write(group.map_err(|e| format!("Write row group failed: {}", e))?)
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
///
/// 性能特性:
/// - Row Group 级别谓词下推
/// - 时间戳统计信息缓存
/// - 选择性读取（跳过不相关 Row Groups）
pub struct ParquetSSTable {
    file_path: PathBuf,
    metadata: SSTableMetadata,
    schema: Arc<Schema>,
    /// Row Group 时间戳范围（用于谓词下推）
    row_group_ranges: Vec<RowGroupTimeRange>,
}

impl ParquetSSTable {
    /// 打开 Parquet SSTable
    ///
    /// 性能优化：提取所有 Row Group 的时间戳统计信息用于谓词下推
    pub fn open<P: AsRef<Path>>(file_path: P) -> Result<Self, String> {
        let file_path = file_path.as_ref().to_path_buf();

        // 读取 Parquet 元数据
        let mut file =
            File::open(&file_path).map_err(|e| format!("Open parquet file failed: {}", e))?;

        let parquet_metadata =
            read_metadata(&mut file).map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        let schema =
            infer_schema(&parquet_metadata).map_err(|e| format!("Infer schema failed: {}", e))?;

        // 提取 Row Group 时间戳统计信息
        let mut entry_count = 0u64;
        let mut global_min_ts = i64::MAX;
        let mut global_max_ts = i64::MIN;
        let mut row_group_ranges = Vec::with_capacity(parquet_metadata.row_groups.len());

        for (index, row_group) in parquet_metadata.row_groups.iter().enumerate() {
            let row_count = row_group.num_rows();
            entry_count += row_count as u64;

            // 提取时间戳列（第一列）的统计信息
            let (min_ts, max_ts) = Self::extract_timestamp_statistics(row_group);

            if min_ts < global_min_ts {
                global_min_ts = min_ts;
            }
            if max_ts > global_max_ts {
                global_max_ts = max_ts;
            }

            row_group_ranges.push(RowGroupTimeRange {
                index,
                min_timestamp: min_ts,
                max_timestamp: max_ts,
                row_count,
            });
        }

        // 处理空文件情况
        if global_min_ts == i64::MAX {
            global_min_ts = 0;
        }
        if global_max_ts == i64::MIN {
            global_max_ts = 0;
        }

        let file_size = std::fs::metadata(&file_path)
            .map_err(|e| format!("Get file size failed: {}", e))?
            .len();

        let metadata = SSTableMetadata {
            version: 2,
            entry_count,
            min_timestamp: global_min_ts,
            max_timestamp: global_max_ts,
            min_key: Vec::new(),
            max_key: Vec::new(),
            file_size,
            block_offsets: Vec::new(),
            bloom_filter: None,
            created_at: chrono::Utc::now().timestamp(),
        };

        log::debug!(
            "Opened ParquetSSTable {:?}: {} entries, {} row groups, time range [{}, {}]",
            file_path,
            entry_count,
            row_group_ranges.len(),
            global_min_ts,
            global_max_ts
        );

        Ok(Self {
            file_path,
            metadata,
            schema: Arc::new(schema),
            row_group_ranges,
        })
    }

    /// 从 Row Group 元数据中提取时间戳列的 min/max 统计信息
    ///
    /// 性能特性：
    /// - 零分配提取（直接从元数据读取）
    /// - 支持 Int64 物理类型（纳秒时间戳）
    /// - 优雅降级（无统计时返回宽松范围）
    #[inline]
    fn extract_timestamp_statistics(row_group: &RowGroupMetaData) -> (i64, i64) {
        // 时间戳是第一列 (index 0)
        if row_group.columns().is_empty() {
            return (i64::MIN, i64::MAX);
        }

        let column_meta = &row_group.columns()[0];

        // 尝试从 column chunk 统计信息中提取
        // statistics() 返回 Option<Result<Arc<dyn Statistics>>>
        if let Some(Ok(stats)) = column_meta.statistics() {
            // 检查物理类型是否为 Int64
            if *stats.physical_type() == ParquetPhysicalType::Int64 {
                // 安全地 downcast 到 PrimitiveStatistics<i64>
                if let Some(int_stats) = stats.as_any().downcast_ref::<PrimitiveStatistics<i64>>() {
                    let min = int_stats.min_value.unwrap_or(i64::MIN);
                    let max = int_stats.max_value.unwrap_or(i64::MAX);
                    return (min, max);
                }
            }
        }

        // 如果没有统计信息或类型不匹配，返回宽松范围（不做过滤）
        (i64::MIN, i64::MAX)
    }

    /// 获取 Row Group 时间戳范围（用于外部优化）
    pub fn row_group_ranges(&self) -> &[RowGroupTimeRange] {
        &self.row_group_ranges
    }

    /// 获取元数据
    pub fn metadata(&self) -> &SSTableMetadata {
        &self.metadata
    }

    /// 获取 Schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// 范围查询（带谓词下推）
    ///
    /// 返回时间戳范围内的所有 Chunk
    ///
    /// 性能优化：
    /// 1. 文件级别时间范围快速过滤
    /// 2. Row Group 级别谓词下推（跳过不相关 Row Groups）
    /// 3. 行级别精确过滤
    ///
    /// # Arguments
    /// * `start_ts` - 起始时间戳（纳秒）
    /// * `end_ts` - 结束时间戳（纳秒）
    pub fn range_query(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        // 快速路径 1：文件级别时间范围不重叠
        if self.metadata.max_timestamp != 0
            && (end_ts < self.metadata.min_timestamp || start_ts > self.metadata.max_timestamp)
        {
            log::trace!(
                "Skipping file {:?}: time range [{}, {}] does not overlap [{}, {}]",
                self.file_path,
                self.metadata.min_timestamp,
                self.metadata.max_timestamp,
                start_ts,
                end_ts
            );
            return Ok(Vec::new());
        }

        // 快速路径 2：确定需要读取的 Row Groups（谓词下推）
        let relevant_row_groups: Vec<usize> = self
            .row_group_ranges
            .iter()
            .filter(|rg| {
                // Row Group 时间范围与查询范围有重叠
                rg.max_timestamp >= start_ts && rg.min_timestamp <= end_ts
            })
            .map(|rg| rg.index)
            .collect();

        if relevant_row_groups.is_empty() {
            log::trace!(
                "Skipping file {:?}: no relevant row groups for time range [{}, {}]",
                self.file_path,
                start_ts,
                end_ts
            );
            return Ok(Vec::new());
        }

        let skipped_count = self.row_group_ranges.len() - relevant_row_groups.len();
        if skipped_count > 0 {
            log::debug!(
                "Predicate pushdown: reading {}/{} row groups, skipped {} for {:?}",
                relevant_row_groups.len(),
                self.row_group_ranges.len(),
                skipped_count,
                self.file_path
            );
        }

        // 打开文件并读取元数据
        let mut file = File::open(&self.file_path)
            .map_err(|e| format!("Open parquet file for query failed: {}", e))?;

        let parquet_metadata =
            read_metadata(&mut file).map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        // 只选择相关的 Row Groups
        let selected_row_groups: Vec<RowGroupMetaData> = relevant_row_groups
            .iter()
            .filter_map(|&idx| parquet_metadata.row_groups.get(idx).cloned())
            .collect();

        let reader = FileReader::new(
            file,
            selected_row_groups,
            (*self.schema).clone(),
            None,
            None,
            None,
        );

        let mut chunks = Vec::new();

        // 读取选中的 Row Groups
        for chunk_result in reader {
            let chunk = chunk_result.map_err(|e| format!("Read chunk failed: {}", e))?;

            // 行级别精确过滤
            let filtered_chunk = filter_chunk_by_timestamp(&chunk, start_ts, end_ts)?;

            if !filtered_chunk.is_empty() {
                chunks.push(filtered_chunk);
            }
        }

        Ok(chunks)
    }

    /// 高性能范围查询（返回行数估算，用于查询优化）
    ///
    /// 不读取实际数据，仅基于统计信息估算匹配行数
    pub fn estimate_row_count(&self, start_ts: i64, end_ts: i64) -> usize {
        if end_ts < self.metadata.min_timestamp || start_ts > self.metadata.max_timestamp {
            return 0;
        }

        self.row_group_ranges
            .iter()
            .filter(|rg| rg.max_timestamp >= start_ts && rg.min_timestamp <= end_ts)
            .map(|rg| {
                // 估算重叠比例
                let rg_span = (rg.max_timestamp - rg.min_timestamp).max(1) as f64;
                let overlap_start = start_ts.max(rg.min_timestamp);
                let overlap_end = end_ts.min(rg.max_timestamp);
                let overlap_span = (overlap_end - overlap_start).max(0) as f64;
                let ratio = (overlap_span / rg_span).min(1.0);
                (rg.row_count as f64 * ratio) as usize
            })
            .sum()
    }

    /// 扫描整个文件（用于全量查询）
    pub fn scan(&self) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        let mut file = File::open(&self.file_path)
            .map_err(|e| format!("Open parquet file for scan failed: {}", e))?;

        let parquet_metadata =
            read_metadata(&mut file).map_err(|e| format!("Read parquet metadata failed: {}", e))?;

        let reader = FileReader::new(
            file,
            parquet_metadata.row_groups,
            (*self.schema).clone(),
            None,
            None,
            None,
        );

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
        let is_in_range = ts_opt
            .map(|&ts| ts >= start_ts && ts <= end_ts)
            .unwrap_or(false);
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
    use arrow2::array::{FixedSizeBinaryArray, PrimitiveArray};
    let filtered_arrays: Vec<Box<dyn Array>> = chunk
        .arrays()
        .iter()
        .map(|array| {
            // 根据类型处理不同的数组
            if let Some(prim_i64) = array.as_any().downcast_ref::<PrimitiveArray<i64>>() {
                let filtered: PrimitiveArray<i64> =
                    true_indices.iter().map(|&idx| prim_i64.get(idx)).collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_i32) = array.as_any().downcast_ref::<PrimitiveArray<i32>>() {
                let filtered: PrimitiveArray<i32> =
                    true_indices.iter().map(|&idx| prim_i32.get(idx)).collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_u64) = array.as_any().downcast_ref::<PrimitiveArray<u64>>() {
                let filtered: PrimitiveArray<u64> =
                    true_indices.iter().map(|&idx| prim_u64.get(idx)).collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_u8) = array.as_any().downcast_ref::<PrimitiveArray<u8>>() {
                let filtered: PrimitiveArray<u8> =
                    true_indices.iter().map(|&idx| prim_u8.get(idx)).collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(prim_f64) = array.as_any().downcast_ref::<PrimitiveArray<f64>>() {
                let filtered: PrimitiveArray<f64> =
                    true_indices.iter().map(|&idx| prim_f64.get(idx)).collect();
                Box::new(filtered) as Box<dyn Array>
            } else if let Some(fixed_bin) = array.as_any().downcast_ref::<FixedSizeBinaryArray>() {
                let size = fixed_bin.size();
                let mut builder =
                    MutableFixedSizeBinaryArray::with_capacity(size, true_indices.len());
                for &idx in &true_indices {
                    builder.push(fixed_bin.get(idx));
                }
                let filtered: FixedSizeBinaryArray = builder.into();
                Box::new(filtered) as Box<dyn Array>
            } else {
                // 未知类型，panic以便调试
                panic!(
                    "Unsupported array type in filter_chunk_by_timestamp: {:?}",
                    array.data_type()
                )
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
            let mut writer =
                ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

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

        let mut writer =
            ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

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

        let mut writer =
            ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

        writer.write_chunk(memtable.chunk()).unwrap();
        writer.finish().unwrap();

        let sstable = ParquetSSTable::open(&file_path).unwrap();
        let chunks = sstable.scan().unwrap();

        let total_rows: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total_rows, 100);
    }
}
