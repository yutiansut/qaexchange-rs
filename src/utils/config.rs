//! 配置管理模块

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub server: ServerConfig,
    pub http: HttpConfig,
    pub websocket: WebSocketConfig,
    pub storage: StorageConfig,
    pub instruments: Vec<InstrumentConfig>,
}

/// 性能优化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default)]
    pub batch_write: BatchWriteConfig,
    #[serde(default)]
    pub websocket: WebSocketPerfConfig,
    #[serde(default)]
    pub priority_queue: PriorityQueueConfig,
    #[serde(default)]
    pub memtable: MemTableConfig,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            batch_write: BatchWriteConfig::default(),
            websocket: WebSocketPerfConfig::default(),
            priority_queue: PriorityQueueConfig::default(),
            memtable: MemTableConfig::default(),
        }
    }
}

impl PerformanceConfig {
    /// 从文件加载性能配置
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read performance config file: {}", e))?;
        toml::from_str(&content)
            .map_err(|e| format!("Failed to parse performance config file: {}", e))
    }

    /// 加载默认性能配置文件
    pub fn load_default() -> Result<Self, String> {
        Self::load_from_file("config/performance.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWriteConfig {
    #[serde(default = "default_buffer_size")]
    pub tick_buffer_size: usize,
    #[serde(default = "default_flush_interval")]
    pub tick_flush_interval_ms: u64,
    #[serde(default = "default_batch_size")]
    pub max_batch_size: usize,
    #[serde(default = "default_snapshot_interval")]
    pub snapshot_interval_ms: u64,
}

impl Default for BatchWriteConfig {
    fn default() -> Self {
        Self {
            tick_buffer_size: 1000,
            tick_flush_interval_ms: 10,
            max_batch_size: 1000,
            snapshot_interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketPerfConfig {
    #[serde(default = "default_ws_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_flush_interval")]
    pub batch_timeout_ms: u64,
    #[serde(default = "default_queue_threshold")]
    pub queue_threshold: usize,
}

impl Default for WebSocketPerfConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            batch_timeout_ms: 10,
            queue_threshold: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_low_queue_limit")]
    pub low_queue_limit: usize,
    #[serde(default = "default_critical_threshold")]
    pub critical_amount_threshold: f64,
    #[serde(default)]
    pub vip_users: Vec<String>,
}

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            low_queue_limit: 100,
            critical_amount_threshold: 1_000_000.0,
            vip_users: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemTableConfig {
    #[serde(default = "default_memtable_size")]
    pub max_size_mb: usize,
    #[serde(default = "default_entry_size")]
    pub estimated_entry_size: usize,
}

impl Default for MemTableConfig {
    fn default() -> Self {
        Self {
            max_size_mb: 64,
            estimated_entry_size: 256,
        }
    }
}

// 默认值函数
fn default_buffer_size() -> usize { 1000 }
fn default_flush_interval() -> u64 { 10 }
fn default_batch_size() -> usize { 1000 }
fn default_snapshot_interval() -> u64 { 1000 }
fn default_ws_batch_size() -> usize { 100 }
fn default_queue_threshold() -> usize { 500 }
fn default_true() -> bool { true }
fn default_low_queue_limit() -> usize { 100 }
fn default_critical_threshold() -> f64 { 1_000_000.0 }
fn default_memtable_size() -> usize { 64 }
fn default_entry_size() -> usize { 256 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub environment: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

impl HttpConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    pub host: String,
    pub port: u16,
}

impl WebSocketConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub enabled: bool,
    pub base_path: String,
    pub subscriber: SubscriberConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriberConfig {
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentConfig {
    pub instrument_id: String,
    pub name: String,
    pub exchange_id: String,
    pub product_type: String,
    pub init_price: f64,
    pub is_trading: bool,
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
    #[serde(default = "default_tick_size")]
    pub tick_size: f64,
}

fn default_multiplier() -> f64 {
    300.0
}

fn default_tick_size() -> f64 {
    0.2
}

impl ExchangeConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        toml::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))
    }

    pub fn load_default() -> Result<Self, String> {
        Self::load_from_file("config/exchange.toml")
    }
}
