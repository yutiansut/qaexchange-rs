//! 基础增量算子 - Sum, Count, Avg, Min, Max
//!
//! @yutiansut @quantaxis
//!
//! 提供 O(1) 复杂度的基础统计增量计算：
//! - Sum: 累加求和
//! - Count: 计数器
//! - Avg: 均值 (基于 Sum/Count)
//! - Min/Max: 最值追踪 (需要窗口支持完全增量)

use super::ring_buffer::RingBuffer;
use super::IncrementalOperator;
use std::cmp::Ordering;

// ═══════════════════════════════════════════════════════════════════════════
// Sum 算子
// ═══════════════════════════════════════════════════════════════════════════

/// Sum 累加状态
#[derive(Debug, Clone, Default)]
pub struct SumState {
    pub sum: f64,
    pub count: u64,
}

/// Sum 增量算子
pub struct Sum;

impl IncrementalOperator for Sum {
    type State = SumState;
    type Input = f64;
    type Output = f64;

    fn init() -> Self::State {
        SumState::default()
    }

    fn update(state: &mut Self::State, input: Self::Input) {
        state.sum += input;
        state.count += 1;
    }

    fn value(state: &Self::State) -> Self::Output {
        state.sum
    }

    fn expire(state: &mut Self::State, expired: Self::Input) {
        state.sum -= expired;
        state.count = state.count.saturating_sub(1);
    }

    fn merge(left: Self::State, right: Self::State) -> Self::State {
        SumState {
            sum: left.sum + right.sum,
            count: left.count + right.count,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Count 算子
// ═══════════════════════════════════════════════════════════════════════════

/// Count 状态
#[derive(Debug, Clone, Default)]
pub struct CountState {
    pub count: u64,
}

/// Count 增量算子
pub struct Count;

impl IncrementalOperator for Count {
    type State = CountState;
    type Input = ();
    type Output = u64;

    fn init() -> Self::State {
        CountState::default()
    }

    fn update(state: &mut Self::State, _input: Self::Input) {
        state.count += 1;
    }

    fn value(state: &Self::State) -> Self::Output {
        state.count
    }

    fn expire(state: &mut Self::State, _expired: Self::Input) {
        state.count = state.count.saturating_sub(1);
    }

    fn merge(left: Self::State, right: Self::State) -> Self::State {
        CountState {
            count: left.count + right.count,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Avg 算子
// ═══════════════════════════════════════════════════════════════════════════

/// Avg 状态 (复用 SumState)
pub type AvgState = SumState;

/// Avg 增量算子
pub struct Avg;

impl IncrementalOperator for Avg {
    type State = AvgState;
    type Input = f64;
    type Output = f64;

    fn init() -> Self::State {
        AvgState::default()
    }

    fn update(state: &mut Self::State, input: Self::Input) {
        state.sum += input;
        state.count += 1;
    }

    fn value(state: &Self::State) -> Self::Output {
        if state.count == 0 {
            0.0
        } else {
            state.sum / state.count as f64
        }
    }

    fn expire(state: &mut Self::State, expired: Self::Input) {
        state.sum -= expired;
        state.count = state.count.saturating_sub(1);
    }

    fn merge(left: Self::State, right: Self::State) -> Self::State {
        AvgState {
            sum: left.sum + right.sum,
            count: left.count + right.count,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Min/Max 算子 (窗口版本 - 需要完整数据支持)
// ═══════════════════════════════════════════════════════════════════════════

/// Min 状态 (需要维护完整窗口数据)
#[derive(Debug, Clone)]
pub struct MinState {
    pub buffer: RingBuffer<f64>,
    pub current_min: Option<f64>,
}

impl Default for MinState {
    fn default() -> Self {
        Self {
            buffer: RingBuffer::new(1024), // 默认窗口
            current_min: None,
        }
    }
}

impl MinState {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: RingBuffer::new(capacity),
            current_min: None,
        }
    }

    fn recalculate_min(&mut self) {
        self.current_min = self.buffer.iter().copied().reduce(f64::min);
    }
}

/// Min 增量算子
pub struct Min;

impl IncrementalOperator for Min {
    type State = MinState;
    type Input = f64;
    type Output = Option<f64>;

    fn init() -> Self::State {
        MinState::default()
    }

    fn update(state: &mut Self::State, input: Self::Input) {
        let expired = state.buffer.push(input);

        // 更新最小值
        match state.current_min {
            None => state.current_min = Some(input),
            Some(min) if input < min => state.current_min = Some(input),
            Some(min) if expired == Some(min) => state.recalculate_min(),
            _ => {}
        }
    }

    fn value(state: &Self::State) -> Self::Output {
        state.current_min
    }

    fn expire(state: &mut Self::State, expired: Self::Input) {
        if state.current_min == Some(expired) {
            state.recalculate_min();
        }
    }

    fn merge(left: Self::State, right: Self::State) -> Self::State {
        let current_min = match (left.current_min, right.current_min) {
            (Some(l), Some(r)) => Some(l.min(r)),
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (None, None) => None,
        };

        // 合并 buffer 需要更复杂的逻辑，这里简化处理
        MinState {
            buffer: left.buffer, // 保留左侧 buffer
            current_min,
        }
    }
}

/// Max 状态
#[derive(Debug, Clone)]
pub struct MaxState {
    pub buffer: RingBuffer<f64>,
    pub current_max: Option<f64>,
}

impl Default for MaxState {
    fn default() -> Self {
        Self {
            buffer: RingBuffer::new(1024),
            current_max: None,
        }
    }
}

impl MaxState {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: RingBuffer::new(capacity),
            current_max: None,
        }
    }

    fn recalculate_max(&mut self) {
        self.current_max = self.buffer.iter().copied().reduce(f64::max);
    }
}

/// Max 增量算子
pub struct Max;

impl IncrementalOperator for Max {
    type State = MaxState;
    type Input = f64;
    type Output = Option<f64>;

    fn init() -> Self::State {
        MaxState::default()
    }

    fn update(state: &mut Self::State, input: Self::Input) {
        let expired = state.buffer.push(input);

        match state.current_max {
            None => state.current_max = Some(input),
            Some(max) if input > max => state.current_max = Some(input),
            Some(max) if expired == Some(max) => state.recalculate_max(),
            _ => {}
        }
    }

    fn value(state: &Self::State) -> Self::Output {
        state.current_max
    }

    fn expire(state: &mut Self::State, expired: Self::Input) {
        if state.current_max == Some(expired) {
            state.recalculate_max();
        }
    }

    fn merge(left: Self::State, right: Self::State) -> Self::State {
        let current_max = match (left.current_max, right.current_max) {
            (Some(l), Some(r)) => Some(l.max(r)),
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (None, None) => None,
        };

        MaxState {
            buffer: left.buffer,
            current_max,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 窗口化算子封装
// ═══════════════════════════════════════════════════════════════════════════

/// 窗口化 Sum
#[derive(Debug, Clone)]
pub struct WindowedSum {
    window_size: usize,
    buffer: RingBuffer<f64>,
    sum: f64,
}

impl WindowedSum {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: RingBuffer::new(window_size),
            sum: 0.0,
        }
    }

    pub fn update(&mut self, value: f64) {
        if let Some(expired) = self.buffer.push(value) {
            self.sum -= expired;
        }
        self.sum += value;
    }

    pub fn value(&self) -> f64 {
        self.sum
    }

    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
    }
}

/// 窗口化 Avg
#[derive(Debug, Clone)]
pub struct WindowedAvg {
    inner: WindowedSum,
}

impl WindowedAvg {
    pub fn new(window_size: usize) -> Self {
        Self {
            inner: WindowedSum::new(window_size),
        }
    }

    pub fn update(&mut self, value: f64) {
        self.inner.update(value);
    }

    pub fn value(&self) -> f64 {
        let count = self.inner.count();
        if count == 0 {
            0.0
        } else {
            self.inner.value() / count as f64
        }
    }

    pub fn count(&self) -> usize {
        self.inner.count()
    }

    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// 窗口化 Min
#[derive(Debug, Clone)]
pub struct WindowedMin {
    window_size: usize,
    buffer: RingBuffer<f64>,
    current_min: Option<f64>,
}

impl WindowedMin {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: RingBuffer::new(window_size),
            current_min: None,
        }
    }

    pub fn update(&mut self, value: f64) {
        let expired = self.buffer.push(value);

        match (self.current_min, expired) {
            (None, _) => self.current_min = Some(value),
            (Some(min), _) if value < min => self.current_min = Some(value),
            (Some(min), Some(exp)) if (exp - min).abs() < f64::EPSILON => {
                self.recalculate();
            }
            _ => {}
        }
    }

    fn recalculate(&mut self) {
        self.current_min = self.buffer.iter().copied().reduce(f64::min);
    }

    pub fn value(&self) -> Option<f64> {
        self.current_min
    }

    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_min = None;
    }
}

/// 窗口化 Max
#[derive(Debug, Clone)]
pub struct WindowedMax {
    window_size: usize,
    buffer: RingBuffer<f64>,
    current_max: Option<f64>,
}

impl WindowedMax {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: RingBuffer::new(window_size),
            current_max: None,
        }
    }

    pub fn update(&mut self, value: f64) {
        let expired = self.buffer.push(value);

        match (self.current_max, expired) {
            (None, _) => self.current_max = Some(value),
            (Some(max), _) if value > max => self.current_max = Some(value),
            (Some(max), Some(exp)) if (exp - max).abs() < f64::EPSILON => {
                self.recalculate();
            }
            _ => {}
        }
    }

    fn recalculate(&mut self) {
        self.current_max = self.buffer.iter().copied().reduce(f64::max);
    }

    pub fn value(&self) -> Option<f64> {
        self.current_max
    }

    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_max = None;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sum_basic() {
        let mut state = Sum::init();

        for i in 1..=5 {
            Sum::update(&mut state, i as f64);
        }

        assert_eq!(Sum::value(&state), 15.0);
        assert_eq!(state.count, 5);
    }

    #[test]
    fn test_sum_expire() {
        let mut state = Sum::init();

        Sum::update(&mut state, 10.0);
        Sum::update(&mut state, 20.0);
        Sum::update(&mut state, 30.0);

        assert_eq!(Sum::value(&state), 60.0);

        Sum::expire(&mut state, 10.0);
        assert_eq!(Sum::value(&state), 50.0);
    }

    #[test]
    fn test_sum_merge() {
        let mut state1 = Sum::init();
        let mut state2 = Sum::init();

        Sum::update(&mut state1, 10.0);
        Sum::update(&mut state1, 20.0);

        Sum::update(&mut state2, 30.0);
        Sum::update(&mut state2, 40.0);

        let merged = Sum::merge(state1, state2);
        assert_eq!(Sum::value(&merged), 100.0);
        assert_eq!(merged.count, 4);
    }

    #[test]
    fn test_count() {
        let mut state = Count::init();

        for _ in 0..10 {
            Count::update(&mut state, ());
        }

        assert_eq!(Count::value(&state), 10);

        Count::expire(&mut state, ());
        assert_eq!(Count::value(&state), 9);
    }

    #[test]
    fn test_avg() {
        let mut state = Avg::init();

        for i in 1..=5 {
            Avg::update(&mut state, i as f64);
        }

        assert!((Avg::value(&state) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_windowed_sum() {
        let mut ws = WindowedSum::new(3);

        ws.update(1.0);
        ws.update(2.0);
        ws.update(3.0);
        assert_eq!(ws.value(), 6.0);

        ws.update(4.0); // 1.0 过期
        assert_eq!(ws.value(), 9.0); // 2 + 3 + 4

        ws.update(5.0); // 2.0 过期
        assert_eq!(ws.value(), 12.0); // 3 + 4 + 5
    }

    #[test]
    fn test_windowed_avg() {
        let mut wa = WindowedAvg::new(3);

        wa.update(1.0);
        wa.update(2.0);
        wa.update(3.0);
        assert!((wa.value() - 2.0).abs() < 1e-10);

        wa.update(4.0);
        assert!((wa.value() - 3.0).abs() < 1e-10); // (2+3+4)/3
    }

    #[test]
    fn test_windowed_min() {
        let mut wm = WindowedMin::new(3);

        wm.update(3.0);
        wm.update(1.0);
        wm.update(2.0);
        assert_eq!(wm.value(), Some(1.0));

        wm.update(4.0); // 3.0 过期
        assert_eq!(wm.value(), Some(1.0)); // min still 1.0

        wm.update(5.0); // 1.0 过期
        assert_eq!(wm.value(), Some(2.0)); // min is now 2.0
    }

    #[test]
    fn test_windowed_max() {
        let mut wm = WindowedMax::new(3);

        wm.update(1.0);
        wm.update(3.0);
        wm.update(2.0);
        assert_eq!(wm.value(), Some(3.0));

        wm.update(1.0); // 1.0 过期
        assert_eq!(wm.value(), Some(3.0));

        wm.update(1.0); // 3.0 过期
        assert_eq!(wm.value(), Some(2.0));
    }
}
