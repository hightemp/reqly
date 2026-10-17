//! rustls client configuration shared by WebSocket and gRPC.

use std::sync::Arc;

use reqly_core::{CoreError, CoreResult};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

#[derive(Debug, Clone, Default)]
pub struct TlsOptions {
    /// `false` accepts any certificate (like curl `-k`).
    pub verify: bool,
    /// Extra trusted CAs (PEM).
    pub ca_pems: Vec<String>,
    /// Client certificate chain and private key (PEM).
    pub identity: Option<(String, String)>,
    /// ALPN protocols, e.g. `h2` for gRPC.
    pub alpn: Vec<Vec<u8>>,
}

fn tls_error(message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new("http.tls", message.clone()).with_param("message", message)
}

fn certs(pem: &str) -> CoreResult<Vec<CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| tls_error(format!("invalid certificate: {e}")))
}

pub fn client_config(options: &TlsOptions) -> CoreResult<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| tls_error(e.to_string()))?;
    let builder = if options.verify {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        for pem in &options.ca_pems {
            for cert in certs(pem)? {
                roots
                    .add(cert)
                    .map_err(|e| tls_error(format!("invalid CA: {e}")))?;
            }
        }
        builder.with_root_certificates(roots)
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAny(provider)))
    };
    let mut config = match &options.identity {
        Some((cert, key)) => {
            let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key.as_bytes())
                .map_err(|e| tls_error(format!("invalid private key: {e}")))?
                .ok_or_else(|| tls_error("no private key in the PEM"))?;
            builder
                .with_client_auth_cert(certs(cert)?, key)
                .map_err(|e| tls_error(e.to_string()))?
        }
        None => builder.with_no_client_auth(),
    };
    config.alpn_protocols = options.alpn.clone();
    Ok(Arc::new(config))
}

/// Verification turned off: any certificate is accepted, signatures are still checked.
#[derive(Debug)]
struct AcceptAny(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
