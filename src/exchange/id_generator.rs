//! 交易所ID生成器
//!
//! 为每个instrument维护统一的事件序列（event sequence），保证事件顺序性

use dashmap::DashMap;
use std::sync::atomic::{AtomicI64, Ordering};

/// 交易所ID生成器
///
/// 为每个instrument维护统一的event sequence，所有事件（下单、撤单、成交）共用同一个序列：
/// 1. 形成顺序流（event stream）概念
/// 2. 严格递增，保证时间顺序
/// 3. 使用i64存储，简单高效
pub struct ExchangeIdGenerator {
    /// 事件序列计数器 (instrument_id -> AtomicI64)
    /// 所有事件（下单、撤单、成交）都用这个序列
    event_sequences: DashMap<String, AtomicI64>,
}

impl ExchangeIdGenerator {
    /// 创建新的ID生成器
    pub fn new() -> Self {
        Self {
            event_sequences: DashMap::new(),
        }
    }

    /// 生成下一个事件序列号（统一序列）
    ///
    /// # 参数
    /// - `instrument_id`: 合约ID
    ///
    /// # 返回
    /// 自增的i64序列号
    ///
    /// # 说明
    /// - 下单、撤单、成交都用同一个序列
    /// - 保证同一合约的所有事件严格有序
    pub fn next_sequence(&self, instrument_id: &str) -> i64 {
        let counter = self
            .event_sequences
            .entry(instrument_id.to_string())
            .or_insert_with(|| AtomicI64::new(0));

        // fetch_add返回旧值，所以返回值+1就是新值
        counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 获取当前事件序列号（用于测试/调试）
    pub fn current_sequence(&self, instrument_id: &str) -> i64 {
        self.event_sequences
            .get(instrument_id)
            .map(|counter| counter.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

impl Default for ExchangeIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_increment() {
        let generator = ExchangeIdGenerator::new();

        // 同一合约的事件序列应该严格递增
        let seq1 = generator.next_sequence("SHFE.cu2501");
        let seq2 = generator.next_sequence("SHFE.cu2501");
        let seq3 = generator.next_sequence("SHFE.cu2501");

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(seq3, 3);
    }

    #[test]
    fn test_different_instruments_independent() {
        let generator = ExchangeIdGenerator::new();

        // 不同合约的序列应该独立
        let cu_seq1 = generator.next_sequence("SHFE.cu2501");
        let ag_seq1 = generator.next_sequence("SHFE.ag2501");
        let cu_seq2 = generator.next_sequence("SHFE.cu2501");

        assert_eq!(cu_seq1, 1);
        assert_eq!(ag_seq1, 1);
        assert_eq!(cu_seq2, 2);
    }

    #[test]
    fn test_unified_sequence() {
        let generator = ExchangeIdGenerator::new();

        // 下单、撤单、成交都用同一个序列（模拟事件流）
        let order_seq = generator.next_sequence("SHFE.cu2501"); // 下单
        let cancel_seq = generator.next_sequence("SHFE.cu2501"); // 撤单
        let trade_seq = generator.next_sequence("SHFE.cu2501"); // 成交

        // 应该形成严格递增的事件流
        assert_eq!(order_seq, 1);
        assert_eq!(cancel_seq, 2);
        assert_eq!(trade_seq, 3);
    }

    #[test]
    fn test_concurrent_generation() {
        use std::sync::Arc;
        use std::thread;

        let generator = Arc::new(ExchangeIdGenerator::new());
        let mut handles = vec![];

        // 10个线程并发生成序列号
        for _ in 0..10 {
            let gen = generator.clone();
            let handle = thread::spawn(move || {
                let mut seqs = Vec::new();
                for _ in 0..100 {
                    seqs.push(gen.next_sequence("SHFE.cu2501"));
                }
                seqs
            });
            handles.push(handle);
        }

        // 收集所有序列号
        let mut all_seqs = Vec::new();
        for handle in handles {
            let seqs = handle.join().unwrap();
            all_seqs.extend(seqs);
        }

        // 应该有1000个序列号，且都不重复
        all_seqs.sort();
        all_seqs.dedup();
        assert_eq!(all_seqs.len(), 1000);

        // 最大值应该是1000
        assert_eq!(*all_seqs.last().unwrap(), 1000);
    }

    #[test]
    fn test_current_sequence() {
        let generator = ExchangeIdGenerator::new();

        assert_eq!(generator.current_sequence("SHFE.cu2501"), 0);

        generator.next_sequence("SHFE.cu2501");
        generator.next_sequence("SHFE.cu2501");
        generator.next_sequence("SHFE.cu2501");

        assert_eq!(generator.current_sequence("SHFE.cu2501"), 3);
    }
}
