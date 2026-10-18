//! Reqly template language: variables, functions (template tags) and faker.

pub mod ast;
pub mod functions;
mod parser;
mod render;

pub use functions::{FunctionInfo, catalog, dynamic_variables};
pub use parser::{has_template, parse};
pub use render::{Host, Locale, RenderOptions, Renderer, lookup, path_name, value_to_string};
