use std::time::{Duration, Instant};

use reqly_core::model::{
    Body, BodyKind, HttpRequest, KeyValue, KeyValueKind, ProxyMode, ProxySettings,
};
use reqly_http::engine::{CancelToken, ClientIdentity, ExecOptions, TimelineKind, execute};
use reqly_http::prepare::prepare;
use reqly_testserver::{TestServer, TlsMode};
use serde_json::Value;

fn get(url: String) -> HttpRequest {
    HttpRequest {
        url,
        ..Default::default()
    }
}

async fn send(
    r: &HttpRequest,
    opts: ExecOptions,
) -> reqly_core::CoreResult<reqly_http::engine::HttpResponse> {
    execute(prepare(r, &[])?, opts, CancelToken::new(), None).await
}

fn json(res: &reqly_http::engine::HttpResponse) -> Value {
    serde_json::from_slice(&res.body.to_vec().unwrap()).unwrap()
}

#[tokio::test]
async fn sends_method_headers_and_body() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/echo"));
    r.method = "PATCH".into();
    r.params = vec![KeyValue::new("q", "a b")];
    r.headers = vec![KeyValue::new("X-Test", "1")];
    r.body = Body {
        kind: BodyKind::Json,
        text: "{\"x\":1}".into(),
        ..Default::default()
    };
    let res = send(&r, ExecOptions::default()).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.status_text, "OK");
    let body = json(&res);
    assert_eq!(body["method"], "PATCH");
    assert_eq!(body["query"], serde_json::json!([["q", "a%20b"]]));
    assert_eq!(body["body"], "{\"x\":1}");
    let headers = body["headers"].as_array().unwrap();
    let has = |k: &str, v: &str| headers.iter().any(|h| h[0] == k && h[1] == v);
    assert!(has("x-test", "1"));
    assert!(has("content-type", "application/json"));
    assert!(
        headers
            .iter()
            .any(|h| h[0] == "user-agent" && h[1].as_str().unwrap().starts_with("reqly/"))
    );
    assert!(
        !headers.iter().any(|h| h[0] == "expect"),
        "no Expect: 100-continue"
    );
}

#[tokio::test]
async fn reports_response_metadata() {
    let server = TestServer::start().await;
    let res = send(&get(server.url("/status/404")), ExecOptions::default())
        .await
        .unwrap();
    assert_eq!(res.status, 404);
    assert_eq!(res.status_text, "Not Found");
    assert_eq!(res.http_version, "HTTP/1.1");
    assert!(res.final_url.ends_with("/status/404"));
    assert!(res.timings.total_ms > 0.0);
    assert!(res.headers_size > 0);
    assert_eq!(
        res.remote_addr
            .as_deref()
            .map(|a| a.starts_with("127.0.0.1")),
        Some(true)
    );
}

#[tokio::test]
async fn follows_redirects_and_records_timeline() {
    let server = TestServer::start().await;
    let res = send(&get(server.url("/redirect/3")), ExecOptions::default())
        .await
        .unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.redirects, 3);
    assert!(res.final_url.ends_with("/echo"));
    // Only the final response's headers are reported.
    assert!(
        !res.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("location"))
    );
    let out: Vec<_> = res
        .timeline
        .iter()
        .filter(|e| e.kind == TimelineKind::HeaderOut)
        .collect();
    assert!(out.iter().any(|e| e.text.starts_with("GET /redirect/3")));
    assert!(out.iter().any(|e| e.text.starts_with("GET /echo")));
    assert!(
        res.timeline
            .iter()
            .any(|e| e.kind == TimelineKind::HeaderIn && e.text.starts_with("HTTP/1.1 302"))
    );
}

#[tokio::test]
async fn redirects_can_be_disabled_or_limited() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/redirect/3"));
    r.settings.follow_redirects = Some(false);
    let res = send(&r, ExecOptions::default()).await.unwrap();
    assert_eq!(res.status, 302);

    r.settings.follow_redirects = Some(true);
    r.settings.max_redirects = Some(1);
    let err = send(&r, ExecOptions::default()).await.unwrap_err();
    assert_eq!(err.code, "http.too_many_redirects");
}

#[tokio::test]
async fn decompresses_and_reports_sizes() {
    let server = TestServer::start().await;
    let res = send(&get(server.url("/compressed/gzip")), ExecOptions::default())
        .await
        .unwrap();
    assert_eq!(json(&res)["encoding"], "gzip");
    assert_eq!(res.body_size as usize, res.body.to_vec().unwrap().len());
}

#[tokio::test]
async fn large_bodies_spill_to_disk() {
    let server = TestServer::start().await;
    let opts = ExecOptions {
        max_body_in_memory: 1024,
        ..Default::default()
    };
    let res = send(&get(server.url("/bytes/200000")), opts).await.unwrap();
    assert!(res.body.is_file());
    assert_eq!(res.body_size, 200_000);
    assert_eq!(res.body.to_vec().unwrap().len(), 200_000);
}

#[tokio::test]
async fn timeout_and_connection_errors_have_codes() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/delay/2000"));
    r.settings.timeout_ms = Some(200);
    assert_eq!(
        send(&r, ExecOptions::default()).await.unwrap_err().code,
        "http.timeout"
    );

    // Port 9 (discard) on localhost is normally closed.
    let err = send(&get("http://127.0.0.1:9/".into()), ExecOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, "http.connect");
    assert!(
        err.params["timeline"]
            .as_array()
            .is_some_and(|t| !t.is_empty()),
        "timeline kept on error"
    );

    let err = send(
        &get("http://no-such-host.invalid/".into()),
        ExecOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "http.dns");
}

#[tokio::test]
async fn cancel_aborts_in_flight_request() {
    let server = TestServer::start().await;
    let prepared = prepare(&get(server.url("/delay/5000")), &[]).unwrap();
    let cancel = CancelToken::new();
    let handle = tokio::spawn(execute(
        prepared,
        ExecOptions::default(),
        cancel.clone(),
        None,
    ));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let started = Instant::now();
    cancel.cancel();
    let err = handle.await.unwrap().unwrap_err();
    assert_eq!(err.code, "http.cancelled");
    assert!(started.elapsed() < Duration::from_millis(1500));
}

#[tokio::test]
async fn tls_uses_custom_ca_or_can_skip_verification() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    let r = get(server.url("/echo"));
    assert_eq!(
        send(&r, ExecOptions::default()).await.unwrap_err().code,
        "http.tls"
    );

    let opts = ExecOptions {
        ca_pems: vec![server.ca_pem().to_string()],
        ..Default::default()
    };
    let res = send(&r, opts).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.http_version, "HTTP/2");
    assert!(res.timings.tls_ms.is_some());
    assert!(
        res.timeline
            .iter()
            .any(|e| e.kind == TimelineKind::Info && e.text.contains("SSL connection"))
    );

    let mut insecure = r.clone();
    insecure.settings.verify_tls = Some(false);
    assert_eq!(
        send(&insecure, ExecOptions::default())
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn mutual_tls_with_client_identity() {
    let server = TestServer::start_tls(TlsMode::Mutual).await;
    let r = get(server.url("/echo"));
    let base = ExecOptions {
        ca_pems: vec![server.ca_pem().to_string()],
        ..Default::default()
    };
    assert!(send(&r, base.clone()).await.is_err());
    let opts = ExecOptions {
        client_identity: Some(ClientIdentity::Pem {
            cert: server.client_cert_pem().to_string(),
            key: server.client_key_pem().to_string(),
        }),
        ..base
    };
    assert_eq!(send(&r, opts).await.unwrap().status, 200);
}

#[tokio::test]
async fn cookies_are_sent_carried_through_redirects_and_returned() {
    let server = TestServer::start().await;
    let host = "127.0.0.1";
    let opts = ExecOptions {
        cookies: vec![format!("{host}\tFALSE\t/\tFALSE\t0\tseeded\tyes")],
        ..Default::default()
    };
    let res = send(&get(server.url("/cookies/redirect?fresh=1")), opts)
        .await
        .unwrap();
    let body = json(&res);
    assert_eq!(body["seeded"], "yes");
    assert_eq!(
        body["fresh"], "1",
        "cookie set on the redirect hop is sent to the target"
    );
    assert!(res.cookies.iter().any(|c| c.contains("\tfresh\t1")));

    let mut no_send = get(server.url("/cookies"));
    no_send.settings.send_cookies = Some(false);
    let opts = ExecOptions {
        cookies: vec![format!("{host}\tFALSE\t/\tFALSE\t0\tseeded\tyes")],
        ..Default::default()
    };
    assert_eq!(
        json(&send(&no_send, opts).await.unwrap()),
        serde_json::json!({})
    );
}

#[tokio::test]
async fn proxy_modes() {
    let target = TestServer::start().await;
    let proxy = TestServer::start_proxy().await;
    let mut r = get(target.url("/echo"));
    r.settings.proxy = Some(ProxySettings {
        mode: ProxyMode::Manual,
        url: format!("http://{}", proxy.addr()),
        username: "u".into(),
        password: "p".into(),
        no_proxy: String::new(),
    });
    let body = json(&send(&r, ExecOptions::default()).await.unwrap());
    assert_eq!(body["proxied"], true);
    assert!(body["uri"].as_str().unwrap().ends_with("/echo"));
    assert_eq!(body["proxyAuthorization"], "Basic dTpw");

    // Mode none ignores HTTP_PROXY-style environment (the default).
    r.settings.proxy = Some(ProxySettings::default());
    let body = json(&send(&r, ExecOptions::default()).await.unwrap());
    assert_eq!(body["method"], "GET");
}

#[tokio::test]
async fn multipart_and_file_bodies() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    std::fs::write(&path, "file-content").unwrap();

    let mut file_field = KeyValue::new("doc", path.to_string_lossy());
    file_field.kind = KeyValueKind::File;
    let mut r = get(server.url("/echo"));
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Multipart,
        fields: vec![KeyValue::new("title", "Hi"), file_field],
        ..Default::default()
    };
    let body = json(&send(&r, ExecOptions::default()).await.unwrap());
    let text = body["body"].as_str().unwrap();
    assert!(text.contains("name=\"title\"") && text.contains("Hi"));
    assert!(text.contains("filename=\"note.txt\"") && text.contains("file-content"));

    r.body = Body {
        kind: BodyKind::Binary,
        file: Some(reqly_core::model::FileRef::Path {
            path: path.to_string_lossy().into(),
        }),
        ..Default::default()
    };
    let body = json(&send(&r, ExecOptions::default()).await.unwrap());
    assert_eq!(body["body"], "file-content");
}

#[tokio::test]
async fn head_requests_do_not_wait_for_a_body() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/bytes/1000"));
    r.method = "HEAD".into();
    let res = send(&r, ExecOptions::default()).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body_size, 0);
}
