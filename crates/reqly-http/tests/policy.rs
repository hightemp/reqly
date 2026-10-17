use reqly_core::model::{Body, BodyKind, HttpRequest};
use reqly_http::engine::{CancelToken, ExecOptions};
use reqly_http::policy::{NetworkPolicy, send_checked};
use reqly_http::prepare::prepare;
use reqly_testserver::TestServer;

fn get(url: String) -> HttpRequest {
    HttpRequest {
        url,
        ..Default::default()
    }
}

async fn send(
    r: &HttpRequest,
    policy: &NetworkPolicy,
) -> reqly_core::CoreResult<reqly_http::engine::HttpResponse> {
    send_checked(
        prepare(r, &[]).unwrap(),
        ExecOptions::default(),
        CancelToken::new(),
        None,
        policy,
    )
    .await
}

#[test]
fn server_default_denies_metadata_and_link_local() {
    let p = NetworkPolicy::server_default();
    for ip in [
        "169.254.169.254",
        "100.100.100.200",
        "fe80::1",
        "fd00:ec2::254",
        "0.0.0.0",
    ] {
        assert!(p.check_ip(ip.parse().unwrap()).is_err(), "{ip}");
    }
    for ip in ["8.8.8.8", "10.0.0.1", "127.0.0.1", "2001:db8::1"] {
        assert!(p.check_ip(ip.parse().unwrap()).is_ok(), "{ip}");
    }
}

#[test]
fn private_and_loopback_blocking_is_optional() {
    let p = NetworkPolicy {
        block_private: true,
        block_loopback: true,
        ..NetworkPolicy::server_default()
    };
    for ip in [
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.1",
        "fc00::1",
        "127.0.0.1",
        "::1",
    ] {
        assert!(p.check_ip(ip.parse().unwrap()).is_err(), "{ip}");
    }
}

#[test]
fn ipv4_mapped_ipv6_is_checked_as_ipv4() {
    let p = NetworkPolicy::server_default();
    assert!(
        p.check_ip("::ffff:169.254.169.254".parse().unwrap())
            .is_err()
    );
}

#[tokio::test]
async fn allowed_requests_pass_through() {
    let server = TestServer::start().await;
    let res = send(
        &get(server.url("/redirect/2")),
        &NetworkPolicy::server_default(),
    )
    .await
    .unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.redirects, 2);
    assert!(res.final_url.ends_with("/echo"));
}

#[tokio::test]
async fn blocks_literal_ip() {
    let err = send(
        &get("http://169.254.169.254/latest/meta-data".into()),
        &NetworkPolicy::server_default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "http.blocked");
    assert_eq!(err.params["ip"], "169.254.169.254");
}

#[tokio::test]
async fn blocks_hostnames_resolving_to_denied_ips() {
    let policy = NetworkPolicy {
        block_loopback: true,
        ..NetworkPolicy::server_default()
    };
    let err = send(&get("http://localhost:9/".into()), &policy)
        .await
        .unwrap_err();
    assert_eq!(err.code, "http.blocked");
}

#[tokio::test]
async fn blocks_redirects_to_denied_addresses() {
    let server = TestServer::start().await;
    let url = server.url("/redirect-to?url=http://169.254.169.254/");
    let err = send(&get(url), &NetworkPolicy::server_default())
        .await
        .unwrap_err();
    assert_eq!(err.code, "http.blocked");
}

#[tokio::test]
async fn allow_hosts_bypass_checks() {
    let server = TestServer::start().await;
    let policy = NetworkPolicy {
        block_loopback: true,
        allow_hosts: vec!["127.0.0.1".into()],
        ..NetworkPolicy::server_default()
    };
    assert_eq!(
        send(&get(server.url("/echo")), &policy)
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn see_other_switches_to_get_and_drops_body() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/redirect-to?status=303&url=/echo"));
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Text,
        text: "payload".into(),
        ..Default::default()
    };
    let res = send(&r, &NetworkPolicy::server_default()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&res.body.to_vec().unwrap()).unwrap();
    assert_eq!(body["method"], "GET");
    assert_eq!(body["body"], "");
}

#[tokio::test]
async fn temporary_redirect_keeps_method_and_body() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/redirect-to?status=307&url=/echo"));
    r.method = "PUT".into();
    r.body = Body {
        kind: BodyKind::Text,
        text: "payload".into(),
        ..Default::default()
    };
    let res = send(&r, &NetworkPolicy::server_default()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&res.body.to_vec().unwrap()).unwrap();
    assert_eq!(body["method"], "PUT");
    assert_eq!(body["body"], "payload");
}

#[tokio::test]
async fn redirect_limit_applies() {
    let server = TestServer::start().await;
    let mut r = get(server.url("/redirect/5"));
    r.settings.max_redirects = Some(2);
    assert_eq!(
        send(&r, &NetworkPolicy::server_default())
            .await
            .unwrap_err()
            .code,
        "http.too_many_redirects"
    );
}
