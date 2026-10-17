use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FolderData, HttpRequest, KeyValue, RequestSettings,
};
use reqly_http::prepare::{PreparedBody, prepare};

fn req(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.into(),
        ..Default::default()
    }
}

fn header<'a>(p: &'a reqly_http::prepare::PreparedRequest, name: &str) -> Option<&'a str> {
    p.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[test]
fn appends_enabled_params_in_order() {
    let mut r = req("https://api.test/users");
    r.params = vec![
        KeyValue::new("page", "1"),
        KeyValue::new("skip", "x").disabled(),
        KeyValue::new("tag", "a"),
        KeyValue::new("tag", "b"),
    ];
    let p = prepare(&r, &[]).unwrap();
    assert_eq!(p.url, "https://api.test/users?page=1&tag=a&tag=b");
}

#[test]
fn keeps_existing_query_and_fragment_order() {
    let mut r = req("https://api.test/users?x=1#top");
    r.params = vec![KeyValue::new("y", "2")];
    assert_eq!(
        prepare(&r, &[]).unwrap().url,
        "https://api.test/users?x=1&y=2#top"
    );
}

#[test]
fn encodes_params_when_enabled() {
    let mut r = req("https://api.test/search");
    r.params = vec![
        KeyValue::new("q", "a b&c=d/ü"),
        KeyValue::new("pre", "100%25"),
    ];
    assert_eq!(
        prepare(&r, &[]).unwrap().url,
        "https://api.test/search?q=a%20b%26c%3Dd%2F%C3%BC&pre=100%25"
    );

    r.settings.encode_url = Some(false);
    assert_eq!(
        prepare(&r, &[]).unwrap().url,
        "https://api.test/search?q=a b&c=d/ü&pre=100%25"
    );
}

#[test]
fn encodes_unsafe_path_characters_only() {
    let r = req("https://api.test/files/my report.pdf?x=ü");
    assert_eq!(
        prepare(&r, &[]).unwrap().url,
        "https://api.test/files/my%20report.pdf?x=%C3%BC"
    );
}

#[test]
fn substitutes_path_params() {
    let mut r = req("https://api.test/users/:id/posts/:postId");
    r.path_params = vec![KeyValue::new("id", "42"), KeyValue::new("postId", "a/b")];
    assert_eq!(
        prepare(&r, &[]).unwrap().url,
        "https://api.test/users/42/posts/a%2Fb"
    );
}

#[test]
fn adds_scheme_when_missing() {
    assert_eq!(
        prepare(&req("localhost:8080/x"), &[]).unwrap().url,
        "http://localhost:8080/x"
    );
}

#[test]
fn rejects_empty_url() {
    assert_eq!(
        prepare(&req("  "), &[]).unwrap_err().code,
        "http.invalid_url"
    );
}

#[test]
fn folder_headers_are_inherited_and_overridden() {
    let outer = FolderData {
        headers: vec![
            KeyValue::new("X-Team", "core"),
            KeyValue::new("X-Env", "outer"),
        ],
        ..Default::default()
    };
    let inner = FolderData {
        headers: vec![KeyValue::new("x-env", "inner")],
        ..Default::default()
    };
    let mut r = req("http://a");
    r.headers = vec![
        KeyValue::new("X-Req", "1"),
        KeyValue::new("X-Off", "no").disabled(),
    ];
    // Folders are passed outermost first.
    let p = prepare(&r, &[&outer, &inner]).unwrap();
    assert_eq!(header(&p, "X-Team"), Some("core"));
    assert_eq!(header(&p, "X-Env"), Some("inner"));
    assert_eq!(header(&p, "X-Req"), Some("1"));
    assert_eq!(header(&p, "X-Off"), None);
    assert_eq!(
        p.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("x-env"))
            .count(),
        1
    );
}

#[test]
fn request_header_overrides_folder_header() {
    let folder = FolderData {
        headers: vec![KeyValue::new("Accept", "text/plain")],
        ..Default::default()
    };
    let mut r = req("http://a");
    r.headers = vec![KeyValue::new("accept", "application/json")];
    let p = prepare(&r, &[&folder]).unwrap();
    assert_eq!(header(&p, "Accept"), Some("application/json"));
}

#[test]
fn basic_and_bearer_auth() {
    let mut r = req("http://a");
    r.auth = Auth::Basic {
        username: "user".into(),
        password: "pä:ss".into(),
    };
    assert_eq!(
        header(&prepare(&r, &[]).unwrap(), "Authorization"),
        Some("Basic dXNlcjpww6Q6c3M=")
    );

    r.auth = Auth::Bearer {
        token: "t0k".into(),
        prefix: String::new(),
    };
    assert_eq!(
        header(&prepare(&r, &[]).unwrap(), "Authorization"),
        Some("Bearer t0k")
    );

    r.auth = Auth::Bearer {
        token: "t0k".into(),
        prefix: "Token".into(),
    };
    assert_eq!(
        header(&prepare(&r, &[]).unwrap(), "Authorization"),
        Some("Token t0k")
    );
}

#[test]
fn api_key_in_header_or_query() {
    let mut r = req("http://a/x");
    r.auth = Auth::ApiKey {
        key: "X-Key".into(),
        value: "s3".into(),
        location: ApiKeyLocation::Header,
    };
    assert_eq!(header(&prepare(&r, &[]).unwrap(), "X-Key"), Some("s3"));
    r.auth = Auth::ApiKey {
        key: "api_key".into(),
        value: "s 3".into(),
        location: ApiKeyLocation::Query,
    };
    assert_eq!(prepare(&r, &[]).unwrap().url, "http://a/x?api_key=s%203");
}

#[test]
fn auth_inherits_from_nearest_folder_that_sets_it() {
    let outer = FolderData {
        auth: Auth::Bearer {
            token: "outer".into(),
            prefix: String::new(),
        },
        ..Default::default()
    };
    let middle = FolderData::default(); // Inherit
    let inner_none = FolderData {
        auth: Auth::None,
        ..Default::default()
    };
    let r = req("http://a");
    assert_eq!(
        header(&prepare(&r, &[&outer, &middle]).unwrap(), "Authorization"),
        Some("Bearer outer")
    );
    assert_eq!(
        header(
            &prepare(&r, &[&outer, &inner_none]).unwrap(),
            "Authorization"
        ),
        None
    );
}

#[test]
fn explicit_authorization_header_wins_over_auth() {
    let mut r = req("http://a");
    r.auth = Auth::Bearer {
        token: "x".into(),
        prefix: String::new(),
    };
    r.headers = vec![KeyValue::new("Authorization", "Custom y")];
    assert_eq!(
        header(&prepare(&r, &[]).unwrap(), "Authorization"),
        Some("Custom y")
    );
}

#[test]
fn text_bodies_set_content_type_unless_given() {
    let mut r = req("http://a");
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Json,
        text: "{\"a\":1}".into(),
        ..Default::default()
    };
    let p = prepare(&r, &[]).unwrap();
    assert_eq!(header(&p, "Content-Type"), Some("application/json"));
    assert_eq!(p.body, PreparedBody::Bytes(b"{\"a\":1}".to_vec()));

    r.headers = vec![KeyValue::new("content-type", "application/vnd.api+json")];
    let p = prepare(&r, &[]).unwrap();
    assert_eq!(header(&p, "Content-Type"), Some("application/vnd.api+json"));
    assert_eq!(
        p.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .count(),
        1
    );
}

#[test]
fn form_urlencoded_body() {
    let mut r = req("http://a");
    r.body = Body {
        kind: BodyKind::FormUrlencoded,
        fields: vec![
            KeyValue::new("name", "Ann Lee"),
            KeyValue::new("x", "1").disabled(),
            KeyValue::new("q", "a&b"),
        ],
        ..Default::default()
    };
    let p = prepare(&r, &[]).unwrap();
    assert_eq!(
        p.body,
        PreparedBody::Bytes(b"name=Ann+Lee&q=a%26b".to_vec())
    );
    assert_eq!(
        header(&p, "Content-Type"),
        Some("application/x-www-form-urlencoded")
    );
}

#[test]
fn multipart_body_keeps_parts() {
    let mut r = req("http://a");
    let mut file = KeyValue::new("doc", "/tmp/a.txt");
    file.kind = reqly_core::model::KeyValueKind::File;
    r.body = Body {
        kind: BodyKind::Multipart,
        fields: vec![KeyValue::new("title", "Hi"), file],
        ..Default::default()
    };
    let p = prepare(&r, &[]).unwrap();
    let PreparedBody::Multipart(parts) = &p.body else {
        panic!()
    };
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "title");
    assert_eq!(
        parts[1].file.as_deref(),
        Some(std::path::Path::new("/tmp/a.txt"))
    );
    assert_eq!(
        header(&p, "Content-Type"),
        None,
        "curl sets the boundary header"
    );
}

#[test]
fn none_body_ignores_stale_text() {
    let mut r = req("http://a");
    r.body = Body {
        kind: BodyKind::None,
        text: "leftover".into(),
        ..Default::default()
    };
    let p = prepare(&r, &[]).unwrap();
    assert_eq!(p.body, PreparedBody::None);
    assert_eq!(header(&p, "Content-Type"), None);
}

#[test]
fn settings_merge_request_over_folders_over_defaults() {
    let outer = FolderData {
        settings: RequestSettings {
            timeout_ms: Some(1000),
            verify_tls: Some(false),
            ..Default::default()
        },
        ..Default::default()
    };
    let inner = FolderData {
        settings: RequestSettings {
            timeout_ms: Some(2000),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut r = req("http://a");
    r.settings.follow_redirects = Some(false);
    let s = prepare(&r, &[&outer, &inner]).unwrap().settings;
    assert_eq!(s.timeout_ms, Some(2000));
    assert_eq!(s.verify_tls, Some(false));
    assert_eq!(s.follow_redirects, Some(false));
    assert_eq!(s.max_redirects, Some(10));
}

#[test]
fn method_is_uppercased_and_defaults_to_get() {
    let mut r = req("http://a");
    r.method = " patch ".into();
    assert_eq!(prepare(&r, &[]).unwrap().method, "PATCH");
    r.method = "".into();
    assert_eq!(prepare(&r, &[]).unwrap().method, "GET");
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn encoded_param_values_round_trip(value in "\\PC{0,20}") {
            let mut r = req("http://a/x");
            r.params = vec![KeyValue::new("v", value.clone())];
            let url = prepare(&r, &[]).unwrap().url;
            let encoded = url.split_once("?v=").unwrap().1;
            let decoded = percent_encoding::percent_decode_str(encoded).decode_utf8_lossy();
            // Pre-encoded `%XX` sequences are kept as-is, so compare after decoding both.
            let expected = percent_encoding::percent_decode_str(&value).decode_utf8_lossy();
            prop_assert_eq!(decoded, expected);
        }
    }
}
