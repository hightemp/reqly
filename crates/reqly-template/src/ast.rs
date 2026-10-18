use std::ops::Range;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum PathPart {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Variable reference, e.g. `api.version` or `items[0]`.
    Var(Vec<PathPart>),
    /// Function / template tag call, e.g. `uuid('v4')`.
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Lit(Value),
    /// `[a, 'b', 1]`
    Array(Vec<Expr>),
    /// `{ min: 1, 'max': n }`
    Object(Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Expr {
        expr: Expr,
        /// Byte range of the whole `{{ … }}` / `{% … %}` in the source.
        span: Range<usize>,
    },
}
