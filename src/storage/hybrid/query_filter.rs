//! 高性能查询过滤器
//!
//! @yutiansut @quantaxis
//!
//! 设计原则：
//! - 零拷贝过滤：使用引用和迭代器避免克隆
//! - 编译期优化：使用泛型和 const 泛型
//! - SIMD 友好：数据布局支持向量化
//! - 无锁设计：适合并发查询场景

use crate::storage::wal::record::WalRecord;
use std::collections::HashSet;
use std::ops::RangeInclusive;

// ═══════════════════════════════════════════════════════════════════════════
// 记录类型枚举（编译期已知，支持位运算过滤）
// ═══════════════════════════════════════════════════════════════════════════

/// 记录类型位标志（用于高效过滤）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum RecordType {
    // 账户类型 (0x00xx)
    AccountOpen = 0x0001,
    AccountUpdate = 0x0002,

    // 用户类型 (0x01xx)
    UserRegister = 0x0100,
    AccountBind = 0x0101,

    // 订单类型 (0x02xx)
    OrderInsert = 0x0200,
    TradeExecuted = 0x0201,

    // 行情类型 (0x03xx)
    TickData = 0x0300,
    OrderBookSnapshot = 0x0301,
    OrderBookDelta = 0x0302,
    KLineFinished = 0x0303,

    // 交易所逐笔 (0x04xx)
    ExchangeOrderRecord = 0x0400,
    ExchangeTradeRecord = 0x0401,
    ExchangeResponseRecord = 0x0402,

    // 因子类型 (0x05xx)
    FactorUpdate = 0x0500,
    FactorSnapshot = 0x0501,

    // 系统类型 (0xFFxx)
    Checkpoint = 0xFF00,
}

impl RecordType {
    /// 从 WalRecord 提取类型
    #[inline(always)]
    pub fn from_wal_record(record: &WalRecord) -> Self {
        match record {
            WalRecord::AccountOpen { .. } => Self::AccountOpen,
            WalRecord::AccountUpdate { .. } => Self::AccountUpdate,
            WalRecord::UserRegister { .. } => Self::UserRegister,
            WalRecord::AccountBind { .. } => Self::AccountBind,
            WalRecord::OrderInsert { .. } => Self::OrderInsert,
            WalRecord::TradeExecuted { .. } => Self::TradeExecuted,
            WalRecord::TickData { .. } => Self::TickData,
            WalRecord::OrderBookSnapshot { .. } => Self::OrderBookSnapshot,
            WalRecord::OrderBookDelta { .. } => Self::OrderBookDelta,
            WalRecord::KLineFinished { .. } => Self::KLineFinished,
            WalRecord::ExchangeOrderRecord { .. } => Self::ExchangeOrderRecord,
            WalRecord::ExchangeTradeRecord { .. } => Self::ExchangeTradeRecord,
            WalRecord::ExchangeResponseRecord { .. } => Self::ExchangeResponseRecord,
            WalRecord::FactorUpdate { .. } => Self::FactorUpdate,
            WalRecord::FactorSnapshot { .. } => Self::FactorSnapshot,
            WalRecord::Checkpoint { .. } => Self::Checkpoint,
        }
    }

    /// 类型名称（用于日志和调试）
    #[inline]
    pub fn name(&self) -> &'static str {
        match self {
            Self::AccountOpen => "AccountOpen",
            Self::AccountUpdate => "AccountUpdate",
            Self::UserRegister => "UserRegister",
            Self::AccountBind => "AccountBind",
            Self::OrderInsert => "OrderInsert",
            Self::TradeExecuted => "TradeExecuted",
            Self::TickData => "TickData",
            Self::OrderBookSnapshot => "OrderBookSnapshot",
            Self::OrderBookDelta => "OrderBookDelta",
            Self::KLineFinished => "KLineFinished",
            Self::ExchangeOrderRecord => "ExchangeOrderRecord",
            Self::ExchangeTradeRecord => "ExchangeTradeRecord",
            Self::ExchangeResponseRecord => "ExchangeResponseRecord",
            Self::FactorUpdate => "FactorUpdate",
            Self::FactorSnapshot => "FactorSnapshot",
            Self::Checkpoint => "Checkpoint",
        }
    }

    /// 获取类型类别（用于压缩策略）
    #[inline]
    pub fn category(&self) -> RecordCategory {
        match *self as u16 >> 8 {
            0x00 => RecordCategory::Account,
            0x01 => RecordCategory::User,
            0x02 => RecordCategory::Order,
            0x03 => RecordCategory::MarketData,
            0x04 => RecordCategory::Exchange,
            0x05 => RecordCategory::Factor,
            _ => RecordCategory::System,
        }
    }

    /// 从 i32 值转换为 RecordType（用于 OLAP 过滤）
    ///
    /// OLAP 存储将 record_type 作为 i32 列存储
    /// 返回 None 如果值不匹配任何已知类型
    #[inline]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value as u16 {
            0x0001 => Some(Self::AccountOpen),
            0x0002 => Some(Self::AccountUpdate),
            0x0100 => Some(Self::UserRegister),
            0x0101 => Some(Self::AccountBind),
            0x0200 => Some(Self::OrderInsert),
            0x0201 => Some(Self::TradeExecuted),
            0x0300 => Some(Self::TickData),
            0x0301 => Some(Self::OrderBookSnapshot),
            0x0302 => Some(Self::OrderBookDelta),
            0x0303 => Some(Self::KLineFinished),
            0x0400 => Some(Self::ExchangeOrderRecord),
            0x0401 => Some(Self::ExchangeTradeRecord),
            0x0402 => Some(Self::ExchangeResponseRecord),
            0x0500 => Some(Self::FactorUpdate),
            0x0501 => Some(Self::FactorSnapshot),
            0xFF00 => Some(Self::Checkpoint),
            _ => None,
        }
    }

    /// 转换为 i32（用于 OLAP 存储）
    #[inline]
    pub fn to_i32(self) -> i32 {
        self as i32
    }
}

/// 记录类别（用于压缩策略选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordCategory {
    Account,
    User,
    Order,
    MarketData,
    Exchange,
    Factor,
    System,
}

// ═══════════════════════════════════════════════════════════════════════════
// 类型集合（位掩码实现，O(1) 查询）
// ═══════════════════════════════════════════════════════════════════════════

/// 记录类型集合 - 使用位掩码实现 O(1) 包含检查
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordTypeSet {
    /// 位掩码：每个位对应一种类型
    mask: u32,
}

impl RecordTypeSet {
    /// 空集合
    pub const EMPTY: Self = Self { mask: 0 };

    /// 所有类型
    pub const ALL: Self = Self { mask: u32::MAX };

    /// 账户相关类型
    pub const ACCOUNT: Self = Self {
        mask: (1 << 0) | (1 << 1),
    };

    /// 用户相关类型
    pub const USER: Self = Self {
        mask: (1 << 2) | (1 << 3),
    };

    /// 订单相关类型
    pub const ORDER: Self = Self {
        mask: (1 << 4) | (1 << 5),
    };

    /// 行情相关类型
    pub const MARKET_DATA: Self = Self {
        mask: (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9),
    };

    /// 交易所逐笔类型
    pub const EXCHANGE: Self = Self {
        mask: (1 << 10) | (1 << 11) | (1 << 12),
    };

    /// 因子相关类型
    pub const FACTOR: Self = Self {
        mask: (1 << 13) | (1 << 14),
    };

    /// 创建新集合
    #[inline]
    pub fn new() -> Self {
        Self::EMPTY
    }

    /// 添加类型
    #[inline]
    pub fn insert(mut self, record_type: RecordType) -> Self {
        self.mask |= Self::type_to_bit(record_type);
        self
    }

    /// 移除类型
    #[inline]
    pub fn remove(mut self, record_type: RecordType) -> Self {
        self.mask &= !Self::type_to_bit(record_type);
        self
    }

    /// 检查是否包含类型（O(1)）
    #[inline(always)]
    pub fn contains(&self, record_type: RecordType) -> bool {
        (self.mask & Self::type_to_bit(record_type)) != 0
    }

    /// 检查是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mask == 0
    }

    /// 检查是否包含所有类型
    #[inline]
    pub fn is_all(&self) -> bool {
        // 检查前 15 位（实际使用的类型数）
        (self.mask & 0x7FFF) == 0x7FFF
    }

    /// 合并两个集合
    #[inline]
    pub fn union(self, other: Self) -> Self {
        Self {
            mask: self.mask | other.mask,
        }
    }

    /// 交集
    #[inline]
    pub fn intersection(self, other: Self) -> Self {
        Self {
            mask: self.mask & other.mask,
        }
    }

    /// 类型到位索引的映射
    #[inline(always)]
    fn type_to_bit(record_type: RecordType) -> u32 {
        match record_type {
            RecordType::AccountOpen => 1 << 0,
            RecordType::AccountUpdate => 1 << 1,
            RecordType::UserRegister => 1 << 2,
            RecordType::AccountBind => 1 << 3,
            RecordType::OrderInsert => 1 << 4,
            RecordType::TradeExecuted => 1 << 5,
            RecordType::TickData => 1 << 6,
            RecordType::OrderBookSnapshot => 1 << 7,
            RecordType::OrderBookDelta => 1 << 8,
            RecordType::KLineFinished => 1 << 9,
            RecordType::ExchangeOrderRecord => 1 << 10,
            RecordType::ExchangeTradeRecord => 1 << 11,
            RecordType::ExchangeResponseRecord => 1 << 12,
            RecordType::FactorUpdate => 1 << 13,
            RecordType::FactorSnapshot => 1 << 14,
            RecordType::Checkpoint => 1 << 15,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 查询过滤器（组合多种过滤条件）
// ═══════════════════════════════════════════════════════════════════════════

/// 高性能查询过滤器
///
/// 使用 Builder 模式构建，支持链式调用
#[derive(Debug, Clone)]
pub struct QueryFilter {
    /// 时间范围过滤
    pub time_range: Option<RangeInclusive<i64>>,
    /// 记录类型过滤（位掩码）
    pub record_types: RecordTypeSet,
    /// 合约过滤（HashSet 用于 O(1) 查找）
    pub instruments: Option<HashSet<String>>,
    /// 价格范围过滤
    pub price_range: Option<RangeInclusive<f64>>,
    /// 最大返回数量
    pub limit: Option<usize>,
    /// 跳过数量（分页）
    pub offset: usize,
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            time_range: None,
            record_types: RecordTypeSet::ALL,
            instruments: None,
            price_range: None,
            limit: None,
            offset: 0,
        }
    }
}

impl QueryFilter {
    /// 创建新的过滤器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置时间范围
    #[inline]
    pub fn with_time_range(mut self, start: i64, end: i64) -> Self {
        self.time_range = Some(start..=end);
        self
    }

    /// 设置记录类型过滤
    #[inline]
    pub fn with_record_types(mut self, types: RecordTypeSet) -> Self {
        self.record_types = types;
        self
    }

    /// 添加单个记录类型
    #[inline]
    pub fn with_record_type(mut self, record_type: RecordType) -> Self {
        if self.record_types.is_all() {
            self.record_types = RecordTypeSet::EMPTY;
        }
        self.record_types = self.record_types.insert(record_type);
        self
    }

    /// 设置合约过滤
    #[inline]
    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.instruments = Some(instruments.into_iter().collect());
        self
    }

    /// 添加单个合约
    #[inline]
    pub fn with_instrument(mut self, instrument: impl Into<String>) -> Self {
        self.instruments
            .get_or_insert_with(HashSet::new)
            .insert(instrument.into());
        self
    }

    /// 设置价格范围
    #[inline]
    pub fn with_price_range(mut self, min: f64, max: f64) -> Self {
        self.price_range = Some(min..=max);
        self
    }

    /// 设置最大返回数量
    #[inline]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 设置分页偏移
    #[inline]
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// 检查 WalRecord 是否匹配过滤器（零拷贝）
    #[inline]
    pub fn matches(&self, record: &WalRecord, timestamp: i64) -> bool {
        // 1. 时间范围检查（最快退出）
        if let Some(ref range) = self.time_range {
            if !range.contains(&timestamp) {
                return false;
            }
        }

        // 2. 类型检查（O(1) 位运算）
        let record_type = RecordType::from_wal_record(record);
        if !self.record_types.contains(record_type) {
            return false;
        }

        // 3. 合约检查
        if let Some(ref instruments) = self.instruments {
            let instrument_id = Self::extract_instrument_id(record);
            if let Some(id) = instrument_id {
                if !instruments.contains(id) {
                    return false;
                }
            }
        }

        // 4. 价格范围检查
        if let Some(ref price_range) = self.price_range {
            if let Some(price) = Self::extract_price(record) {
                if !price_range.contains(&price) {
                    return false;
                }
            }
        }

        true
    }

    /// 提取合约 ID（零拷贝返回引用）
    #[inline]
    fn extract_instrument_id(record: &WalRecord) -> Option<&str> {
        // 大部分记录类型包含 instrument_id 字段
        // 这里返回 None 表示该记录类型没有合约字段
        match record {
            WalRecord::OrderInsert { instrument_id, .. }
            | WalRecord::TickData { instrument_id, .. }
            | WalRecord::OrderBookSnapshot { instrument_id, .. }
            | WalRecord::OrderBookDelta { instrument_id, .. }
            | WalRecord::KLineFinished { instrument_id, .. }
            | WalRecord::FactorUpdate { instrument_id, .. } => {
                // 零拷贝：直接从固定数组创建 &str
                let s = std::str::from_utf8(instrument_id)
                    .ok()?
                    .trim_end_matches('\0');
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            // Exchange 记录使用 `instrument` 字段而非 `instrument_id`
            WalRecord::ExchangeOrderRecord { instrument, .. }
            | WalRecord::ExchangeTradeRecord { instrument, .. }
            | WalRecord::ExchangeResponseRecord { instrument, .. } => {
                let s = std::str::from_utf8(instrument)
                    .ok()?
                    .trim_end_matches('\0');
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            _ => None,
        }
    }

    /// 提取价格字段
    #[inline]
    fn extract_price(record: &WalRecord) -> Option<f64> {
        match record {
            WalRecord::OrderInsert { price, .. } => Some(*price),
            WalRecord::TradeExecuted { price, .. } => Some(*price),
            WalRecord::TickData { last_price, .. } => Some(*last_price),
            WalRecord::OrderBookSnapshot { last_price, .. } => Some(*last_price),
            WalRecord::OrderBookDelta { price, .. } => Some(*price),
            WalRecord::KLineFinished { close, .. } => Some(*close),
            WalRecord::ExchangeOrderRecord { price, .. } => Some(*price),
            WalRecord::ExchangeTradeRecord { deal_price, .. } => Some(*deal_price),
            _ => None,
        }
    }

    /// 创建迭代器适配器（用于过滤 Vec）
    #[inline]
    pub fn filter_iter<'a, I>(
        &'a self,
        iter: I,
    ) -> impl Iterator<Item = (i64, u64, WalRecord)> + 'a
    where
        I: Iterator<Item = (i64, u64, WalRecord)> + 'a,
    {
        let mut count = 0usize;
        let mut skipped = 0usize;
        let limit = self.limit;
        let offset = self.offset;

        iter.filter(move |(ts, _, record)| self.matches(record, *ts))
            .skip_while(move |_| {
                if skipped < offset {
                    skipped += 1;
                    true
                } else {
                    false
                }
            })
            .take_while(move |_| {
                if let Some(l) = limit {
                    count += 1;
                    count <= l
                } else {
                    true
                }
            })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 预定义过滤器（常用查询模式）
// ═══════════════════════════════════════════════════════════════════════════

impl QueryFilter {
    /// K 线数据过滤器
    pub fn klines() -> Self {
        Self::new().with_record_types(
            RecordTypeSet::EMPTY.insert(RecordType::KLineFinished),
        )
    }

    /// Tick 数据过滤器
    pub fn ticks() -> Self {
        Self::new().with_record_types(
            RecordTypeSet::EMPTY.insert(RecordType::TickData),
        )
    }

    /// 订单簿数据过滤器
    pub fn orderbook() -> Self {
        Self::new().with_record_types(
            RecordTypeSet::EMPTY
                .insert(RecordType::OrderBookSnapshot)
                .insert(RecordType::OrderBookDelta),
        )
    }

    /// 交易数据过滤器
    pub fn trades() -> Self {
        Self::new().with_record_types(
            RecordTypeSet::EMPTY
                .insert(RecordType::TradeExecuted)
                .insert(RecordType::ExchangeTradeRecord),
        )
    }

    /// 因子数据过滤器
    pub fn factors() -> Self {
        Self::new().with_record_types(RecordTypeSet::FACTOR)
    }

    /// 市场数据过滤器（所有行情相关）
    pub fn market_data() -> Self {
        Self::new().with_record_types(RecordTypeSet::MARKET_DATA)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_set() {
        let set = RecordTypeSet::EMPTY
            .insert(RecordType::TickData)
            .insert(RecordType::KLineFinished);

        assert!(set.contains(RecordType::TickData));
        assert!(set.contains(RecordType::KLineFinished));
        assert!(!set.contains(RecordType::OrderInsert));
    }

    #[test]
    fn test_record_type_set_predefined() {
        assert!(RecordTypeSet::MARKET_DATA.contains(RecordType::TickData));
        assert!(RecordTypeSet::MARKET_DATA.contains(RecordType::KLineFinished));
        assert!(!RecordTypeSet::MARKET_DATA.contains(RecordType::OrderInsert));
    }

    #[test]
    fn test_query_filter_time_range() {
        let filter = QueryFilter::new().with_time_range(1000, 2000);

        let record = WalRecord::Checkpoint {
            sequence: 1,
            timestamp: 1500,
        };
        assert!(filter.matches(&record, 1500));
        assert!(!filter.matches(&record, 500));
        assert!(!filter.matches(&record, 2500));
    }

    #[test]
    fn test_query_filter_type() {
        let filter = QueryFilter::klines();

        let kline = WalRecord::KLineFinished {
            instrument_id: [0u8; 16],
            period: 60,
            kline_timestamp: 1000,
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 105.0,
            volume: 1000,
            amount: 100000.0,
            open_oi: 500,
            close_oi: 510,
            timestamp: 1000,
        };

        let tick = WalRecord::TickData {
            instrument_id: [0u8; 16],
            last_price: 100.0,
            bid_price: 99.9,
            ask_price: 100.1,
            volume: 100,
            timestamp: 1000,
        };

        assert!(filter.matches(&kline, 1000));
        assert!(!filter.matches(&tick, 1000));
    }

    #[test]
    fn test_record_category() {
        assert_eq!(RecordType::TickData.category(), RecordCategory::MarketData);
        assert_eq!(RecordType::FactorUpdate.category(), RecordCategory::Factor);
        assert_eq!(RecordType::OrderInsert.category(), RecordCategory::Order);
    }
}
