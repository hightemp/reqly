use reqly_core::model::{Auth, BodyKind, HttpRequest, KeyValue, KeyValueKind, ProxyMode};
use reqly_http::curl::{parse_curl, parse_curl_many, to_curl, tokenize};

fn kv(rows: &[KeyValue]) -> Vec<(&str, &str)> {
    rows.iter()
        .map(|r| (r.name.as_str(), r.value.as_str()))
        .collect()
}

fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

#[test]
fn tokenizes_bash_quoting() {
    assert_eq!(
        tokenize(
            r#"curl 'a b' "c \"d\" $x" e\ f $'g\nh' \
  --last"#
        )
        .unwrap(),
        ["curl", "a b", "c \"d\" $x", "e f", "g\nh", "--last"]
    );
    assert!(tokenize("curl 'unterminated").is_err());
}

#[test]
fn simple_get_splits_query_into_params() {
    let p = parse_curl("curl 'https://api.test/users?page=2&q=a%20b'").unwrap();
    assert_eq!(p.request.method, "GET");
    assert_eq!(p.request.url, "https://api.test/users");
    assert_eq!(kv(&p.request.params), [("page", "2"), ("q", "a b")]);
}

#[test]
fn chrome_copy_as_curl_bash() {
    let cmd = r#"curl 'https://api.example.com/v1/orders' \
  -H 'accept: application/json' \
  -H 'content-type: application/json' \
  -b 'sid=abc; theme=dark' \
  --data-raw '{"item":"book","qty":2}' \
  --compressed"#;
    let p = parse_curl(cmd).unwrap();
    let r = &p.request;
    assert_eq!(r.method, "POST");
    assert_eq!(header(r, "accept"), Some("application/json"));
    assert_eq!(header(r, "cookie"), Some("sid=abc; theme=dark"));
    assert_eq!(r.body.kind, BodyKind::Json);
    assert_eq!(r.body.text, r#"{"item":"book","qty":2}"#);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

#[test]
fn chrome_copy_as_curl_cmd() {
    let cmd = r#"curl ^"https://api.example.com/v1/orders^" ^
  -H ^"content-type: application/json^" ^
  --data-raw ^"^{^\^"item^\^":^\^"book^\^"^}^""#;
    let p = parse_curl(cmd).unwrap();
    assert_eq!(p.request.url, "https://api.example.com/v1/orders");
    assert_eq!(p.request.body.text, r#"{"item":"book"}"#);
}

#[test]
fn explicit_method_attached_forms_and_combined_flags() {
    let p = parse_curl("curl -sSL -XPUT -H'X-A: 1' --header=X-B:2 http://h/x").unwrap();
    assert_eq!(p.request.method, "PUT");
    assert_eq!(header(&p.request, "X-A"), Some("1"));
    assert_eq!(header(&p.request, "X-B"), Some("2"));
    assert_eq!(p.request.url, "http://h/x");
}

#[test]
fn form_data_defaults_to_urlencoded_fields() {
    let p = parse_curl("curl http://h -d 'name=Ann+Lee' -d 'city=K%C3%B6ln'").unwrap();
    assert_eq!(p.request.method, "POST");
    assert_eq!(p.request.body.kind, BodyKind::FormUrlencoded);
    assert_eq!(
        kv(&p.request.body.fields),
        [("name", "Ann Lee"), ("city", "Köln")]
    );
}

#[test]
fn non_form_data_is_text() {
    let p = parse_curl("curl http://h -H 'Content-Type: text/plain' -d 'hello world'").unwrap();
    assert_eq!(p.request.body.kind, BodyKind::Text);
    assert_eq!(p.request.body.text, "hello world");

    let p = parse_curl("curl http://h --data-binary 'raw data'").unwrap();
    assert_eq!(p.request.body.kind, BodyKind::Text);
}

#[test]
fn get_flag_moves_data_to_query() {
    let p = parse_curl("curl -G http://h/s -d q=rust --data-urlencode 'tag=a b'").unwrap();
    assert_eq!(p.request.method, "GET");
    assert_eq!(kv(&p.request.params), [("q", "rust"), ("tag", "a b")]);
    assert_eq!(p.request.body.kind, BodyKind::None);
}

#[test]
fn multipart_form_with_files() {
    let p = parse_curl("curl http://h -F 'title=Hi' -F 'doc=@/tmp/a.txt;type=text/plain'").unwrap();
    assert_eq!(p.request.method, "POST");
    assert_eq!(p.request.body.kind, BodyKind::Multipart);
    let f = &p.request.body.fields;
    assert_eq!(
        (f[0].name.as_str(), f[0].value.as_str(), f[0].kind),
        ("title", "Hi", KeyValueKind::Text)
    );
    assert_eq!(
        (f[1].name.as_str(), f[1].value.as_str(), f[1].kind),
        ("doc", "/tmp/a.txt", KeyValueKind::File)
    );
}

#[test]
fn auth_head_insecure_proxy_and_timeouts() {
    let p = parse_curl(
        "curl -I -u user:p:ss -k -x http://proxy:3128 -U pu:pp -m 5 --connect-timeout 2 http://h",
    )
    .unwrap();
    let r = &p.request;
    assert_eq!(r.method, "HEAD");
    assert_eq!(
        r.auth,
        Auth::Basic {
            username: "user".into(),
            password: "p:ss".into()
        }
    );
    assert_eq!(r.settings.verify_tls, Some(false));
    let proxy = r.settings.proxy.as_ref().unwrap();
    assert_eq!(proxy.mode, ProxyMode::Manual);
    assert_eq!(
        (
            proxy.url.as_str(),
            proxy.username.as_str(),
            proxy.password.as_str()
        ),
        ("http://proxy:3128", "pu", "pp")
    );
    assert_eq!(r.settings.timeout_ms, Some(5000));
    assert_eq!(r.settings.connect_timeout_ms, Some(2000));
}

#[test]
fn user_agent_referer_and_url_option() {
    let p = parse_curl("curl -A 'Bot/1' -e https://ref --url http://h/x").unwrap();
    assert_eq!(header(&p.request, "User-Agent"), Some("Bot/1"));
    assert_eq!(header(&p.request, "Referer"), Some("https://ref"));
    assert_eq!(p.request.url, "http://h/x");
}

#[test]
fn unknown_options_produce_warnings_not_errors() {
    let p = parse_curl("curl --frobnicate http://h --cert c.pem").unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("--frobnicate")));
    assert!(p.warnings.iter().any(|w| w.contains("--cert")));
}

#[test]
fn rejects_non_curl_and_missing_url() {
    assert_eq!(
        parse_curl("wget http://h").unwrap_err().code,
        "curl.not_curl"
    );
    assert_eq!(
        parse_curl("curl -H 'a: b'").unwrap_err().code,
        "curl.no_url"
    );
}

#[test]
fn parses_several_commands() {
    let many = parse_curl_many("curl http://a\ncurl -X POST http://b && curl http://c").unwrap();
    assert_eq!(
        many.iter()
            .map(|p| p.request.url.as_str())
            .collect::<Vec<_>>(),
        ["http://a", "http://b", "http://c"]
    );
}

#[test]
fn exports_round_trip_through_the_parser() {
    let p = parse_curl(r#"curl -X PATCH 'https://h/x?a=1' -H 'X-Q: it'\''s' --data-raw '{"a":"b c"}' -H 'Content-Type: application/json'"#).unwrap();
    let prepared = reqly_http::prepare::prepare(&p.request, &[]).unwrap();
    let exported = to_curl(&prepared);
    assert!(
        exported.starts_with("curl --request PATCH \\\n  --url 'https://h/x?a=1'"),
        "{exported}"
    );
    let back = parse_curl(&exported).unwrap();
    assert_eq!(back.request.method, "PATCH");
    assert_eq!(header(&back.request, "X-Q"), Some("it's"));
    assert_eq!(back.request.body.text, r#"{"a":"b c"}"#);
    assert_eq!(kv(&back.request.params), [("a", "1")]);
}

#[test]
fn exports_multipart_and_file_bodies() {
    let p = parse_curl("curl http://h -F 'a=1' -F 'f=@/tmp/x y.bin'").unwrap();
    let out = to_curl(&reqly_http::prepare::prepare(&p.request, &[]).unwrap());
    assert!(out.contains("--form 'a=1'"), "{out}");
    assert!(out.contains("--form 'f=@/tmp/x y.bin'"), "{out}");
}
