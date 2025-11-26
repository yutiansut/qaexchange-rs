//! 因子计算模块
//!
//! @yutiansut @quantaxis
//!
//! 提供流批一体化的因子计算能力：
//! - 增量算子 (operators) - 实时流处理 O(1) 更新
//! - 物化视图 (view) - 维护计算结果快照
//! - 因子状态存储 (state) - 持久化因子状态
//! - 依赖DAG管理 (dag) - 拓扑排序计算
//! - 统一引擎 (engine) - 流批一体化执行引擎，集成 Polars
//! - WAL持久化 (wal_persister) - 因子数据流批存储

pub mod operators;
pub mod view;
pub mod state;
pub mod dag;
pub mod engine;
pub mod wal_persister;

pub use operators::*;
pub use view::*;
pub use state::*;
pub use dag::*;
pub use engine::*;
pub use wal_persister::*;
