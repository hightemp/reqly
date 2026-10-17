//! Local HTTP(S) server with fixtures for Reqly integration tests.
//!
//! ```ignore
//! let server = TestServer::start().await;
//! let url = server.url("/echo");
//! ```

mod certs;
mod routes;

use std::net::SocketAddr;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use tokio::task::JoinHandle;

pub use certs::TestPki;

pub enum TlsMode {
    ServerOnly,
    /// The server requires a client certificate signed by the test CA.
    Mutual,
}

pub struct TestServer {
    addr: SocketAddr,
    scheme: &'static str,
    pki: Option<TestPki>,
    task: JoinHandle<()>,
}

impl TestServer {
    /// Plain HTTP on a random localhost port.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let task = tokio::spawn(async move {
            axum::serve(listener, routes::router())
                .await
                .expect("serve");
        });
        Self {
            addr,
            scheme: "http",
            pki: None,
            task,
        }
    }

    /// HTTPS on a random localhost port with a freshly generated CA.
    pub async fn start_tls(mode: TlsMode) -> Self {
        let pki = TestPki::generate();
        let config = pki.server_config(matches!(mode, TlsMode::Mutual));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("local addr");
        let server =
            axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(Arc::new(config)))
                .expect("tls listener");
        let task = tokio::spawn(async move {
            server
                .serve(routes::router().into_make_service())
                .await
                .expect("serve");
        });
        Self {
            addr,
            scheme: "https",
            pki: Some(pki),
            task,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Absolute URL for `path`. TLS servers use `localhost` to match the certificate.
    pub fn url(&self, path: &str) -> String {
        let host = if self.pki.is_some() {
            "localhost"
        } else {
            "127.0.0.1"
        };
        format!("{}://{host}:{}{path}", self.scheme, self.addr.port())
    }

    fn pki(&self) -> &TestPki {
        self.pki.as_ref().expect("server started without TLS")
    }

    pub fn ca_pem(&self) -> &str {
        &self.pki().ca_pem
    }

    pub fn client_cert_pem(&self) -> &str {
        &self.pki().client_cert_pem
    }

    pub fn client_key_pem(&self) -> &str {
        &self.pki().client_key_pem
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
