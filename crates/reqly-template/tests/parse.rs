use reqly_template::ast::{Expr, PathPart, Segment};
use reqly_template::parse;
use serde_json::json;

fn exprs(src: &str) -> Vec<Expr> {
    parse(src)
        .unwrap()
        .into_iter()
        .filter_map(|s| match s {
            Segment::Expr { expr, .. } => Some(expr),
            Segment::Text(_) => None,
        })
        .collect()
}

#[test]
fn plain_text_is_one_segment() {
    assert_eq!(
        parse("hello world").unwrap(),
        vec![Segment::Text("hello world".into())]
    );
    assert_eq!(parse("").unwrap(), vec![]);
}

#[test]
fn variables_with_paths_and_insomnia_prefix() {
    let e = exprs("{{base_url}}/x/{{ _.api.version }}/{{ items[0].id }}/{{ _['odd key'] }}");
    assert_eq!(e[0], Expr::Var(vec![PathPart::Key("base_url".into())]));
    assert_eq!(
        e[1],
        Expr::Var(vec![
            PathPart::Key("api".into()),
            PathPart::Key("version".into())
        ])
    );
    assert_eq!(
        e[2],
        Expr::Var(vec![
            PathPart::Key("items".into()),
            PathPart::Index(0),
            PathPart::Key("id".into())
        ])
    );
    assert_eq!(e[3], Expr::Var(vec![PathPart::Key("odd key".into())]));
}

#[test]
fn dollar_calls_and_faker_shorthand() {
    let e = exprs(
        "{{ $uuid }} {{ $random('int', 1, 10) }} {{ $faker.person.firstName }} {{ $faker.number.int(1, 5) }}",
    );
    assert_eq!(
        e[0],
        Expr::Call {
            name: "uuid".into(),
            args: vec![]
        }
    );
    assert_eq!(
        e[1],
        Expr::Call {
            name: "random".into(),
            args: vec![
                Expr::Lit(json!("int")),
                Expr::Lit(json!(1)),
                Expr::Lit(json!(10))
            ]
        }
    );
    assert_eq!(
        e[2],
        Expr::Call {
            name: "faker".into(),
            args: vec![Expr::Lit(json!("person.firstName"))]
        }
    );
    assert_eq!(
        e[3],
        Expr::Call {
            name: "faker".into(),
            args: vec![
                Expr::Lit(json!("number.int")),
                Expr::Lit(json!(1)),
                Expr::Lit(json!(5))
            ]
        }
    );
}

#[test]
fn insomnia_tags_with_mixed_arguments() {
    let e = exprs(r#"{% hash 'sha256', "hex", token, 1.5, true, null %}{% uuid %}"#);
    assert_eq!(
        e[0],
        Expr::Call {
            name: "hash".into(),
            args: vec![
                Expr::Lit(json!("sha256")),
                Expr::Lit(json!("hex")),
                Expr::Var(vec![PathPart::Key("token".into())]),
                Expr::Lit(json!(1.5)),
                Expr::Lit(json!(true)),
                Expr::Lit(serde_json::Value::Null),
            ]
        }
    );
    assert_eq!(
        e[1],
        Expr::Call {
            name: "uuid".into(),
            args: vec![]
        }
    );
}

#[test]
fn string_escapes_and_nested_calls() {
    let e = exprs(r#"{% base64 'encode', 'it\'s', $now('unix') %}"#);
    assert_eq!(
        e[0],
        Expr::Call {
            name: "base64".into(),
            args: vec![
                Expr::Lit(json!("encode")),
                Expr::Lit(json!("it's")),
                Expr::Call {
                    name: "now".into(),
                    args: vec![Expr::Lit(json!("unix"))]
                }
            ]
        }
    );
}

#[test]
fn spans_cover_the_whole_tag() {
    let src = "a{{ x }}b{% uuid %}";
    let spans: Vec<_> = parse(src)
        .unwrap()
        .into_iter()
        .filter_map(|s| match s {
            Segment::Expr { span, .. } => Some(&src[span]),
            Segment::Text(_) => None,
        })
        .collect();
    assert_eq!(spans, ["{{ x }}", "{% uuid %}"]);
}

#[test]
fn comments_and_escapes() {
    assert_eq!(
        parse("a{# hidden #}b").unwrap(),
        vec![Segment::Text("a".into()), Segment::Text("b".into())]
    );
    assert_eq!(
        parse(r"\{{ not a var }}").unwrap(),
        vec![Segment::Text("{{ not a var }}".into())]
    );
}

#[test]
fn json_bodies_with_single_braces_are_text() {
    let body = r#"{"a": {"b": [1, {}]}}"#;
    assert_eq!(parse(body).unwrap(), vec![Segment::Text(body.into())]);
}

#[test]
fn errors_have_positions() {
    let err = parse("ok {{ x ").unwrap_err();
    assert_eq!(err.code, "template.syntax");
    assert_eq!(err.params["offset"], 3);
    assert!(parse("{{ }}").is_err());
    assert!(parse("{{ a.. }}").is_err());
    assert!(parse("{% %}").is_err());
    assert!(parse("{{ $f('x' }}").is_err());
}

mod props {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn text_without_markers_round_trips(s in "[^{}\\\\]{0,40}") {
            let parsed = reqly_template::parse(&s).unwrap();
            let joined: String = parsed.iter().map(|seg| match seg {
                reqly_template::ast::Segment::Text(t) => t.clone(),
                _ => unreachable!(),
            }).collect();
            prop_assert_eq!(joined, s);
        }
    }
}
