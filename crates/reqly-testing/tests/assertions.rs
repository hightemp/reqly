use reqly_core::model::{Assertion, AssertionOperator as Op, AssertionSource as Src};
use reqly_testing::{AssertionResult, ResponseFacts, evaluate, evaluate_all};

const BODY: &str =
    r#"{"id":7,"name":"Ada","tags":["admin","dev"],"profile":{"age":36},"score":9.5}"#;

fn headers() -> Vec<(String, String)> {
    vec![
        (
            "Content-Type".into(),
            "application/json; charset=utf-8".into(),
        ),
        ("Set-Cookie".into(), "a=1".into()),
        ("set-cookie".into(), "b=2".into()),
    ]
}

fn check(source: Src, property: &str, op: Op, value: &str) -> AssertionResult {
    let headers = headers();
    let facts = ResponseFacts {
        status: Some(201),
        total_ms: Some(120.5),
        headers: &headers,
        body: BODY,
    };
    evaluate(&Assertion::new(source, property, op, value), &facts)
}

fn passes(source: Src, property: &str, op: Op, value: &str) -> bool {
    let r = check(source, property, op, value);
    assert!(r.error.is_none(), "unexpected error: {:?}", r.error);
    r.passed
}

fn error_code(source: Src, property: &str, op: Op, value: &str) -> String {
    let r = check(source, property, op, value);
    assert!(!r.passed);
    r.error.expect("an error").code
}

#[test]
fn status() {
    assert!(passes(Src::Status, "", Op::Equals, "201"));
    assert!(!passes(Src::Status, "", Op::Equals, "200"));
    assert!(passes(Src::Status, "", Op::NotEquals, "200"));
    assert!(passes(Src::Status, "", Op::LessThan, "300"));
    assert!(passes(Src::Status, "", Op::GreaterThanOrEqual, "201"));
    assert!(!passes(Src::Status, "", Op::GreaterThan, "201"));
    assert!(passes(Src::Status, "", Op::Matches, "^2\\d\\d$"));
    assert!(passes(Src::Status, "", Op::Exists, ""));
    assert_eq!(
        check(Src::Status, "", Op::Equals, "200").actual.as_deref(),
        Some("201")
    );
}

#[test]
fn missing_status_fails_but_not_exists_passes() {
    let facts = ResponseFacts {
        status: None,
        total_ms: None,
        headers: &[],
        body: "",
    };
    let eq = evaluate(&Assertion::new(Src::Status, "", Op::Equals, "200"), &facts);
    assert!(!eq.passed);
    assert_eq!(eq.actual, None);
    let none = evaluate(&Assertion::new(Src::Status, "", Op::NotExists, ""), &facts);
    assert!(none.passed);
}

#[test]
fn response_time() {
    assert!(passes(Src::ResponseTime, "", Op::LessThan, "500"));
    assert!(passes(Src::ResponseTime, "", Op::LessThanOrEqual, "120.5"));
    assert!(!passes(Src::ResponseTime, "", Op::LessThan, "100"));
    assert!(passes(Src::ResponseTime, "", Op::GreaterThan, "100"));
}

#[test]
fn comparing_with_a_non_number_is_an_error() {
    assert_eq!(
        error_code(Src::ResponseTime, "", Op::LessThan, "fast"),
        "assertion.notNumber"
    );
    assert_eq!(
        error_code(Src::JsonPath, "$.name", Op::GreaterThan, "1"),
        "assertion.notNumber"
    );
}

#[test]
fn header() {
    assert!(passes(Src::Header, "content-type", Op::Exists, ""));
    assert!(passes(Src::Header, "X-Missing", Op::NotExists, ""));
    assert!(!passes(Src::Header, "X-Missing", Op::Exists, ""));
    assert!(passes(
        Src::Header,
        "Content-Type",
        Op::Contains,
        "application/json"
    ));
    assert!(!passes(
        Src::Header,
        "Content-Type",
        Op::Equals,
        "application/json"
    ));
    assert!(passes(
        Src::Header,
        "Content-Type",
        Op::Equals,
        "application/json; charset=utf-8"
    ));
    assert!(passes(Src::Header, "Content-Type", Op::NotContains, "xml"));
    // Repeated headers are joined, as when displayed.
    assert!(passes(Src::Header, "set-cookie", Op::Equals, "a=1, b=2"));
}

#[test]
fn body_text() {
    assert!(passes(Src::Body, "", Op::Contains, "\"Ada\""));
    assert!(passes(Src::Body, "", Op::Matches, r#""id":\d+"#));
    assert!(passes(Src::Body, "", Op::Equals, BODY));
    assert!(passes(Src::Body, "", Op::Exists, ""));
}

#[test]
fn json_path_values() {
    assert!(passes(Src::JsonPath, "$.id", Op::Equals, "7"));
    assert!(passes(Src::JsonPath, "$.id", Op::Equals, "7.0"));
    assert!(passes(Src::JsonPath, "$.name", Op::Equals, "Ada"));
    assert!(passes(Src::JsonPath, "$.name", Op::Equals, "\"Ada\""));
    assert!(passes(
        Src::JsonPath,
        "$.profile",
        Op::Equals,
        r#"{ "age": 36 }"#
    ));
    assert!(passes(Src::JsonPath, "$.profile.age", Op::LessThan, "40"));
    assert!(passes(Src::JsonPath, "$.score", Op::GreaterThan, "9"));
    assert!(passes(Src::JsonPath, "$.tags", Op::Contains, "admin"));
    assert!(!passes(Src::JsonPath, "$.tags", Op::Contains, "root"));
    assert!(passes(Src::JsonPath, "$.profile", Op::Contains, "age"));
    assert!(passes(Src::JsonPath, "$.name", Op::Contains, "d"));
    assert!(passes(Src::JsonPath, "$.name", Op::Matches, "^A.a$"));
    assert!(passes(Src::JsonPath, "$.missing", Op::NotExists, ""));
    assert!(!passes(Src::JsonPath, "$.missing", Op::Equals, "1"));
    // Several matches compare as an array.
    assert!(passes(
        Src::JsonPath,
        "$.tags[*]",
        Op::Equals,
        r#"["admin","dev"]"#
    ));
    assert_eq!(
        check(Src::JsonPath, "$.profile", Op::Equals, "1")
            .actual
            .as_deref(),
        Some(r#"{"age":36}"#)
    );
}

#[test]
fn jq_values() {
    assert!(passes(Src::Jq, ".tags | length", Op::Equals, "2"));
    assert!(passes(Src::Jq, ".name | ascii_downcase", Op::Equals, "ada"));
    assert!(passes(Src::Jq, ".tags[]", Op::Contains, "dev"));
    assert!(passes(Src::Jq, ".missing", Op::Equals, "null"));
}

#[test]
fn expression_and_body_errors() {
    assert_eq!(
        error_code(Src::JsonPath, "$..[", Op::Exists, ""),
        "filter.syntax"
    );
    assert_eq!(error_code(Src::Jq, ".[", Op::Exists, ""), "filter.syntax");
    assert_eq!(
        error_code(Src::Status, "", Op::Matches, "("),
        "assertion.regex"
    );

    let facts = ResponseFacts {
        status: Some(200),
        total_ms: None,
        headers: &[],
        body: "<html>",
    };
    let r = evaluate(
        &Assertion::new(Src::JsonPath, "$.a", Op::Exists, ""),
        &facts,
    );
    assert_eq!(r.error.unwrap().code, "filter.input");
}

#[test]
fn json_schema() {
    let schema = r#"{
        "type": "object",
        "required": ["id", "name"],
        "properties": { "id": { "type": "integer" }, "tags": { "type": "array", "items": { "type": "string" } } }
    }"#;
    assert!(passes(Src::Body, "", Op::MatchesSchema, schema));
    assert!(passes(
        Src::JsonPath,
        "$.profile.age",
        Op::MatchesSchema,
        r#"{"type":"integer","minimum":18}"#
    ));

    let r = check(
        Src::Body,
        "",
        Op::MatchesSchema,
        r#"{"required":["email"]}"#,
    );
    assert!(!r.passed);
    assert!(r.error.is_none());
    assert!(r.detail.unwrap().contains("email"));

    assert_eq!(
        error_code(Src::Body, "", Op::MatchesSchema, "not json"),
        "assertion.schema"
    );
    assert_eq!(
        error_code(Src::Body, "", Op::MatchesSchema, r#"{"type":"nope"}"#),
        "assertion.schema"
    );
}

#[test]
fn results_describe_the_check() {
    let r = check(Src::Header, "Content-Type", Op::Contains, "json");
    assert_eq!(r.source, Src::Header);
    assert_eq!(r.property, "Content-Type");
    assert_eq!(r.operator, Op::Contains);
    assert_eq!(r.expected, "json");
    assert_eq!(r.describe(), "header Content-Type contains json");
    assert_eq!(
        check(Src::Status, "", Op::Equals, "200").describe(),
        "status equals 200"
    );
    assert_eq!(
        check(Src::ResponseTime, "", Op::LessThan, "500").describe(),
        "response time less than 500"
    );
    assert_eq!(
        check(Src::JsonPath, "$.id", Op::Exists, "x").describe(),
        "JSONPath $.id exists"
    );
    assert_eq!(
        check(Src::Body, "", Op::MatchesSchema, "{}").describe(),
        "body matches schema"
    );
}

#[test]
fn evaluate_all_skips_disabled_and_keeps_ids() {
    let mut off = Assertion::new(Src::Status, "", Op::Equals, "500");
    off.enabled = false;
    let on = Assertion::new(Src::Status, "", Op::Equals, "200");
    let facts = ResponseFacts {
        status: Some(200),
        total_ms: None,
        headers: &[],
        body: "",
    };
    let results = evaluate_all(&[off, on.clone()], &facts);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, on.id);
    assert!(results[0].passed);
}
