//! WebSocket and GraphQL subscription clients against the test server.

use std::time::Duration;

use reqly_protocols::graphql_ws::{self, GqlEvent};
use reqly_protocols::tls::TlsOptions;
use reqly_protocols::ws::{self, WsConfig, WsEvent, WsFrame};
use reqly_testserver::{TestServer, TlsMode};
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;

async fn next<T>(rx: &mut UnboundedReceiver<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed")
}

fn config(url: String) -> WsConfig {
    WsConfig {
        url,
        ..WsConfig::default()
    }
}

#[tokio::test]
async fn echo_text_binary_and_headers() {
    let server = TestServer::start().await;
    let mut cfg = config(server.url("/ws/echo"));
    cfg.headers.push(("X-Hello".into(), "world".into()));
    cfg.subprotocols = vec!["chat".into()];
    let (handshake, tx, mut rx) = ws::connect(cfg).await.unwrap();
    assert_eq!(handshake.status, 101);
    assert_eq!(handshake.protocol.as_deref(), Some("chat"));
    assert!(handshake.url.starts_with("ws://"), "http:// becomes ws://");

    tx.send(WsFrame::Text("hi".into())).unwrap();
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Message(WsFrame::Text("hi".into()))
    );
    tx.send(WsFrame::Binary(vec![0, 1, 255])).unwrap();
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Message(WsFrame::Binary(vec![0, 1, 255]))
    );
    tx.send(WsFrame::Text("headers".into())).unwrap();
    let WsEvent::Message(WsFrame::Text(headers)) = next(&mut rx).await else {
        panic!()
    };
    let headers: serde_json::Value = serde_json::from_str(&headers).unwrap();
    assert_eq!(headers["x-hello"], "world");

    tx.close(1000, "bye");
    let WsEvent::Closed { code, .. } = next(&mut rx).await else {
        panic!()
    };
    assert_eq!(code, Some(1000));
}

#[tokio::test]
async fn server_close_codes_and_server_messages() {
    let server = TestServer::start().await;
    let (_, tx, mut rx) = ws::connect(config(server.url("/ws/echo"))).await.unwrap();
    tx.send(WsFrame::Text("close:4001:custom reason".into()))
        .unwrap();
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Closed {
            code: Some(4001),
            reason: "custom reason".into(),
            error: None
        }
    );

    let (_, _tx, mut rx) = ws::connect(config(server.url("/ws/ticker?n=2")))
        .await
        .unwrap();
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Message(WsFrame::Text("tick 1".into()))
    );
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Message(WsFrame::Text("tick 2".into()))
    );
    let WsEvent::Closed { code, reason, .. } = next(&mut rx).await else {
        panic!()
    };
    assert_eq!((code, reason.as_str()), (Some(1000), "done"));
}

#[tokio::test]
async fn handshake_and_connect_errors() {
    let server = TestServer::start().await;
    let err = ws::connect(config(server.url("/status/403")))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code, "ws.handshake");
    assert_eq!(err.params["status"], 403);
    let err = ws::connect(config("ws://127.0.0.1:9/".into()))
        .await
        .err()
        .unwrap();
    assert_eq!(err.code, "http.connect");
}

#[tokio::test]
async fn wss_with_a_custom_ca_and_without_verification() {
    let server = TestServer::start_tls(TlsMode::ServerOnly).await;
    let url = server.url("/ws/echo");
    assert!(url.starts_with("https://"));
    // Unknown CA: refused.
    let err = ws::connect(config(url.clone())).await.err().unwrap();
    assert_eq!(err.code, "http.tls", "{err:?}");
    // Trusted CA.
    let mut cfg = config(url.clone());
    cfg.tls = TlsOptions {
        verify: true,
        ca_pems: vec![server.ca_pem().into()],
        ..TlsOptions::default()
    };
    let (handshake, tx, mut rx) = ws::connect(cfg).await.unwrap();
    assert!(handshake.url.starts_with("wss://"));
    tx.send(WsFrame::Text("secure".into())).unwrap();
    assert_eq!(
        next(&mut rx).await,
        WsEvent::Message(WsFrame::Text("secure".into()))
    );
    // Verification off.
    let mut cfg = config(url);
    cfg.tls.verify = false;
    assert!(ws::connect(cfg).await.is_ok());
}

#[tokio::test]
async fn graphql_subscription() {
    let server = TestServer::start().await;
    let (sub, mut rx) = graphql_ws::subscribe(
        config(server.url("/graphql/ws")),
        None,
        json!({ "query": "subscription { counter(to: 3) }" }),
    )
    .await
    .unwrap();
    let GqlEvent::Started { protocol } = next(&mut rx).await else {
        panic!()
    };
    assert_eq!(protocol, graphql_ws::TRANSPORT_WS);
    for n in 1..=3 {
        assert_eq!(
            next(&mut rx).await,
            GqlEvent::Next(json!({ "data": { "counter": n } }))
        );
    }
    assert_eq!(next(&mut rx).await, GqlEvent::Complete);
    sub.stop();

    // Validation errors arrive as a result with `errors` (or an `error` message).
    let (_sub, mut rx) = graphql_ws::subscribe(
        config(server.url("/graphql/ws")),
        None,
        json!({ "query": "subscription { nope }" }),
    )
    .await
    .unwrap();
    let _ = next(&mut rx).await;
    let errors = match next(&mut rx).await {
        GqlEvent::Error(e) => e,
        GqlEvent::Next(result) => result["errors"].clone(),
        other => panic!("{other:?}"),
    };
    assert!(errors.to_string().contains("nope"), "{errors}");
}
