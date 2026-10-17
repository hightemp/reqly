use reqly_testserver::{TestServer, TlsMode};
use serde_json::Value;

/// Test clients never use the developer's system proxy.
fn builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().no_proxy()
}

fn client() -> reqwest::Client {
    builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn echo_reflects_request() {
    let server = TestServer::start().await;
    let res = client()
        .post(server.url("/echo?a=1&a=2&b=x"))
        .header("X-Test", "yes")
        .body("hello")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["method"], "POST");
    assert_eq!(json["path"], "/echo");
    assert_eq!(
        json["query"],
        serde_json::json!([["a", "1"], ["a", "2"], ["b", "x"]])
    );
    assert_eq!(json["body"], "hello");
    assert!(
        json["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "x-test" && h[1] == "yes")
    );
}

#[tokio::test]
async fn status_returns_requested_code() {
    let server = TestServer::start().await;
    let res = client()
        .get(server.url("/status/418"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 418);
}

#[tokio::test]
async fn redirect_chain_counts_down() {
    let server = TestServer::start().await;
    let res = client()
        .get(server.url("/redirect/3"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 302);
    assert_eq!(res.headers()["location"], "/redirect/2");

    let res = client()
        .get(server.url("/redirect/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.headers()["location"], "/echo");

    let follow = builder().build().unwrap();
    let res = follow.get(server.url("/redirect/3")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.url().path(), "/echo");
}

#[tokio::test]
async fn cookies_round_trip() {
    let server = TestServer::start().await;
    let jar = builder().cookie_store(true).build().unwrap();
    jar.get(server.url("/cookies/set?session=abc&theme=dark"))
        .send()
        .await
        .unwrap();
    let json: Value = jar
        .get(server.url("/cookies"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["session"], "abc");
    assert_eq!(json["theme"], "dark");
}

#[tokio::test]
async fn delay_waits() {
    let server = TestServer::start().await;
    let started = std::time::Instant::now();
    client().get(server.url("/delay/150")).send().await.unwrap();
    assert!(started.elapsed().as_millis() >= 150);
}

#[tokio::test]
async fn bytes_returns_exact_size() {
    let server = TestServer::start().await;
    let body = client()
        .get(server.url("/bytes/12345"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await;
    assert_eq!(body.unwrap().len(), 12345);
}

#[tokio::test]
async fn stream_sends_chunks() {
    let server = TestServer::start().await;
    let res = client().get(server.url("/stream/5")).send().await.unwrap();
    assert!(res.headers().get("content-length").is_none(), "chunked");
    let text = res.text().await.unwrap();
    assert_eq!(text.lines().count(), 5);
}

#[tokio::test]
async fn compression_is_negotiated() {
    let server = TestServer::start().await;
    for (encoding, client) in [
        ("gzip", builder().gzip(true).build().unwrap()),
        ("br", builder().brotli(true).build().unwrap()),
    ] {
        let res = client
            .get(server.url(&format!("/compressed/{encoding}")))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let json: Value = res.json().await.unwrap();
        assert_eq!(json["encoding"], encoding);
    }
}

#[tokio::test]
async fn basic_auth_checks_credentials() {
    let server = TestServer::start().await;
    let url = server.url("/basic-auth/user/pass");
    assert_eq!(client().get(&url).send().await.unwrap().status(), 401);
    let ok = client()
        .get(&url)
        .basic_auth("user", Some("pass"))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    let bad = client()
        .get(&url)
        .basic_auth("user", Some("nope"))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 401);
}

#[tokio::test]
async fn bearer_checks_token() {
    let server = TestServer::start().await;
    let url = server.url("/bearer/secret");
    assert_eq!(client().get(&url).send().await.unwrap().status(), 401);
    let ok = client()
        .get(&url)
        .bearer_auth("secret")
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
}

#[tokio::test]
async fn tls_server_trusts_generated_ca() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    assert!(server.url("/echo").starts_with("https://localhost:"));

    let untrusted = builder().build().unwrap();
    assert!(untrusted.get(server.url("/echo")).send().await.is_err());

    let ca = reqwest::Certificate::from_pem(server.ca_pem().as_bytes()).unwrap();
    let trusted = builder().tls_certs_only([ca]).build().unwrap();
    assert_eq!(
        trusted
            .get(server.url("/echo"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn mtls_requires_client_certificate() {
    let server = TestServer::start_tls(TlsMode::Mutual).await;
    let ca = reqwest::Certificate::from_pem(server.ca_pem().as_bytes()).unwrap();

    let anonymous = builder().tls_certs_only([ca.clone()]).build().unwrap();
    assert!(anonymous.get(server.url("/echo")).send().await.is_err());

    let identity_pem = format!("{}{}", server.client_cert_pem(), server.client_key_pem());
    let identity = reqwest::Identity::from_pem(identity_pem.as_bytes()).unwrap();
    let client = builder()
        .tls_certs_only([ca])
        .identity(identity)
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(server.url("/echo"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}
