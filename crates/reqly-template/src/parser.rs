//! Template parser.
//!
//! ```text
//! {{ var.path[0] }}            variable (Insomnia's `_.` prefix is accepted)
//! {{ $name(args) }}            function call
//! {{ $faker.person.firstName }} faker shorthand → faker('person.firstName')
//! {% name arg, arg %}          template tag (Insomnia style)
//! {# comment #}                ignored
//! \{{                          literal "{{"
//! ```

use reqly_core::{CoreError, CoreResult};
use serde_json::{Number, Value};

use crate::ast::{Expr, PathPart, Segment};

fn error(offset: usize, message: &str) -> CoreError {
    CoreError::new("template.syntax", message.to_string()).with_param("offset", offset as u64)
}

struct Cursor<'a> {
    src: &'a str,
    pos: usize,
    /// Start of the enclosing tag, for error positions.
    tag_start: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn err(&self, message: &str) -> CoreError {
        error(self.tag_start, message).with_param("at", self.pos as u64)
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            self.bump();
        }
        (self.pos > start).then(|| self.src[start..self.pos].to_string())
    }

    fn string(&mut self, quote: char) -> CoreResult<String> {
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some(c) => out.push(c),
                    None => return Err(self.err("unterminated string")),
                },
                Some(c) if c == quote => return Ok(out),
                Some(c) => out.push(c),
            }
        }
    }

    fn number(&mut self) -> CoreResult<Value> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.bump();
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E')
        {
            self.bump();
        }
        let text = &self.src[start..self.pos];
        if let Ok(i) = text.parse::<i64>() {
            return Ok(Value::Number(i.into()));
        }
        text.parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| self.err("invalid number"))
    }

    /// `name`, `name.more`, `name[0]`, `name['key']`.
    fn path(&mut self, first: String) -> CoreResult<Vec<PathPart>> {
        let mut parts = vec![PathPart::Key(first)];
        loop {
            if self.eat(".") {
                let key = self
                    .ident()
                    .ok_or_else(|| self.err("expected a name after '.'"))?;
                parts.push(PathPart::Key(key));
            } else if self.eat("[") {
                self.skip_ws();
                let part = match self.peek() {
                    Some(q @ ('\'' | '"')) => {
                        self.bump();
                        PathPart::Key(self.string(q)?)
                    }
                    Some(c) if c.is_ascii_digit() => {
                        let n = self.number()?;
                        PathPart::Index(
                            n.as_u64().ok_or_else(|| self.err("invalid index"))? as usize
                        )
                    }
                    _ => return Err(self.err("expected an index or quoted key")),
                };
                self.skip_ws();
                if !self.eat("]") {
                    return Err(self.err("expected ']'"));
                }
                parts.push(part);
            } else {
                break;
            }
        }
        // Insomnia addresses environment values as `_.name`.
        if matches!(parts.first(), Some(PathPart::Key(k)) if k == "_") && parts.len() > 1 {
            parts.remove(0);
        }
        Ok(parts)
    }

    fn args(&mut self, close: &str) -> CoreResult<Vec<Expr>> {
        let mut args = Vec::new();
        self.skip_ws();
        if self.rest().starts_with(close) {
            return Ok(args);
        }
        loop {
            self.skip_ws();
            args.push(self.expr()?);
            self.skip_ws();
            if !self.eat(",") {
                break;
            }
        }
        Ok(args)
    }

    fn expr(&mut self) -> CoreResult<Expr> {
        self.skip_ws();
        match self.peek() {
            Some(q @ ('\'' | '"')) => {
                self.bump();
                Ok(Expr::Lit(Value::String(self.string(q)?)))
            }
            Some(c) if c.is_ascii_digit() || c == '-' => Ok(Expr::Lit(self.number()?)),
            Some('$') => {
                self.bump();
                let mut name = self
                    .ident()
                    .ok_or_else(|| self.err("expected a function name"))?;
                // `$faker.person.firstName` → faker('person.firstName', …)
                let mut faker_path = None;
                if name == "faker" && self.eat(".") {
                    let mut path = self
                        .ident()
                        .ok_or_else(|| self.err("expected a faker function"))?;
                    while self.eat(".") {
                        path.push('.');
                        path.push_str(
                            &self
                                .ident()
                                .ok_or_else(|| self.err("expected a name after '.'"))?,
                        );
                    }
                    faker_path = Some(path);
                } else {
                    while self.rest().starts_with('.') && !self.rest().starts_with("..") {
                        self.bump();
                        name.push('.');
                        name.push_str(
                            &self
                                .ident()
                                .ok_or_else(|| self.err("expected a name after '.'"))?,
                        );
                    }
                }
                let mut args = Vec::new();
                if let Some(path) = faker_path {
                    args.push(Expr::Lit(Value::String(path)));
                }
                if self.eat("(") {
                    args.extend(self.args(")")?);
                    self.skip_ws();
                    if !self.eat(")") {
                        return Err(self.err("expected ')'"));
                    }
                }
                Ok(Expr::Call { name, args })
            }
            Some(_) => {
                let first = self
                    .ident()
                    .ok_or_else(|| self.err("expected an expression"))?;
                match first.as_str() {
                    "true" => return Ok(Expr::Lit(Value::Bool(true))),
                    "false" => return Ok(Expr::Lit(Value::Bool(false))),
                    "null" => return Ok(Expr::Lit(Value::Null)),
                    _ => {}
                }
                if first == "_" && self.rest().starts_with('[') {
                    return Ok(Expr::Var(self.path(first)?));
                }
                Ok(Expr::Var(self.path(first)?))
            }
            None => Err(self.err("unexpected end")),
        }
    }
}

pub fn parse(src: &str) -> CoreResult<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut c = Cursor {
        src,
        pos: 0,
        tag_start: 0,
    };

    let flush = |text: &mut String, segments: &mut Vec<Segment>| {
        if !text.is_empty() {
            segments.push(Segment::Text(std::mem::take(text)));
        }
    };

    while c.pos < src.len() {
        let rest = c.rest();
        if rest.starts_with("\\{{") || rest.starts_with("\\{%") {
            c.pos += 1;
            text.push_str(&rest[1..3]);
            c.pos += 2;
        } else if rest.starts_with("{#") {
            let end = rest
                .find("#}")
                .ok_or_else(|| error(c.pos, "unterminated comment"))?;
            flush(&mut text, &mut segments);
            c.pos += end + 2;
        } else if rest.starts_with("{{") {
            flush(&mut text, &mut segments);
            let start = c.pos;
            c.tag_start = start;
            c.pos += 2;
            let expr = c.expr()?;
            c.skip_ws();
            if !c.eat("}}") {
                return Err(error(start, "expected '}}'"));
            }
            segments.push(Segment::Expr {
                expr,
                span: start..c.pos,
            });
        } else if rest.starts_with("{%") {
            flush(&mut text, &mut segments);
            let start = c.pos;
            c.tag_start = start;
            c.pos += 2;
            c.skip_ws();
            let name = c
                .ident()
                .ok_or_else(|| error(start, "expected a tag name"))?;
            let args = c.args("%}")?;
            c.skip_ws();
            if !c.eat("%}") {
                return Err(error(start, "expected '%}'"));
            }
            segments.push(Segment::Expr {
                expr: Expr::Call { name, args },
                span: start..c.pos,
            });
        } else {
            let ch = c.bump().expect("not at end");
            text.push(ch);
        }
    }
    flush(&mut text, &mut segments);
    Ok(segments)
}

/// True when `src` contains template syntax (fast path: no parsing needed).
pub fn has_template(src: &str) -> bool {
    src.contains("{{") || src.contains("{%") || src.contains("{#")
}
