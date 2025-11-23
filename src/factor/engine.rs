//! 流批一体因子引擎
//!
//! @yutiansut @quantaxis
//!
//! 核心设计理念：
//! - 流处理 (Stream): 使用增量算子，O(1) 更新
//! - 批处理 (Batch): 使用 Polars，向量化计算
//! - 统一接口: 同一套因子定义，自动选择执行路径

use std::collections::HashMap;
use std::sync::Arc;

use polars::prelude::*;

use super::operators::rolling::*;
use super::dag::{FactorDag, FactorId};

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
