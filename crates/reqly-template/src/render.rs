//! Rendering: resolves variables (recursively, with cycle detection) and
//! calls functions — built-in pure ones here, context-dependent ones
//! (cookies, other responses, prompts…) through a [`Host`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use async_trait::async_trait;
use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::ast::{Expr, PathPart, Segment};
use crate::functions;
use crate::parser::{has_template, parse};

const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Locale {
    #[default]
    En,
    Ru,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Undefined variables are errors (otherwise they render as empty).
    pub strict: bool,
    /// The same call (e.g. `{{ $uuid }}`) yields one value per render.
    pub stable: bool,
    pub locale: Locale,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            strict: true,
            stable: false,
            locale: Locale::En,
        }
    }
}

/// Functions that need application context.
#[async_trait]
pub trait Host: Send + Sync {
    /// `None` when the host does not know `name`.
    async fn call(&self, name: &str, args: &[Value]) -> Option<CoreResult<Value>>;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct Renderer<'a> {
    vars: &'a Value,
    host: &'a dyn Host,
    options: RenderOptions,
    cache: Mutex<HashMap<String, Value>>,
}

/// Text form of a value: strings as-is, null empty, everything else JSON.
pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn path_name(path: &[PathPart]) -> String {
    let mut out = String::new();
    for (i, p) in path.iter().enumerate() {
        match p {
            PathPart::Key(k) if i == 0 => out.push_str(k),
            PathPart::Key(k) => {
                out.push('.');
                out.push_str(k);
            }
            PathPart::Index(n) => out.push_str(&format!("[{n}]")),
        }
    }
    out
}

pub fn lookup<'v>(vars: &'v Value, path: &[PathPart]) -> Option<&'v Value> {
    path.iter().try_fold(vars, |cur, part| match part {
        PathPart::Key(k) => cur.get(k),
        PathPart::Index(i) => cur.get(*i),
    })
}

impl<'a> Renderer<'a> {
    pub fn new(vars: &'a Value, host: &'a dyn Host, options: RenderOptions) -> Self {
        Self {
            vars,
            host,
            options,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn render(&self, src: &str) -> CoreResult<String> {
        if !has_template(src) {
            return Ok(src.to_string());
        }
        self.render_inner(src.to_string(), Vec::new()).await
    }

    /// Evaluates a single expression source like `{{ x }}` to a JSON value.
    pub async fn evaluate(&self, src: &str) -> CoreResult<Value> {
        let segments = parse(src)?;
        match segments.as_slice() {
            [Segment::Expr { expr, .. }] => self.eval(expr.clone(), Vec::new()).await,
            _ => Ok(Value::String(self.render(src).await?)),
        }
    }

    fn render_inner(&self, src: String, stack: Vec<String>) -> BoxFuture<'_, CoreResult<String>> {
        Box::pin(async move {
            if stack.len() > MAX_DEPTH {
                return Err(CoreError::new(
                    "template.too_deep",
                    "templates nest too deeply",
                ));
            }
            let mut out = String::new();
            for segment in parse(&src)? {
                match segment {
                    Segment::Text(t) => out.push_str(&t),
                    Segment::Expr { expr, .. } => {
                        out.push_str(&value_to_string(&self.eval(expr, stack.clone()).await?))
                    }
                }
            }
            Ok(out)
        })
    }

    fn eval(&self, expr: Expr, stack: Vec<String>) -> BoxFuture<'_, CoreResult<Value>> {
        Box::pin(async move {
            match expr {
                Expr::Lit(v) => Ok(v),
                Expr::Var(path) => {
                    let name = path_name(&path);
                    let Some(value) = lookup(self.vars, &path) else {
                        if self.options.strict {
                            return Err(CoreError::new(
                                "template.undefined",
                                format!("variable {name} is not defined"),
                            )
                            .with_param("name", name));
                        }
                        return Ok(Value::Null);
                    };
                    match value {
                        Value::String(s) if has_template(s) => {
                            if stack.contains(&name) {
                                return Err(CoreError::new(
                                    "template.cycle",
                                    "variables reference each other",
                                )
                                .with_param("name", name)
                                .with_param("chain", stack.join(" → ")));
                            }
                            let mut next = stack;
                            next.push(name);
                            Ok(Value::String(self.render_inner(s.clone(), next).await?))
                        }
                        other => Ok(other.clone()),
                    }
                }
                Expr::Call { name, args } => {
                    let mut values = Vec::with_capacity(args.len());
                    for arg in args {
                        values.push(self.eval(arg, stack.clone()).await?);
                    }
                    let (name, values) = functions::resolve_alias(&name, values);
                    let key = self.options.stable.then(|| format!("{name}{values:?}"));
                    if let Some(key) = &key
                        && let Some(hit) = self.cache.lock().ok().and_then(|c| c.get(key).cloned())
                    {
                        return Ok(hit);
                    }
                    let result = match functions::call(&name, &values, self.options.locale) {
                        Some(r) => r,
                        None => match self.host.call(&name, &values).await {
                            Some(r) => r,
                            None => {
                                return Err(CoreError::new(
                                    "template.unknown_function",
                                    format!("unknown function {name}"),
                                )
                                .with_param("name", name));
                            }
                        },
                    }?;
                    if let Some(key) = key
                        && let Ok(mut cache) = self.cache.lock()
                    {
                        cache.insert(key, result.clone());
                    }
                    Ok(result)
                }
            }
        })
    }
}
