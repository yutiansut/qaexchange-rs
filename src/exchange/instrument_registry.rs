//! 合约注册表 - 完整的合约生命周期管理
//!
//! 支持合约的上市、下市、暂停交易、参数修改等全流程管理

use chrono::{NaiveDate, Utc};
use dashmap::DashMap;
use log;
use serde::{Deserialize, Serialize};

use crate::ExchangeError;

/// 合约状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstrumentStatus {
    /// 正常交易
    Active,
    /// 暂停交易
    Suspended,
    /// 已下市
    Delisted,
}

/// 合约类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentType {
    /// 股指期货
    IndexFuture,
    /// 商品期货
    CommodityFuture,
    /// 股票
    Stock,
    /// 期权
    Option,
}

/// 合约完整信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentInfo {
    /// 合约代码
    pub instrument_id: String,

    /// 合约名称
    pub instrument_name: String,

    /// 合约类型
    pub instrument_type: InstrumentType,

    /// 交易所代码
    pub exchange: String,

    /// 合约乘数
    pub contract_multiplier: i32,

    /// 最小变动价位
    pub price_tick: f64,

    /// 保证金率
    pub margin_rate: f64,

    /// 手续费率
    pub commission_rate: f64,

    /// 涨停板比例
    pub limit_up_rate: f64,

    /// 跌停板比例
    pub limit_down_rate: f64,

    /// 合约状态
    pub status: InstrumentStatus,

    /// 上市日期
    pub list_date: Option<String>,

    /// 到期日期
    pub expire_date: Option<String>,

    /// 创建时间
    pub created_at: String,

    /// 更新时间
    pub updated_at: String,
}

impl InstrumentInfo {
    /// 创建新合约
    pub fn new(
        instrument_id: String,
        instrument_name: String,
        instrument_type: InstrumentType,
        exchange: String,
    ) -> Self {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        Self {
            instrument_id,
            instrument_name,
            instrument_type,
            exchange,
            contract_multiplier: 300,
            price_tick: 0.2,
            margin_rate: 0.12,
            commission_rate: 0.0001,
            limit_up_rate: 0.1,
            limit_down_rate: 0.1,
            status: InstrumentStatus::Active,
            list_date: None,
            expire_date: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// 合约注册表
pub struct InstrumentRegistry {
    instruments: DashMap<String, InstrumentInfo>,
}

impl InstrumentRegistry {
    pub fn new() -> Self {
        Self {
            instruments: DashMap::new(),
        }
    }

    /// 注册/上市新合约
    pub fn register(&self, info: InstrumentInfo) -> Result<(), ExchangeError> {
        if self.instruments.contains_key(&info.instrument_id) {
            return Err(ExchangeError::InstrumentError(format!(
                "Instrument {} already exists",
                info.instrument_id
            )));
        }

        log::info!("Registering instrument: {}", info.instrument_id);
        self.instruments.insert(info.instrument_id.clone(), info);
        Ok(())
    }

    /// 获取合约信息
    pub fn get(&self, instrument_id: &str) -> Option<InstrumentInfo> {
        self.instruments
            .get(instrument_id)
            .map(|r| r.value().clone())
    }

    /// 列出所有合约
    pub fn list_all(&self) -> Vec<InstrumentInfo> {
        self.instruments.iter().map(|r| r.value().clone()).collect()
    }

    /// 列出指定状态的合约
    pub fn list_by_status(&self, status: InstrumentStatus) -> Vec<InstrumentInfo> {
        self.instruments
            .iter()
            .filter(|r| r.value().status == status)
            .map(|r| r.value().clone())
            .collect()
    }

    /// 更新合约信息
    pub fn update(
        &self,
        instrument_id: &str,
        update_fn: impl FnOnce(&mut InstrumentInfo),
    ) -> Result<(), ExchangeError> {
        match self.instruments.get_mut(instrument_id) {
            Some(mut info) => {
                update_fn(info.value_mut());
                info.value_mut().updated_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                log::info!("Updated instrument: {}", instrument_id);
                Ok(())
            }
            None => Err(ExchangeError::InstrumentError(format!(
                "Instrument {} not found",
                instrument_id
            ))),
        }
    }

    /// 暂停交易
    pub fn suspend(&self, instrument_id: &str) -> Result<(), ExchangeError> {
        self.update(instrument_id, |info| {
            info.status = InstrumentStatus::Suspended;
        })?;
        log::info!("Suspended trading for instrument: {}", instrument_id);
        Ok(())
    }

    /// 恢复交易
    pub fn resume(&self, instrument_id: &str) -> Result<(), ExchangeError> {
        self.update(instrument_id, |info| {
            info.status = InstrumentStatus::Active;
        })?;
        log::info!("Resumed trading for instrument: {}", instrument_id);
        Ok(())
    }

    /// 下市合约
    pub fn delist(&self, instrument_id: &str) -> Result<(), ExchangeError> {
        self.update(instrument_id, |info| {
            info.status = InstrumentStatus::Delisted;
        })?;
        log::info!("Delisted instrument: {}", instrument_id);
        Ok(())
    }

    /// 检查合约是否可交易
    pub fn is_trading(&self, instrument_id: &str) -> bool {
        self.instruments
            .get(instrument_id)
            .map(|r| r.value().status == InstrumentStatus::Active)
            .unwrap_or(false)
    }
}

impl Default for InstrumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_registry() {
        let registry = InstrumentRegistry::new();

        // 创建合约
        let instrument = InstrumentInfo::new(
            "IF2501".to_string(),
            "沪深300股指期货2501".to_string(),
            InstrumentType::IndexFuture,
            "CFFEX".to_string(),
        );

        // 注册
        assert!(registry.register(instrument.clone()).is_ok());

        // 重复注册应该失败
        assert!(registry.register(instrument).is_err());

        // 获取
        assert!(registry.get("IF2501").is_some());
        assert!(registry.get("NONEXIST").is_none());

        // 暂停
        assert!(registry.suspend("IF2501").is_ok());
        assert!(!registry.is_trading("IF2501"));

        // 恢复
        assert!(registry.resume("IF2501").is_ok());
        assert!(registry.is_trading("IF2501"));

        // 下市
        assert!(registry.delist("IF2501").is_ok());
        assert!(!registry.is_trading("IF2501"));
    }
}
