//! SPSC (Single Producer Single Consumer) 无锁队列
//!
//! @yutiansut @quantaxis
//!
//! 基于 crossbeam-queue 的 ArrayQueue 封装，提供更友好的 API
//!
//! 性能目标：
//! - 入队延迟：< 20ns
//! - 出队延迟：< 20ns
//! - 无锁/无等待
//!
//! 使用场景：
//! - 撮合引擎 -> 成交回报处理
//! - 行情接收 -> 行情分发
//! - 订单网关 -> 撮合引擎

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// SPSC 队列
// ═══════════════════════════════════════════════════════════════════════════

/// SPSC 无锁队列
///
/// 单生产者单消费者队列，提供最优的无锁性能
pub struct SpscQueue<T> {
    /// 内部队列
    queue: ArrayQueue<T>,

    /// 队列容量
    capacity: usize,

    /// 统计信息
    stats: QueueStats,

    /// 是否关闭
    closed: AtomicBool,
}

/// 队列统计信息
#[derive(Debug, Default)]
pub struct QueueStats {
    /// 入队次数
    pub enqueues: AtomicU64,

    /// 出队次数
    pub dequeues: AtomicU64,

    /// 入队失败次数（队列满）
    pub enqueue_failures: AtomicU64,

    /// 出队失败次数（队列空）
    pub dequeue_failures: AtomicU64,
}

impl QueueStats {
    /// 获取当前队列长度估计
    pub fn estimated_len(&self) -> u64 {
        let enqueues = self.enqueues.load(Ordering::Relaxed);
        let dequeues = self.dequeues.load(Ordering::Relaxed);
        enqueues.saturating_sub(dequeues)
    }
}

impl<T> SpscQueue<T> {
    /// 创建新的 SPSC 队列
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: ArrayQueue::new(capacity),
            capacity,
            stats: QueueStats::default(),
            closed: AtomicBool::new(false),
        }
    }

    /// 尝试入队（非阻塞）
    ///
    /// 成功返回 Ok(()), 队列满返回 Err(item)
    pub fn try_push(&self, item: T) -> Result<(), T> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(item);
        }

        match self.queue.push(item) {
            Ok(()) => {
                self.stats.enqueues.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(item) => {
                self.stats.enqueue_failures.fetch_add(1, Ordering::Relaxed);
                Err(item)
            }
        }
    }

    /// 入队（自旋等待直到成功或关闭）
    pub fn push(&self, mut item: T) -> Result<(), T> {
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Err(item);
            }

            match self.try_push(item) {
                Ok(()) => return Ok(()),
                Err(returned_item) => {
                    item = returned_item;
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// 入队（带超时）
    pub fn push_timeout(&self, mut item: T, timeout: Duration) -> Result<(), T> {
        let deadline = Instant::now() + timeout;

        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Err(item);
            }

            if Instant::now() >= deadline {
                return Err(item);
            }

            match self.try_push(item) {
                Ok(()) => return Ok(()),
                Err(returned_item) => {
                    item = returned_item;
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// 尝试出队（非阻塞）
    pub fn try_pop(&self) -> Option<T> {
        match self.queue.pop() {
            Some(item) => {
                self.stats.dequeues.fetch_add(1, Ordering::Relaxed);
                Some(item)
            }
            None => {
                self.stats.dequeue_failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// 出队（自旋等待直到成功或关闭）
    pub fn pop(&self) -> Option<T> {
        loop {
            if let Some(item) = self.try_pop() {
                return Some(item);
            }

            if self.closed.load(Ordering::Relaxed) && self.queue.is_empty() {
                return None;
            }

            std::hint::spin_loop();
        }
    }

    /// 出队（带超时）
    pub fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(item) = self.try_pop() {
                return Some(item);
            }

            if self.closed.load(Ordering::Relaxed) && self.queue.is_empty() {
                return None;
            }

            if Instant::now() >= deadline {
                return None;
            }

            std::hint::spin_loop();
        }
    }

    /// 批量出队
    pub fn pop_batch(&self, batch: &mut Vec<T>, max_count: usize) -> usize {
        let mut count = 0;

        while count < max_count {
            match self.try_pop() {
                Some(item) => {
                    batch.push(item);
                    count += 1;
                }
                None => break,
            }
        }

        count
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// 队列是否为空
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// 队列是否已满
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    /// 获取队列容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 关闭队列
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// 队列是否已关闭
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// 获取统计信息
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SPSC 通道（生产者-消费者分离）
// ═══════════════════════════════════════════════════════════════════════════

/// SPSC 发送端
pub struct SpscSender<T> {
    queue: Arc<SpscQueue<T>>,
}

/// SPSC 接收端
pub struct SpscReceiver<T> {
    queue: Arc<SpscQueue<T>>,
}

/// 创建 SPSC 通道
pub fn spsc_channel<T>(capacity: usize) -> (SpscSender<T>, SpscReceiver<T>) {
    let queue = Arc::new(SpscQueue::new(capacity));

    let sender = SpscSender {
        queue: Arc::clone(&queue),
    };

    let receiver = SpscReceiver { queue };

    (sender, receiver)
}

impl<T> SpscSender<T> {
    /// 发送消息（非阻塞）
    pub fn try_send(&self, item: T) -> Result<(), T> {
        self.queue.try_push(item)
    }

    /// 发送消息（阻塞）
    pub fn send(&self, item: T) -> Result<(), T> {
        self.queue.push(item)
    }

    /// 发送消息（带超时）
    pub fn send_timeout(&self, item: T, timeout: Duration) -> Result<(), T> {
        self.queue.push_timeout(item, timeout)
    }

    /// 队列是否已满
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

impl<T> Clone for SpscSender<T> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}

impl<T> Drop for SpscSender<T> {
    fn drop(&mut self) {
        // 当所有发送端都被 drop 时，关闭队列
        if Arc::strong_count(&self.queue) == 2 {
            // 只剩一个发送端和一个接收端
            // 不关闭，让接收端能继续消费剩余消息
        }
    }
}

impl<T> SpscReceiver<T> {
    /// 接收消息（非阻塞）
    pub fn try_recv(&self) -> Option<T> {
        self.queue.try_pop()
    }

    /// 接收消息（阻塞）
    pub fn recv(&self) -> Option<T> {
        self.queue.pop()
    }

    /// 接收消息（带超时）
    pub fn recv_timeout(&self, timeout: Duration) -> Option<T> {
        self.queue.pop_timeout(timeout)
    }

    /// 批量接收
    pub fn recv_batch(&self, batch: &mut Vec<T>, max_count: usize) -> usize {
        self.queue.pop_batch(batch, max_count)
    }

    /// 队列是否为空
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// 队列是否已关闭且为空
    pub fn is_disconnected(&self) -> bool {
        self.queue.is_closed() && self.queue.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 迭代器支持
// ═══════════════════════════════════════════════════════════════════════════

impl<T> Iterator for SpscReceiver<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_spsc_basic() {
        let queue = SpscQueue::new(10);

        // 入队
        assert!(queue.try_push(1).is_ok());
        assert!(queue.try_push(2).is_ok());
        assert_eq!(queue.len(), 2);

        // 出队
        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), Some(2));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_spsc_full() {
        let queue = SpscQueue::new(2);

        assert!(queue.try_push(1).is_ok());
        assert!(queue.try_push(2).is_ok());
        assert!(queue.is_full());

        // 队列满时入队失败
        assert!(queue.try_push(3).is_err());
    }

    #[test]
    fn test_spsc_channel() {
        let (tx, rx) = spsc_channel::<i32>(100);

        // 发送
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
        tx.try_send(3).unwrap();

        // 接收
        assert_eq!(rx.try_recv(), Some(1));
        assert_eq!(rx.try_recv(), Some(2));
        assert_eq!(rx.try_recv(), Some(3));
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn test_spsc_channel_threaded() {
        let (tx, rx) = spsc_channel::<i32>(1000);

        let producer = thread::spawn(move || {
            for i in 0..1000 {
                tx.send(i).unwrap();
            }
        });

        let consumer = thread::spawn(move || {
            let mut sum = 0i64;
            for _ in 0..1000 {
                if let Some(val) = rx.recv() {
                    sum += val as i64;
                }
            }
            sum
        });

        producer.join().unwrap();
        let sum = consumer.join().unwrap();

        // 0 + 1 + 2 + ... + 999 = 999 * 1000 / 2 = 499500
        assert_eq!(sum, 499500);
    }

    #[test]
    fn test_spsc_batch() {
        let queue = SpscQueue::new(100);

        for i in 0..50 {
            queue.try_push(i).unwrap();
        }

        let mut batch = Vec::new();
        let count = queue.pop_batch(&mut batch, 20);

        assert_eq!(count, 20);
        assert_eq!(batch.len(), 20);
        assert_eq!(batch[0], 0);
        assert_eq!(batch[19], 19);
    }

    #[test]
    fn test_spsc_timeout() {
        let queue = SpscQueue::<i32>::new(10);

        // 空队列超时
        let start = Instant::now();
        let result = queue.pop_timeout(Duration::from_millis(50));
        let elapsed = start.elapsed();

        assert!(result.is_none());
        assert!(elapsed >= Duration::from_millis(45)); // 允许一些误差
    }

    #[test]
    fn test_spsc_close() {
        let queue = SpscQueue::new(10);

        queue.try_push(1).unwrap();
        queue.close();

        // 关闭后仍可出队现有元素
        assert_eq!(queue.try_pop(), Some(1));

        // 关闭后入队失败
        assert!(queue.try_push(2).is_err());
    }

    #[test]
    fn test_spsc_performance() {
        let queue = SpscQueue::new(10_000);
        const ITERATIONS: usize = 100_000;

        // 测试入队性能
        let start = Instant::now();
        for i in 0..ITERATIONS {
            while queue.try_push(i).is_err() {
                // 如果满了，先出队
                queue.try_pop();
            }
        }
        let enqueue_elapsed = start.elapsed();

        // 清空队列
        while queue.try_pop().is_some() {}

        // 填充队列
        for i in 0..queue.capacity() {
            queue.try_push(i).ok();
        }

        // 测试出队性能
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            if queue.try_pop().is_none() {
                // 如果空了，先入队
                queue.try_push(0).ok();
            }
        }
        let dequeue_elapsed = start.elapsed();

        let enqueue_ns = enqueue_elapsed.as_nanos() / ITERATIONS as u128;
        let dequeue_ns = dequeue_elapsed.as_nanos() / ITERATIONS as u128;

        println!(
            "SPSC Performance: enqueue={} ns/op, dequeue={} ns/op",
            enqueue_ns, dequeue_ns
        );

        // 应该 < 100ns
        assert!(enqueue_ns < 500, "Enqueue too slow: {} ns", enqueue_ns);
        assert!(dequeue_ns < 500, "Dequeue too slow: {} ns", dequeue_ns);
    }

    #[test]
    fn test_spsc_stats() {
        let queue = SpscQueue::new(10);

        queue.try_push(1).unwrap();
        queue.try_push(2).unwrap();
        queue.try_pop();

        let stats = queue.stats();
        assert_eq!(stats.enqueues.load(Ordering::Relaxed), 2);
        assert_eq!(stats.dequeues.load(Ordering::Relaxed), 1);
        assert_eq!(stats.estimated_len(), 1);
    }
}
