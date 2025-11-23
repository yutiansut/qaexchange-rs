//! Welford 算法 - 数值稳定的在线统计计算
//!
//! @yutiansut @quantaxis
//!
//! Welford 算法提供数值稳定的增量统计计算：
//! - 避免大数相减导致的精度损失
//! - O(1) 时间复杂度更新
//! - 支持均值、方差、标准差、偏度、峰度

use super::ring_buffer::RingBuffer;

/// Welford 单变量统计状态
#[derive(Debug, Clone, Default)]
pub struct WelfordState {
    /// 数据点数量
    pub count: u64,
    /// 均值
    pub mean: f64,
    /// M2 = Σ(x - mean)²
    pub m2: f64,
    /// M3 = Σ(x - mean)³ (用于偏度)
    pub m3: f64,
    /// M4 = Σ(x - mean)⁴ (用于峰度)
    pub m4: f64,
}

impl WelfordState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 增量更新
    pub fn update(&mut self, x: f64) {
        self.count += 1;
        let n = self.count as f64;

        let delta = x - self.mean;
        let delta_n = delta / n;
        let delta_n2 = delta_n * delta_n;
        let term1 = delta * delta_n * (n - 1.0);

        // 更新均值
        self.mean += delta_n;

        // 更新高阶矩 (按正确顺序)
        self.m4 += term1 * delta_n2 * (n * n - 3.0 * n + 3.0)
            + 6.0 * delta_n2 * self.m2
            - 4.0 * delta_n * self.m3;
        self.m3 += term1 * delta_n * (n - 2.0) - 3.0 * delta_n * self.m2;
        self.m2 += term1;
    }

    /// 方差 (总体)
    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / self.count as f64
        }
    }

    /// 方差 (样本)
    pub fn sample_variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / (self.count - 1) as f64
        }
    }

    /// 标准差 (总体)
    pub fn std(&self) -> f64 {
        self.variance().sqrt()
    }

    /// 标准差 (样本)
    pub fn sample_std(&self) -> f64 {
        self.sample_variance().sqrt()
    }

    /// 偏度 (Skewness)
    pub fn skewness(&self) -> f64 {
        if self.count < 3 || self.m2 == 0.0 {
            0.0
        } else {
            let n = self.count as f64;
            (n.sqrt() * self.m3) / self.m2.powf(1.5)
        }
    }

    /// 峰度 (Kurtosis, excess kurtosis)
    pub fn kurtosis(&self) -> f64 {
        if self.count < 4 || self.m2 == 0.0 {
            0.0
        } else {
            let n = self.count as f64;
            (n * self.m4) / (self.m2 * self.m2) - 3.0
        }
    }

    /// 合并两个状态 (并行聚合)
    pub fn merge(&self, other: &WelfordState) -> WelfordState {
        if self.count == 0 {
            return other.clone();
        }
        if other.count == 0 {
            return self.clone();
        }

        let combined_count = self.count + other.count;
        let delta = other.mean - self.mean;
        let delta2 = delta * delta;
        let delta3 = delta * delta2;
        let delta4 = delta2 * delta2;

        let n1 = self.count as f64;
        let n2 = other.count as f64;
        let n = combined_count as f64;

        let combined_mean = (self.mean * n1 + other.mean * n2) / n;

        let combined_m2 = self.m2 + other.m2 + delta2 * n1 * n2 / n;

        let combined_m3 = self.m3 + other.m3
            + delta3 * n1 * n2 * (n1 - n2) / (n * n)
            + 3.0 * delta * (n1 * other.m2 - n2 * self.m2) / n;

        let combined_m4 = self.m4 + other.m4
            + delta4 * n1 * n2 * (n1 * n1 - n1 * n2 + n2 * n2) / (n * n * n)
            + 6.0 * delta2 * (n1 * n1 * other.m2 + n2 * n2 * self.m2) / (n * n)
            + 4.0 * delta * (n1 * other.m3 - n2 * self.m3) / n;

        WelfordState {
            count: combined_count,
            mean: combined_mean,
            m2: combined_m2,
            m3: combined_m3,
            m4: combined_m4,
        }
    }

    /// 重置状态
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// 滑动窗口 Welford 状态
#[derive(Debug, Clone)]
pub struct WindowedWelfordState {
    /// 窗口大小
    window_size: usize,
    /// 数据缓冲区
    buffer: RingBuffer<f64>,
    /// 当前统计状态
    state: WelfordState,
}

impl WindowedWelfordState {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: RingBuffer::new(window_size),
            state: WelfordState::new(),
        }
    }

    /// 增量更新，自动处理窗口过期
    pub fn update(&mut self, x: f64) {
        if self.buffer.is_full() {
            // 窗口已满，需要重新计算（Welford 不直接支持删除）
            let expired = self.buffer.push(x);
            if expired.is_some() {
                // 重新计算整个窗口
                self.recalculate();
            }
        } else {
            self.buffer.push(x);
            self.state.update(x);
        }
    }

    /// 重新计算整个窗口的统计量
    fn recalculate(&mut self) {
        self.state = WelfordState::new();
        for &val in self.buffer.iter() {
            self.state.update(val);
        }
    }

    /// 获取当前均值
    pub fn mean(&self) -> f64 {
        self.state.mean
    }

    /// 获取当前方差
    pub fn variance(&self) -> f64 {
        self.state.variance()
    }

    /// 获取当前标准差
    pub fn std(&self) -> f64 {
        self.state.std()
    }

    /// 获取偏度
    pub fn skewness(&self) -> f64 {
        self.state.skewness()
    }

    /// 获取峰度
    pub fn kurtosis(&self) -> f64 {
        self.state.kurtosis()
    }

    /// 数据点数量
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    /// 窗口大小
    pub fn window_size(&self) -> usize {
        self.window_size
    }

    /// 重置
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state.reset();
    }
}

impl Default for WindowedWelfordState {
    fn default() -> Self {
        Self::new(20)
    }
}

/// 双变量 Welford (协方差/相关性)
#[derive(Debug, Clone, Default)]
pub struct WelfordCovarianceState {
    pub count: u64,
    pub mean_x: f64,
    pub mean_y: f64,
    pub m2_x: f64,
    pub m2_y: f64,
    pub c: f64, // co-moment
}

impl WelfordCovarianceState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 增量更新
    pub fn update(&mut self, x: f64, y: f64) {
        self.count += 1;
        let n = self.count as f64;

        let dx = x - self.mean_x;
        let dy = y - self.mean_y;

        self.mean_x += dx / n;
        self.mean_y += dy / n;

        let dx2 = x - self.mean_x;
        let dy2 = y - self.mean_y;

        self.m2_x += dx * dx2;
        self.m2_y += dy * dy2;
        self.c += dx * dy2;
    }

    /// 协方差 (总体)
    pub fn covariance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.c / self.count as f64
        }
    }

    /// 协方差 (样本)
    pub fn sample_covariance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.c / (self.count - 1) as f64
        }
    }

    /// 相关系数
    pub fn correlation(&self) -> f64 {
        if self.count < 2 || self.m2_x <= 0.0 || self.m2_y <= 0.0 {
            0.0
        } else {
            self.c / (self.m2_x.sqrt() * self.m2_y.sqrt())
        }
    }

    /// X 的标准差
    pub fn std_x(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            (self.m2_x / self.count as f64).sqrt()
        }
    }

    /// Y 的标准差
    pub fn std_y(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            (self.m2_y / self.count as f64).sqrt()
        }
    }

    /// 合并两个状态
    pub fn merge(&self, other: &Self) -> Self {
        if self.count == 0 {
            return other.clone();
        }
        if other.count == 0 {
            return self.clone();
        }

        let n1 = self.count as f64;
        let n2 = other.count as f64;
        let n = n1 + n2;

        let dx = other.mean_x - self.mean_x;
        let dy = other.mean_y - self.mean_y;

        Self {
            count: self.count + other.count,
            mean_x: (self.mean_x * n1 + other.mean_x * n2) / n,
            mean_y: (self.mean_y * n1 + other.mean_y * n2) / n,
            m2_x: self.m2_x + other.m2_x + dx * dx * n1 * n2 / n,
            m2_y: self.m2_y + other.m2_y + dy * dy * n1 * n2 / n,
            c: self.c + other.c + dx * dy * n1 * n2 / n,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welford_basic() {
        let mut state = WelfordState::new();

        // 简单序列 [1, 2, 3, 4, 5]
        for i in 1..=5 {
            state.update(i as f64);
        }

        assert_eq!(state.count, 5);
        assert!((state.mean - 3.0).abs() < 1e-10);

        // 方差 = 2.0 (总体), 2.5 (样本)
        assert!((state.variance() - 2.0).abs() < 1e-10);
        assert!((state.sample_variance() - 2.5).abs() < 1e-10);
    }

    #[test]
    fn test_welford_std() {
        let mut state = WelfordState::new();

        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        for &x in &data {
            state.update(x);
        }

        // 均值 = 5.0
        assert!((state.mean - 5.0).abs() < 1e-10);

        // 总体标准差 = 2.0
        assert!((state.std() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_welford_merge() {
        let mut state1 = WelfordState::new();
        let mut state2 = WelfordState::new();
        let mut combined = WelfordState::new();

        let data1 = [1.0, 2.0, 3.0];
        let data2 = [4.0, 5.0, 6.0];

        for &x in &data1 {
            state1.update(x);
            combined.update(x);
        }
        for &x in &data2 {
            state2.update(x);
            combined.update(x);
        }

        let merged = state1.merge(&state2);

        assert!((merged.mean - combined.mean).abs() < 1e-10);
        assert!((merged.variance() - combined.variance()).abs() < 1e-10);
    }

    #[test]
    fn test_windowed_welford() {
        let mut state = WindowedWelfordState::new(3);

        state.update(1.0);
        state.update(2.0);
        state.update(3.0);

        assert_eq!(state.count(), 3);
        assert!((state.mean() - 2.0).abs() < 1e-10);

        // 添加新值，窗口滑动
        state.update(4.0);
        assert_eq!(state.count(), 3);
        assert!((state.mean() - 3.0).abs() < 1e-10); // [2, 3, 4]
    }

    #[test]
    fn test_welford_covariance() {
        let mut state = WelfordCovarianceState::new();

        // 完美正相关
        for i in 1..=5 {
            state.update(i as f64, i as f64 * 2.0);
        }

        let corr = state.correlation();
        assert!(
            (corr - 1.0).abs() < 1e-10,
            "Expected correlation ~1.0, got {}",
            corr
        );
    }

    #[test]
    fn test_welford_negative_correlation() {
        let mut state = WelfordCovarianceState::new();

        // 负相关
        state.update(1.0, 5.0);
        state.update(2.0, 4.0);
        state.update(3.0, 3.0);
        state.update(4.0, 2.0);
        state.update(5.0, 1.0);

        let corr = state.correlation();
        assert!(
            (corr + 1.0).abs() < 1e-10,
            "Expected correlation ~-1.0, got {}",
            corr
        );
    }
}
