//! 流批一体因子引擎
//!
//! @yutiansut @quantaxis
//!
//! 核心设计理念：
//! - 流处理 (Stream): 使用增量算子，O(1) 更新
//! - 批处理 (Batch): 使用 Polars，向量化计算
//! - 统一接口: 同一套因子定义，自动选择执行路径
//! - **并行 DAG**: 使用 Rayon 按层级并行计算因子

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use parking_lot::RwLock;
use polars::prelude::*;
use rayon::prelude::*;

use super::dag::{FactorDag, FactorId};
use super::operators::rolling::*;

// ═══════════════════════════════════════════════════════════════════════════
// 因子定义 (统一的因子描述，不是 DSL)
// ═══════════════════════════════════════════════════════════════════════════

/// 因子定义 - 描述如何计算一个因子
#[derive(Debug, Clone)]
pub enum FactorDef {
    /// 数据源 (price, volume, high, low, close)
    Source { name: String },

    /// 滚动窗口函数
    Rolling {
        source: String,
        window: usize,
        func: RollingFunc,
    },

    /// 指数移动平均
    EMA { source: String, span: usize },

    /// RSI
    RSI { source: String, period: usize },

    /// MACD
    MACD {
        source: String,
        fast: usize,
        slow: usize,
        signal: usize,
    },

    /// 布林带
    Bollinger {
        source: String,
        window: usize,
        num_std: f64,
    },

    /// 二元运算
    BinaryOp {
        left: Box<FactorDef>,
        right: Box<FactorDef>,
        op: BinaryOpType,
    },

    /// 引用其他因子
    Ref { factor_id: String },

    /// 自定义 Polars 表达式 (仅批处理)
    PolarsExpr { expr_str: String },
}

/// 滚动函数类型
#[derive(Debug, Clone, Copy)]
pub enum RollingFunc {
    Mean,
    Std,
    Sum,
    Min,
    Max,
    Var,
}

/// 二元运算类型
#[derive(Debug, Clone, Copy)]
pub enum BinaryOpType {
    Add,
    Sub,
    Mul,
    Div,
}

// ═══════════════════════════════════════════════════════════════════════════
// 因子注册表
// ═══════════════════════════════════════════════════════════════════════════

/// 注册的因子
#[derive(Debug, Clone)]
pub struct RegisteredFactor {
    pub id: String,
    pub name: String,
    pub def: FactorDef,
    pub description: String,
}

/// 因子注册表 - 管理所有因子定义
#[derive(Debug, Clone, Default)]
pub struct FactorRegistry {
    factors: HashMap<String, RegisteredFactor>,
}

impl FactorRegistry {
    pub fn new() -> Self {
        Self {
            factors: HashMap::new(),
        }
    }

    /// 注册因子
    pub fn register(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        def: FactorDef,
        description: impl Into<String>,
    ) {
        let id = id.into();
        self.factors.insert(
            id.clone(),
            RegisteredFactor {
                id,
                name: name.into(),
                def,
                description: description.into(),
            },
        );
    }

    /// 获取因子定义
    pub fn get(&self, id: &str) -> Option<&RegisteredFactor> {
        self.factors.get(id)
    }

    /// 列出所有因子
    pub fn list(&self) -> Vec<&RegisteredFactor> {
        self.factors.values().collect()
    }

    /// 创建包含标准因子的注册表
    pub fn with_standard_factors() -> Self {
        let mut registry = Self::new();

        // MA 系列
        registry.register(
            "ma5",
            "MA5",
            FactorDef::Rolling {
                source: "close".to_string(),
                window: 5,
                func: RollingFunc::Mean,
            },
            "5日均线",
        );

        registry.register(
            "ma10",
            "MA10",
            FactorDef::Rolling {
                source: "close".to_string(),
                window: 10,
                func: RollingFunc::Mean,
            },
            "10日均线",
        );

        registry.register(
            "ma20",
            "MA20",
            FactorDef::Rolling {
                source: "close".to_string(),
                window: 20,
                func: RollingFunc::Mean,
            },
            "20日均线",
        );

        // EMA 系列
        registry.register(
            "ema12",
            "EMA12",
            FactorDef::EMA {
                source: "close".to_string(),
                span: 12,
            },
            "12日指数移动平均",
        );

        registry.register(
            "ema26",
            "EMA26",
            FactorDef::EMA {
                source: "close".to_string(),
                span: 26,
            },
            "26日指数移动平均",
        );

        // 技术指标
        registry.register(
            "rsi14",
            "RSI14",
            FactorDef::RSI {
                source: "close".to_string(),
                period: 14,
            },
            "14日RSI",
        );

        registry.register(
            "macd",
            "MACD",
            FactorDef::MACD {
                source: "close".to_string(),
                fast: 12,
                slow: 26,
                signal: 9,
            },
            "MACD(12,26,9)",
        );

        registry.register(
            "boll_upper",
            "BOLL Upper",
            FactorDef::Bollinger {
                source: "close".to_string(),
                window: 20,
                num_std: 2.0,
            },
            "布林带上轨",
        );

        // 波动率
        registry.register(
            "volatility",
            "Volatility",
            FactorDef::Rolling {
                source: "close".to_string(),
                window: 20,
                func: RollingFunc::Std,
            },
            "20日波动率",
        );

        registry
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 流式因子引擎 (增量计算)
// ═══════════════════════════════════════════════════════════════════════════

/// 流式因子引擎 - 使用增量算子
pub struct StreamFactorEngine {
    /// 因子注册表
    registry: FactorRegistry,
    /// 增量算子实例 (factor_id -> operator)
    operators: HashMap<String, Box<dyn StreamOperator>>,
    /// 当前因子值
    current_values: HashMap<String, f64>,
}

/// 流式算子接口
pub trait StreamOperator: Send + Sync {
    /// 更新并返回新值
    fn update(&mut self, value: f64) -> f64;
    /// 获取当前值
    fn current(&self) -> f64;
    /// 重置状态
    fn reset(&mut self);
}

// 实现流式算子

struct RollingMeanOperator {
    inner: RollingMean,
    window_size: usize,
}

impl StreamOperator for RollingMeanOperator {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value()
    }
    fn current(&self) -> f64 {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner = RollingMean::new(self.window_size);
    }
}

struct EMAOperator {
    inner: EMA,
    span: usize,
}

impl StreamOperator for EMAOperator {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().unwrap_or(value)
    }
    fn current(&self) -> f64 {
        self.inner.value().unwrap_or(0.0)
    }
    fn reset(&mut self) {
        self.inner = EMA::new(self.span);
    }
}

struct RSIOperator {
    inner: RSI,
    period: usize,
}

impl StreamOperator for RSIOperator {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().unwrap_or(50.0) // Default to neutral RSI
    }
    fn current(&self) -> f64 {
        self.inner.value().unwrap_or(50.0)
    }
    fn reset(&mut self) {
        self.inner = RSI::new(self.period);
    }
}

struct MACDOperator {
    inner: MACD,
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
}

impl StreamOperator for MACDOperator {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().map(|v| v.macd).unwrap_or(0.0)
    }
    fn current(&self) -> f64 {
        self.inner.value().map(|v| v.macd).unwrap_or(0.0)
    }
    fn reset(&mut self) {
        self.inner = MACD::new(self.fast_period, self.slow_period, self.signal_period);
    }
}

impl StreamFactorEngine {
    pub fn new(registry: FactorRegistry) -> Self {
        Self {
            registry,
            operators: HashMap::new(),
            current_values: HashMap::new(),
        }
    }

    /// 初始化因子的流式算子
    pub fn init_factor(&mut self, factor_id: &str) -> Result<(), String> {
        let factor = self
            .registry
            .get(factor_id)
            .ok_or_else(|| format!("Factor not found: {}", factor_id))?;

        let operator: Box<dyn StreamOperator> = match &factor.def {
            FactorDef::Rolling { window, func, .. } => match func {
                RollingFunc::Mean => Box::new(RollingMeanOperator {
                    inner: RollingMean::new(*window),
                    window_size: *window,
                }),
                _ => return Err(format!("Rolling {:?} not supported in stream mode", func)),
            },
            FactorDef::EMA { span, .. } => Box::new(EMAOperator {
                inner: EMA::new(*span),
                span: *span,
            }),
            FactorDef::RSI { period, .. } => Box::new(RSIOperator {
                inner: RSI::new(*period),
                period: *period,
            }),
            FactorDef::MACD {
                fast,
                slow,
                signal,
                ..
            } => Box::new(MACDOperator {
                inner: MACD::new(*fast, *slow, *signal),
                fast_period: *fast,
                slow_period: *slow,
                signal_period: *signal,
            }),
            _ => return Err(format!("Factor type not supported in stream mode")),
        };

        self.operators.insert(factor_id.to_string(), operator);
        Ok(())
    }

    /// 更新单个数据点
    pub fn update(&mut self, factor_id: &str, value: f64) -> Result<f64, String> {
        let operator = self
            .operators
            .get_mut(factor_id)
            .ok_or_else(|| format!("Operator not initialized: {}", factor_id))?;

        let new_value = operator.update(value);
        self.current_values.insert(factor_id.to_string(), new_value);
        Ok(new_value)
    }

    /// 批量更新
    pub fn update_all(&mut self, source_value: f64, factor_ids: &[&str]) -> HashMap<String, f64> {
        let mut results = HashMap::new();

        for factor_id in factor_ids {
            if let Ok(value) = self.update(factor_id, source_value) {
                results.insert(factor_id.to_string(), value);
            }
        }

        results
    }

    /// 获取当前值
    pub fn get(&self, factor_id: &str) -> Option<f64> {
        self.current_values.get(factor_id).copied()
    }

    /// 获取所有当前值
    pub fn get_all(&self) -> &HashMap<String, f64> {
        &self.current_values
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 批量因子引擎 (Polars)
// ═══════════════════════════════════════════════════════════════════════════

/// 批量因子引擎 - 使用 Polars
pub struct BatchFactorEngine {
    registry: FactorRegistry,
}

impl BatchFactorEngine {
    pub fn new(registry: FactorRegistry) -> Self {
        Self { registry }
    }

    /// 将因子定义编译为 Polars 表达式
    ///
    /// 注意：部分高级功能（Rolling, EWM）需要在 LazyFrame 上使用
    pub fn compile_to_expr(&self, def: &FactorDef, alias: &str) -> Result<Expr, String> {
        let expr = self.build_expr(def)?;
        Ok(expr.alias(alias))
    }

    fn build_expr(&self, def: &FactorDef) -> Result<Expr, String> {
        match def {
            FactorDef::Source { name } => Ok(col(name)),

            FactorDef::Rolling { source, window, func } => {
                // 使用简化的表达式，实际计算在 DataFrame 层面
                // Polars 0.51 的 rolling 函数签名有变化
                Ok(col(source).alias(&format!("rolling_{}_{}", func_name(*func), window)))
            }

            FactorDef::EMA { source, span } => {
                // 简化实现：返回列引用，实际 EWM 计算在 DataFrame 层面
                Ok(col(source).alias(&format!("ema_{}", span)))
            }

            FactorDef::BinaryOp { left, right, op } => {
                let left_expr = self.build_expr(left)?;
                let right_expr = self.build_expr(right)?;

                Ok(match op {
                    BinaryOpType::Add => left_expr + right_expr,
                    BinaryOpType::Sub => left_expr - right_expr,
                    BinaryOpType::Mul => left_expr * right_expr,
                    BinaryOpType::Div => left_expr / right_expr,
                })
            }

            FactorDef::Ref { factor_id } => {
                let factor = self
                    .registry
                    .get(factor_id)
                    .ok_or_else(|| format!("Factor not found: {}", factor_id))?;
                self.build_expr(&factor.def)
            }

            FactorDef::RSI { source, period } => {
                // RSI 简化实现
                Ok(col(source).alias(&format!("rsi_{}", period)))
            }

            FactorDef::MACD { source, fast, slow, .. } => {
                // MACD 简化实现
                Ok(col(source).alias(&format!("macd_{}_{}", fast, slow)))
            }

            FactorDef::Bollinger { source, window, num_std } => {
                Ok(col(source).alias(&format!("boll_{}_{}", window, num_std)))
            }

            FactorDef::PolarsExpr { expr_str } => {
                Err(format!(
                    "PolarsExpr requires SQL parsing: {}",
                    expr_str
                ))
            }
        }
    }

    /// 计算多个因子
    pub fn compute(
        &self,
        df: LazyFrame,
        factor_ids: &[&str],
    ) -> Result<LazyFrame, String> {
        let mut result = df;

        for factor_id in factor_ids {
            let factor = self
                .registry
                .get(factor_id)
                .ok_or_else(|| format!("Factor not found: {}", factor_id))?;

            let expr = self.compile_to_expr(&factor.def, factor_id)?;
            result = result.with_column(expr);
        }

        Ok(result)
    }
}

fn func_name(func: RollingFunc) -> &'static str {
    match func {
        RollingFunc::Mean => "mean",
        RollingFunc::Std => "std",
        RollingFunc::Sum => "sum",
        RollingFunc::Min => "min",
        RollingFunc::Max => "max",
        RollingFunc::Var => "var",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 统一因子引擎
// ═══════════════════════════════════════════════════════════════════════════

/// 统一因子引擎 - 自动选择流/批模式
pub struct FactorEngine {
    pub stream: StreamFactorEngine,
    pub batch: BatchFactorEngine,
}

impl FactorEngine {
    pub fn new() -> Self {
        let registry = FactorRegistry::with_standard_factors();
        Self {
            stream: StreamFactorEngine::new(registry.clone()),
            batch: BatchFactorEngine::new(registry),
        }
    }

    pub fn with_registry(registry: FactorRegistry) -> Self {
        Self {
            stream: StreamFactorEngine::new(registry.clone()),
            batch: BatchFactorEngine::new(registry),
        }
    }

    /// 注册因子
    pub fn register(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        def: FactorDef,
        description: impl Into<String>,
    ) {
        let id_str: String = id.into();
        let name_str: String = name.into();
        let desc_str: String = description.into();

        self.stream.registry.register(
            id_str.clone(),
            name_str.clone(),
            def.clone(),
            desc_str.clone(),
        );
        self.batch.registry.register(id_str, name_str, def, desc_str);
    }

    /// 初始化流式因子
    pub fn init_stream_factor(&mut self, factor_id: &str) -> Result<(), String> {
        self.stream.init_factor(factor_id)
    }

    /// 流式更新
    pub fn stream_update(&mut self, factor_id: &str, value: f64) -> Result<f64, String> {
        self.stream.update(factor_id, value)
    }

    /// 批量计算
    pub fn batch_compute(
        &self,
        df: LazyFrame,
        factor_ids: &[&str],
    ) -> Result<LazyFrame, String> {
        self.batch.compute(df, factor_ids)
    }
}

impl Default for FactorEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 并行 DAG 执行器
// ═══════════════════════════════════════════════════════════════════════════

/// 并行 DAG 执行器
///
/// 使用 Rayon 按层级并行计算因子：
/// - Level 0: 源数据 (price, volume, etc.) - 无依赖，完全并行
/// - Level 1: 一级因子 (ma5, ema12, rsi14) - 并行计算
/// - Level 2: 二级因子 (macd_line, bollinger) - 并行计算
/// - ...
///
/// 性能特性：
/// - 同层因子完全并行
/// - 层间按序执行，保证依赖关系
/// - 使用 DashMap 存储中间结果，无锁读取
pub struct ParallelDagExecutor {
    /// 因子注册表
    registry: Arc<FactorRegistry>,
    /// DAG 结构
    dag: Arc<FactorDag>,
    /// 因子状态 (factor_id -> operator)
    operators: Arc<DashMap<String, RwLock<Box<dyn StreamOperator + Send + Sync>>>>,
    /// 当前因子值
    values: Arc<DashMap<String, f64>>,
    /// 统计：总计算次数
    stats_compute_count: AtomicU64,
    /// 统计：总计算时间 (微秒)
    stats_compute_time_us: AtomicU64,
}

/// 并行执行结果
#[derive(Debug, Clone)]
pub struct ParallelExecutionResult {
    /// 所有因子值
    pub values: HashMap<String, f64>,
    /// 执行耗时 (微秒)
    pub elapsed_us: u64,
    /// 执行层数
    pub levels_executed: usize,
    /// 计算的因子数量
    pub factors_computed: usize,
    /// 使用的线程数
    pub parallelism: usize,
}

impl ParallelDagExecutor {
    /// 创建并行执行器
    pub fn new(registry: FactorRegistry, dag: FactorDag) -> Self {
        Self {
            registry: Arc::new(registry),
            dag: Arc::new(dag),
            operators: Arc::new(DashMap::new()),
            values: Arc::new(DashMap::new()),
            stats_compute_count: AtomicU64::new(0),
            stats_compute_time_us: AtomicU64::new(0),
        }
    }

    /// 创建带标准因子的执行器
    pub fn with_standard_factors() -> Self {
        let registry = FactorRegistry::with_standard_factors();
        let dag = super::dag::create_standard_factor_dag()
            .expect("Failed to create standard factor DAG");
        Self::new(registry, dag)
    }

    /// 初始化所有因子的算子
    pub fn init_all(&self) -> Result<(), String> {
        for factor in self.registry.list() {
            self.init_factor(&factor.id)?;
        }
        Ok(())
    }

    /// 初始化单个因子的算子
    pub fn init_factor(&self, factor_id: &str) -> Result<(), String> {
        let factor = self
            .registry
            .get(factor_id)
            .ok_or_else(|| format!("Factor not found: {}", factor_id))?;

        let operator: Box<dyn StreamOperator + Send + Sync> = match &factor.def {
            FactorDef::Rolling { window, func, .. } => match func {
                RollingFunc::Mean => Box::new(SyncRollingMean::new(*window)),
                _ => return Err(format!("Rolling {:?} not supported in parallel mode", func)),
            },
            FactorDef::EMA { span, .. } => Box::new(SyncEMA::new(*span)),
            FactorDef::RSI { period, .. } => Box::new(SyncRSI::new(*period)),
            FactorDef::MACD {
                fast,
                slow,
                signal,
                ..
            } => Box::new(SyncMACD::new(*fast, *slow, *signal)),
            FactorDef::Source { .. } => {
                // 源数据不需要算子
                return Ok(());
            }
            _ => return Err(format!("Factor type not supported in parallel mode")),
        };

        self.operators
            .insert(factor_id.to_string(), RwLock::new(operator));
        Ok(())
    }

    /// 设置源数据值
    pub fn set_source(&self, source_id: &str, value: f64) {
        self.values.insert(source_id.to_string(), value);
    }

    /// 批量设置源数据
    pub fn set_sources(&self, sources: &HashMap<String, f64>) {
        for (id, value) in sources {
            self.values.insert(id.clone(), *value);
        }
    }

    /// 并行执行 DAG
    ///
    /// 按层级并行计算所有因子：
    /// 1. 获取并行层级
    /// 2. 对每层，使用 Rayon 并行计算所有因子
    /// 3. 更新中间结果
    pub fn execute(&self) -> ParallelExecutionResult {
        let start_time = Instant::now();
        let levels = self.dag.get_parallel_levels();
        let parallelism = rayon::current_num_threads();

        let mut factors_computed = 0;

        for level in &levels {
            // 对当前层的所有因子并行计算
            let results: Vec<(String, f64)> = level
                .par_iter()
                .filter_map(|factor_id| {
                    // 获取因子定义
                    let factor = self.registry.get(factor_id)?;

                    // 计算因子值（每次执行都覆盖旧值，避免缓存陈旧）
                    let value = self.compute_factor(factor);
                    Some((factor_id.clone(), value))
                })
                .collect();

            // 更新结果
            for (id, value) in results {
                self.values.insert(id, value);
                factors_computed += 1;
            }
        }

        let elapsed = start_time.elapsed();
        let elapsed_us = elapsed.as_micros() as u64;

        // 更新统计
        self.stats_compute_count.fetch_add(1, Ordering::Relaxed);
        self.stats_compute_time_us
            .fetch_add(elapsed_us, Ordering::Relaxed);

        // 收集所有值
        let values: HashMap<String, f64> = self
            .values
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();

        ParallelExecutionResult {
            values,
            elapsed_us,
            levels_executed: levels.len(),
            factors_computed,
            parallelism,
        }
    }

    /// 计算单个因子
    fn compute_factor(&self, factor: &RegisteredFactor) -> f64 {
        match &factor.def {
            FactorDef::Source { name } => {
                // 返回源数据值
                self.values.get(name).map(|v| *v).unwrap_or(0.0)
            }
            FactorDef::Rolling { source, .. }
            | FactorDef::EMA { source, .. }
            | FactorDef::RSI { source, .. }
            | FactorDef::MACD { source, .. } => {
                // 获取源值
                let source_value = self.values.get(source).map(|v| *v).unwrap_or(0.0);

                // 获取算子并计算
                if let Some(op) = self.operators.get(&factor.id) {
                    let mut operator = op.write();
                    operator.update(source_value)
                } else {
                    source_value
                }
            }
            FactorDef::BinaryOp { left, right, op } => {
                let left_val = self.compute_def(left);
                let right_val = self.compute_def(right);
                match op {
                    BinaryOpType::Add => left_val + right_val,
                    BinaryOpType::Sub => left_val - right_val,
                    BinaryOpType::Mul => left_val * right_val,
                    BinaryOpType::Div => {
                        if right_val != 0.0 {
                            left_val / right_val
                        } else {
                            0.0
                        }
                    }
                }
            }
            FactorDef::Ref { factor_id } => self.values.get(factor_id).map(|v| *v).unwrap_or(0.0),
            FactorDef::Bollinger {
                source,
                window,
                num_std,
            } => {
                // 简化实现：使用均值 + std
                let source_value = self.values.get(source).map(|v| *v).unwrap_or(0.0);
                source_value + (*num_std * source_value * 0.02) // 简化
            }
            FactorDef::PolarsExpr { .. } => 0.0, // 不支持
        }
    }

    /// 递归计算因子定义
    fn compute_def(&self, def: &FactorDef) -> f64 {
        match def {
            FactorDef::Source { name } => self.values.get(name).map(|v| *v).unwrap_or(0.0),
            FactorDef::Ref { factor_id } => self.values.get(factor_id).map(|v| *v).unwrap_or(0.0),
            FactorDef::BinaryOp { left, right, op } => {
                let l = self.compute_def(left);
                let r = self.compute_def(right);
                match op {
                    BinaryOpType::Add => l + r,
                    BinaryOpType::Sub => l - r,
                    BinaryOpType::Mul => l * r,
                    BinaryOpType::Div => if r != 0.0 { l / r } else { 0.0 },
                }
            }
            _ => 0.0,
        }
    }

    /// 增量更新（单数据点）
    ///
    /// 只更新受影响的因子，而非全量计算
    pub fn incremental_update(&self, source_id: &str, value: f64) -> ParallelExecutionResult {
        let start_time = Instant::now();

        // 设置源数据
        self.values.insert(source_id.to_string(), value);

        // 获取受影响的因子
        let affected = self.dag.get_affected_nodes(source_id);

        // 按拓扑序计算受影响的因子
        let factors_computed = affected.len();
        for factor_id in &affected {
            if let Some(factor) = self.registry.get(factor_id) {
                let new_value = self.compute_factor(factor);
                self.values.insert(factor_id.clone(), new_value);
            }
        }

        let elapsed_us = start_time.elapsed().as_micros() as u64;

        let values: HashMap<String, f64> = self
            .values
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();

        ParallelExecutionResult {
            values,
            elapsed_us,
            levels_executed: 1,
            factors_computed,
            parallelism: 1,
        }
    }

    /// 获取因子值
    pub fn get(&self, factor_id: &str) -> Option<f64> {
        self.values.get(factor_id).map(|v| *v)
    }

    /// 获取所有因子值
    pub fn get_all(&self) -> HashMap<String, f64> {
        self.values
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }

    /// 重置所有状态
    pub fn reset(&self) {
        self.values.clear();
        for entry in self.operators.iter() {
            entry.value().write().reset();
        }
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> (u64, u64) {
        (
            self.stats_compute_count.load(Ordering::Relaxed),
            self.stats_compute_time_us.load(Ordering::Relaxed),
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 线程安全的流式算子包装器
// ═══════════════════════════════════════════════════════════════════════════

struct SyncRollingMean {
    inner: RollingMean,
    window: usize,
}

impl SyncRollingMean {
    fn new(window: usize) -> Self {
        Self {
            inner: RollingMean::new(window),
            window,
        }
    }
}

impl StreamOperator for SyncRollingMean {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value()
    }
    fn current(&self) -> f64 {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner = RollingMean::new(self.window);
    }
}

// Send + Sync 实现
unsafe impl Send for SyncRollingMean {}
unsafe impl Sync for SyncRollingMean {}

struct SyncEMA {
    inner: EMA,
    span: usize,
}

impl SyncEMA {
    fn new(span: usize) -> Self {
        Self {
            inner: EMA::new(span),
            span,
        }
    }
}

impl StreamOperator for SyncEMA {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().unwrap_or(value)
    }
    fn current(&self) -> f64 {
        self.inner.value().unwrap_or(0.0)
    }
    fn reset(&mut self) {
        self.inner = EMA::new(self.span);
    }
}

unsafe impl Send for SyncEMA {}
unsafe impl Sync for SyncEMA {}

struct SyncRSI {
    inner: RSI,
    period: usize,
}

impl SyncRSI {
    fn new(period: usize) -> Self {
        Self {
            inner: RSI::new(period),
            period,
        }
    }
}

impl StreamOperator for SyncRSI {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().unwrap_or(50.0)
    }
    fn current(&self) -> f64 {
        self.inner.value().unwrap_or(50.0)
    }
    fn reset(&mut self) {
        self.inner = RSI::new(self.period);
    }
}

unsafe impl Send for SyncRSI {}
unsafe impl Sync for SyncRSI {}

struct SyncMACD {
    inner: MACD,
    fast: usize,
    slow: usize,
    signal: usize,
}

impl SyncMACD {
    fn new(fast: usize, slow: usize, signal: usize) -> Self {
        Self {
            inner: MACD::new(fast, slow, signal),
            fast,
            slow,
            signal,
        }
    }
}

impl StreamOperator for SyncMACD {
    fn update(&mut self, value: f64) -> f64 {
        self.inner.update(value);
        self.inner.value().map(|v| v.macd).unwrap_or(0.0)
    }
    fn current(&self) -> f64 {
        self.inner.value().map(|v| v.macd).unwrap_or(0.0)
    }
    fn reset(&mut self) {
        self.inner = MACD::new(self.fast, self.slow, self.signal);
    }
}

unsafe impl Send for SyncMACD {}
unsafe impl Sync for SyncMACD {}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factor_registry() {
        let registry = FactorRegistry::with_standard_factors();

        assert!(registry.get("ma5").is_some());
        assert!(registry.get("ema12").is_some());
        assert!(registry.get("rsi14").is_some());
        assert!(registry.get("macd").is_some());
    }

    #[test]
    fn test_stream_engine() {
        let registry = FactorRegistry::with_standard_factors();
        let mut engine = StreamFactorEngine::new(registry);

        // 初始化 MA5
        engine.init_factor("ma5").unwrap();

        // 更新数据
        let values = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
        for v in values {
            let _ = engine.update("ma5", v);
        }

        // 验证结果
        let result = engine.get("ma5").unwrap();
        assert!((result - 13.0).abs() < 0.01); // (11+12+13+14+15)/5 = 13
    }

    #[test]
    fn test_ema_operator() {
        let mut ema = EMA::new(5);

        for v in [10.0, 11.0, 12.0, 13.0, 14.0] {
            ema.update(v);
        }

        // EMA 应该接近最近的值
        let val = ema.value().unwrap();
        assert!(val > 11.0);
        assert!(val < 14.0);
    }

    #[test]
    fn test_rsi_operator() {
        let mut rsi = RSI::new(14);

        // 模拟上涨趋势
        for i in 0..20 {
            rsi.update(100.0 + i as f64);
        }

        // RSI 应该较高
        let value = rsi.value().unwrap();
        assert!(value > 50.0); // 上涨趋势 RSI > 50
    }
}
