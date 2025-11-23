//! 因子计算模块
//!
//! @yutiansut @quantaxis
//!
//! 提供增量因子计算能力，包括：
//! - 增量算子 (operators)
//! - 物化视图 (view)
//! - 因子状态存储 (state)
//! - 依赖DAG管理 (dag)

pub mod operators;
pub mod view;
pub mod state;
pub mod dag;

pub use operators::*;
pub use view::*;
pub use state::*;
pub use dag::*;
