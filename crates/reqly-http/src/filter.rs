//! Response body filters: JSONPath (RFC 9535), jq (via jaq) and XPath 1.0.

use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FilterKind {
    JsonPath,
    Jq,
    XPath,
}

fn syntax(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("filter.syntax", e.to_string()).with_param("detail", e.to_string())
}

fn input(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("filter.input", e.to_string()).with_param("detail", e.to_string())
}

fn runtime(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("filter.runtime", e.to_string()).with_param("detail", e.to_string())
}

pub fn apply_filter(kind: FilterKind, body: &str, expression: &str) -> CoreResult<String> {
    match kind {
        FilterKind::JsonPath => json_path(body, expression),
        FilterKind::Jq => jq(body, expression),
        FilterKind::XPath => xpath(body, expression),
    }
}

/// Values a JSONPath or jq expression selects from a JSON body, for
/// assertions. XPath is not supported here.
pub fn query_json(
    kind: FilterKind,
    body: &str,
    expression: &str,
) -> CoreResult<Vec<serde_json::Value>> {
    match kind {
        FilterKind::JsonPath => json_path_values(body, expression),
        FilterKind::Jq => jq_values(body, expression)?
            .into_iter()
            .map(|v| serde_json::from_str(&v).map_err(runtime))
            .collect(),
        FilterKind::XPath => Err(CoreError::new(
            "filter.unsupported",
            "XPath does not produce JSON values",
        )),
    }
}

fn json_path_values(body: &str, expression: &str) -> CoreResult<Vec<serde_json::Value>> {
    let path = serde_json_path::JsonPath::parse(expression).map_err(syntax)?;
    let value: serde_json::Value = serde_json::from_str(body).map_err(input)?;
    Ok(path.query(&value).all().into_iter().cloned().collect())
}

fn json_path(body: &str, expression: &str) -> CoreResult<String> {
    let nodes = json_path_values(body, expression)?;
    let pretty = |v: &serde_json::Value| serde_json::to_string_pretty(v).unwrap_or_default();
    Ok(match nodes.as_slice() {
        [single] => pretty(single),
        many => pretty(&serde_json::Value::Array(many.to_vec())),
    })
}

fn jq(body: &str, expression: &str) -> CoreResult<String> {
    Ok(jq_values(body, expression)?.join("\n"))
}

/// Each output of a jq program, as compact JSON.
fn jq_values(body: &str, expression: &str) -> CoreResult<Vec<String>> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
    use jaq_json::{Val, read};

    let input_val = read::parse_single(body.as_bytes()).map_err(input)?;
    let program = File {
        code: expression,
        path: (),
    };
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(&arena, program)
        .map_err(|errs| syntax(format!("{errs:?}")))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| syntax(format!("{errs:?}")))?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut out = Vec::new();
    for result in filter.id.run((ctx, input_val)).map(unwrap_valr) {
        match result {
            Ok(v) => out.push(v.to_string()),
            Err(e) => return Err(runtime(e)),
        }
        if out.len() > 10_000 {
            break;
        }
    }
    Ok(out)
}

fn escape_xml(s: &str, attr: bool) -> String {
    let mut out = s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attr {
        out = out.replace('"', "&quot;");
    }
    out
}

fn render_node(node: sxd_xpath::nodeset::Node<'_>, out: &mut String) {
    use sxd_xpath::nodeset::Node;
    match node {
        Node::Element(e) => {
            let name = e.name().local_part();
            out.push('<');
            out.push_str(name);
            for a in e.attributes() {
                out.push_str(&format!(
                    " {}=\"{}\"",
                    a.name().local_part(),
                    escape_xml(a.value(), true)
                ));
            }
            let children = e.children();
            if children.is_empty() {
                out.push_str("/>");
                return;
            }
            out.push('>');
            for child in children {
                render_node(child.into(), out);
            }
            out.push_str(&format!("</{name}>"));
        }
        Node::Attribute(a) => out.push_str(&format!(
            "{}=\"{}\"",
            a.name().local_part(),
            escape_xml(a.value(), true)
        )),
        Node::Text(t) => out.push_str(&escape_xml(t.text(), false)),
        Node::Comment(c) => out.push_str(&format!("<!--{}-->", c.text())),
        other => out.push_str(&escape_xml(&other.string_value(), false)),
    }
}

fn xpath(body: &str, expression: &str) -> CoreResult<String> {
    use sxd_xpath::{Context, Factory, Value};

    let package = sxd_document::parser::parse(body).map_err(input)?;
    let document = package.as_document();
    let xpath = Factory::new()
        .build(expression)
        .map_err(syntax)?
        .ok_or_else(|| syntax("empty expression"))?;
    let value = xpath
        .evaluate(&Context::new(), document.root())
        .map_err(runtime)?;
    Ok(match value {
        Value::Nodeset(nodes) => nodes
            .document_order()
            .into_iter()
            .map(|n| {
                let mut s = String::new();
                render_node(n, &mut s);
                s
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", n as i64)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => s,
    })
}
