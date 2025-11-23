//! 因子 DSL 模块
//!
//! @yutiansut @quantaxis
//!
//! 提供因子定义语言 (DSL) 的完整实现：
//! - 语法定义 (grammar.pest)
//! - AST 结构 (ast.rs)
//! - 解析器 (parser.rs)
//! - 执行引擎 (executor/)

pub mod ast;
pub mod executor;
pub mod parser;

pub use ast::*;
pub use executor::*;
pub use parser::*;
