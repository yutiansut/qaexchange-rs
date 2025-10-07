// WAL Record 数据结构
//
// 支持的记录类型：
// - OrderInsert: 订单写入
// - TradeExecuted: 成交回报
// - AccountUpdate: 账户更新
// - Checkpoint: 检查点标记
// - TickData: Tick 行情（新增）
// - OrderBookSnapshot: 订单簿快照（新增）
// - OrderBookDelta: 订单簿增量更新（新增）
// - KLineFinished: K线数据（新增）
//
// 优化设计：
// - OrderID 品种内唯一（u64），无需全局唯一 UUID
// - 空间节省：40 bytes → 8 bytes (80% reduction)
// - 性能提升：ID 生成 AtomicU64::fetch_add (~5ns) vs UUID (~100ns) = 20x faster
// - 行情数据使用固定数组避免动态分配

use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};

/// WAL 记录类型（仅使用 rkyv 序列化，不需要 serde）
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub enum WalRecord {
    /// 账户开户
    AccountOpen {
        account_id: [u8; 64],        // 账户ID (Phase 10: 新增)
        user_id: [u8; 32],           // 用户ID (所有者)
        account_name: [u8; 64],      // 账户名称 (Phase 10: 修正语义)
        init_cash: f64,              // 初始资金
        account_type: u8,            // 0=个人, 1=机构
        timestamp: i64,              // 纳秒时间戳
    },

    /// 订单写入
    OrderInsert {
        order_id: u64,               // 品种内递增 ID (8 bytes)
        user_id: [u8; 32],           // 用户ID
        instrument_id: [u8; 16],     // 合约ID（已隐含在 Per-Instrument WAL 中，但保留用于跨品种查询）
        direction: u8,               // 0=BUY, 1=SELL
        offset: u8,                  // 0=OPEN, 1=CLOSE
        price: f64,
        volume: f64,
        timestamp: i64,              // 纳秒时间戳
    },

    /// 成交回报
    TradeExecuted {
        trade_id: u64,               // 品种内递增 trade ID
        order_id: u64,               // 品种内 order ID
        exchange_order_id: u64,      // 交易所 order ID (如果是模拟盘则等于 order_id)
        price: f64,
        volume: f64,
        timestamp: i64,
    },

    /// 账户更新
    AccountUpdate {
        user_id: [u8; 32],
        balance: f64,
        available: f64,
        frozen: f64,
        margin: f64,
        timestamp: i64,
    },

    /// Checkpoint（标记可以安全截断的位置）
    Checkpoint {
        sequence: u64,
        timestamp: i64,
    },

    /// Tick 行情数据
    TickData {
        instrument_id: [u8; 16],     // 合约ID（定长）
        last_price: f64,             // 最新价
        bid_price: f64,              // 买一价（0.0 表示无）
        ask_price: f64,              // 卖一价（0.0 表示无）
        volume: i64,                 // 成交量
        timestamp: i64,              // 纳秒时间戳
    },

    /// 订单簿快照（Level2，10档）
    OrderBookSnapshot {
        instrument_id: [u8; 16],     // 合约ID
        bids: [(f64, i64); 10],      // 买盘10档 (价格, 数量)，固定数组避免动态分配
        asks: [(f64, i64); 10],      // 卖盘10档 (价格, 数量)
        last_price: f64,             // 最新价
        timestamp: i64,              // 纳秒时间戳
    },

    /// 订单簿增量更新（Level1）
    OrderBookDelta {
        instrument_id: [u8; 16],     // 合约ID
        side: u8,                    // 0=bid, 1=ask
        price: f64,                  // 价格
        volume: i64,                 // 数量（0 表示删除该价格档位）
        timestamp: i64,              // 纳秒时间戳
    },

    /// 用户注册
    UserRegister {
        user_id: [u8; 40],           // 用户ID (UUID, 36 chars + padding)
        username: [u8; 32],          // 用户名
        password_hash: [u8; 64],     // 密码哈希 (bcrypt, 60字符)
        phone: [u8; 16],             // 手机号（可选）
        email: [u8; 32],             // 邮箱（可选）
        created_at: i64,             // 创建时间戳
    },

    /// 账户绑定到用户
    AccountBind {
        user_id: [u8; 40],           // 用户ID (UUID, 36 chars + padding)
        account_id: [u8; 40],        // 账户ID (UUID, 36 chars + padding)
        timestamp: i64,              // 绑定时间戳
    },

    /// 交易所内部逐笔委托记录 (Phase 5)
    /// 存储路径: {instrument_id}/orders/
    ExchangeOrderRecord {
        exchange: [u8; 16],          // 交易所代码 (e.g. "SHFE")
        instrument: [u8; 16],        // 合约代码 (e.g. "cu2501")
        exchange_order_id: i64,      // 交易所订单号（统一事件序列）
        direction: u8,               // 0=BUY, 1=SELL
        offset: u8,                  // 0=OPEN, 1=CLOSE, 2=CLOSETODAY
        price_type: u8,              // 0=LIMIT, 1=MARKET
        price: f64,                  // 委托价格
        volume: f64,                 // 委托数量
        time: i64,                   // 纳秒时间戳
        internal_order_id: [u8; 32], // 内部订单ID (用于映射)
        user_id: [u8; 32],           // 用户ID (所有者)
    },

    /// 交易所内部逐笔成交记录 (Phase 5)
    /// 存储路径: {instrument_id}/trades/
    ExchangeTradeRecord {
        exchange: [u8; 16],          // 交易所代码
        instrument: [u8; 16],        // 合约代码
        buy_exchange_order_id: i64,  // 买方交易所订单号
        sell_exchange_order_id: i64, // 卖方交易所订单号
        deal_price: f64,             // 成交价格
        deal_volume: f64,            // 成交数量
        time: i64,                   // 纳秒时间戳
        trade_id: i64,               // 成交ID（统一事件序列）
    },

    /// 交易所回报记录 (Phase 5)
    /// 存储路径: __ACCOUNT__/{user_id}/
    /// 包含5种回报类型: OrderAccepted, OrderRejected, Trade, CancelAccepted, CancelRejected
    ExchangeResponseRecord {
        response_type: u8,           // 0=OrderAccepted, 1=OrderRejected, 2=Trade, 3=CancelAccepted, 4=CancelRejected
        exchange_order_id: i64,      // 交易所订单号
        instrument: [u8; 16],        // 合约代码
        user_id: [u8; 32],           // 用户ID
        timestamp: i64,              // 纳秒时间戳
        // 可选字段 (根据response_type使用)
        trade_id: i64,               // 仅Trade类型使用
        volume: f64,                 // Trade类型: 成交量
        price: f64,                  // Trade类型: 成交价格
        reason: [u8; 128],           // Rejected类型: 拒绝原因
    },

    /// K线数据（完成的K线）
    /// 存储路径: {instrument_id}/klines/
    /// 用于K线数据的持久化和恢复
    /// @yutiansut @quantaxis
    KLineFinished {
        instrument_id: [u8; 16],     // 合约ID
        period: i32,                 // 周期（HQChart格式: 0=Day, 3=3s, 4=1min, 5=5min, 6=15min, 7=30min, 8=60min）
        kline_timestamp: i64,        // K线起始时间戳（毫秒）
        open: f64,                   // 开盘价
        high: f64,                   // 最高价
        low: f64,                    // 最低价
        close: f64,                  // 收盘价
        volume: i64,                 // 成交量
        amount: f64,                 // 成交额
        open_oi: i64,                // 起始持仓量
        close_oi: i64,               // 结束持仓量
        timestamp: i64,              // 记录写入时间戳（纳秒）
    },
}

impl WalRecord {
    /// 辅助函数：字符串转固定长度数组 [u8; 16]
    pub fn to_fixed_array_16(s: &str) -> [u8; 16] {
        let mut arr = [0u8; 16];
        let bytes = s.as_bytes();
        let len = bytes.len().min(16);
        arr[..len].copy_from_slice(&bytes[..len]);
        arr
    }

    /// 辅助函数：字符串转固定长度数组 [u8; 32]
    pub fn to_fixed_array_32(s: &str) -> [u8; 32] {
        let mut arr = [0u8; 32];
        let bytes = s.as_bytes();
        let len = bytes.len().min(32);
        arr[..len].copy_from_slice(&bytes[..len]);
        arr
    }

    /// 辅助函数：字符串转固定长度数组 [u8; 40] (用于UUID)
    pub fn to_fixed_array_40(s: &str) -> [u8; 40] {
        let mut arr = [0u8; 40];
        let bytes = s.as_bytes();
        let len = bytes.len().min(40);
        arr[..len].copy_from_slice(&bytes[..len]);
        arr
    }

    /// 辅助函数：字符串转固定长度数组 [u8; 64]
    pub fn to_fixed_array_64(s: &str) -> [u8; 64] {
        let mut arr = [0u8; 64];
        let bytes = s.as_bytes();
        let len = bytes.len().min(64);
        arr[..len].copy_from_slice(&bytes[..len]);
        arr
    }

    /// 辅助函数：字符串转固定长度数组 [u8; 128] (Phase 5: 用于拒绝原因)
    pub fn to_fixed_array_128(s: &str) -> [u8; 128] {
        let mut arr = [0u8; 128];
        let bytes = s.as_bytes();
        let len = bytes.len().min(128);
        arr[..len].copy_from_slice(&bytes[..len]);
        arr
    }

    /// 辅助函数：固定数组转字符串
    pub fn from_fixed_array(arr: &[u8]) -> String {
        String::from_utf8_lossy(arr)
            .trim_end_matches('\0')
            .to_string()
    }
}

/// WAL 日志条目
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct WalEntry {
    pub sequence: u64,           // 递增序列号
    pub crc32: u32,              // 数据校验和
    pub timestamp: i64,          // 纳秒时间戳
    pub record: WalRecord,       // 实际数据
}

impl WalEntry {
    /// 创建新的 WAL 条目
    pub fn new(sequence: u64, record: WalRecord) -> Self {
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        Self {
            sequence,
            crc32: 0,  // 稍后计算
            timestamp,
            record,
        }
    }

    /// 序列化为字节流（rkyv）
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        rkyv::to_bytes::<_, 2048>(self)
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("rkyv serialization failed: {}", e))
    }

    /// 从字节流反序列化（零拷贝）
    pub fn from_bytes(bytes: &[u8]) -> Result<&ArchivedWalEntry, String> {
        rkyv::check_archived_root::<WalEntry>(bytes)
            .map_err(|e| format!("WAL deserialization failed: {}", e))
    }

    /// 计算 CRC32 校验和
    pub fn calculate_crc32(&self) -> u32 {
        use crc32fast::Hasher;

        let mut hasher = Hasher::new();

        // 将 record 序列化后计算 CRC32
        if let Ok(bytes) = rkyv::to_bytes::<_, 2048>(&self.record) {
            hasher.update(&bytes);
            hasher.finalize()
        } else {
            0
        }
    }

    /// 设置 CRC32
    pub fn with_crc32(mut self) -> Self {
        self.crc32 = self.calculate_crc32();
        self
    }

    /// 验证 CRC32
    pub fn verify_crc32(&self) -> bool {
        self.crc32 == self.calculate_crc32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_record_serialization() {
        let record = WalRecord::OrderInsert {
            order_id: 1,
            user_id: [2u8; 32],
            instrument_id: [3u8; 16],
            direction: 0,
            offset: 0,
            price: 100.0,
            volume: 10.0,
            timestamp: 12345,
        };

        let entry = WalEntry::new(1, record).with_crc32();

        // 序列化
        let bytes = entry.to_bytes().unwrap();

        // 反序列化
        let archived = WalEntry::from_bytes(&bytes).unwrap();

        assert_eq!(archived.sequence, 1);
        assert_eq!(archived.crc32, entry.crc32);
    }

    #[test]
    fn test_crc32_validation() {
        let record = WalRecord::TradeExecuted {
            trade_id: 1,
            order_id: 2,
            exchange_order_id: 3,
            price: 100.0,
            volume: 10.0,
            timestamp: 12345,
        };

        let entry = WalEntry::new(1, record.clone()).with_crc32();

        // CRC32 应该匹配
        assert!(entry.verify_crc32());

        // 修改 record 数据后不匹配
        let bad_record = WalRecord::TradeExecuted {
            trade_id: 1,
            order_id: 2,
            exchange_order_id: 3,
            price: 999.0,  // Changed price
            volume: 10.0,
            timestamp: 12345,
        };
        let mut bad_entry = WalEntry::new(1, bad_record).with_crc32();
        bad_entry.crc32 = entry.crc32;  // Use wrong CRC32
        assert!(!bad_entry.verify_crc32());
    }

    #[test]
    fn test_round_trip() {
        let record = WalRecord::AccountUpdate {
            user_id: [1u8; 32],
            balance: 1000000.0,
            available: 900000.0,
            frozen: 100000.0,
            margin: 50000.0,
            timestamp: 12345,
        };

        let entry = WalEntry::new(1, record).with_crc32();

        // 序列化
        let bytes = entry.to_bytes().unwrap();

        // 反序列化
        let archived = WalEntry::from_bytes(&bytes).unwrap();

        // 转换为 owned
        let recovered: WalEntry = archived.deserialize(&mut rkyv::Infallible).unwrap();

        assert_eq!(recovered.sequence, entry.sequence);
        assert_eq!(recovered.crc32, entry.crc32);
        assert!(recovered.verify_crc32());
    }
}
