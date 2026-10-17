//! gRPC test service with every call type, server reflection and optional TLS.

use std::net::SocketAddr;
use std::pin::Pin;

use futures::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

// Module nesting mirrors the protobuf packages (generated code refers to
// other packages through `super::`).
pub mod pb {
    pub mod common {
        tonic::include_proto!("common");
    }
    pub mod reqly {
        pub mod test {
            pub mod v1 {
                tonic::include_proto!("reqly.test.v1");
            }
        }
    }
    pub use reqly::test::v1 as test;
    pub const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/test_descriptor.bin"));
}

use pb::common::Number;
use pb::test::test_service_server::{TestService, TestServiceServer};
use pb::test::{ChatMessage, CountRequest, EchoReply, EchoRequest, Total};

/// The test protos, for clients that load `.proto` files.
pub const TEST_PROTO: &str = include_str!("../proto/test.proto");
pub const TYPES_PROTO: &str = include_str!("../proto/common/types.proto");

#[derive(Default)]
struct Service;

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl TestService for Service {
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoReply>, Status> {
        let metadata = request
            .metadata()
            .get("x-echo")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let req = request.into_inner();
        if req.fail_code > 0 {
            return Err(Status::new(
                tonic::Code::from(req.fail_code),
                format!("failed on purpose: {}", req.message),
            ));
        }
        let mut response = Response::new(EchoReply {
            message: req.message.clone(),
            metadata,
            request: Some(req),
        });
        response
            .metadata_mut()
            .insert("x-served-by", "reqly-test".parse().expect("ascii"));
        Ok(response)
    }

    type CountStream = BoxStream<Number>;

    async fn count(
        &self,
        request: Request<CountRequest>,
    ) -> Result<Response<Self::CountStream>, Status> {
        let to = request.into_inner().to;
        let stream = futures::stream::iter((1..=to).map(|value| Ok(Number { value })));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn sum(&self, request: Request<Streaming<Number>>) -> Result<Response<Total>, Status> {
        let mut stream = request.into_inner();
        let mut total = Total::default();
        while let Some(n) = stream.next().await {
            total.sum += n?.value;
            total.count += 1;
        }
        Ok(Response::new(total))
    }

    type ChatStream = BoxStream<ChatMessage>;

    async fn chat(
        &self,
        request: Request<Streaming<ChatMessage>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let stream = request.into_inner().map(|m| {
            m.map(|m| ChatMessage {
                text: format!("you said: {}", m.text),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

pub struct GrpcTestServer {
    addr: SocketAddr,
    tls: bool,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl GrpcTestServer {
    /// Plaintext (h2c) server with reflection.
    pub async fn start() -> Self {
        Self::spawn(None).await
    }

    /// TLS server; the certificate is signed by the given test CA.
    pub async fn start_tls(cert_pem: &str, key_pem: &str) -> Self {
        // Several rustls providers are compiled in; tonic's server needs one chosen.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);
        Self::spawn(Some(
            tonic::transport::ServerTlsConfig::new().identity(identity),
        ))
        .await
    }

    async fn spawn(tls: Option<tonic::transport::ServerTlsConfig>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(pb::DESCRIPTOR)
            .build_v1()
            .expect("reflection");
        let reflection_alpha = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(pb::DESCRIPTOR)
            .build_v1alpha()
            .expect("reflection");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let is_tls = tls.is_some();
        let mut builder = tonic::transport::Server::builder();
        if let Some(tls) = tls {
            builder = builder.tls_config(tls).expect("tls");
        }
        let router = builder
            .add_service(TestServiceServer::new(Service))
            .add_service(reflection)
            .add_service(reflection_alpha);
        tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            let _ = router
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = rx.await;
                })
                .await;
        });
        Self {
            addr,
            tls: is_tls,
            _shutdown: tx,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `grpc://127.0.0.1:port` or `grpcs://localhost:port`.
    pub fn url(&self) -> String {
        if self.tls {
            format!("grpcs://localhost:{}", self.addr.port())
        } else {
            format!("grpc://127.0.0.1:{}", self.addr.port())
        }
    }
}
