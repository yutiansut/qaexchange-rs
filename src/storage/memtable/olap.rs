// OLAP MemTable - Arrow2 列式内存表
//
// 设计理念:
// - 批量构建，不支持单条插入
// - 列式存储，适合扫描和聚合查询
// - 不可变，一旦构建完成不可修改
// - 支持高效的列式过滤和投影

use arrow2::array::{
    Array, BooleanArray, FixedSizeBinaryArray, MutableArray, MutableFixedSizeBinaryArray,
    MutablePrimitiveArray, PrimitiveArray,
};
use arrow2::chunk::Chunk;
use arrow2::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

use super::types::{MemTableKey, MemTableValue};
use crate::storage::wal::record::WalRecord;

/// OLAP MemTable Schema
///
/// 统一的列式 Schema，支持所有记录类型
pub fn create_olap_schema() -> Schema {
    Schema::from(vec![
        // 主键字段
        Field::new("timestamp", DataType::Int64, false), // 纳秒时间戳
        Field::new("sequence", DataType::UInt64, false), // WAL 序列号
        Field::new("record_type", DataType::UInt8, false), // 0=OrderInsert, 1=TradeExecuted, 2=AccountUpdate, 3=Checkpoint, 13=KLineFinished
        // Order 字段
        Field::new("order_id", DataType::UInt64, true),
        Field::new("user_id", DataType::FixedSizeBinary(32), true),
        Field::new("instrument_id", DataType::FixedSizeBinary(16), true),
        Field::new("direction", DataType::UInt8, true),
        Field::new("offset", DataType::UInt8, true),
        Field::new("price", DataType::Float64, true),
        Field::new("volume", DataType::Float64, true),
        // Trade 字段
        Field::new("trade_id", DataType::UInt64, true),
        Field::new("exchange_order_id", DataType::UInt64, true),
        // Account 字段
        Field::new("balance", DataType::Float64, true),
        Field::new("available", DataType::Float64, true),
        Field::new("frozen", DataType::Float64, true),
        Field::new("margin", DataType::Float64, true),
        // K线字段
        Field::new("kline_period", DataType::Int32, true), // K线周期
        Field::new("kline_timestamp", DataType::Int64, true), // K线起始时间戳（毫秒）
        Field::new("kline_open", DataType::Float64, true), // 开盘价
        Field::new("kline_high", DataType::Float64, true), // 最高价
        Field::new("kline_low", DataType::Float64, true),  // 最低价
        Field::new("kline_close", DataType::Float64, true), // 收盘价
        Field::new("kline_volume", DataType::Int64, true), // 成交量
        Field::new("kline_amount", DataType::Float64, true), // 成交额
        Field::new("kline_open_oi", DataType::Int64, true), // 起始持仓量
        Field::new("kline_close_oi", DataType::Int64, true), // 结束持仓量
    ])
}

/// OLAP MemTable - 批量列式存储
///
/// 特点:
/// - 不可变（Immutable）
/// - 批量构建
/// - 列式存储（Arrow2）
/// - 高效范围查询
pub struct OlapMemTable {
    /// Arrow2 Schema
    schema: Arc<Schema>,

    /// Arrow2 Chunk (一批数据)
    chunk: Arc<Chunk<Box<dyn Array>>>,

    /// 统计信息
    entry_count: usize,
    min_timestamp: i64,
    max_timestamp: i64,
}

impl OlapMemTable {
    /// 从 OLTP 记录批量构建 OLAP MemTable
    ///
    /// # Arguments
    /// * `records` - (key, record) 元组列表，必须按时间戳排序
    pub fn from_records(records: Vec<(MemTableKey, WalRecord)>) -> Self {
        if records.is_empty() {
            return Self::empty();
        }

        let entry_count = records.len();
        let min_timestamp = records.first().unwrap().0.timestamp;
        let max_timestamp = records.last().unwrap().0.timestamp;

        // 构建 Arrow2 数组
        let schema = Arc::new(create_olap_schema());
        let chunk = Arc::new(build_chunk(&records));

        Self {
            schema,
            chunk,
            entry_count,
            min_timestamp,
            max_timestamp,
        }
    }

    /// 创建空的 MemTable
    fn empty() -> Self {
        let schema = Arc::new(create_olap_schema());
        let chunk = Arc::new(Chunk::new(vec![]));

        Self {
            schema,
            chunk,
            entry_count: 0,
            min_timestamp: i64::MAX,
            max_timestamp: i64::MIN,
        }
    }

    /// 获取条目数量
    pub fn len(&self) -> usize {
        self.entry_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// 获取时间戳范围
    pub fn time_range(&self) -> (i64, i64) {
        (self.min_timestamp, self.max_timestamp)
    }

    /// 获取 Schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// 获取 Chunk（用于写入 Parquet）
    pub fn chunk(&self) -> &Chunk<Box<dyn Array>> {
        &self.chunk
    }

    /// 范围查询
    ///
    /// 返回时间戳范围内的所有记录
    pub fn range_query(&self, start_ts: i64, end_ts: i64) -> Vec<(MemTableKey, WalRecord)> {
        if self.is_empty() {
            return Vec::new();
        }

        // 快速路径：时间范围不重叠
        if end_ts < self.min_timestamp || start_ts > self.max_timestamp {
            return Vec::new();
        }

        // 提取时间戳列和序列号列
        let timestamp_array = self.chunk.arrays()[0]
            .as_any()
            .downcast_ref::<PrimitiveArray<i64>>()
            .unwrap();

        let sequence_array = self.chunk.arrays()[1]
            .as_any()
            .downcast_ref::<PrimitiveArray<u64>>()
            .unwrap();

        let record_type_array = self.chunk.arrays()[2]
            .as_any()
            .downcast_ref::<PrimitiveArray<u8>>()
            .unwrap();

        let mut results = Vec::new();

        // 线性扫描（因为数据已按时间戳排序，可以优化）
        for i in 0..self.entry_count {
            let ts = timestamp_array.value(i);

            // 早停优化
            if ts < start_ts {
                continue;
            }
            if ts > end_ts {
                break;
            }

            let sequence = sequence_array.value(i);
            let key = MemTableKey {
                timestamp: ts,
                sequence,
            };

            // 重建 WalRecord
            let record = reconstruct_record(i, record_type_array.value(i), &self.chunk);

            results.push((key, record));
        }

        results
    }

    /// 估算内存占用（字节）
    pub fn memory_size(&self) -> usize {
        self.chunk
            .arrays()
            .iter()
            .map(|arr| {
                // 粗略估算：每个元素平均 8 字节
                arr.len() * 8
            })
            .sum()
    }
}

/// 从记录列表构建 Arrow2 Chunk
fn build_chunk(records: &[(MemTableKey, WalRecord)]) -> Chunk<Box<dyn Array>> {
    let len = records.len();

    // 创建 builders
    let mut timestamp_builder = MutablePrimitiveArray::<i64>::with_capacity(len);
    let mut sequence_builder = MutablePrimitiveArray::<u64>::with_capacity(len);
    let mut record_type_builder = MutablePrimitiveArray::<u8>::with_capacity(len);

    // Order 字段
    let mut order_id_builder = MutablePrimitiveArray::<u64>::with_capacity(len);
    let mut user_id_builder = MutableFixedSizeBinaryArray::with_capacity(32, len);
    let mut instrument_id_builder = MutableFixedSizeBinaryArray::with_capacity(16, len);
    let mut direction_builder = MutablePrimitiveArray::<u8>::with_capacity(len);
    let mut offset_builder = MutablePrimitiveArray::<u8>::with_capacity(len);
    let mut price_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut volume_builder = MutablePrimitiveArray::<f64>::with_capacity(len);

    // Trade 字段
    let mut trade_id_builder = MutablePrimitiveArray::<u64>::with_capacity(len);
    let mut exchange_order_id_builder = MutablePrimitiveArray::<u64>::with_capacity(len);

    // Account 字段
    let mut balance_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut available_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut frozen_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut margin_builder = MutablePrimitiveArray::<f64>::with_capacity(len);

    // K线字段
    let mut kline_period_builder = MutablePrimitiveArray::<i32>::with_capacity(len);
    let mut kline_timestamp_builder = MutablePrimitiveArray::<i64>::with_capacity(len);
    let mut kline_open_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut kline_high_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut kline_low_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut kline_close_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut kline_volume_builder = MutablePrimitiveArray::<i64>::with_capacity(len);
    let mut kline_amount_builder = MutablePrimitiveArray::<f64>::with_capacity(len);
    let mut kline_open_oi_builder = MutablePrimitiveArray::<i64>::with_capacity(len);
    let mut kline_close_oi_builder = MutablePrimitiveArray::<i64>::with_capacity(len);

    // Helper macro to push null K-line fields
    macro_rules! push_null_kline_fields {
        () => {
            kline_period_builder.push(None);
            kline_timestamp_builder.push(None);
            kline_open_builder.push(None);
            kline_high_builder.push(None);
            kline_low_builder.push(None);
            kline_close_builder.push(None);
            kline_volume_builder.push(None);
            kline_amount_builder.push(None);
            kline_open_oi_builder.push(None);
            kline_close_oi_builder.push(None);
        };
    }

    // 填充数据
    for (key, record) in records {
        timestamp_builder.push(Some(key.timestamp));
        sequence_builder.push(Some(key.sequence));

        match record {
            WalRecord::OrderInsert {
                order_id,
                user_id,
                instrument_id,
                direction,
                offset,
                price,
                volume,
                ..
            } => {
                record_type_builder.push(Some(0));
                order_id_builder.push(Some(*order_id));
                user_id_builder.push(Some(user_id));
                instrument_id_builder.push(Some(instrument_id));
                direction_builder.push(Some(*direction));
                offset_builder.push(Some(*offset));
                price_builder.push(Some(*price));
                volume_builder.push(Some(*volume));

                // Trade 字段为 null
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);

                // Account 字段为 null
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::TradeExecuted {
                trade_id,
                order_id,
                exchange_order_id,
                price,
                volume,
                ..
            } => {
                record_type_builder.push(Some(1));

                // Order 字段部分填充
                order_id_builder.push(Some(*order_id));
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(Some(*price));
                volume_builder.push(Some(*volume));

                // Trade 字段
                trade_id_builder.push(Some(*trade_id));
                exchange_order_id_builder.push(Some(*exchange_order_id));

                // Account 字段为 null
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::AccountUpdate {
                user_id,
                balance,
                available,
                frozen,
                margin,
                ..
            } => {
                record_type_builder.push(Some(2));

                // Order 字段（部分为 null）
                order_id_builder.push(None);
                user_id_builder.push(Some(user_id.as_slice()));
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);

                // Trade 字段为 null
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);

                // Account 字段
                balance_builder.push(Some(*balance));
                available_builder.push(Some(*available));
                frozen_builder.push(Some(*frozen));
                margin_builder.push(Some(*margin));

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::AccountOpen {
                user_id, init_cash, ..
            } => {
                record_type_builder.push(Some(4)); // 新类型ID

                // Order 字段（部分为 null）
                order_id_builder.push(None);
                user_id_builder.push(Some(user_id.as_slice()));
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);

                // Trade 字段为 null
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);

                // Account 字段
                balance_builder.push(Some(*init_cash));
                available_builder.push(Some(*init_cash));
                frozen_builder.push(Some(0.0));
                margin_builder.push(Some(0.0));

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::Checkpoint { .. } => {
                record_type_builder.push(Some(3));

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            // 行情记录（OLAP 主要存储交易数据，行情数据暂不存储到 OLAP）
            WalRecord::TickData { .. } => {
                record_type_builder.push(Some(5)); // TickData type ID

                // 所有字段为 null（行情数据不存储到 OLAP）
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::OrderBookSnapshot { .. } => {
                record_type_builder.push(Some(6)); // OrderBookSnapshot type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::OrderBookDelta { .. } => {
                record_type_builder.push(Some(7)); // OrderBookDelta type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            // 用户记录（OLAP 主要存储交易数据，用户数据暂不存储到 OLAP）
            WalRecord::UserRegister { .. } => {
                record_type_builder.push(Some(8)); // UserRegister type ID

                // 所有字段为 null（用户数据不存储到 OLAP）
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::AccountBind { .. } => {
                record_type_builder.push(Some(9)); // AccountBind type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            // Phase 5: 交易所内部记录（暂不存储到 OLAP）
            WalRecord::ExchangeOrderRecord { .. } => {
                record_type_builder.push(Some(10)); // ExchangeOrderRecord type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::ExchangeTradeRecord { .. } => {
                record_type_builder.push(Some(11)); // ExchangeTradeRecord type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::ExchangeResponseRecord { .. } => {
                record_type_builder.push(Some(12)); // ExchangeResponseRecord type ID

                // 所有字段为 null
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(None::<&[u8]>);
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段为 null
                push_null_kline_fields!();
            }

            WalRecord::KLineFinished {
                instrument_id: kline_instrument_id,
                period,
                kline_timestamp,
                open,
                high,
                low,
                close,
                volume: kline_volume,
                amount,
                open_oi,
                close_oi,
                ..
            } => {
                record_type_builder.push(Some(13)); // KLineFinished type ID

                // Order/Trade/Account字段为 null（K线记录不使用这些字段）
                order_id_builder.push(None);
                user_id_builder.push(None::<&[u8]>);
                instrument_id_builder.push(Some(kline_instrument_id.as_slice())); // K线合约ID
                direction_builder.push(None);
                offset_builder.push(None);
                price_builder.push(None);
                volume_builder.push(None);
                trade_id_builder.push(None);
                exchange_order_id_builder.push(None);
                balance_builder.push(None);
                available_builder.push(None);
                frozen_builder.push(None);
                margin_builder.push(None);

                // K线字段填充实际数据
                kline_period_builder.push(Some(*period));
                kline_timestamp_builder.push(Some(*kline_timestamp));
                kline_open_builder.push(Some(*open));
                kline_high_builder.push(Some(*high));
                kline_low_builder.push(Some(*low));
                kline_close_builder.push(Some(*close));
                kline_volume_builder.push(Some(*kline_volume));
                kline_amount_builder.push(Some(*amount));
                kline_open_oi_builder.push(Some(*open_oi));
                kline_close_oi_builder.push(Some(*close_oi));
            }
        }
    }

    // 转换为不可变数组
    let timestamp_array: PrimitiveArray<i64> = timestamp_builder.into();
    let sequence_array: PrimitiveArray<u64> = sequence_builder.into();
    let record_type_array: PrimitiveArray<u8> = record_type_builder.into();
    let order_id_array: PrimitiveArray<u64> = order_id_builder.into();
    let user_id_array: FixedSizeBinaryArray = user_id_builder.into();
    let instrument_id_array: FixedSizeBinaryArray = instrument_id_builder.into();
    let direction_array: PrimitiveArray<u8> = direction_builder.into();
    let offset_array: PrimitiveArray<u8> = offset_builder.into();
    let price_array: PrimitiveArray<f64> = price_builder.into();
    let volume_array: PrimitiveArray<f64> = volume_builder.into();
    let trade_id_array: PrimitiveArray<u64> = trade_id_builder.into();
    let exchange_order_id_array: PrimitiveArray<u64> = exchange_order_id_builder.into();
    let balance_array: PrimitiveArray<f64> = balance_builder.into();
    let available_array: PrimitiveArray<f64> = available_builder.into();
    let frozen_array: PrimitiveArray<f64> = frozen_builder.into();
    let margin_array: PrimitiveArray<f64> = margin_builder.into();

    // K线字段
    let kline_period_array: PrimitiveArray<i32> = kline_period_builder.into();
    let kline_timestamp_array: PrimitiveArray<i64> = kline_timestamp_builder.into();
    let kline_open_array: PrimitiveArray<f64> = kline_open_builder.into();
    let kline_high_array: PrimitiveArray<f64> = kline_high_builder.into();
    let kline_low_array: PrimitiveArray<f64> = kline_low_builder.into();
    let kline_close_array: PrimitiveArray<f64> = kline_close_builder.into();
    let kline_volume_array: PrimitiveArray<i64> = kline_volume_builder.into();
    let kline_amount_array: PrimitiveArray<f64> = kline_amount_builder.into();
    let kline_open_oi_array: PrimitiveArray<i64> = kline_open_oi_builder.into();
    let kline_close_oi_array: PrimitiveArray<i64> = kline_close_oi_builder.into();

    let arrays: Vec<Box<dyn Array>> = vec![
        Box::new(timestamp_array),
        Box::new(sequence_array),
        Box::new(record_type_array),
        Box::new(order_id_array),
        Box::new(user_id_array),
        Box::new(instrument_id_array),
        Box::new(direction_array),
        Box::new(offset_array),
        Box::new(price_array),
        Box::new(volume_array),
        Box::new(trade_id_array),
        Box::new(exchange_order_id_array),
        Box::new(balance_array),
        Box::new(available_array),
        Box::new(frozen_array),
        Box::new(margin_array),
        Box::new(kline_period_array),
        Box::new(kline_timestamp_array),
        Box::new(kline_open_array),
        Box::new(kline_high_array),
        Box::new(kline_low_array),
        Box::new(kline_close_array),
        Box::new(kline_volume_array),
        Box::new(kline_amount_array),
        Box::new(kline_open_oi_array),
        Box::new(kline_close_oi_array),
    ];

    Chunk::new(arrays)
}

/// 从 Chunk 中重建 WalRecord
fn reconstruct_record(index: usize, record_type: u8, chunk: &Chunk<Box<dyn Array>>) -> WalRecord {
    match record_type {
        0 => {
            // OrderInsert
            let timestamp = chunk.arrays()[0]
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .unwrap()
                .value(index);

            let order_id = chunk.arrays()[3]
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .unwrap()
                .value(index);

            let user_id_array = chunk.arrays()[4]
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            let mut user_id = [0u8; 32];
            user_id.copy_from_slice(user_id_array.value(index));

            let instrument_id_array = chunk.arrays()[5]
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            let mut instrument_id = [0u8; 16];
            instrument_id.copy_from_slice(instrument_id_array.value(index));

            let direction = chunk.arrays()[6]
                .as_any()
                .downcast_ref::<PrimitiveArray<u8>>()
                .unwrap()
                .value(index);

            let offset = chunk.arrays()[7]
                .as_any()
                .downcast_ref::<PrimitiveArray<u8>>()
                .unwrap()
                .value(index);

            let price = chunk.arrays()[8]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            let volume = chunk.arrays()[9]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            WalRecord::OrderInsert {
                order_id,
                user_id,
                instrument_id,
                direction,
                offset,
                price,
                volume,
                timestamp,
            }
        }

        1 => {
            // TradeExecuted
            let timestamp = chunk.arrays()[0]
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .unwrap()
                .value(index);

            let trade_id = chunk.arrays()[10]
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .unwrap()
                .value(index);

            let order_id = chunk.arrays()[3]
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .unwrap()
                .value(index);

            let exchange_order_id = chunk.arrays()[11]
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .unwrap()
                .value(index);

            let price = chunk.arrays()[8]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            let volume = chunk.arrays()[9]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            WalRecord::TradeExecuted {
                trade_id,
                order_id,
                exchange_order_id,
                price,
                volume,
                timestamp,
            }
        }

        2 => {
            // AccountUpdate
            let timestamp = chunk.arrays()[0]
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .unwrap()
                .value(index);

            let user_id_array = chunk.arrays()[4]
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            let mut user_id = [0u8; 32];
            user_id.copy_from_slice(user_id_array.value(index));

            let balance = chunk.arrays()[12]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            let available = chunk.arrays()[13]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            let frozen = chunk.arrays()[14]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            let margin = chunk.arrays()[15]
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .unwrap()
                .value(index);

            WalRecord::AccountUpdate {
                user_id,
                balance,
                available,
                frozen,
                margin,
                timestamp,
            }
        }

        3 => {
            // Checkpoint
            let timestamp = chunk.arrays()[0]
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .unwrap()
                .value(index);

            let sequence = chunk.arrays()[1]
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .unwrap()
                .value(index);

            WalRecord::Checkpoint {
                sequence,
                timestamp,
            }
        }

        _ => panic!("Unknown record type: {}", record_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_records(count: usize) -> Vec<(MemTableKey, WalRecord)> {
        (0..count)
            .map(|i| {
                let key = MemTableKey {
                    timestamp: 1000 + i as i64,
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
    fn test_from_records() {
        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        assert_eq!(memtable.len(), 100);
        assert_eq!(memtable.time_range(), (1000, 1099));
    }

    #[test]
    fn test_range_query() {
        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        let results = memtable.range_query(1010, 1020);
        assert_eq!(results.len(), 11); // 1010-1020 inclusive

        // 验证第一条记录
        assert_eq!(results[0].0.timestamp, 1010);
    }

    #[test]
    fn test_range_query_no_overlap() {
        let records = create_test_records(100);
        let memtable = OlapMemTable::from_records(records);

        let results = memtable.range_query(2000, 3000);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_empty_memtable() {
        let memtable = OlapMemTable::from_records(vec![]);
        assert!(memtable.is_empty());
        assert_eq!(memtable.len(), 0);
    }

    #[test]
    fn test_mixed_record_types() {
        let records = vec![
            (
                MemTableKey {
                    timestamp: 1000,
                    sequence: 1,
                },
                WalRecord::OrderInsert {
                    order_id: 1,
                    user_id: [1u8; 32],
                    instrument_id: [2u8; 16],
                    direction: 0,
                    offset: 0,
                    price: 100.0,
                    volume: 10.0,
                    timestamp: 1000,
                },
            ),
            (
                MemTableKey {
                    timestamp: 1001,
                    sequence: 2,
                },
                WalRecord::TradeExecuted {
                    trade_id: 1,
                    order_id: 1,
                    exchange_order_id: 100,
                    price: 100.0,
                    volume: 10.0,
                    timestamp: 1001,
                },
            ),
            (
                MemTableKey {
                    timestamp: 1002,
                    sequence: 3,
                },
                WalRecord::AccountUpdate {
                    user_id: [1u8; 32],
                    balance: 10000.0,
                    available: 9000.0,
                    frozen: 1000.0,
                    margin: 0.0,
                    timestamp: 1002,
                },
            ),
        ];

        let memtable = OlapMemTable::from_records(records);
        assert_eq!(memtable.len(), 3);

        let results = memtable.range_query(1000, 1002);
        assert_eq!(results.len(), 3);
    }
}
