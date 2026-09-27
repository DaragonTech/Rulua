//! AST node types for the Lua grammar.
//!
//! The AST is an intermediate representation between parsing and code generation.
//! rilua uses an explicit AST (following Luau's approach) rather than PUC-Rio's
//! single-pass parser-compiler. This separation gives clearer error boundaries
//! and independent testability per phase.

use super::token::Span;

/// A block is a sequence of statements.
pub type Block = Vec<Stat>;

/// A statement in Lua.
#[derive(Debug, Clone)]
pub enum Stat {
    /// `var1, var2, ... = expr1, expr2, ...`
    Assign {
        targets: Vec<Expr>,
        values: Vec<Expr>,
        span: Span,
    },
    /// `local name1, name2, ... = expr1, expr2, ...`
    LocalDecl {
        names: Vec<String>,
        values: Vec<Expr>,
        span: Span,
    },
    /// `do ... end`
    Do {
        body: Block,
        span: Span,
        /// Line number of the closing `end` keyword.
        end_line: u32,
    },
    /// `while expr do ... end`
    While {
        condition: Expr,
        body: Block,
        span: Span,
        /// Line number of the closing `end` keyword.
        end_line: u32,
    },
    /// `repeat ... until expr`
    Repeat {
        body: Block,
        condition: Expr,
        span: Span,
    },
    /// `if expr then ... {elseif expr then ...} [else ...] end`
    If {
        /// Condition-body pairs: the first is the `if`, rest are `elseif`.
        conditions: Vec<Expr>,
        bodies: Vec<Block>,
        /// Optional `else` block.
        else_body: Option<Block>,
        span: Span,
        /// Line number of the closing `end` keyword.
        end_line: u32,
    },
    /// `for name = start, stop[, step] do ... end`
    NumericFor {
        name: String,
        start: Expr,
        stop: Expr,
        step: Option<Expr>,
        body: Block,
        span: Span,
        /// Line number of the closing `end` keyword.
        end_line: u32,
    },
    /// `for name1, name2, ... in expr1, expr2, ... do ... end`
    GenericFor {
        names: Vec<String>,
        iterators: Vec<Expr>,
        body: Block,
        /// Line number after `in` (where iterator expressions start).
        /// Used by codegen for TFORLOOP line info (PUC-Rio `luaK_fixline`).
        iter_line: u32,
        span: Span,
        /// Line number of the closing `end` keyword.
        end_line: u32,
    },
    /// `function funcname funcbody`
    FuncDecl {
        name: FuncName,
        body: FuncBody,
        span: Span,
    },
    /// `local function name funcbody`
    LocalFunc {
        name: String,
        body: FuncBody,
        span: Span,
    },
    /// `return [explist]`
    Return { values: Vec<Expr>, span: Span },
    /// `break`
    Break { span: Span },
    /// An expression used as a statement (function call).
    ExprStat { expr: Expr, span: Span },
}

/// An expression in Lua.
#[derive(Debug, Clone)]
pub enum Expr {
    /// `nil`
    Nil(Span),
    /// `true`
    True(Span),
    /// `false`
    False(Span),
    /// Numeric literal.
    Number(f64, Span),
    /// String literal (raw bytes; Lua strings may contain arbitrary bytes).
    Str(Vec<u8>, Span),
    /// `...` (vararg expression).
    VarArg(Span),
    /// Variable name reference.
    Name(String, Span),
    /// Binary operation: `left op right`.
    BinOp {
        op: BinOp,
        left: Box<Self>,
        right: Box<Self>,
        span: Span,
    },
    /// Unary operation: `op operand`.
    UnOp {
        op: UnOp,
        operand: Box<Self>,
        span: Span,
    },
    /// Table index: `table[key]`.
    Index {
        table: Box<Self>,
        key: Box<Self>,
        span: Span,
    },
    /// Field access: `table.field` (desugars to `table["field"]`).
    Field {
        table: Box<Self>,
        field: String,
        span: Span,
    },
    /// Method call: `table:method(args)`.
    MethodCall {
        table: Box<Self>,
        method: String,
        args: Vec<Self>,
        span: Span,
    },
    /// Function call: `func(args)`.
    Call {
        func: Box<Self>,
        args: Vec<Self>,
        span: Span,
    },
    /// Function definition: `function(params) ... end`.
    FuncDef { body: FuncBody, span: Span },
    /// Table constructor: `{ fields }`.
    TableCtor { fields: Vec<TableField>, span: Span },
    /// Parenthesized expression: `(expr)`.
    /// Forces multi-return expressions (calls, vararg) to produce exactly
    /// one value. Maps to PUC-Rio's `prefixexp → '(' expr ')'` which calls
    /// `luaK_dischargevars` (triggering `luaK_setoneret`).
    Paren(Box<Self>, Span),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `-` (negation)
    Neg,
    /// `not`
    Not,
    /// `#` (length)
    Len,
}

/// A field in a table constructor.
#[derive(Debug, Clone)]
pub enum TableField {
    /// `[expr] = expr`
    IndexField { key: Expr, value: Expr, span: Span },
    /// `name = expr`
    NameField {
        name: String,
        value: Expr,
        span: Span,
    },
    /// `expr` (positional/array element)
    ValueField { value: Expr, span: Span },
}

/// Function body: parameters and block.
#[derive(Debug, Clone)]
pub struct FuncBody {
    /// Parameter names.
    pub params: Vec<String>,
    /// Whether the function accepts varargs (`...`).
    pub has_varargs: bool,
    /// Function body statements.
    pub body: Block,
    /// Span covering the entire function definition.
    pub span: Span,
    /// Line number of the closing `end` keyword.
    pub end_line: u32,
}

/// Function name for `function` declarations.
///
/// Represents `NAME { '.' NAME } [ ':' NAME ]`.
#[derive(Debug, Clone)]
pub struct FuncName {
    /// Dotted path components (at least one).
    pub parts: Vec<String>,
    /// Optional method name (after `:`).
    pub method: Option<String>,
    /// Span of the function name.
    pub span: Span,
}

impl Stat {
    /// Returns the span of this statement.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Assign { span, .. }
            | Self::LocalDecl { span, .. }
            | Self::Do { span, .. }
            | Self::While { span, .. }
            | Self::Repeat { span, .. }
            | Self::If { span, .. }
            | Self::NumericFor { span, .. }
            | Self::GenericFor { span, .. }
            | Self::FuncDecl { span, .. }
            | Self::LocalFunc { span, .. }
            | Self::Return { span, .. }
            | Self::Break { span, .. }
            | Self::ExprStat { span, .. } => *span,
        }
    }
}

impl Expr {
    /// Returns the span of this expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Nil(span)
            | Self::True(span)
            | Self::False(span)
            | Self::Number(_, span)
            | Self::Str(_, span)
            | Self::VarArg(span)
            | Self::Name(_, span)
            | Self::BinOp { span, .. }
            | Self::UnOp { span, .. }
            | Self::Index { span, .. }
            | Self::Field { span, .. }
            | Self::MethodCall { span, .. }
            | Self::Call { span, .. }
            | Self::FuncDef { span, .. }
            | Self::TableCtor { span, .. }
            | Self::Paren(_, span) => *span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_span() {
        let s = Stat::Break {
            span: Span::new(5, 1),
        };
        assert_eq!(s.span(), Span::new(5, 1));
    }

    #[test]
    fn expr_span() {
        let e = Expr::Number(42.0, Span::new(1, 1));
        assert_eq!(e.span(), Span::new(1, 1));
    }

    #[test]
    fn expr_nil_span() {
        let e = Expr::Nil(Span::new(3, 10));
        assert_eq!(e.span(), Span::new(3, 10));
    }

    #[test]
    fn binop_expr_span() {
        let e = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Number(1.0, Span::new(1, 1))),
            right: Box::new(Expr::Number(2.0, Span::new(1, 5))),
            span: Span::new(1, 1),
        };
        assert_eq!(e.span(), Span::new(1, 1));
    }

    #[test]
    fn unop_expr_span() {
        let e = Expr::UnOp {
            op: UnOp::Neg,
            operand: Box::new(Expr::Number(1.0, Span::new(1, 2))),
            span: Span::new(1, 1),
        };
        assert_eq!(e.span(), Span::new(1, 1));
    }

    #[test]
    fn func_name_simple() {
        let name = FuncName {
            parts: vec!["foo".into()],
            method: None,
            span: Span::new(1, 1),
        };
        assert_eq!(name.parts.len(), 1);
        assert!(name.method.is_none());
    }

    #[test]
    fn func_name_dotted_with_method() {
        let name = FuncName {
            parts: vec!["a".into(), "b".into()],
            method: Some("c".into()),
            span: Span::new(1, 1),
        };
        assert_eq!(name.parts.len(), 2);
        assert_eq!(name.method.as_deref(), Some("c"));
    }

    #[test]
    fn func_body_construction() {
        let body = FuncBody {
            params: vec!["x".into(), "y".into()],
            has_varargs: true,
            body: vec![],
            span: Span::new(1, 1),
            end_line: 5,
        };
        assert_eq!(body.params.len(), 2);
        assert!(body.has_varargs);
    }

    #[test]
    fn table_field_variants() {
        let _idx = TableField::IndexField {
            key: Expr::Number(1.0, Span::new(1, 2)),
            value: Expr::Str(b"a".to_vec(), Span::new(1, 7)),
            span: Span::new(1, 1),
        };
        let _name = TableField::NameField {
            name: "x".into(),
            value: Expr::Number(1.0, Span::new(1, 5)),
            span: Span::new(1, 1),
        };
        let _val = TableField::ValueField {
            value: Expr::Number(1.0, Span::new(1, 1)),
            span: Span::new(1, 1),
        };
    }

    #[test]
    fn call_expr() {
        let e = Expr::Call {
            func: Box::new(Expr::Name("print".into(), Span::new(1, 1))),
            args: vec![Expr::Str(b"hello".to_vec(), Span::new(1, 7))],
            span: Span::new(1, 1),
        };
        assert_eq!(e.span(), Span::new(1, 1));
    }
}
