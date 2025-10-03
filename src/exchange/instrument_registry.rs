//! 合约注册表

use serde::{Deserialize, Serialize};
use dashmap::DashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentInfo {
    pub instrument_id: String,
    pub name: String,
    pub exchange_id: String,
    pub product_type: String,
    pub is_trading: bool,
}

pub struct InstrumentRegistry {
    instruments: DashMap<String, InstrumentInfo>,
}

impl InstrumentRegistry {
    pub fn new() -> Self {
        Self {
            instruments: DashMap::new(),
        }
    }

    pub fn register(&self, info: InstrumentInfo) {
        self.instruments.insert(info.instrument_id.clone(), info);
    }

    pub fn get(&self, instrument_id: &str) -> Option<InstrumentInfo> {
        self.instruments.get(instrument_id).map(|r| r.value().clone())
    }

    pub fn list_all(&self) -> Vec<InstrumentInfo> {
        self.instruments.iter().map(|r| r.value().clone()).collect()
    }
}

impl Default for InstrumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
