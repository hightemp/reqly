//! Same scenarios for both engines; prints a capability table (run with --nocapture).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqly_testserver::{TestServer, TlsMode};
use spike_http_engine::{Req, curl_engine, reqwest_engine};

fn get(url: String) -> Req {
    Req {
        method: "GET".into(),
        url,
        follow_redirects: true,
        ..Default::default()
    }
}

async fn curl(req: Req) -> Result<spike_http_engine::Res, String> {
    tokio::task::spawn_blocking(move || curl_engine::send(&req, Arc::new(AtomicBool::new(false))))
        .await
        .unwrap()
}

#[tokio::test]
async fn basic_post_echo() {
    let server = TestServer::start().await;
    let mut req = get(server.url("/echo?x=1"));
    req.method = "POST".into();
    req.headers.push(("X-A".into(), "b".into()));
    req.body = b"hello".to_vec();

    for res in [
        curl(req.clone()).await.unwrap(),
        reqwest_engine::send(&req).await.unwrap(),
    ] {
        assert_eq!(res.status, 200);
        let body = String::from_utf8(res.body).unwrap();
        assert!(body.contains("\"body\":\"hello\""), "{body}");
        assert!(body.contains("x-a"));
    }
}

#[tokio::test]
async fn curl_timeline_and_timings() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    let mut req = get(server.url("/redirect/2"));
    req.ca_pem = Some(server.ca_pem().into());
    let res = curl(req).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.redirects, 2);
    assert!(
        res.timeline
            .iter()
            .any(|l| l.starts_with("> GET /redirect/2")),
        "{:#?}",
        res.timeline
    );
    assert!(res.timeline.iter().any(|l| l.starts_with("< HTTP/")));
    assert!(
        res.timeline
            .iter()
            .any(|l| l.contains("SSL connection using") || l.contains("TLS"))
    );
    assert!(res.timings.tls.is_some(), "{:?}", res.timings);
    assert!(res.timings.ttfb.is_some());
    println!(
        "curl timeline sample:\n{}",
        res.timeline
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("curl timings: {:?}", res.timings);
}

#[tokio::test]
async fn http2_over_tls() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    let mut req = get(server.url("/echo"));
    req.ca_pem = Some(server.ca_pem().into());
    req.http2 = true;
    let c = curl(req.clone()).await.unwrap();
    let r = reqwest_engine::send(&req).await.unwrap();
    println!("http2: curl={} reqwest={}", c.http_version, r.http_version);
    assert_eq!(c.http_version, "HTTP/2");
    assert_eq!(r.http_version, "HTTP/2.0");
}

#[tokio::test]
async fn mutual_tls() {
    let server = TestServer::start_tls(TlsMode::Mutual).await;
    let mut req = get(server.url("/echo"));
    req.ca_pem = Some(server.ca_pem().into());
    req.client_cert_pem = Some(server.client_cert_pem().into());
    req.client_key_pem = Some(server.client_key_pem().into());
    assert_eq!(curl(req.clone()).await.unwrap().status, 200);
    assert_eq!(reqwest_engine::send(&req).await.unwrap().status, 200);
}

#[tokio::test]
async fn curl_cancel_in_flight() {
    let server = TestServer::start().await;
    let req = get(server.url("/delay/3000"));
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let task = tokio::task::spawn_blocking(move || curl_engine::send(&req, flag));
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.store(true, Ordering::Relaxed);
    let started = std::time::Instant::now();
    let err = task.await.unwrap().unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2), "cancel latency");
    println!("curl cancel error: {err}");
}

#[tokio::test]
async fn reqwest_cancel_by_drop() {
    let server = TestServer::start().await;
    let req = get(server.url("/delay/3000"));
    let started = std::time::Instant::now();
    let res = tokio::time::timeout(Duration::from_millis(200), reqwest_engine::send(&req)).await;
    assert!(res.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
}
