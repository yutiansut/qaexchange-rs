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
    use std::sync::Arc;

    // ==================== InstrumentStatus 测试 @yutiansut @quantaxis ====================

    /// 测试 InstrumentStatus 枚举变体
    #[test]
    fn test_instrument_status_variants() {
        let active = InstrumentStatus::Active;
        let suspended = InstrumentStatus::Suspended;
        let delisted = InstrumentStatus::Delisted;

        assert_ne!(active, suspended);
        assert_ne!(suspended, delisted);
        assert_ne!(active, delisted);
    }

    /// 测试 InstrumentStatus Copy trait
    #[test]
    fn test_instrument_status_copy() {
        let status = InstrumentStatus::Active;
        let status_copy = status;
        assert_eq!(status, status_copy);
    }

    /// 测试 InstrumentStatus Debug trait
    #[test]
    fn test_instrument_status_debug() {
        let status = InstrumentStatus::Active;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Active"));
    }

    // ==================== InstrumentType 测试 @yutiansut @quantaxis ====================

    /// 测试 InstrumentType 枚举变体
    #[test]
    fn test_instrument_type_variants() {
        let index = InstrumentType::IndexFuture;
        let commodity = InstrumentType::CommodityFuture;
        let stock = InstrumentType::Stock;
        let option = InstrumentType::Option;

        assert_ne!(index, commodity);
        assert_ne!(stock, option);
    }

    /// 测试 InstrumentType Copy trait
    #[test]
    fn test_instrument_type_copy() {
        let inst_type = InstrumentType::IndexFuture;
        let inst_type_copy = inst_type;
        assert_eq!(inst_type, inst_type_copy);
    }

    // ==================== InstrumentInfo 测试 @yutiansut @quantaxis ====================

    /// 测试 InstrumentInfo::new() 默认值
    #[test]
    fn test_instrument_info_new_defaults() {
        let info = InstrumentInfo::new(
            "IF2501".to_string(),
            "沪深300股指期货".to_string(),
            InstrumentType::IndexFuture,
            "CFFEX".to_string(),
        );

        assert_eq!(info.instrument_id, "IF2501");
        assert_eq!(info.instrument_name, "沪深300股指期货");
        assert_eq!(info.instrument_type, InstrumentType::IndexFuture);
        assert_eq!(info.exchange, "CFFEX");
        assert_eq!(info.contract_multiplier, 300);
        assert_eq!(info.price_tick, 0.2);
        assert_eq!(info.margin_rate, 0.12);
        assert_eq!(info.commission_rate, 0.0001);
        assert_eq!(info.limit_up_rate, 0.1);
        assert_eq!(info.limit_down_rate, 0.1);
        assert_eq!(info.status, InstrumentStatus::Active);
        assert!(info.list_date.is_none());
        assert!(info.expire_date.is_none());
    }

    /// 测试 InstrumentInfo Clone trait
    #[test]
    fn test_instrument_info_clone() {
        let info = InstrumentInfo::new(
            "cu2501".to_string(),
            "铜期货".to_string(),
            InstrumentType::CommodityFuture,
            "SHFE".to_string(),
        );

        let cloned = info.clone();
        assert_eq!(info.instrument_id, cloned.instrument_id);
        assert_eq!(info.exchange, cloned.exchange);
    }

    /// 测试不同合约类型
    #[test]
    fn test_instrument_info_different_types() {
        let types = [
            InstrumentType::IndexFuture,
            InstrumentType::CommodityFuture,
            InstrumentType::Stock,
            InstrumentType::Option,
        ];

        for inst_type in types {
            let info = InstrumentInfo::new(
                format!("TEST_{:?}", inst_type),
                "Test".to_string(),
                inst_type,
                "TEST".to_string(),
            );
            assert_eq!(info.instrument_type, inst_type);
        }
    }

    // ==================== InstrumentRegistry 基础测试 @yutiansut @quantaxis ====================

    /// 测试 InstrumentRegistry::new()
    #[test]
    fn test_instrument_registry_new() {
        let registry = InstrumentRegistry::new();
        assert!(registry.instruments.is_empty());
    }

    /// 测试 Default trait
    #[test]
    fn test_instrument_registry_default() {
        let registry = InstrumentRegistry::default();
        assert!(registry.instruments.is_empty());
    }

    /// 综合测试（原有测试）
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

    // ==================== register 测试 @yutiansut @quantaxis ====================

    /// 测试 register 成功
    #[test]
    fn test_register_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "au2512".to_string(),
            "黄金期货".to_string(),
            InstrumentType::CommodityFuture,
            "SHFE".to_string(),
        );

        let result = registry.register(info);
        assert!(result.is_ok());
        assert!(registry.get("au2512").is_some());
    }

    /// 测试 register 重复
    #[test]
    fn test_register_duplicate() {
        let registry = InstrumentRegistry::new();

        let info1 = InstrumentInfo::new(
            "dup_test".to_string(),
            "Test 1".to_string(),
            InstrumentType::IndexFuture,
            "TEST".to_string(),
        );

        let info2 = InstrumentInfo::new(
            "dup_test".to_string(),
            "Test 2".to_string(),
            InstrumentType::Stock,
            "TEST".to_string(),
        );

        assert!(registry.register(info1).is_ok());
        let result = registry.register(info2);
        assert!(result.is_err());

        match result {
            Err(ExchangeError::InstrumentError(msg)) => {
                assert!(msg.contains("already exists"));
            }
            _ => panic!("Expected InstrumentError"),
        }
    }

    /// 测试注册多个合约
    #[test]
    fn test_register_multiple() {
        let registry = InstrumentRegistry::new();

        for i in 0..5 {
            let info = InstrumentInfo::new(
                format!("INST_{}", i),
                format!("Instrument {}", i),
                InstrumentType::CommodityFuture,
                "TEST".to_string(),
            );
            registry.register(info).unwrap();
        }

        assert_eq!(registry.list_all().len(), 5);
    }

    // ==================== get 测试 @yutiansut @quantaxis ====================

    /// 测试 get 成功
    #[test]
    fn test_get_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "get_test".to_string(),
            "Get Test".to_string(),
            InstrumentType::Stock,
            "SSE".to_string(),
        );

        registry.register(info).unwrap();

        let result = registry.get("get_test");
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.instrument_id, "get_test");
        assert_eq!(retrieved.exchange, "SSE");
    }

    /// 测试 get 不存在
    #[test]
    fn test_get_not_found() {
        let registry = InstrumentRegistry::new();

        let result = registry.get("non_existent");
        assert!(result.is_none());
    }

    // ==================== list_all 测试 @yutiansut @quantaxis ====================

    /// 测试 list_all 空列表
    #[test]
    fn test_list_all_empty() {
        let registry = InstrumentRegistry::new();

        let all = registry.list_all();
        assert!(all.is_empty());
    }

    /// 测试 list_all 有数据
    #[test]
    fn test_list_all_with_data() {
        let registry = InstrumentRegistry::new();

        for i in 0..3 {
            let info = InstrumentInfo::new(
                format!("LIST_{}", i),
                format!("List {}", i),
                InstrumentType::IndexFuture,
                "TEST".to_string(),
            );
            registry.register(info).unwrap();
        }

        let all = registry.list_all();
        assert_eq!(all.len(), 3);
    }

    // ==================== list_by_status 测试 @yutiansut @quantaxis ====================

    /// 测试 list_by_status
    #[test]
    fn test_list_by_status() {
        let registry = InstrumentRegistry::new();

        // 注册多个合约
        for i in 0..6 {
            let info = InstrumentInfo::new(
                format!("STATUS_{}", i),
                format!("Status {}", i),
                InstrumentType::CommodityFuture,
                "TEST".to_string(),
            );
            registry.register(info).unwrap();
        }

        // 暂停一些
        registry.suspend("STATUS_1").unwrap();
        registry.suspend("STATUS_3").unwrap();

        // 下市一些
        registry.delist("STATUS_5").unwrap();

        // 检查各状态数量
        let active = registry.list_by_status(InstrumentStatus::Active);
        assert_eq!(active.len(), 3); // 0, 2, 4

        let suspended = registry.list_by_status(InstrumentStatus::Suspended);
        assert_eq!(suspended.len(), 2); // 1, 3

        let delisted = registry.list_by_status(InstrumentStatus::Delisted);
        assert_eq!(delisted.len(), 1); // 5
    }

    /// 测试 list_by_status 空结果
    #[test]
    fn test_list_by_status_empty() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "active_only".to_string(),
            "Active Only".to_string(),
            InstrumentType::Stock,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        let suspended = registry.list_by_status(InstrumentStatus::Suspended);
        assert!(suspended.is_empty());

        let delisted = registry.list_by_status(InstrumentStatus::Delisted);
        assert!(delisted.is_empty());
    }

    // ==================== update 测试 @yutiansut @quantaxis ====================

    /// 测试 update 成功
    #[test]
    fn test_update_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "update_test".to_string(),
            "Update Test".to_string(),
            InstrumentType::CommodityFuture,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        // 更新保证金率
        let result = registry.update("update_test", |info| {
            info.margin_rate = 0.15;
            info.commission_rate = 0.0002;
        });

        assert!(result.is_ok());

        let updated = registry.get("update_test").unwrap();
        assert_eq!(updated.margin_rate, 0.15);
        assert_eq!(updated.commission_rate, 0.0002);
    }

    /// 测试 update 不存在的合约
    #[test]
    fn test_update_not_found() {
        let registry = InstrumentRegistry::new();

        let result = registry.update("non_existent", |info| {
            info.margin_rate = 0.2;
        });

        assert!(result.is_err());
        match result {
            Err(ExchangeError::InstrumentError(msg)) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected InstrumentError"),
        }
    }

    // ==================== suspend 测试 @yutiansut @quantaxis ====================

    /// 测试 suspend 成功
    #[test]
    fn test_suspend_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "suspend_test".to_string(),
            "Suspend Test".to_string(),
            InstrumentType::IndexFuture,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        assert!(registry.is_trading("suspend_test"));

        let result = registry.suspend("suspend_test");
        assert!(result.is_ok());

        assert!(!registry.is_trading("suspend_test"));

        let info = registry.get("suspend_test").unwrap();
        assert_eq!(info.status, InstrumentStatus::Suspended);
    }

    /// 测试 suspend 不存在的合约
    #[test]
    fn test_suspend_not_found() {
        let registry = InstrumentRegistry::new();

        let result = registry.suspend("non_existent");
        assert!(result.is_err());
    }

    // ==================== resume 测试 @yutiansut @quantaxis ====================

    /// 测试 resume 成功
    #[test]
    fn test_resume_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "resume_test".to_string(),
            "Resume Test".to_string(),
            InstrumentType::Option,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        // 先暂停
        registry.suspend("resume_test").unwrap();
        assert!(!registry.is_trading("resume_test"));

        // 恢复
        let result = registry.resume("resume_test");
        assert!(result.is_ok());
        assert!(registry.is_trading("resume_test"));

        let info = registry.get("resume_test").unwrap();
        assert_eq!(info.status, InstrumentStatus::Active);
    }

    /// 测试 resume 不存在的合约
    #[test]
    fn test_resume_not_found() {
        let registry = InstrumentRegistry::new();

        let result = registry.resume("non_existent");
        assert!(result.is_err());
    }

    // ==================== delist 测试 @yutiansut @quantaxis ====================

    /// 测试 delist 成功
    #[test]
    fn test_delist_success() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "delist_test".to_string(),
            "Delist Test".to_string(),
            InstrumentType::Stock,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        let result = registry.delist("delist_test");
        assert!(result.is_ok());

        assert!(!registry.is_trading("delist_test"));

        let info = registry.get("delist_test").unwrap();
        assert_eq!(info.status, InstrumentStatus::Delisted);
    }

    /// 测试 delist 不存在的合约
    #[test]
    fn test_delist_not_found() {
        let registry = InstrumentRegistry::new();

        let result = registry.delist("non_existent");
        assert!(result.is_err());
    }

    // ==================== is_trading 测试 @yutiansut @quantaxis ====================

    /// 测试 is_trading 不同状态
    #[test]
    fn test_is_trading_different_status() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "trading_test".to_string(),
            "Trading Test".to_string(),
            InstrumentType::IndexFuture,
            "TEST".to_string(),
        );
        registry.register(info).unwrap();

        // Active 状态可交易
        assert!(registry.is_trading("trading_test"));

        // Suspended 状态不可交易
        registry.suspend("trading_test").unwrap();
        assert!(!registry.is_trading("trading_test"));

        // 恢复后可交易
        registry.resume("trading_test").unwrap();
        assert!(registry.is_trading("trading_test"));

        // Delisted 状态不可交易
        registry.delist("trading_test").unwrap();
        assert!(!registry.is_trading("trading_test"));
    }

    /// 测试 is_trading 合约不存在
    #[test]
    fn test_is_trading_not_found() {
        let registry = InstrumentRegistry::new();

        assert!(!registry.is_trading("non_existent"));
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发注册
    #[test]
    fn test_concurrent_register() {
        use std::thread;

        let registry = Arc::new(InstrumentRegistry::new());
        let mut handles = vec![];

        for i in 0..10 {
            let registry_clone = registry.clone();
            handles.push(thread::spawn(move || {
                let info = InstrumentInfo::new(
                    format!("CONC_{}", i),
                    format!("Concurrent {}", i),
                    InstrumentType::CommodityFuture,
                    "TEST".to_string(),
                );
                registry_clone.register(info)
            }));
        }

        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }

        assert_eq!(registry.list_all().len(), 10);
    }

    /// 测试并发读取
    #[test]
    fn test_concurrent_read() {
        use std::thread;

        let registry = Arc::new(InstrumentRegistry::new());

        // 先注册一些合约
        for i in 0..5 {
            let info = InstrumentInfo::new(
                format!("READ_{}", i),
                format!("Read {}", i),
                InstrumentType::Stock,
                "TEST".to_string(),
            );
            registry.register(info).unwrap();
        }

        // 并发读取
        let mut handles = vec![];
        for i in 0..20 {
            let registry_clone = registry.clone();
            handles.push(thread::spawn(move || {
                let _all = registry_clone.list_all();
                let _info = registry_clone.get(&format!("READ_{}", i % 5));
                let _active = registry_clone.list_by_status(InstrumentStatus::Active);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// 测试并发状态切换
    #[test]
    fn test_concurrent_status_change() {
        use std::thread;

        let registry = Arc::new(InstrumentRegistry::new());

        // 注册合约
        for i in 0..5 {
            let info = InstrumentInfo::new(
                format!("STAT_{}", i),
                format!("Status {}", i),
                InstrumentType::IndexFuture,
                "TEST".to_string(),
            );
            registry.register(info).unwrap();
        }

        // 并发状态切换
        let mut handles = vec![];
        for i in 0..10 {
            let registry_clone = registry.clone();
            handles.push(thread::spawn(move || {
                let id = format!("STAT_{}", i % 5);
                if i % 2 == 0 {
                    let _ = registry_clone.suspend(&id);
                } else {
                    let _ = registry_clone.resume(&id);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // ==================== 边界条件测试 @yutiansut @quantaxis ====================

    /// 测试空字符串合约ID
    #[test]
    fn test_empty_instrument_id() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "".to_string(),
            "Empty ID".to_string(),
            InstrumentType::Stock,
            "TEST".to_string(),
        );

        // 空字符串技术上可以注册
        assert!(registry.register(info).is_ok());
        assert!(registry.get("").is_some());
    }

    /// 测试特殊字符合约ID
    #[test]
    fn test_special_chars_instrument_id() {
        let registry = InstrumentRegistry::new();

        let info = InstrumentInfo::new(
            "SHFE.cu2501".to_string(),
            "带交易所前缀的合约".to_string(),
            InstrumentType::CommodityFuture,
            "SHFE".to_string(),
        );

        assert!(registry.register(info).is_ok());
        assert!(registry.get("SHFE.cu2501").is_some());
    }

    /// 测试合约完整生命周期
    #[test]
    fn test_instrument_full_lifecycle() {
        let registry = InstrumentRegistry::new();

        // 1. 上市
        let info = InstrumentInfo::new(
            "LIFECYCLE".to_string(),
            "Lifecycle Test".to_string(),
            InstrumentType::IndexFuture,
            "CFFEX".to_string(),
        );
        registry.register(info).unwrap();
        assert!(registry.is_trading("LIFECYCLE"));

        // 2. 更新参数
        registry
            .update("LIFECYCLE", |info| {
                info.margin_rate = 0.15;
            })
            .unwrap();

        // 3. 暂停
        registry.suspend("LIFECYCLE").unwrap();
        assert!(!registry.is_trading("LIFECYCLE"));

        // 4. 恢复
        registry.resume("LIFECYCLE").unwrap();
        assert!(registry.is_trading("LIFECYCLE"));

        // 5. 下市
        registry.delist("LIFECYCLE").unwrap();
        assert!(!registry.is_trading("LIFECYCLE"));

        // 验证最终状态
        let final_info = registry.get("LIFECYCLE").unwrap();
        assert_eq!(final_info.status, InstrumentStatus::Delisted);
        assert_eq!(final_info.margin_rate, 0.15);
    }
}
