//! 增量状态存储
//!
//! @yutiansut @quantaxis
//!
//! 提供因子状态的持久化存储功能：
//! - 状态序列化/反序列化
//! - 快照保存与恢复
//! - 检查点管理
//! - 状态压缩

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════════════════════
// 可序列化的因子状态
// ═══════════════════════════════════════════════════════════════════════════

/// 可序列化的滚动统计状态
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct SerializableRollingState {
    pub window_size: usize,
    pub values: Vec<f64>,
    pub sum: f64,
    pub count: u64,
}

/// 可序列化的 Welford 状态
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct SerializableWelfordState {
    pub count: u64,
    pub mean: f64,
    pub m2: f64,
    pub m3: f64,
    pub m4: f64,
}

/// 可序列化的 EMA 状态
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct SerializableEMAState {
    pub alpha: f64,
    pub value: Option<f64>,
    pub count: u64,
}

/// 可序列化的 RSI 状态
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct SerializableRSIState {
    pub period: usize,
    pub prev_price: Option<f64>,
    pub avg_gain: f64,
    pub avg_loss: f64,
    pub count: u64,
}

/// 可序列化的因子值
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub enum SerializableFactorValue {
    Scalar(f64),
    Optional(Option<f64>),
    Vector(Vec<f64>),
}

/// 单合约的完整因子状态快照
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct InstrumentStateSnapshot {
    pub instrument_id: String,
    pub rolling_states: HashMap<String, SerializableRollingState>,
    pub welford_states: HashMap<String, SerializableWelfordState>,
    pub ema_states: HashMap<String, SerializableEMAState>,
    pub rsi_states: HashMap<String, SerializableRSIState>,
    pub custom_values: HashMap<String, SerializableFactorValue>,
    pub update_count: u64,
    pub timestamp_ms: u64,
}

/// 全局状态快照
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(check_bytes)]
pub struct GlobalStateSnapshot {
    pub version: u32,
    pub instruments: Vec<InstrumentStateSnapshot>,
    pub checkpoint_id: u64,
    pub created_at_ms: u64,
}

impl GlobalStateSnapshot {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(checkpoint_id: u64) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            instruments: Vec::new(),
            checkpoint_id,
            created_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 状态存储管理器
// ═══════════════════════════════════════════════════════════════════════════

/// 状态存储错误
#[derive(Debug)]
pub enum StateStoreError {
    IoError(std::io::Error),
    SerializationError(String),
    DeserializationError(String),
    VersionMismatch { expected: u32, found: u32 },
    CheckpointNotFound(u64),
    CorruptedData(String),
}

impl From<std::io::Error> for StateStoreError {
    fn from(e: std::io::Error) -> Self {
        StateStoreError::IoError(e)
    }
}

impl std::fmt::Display for StateStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateStoreError::IoError(e) => write!(f, "IO error: {}", e),
            StateStoreError::SerializationError(s) => write!(f, "Serialization error: {}", s),
            StateStoreError::DeserializationError(s) => write!(f, "Deserialization error: {}", s),
            StateStoreError::VersionMismatch { expected, found } => {
                write!(f, "Version mismatch: expected {}, found {}", expected, found)
            }
            StateStoreError::CheckpointNotFound(id) => {
                write!(f, "Checkpoint not found: {}", id)
            }
            StateStoreError::CorruptedData(s) => write!(f, "Corrupted data: {}", s),
        }
    }
}

impl std::error::Error for StateStoreError {}

pub type StateStoreResult<T> = Result<T, StateStoreError>;

/// 状态存储配置
#[derive(Debug, Clone)]
pub struct StateStoreConfig {
    /// 存储目录
    pub base_path: PathBuf,
    /// 最大检查点保留数
    pub max_checkpoints: usize,
    /// 自动检查点间隔
    pub checkpoint_interval: Duration,
    /// 是否压缩
    pub compress: bool,
}

impl Default for StateStoreConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("./data/factor_state"),
            max_checkpoints: 10,
            checkpoint_interval: Duration::from_secs(300), // 5 分钟
            compress: true,
        }
    }
}

/// 状态存储管理器
pub struct StateStore {
    config: StateStoreConfig,
    checkpoint_counter: std::sync::atomic::AtomicU64,
}

impl StateStore {
    pub fn new(config: StateStoreConfig) -> StateStoreResult<Self> {
        // 确保目录存在
        fs::create_dir_all(&config.base_path)?;

        // 获取最新检查点 ID
        let latest_id = Self::find_latest_checkpoint_id(&config.base_path)?;

        Ok(Self {
            config,
            checkpoint_counter: std::sync::atomic::AtomicU64::new(latest_id + 1),
        })
    }

    /// 查找最新检查点 ID
    fn find_latest_checkpoint_id(base_path: &Path) -> StateStoreResult<u64> {
        let mut max_id = 0u64;

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("checkpoint_") && name.ends_with(".rkyv") {
                        if let Ok(id) = name
                            .trim_start_matches("checkpoint_")
                            .trim_end_matches(".rkyv")
                            .parse::<u64>()
                        {
                            max_id = max_id.max(id);
                        }
                    }
                }
            }
        }

        Ok(max_id)
    }

    /// 生成新的检查点 ID
    fn next_checkpoint_id(&self) -> u64 {
        self.checkpoint_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// 检查点文件路径
    fn checkpoint_path(&self, checkpoint_id: u64) -> PathBuf {
        self.config
            .base_path
            .join(format!("checkpoint_{:016}.rkyv", checkpoint_id))
    }

    /// 保存检查点
    pub fn save_checkpoint(&self, snapshot: &GlobalStateSnapshot) -> StateStoreResult<u64> {
        let checkpoint_id = self.next_checkpoint_id();
        let path = self.checkpoint_path(checkpoint_id);

        // 序列化 (rkyv 0.7 API)
        let bytes = rkyv::to_bytes::<_, 256>(snapshot)
            .map_err(|e| StateStoreError::SerializationError(e.to_string()))?;

        // 写入文件
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        if self.config.compress {
            // TODO: 添加 zstd 压缩
            writer.write_all(&bytes)?;
        } else {
            writer.write_all(&bytes)?;
        }

        writer.flush()?;

        // 清理旧检查点
        self.cleanup_old_checkpoints()?;

        log::info!("Saved checkpoint {} to {:?}", checkpoint_id, path);

        Ok(checkpoint_id)
    }

    /// 加载检查点
    pub fn load_checkpoint(&self, checkpoint_id: u64) -> StateStoreResult<GlobalStateSnapshot> {
        let path = self.checkpoint_path(checkpoint_id);

        if !path.exists() {
            return Err(StateStoreError::CheckpointNotFound(checkpoint_id));
        }

        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();

        if self.config.compress {
            // TODO: 添加 zstd 解压
            reader.read_to_end(&mut bytes)?;
        } else {
            reader.read_to_end(&mut bytes)?;
        }

        // 反序列化 (rkyv 0.7 API)
        let archived = unsafe { rkyv::archived_root::<GlobalStateSnapshot>(&bytes) };

        let snapshot: GlobalStateSnapshot = archived
            .deserialize(&mut rkyv::Infallible)
            .map_err(|_| StateStoreError::DeserializationError("Deserialization failed".to_string()))?;

        // 版本检查
        if snapshot.version != GlobalStateSnapshot::CURRENT_VERSION {
            return Err(StateStoreError::VersionMismatch {
                expected: GlobalStateSnapshot::CURRENT_VERSION,
                found: snapshot.version,
            });
        }

        log::info!("Loaded checkpoint {} from {:?}", checkpoint_id, path);

        Ok(snapshot)
    }

    /// 加载最新检查点
    pub fn load_latest_checkpoint(&self) -> StateStoreResult<Option<GlobalStateSnapshot>> {
        let latest_id = Self::find_latest_checkpoint_id(&self.config.base_path)?;

        if latest_id == 0 {
            return Ok(None);
        }

        self.load_checkpoint(latest_id).map(Some)
    }

    /// 清理旧检查点
    fn cleanup_old_checkpoints(&self) -> StateStoreResult<()> {
        let mut checkpoints: Vec<(u64, PathBuf)> = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.config.base_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("checkpoint_") && name.ends_with(".rkyv") {
                        if let Ok(id) = name
                            .trim_start_matches("checkpoint_")
                            .trim_end_matches(".rkyv")
                            .parse::<u64>()
                        {
                            checkpoints.push((id, entry.path()));
                        }
                    }
                }
            }
        }

        // 按 ID 排序
        checkpoints.sort_by_key(|(id, _)| *id);

        // 删除超出保留数量的旧检查点
        while checkpoints.len() > self.config.max_checkpoints {
            if let Some((id, path)) = checkpoints.first() {
                log::info!("Removing old checkpoint {}: {:?}", id, path);
                fs::remove_file(path)?;
                checkpoints.remove(0);
            }
        }

        Ok(())
    }

    /// 列出所有检查点
    pub fn list_checkpoints(&self) -> StateStoreResult<Vec<u64>> {
        let mut checkpoints = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.config.base_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("checkpoint_") && name.ends_with(".rkyv") {
                        if let Ok(id) = name
                            .trim_start_matches("checkpoint_")
                            .trim_end_matches(".rkyv")
                            .parse::<u64>()
                        {
                            checkpoints.push(id);
                        }
                    }
                }
            }
        }

        checkpoints.sort();
        Ok(checkpoints)
    }

    /// 删除指定检查点
    pub fn delete_checkpoint(&self, checkpoint_id: u64) -> StateStoreResult<()> {
        let path = self.checkpoint_path(checkpoint_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 状态缓存
// ═══════════════════════════════════════════════════════════════════════════

/// 内存状态缓存
pub struct StateCache {
    /// 合约状态缓存
    instruments: DashMap<String, InstrumentStateSnapshot>,
    /// 脏标记
    dirty: DashMap<String, bool>,
    /// 最后保存时间
    last_save: std::sync::atomic::AtomicU64,
}

impl StateCache {
    pub fn new() -> Self {
        Self {
            instruments: DashMap::new(),
            dirty: DashMap::new(),
            last_save: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 更新合约状态
    pub fn update(&self, snapshot: InstrumentStateSnapshot) {
        let id = snapshot.instrument_id.clone();
        self.instruments.insert(id.clone(), snapshot);
        self.dirty.insert(id, true);
    }

    /// 获取合约状态
    pub fn get(&self, instrument_id: &str) -> Option<InstrumentStateSnapshot> {
        self.instruments.get(instrument_id).map(|r| r.clone())
    }

    /// 标记为已保存
    pub fn mark_saved(&self, instrument_id: &str) {
        self.dirty.insert(instrument_id.to_string(), false);
    }

    /// 获取脏数据
    pub fn get_dirty(&self) -> Vec<InstrumentStateSnapshot> {
        self.instruments
            .iter()
            .filter(|r| {
                self.dirty
                    .get(r.key())
                    .map(|d| *d)
                    .unwrap_or(false)
            })
            .map(|r| r.value().clone())
            .collect()
    }

    /// 创建全局快照
    pub fn create_snapshot(&self, checkpoint_id: u64) -> GlobalStateSnapshot {
        let instruments: Vec<InstrumentStateSnapshot> =
            self.instruments.iter().map(|r| r.value().clone()).collect();

        GlobalStateSnapshot {
            version: GlobalStateSnapshot::CURRENT_VERSION,
            instruments,
            checkpoint_id,
            created_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    /// 从快照恢复
    pub fn restore_from_snapshot(&self, snapshot: GlobalStateSnapshot) {
        self.instruments.clear();
        self.dirty.clear();

        for inst in snapshot.instruments {
            let id = inst.instrument_id.clone();
            self.instruments.insert(id.clone(), inst);
            self.dirty.insert(id, false);
        }
    }

    /// 清空缓存
    pub fn clear(&self) {
        self.instruments.clear();
        self.dirty.clear();
    }

    /// 缓存大小
    pub fn len(&self) -> usize {
        self.instruments.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.instruments.is_empty()
    }
}

impl Default for StateCache {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_state_store_save_load() {
        let temp_dir = tempdir().unwrap();
        let config = StateStoreConfig {
            base_path: temp_dir.path().to_path_buf(),
            max_checkpoints: 5,
            checkpoint_interval: Duration::from_secs(60),
            compress: false,
        };

        let store = StateStore::new(config).unwrap();

        // 创建测试快照
        let mut snapshot = GlobalStateSnapshot::new(1);
        snapshot.instruments.push(InstrumentStateSnapshot {
            instrument_id: "cu2501".to_string(),
            rolling_states: HashMap::new(),
            welford_states: HashMap::new(),
            ema_states: HashMap::new(),
            rsi_states: HashMap::new(),
            custom_values: HashMap::new(),
            update_count: 100,
            timestamp_ms: 1234567890,
        });

        // 保存
        let checkpoint_id = store.save_checkpoint(&snapshot).unwrap();

        // 加载
        let loaded = store.load_checkpoint(checkpoint_id).unwrap();

        assert_eq!(loaded.instruments.len(), 1);
        assert_eq!(loaded.instruments[0].instrument_id, "cu2501");
        assert_eq!(loaded.instruments[0].update_count, 100);
    }

    #[test]
    fn test_state_cache() {
        let cache = StateCache::new();

        // 添加状态
        let snapshot = InstrumentStateSnapshot {
            instrument_id: "au2501".to_string(),
            rolling_states: HashMap::new(),
            welford_states: HashMap::new(),
            ema_states: HashMap::new(),
            rsi_states: HashMap::new(),
            custom_values: HashMap::new(),
            update_count: 50,
            timestamp_ms: 1234567890,
        };

        cache.update(snapshot);

        // 验证脏数据
        let dirty = cache.get_dirty();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0].instrument_id, "au2501");

        // 标记已保存
        cache.mark_saved("au2501");
        let dirty = cache.get_dirty();
        assert_eq!(dirty.len(), 0);
    }

    #[test]
    fn test_cleanup_old_checkpoints() {
        let temp_dir = tempdir().unwrap();
        let config = StateStoreConfig {
            base_path: temp_dir.path().to_path_buf(),
            max_checkpoints: 3,
            checkpoint_interval: Duration::from_secs(60),
            compress: false,
        };

        let store = StateStore::new(config).unwrap();

        // 创建多个检查点
        for i in 0..5 {
            let snapshot = GlobalStateSnapshot::new(i);
            store.save_checkpoint(&snapshot).unwrap();
        }

        // 验证只保留最新的 3 个
        let checkpoints = store.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 3);
    }
}
