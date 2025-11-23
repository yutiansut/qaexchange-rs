//! 因子表达式 AST 定义
//!
//! @yutiansut @quantaxis
//!
//! 提供因子 DSL 的抽象语法树定义

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// 程序结构
// ═══════════════════════════════════════════════════════════════════════════

/// 程序 (语句列表)
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

/// 语句
#[derive(Debug, Clone)]
pub enum Statement {
    /// 因子定义
    FactorDef(FactorDef),
    /// 赋值语句
    Assignment(Assignment),
    /// 表达式语句
    Expression(Expression),
}

/// 因子定义
#[derive(Debug, Clone)]
pub struct FactorDef {
    pub name: String,
    pub expr: Expression,
    /// 元数据
    pub metadata: FactorMetadata,
}

/// 因子元数据
#[derive(Debug, Clone, Default)]
pub struct FactorMetadata {
    pub description: Option<String>,
    pub category: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
}

/// 赋值语句
#[derive(Debug, Clone)]
pub struct Assignment {
    pub name: String,
    pub expr: Expression,
}

// ═══════════════════════════════════════════════════════════════════════════
// 表达式
// ═══════════════════════════════════════════════════════════════════════════

/// 表达式
#[derive(Debug, Clone)]
pub enum Expression {
    /// 字面量
    Literal(Literal),
    /// 标识符
    Identifier(String),
    /// 二元运算
    BinaryOp(Box<BinaryOp>),
    /// 一元运算
    UnaryOp(Box<UnaryOp>),
    /// 函数调用
    FunctionCall(FunctionCall),
    /// 条件表达式
    Conditional(Box<Conditional>),
}

/// 字面量
#[derive(Debug, Clone)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
}

/// 二元运算
#[derive(Debug, Clone)]
pub struct BinaryOp {
    pub op: BinaryOperator,
    pub left: Expression,
    pub right: Expression,
}

/// 二元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    // 算术
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    // 比较
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // 逻辑
    And,
    Or,
}

impl BinaryOperator {
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOperator::Or => 1,
            BinaryOperator::And => 2,
            BinaryOperator::Eq | BinaryOperator::Ne => 3,
            BinaryOperator::Lt | BinaryOperator::Le | BinaryOperator::Gt | BinaryOperator::Ge => 4,
            BinaryOperator::Add | BinaryOperator::Sub => 5,
            BinaryOperator::Mul | BinaryOperator::Div | BinaryOperator::Mod => 6,
            BinaryOperator::Pow => 7,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "+" => Some(BinaryOperator::Add),
            "-" => Some(BinaryOperator::Sub),
            "*" => Some(BinaryOperator::Mul),
            "/" => Some(BinaryOperator::Div),
            "%" => Some(BinaryOperator::Mod),
            "**" => Some(BinaryOperator::Pow),
            "==" => Some(BinaryOperator::Eq),
            "!=" => Some(BinaryOperator::Ne),
            "<" => Some(BinaryOperator::Lt),
            "<=" => Some(BinaryOperator::Le),
            ">" => Some(BinaryOperator::Gt),
            ">=" => Some(BinaryOperator::Ge),
            "&&" => Some(BinaryOperator::And),
            "||" => Some(BinaryOperator::Or),
            _ => None,
        }
    }
}

/// 一元运算
#[derive(Debug, Clone)]
pub struct UnaryOp {
    pub op: UnaryOperator,
    pub operand: Expression,
}

/// 一元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Neg,
    Not,
}

/// 函数调用
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<Expression>,
}

/// 条件表达式
#[derive(Debug, Clone)]
pub struct Conditional {
    pub condition: Expression,
    pub then_branch: Expression,
    pub else_branch: Expression,
}

// ═══════════════════════════════════════════════════════════════════════════
// 内置函数定义
// ═══════════════════════════════════════════════════════════════════════════

/// 内置函数签名
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<ParamDef>,
    pub return_type: DataType,
    pub description: String,
}

/// 参数定义
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub data_type: DataType,
    pub default_value: Option<Literal>,
}

/// 数据类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Float,
    Integer,
    Boolean,
    String,
    Series,     // 时间序列
    CrossSection, // 横截面数据
    Any,
}

/// 获取内置函数列表
pub fn builtin_functions() -> HashMap<String, FunctionSignature> {
    let mut funcs = HashMap::new();

    // ma(source, period) - 移动平均
    funcs.insert(
        "ma".to_string(),
        FunctionSignature {
            name: "ma".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Simple Moving Average".to_string(),
        },
    );

    // ema(source, period) - 指数移动平均
    funcs.insert(
        "ema".to_string(),
        FunctionSignature {
            name: "ema".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Exponential Moving Average".to_string(),
        },
    );

    // std(source, period) - 标准差
    funcs.insert(
        "std".to_string(),
        FunctionSignature {
            name: "std".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Rolling Standard Deviation".to_string(),
        },
    );

    // sum(source, period) - 求和
    funcs.insert(
        "sum".to_string(),
        FunctionSignature {
            name: "sum".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Rolling Sum".to_string(),
        },
    );

    // max(source, period) - 最大值
    funcs.insert(
        "max".to_string(),
        FunctionSignature {
            name: "max".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Rolling Maximum".to_string(),
        },
    );

    // min(source, period) - 最小值
    funcs.insert(
        "min".to_string(),
        FunctionSignature {
            name: "min".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(20)),
                },
            ],
            return_type: DataType::Series,
            description: "Rolling Minimum".to_string(),
        },
    );

    // rsi(source, period) - RSI
    funcs.insert(
        "rsi".to_string(),
        FunctionSignature {
            name: "rsi".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(14)),
                },
            ],
            return_type: DataType::Series,
            description: "Relative Strength Index".to_string(),
        },
    );

    // macd(source, fast, slow, signal) - MACD
    funcs.insert(
        "macd".to_string(),
        FunctionSignature {
            name: "macd".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "fast".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(12)),
                },
                ParamDef {
                    name: "slow".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(26)),
                },
                ParamDef {
                    name: "signal".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(9)),
                },
            ],
            return_type: DataType::Series,
            description: "Moving Average Convergence Divergence".to_string(),
        },
    );

    // rank(source) - 横截面排名
    funcs.insert(
        "rank".to_string(),
        FunctionSignature {
            name: "rank".to_string(),
            params: vec![ParamDef {
                name: "source".to_string(),
                data_type: DataType::CrossSection,
                default_value: None,
            }],
            return_type: DataType::CrossSection,
            description: "Cross-sectional Rank".to_string(),
        },
    );

    // delay(source, period) - 延迟
    funcs.insert(
        "delay".to_string(),
        FunctionSignature {
            name: "delay".to_string(),
            params: vec![
                ParamDef {
                    name: "source".to_string(),
                    data_type: DataType::Series,
                    default_value: None,
                },
                ParamDef {
                    name: "period".to_string(),
                    data_type: DataType::Integer,
                    default_value: Some(Literal::Integer(1)),
                },
            ],
            return_type: DataType::Series,
            description: "Delay/Lag values".to_string(),
        },
    );

    funcs
}

// ═══════════════════════════════════════════════════════════════════════════
// AST 访问者模式
// ═══════════════════════════════════════════════════════════════════════════

/// AST 访问者 trait
pub trait AstVisitor {
    type Result;

    fn visit_program(&mut self, program: &Program) -> Self::Result;
    fn visit_statement(&mut self, stmt: &Statement) -> Self::Result;
    fn visit_expression(&mut self, expr: &Expression) -> Self::Result;
    fn visit_literal(&mut self, lit: &Literal) -> Self::Result;
    fn visit_identifier(&mut self, name: &str) -> Self::Result;
    fn visit_binary_op(&mut self, op: &BinaryOp) -> Self::Result;
    fn visit_unary_op(&mut self, op: &UnaryOp) -> Self::Result;
    fn visit_function_call(&mut self, call: &FunctionCall) -> Self::Result;
}

/// AST 打印器
pub struct AstPrinter {
    indent: usize,
}

impl AstPrinter {
    pub fn new() -> Self {
        Self { indent: 0 }
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }

    pub fn print(&mut self, program: &Program) -> String {
        let mut output = String::new();
        output.push_str("Program {\n");
        self.indent += 1;

        for stmt in &program.statements {
            output.push_str(&self.print_statement(stmt));
        }

        self.indent -= 1;
        output.push_str("}\n");
        output
    }

    fn print_statement(&mut self, stmt: &Statement) -> String {
        let mut output = format!("{}Statement: ", self.indent_str());

        match stmt {
            Statement::FactorDef(def) => {
                output.push_str(&format!("FactorDef({}) = ", def.name));
                output.push_str(&self.print_expression(&def.expr));
            }
            Statement::Assignment(assign) => {
                output.push_str(&format!("Assignment({}) = ", assign.name));
                output.push_str(&self.print_expression(&assign.expr));
            }
            Statement::Expression(expr) => {
                output.push_str(&self.print_expression(expr));
            }
        }

        output.push('\n');
        output
    }

    fn print_expression(&self, expr: &Expression) -> String {
        match expr {
            Expression::Literal(lit) => format!("{:?}", lit),
            Expression::Identifier(name) => name.clone(),
            Expression::BinaryOp(op) => {
                format!(
                    "({} {:?} {})",
                    self.print_expression(&op.left),
                    op.op,
                    self.print_expression(&op.right)
                )
            }
            Expression::UnaryOp(op) => {
                format!("({:?} {})", op.op, self.print_expression(&op.operand))
            }
            Expression::FunctionCall(call) => {
                let args: Vec<String> =
                    call.args.iter().map(|a| self.print_expression(a)).collect();
                format!("{}({})", call.name, args.join(", "))
            }
            Expression::Conditional(cond) => {
                format!(
                    "if {} then {} else {}",
                    self.print_expression(&cond.condition),
                    self.print_expression(&cond.then_branch),
                    self.print_expression(&cond.else_branch)
                )
            }
        }
    }
}

impl Default for AstPrinter {
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
    fn test_builtin_functions() {
        let funcs = builtin_functions();

        assert!(funcs.contains_key("ma"));
        assert!(funcs.contains_key("ema"));
        assert!(funcs.contains_key("rsi"));
        assert!(funcs.contains_key("macd"));

        // 验证 MACD 有 4 个参数
        let macd = funcs.get("macd").unwrap();
        assert_eq!(macd.params.len(), 4);
    }

    #[test]
    fn test_binary_operator_precedence() {
        assert!(BinaryOperator::Mul.precedence() > BinaryOperator::Add.precedence());
        assert!(BinaryOperator::And.precedence() > BinaryOperator::Or.precedence());
    }

    #[test]
    fn test_ast_printer() {
        let program = Program {
            statements: vec![Statement::FactorDef(FactorDef {
                name: "my_ma".to_string(),
                expr: Expression::FunctionCall(FunctionCall {
                    name: "ma".to_string(),
                    args: vec![
                        Expression::Identifier("close".to_string()),
                        Expression::Literal(Literal::Integer(20)),
                    ],
                }),
                metadata: FactorMetadata::default(),
            })],
        };

        let mut printer = AstPrinter::new();
        let output = printer.print(&program);

        assert!(output.contains("FactorDef(my_ma)"));
        assert!(output.contains("ma(close, Integer(20))"));
    }
}
