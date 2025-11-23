//! 增量算子模块
//!
//! @yutiansut @quantaxis
//!
//! 提供各类增量计算算子：
//! - 基础算子 (sum, count, avg, min, max)
//! - 滚动窗口算子 (rolling_mean, rolling_std, rolling_corr)
//! - Welford算法 (数值稳定的方差计算)
//! - 环形缓冲区 (滑动窗口数据结构)

pub mod ring_buffer;
pub mod welford;
pub mod basic;
pub mod rolling;

pub use ring_buffer::*;
pub use welford::*;
pub use basic::*;
pub use rolling::*;

/// 增量算子核心 Trait
///
/// 设计原则:
/// 1. 状态封装: 每个算子管理自己的增量状态
/// 2. 零拷贝: 状态更新不产生内存分配
/// 3. 类型安全: 编译期确保状态类型正确
pub trait IncrementalOperator: Send + Sync {
    /// 算子状态类型
    type State: Default + Clone + Send + Sync;

    /// 输入类型
    type Input;

    /// 输出类型
    type Output;

    /// 创建初始状态
    fn init() -> Self::State;

    /// 增量更新 (核心 - O(1)复杂度)
    fn update(state: &mut Self::State, input: Self::Input);

    /// 获取当前值
    fn value(state: &Self::State) -> Self::Output;

    /// 窗口过期处理 (滑动窗口专用)
    fn expire(_state: &mut Self::State, _expired: Self::Input) {
        // 默认空实现，非窗口算子不需要
    }

    /// 合并两个状态 (分布式聚合专用)
    fn merge(left: Self::State, right: Self::State) -> Self::State;

    /// 重置状态
    fn reset(state: &mut Self::State) {
        *state = Self::init();
    }
}

/// 窗口增量算子 Trait
pub trait WindowedOperator: IncrementalOperator {
    /// 获取窗口大小
    fn window_size(&self) -> usize;

    /// 是否已满
    fn is_full(state: &Self::State) -> bool;

    /// 当前数据点数量
    fn count(state: &Self::State) -> usize;
}
