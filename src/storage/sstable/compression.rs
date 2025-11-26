//! 数据类型感知的压缩策略
//!
//! @yutiansut @quantaxis
//!
//! 不同数据类型的特点和推荐压缩算法：
//!
//! | 数据类型 | 特点 | 压缩算法 | 原因 |
//! |----------|------|----------|------|
//! | K线数据 | 数值密集、高压缩比 | ZSTD-3 | 高压缩比，查询不频繁 |
//! | Tick数据 | 高频写入、读取频繁 | LZ4 | 超快解压，牺牲压缩比 |
//! | 订单簿 | 结构化、增量小 | Snappy | 平衡压缩比和速度 |
//! | 因子数据 | 数值密集、批量查询 | ZSTD-1 | 中等压缩，快速解压 |
//! | 交易数据 | 审计需要、低频访问 | ZSTD-6 | 最高压缩比 |
//! | 账户数据 | 小数据量、频繁更新 | Uncompressed | 延迟敏感 |

use crate::storage::hybrid::query_filter::RecordCategory;
use parquet2::compression::{CompressionOptions, ZstdLevel};

// ═══════════════════════════════════════════════════════════════════════════
// 压缩策略枚举
// ═══════════════════════════════════════════════════════════════════════════

/// 压缩算法选择
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// 无压缩（最低延迟）
    Uncompressed,
    /// Snappy（平衡）
    Snappy,
    /// LZ4（最快解压）
    Lz4,
    /// ZSTD Level 1（快速压缩）
    Zstd1,
    /// ZSTD Level 3（默认）
    Zstd3,
    /// ZSTD Level 6（高压缩）
    Zstd6,
    /// ZSTD Level 9（最高压缩）
    Zstd9,
}

impl CompressionAlgorithm {
    /// 转换为 Parquet 压缩选项
    pub fn to_parquet_options(self) -> CompressionOptions {
        match self {
            Self::Uncompressed => CompressionOptions::Uncompressed,
            Self::Snappy => CompressionOptions::Snappy,
            Self::Lz4 => CompressionOptions::Lz4,
            Self::Zstd1 => CompressionOptions::Zstd(Some(ZstdLevel::try_new(1).unwrap())),
            Self::Zstd3 => CompressionOptions::Zstd(Some(ZstdLevel::try_new(3).unwrap())),
            Self::Zstd6 => CompressionOptions::Zstd(Some(ZstdLevel::try_new(6).unwrap())),
            Self::Zstd9 => CompressionOptions::Zstd(Some(ZstdLevel::try_new(9).unwrap())),
        }
    }

    /// 获取压缩级别名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::Uncompressed => "uncompressed",
            Self::Snappy => "snappy",
            Self::Lz4 => "lz4",
            Self::Zstd1 => "zstd-1",
            Self::Zstd3 => "zstd-3",
            Self::Zstd6 => "zstd-6",
            Self::Zstd9 => "zstd-9",
        }
    }

    /// 预估压缩比（用于容量规划）
    pub fn estimated_ratio(&self) -> f64 {
        match self {
            Self::Uncompressed => 1.0,
            Self::Snappy => 0.6,
            Self::Lz4 => 0.65,
            Self::Zstd1 => 0.5,
            Self::Zstd3 => 0.4,
            Self::Zstd6 => 0.35,
            Self::Zstd9 => 0.3,
        }
    }

    /// 预估解压速度（MB/s，用于查询规划）
    pub fn estimated_decompress_speed_mbps(&self) -> u32 {
        match self {
            Self::Uncompressed => 10000, // 内存带宽
            Self::Snappy => 1500,
            Self::Lz4 => 3000,
            Self::Zstd1 => 1200,
            Self::Zstd3 => 1000,
            Self::Zstd6 => 800,
            Self::Zstd9 => 600,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 压缩策略配置
// ═══════════════════════════════════════════════════════════════════════════

/// 压缩策略配置
#[derive(Debug, Clone)]
pub struct CompressionStrategy {
    /// 账户数据压缩
    pub account: CompressionAlgorithm,
    /// 用户数据压缩
    pub user: CompressionAlgorithm,
    /// 订单数据压缩
    pub order: CompressionAlgorithm,
    /// 行情数据压缩（Tick/OrderBook）
    pub market_data: CompressionAlgorithm,
    /// K线数据压缩
    pub kline: CompressionAlgorithm,
    /// 交易所逐笔压缩
    pub exchange: CompressionAlgorithm,
    /// 因子数据压缩
    pub factor: CompressionAlgorithm,
    /// 系统数据压缩（Checkpoint等）
    pub system: CompressionAlgorithm,
    /// 默认压缩（未知类型）
    pub default: CompressionAlgorithm,
}

impl Default for CompressionStrategy {
    fn default() -> Self {
        Self::balanced()
    }
}

impl CompressionStrategy {
    /// 平衡策略（默认）
    /// 在压缩比和速度之间取得平衡
    pub fn balanced() -> Self {
        Self {
            account: CompressionAlgorithm::Uncompressed, // 延迟敏感
            user: CompressionAlgorithm::Snappy,           // 小数据量
            order: CompressionAlgorithm::Lz4,             // 高频访问
            market_data: CompressionAlgorithm::Lz4,       // 高频访问
            kline: CompressionAlgorithm::Zstd3,           // 批量查询
            exchange: CompressionAlgorithm::Zstd6,        // 归档存储
            factor: CompressionAlgorithm::Zstd1,          // 数值密集
            system: CompressionAlgorithm::Snappy,         // 小数据量
            default: CompressionAlgorithm::Snappy,
        }
    }

    /// 低延迟策略
    /// 优先保证查询速度
    pub fn low_latency() -> Self {
        Self {
            account: CompressionAlgorithm::Uncompressed,
            user: CompressionAlgorithm::Uncompressed,
            order: CompressionAlgorithm::Lz4,
            market_data: CompressionAlgorithm::Lz4,
            kline: CompressionAlgorithm::Lz4,
            exchange: CompressionAlgorithm::Lz4,
            factor: CompressionAlgorithm::Lz4,
            system: CompressionAlgorithm::Uncompressed,
            default: CompressionAlgorithm::Lz4,
        }
    }

    /// 高压缩策略
    /// 优先节省存储空间
    pub fn high_compression() -> Self {
        Self {
            account: CompressionAlgorithm::Zstd3,
            user: CompressionAlgorithm::Zstd3,
            order: CompressionAlgorithm::Zstd6,
            market_data: CompressionAlgorithm::Zstd6,
            kline: CompressionAlgorithm::Zstd9,
            exchange: CompressionAlgorithm::Zstd9,
            factor: CompressionAlgorithm::Zstd6,
            system: CompressionAlgorithm::Zstd3,
            default: CompressionAlgorithm::Zstd6,
        }
    }

    /// 归档策略
    /// 用于冷数据长期存储
    pub fn archive() -> Self {
        Self {
            account: CompressionAlgorithm::Zstd6,
            user: CompressionAlgorithm::Zstd6,
            order: CompressionAlgorithm::Zstd9,
            market_data: CompressionAlgorithm::Zstd9,
            kline: CompressionAlgorithm::Zstd9,
            exchange: CompressionAlgorithm::Zstd9,
            factor: CompressionAlgorithm::Zstd9,
            system: CompressionAlgorithm::Zstd6,
            default: CompressionAlgorithm::Zstd9,
        }
    }

    /// 根据记录类别获取压缩算法
    #[inline]
    pub fn get_for_category(&self, category: RecordCategory) -> CompressionAlgorithm {
        match category {
            RecordCategory::Account => self.account,
            RecordCategory::User => self.user,
            RecordCategory::Order => self.order,
            RecordCategory::MarketData => self.market_data,
            RecordCategory::Exchange => self.exchange,
            RecordCategory::Factor => self.factor,
            RecordCategory::System => self.system,
        }
    }

    /// 自定义配置 builder
    pub fn builder() -> CompressionStrategyBuilder {
        CompressionStrategyBuilder::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Builder 模式
// ═══════════════════════════════════════════════════════════════════════════

/// 压缩策略构建器
#[derive(Debug, Clone)]
pub struct CompressionStrategyBuilder {
    strategy: CompressionStrategy,
}

impl CompressionStrategyBuilder {
    pub fn new() -> Self {
        Self {
            strategy: CompressionStrategy::balanced(),
        }
    }

    pub fn account(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.account = alg;
        self
    }

    pub fn user(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.user = alg;
        self
    }

    pub fn order(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.order = alg;
        self
    }

    pub fn market_data(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.market_data = alg;
        self
    }

    pub fn kline(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.kline = alg;
        self
    }

    pub fn exchange(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.exchange = alg;
        self
    }

    pub fn factor(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.factor = alg;
        self
    }

    pub fn system(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.system = alg;
        self
    }

    pub fn default_compression(mut self, alg: CompressionAlgorithm) -> Self {
        self.strategy.default = alg;
        self
    }

    pub fn build(self) -> CompressionStrategy {
        self.strategy
    }
}

impl Default for CompressionStrategyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 压缩统计
// ═══════════════════════════════════════════════════════════════════════════

/// 压缩统计信息
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// 原始大小（字节）
    pub raw_bytes: u64,
    /// 压缩后大小（字节）
    pub compressed_bytes: u64,
    /// 压缩耗时（纳秒）
    pub compress_time_ns: u64,
    /// 解压耗时（纳秒）
    pub decompress_time_ns: u64,
    /// 压缩次数
    pub compress_count: u64,
    /// 解压次数
    pub decompress_count: u64,
}

impl CompressionStats {
    /// 计算压缩比
    pub fn compression_ratio(&self) -> f64 {
        if self.raw_bytes == 0 {
            1.0
        } else {
            self.compressed_bytes as f64 / self.raw_bytes as f64
        }
    }

    /// 计算平均压缩速度（MB/s）
    pub fn avg_compress_speed_mbps(&self) -> f64 {
        if self.compress_time_ns == 0 || self.compress_count == 0 {
            0.0
        } else {
            let avg_time_s = (self.compress_time_ns as f64 / self.compress_count as f64) / 1e9;
            let avg_bytes = self.raw_bytes as f64 / self.compress_count as f64;
            (avg_bytes / 1024.0 / 1024.0) / avg_time_s
        }
    }

    /// 计算平均解压速度（MB/s）
    pub fn avg_decompress_speed_mbps(&self) -> f64 {
        if self.decompress_time_ns == 0 || self.decompress_count == 0 {
            0.0
        } else {
            let avg_time_s = (self.decompress_time_ns as f64 / self.decompress_count as f64) / 1e9;
            let avg_bytes = self.raw_bytes as f64 / self.decompress_count as f64;
            (avg_bytes / 1024.0 / 1024.0) / avg_time_s
        }
    }

    /// 合并统计
    pub fn merge(&mut self, other: &Self) {
        self.raw_bytes += other.raw_bytes;
        self.compressed_bytes += other.compressed_bytes;
        self.compress_time_ns += other.compress_time_ns;
        self.decompress_time_ns += other.decompress_time_ns;
        self.compress_count += other.compress_count;
        self.decompress_count += other.decompress_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_algorithm_parquet() {
        let alg = CompressionAlgorithm::Zstd3;
        let opts = alg.to_parquet_options();
        // 验证返回的是 Zstd 压缩，级别为 3
        match opts {
            CompressionOptions::Zstd(Some(level)) => {
                assert_eq!(level.compression_level(), 3);
            }
            _ => panic!("Expected Zstd compression with level 3"),
        }
    }

    #[test]
    fn test_compression_strategy_balanced() {
        let strategy = CompressionStrategy::balanced();
        assert_eq!(strategy.account, CompressionAlgorithm::Uncompressed);
        assert_eq!(strategy.market_data, CompressionAlgorithm::Lz4);
        assert_eq!(strategy.kline, CompressionAlgorithm::Zstd3);
    }

    #[test]
    fn test_compression_strategy_builder() {
        let strategy = CompressionStrategy::builder()
            .account(CompressionAlgorithm::Lz4)
            .kline(CompressionAlgorithm::Zstd9)
            .build();

        assert_eq!(strategy.account, CompressionAlgorithm::Lz4);
        assert_eq!(strategy.kline, CompressionAlgorithm::Zstd9);
    }

    #[test]
    fn test_compression_stats() {
        let stats = CompressionStats {
            raw_bytes: 1000,
            compressed_bytes: 400,
            compress_time_ns: 1_000_000,
            decompress_time_ns: 500_000,
            compress_count: 10,
            decompress_count: 10,
        };

        assert!((stats.compression_ratio() - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_get_for_category() {
        let strategy = CompressionStrategy::balanced();
        assert_eq!(
            strategy.get_for_category(RecordCategory::MarketData),
            CompressionAlgorithm::Lz4
        );
        assert_eq!(
            strategy.get_for_category(RecordCategory::Factor),
            CompressionAlgorithm::Zstd1
        );
    }
}
