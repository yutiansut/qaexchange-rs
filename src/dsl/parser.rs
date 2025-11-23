//! 因子 DSL 解析器
//!
//! @yutiansut @quantaxis
//!
//! 基于 pest 的因子 DSL 解析器

use pest::Parser;
use pest_derive::Parser;

use super::ast::*;

// ═══════════════════════════════════════════════════════════════════════════
// Pest 解析器定义
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Parser)]
#[grammar = "dsl/grammar.pest"]
pub struct FactorParser;

// ═══════════════════════════════════════════════════════════════════════════
// 解析错误
// ═══════════════════════════════════════════════════════════════════════════

/// 解析错误
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parse error at line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

// ═══════════════════════════════════════════════════════════════════════════
// AST 构建器
// ═══════════════════════════════════════════════════════════════════════════

/// AST 构建器
pub struct AstBuilder;

impl AstBuilder {
    /// 解析程序
    pub fn parse(input: &str) -> ParseResult<Program> {
        let pairs = FactorParser::parse(Rule::program, input).map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError {
                message: e.to_string(),
                line,
                column,
            }
        })?;

        let mut statements = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::statement => {
                    if let Some(stmt) = Self::build_statement(pair)? {
                        statements.push(stmt);
                    }
                }
                Rule::EOI => {}
                _ => {}
            }
        }

        Ok(Program { statements })
    }

    /// 解析单个表达式
    pub fn parse_expression(input: &str) -> ParseResult<Expression> {
        let pairs = FactorParser::parse(Rule::expression, input).map_err(|e| {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            ParseError {
                message: e.to_string(),
                line,
                column,
            }
        })?;

        for pair in pairs {
            return Self::build_expression(pair);
        }

        Err(ParseError {
            message: "Empty expression".to_string(),
            line: 0,
            column: 0,
        })
    }

    fn build_statement(pair: pest::iterators::Pair<Rule>) -> ParseResult<Option<Statement>> {
        let inner = pair.into_inner().next();

        match inner {
            Some(inner_pair) => match inner_pair.as_rule() {
                Rule::factor_def => {
                    let def = Self::build_factor_def(inner_pair)?;
                    Ok(Some(Statement::FactorDef(def)))
                }
                Rule::assignment => {
                    let assign = Self::build_assignment(inner_pair)?;
                    Ok(Some(Statement::Assignment(assign)))
                }
                Rule::expression => {
                    let expr = Self::build_expression(inner_pair)?;
                    Ok(Some(Statement::Expression(expr)))
                }
                _ => Ok(None),
            },
            None => Ok(None),
        }
    }

    fn build_factor_def(pair: pest::iterators::Pair<Rule>) -> ParseResult<FactorDef> {
        let mut inner = pair.into_inner();

        let name = inner
            .next()
            .ok_or_else(|| ParseError {
                message: "Expected factor name".to_string(),
                line: 0,
                column: 0,
            })?
            .as_str()
            .to_string();

        let expr = Self::build_expression(inner.next().ok_or_else(|| ParseError {
            message: "Expected expression".to_string(),
            line: 0,
            column: 0,
        })?)?;

        Ok(FactorDef {
            name,
            expr,
            metadata: FactorMetadata::default(),
        })
    }

    fn build_assignment(pair: pest::iterators::Pair<Rule>) -> ParseResult<Assignment> {
        let mut inner = pair.into_inner();

        let name = inner
            .next()
            .ok_or_else(|| ParseError {
                message: "Expected variable name".to_string(),
                line: 0,
                column: 0,
            })?
            .as_str()
            .to_string();

        let expr = Self::build_expression(inner.next().ok_or_else(|| ParseError {
            message: "Expected expression".to_string(),
            line: 0,
            column: 0,
        })?)?;

        Ok(Assignment { name, expr })
    }

    fn build_expression(pair: pest::iterators::Pair<Rule>) -> ParseResult<Expression> {
        let mut inner = pair.into_inner().peekable();

        // 解析第一个 term
        let first = inner.next().ok_or_else(|| ParseError {
            message: "Expected expression term".to_string(),
            line: 0,
            column: 0,
        })?;

        let mut left = Self::build_term(first)?;

        // 解析后续的 (binary_op term)* 序列
        while inner.peek().is_some() {
            let op_pair = inner.next().unwrap();
            let right_pair = inner.next().ok_or_else(|| ParseError {
                message: "Expected right operand".to_string(),
                line: 0,
                column: 0,
            })?;

            let op = Self::build_binary_op(op_pair)?;
            let right = Self::build_term(right_pair)?;

            left = Expression::BinaryOp(Box::new(BinaryOp { op, left, right }));
        }

        Ok(left)
    }

    fn build_term(pair: pest::iterators::Pair<Rule>) -> ParseResult<Expression> {
        let mut inner = pair.into_inner().peekable();

        // 检查是否有一元运算符
        let mut unary_op: Option<UnaryOperator> = None;

        if let Some(first) = inner.peek() {
            if first.as_rule() == Rule::unary_op {
                let op_str = inner.next().unwrap().as_str();
                unary_op = Some(match op_str {
                    "-" => UnaryOperator::Neg,
                    "!" => UnaryOperator::Not,
                    _ => {
                        return Err(ParseError {
                            message: format!("Unknown unary operator: {}", op_str),
                            line: 0,
                            column: 0,
                        })
                    }
                });
            }
        }

        // 解析主体
        let main_pair = inner.next().ok_or_else(|| ParseError {
            message: "Expected term body".to_string(),
            line: 0,
            column: 0,
        })?;

        let expr = match main_pair.as_rule() {
            Rule::function_call => Self::build_function_call(main_pair)?,
            Rule::parenthesized => {
                let inner_expr = main_pair.into_inner().next().ok_or_else(|| ParseError {
                    message: "Expected expression in parentheses".to_string(),
                    line: 0,
                    column: 0,
                })?;
                Self::build_expression(inner_expr)?
            }
            Rule::literal => Self::build_literal(main_pair)?,
            Rule::identifier => Expression::Identifier(main_pair.as_str().to_string()),
            _ => {
                return Err(ParseError {
                    message: format!("Unexpected rule in term: {:?}", main_pair.as_rule()),
                    line: 0,
                    column: 0,
                })
            }
        };

        // 应用一元运算符
        if let Some(op) = unary_op {
            Ok(Expression::UnaryOp(Box::new(UnaryOp { op, operand: expr })))
        } else {
            Ok(expr)
        }
    }

    fn build_function_call(pair: pest::iterators::Pair<Rule>) -> ParseResult<Expression> {
        let mut inner = pair.into_inner();

        let name = inner
            .next()
            .ok_or_else(|| ParseError {
                message: "Expected function name".to_string(),
                line: 0,
                column: 0,
            })?
            .as_str()
            .to_string();

        let mut args = Vec::new();

        // 解析参数列表
        if let Some(arg_list) = inner.next() {
            for arg_pair in arg_list.into_inner() {
                args.push(Self::build_expression(arg_pair)?);
            }
        }

        Ok(Expression::FunctionCall(FunctionCall { name, args }))
    }

    fn build_literal(pair: pest::iterators::Pair<Rule>) -> ParseResult<Expression> {
        let inner = pair.into_inner().next().ok_or_else(|| ParseError {
            message: "Expected literal value".to_string(),
            line: 0,
            column: 0,
        })?;

        let lit = match inner.as_rule() {
            Rule::float => {
                let val: f64 = inner.as_str().parse().map_err(|_| ParseError {
                    message: format!("Invalid float: {}", inner.as_str()),
                    line: 0,
                    column: 0,
                })?;
                Literal::Float(val)
            }
            Rule::integer => {
                let val: i64 = inner.as_str().parse().map_err(|_| ParseError {
                    message: format!("Invalid integer: {}", inner.as_str()),
                    line: 0,
                    column: 0,
                })?;
                Literal::Integer(val)
            }
            Rule::string => {
                let s = inner.as_str();
                // 去掉引号
                let unquoted = &s[1..s.len() - 1];
                Literal::String(unquoted.to_string())
            }
            Rule::boolean => {
                let val = inner.as_str() == "true";
                Literal::Boolean(val)
            }
            _ => {
                return Err(ParseError {
                    message: format!("Unknown literal type: {:?}", inner.as_rule()),
                    line: 0,
                    column: 0,
                })
            }
        };

        Ok(Expression::Literal(lit))
    }

    fn build_binary_op(pair: pest::iterators::Pair<Rule>) -> ParseResult<BinaryOperator> {
        let inner = pair.into_inner().next().ok_or_else(|| ParseError {
            message: "Expected binary operator".to_string(),
            line: 0,
            column: 0,
        })?;

        let op_str = inner.as_str();
        BinaryOperator::from_str(op_str).ok_or_else(|| ParseError {
            message: format!("Unknown binary operator: {}", op_str),
            line: 0,
            column: 0,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 便捷函数
// ═══════════════════════════════════════════════════════════════════════════

/// 解析因子定义字符串
pub fn parse_factor(input: &str) -> ParseResult<Program> {
    AstBuilder::parse(input)
}

/// 解析单个表达式
pub fn parse_expression(input: &str) -> ParseResult<Expression> {
    AstBuilder::parse_expression(input)
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ma() {
        let input = "factor my_ma = ma(close, 20)";
        let program = parse_factor(input).unwrap();

        assert_eq!(program.statements.len(), 1);

        if let Statement::FactorDef(def) = &program.statements[0] {
            assert_eq!(def.name, "my_ma");

            if let Expression::FunctionCall(call) = &def.expr {
                assert_eq!(call.name, "ma");
                assert_eq!(call.args.len(), 2);
            } else {
                panic!("Expected function call");
            }
        } else {
            panic!("Expected factor definition");
        }
    }

    #[test]
    fn test_parse_arithmetic() {
        let input = "factor spread = close - open";
        let program = parse_factor(input).unwrap();

        if let Statement::FactorDef(def) = &program.statements[0] {
            if let Expression::BinaryOp(op) = &def.expr {
                assert_eq!(op.op, BinaryOperator::Sub);
            } else {
                panic!("Expected binary operation");
            }
        }
    }

    #[test]
    fn test_parse_nested_function() {
        let input = "factor zscore_ma = (ma(close, 20) - ma(close, 60)) / std(close, 20)";
        let program = parse_factor(input).unwrap();

        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_expression_only() {
        let input = "ma(close, 20) + ema(close, 10)";
        let expr = parse_expression(input).unwrap();

        if let Expression::BinaryOp(op) = expr {
            assert_eq!(op.op, BinaryOperator::Add);
        } else {
            panic!("Expected binary operation");
        }
    }
}
