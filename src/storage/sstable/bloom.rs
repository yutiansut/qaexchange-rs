// Bloom Filter - 布隆过滤器
//
// 用途：
// - SSTable 快速存在性检查
// - 避免无效的全表扫描
// - P(false positive) = (1 - e^(-kn/m))^k
//
// 参数设置（n=10000 entries, fp_rate=0.01）:
// - m = 95850 bits (12 KB)
// - k = 7 hash functions
//
// 性能：
// - 查询延迟: O(k) = ~100ns
// - 空间开销: ~12 bits/key (1% FP rate)

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Bloom Filter 实现
#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct BloomFilter {
    /// 位数组（按 u64 分组以提高效率）
    bits: Vec<u64>,

    /// 哈希函数数量
    k: u32,

    /// 位数组总位数
    m: u64,

    /// 已插入元素数量
    n: u64,
}

impl BloomFilter {
    /// 创建新的 Bloom Filter
    ///
    /// # 参数
    /// - expected_items: 预期元素数量
    /// - false_positive_rate: 期望的假阳性率（推荐 0.01 = 1%）
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // 计算最优位数组大小: m = -n*ln(p) / (ln(2)^2)
        let m = Self::optimal_m(expected_items, false_positive_rate);

        // 计算最优哈希函数数量: k = (m/n) * ln(2)
        let k = Self::optimal_k(expected_items, m);

        // 位数组按 u64 分组（每组 64 bits）
        let num_words = ((m + 63) / 64) as usize;

        Self {
            bits: vec![0u64; num_words],
            k,
            m,
            n: 0,
        }
    }

    /// 计算最优位数组大小
    fn optimal_m(n: usize, p: f64) -> u64 {
        let m = -(n as f64) * p.ln() / (2.0_f64.ln().powi(2));
        m.ceil() as u64
    }

    /// 计算最优哈希函数数量
    fn optimal_k(n: usize, m: u64) -> u32 {
        let k = (m as f64 / n as f64) * 2.0_f64.ln();
        k.ceil().max(1.0) as u32
    }

    /// 插入元素
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) {
        let hash = self.hash(item);

        for i in 0..self.k {
            let bit_index = self.get_bit_index(hash, i);
            self.set_bit(bit_index);
        }

        self.n += 1;
    }

    /// 检查元素是否可能存在
    ///
    /// 返回值：
    /// - true: 可能存在（可能假阳性）
    /// - false: 一定不存在
    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let hash = self.hash(item);

        for i in 0..self.k {
            let bit_index = self.get_bit_index(hash, i);
            if !self.get_bit(bit_index) {
                return false; // 一定不存在
            }
        }

        true // 可能存在
    }

    /// 哈希函数（使用 DefaultHasher）
    fn hash<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        hasher.finish()
    }

    /// 获取第 i 个哈希函数对应的位索引
    /// 使用双重哈希: h_i(x) = h1(x) + i * h2(x)
    fn get_bit_index(&self, hash: u64, i: u32) -> u64 {
        let h1 = (hash >> 32) as u64;
        let h2 = (hash & 0xFFFFFFFF) as u64;

        // 双重哈希公式
        let combined = h1.wrapping_add(i as u64 * h2);
        combined % self.m
    }

    /// 设置指定位
    fn set_bit(&mut self, index: u64) {
        let word_index = (index / 64) as usize;
        let bit_offset = index % 64;

        if word_index < self.bits.len() {
            self.bits[word_index] |= 1u64 << bit_offset;
        }
    }

    /// 获取指定位
    fn get_bit(&self, index: u64) -> bool {
        let word_index = (index / 64) as usize;
        let bit_offset = index % 64;

        if word_index < self.bits.len() {
            (self.bits[word_index] & (1u64 << bit_offset)) != 0
        } else {
            false
        }
    }

    /// 获取当前假阳性率估计
    pub fn estimated_fpp(&self) -> f64 {
        if self.n == 0 {
            return 0.0;
        }

        // p = (1 - e^(-kn/m))^k
        let exponent = -(self.k as f64) * (self.n as f64) / (self.m as f64);
        (1.0 - exponent.exp()).powi(self.k as i32)
    }

    /// 获取内存占用（字节）
    pub fn memory_usage(&self) -> usize {
        self.bits.len() * 8
    }

    /// 获取统计信息
    pub fn stats(&self) -> BloomFilterStats {
        BloomFilterStats {
            m: self.m,
            k: self.k,
            n: self.n,
            memory_bytes: self.memory_usage(),
            estimated_fpp: self.estimated_fpp(),
        }
    }
}

/// Bloom Filter 统计信息
#[derive(Debug, Clone)]
pub struct BloomFilterStats {
    pub m: u64,              // 位数组大小
    pub k: u32,              // 哈希函数数量
    pub n: u64,              // 已插入元素数
    pub memory_bytes: usize, // 内存占用
    pub estimated_fpp: f64,  // 估计假阳性率
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_basic() {
        let mut filter = BloomFilter::new(1000, 0.01);

        // 插入元素
        for i in 0..1000 {
            filter.insert(&i);
        }

        // 检查存在的元素（应该全部返回 true）
        for i in 0..1000 {
            assert!(filter.contains(&i), "Element {} should exist", i);
        }

        // 检查不存在的元素（可能有假阳性）
        let mut false_positives = 0;
        for i in 1000..10000 {
            if filter.contains(&i) {
                false_positives += 1;
            }
        }

        let actual_fpp = false_positives as f64 / 9000.0;
        println!("Actual FPP: {:.4}, Expected: 0.01", actual_fpp);

        // 实际假阳性率应该接近 1%（允许一定误差）
        assert!(actual_fpp < 0.05, "FPP too high: {}", actual_fpp);
    }

    #[test]
    fn test_bloom_filter_strings() {
        let mut filter = BloomFilter::new(100, 0.01);

        let test_keys = vec![
            "user_123",
            "user_456",
            "user_789",
            "order_abc",
            "order_def",
            "order_xyz",
        ];

        // 插入
        for key in &test_keys {
            filter.insert(key);
        }

        // 验证存在
        for key in &test_keys {
            assert!(filter.contains(key), "Key {} should exist", key);
        }

        // 验证不存在
        assert!(!filter.contains(&"user_999"));
        assert!(!filter.contains(&"order_zzz"));
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut filter = BloomFilter::new(1000, 0.01);

        for i in 0..500 {
            filter.insert(&i);
        }

        // 序列化
        let bytes = rkyv::to_bytes::<_, 16384>(&filter).unwrap();

        // 反序列化
        let archived = rkyv::check_archived_root::<BloomFilter>(&bytes).unwrap();
        let restored: BloomFilter = archived.deserialize(&mut rkyv::Infallible).unwrap();

        // 验证功能
        for i in 0..500 {
            assert!(
                restored.contains(&i),
                "Element {} should exist after restore",
                i
            );
        }

        assert_eq!(restored.n, 500);
        assert_eq!(restored.k, filter.k);
        assert_eq!(restored.m, filter.m);
    }

    #[test]
    fn test_optimal_parameters() {
        // 测试不同规模和假阳性率
        let test_cases = vec![
            (1000, 0.01),    // 1K items, 1% FPP
            (10000, 0.01),   // 10K items, 1% FPP
            (100000, 0.001), // 100K items, 0.1% FPP
        ];

        for (n, p) in test_cases {
            let filter = BloomFilter::new(n, p);
            let stats = filter.stats();

            println!(
                "n={}, p={}: m={}, k={}, memory={}KB",
                n,
                p,
                stats.m,
                stats.k,
                stats.memory_bytes / 1024
            );

            assert!(stats.k > 0);
            assert!(stats.m > 0);
        }
    }
}
