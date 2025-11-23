// SSTable 扫描器 - 统一 OLTP 和 OLAP 数据读取

use crate::storage::sstable::{ParquetSSTable, RkyvSSTable};
use arrow2::array::Array;
use arrow2::chunk::Chunk;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// SSTable 类型
#[derive(Debug, Clone)]
pub enum SSTableType {
    /// OLTP SSTable (rkyv)
    Oltp,
    /// OLAP SSTable (Parquet)
    Olap,
}

/// SSTable 扫描器
///
/// 统一接口读取不同类型的 SSTable
pub struct SSTableScanner {
    /// SSTable 文件路径列表
    sstables: Vec<SSTableEntry>,
}

/// SSTable 条目
struct SSTableEntry {
    path: PathBuf,
    table_type: SSTableType,
}

impl SSTableScanner {
    /// 创建新的扫描器
    pub fn new() -> Self {
        Self {
            sstables: Vec::new(),
        }
    }

    /// 添加 OLAP SSTable
    pub fn add_olap_sstable<P: AsRef<Path>>(&mut self, path: P) {
        self.sstables.push(SSTableEntry {
            path: path.as_ref().to_path_buf(),
            table_type: SSTableType::Olap,
        });
    }

    /// 添加 OLTP SSTable
    pub fn add_oltp_sstable<P: AsRef<Path>>(&mut self, path: P) {
        self.sstables.push(SSTableEntry {
            path: path.as_ref().to_path_buf(),
            table_type: SSTableType::Oltp,
        });
    }

    /// 自动扫描目录并添加所有 SSTable
    pub fn scan_directory<P: AsRef<Path>>(&mut self, dir: P) -> Result<(), String> {
        let dir_path = dir.as_ref();

        if !dir_path.exists() {
            return Err(format!("Directory not found: {:?}", dir_path));
        }

        let entries =
            std::fs::read_dir(dir_path).map_err(|e| format!("Read directory failed: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Read entry failed: {}", e))?;
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    match ext.to_str() {
                        Some("parquet") => {
                            self.add_olap_sstable(path);
                        }
                        Some("sst") => {
                            self.add_oltp_sstable(path);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// 获取 SSTable 数量
    pub fn len(&self) -> usize {
        self.sstables.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.sstables.is_empty()
    }

    /// 范围查询（扫描所有 SSTable）
    ///
    /// 返回合并后的 Chunk 列表
    pub fn range_query(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        let mut all_chunks = Vec::new();

        for entry in &self.sstables {
            match entry.table_type {
                SSTableType::Olap => {
                    let sstable = ParquetSSTable::open(&entry.path)?;
                    let chunks = sstable.range_query(start_ts, end_ts)?;
                    all_chunks.extend(chunks);
                }
                SSTableType::Oltp => {
                    // OLTP SSTable 需要转换为 Chunk
                    // 目前跳过，或实现转换逻辑
                    log::warn!("OLTP SSTable query not yet implemented: {:?}", entry.path);
                }
            }
        }

        Ok(all_chunks)
    }

    /// 全量扫描（扫描所有 SSTable）
    pub fn scan_all(&self) -> Result<Vec<Chunk<Box<dyn Array>>>, String> {
        let mut all_chunks = Vec::new();

        for entry in &self.sstables {
            match entry.table_type {
                SSTableType::Olap => {
                    let sstable = ParquetSSTable::open(&entry.path)?;
                    let chunks = sstable.scan()?;
                    all_chunks.extend(chunks);
                }
                SSTableType::Oltp => {
                    log::warn!("OLTP SSTable scan not yet implemented: {:?}", entry.path);
                }
            }
        }

        Ok(all_chunks)
    }

    /// 获取 Parquet 文件路径列表（用于直接传递给 Polars）
    pub fn get_parquet_paths(&self) -> Vec<PathBuf> {
        self.sstables
            .iter()
            .filter(|e| matches!(e.table_type, SSTableType::Olap))
            .map(|e| e.path.clone())
            .collect()
    }
}

impl Default for SSTableScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memtable::olap::{create_olap_schema, OlapMemTable};
    use crate::storage::memtable::types::MemTableKey;
    use crate::storage::sstable::olap_parquet::ParquetSSTableWriter;
    use crate::storage::wal::WalRecord;

    fn create_test_records(count: usize, start_ts: i64) -> Vec<(MemTableKey, WalRecord)> {
        (0..count)
            .map(|i| {
                let key = MemTableKey {
                    timestamp: start_ts + i as i64,
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
    fn test_scanner_scan_directory() {
        let tmp_dir = tempfile::tempdir().unwrap();

        // 创建 2 个 Parquet 文件
        for i in 0..2 {
            let file_path = tmp_dir.path().join(format!("test_{}.parquet", i));
            let records = create_test_records(100, 1000 + i * 100);
            let memtable = OlapMemTable::from_records(records);

            let mut writer =
                ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

            writer.write_chunk(memtable.chunk()).unwrap();
            writer.finish().unwrap();
        }

        // 扫描目录
        let mut scanner = SSTableScanner::new();
        scanner.scan_directory(tmp_dir.path()).unwrap();

        assert_eq!(scanner.len(), 2);
    }

    #[test]
    fn test_scanner_range_query() {
        let tmp_dir = tempfile::tempdir().unwrap();

        // 创建测试数据
        let file_path = tmp_dir.path().join("test.parquet");
        let records = create_test_records(100, 1000);
        let memtable = OlapMemTable::from_records(records);

        let mut writer =
            ParquetSSTableWriter::create(&file_path, Arc::new(create_olap_schema())).unwrap();

        writer.write_chunk(memtable.chunk()).unwrap();
        writer.finish().unwrap();

        // 使用扫描器查询
        let mut scanner = SSTableScanner::new();
        scanner.add_olap_sstable(&file_path);

        let chunks = scanner.range_query(1010, 1020).unwrap();
        assert!(!chunks.is_empty());

        let total_rows: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total_rows, 11); // 1010-1020 inclusive
    }
}
