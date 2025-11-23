//! 环形缓冲区 - 滑动窗口核心数据结构
//!
//! @yutiansut @quantaxis
//!
//! 高性能环形缓冲区，用于滑动窗口计算：
//! - O(1) 插入和过期
//! - 固定内存分配
//! - 支持泛型元素

use std::collections::VecDeque;

/// 泛型环形缓冲区
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    /// 内部存储
    buffer: VecDeque<T>,
    /// 容量
    capacity: usize,
}

impl<T: Clone> RingBuffer<T> {
    /// 创建指定容量的环形缓冲区
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// 推入新元素，如果已满则弹出最旧的元素
    /// 返回被弹出的元素（如果有）
    pub fn push(&mut self, value: T) -> Option<T> {
        let expired = if self.buffer.len() >= self.capacity {
            self.buffer.pop_front()
        } else {
            None
        };
        self.buffer.push_back(value);
        expired
    }

    /// 获取最新元素
    pub fn latest(&self) -> Option<&T> {
        self.buffer.back()
    }

    /// 获取最旧元素
    pub fn oldest(&self) -> Option<&T> {
        self.buffer.front()
    }

    /// 获取指定位置的元素（0 = 最旧）
    pub fn get(&self, index: usize) -> Option<&T> {
        self.buffer.get(index)
    }

    /// 获取倒数第 n 个元素（0 = 最新）
    pub fn get_from_back(&self, n: usize) -> Option<&T> {
        if n >= self.buffer.len() {
            None
        } else {
            self.buffer.get(self.buffer.len() - 1 - n)
        }
    }

    /// 当前元素数量
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.capacity
    }

    /// 容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// 迭代器（从旧到新）
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter()
    }

    /// 反向迭代器（从新到旧）
    pub fn iter_rev(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter().rev()
    }

    /// 转换为 Vec
    pub fn to_vec(&self) -> Vec<T> {
        self.buffer.iter().cloned().collect()
    }
}

impl<T: Clone> Default for RingBuffer<T> {
    fn default() -> Self {
        Self::new(64) // 默认容量
    }
}

/// f64 特化的环形缓冲区，带统计功能
#[derive(Debug, Clone)]
pub struct NumericRingBuffer {
    buffer: RingBuffer<f64>,
    /// 当前和（用于快速计算均值）
    sum: f64,
}

impl NumericRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: RingBuffer::new(capacity),
            sum: 0.0,
        }
    }

    /// 推入新值，返回被弹出的值
    pub fn push(&mut self, value: f64) -> Option<f64> {
        let expired = self.buffer.push(value);
        self.sum += value;
        if let Some(exp) = expired {
            self.sum -= exp;
        }
        expired
    }

    /// 当前和
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// 当前均值
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() {
            0.0
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// 元素数量
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    /// 容量
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// 获取最新值
    pub fn latest(&self) -> Option<f64> {
        self.buffer.latest().copied()
    }

    /// 获取最旧值
    pub fn oldest(&self) -> Option<f64> {
        self.buffer.oldest().copied()
    }

    /// 获取所有值
    pub fn values(&self) -> Vec<f64> {
        self.buffer.to_vec()
    }

    /// 清空
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
    }

    /// 迭代器
    pub fn iter(&self) -> impl Iterator<Item = &f64> {
        self.buffer.iter()
    }
}

impl Default for NumericRingBuffer {
    fn default() -> Self {
        Self::new(64)
    }
}

/// 双数值环形缓冲区，用于协方差/相关性计算
#[derive(Debug, Clone)]
pub struct PairedRingBuffer {
    x_buffer: RingBuffer<f64>,
    y_buffer: RingBuffer<f64>,
    sum_x: f64,
    sum_y: f64,
    sum_xy: f64,
    sum_x2: f64,
    sum_y2: f64,
}

impl PairedRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            x_buffer: RingBuffer::new(capacity),
            y_buffer: RingBuffer::new(capacity),
            sum_x: 0.0,
            sum_y: 0.0,
            sum_xy: 0.0,
            sum_x2: 0.0,
            sum_y2: 0.0,
        }
    }

    /// 推入一对值
    pub fn push(&mut self, x: f64, y: f64) -> Option<(f64, f64)> {
        let exp_x = self.x_buffer.push(x);
        let exp_y = self.y_buffer.push(y);

        // 更新统计量
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xy += x * y;
        self.sum_x2 += x * x;
        self.sum_y2 += y * y;

        // 减去过期值
        if let (Some(ex), Some(ey)) = (exp_x, exp_y) {
            self.sum_x -= ex;
            self.sum_y -= ey;
            self.sum_xy -= ex * ey;
            self.sum_x2 -= ex * ex;
            self.sum_y2 -= ey * ey;
            Some((ex, ey))
        } else {
            None
        }
    }

    /// 计算协方差
    pub fn covariance(&self) -> f64 {
        let n = self.len() as f64;
        if n < 2.0 {
            return 0.0;
        }
        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;
        (self.sum_xy / n) - (mean_x * mean_y)
    }

    /// 计算相关系数
    pub fn correlation(&self) -> f64 {
        let n = self.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;
        let var_x = (self.sum_x2 / n) - (mean_x * mean_x);
        let var_y = (self.sum_y2 / n) - (mean_y * mean_y);

        if var_x <= 0.0 || var_y <= 0.0 {
            return 0.0;
        }

        let cov = (self.sum_xy / n) - (mean_x * mean_y);
        cov / (var_x.sqrt() * var_y.sqrt())
    }

    pub fn len(&self) -> usize {
        self.x_buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x_buffer.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.x_buffer.is_full()
    }

    pub fn capacity(&self) -> usize {
        self.x_buffer.capacity()
    }

    pub fn clear(&mut self) {
        self.x_buffer.clear();
        self.y_buffer.clear();
        self.sum_x = 0.0;
        self.sum_y = 0.0;
        self.sum_xy = 0.0;
        self.sum_x2 = 0.0;
        self.sum_y2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let mut buf: RingBuffer<i32> = RingBuffer::new(3);

        assert!(buf.is_empty());
        assert!(!buf.is_full());

        buf.push(1);
        buf.push(2);
        buf.push(3);

        assert!(buf.is_full());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.oldest(), Some(&1));
        assert_eq!(buf.latest(), Some(&3));

        // 溢出
        let expired = buf.push(4);
        assert_eq!(expired, Some(1));
        assert_eq!(buf.oldest(), Some(&2));
        assert_eq!(buf.latest(), Some(&4));
    }

    #[test]
    fn test_numeric_ring_buffer() {
        let mut buf = NumericRingBuffer::new(3);

        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);

        assert_eq!(buf.sum(), 6.0);
        assert_eq!(buf.mean(), 2.0);

        buf.push(4.0); // 1.0 被弹出
        assert_eq!(buf.sum(), 9.0);
        assert_eq!(buf.mean(), 3.0);
    }

    #[test]
    fn test_paired_ring_buffer_correlation() {
        let mut buf = PairedRingBuffer::new(5);

        // 完美正相关
        for i in 1..=5 {
            buf.push(i as f64, i as f64 * 2.0);
        }

        let corr = buf.correlation();
        assert!((corr - 1.0).abs() < 0.0001, "Expected ~1.0, got {}", corr);
    }

    #[test]
    fn test_paired_ring_buffer_negative_correlation() {
        let mut buf = PairedRingBuffer::new(5);

        // 负相关
        buf.push(1.0, 5.0);
        buf.push(2.0, 4.0);
        buf.push(3.0, 3.0);
        buf.push(4.0, 2.0);
        buf.push(5.0, 1.0);

        let corr = buf.correlation();
        assert!((corr + 1.0).abs() < 0.0001, "Expected ~-1.0, got {}", corr);
    }
}
