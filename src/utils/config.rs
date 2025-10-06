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
