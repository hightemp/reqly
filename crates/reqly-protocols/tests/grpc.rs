//! gRPC client against the test service.

use std::time::Duration;

use futures::StreamExt;
use reqly_protocols::grpc::{self, ChannelOptions, ProtoFiles};
use reqly_protocols::tls::TlsOptions;
use reqly_testserver::TestPki;
use reqly_testserver::grpc::{GrpcTestServer, TEST_PROTO, TYPES_PROTO};

fn files() -> ProtoFiles {
    ProtoFiles {
        sources: vec![
            ("test.proto".into(), TEST_PROTO.into()),
            ("common/types.proto".into(), TYPES_PROTO.into()),
        ],
        include_dirs: Vec::new(),
    }
}

fn options() -> ChannelOptions {
    ChannelOptions {
        tls: TlsOptions {
            verify: true,
            ..TlsOptions::default()
        },
        connect_timeout: Duration::from_secs(5),
        connect_addr: None,
    }
}

async fn channel(server: &GrpcTestServer) -> tonic::transport::Channel {
    grpc::connect(&grpc::parse_target(&server.url()).unwrap(), options())
        .await
        .unwrap()
}

#[test]
fn compiles_protos_and_describes_services() {
    let pool = grpc::compile(&files()).unwrap();
    let services = grpc::services(&pool);
    assert_eq!(services.len(), 1);
    let svc = &services[0];
    assert_eq!(svc.name, "reqly.test.v1.TestService");
    let kinds: Vec<(&str, bool, bool)> = svc
        .methods
        .iter()
        .map(|m| (m.name.as_str(), m.client_streaming, m.server_streaming))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Echo", false, false),
            ("Count", false, true),
            ("Sum", true, false),
            ("Chat", true, true)
        ]
    );
    let example: serde_json::Value = serde_json::from_str(&svc.methods[0].example).unwrap();
    assert_eq!(example["message"], "");
    assert_eq!(example["tags"], serde_json::json!([""]));
    assert_eq!(example["mood"], "HAPPY");
    assert_eq!(example["nested"], serde_json::json!({ "flag": false }));
    assert_eq!(example["at"], "1970-01-01T00:00:00Z");
    assert!(
        example.get("text").is_some() && example.get("number").is_none(),
        "one member of a oneof"
    );

    let err = grpc::compile(&ProtoFiles {
        sources: vec![("x.proto".into(), "syntax = \"proto3\"; message {".into())],
        ..Default::default()
    })
    .unwrap_err();
    assert_eq!(err.code, "grpc.proto_error");
    let missing = grpc::compile(&ProtoFiles {
        sources: vec![("test.proto".into(), TEST_PROTO.into())],
        ..Default::default()
    })
    .unwrap_err();
    assert!(
        missing.message.contains("common/types.proto"),
        "{missing:?}"
    );
}

#[test]
fn targets() {
    let t = grpc::parse_target("grpcs://api.test").unwrap();
    assert_eq!((t.host.as_str(), t.port, t.tls), ("api.test", 443, true));
    let t = grpc::parse_target("localhost:50051").unwrap();
    assert_eq!(
        (t.host.as_str(), t.port, t.tls),
        ("localhost", 50051, false)
    );
    let t = grpc::parse_target("grpc://[::1]:9").unwrap();
    assert_eq!((t.host.as_str(), t.port), ("::1", 9));
}

#[tokio::test]
async fn all_call_types() {
    let server = GrpcTestServer::start().await;
    let pool = grpc::compile(&files()).unwrap();
    let ch = channel(&server).await;

    // Unary with metadata.
    let echo = grpc::find_method(&pool, "reqly.test.v1.TestService", "Echo").unwrap();
    let msg = grpc::from_json(
        &echo.input(),
        r#"{"message": "hi", "counts": {"a": 1}, "number": "5"}"#,
    )
    .unwrap();
    let mut res = grpc::call(
        ch.clone(),
        &echo,
        &[("x-echo".into(), "meta".into())],
        futures::stream::iter([msg]),
        None,
    )
    .await
    .unwrap();
    assert!(
        res.headers
            .iter()
            .any(|(k, v)| k == "x-served-by" && v == "reqly-test")
    );
    let reply = res.messages.next().await.unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(&grpc::to_json(&reply)).unwrap();
    assert_eq!(json["message"], "hi");
    assert_eq!(json["metadata"], "meta");
    assert_eq!(json["request"]["counts"]["a"], 1);
    assert_eq!(json["request"]["number"], "5");
    assert!(res.messages.next().await.is_none());

    // Error status.
    let failing = grpc::from_json(&echo.input(), r#"{"message": "x", "failCode": 5}"#).unwrap();
    let status = match grpc::call(
        ch.clone(),
        &echo,
        &[],
        futures::stream::iter([failing]),
        None,
    )
    .await
    {
        Err(s) => s,
        Ok(mut r) => r.messages.next().await.unwrap().unwrap_err(),
    };
    assert_eq!(status.code(), tonic::Code::NotFound);
    assert_eq!(grpc::code_name(status.code()), "NOT_FOUND");

    // Server streaming.
    let count = grpc::find_method(&pool, "reqly.test.v1.TestService", "Count").unwrap();
    let msg = grpc::from_json(&count.input(), r#"{"to": 3}"#).unwrap();
    let res = grpc::call(ch.clone(), &count, &[], futures::stream::iter([msg]), None)
        .await
        .unwrap();
    let values: Vec<String> = res
        .messages
        .map(|m| {
            serde_json::from_str::<serde_json::Value>(&grpc::to_json(&m.unwrap())).unwrap()["value"]
                .to_string()
        })
        .collect()
        .await;
    assert_eq!(values, ["\"1\"", "\"2\"", "\"3\""]);

    // Client streaming through a channel the caller feeds.
    let sum = grpc::find_method(&pool, "reqly.test.v1.TestService", "Sum").unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let res = grpc::call(
        ch.clone(),
        &sum,
        &[],
        tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        None,
    );
    for n in [2, 3, 5] {
        tx.send(grpc::from_json(&sum.input(), &format!(r#"{{"value": {n}}}"#)).unwrap())
            .unwrap();
    }
    drop(tx);
    let mut res = res.await.unwrap();
    let total: serde_json::Value =
        serde_json::from_str(&grpc::to_json(&res.messages.next().await.unwrap().unwrap())).unwrap();
    assert_eq!(
        (total["sum"].as_str(), total["count"].as_i64()),
        (Some("10"), Some(3))
    );

    // Bidi.
    let chat = grpc::find_method(&pool, "reqly.test.v1.TestService", "Chat").unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send(grpc::from_json(&chat.input(), r#"{"text": "one"}"#).unwrap())
        .unwrap();
    let mut res = grpc::call(
        ch,
        &chat,
        &[],
        tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        None,
    )
    .await
    .unwrap();
    let first = grpc::to_json(&res.messages.next().await.unwrap().unwrap());
    assert!(first.contains("you said: one"));
    tx.send(grpc::from_json(&chat.input(), r#"{"text": "two"}"#).unwrap())
        .unwrap();
    assert!(grpc::to_json(&res.messages.next().await.unwrap().unwrap()).contains("you said: two"));
    drop(tx);
    assert!(res.messages.next().await.is_none());
}

#[tokio::test]
async fn reflection_and_bad_json() {
    let server = GrpcTestServer::start().await;
    let pool = grpc::reflect(channel(&server).await, &[]).await.unwrap();
    let names: Vec<String> = grpc::services(&pool).into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["reqly.test.v1.TestService"]);
    let echo = grpc::find_method(&pool, "reqly.test.v1.TestService", "Echo").unwrap();
    let err = grpc::from_json(&echo.input(), r#"{"nope": 1}"#).unwrap_err();
    assert_eq!(err.code, "grpc.invalid_message");
}

#[tokio::test]
async fn tls_with_the_test_ca_and_connection_errors() {
    let pki = TestPki::generate();
    let server = GrpcTestServer::start_tls(&pki.server_cert_pem, &pki.server_key_pem).await;
    let target = grpc::parse_target(&server.url()).unwrap();
    // Unknown CA.
    let err = grpc::connect(&target, options()).await.unwrap_err();
    assert_eq!(err.code, "http.tls", "{err:?}");
    let mut opts = options();
    opts.tls.ca_pems = vec![pki.ca_pem.clone()];
    let ch = grpc::connect(&target, opts).await.unwrap();
    let pool = grpc::reflect(ch, &[]).await.unwrap();
    assert_eq!(grpc::services(&pool).len(), 1);

    let err = grpc::connect(&grpc::parse_target("127.0.0.1:9").unwrap(), options())
        .await
        .unwrap_err();
    assert_eq!(err.code, "http.connect");
}

#[tokio::test]
async fn deadlines() {
    let server = GrpcTestServer::start().await;
    let pool = grpc::compile(&files()).unwrap();
    let sum = grpc::find_method(&pool, "reqly.test.v1.TestService", "Sum").unwrap();
    // The client never finishes; the deadline does.
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let result = grpc::call(
        channel(&server).await,
        &sum,
        &[],
        tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        Some(Duration::from_millis(100)),
    )
    .await;
    let status = match result {
        Err(s) => s,
        Ok(mut r) => r.messages.next().await.unwrap().unwrap_err(),
    };
    assert!(
        matches!(
            status.code(),
            tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
        ),
        "{status:?}"
    );
}
