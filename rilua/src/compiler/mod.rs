//! Compilation pipeline: source code to bytecode.
//!
//! Phases: Lexer (source -> tokens) -> Parser (tokens -> AST)
//! -> Codegen (AST -> Proto).

pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod token;

pub use codegen::{compile, compile_with_lexer};
