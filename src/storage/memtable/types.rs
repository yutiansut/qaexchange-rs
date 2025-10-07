// MemTable 数据类型定义

use crate::storage::wal::WalRecord;
use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};

/// MemTable Key：组合键用于排序和查询
///
/// 设计：
/// - timestamp: 纳秒时间戳（主排序键）
/// - sequence: WAL 序列号（次排序键，保证同一时刻的顺序）
///
/// 性能：
/// - 16 bytes (i64 + u64)
/// - 实现 Ord 用于 SkipMap 排序
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MemTableKey {
    pub timestamp: i64,  // 纳秒时间戳
    pub sequence: u64,   // WAL 序列号
}

impl MemTableKey {
    pub fn new(timestamp: i64, sequence: u64) -> Self {
        Self { timestamp, sequence }
    }

    /// 创建范围查询的起始键（指定时间戳，sequence = 0）
    pub fn from_timestamp(timestamp: i64) -> Self {
        Self { timestamp, sequence: 0 }
    }

    /// 创建范围查询的结束键（指定时间戳，sequence = u64::MAX）
    pub fn to_timestamp(timestamp: i64) -> Self {
        Self { timestamp, sequence: u64::MAX }
    }

    /// 序列化为字节数组（用于 Compaction 比较）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes
    }
}

/// MemTable Value：存储实际数据
///
/// 优化设计：
/// - 直接存储 WalRecord，避免重复序列化
/// - 支持零拷贝 rkyv 序列化
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct MemTableValue {
    pub record: WalRecord,
}

impl MemTableValue {
    pub fn new(record: WalRecord) -> Self {
        Self { record }
    }

    /// 获取记录的时间戳
    pub fn timestamp(&self) -> i64 {
        match &self.record {
            WalRecord::AccountOpen { timestamp, .. } => *timestamp,
            WalRecord::OrderInsert { timestamp, .. } => *timestamp,
            WalRecord::TradeExecuted { timestamp, .. } => *timestamp,
            WalRecord::AccountUpdate { timestamp, .. } => *timestamp,
            WalRecord::Checkpoint { timestamp, .. } => *timestamp,
            WalRecord::TickData { timestamp, .. } => *timestamp,
            WalRecord::OrderBookSnapshot { timestamp, .. } => *timestamp,
            WalRecord::OrderBookDelta { timestamp, .. } => *timestamp,
            WalRecord::UserRegister { created_at, .. } => *created_at,
            WalRecord::AccountBind { timestamp, .. } => *timestamp,
            WalRecord::ExchangeOrderRecord { time, .. } => *time,
            WalRecord::ExchangeTradeRecord { time, .. } => *time,
            WalRecord::ExchangeResponseRecord { timestamp, .. } => *timestamp,
            WalRecord::KLineFinished { timestamp, .. } => *timestamp,
        }
    }
}

/// MemTable Entry：完整的键值对
pub struct MemTableEntry {
    pub key: MemTableKey,
    pub value: MemTableValue,
}

impl MemTableEntry {
    pub fn new(key: MemTableKey, value: MemTableValue) -> Self {
        Self { key, value }
    }

    pub fn from_wal(sequence: u64, record: WalRecord) -> Self {
        let timestamp = match &record {
            WalRecord::AccountOpen { timestamp, .. } => *timestamp,
            WalRecord::OrderInsert { timestamp, .. } => *timestamp,
            WalRecord::TradeExecuted { timestamp, .. } => *timestamp,
            WalRecord::AccountUpdate { timestamp, .. } => *timestamp,
            WalRecord::Checkpoint { timestamp, .. } => *timestamp,
            WalRecord::TickData { timestamp, .. } => *timestamp,
            WalRecord::OrderBookSnapshot { timestamp, .. } => *timestamp,
            WalRecord::OrderBookDelta { timestamp, .. } => *timestamp,
            WalRecord::UserRegister { created_at, .. } => *created_at,
            WalRecord::AccountBind { timestamp, .. } => *timestamp,
            WalRecord::ExchangeOrderRecord { time, .. } => *time,
            WalRecord::ExchangeTradeRecord { time, .. } => *time,
            WalRecord::ExchangeResponseRecord { timestamp, .. } => *timestamp,
            WalRecord::KLineFinished { timestamp, .. } => *timestamp,
        };

        Self {
            key: MemTableKey::new(timestamp, sequence),
            value: MemTableValue::new(record),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memtable_key_ordering() {
        let key1 = MemTableKey::new(1000, 1);
        let key2 = MemTableKey::new(1000, 2);
        let key3 = MemTableKey::new(2000, 1);

        // 时间戳优先排序
        assert!(key1 < key3);
        assert!(key2 < key3);

        // 同一时间戳按 sequence 排序
        assert!(key1 < key2);
    }

    #[test]
    fn test_range_keys() {
        let start = MemTableKey::from_timestamp(1000);
        let end = MemTableKey::to_timestamp(1000);

        assert_eq!(start.timestamp, 1000);
        assert_eq!(start.sequence, 0);
        assert_eq!(end.timestamp, 1000);
        assert_eq!(end.sequence, u64::MAX);

        // 范围覆盖所有 sequence
        let key = MemTableKey::new(1000, 12345);
        assert!(start <= key);
        assert!(key <= end);
    }

    #[test]
    fn test_entry_from_wal() {
        let record = WalRecord::OrderInsert {
            order_id: 1,
            user_id: [1u8; 32],
            instrument_id: [1u8; 16],
            direction: 0,
            offset: 0,
            price: 4000.0,
            volume: 10.0,
            timestamp: 12345,
        };

        let entry = MemTableEntry::from_wal(1, record);
        assert_eq!(entry.key.timestamp, 12345);
        assert_eq!(entry.key.sequence, 1);
    }
}
