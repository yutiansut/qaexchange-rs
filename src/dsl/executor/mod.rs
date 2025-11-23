//! DSL 执行引擎
//!
//! @yutiansut @quantaxis
//!
//! 提供因子 DSL 的执行功能：
//! - 表达式求值
//! - 增量计算
//! - 状态管理

use std::collections::HashMap;
use std::sync::Arc;

use crate::factor::operators::rolling::*;
use crate::factor::operators::basic::*;
use super::ast::*;

// ═══════════════════════════════════════════════════════════════════════════
// 执行上下文
// ═══════════════════════════════════════════════════════════════════════════

/// 执行上下文
pub struct ExecutionContext {
    /// 变量表
    variables: HashMap<String, Value>,
    /// 因子定义
    factors: HashMap<String, FactorDef>,
    /// 内置函数
    builtins: HashMap<String, BuiltinFunction>,
}

/// 值类型
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Float(f64),
    Integer(i64),
    Boolean(bool),
    String(String),
    Series(Vec<f64>),
}

impl Value {
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_series(&self) -> Option<&Vec<f64>> {
        match self {
            Value::Series(s) => Some(s),
            _ => None,
        }
    }
}

/// 内置函数类型
type BuiltinFunction = Arc<dyn Fn(&[Value]) -> Result<Value, ExecutionError> + Send + Sync>;

impl ExecutionContext {
    pub fn new() -> Self {
        let mut ctx = Self {
            variables: HashMap::new(),
            factors: HashMap::new(),
            builtins: HashMap::new(),
        };
        ctx.register_builtins();
        ctx
    }

    /// 注册内置函数
    fn register_builtins(&mut self) {
        // abs
        self.builtins.insert(
            "abs".to_string(),
            Arc::new(|args| {
                if args.len() != 1 {
                    return Err(ExecutionError::ArgumentError("abs requires 1 argument".into()));
                }
                let val = args[0].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(val.abs()))
            }),
        );

        // sqrt
        self.builtins.insert(
            "sqrt".to_string(),
            Arc::new(|args| {
                if args.len() != 1 {
                    return Err(ExecutionError::ArgumentError("sqrt requires 1 argument".into()));
                }
                let val = args[0].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(val.sqrt()))
            }),
        );

        // log
        self.builtins.insert(
            "log".to_string(),
            Arc::new(|args| {
                if args.len() != 1 {
                    return Err(ExecutionError::ArgumentError("log requires 1 argument".into()));
                }
                let val = args[0].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(val.ln()))
            }),
        );

        // exp
        self.builtins.insert(
            "exp".to_string(),
            Arc::new(|args| {
                if args.len() != 1 {
                    return Err(ExecutionError::ArgumentError("exp requires 1 argument".into()));
                }
                let val = args[0].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(val.exp()))
            }),
        );

        // pow
        self.builtins.insert(
            "pow".to_string(),
            Arc::new(|args| {
                if args.len() != 2 {
                    return Err(ExecutionError::ArgumentError("pow requires 2 arguments".into()));
                }
                let base = args[0].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                let exp = args[1].as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(base.powf(exp)))
            }),
        );

        // isnull
        self.builtins.insert(
            "isnull".to_string(),
            Arc::new(|args| {
                if args.len() != 1 {
                    return Err(ExecutionError::ArgumentError("isnull requires 1 argument".into()));
                }
                Ok(Value::Boolean(matches!(args[0], Value::Null)))
            }),
        );

        // fillna
        self.builtins.insert(
            "fillna".to_string(),
            Arc::new(|args| {
                if args.len() != 2 {
                    return Err(ExecutionError::ArgumentError("fillna requires 2 arguments".into()));
                }
                match &args[0] {
                    Value::Null => Ok(args[1].clone()),
                    other => Ok(other.clone()),
                }
            }),
        );
    }

    /// 设置变量
    pub fn set_variable(&mut self, name: &str, value: Value) {
        self.variables.insert(name.to_string(), value);
    }

    /// 获取变量
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// 注册因子
    pub fn register_factor(&mut self, def: FactorDef) {
        self.factors.insert(def.name.clone(), def);
    }

    /// 获取因子定义
    pub fn get_factor(&self, name: &str) -> Option<&FactorDef> {
        self.factors.get(name)
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 执行错误
// ═══════════════════════════════════════════════════════════════════════════

/// 执行错误
#[derive(Debug)]
pub enum ExecutionError {
    /// 未定义的变量
    UndefinedVariable(String),
    /// 未定义的函数
    UndefinedFunction(String),
    /// 类型错误
    TypeError(String),
    /// 参数错误
    ArgumentError(String),
    /// 除零错误
    DivisionByZero,
    /// 运行时错误
    RuntimeError(String),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::UndefinedVariable(name) => write!(f, "Undefined variable: {}", name),
            ExecutionError::UndefinedFunction(name) => write!(f, "Undefined function: {}", name),
            ExecutionError::TypeError(msg) => write!(f, "Type error: {}", msg),
            ExecutionError::ArgumentError(msg) => write!(f, "Argument error: {}", msg),
            ExecutionError::DivisionByZero => write!(f, "Division by zero"),
            ExecutionError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
        }
    }
}

impl std::error::Error for ExecutionError {}

pub type ExecutionResult<T> = Result<T, ExecutionError>;

// ═══════════════════════════════════════════════════════════════════════════
// 表达式求值器
// ═══════════════════════════════════════════════════════════════════════════

/// 表达式求值器
pub struct Evaluator<'a> {
    context: &'a ExecutionContext,
}

impl<'a> Evaluator<'a> {
    pub fn new(context: &'a ExecutionContext) -> Self {
        Self { context }
    }

    /// 求值表达式
    pub fn evaluate(&self, expr: &Expression) -> ExecutionResult<Value> {
        match expr {
            Expression::Literal(lit) => self.eval_literal(lit),
            Expression::Identifier(name) => self.eval_identifier(name),
            Expression::BinaryOp(op) => self.eval_binary_op(op),
            Expression::UnaryOp(op) => self.eval_unary_op(op),
            Expression::FunctionCall(call) => self.eval_function_call(call),
            Expression::Conditional(cond) => self.eval_conditional(cond),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> ExecutionResult<Value> {
        Ok(match lit {
            Literal::Integer(i) => Value::Integer(*i),
            Literal::Float(f) => Value::Float(*f),
            Literal::String(s) => Value::String(s.clone()),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
        })
    }

    fn eval_identifier(&self, name: &str) -> ExecutionResult<Value> {
        self.context
            .get_variable(name)
            .cloned()
            .ok_or_else(|| ExecutionError::UndefinedVariable(name.to_string()))
    }

    fn eval_binary_op(&self, op: &BinaryOp) -> ExecutionResult<Value> {
        let left = self.evaluate(&op.left)?;
        let right = self.evaluate(&op.right)?;

        match op.op {
            // 算术运算
            BinaryOperator::Add => self.arithmetic_op(&left, &right, |a, b| a + b),
            BinaryOperator::Sub => self.arithmetic_op(&left, &right, |a, b| a - b),
            BinaryOperator::Mul => self.arithmetic_op(&left, &right, |a, b| a * b),
            BinaryOperator::Div => {
                let r = right.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                if r == 0.0 {
                    return Err(ExecutionError::DivisionByZero);
                }
                self.arithmetic_op(&left, &right, |a, b| a / b)
            }
            BinaryOperator::Mod => self.arithmetic_op(&left, &right, |a, b| a % b),
            BinaryOperator::Pow => self.arithmetic_op(&left, &right, |a, b| a.powf(b)),

            // 比较运算
            BinaryOperator::Eq => self.comparison_op(&left, &right, |a, b| (a - b).abs() < f64::EPSILON),
            BinaryOperator::Ne => self.comparison_op(&left, &right, |a, b| (a - b).abs() >= f64::EPSILON),
            BinaryOperator::Lt => self.comparison_op(&left, &right, |a, b| a < b),
            BinaryOperator::Le => self.comparison_op(&left, &right, |a, b| a <= b),
            BinaryOperator::Gt => self.comparison_op(&left, &right, |a, b| a > b),
            BinaryOperator::Ge => self.comparison_op(&left, &right, |a, b| a >= b),

            // 逻辑运算
            BinaryOperator::And => {
                let l = left.as_bool().ok_or(ExecutionError::TypeError("Expected boolean".into()))?;
                let r = right.as_bool().ok_or(ExecutionError::TypeError("Expected boolean".into()))?;
                Ok(Value::Boolean(l && r))
            }
            BinaryOperator::Or => {
                let l = left.as_bool().ok_or(ExecutionError::TypeError("Expected boolean".into()))?;
                let r = right.as_bool().ok_or(ExecutionError::TypeError("Expected boolean".into()))?;
                Ok(Value::Boolean(l || r))
            }
        }
    }

    fn arithmetic_op<F>(&self, left: &Value, right: &Value, op: F) -> ExecutionResult<Value>
    where
        F: Fn(f64, f64) -> f64,
    {
        let l = left.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
        let r = right.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
        Ok(Value::Float(op(l, r)))
    }

    fn comparison_op<F>(&self, left: &Value, right: &Value, op: F) -> ExecutionResult<Value>
    where
        F: Fn(f64, f64) -> bool,
    {
        let l = left.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
        let r = right.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
        Ok(Value::Boolean(op(l, r)))
    }

    fn eval_unary_op(&self, op: &UnaryOp) -> ExecutionResult<Value> {
        let operand = self.evaluate(&op.operand)?;

        match op.op {
            UnaryOperator::Neg => {
                let val = operand.as_float().ok_or(ExecutionError::TypeError("Expected number".into()))?;
                Ok(Value::Float(-val))
            }
            UnaryOperator::Not => {
                let val = operand.as_bool().ok_or(ExecutionError::TypeError("Expected boolean".into()))?;
                Ok(Value::Boolean(!val))
            }
        }
    }

    fn eval_function_call(&self, call: &FunctionCall) -> ExecutionResult<Value> {
        // 先检查内置函数
        if let Some(builtin) = self.context.builtins.get(&call.name) {
            let args: Vec<Value> = call
                .args
                .iter()
                .map(|arg| self.evaluate(arg))
                .collect::<Result<_, _>>()?;
            return builtin(&args);
        }

        // 检查用户定义的因子
        if let Some(factor) = self.context.get_factor(&call.name) {
            return self.evaluate(&factor.expr);
        }

        Err(ExecutionError::UndefinedFunction(call.name.clone()))
    }

    fn eval_conditional(&self, cond: &Conditional) -> ExecutionResult<Value> {
        let condition = self.evaluate(&cond.condition)?;
        let is_true = condition.as_bool().ok_or(ExecutionError::TypeError("Condition must be boolean".into()))?;

        if is_true {
            self.evaluate(&cond.then_branch)
        } else {
            self.evaluate(&cond.else_branch)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 增量执行器
// ═══════════════════════════════════════════════════════════════════════════

/// 增量因子执行器
pub struct IncrementalExecutor {
    context: ExecutionContext,
    /// 滚动均值状态
    rolling_means: HashMap<String, RollingMean>,
    /// 滚动标准差状态
    rolling_stds: HashMap<String, RollingStd>,
    /// EMA 状态
    emas: HashMap<String, EMA>,
    /// RSI 状态
    rsis: HashMap<String, RSI>,
}

impl IncrementalExecutor {
    pub fn new() -> Self {
        Self {
            context: ExecutionContext::new(),
            rolling_means: HashMap::new(),
            rolling_stds: HashMap::new(),
            emas: HashMap::new(),
            rsis: HashMap::new(),
        }
    }

    /// 设置数据源
    pub fn set_source(&mut self, name: &str, value: f64) {
        self.context.set_variable(name, Value::Float(value));
    }

    /// 更新增量状态
    pub fn update(&mut self, source_name: &str, value: f64) {
        // 更新所有依赖此数据源的增量算子
        for (key, rm) in &mut self.rolling_means {
            if key.starts_with(source_name) {
                rm.update(value);
            }
        }

        for (key, rs) in &mut self.rolling_stds {
            if key.starts_with(source_name) {
                rs.update(value);
            }
        }

        for (key, ema) in &mut self.emas {
            if key.starts_with(source_name) {
                ema.update(value);
            }
        }

        for (key, rsi) in &mut self.rsis {
            if key.starts_with(source_name) {
                rsi.update(value);
            }
        }
    }

    /// 获取或创建滚动均值
    pub fn get_or_create_ma(&mut self, source: &str, period: usize) -> f64 {
        let key = format!("{}_{}", source, period);

        if !self.rolling_means.contains_key(&key) {
            self.rolling_means.insert(key.clone(), RollingMean::new(period));
        }

        self.rolling_means.get(&key).unwrap().value()
    }

    /// 获取或创建滚动标准差
    pub fn get_or_create_std(&mut self, source: &str, period: usize) -> f64 {
        let key = format!("{}_{}", source, period);

        if !self.rolling_stds.contains_key(&key) {
            self.rolling_stds.insert(key.clone(), RollingStd::new(period));
        }

        self.rolling_stds.get(&key).unwrap().value()
    }

    /// 获取或创建 EMA
    pub fn get_or_create_ema(&mut self, source: &str, period: usize) -> Option<f64> {
        let key = format!("{}_{}", source, period);

        if !self.emas.contains_key(&key) {
            self.emas.insert(key.clone(), EMA::new(period));
        }

        self.emas.get(&key).unwrap().value()
    }

    /// 获取或创建 RSI
    pub fn get_or_create_rsi(&mut self, source: &str, period: usize) -> Option<f64> {
        let key = format!("{}_{}", source, period);

        if !self.rsis.contains_key(&key) {
            self.rsis.insert(key.clone(), RSI::new(period));
        }

        self.rsis.get(&key).unwrap().value()
    }

    /// 重置所有状态
    pub fn reset(&mut self) {
        self.rolling_means.clear();
        self.rolling_stds.clear();
        self.emas.clear();
        self.rsis.clear();
    }
}

impl Default for IncrementalExecutor {
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
    fn test_basic_evaluation() {
        let mut ctx = ExecutionContext::new();
        ctx.set_variable("x", Value::Float(10.0));
        ctx.set_variable("y", Value::Float(5.0));

        let evaluator = Evaluator::new(&ctx);

        // x + y
        let expr = Expression::BinaryOp(Box::new(BinaryOp {
            op: BinaryOperator::Add,
            left: Expression::Identifier("x".to_string()),
            right: Expression::Identifier("y".to_string()),
        }));

        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result.as_float(), Some(15.0));
    }

    #[test]
    fn test_builtin_functions() {
        let ctx = ExecutionContext::new();
        let evaluator = Evaluator::new(&ctx);

        // sqrt(16)
        let expr = Expression::FunctionCall(FunctionCall {
            name: "sqrt".to_string(),
            args: vec![Expression::Literal(Literal::Float(16.0))],
        });

        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result.as_float(), Some(4.0));
    }

    #[test]
    fn test_incremental_executor() {
        let mut executor = IncrementalExecutor::new();

        // 模拟价格序列
        let prices = [100.0, 101.0, 102.0, 101.5, 102.5, 103.0, 102.0, 103.5, 104.0, 103.0];

        for price in prices {
            executor.set_source("close", price);
            executor.update("close", price);
        }

        let ma5 = executor.get_or_create_ma("close", 5);
        assert!(ma5 > 0.0);
    }
}
