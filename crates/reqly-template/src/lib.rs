//! Reqly template language: variables, functions (template tags) and faker.

pub mod ast;
mod faker;
pub mod functions;
mod parser;
mod render;

pub use faker::FAKER_FUNCTIONS;
pub use functions::{FunctionInfo, catalog};
pub use parser::{has_template, parse};
pub use render::{Host, Locale, RenderOptions, Renderer, lookup, path_name, value_to_string};
