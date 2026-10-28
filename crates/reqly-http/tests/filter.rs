use reqly_http::filter::{FilterKind, apply_filter, query_json};
use serde_json::json;

const JSON: &str =
    r#"{"store":{"books":[{"title":"A","price":8},{"title":"B","price":12}],"open":true}}"#;
const XML: &str = r#"<?xml version="1.0"?><store><book id="1"><title>A</title></book><book id="2"><title>B &amp; C</title></book></store>"#;

#[test]
fn jsonpath_single_and_many() {
    assert_eq!(
        apply_filter(FilterKind::JsonPath, JSON, "$.store.open").unwrap(),
        "true"
    );
    assert_eq!(
        apply_filter(FilterKind::JsonPath, JSON, "$.store.books[*].title").unwrap(),
        "[\n  \"A\",\n  \"B\"\n]"
    );
    assert_eq!(
        apply_filter(
            FilterKind::JsonPath,
            JSON,
            "$.store.books[?@.price > 10].title"
        )
        .unwrap(),
        "\"B\""
    );
}

#[test]
fn jsonpath_errors() {
    assert_eq!(
        apply_filter(FilterKind::JsonPath, JSON, "$..[")
            .unwrap_err()
            .code,
        "filter.syntax"
    );
    assert_eq!(
        apply_filter(FilterKind::JsonPath, "not json", "$")
            .unwrap_err()
            .code,
        "filter.input"
    );
}

#[test]
fn jq_outputs_each_value_on_its_own_line() {
    assert_eq!(
        apply_filter(FilterKind::Jq, JSON, ".store.books[] | .title").unwrap(),
        "\"A\"\n\"B\""
    );
    assert_eq!(
        apply_filter(FilterKind::Jq, JSON, "[.store.books[].price] | add").unwrap(),
        "20"
    );
    assert_eq!(
        apply_filter(
            FilterKind::Jq,
            JSON,
            ".store.books | map(select(.price > 10)) | length"
        )
        .unwrap(),
        "1"
    );
}

#[test]
fn jq_errors() {
    assert_eq!(
        apply_filter(FilterKind::Jq, JSON, ".[").unwrap_err().code,
        "filter.syntax"
    );
    assert_eq!(
        apply_filter(FilterKind::Jq, JSON, ".store.open | .x")
            .unwrap_err()
            .code,
        "filter.runtime"
    );
}

#[test]
fn query_json_returns_every_value() {
    assert_eq!(
        query_json(FilterKind::JsonPath, JSON, "$.store.books[*].price").unwrap(),
        vec![json!(8), json!(12)]
    );
    assert_eq!(
        query_json(FilterKind::JsonPath, JSON, "$.missing").unwrap(),
        Vec::<serde_json::Value>::new()
    );
    assert_eq!(
        query_json(FilterKind::Jq, JSON, ".store.books[0]").unwrap(),
        vec![json!({"title": "A", "price": 8})]
    );
    assert_eq!(
        query_json(FilterKind::Jq, JSON, ".store.books[] | .title").unwrap(),
        vec![json!("A"), json!("B")]
    );
    assert_eq!(
        query_json(FilterKind::Jq, JSON, ".[").unwrap_err().code,
        "filter.syntax"
    );
    assert_eq!(
        query_json(FilterKind::XPath, JSON, "/a").unwrap_err().code,
        "filter.unsupported"
    );
}

#[test]
fn xpath_nodes_attributes_and_values() {
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "/store/book[2]/title").unwrap(),
        "<title>B &amp; C</title>"
    );
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "//book/@id").unwrap(),
        "id=\"1\"\nid=\"2\""
    );
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "count(//book)").unwrap(),
        "2"
    );
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "string(//book[1]/title)").unwrap(),
        "A"
    );
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "//book[1]").unwrap(),
        "<book id=\"1\"><title>A</title></book>"
    );
}

#[test]
fn xpath_errors() {
    assert_eq!(
        apply_filter(FilterKind::XPath, XML, "//[")
            .unwrap_err()
            .code,
        "filter.syntax"
    );
    assert_eq!(
        apply_filter(FilterKind::XPath, "<a>", "/a")
            .unwrap_err()
            .code,
        "filter.input"
    );
}
