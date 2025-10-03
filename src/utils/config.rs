//! 配置管理

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub server_address: String,
    pub server_port: u16,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            server_address: "127.0.0.1".to_string(),
            server_port: 8080,
        }
    }
}
