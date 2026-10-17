//! Stage 4 network settings.

use std::time::{Duration, Instant};

use reqly_core::model::{HttpRequest, HttpVersion, KeyValue, RequestSettings, TlsVersion};
use reqly_http::engine::{CancelToken, ExecOptions, HttpResponse, execute};
use reqly_http::prepare::prepare;
use reqly_testserver::{TestServer, TlsMode};
use serde_json::Value;

fn get(url: String, settings: RequestSettings) -> HttpRequest {
    HttpRequest {
        url,
        settings,
        ..Default::default()
    }
}

async fn send(r: &HttpRequest) -> reqly_core::CoreResult<HttpResponse> {
    execute(
        prepare(r, &[])?,
        ExecOptions::default(),
        CancelToken::new(),
        None,
    )
    .await
}

fn json(res: &HttpResponse) -> Value {
    serde_json::from_slice(&res.body.to_vec().unwrap()).unwrap()
}

fn header<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h[0] == name)
        .and_then(|h| h[1].as_str())
}

#[tokio::test]
async fn http_versions() {
    let server = TestServer::start().await;
    let v11 = send(&get(
        server.url("/echo"),
        RequestSettings {
            http_version: Some(HttpVersion::Http1_1),
            ..Default::default()
        },
    ))
    .await
    .unwrap();
    assert_eq!(v11.http_version, "HTTP/1.1");
    let h2c = send(&get(
        server.url("/echo"),
        RequestSettings {
            http_version: Some(HttpVersion::Http2PriorKnowledge),
            ..Default::default()
        },
    ))
    .await
    .unwrap();
    assert_eq!(h2c.http_version, "HTTP/2");
    let tls = TestServer::start_tls(TlsMode::ServerOnly).await;
    let r = get(
        tls.url("/echo"),
        RequestSettings {
            http_version: Some(HttpVersion::Http2),
            ..Default::default()
        },
    );
    let opts = ExecOptions {
        ca_pems: vec![tls.ca_pem().to_string()],
        ..Default::default()
    };
    let h2 = execute(prepare(&r, &[]).unwrap(), opts, CancelToken::new(), None)
        .await
        .unwrap();
    assert_eq!(h2.http_version, "HTTP/2");
}

#[tokio::test]
async fn http3_is_reported_when_unavailable() {
    let server = TestServer::start().await;
    let r = get(
        server.url("/echo"),
        RequestSettings {
            http_version: Some(HttpVersion::Http3),
            ..Default::default()
        },
    );
    let err = send(&r).await.unwrap_err();
    assert_eq!(err.code, "http.http3_unsupported");
}

#[tokio::test]
async fn dns_overrides_and_connect_to() {
    let server = TestServer::start().await;
    let port = server.addr().port();
    let resolve = get(
        format!("http://api.reqly.test:{port}/echo"),
        RequestSettings {
            resolve: Some(vec![format!("api.reqly.test:{port}:127.0.0.1")]),
            ..Default::default()
        },
    );
    let body = json(&send(&resolve).await.unwrap());
    assert_eq!(
        header(&body, "host"),
        Some(format!("api.reqly.test:{port}").as_str())
    );

    let connect = get(
        "http://other.reqly.test:1234/echo".into(),
        RequestSettings {
            connect_to: Some(vec![format!("other.reqly.test:1234:127.0.0.1:{port}")]),
            ..Default::default()
        },
    );
    let body = json(&send(&connect).await.unwrap());
    assert_eq!(header(&body, "host"), Some("other.reqly.test:1234"));
}

#[tokio::test]
async fn invalid_override_lines_are_rejected() {
    let r = get(
        "http://example.com/".into(),
        RequestSettings {
            resolve: Some(vec!["not-an-override".into()]),
            ..Default::default()
        },
    );
    assert_eq!(send(&r).await.unwrap_err().code, "http.invalid_setting");
}

#[tokio::test]
async fn tls_version_range() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    let opts = || ExecOptions {
        ca_pems: vec![server.ca_pem().to_string()],
        ..Default::default()
    };
    let ok = get(
        server.url("/echo"),
        RequestSettings {
            tls_min: Some(TlsVersion::Tls1_2),
            tls_max: Some(TlsVersion::Tls1_2),
            ..Default::default()
        },
    );
    let res = execute(prepare(&ok, &[]).unwrap(), opts(), CancelToken::new(), None)
        .await
        .unwrap();
    assert_eq!(res.status, 200);
    assert!(
        res.timeline.iter().any(|t| t.text.contains("TLSv1.2")),
        "negotiated TLS 1.2"
    );

    let bad = get(
        server.url("/echo"),
        RequestSettings {
            tls_min: Some(TlsVersion::Tls1_3),
            tls_max: Some(TlsVersion::Tls1_2),
            ..Default::default()
        },
    );
    assert_eq!(send(&bad).await.unwrap_err().code, "http.invalid_setting");
}

#[tokio::test]
async fn compression_and_keep_alive() {
    let server = TestServer::start().await;
    let on = json(
        &send(&get(server.url("/echo"), RequestSettings::default()))
            .await
            .unwrap(),
    );
    assert!(header(&on, "accept-encoding").is_some_and(|v| v.contains("gzip")));
    assert_ne!(header(&on, "connection"), Some("close"));

    let off = get(
        server.url("/echo"),
        RequestSettings {
            compression: Some(false),
            keep_alive: Some(false),
            ..Default::default()
        },
    );
    let body = json(&send(&off).await.unwrap());
    assert_eq!(header(&body, "accept-encoding"), None);
    assert_eq!(header(&body, "connection"), Some("close"));

    // An explicit Accept-Encoding header wins.
    let mut explicit = get(
        server.url("/echo"),
        RequestSettings {
            compression: Some(false),
            ..Default::default()
        },
    );
    explicit.headers = vec![KeyValue::new("Accept-Encoding", "br")];
    let body = json(&send(&explicit).await.unwrap());
    assert_eq!(header(&body, "accept-encoding"), Some("br"));
}

#[tokio::test]
async fn response_size_limit() {
    let server = TestServer::start().await;
    let small = get(
        server.url("/bytes/500"),
        RequestSettings {
            max_response_bytes: Some(1000),
            ..Default::default()
        },
    );
    assert_eq!(send(&small).await.unwrap().body_size, 500);
    let big = get(
        server.url("/bytes/100000"),
        RequestSettings {
            max_response_bytes: Some(1000),
            ..Default::default()
        },
    );
    let err = send(&big).await.unwrap_err();
    assert_eq!(err.code, "http.too_large");
    assert_eq!(err.params["limit"], 1000);
    // Without Content-Length (streamed) the download is cut too.
    let stream = get(
        server.url("/stream/5000"),
        RequestSettings {
            max_response_bytes: Some(1000),
            ..Default::default()
        },
    );
    assert_eq!(send(&stream).await.unwrap_err().code, "http.too_large");
}

#[tokio::test]
async fn delay_before_sending_can_be_cancelled() {
    let server = TestServer::start().await;
    let r = get(
        server.url("/echo"),
        RequestSettings {
            delay_ms: Some(300),
            ..Default::default()
        },
    );
    let started = Instant::now();
    send(&r).await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(300));

    let slow = get(
        server.url("/echo"),
        RequestSettings {
            delay_ms: Some(30_000),
            ..Default::default()
        },
    );
    let cancel = CancelToken::new();
    let task = tokio::spawn(execute(
        prepare(&slow, &[]).unwrap(),
        ExecOptions::default(),
        cancel.clone(),
        None,
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = Instant::now();
    cancel.cancel();
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(err.code, "http.cancelled");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn interface_binding() {
    let server = TestServer::start().await;
    let r = get(
        server.url("/echo"),
        RequestSettings {
            interface: Some("127.0.0.1".into()),
            ..Default::default()
        },
    );
    assert_eq!(send(&r).await.unwrap().status, 200);
    let bad = get(
        server.url("/echo"),
        RequestSettings {
            interface: Some("no-such-if0".into()),
            ..Default::default()
        },
    );
    assert!(send(&bad).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn unix_domain_socket() {
    let dir = std::env::temp_dir().join(format!("reqly-uds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let server = reqly_testserver::UnixTestServer::start(&dir.join("api.sock")).await;
    let r = get(
        "http://localhost/echo".into(),
        RequestSettings {
            unix_socket: Some(server.path.display().to_string()),
            ..Default::default()
        },
    );
    let res = send(&r).await.unwrap();
    assert_eq!(json(&res)["path"], "/echo");
}
