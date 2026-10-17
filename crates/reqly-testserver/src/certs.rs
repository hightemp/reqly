use std::sync::Arc;

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use rustls::RootCertStore;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;

/// A throwaway CA with a `localhost` server certificate and a client certificate.
pub struct TestPki {
    pub ca_pem: String,
    pub server_cert_pem: String,
    pub server_key_pem: String,
    pub client_cert_pem: String,
    pub client_key_pem: String,
}

impl TestPki {
    pub fn generate() -> Self {
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Reqly Test CA");
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().expect("ca key"))
            .expect("ca cert");

        let server_key = KeyPair::generate().expect("server key");
        let mut server_params =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                .expect("server params");
        server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server_cert = server_params
            .signed_by(&server_key, &ca)
            .expect("server cert");

        let client_key = KeyPair::generate().expect("client key");
        let mut client_params = CertificateParams::new(Vec::<String>::new()).expect("client");
        client_params
            .distinguished_name
            .push(DnType::CommonName, "reqly-test-client");
        client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let client_cert = client_params
            .signed_by(&client_key, &ca)
            .expect("client cert");

        Self {
            ca_pem: ca.pem(),
            server_cert_pem: server_cert.pem(),
            server_key_pem: server_key.serialize_pem(),
            client_cert_pem: client_cert.pem(),
            client_key_pem: client_key.serialize_pem(),
        }
    }

    pub(crate) fn server_config(&self, require_client_cert: bool) -> rustls::ServerConfig {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let certs = vec![
            CertificateDer::from_pem_slice(self.server_cert_pem.as_bytes()).expect("cert"),
            CertificateDer::from_pem_slice(self.ca_pem.as_bytes()).expect("ca"),
        ];
        let key = PrivateKeyDer::from_pem_slice(self.server_key_pem.as_bytes()).expect("key");

        let builder = rustls::ServerConfig::builder();
        let builder = if require_client_cert {
            let mut roots = RootCertStore::empty();
            roots
                .add(CertificateDer::from_pem_slice(self.ca_pem.as_bytes()).expect("ca"))
                .expect("add root");
            let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
                .build()
                .expect("verifier");
            builder.with_client_cert_verifier(verifier)
        } else {
            builder.with_no_client_auth()
        };
        let mut config = builder.with_single_cert(certs, key).expect("server config");
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        config
    }
}
