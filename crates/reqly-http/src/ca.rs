//! Trusted CA bundle passed to libcurl as a PEM blob.
//!
//! The statically linked OpenSSL has no reliable default store on Windows
//! and macOS, so roots come from the OS (rustls-native-certs) with the
//! Mozilla set (webpki-root-certs) as a fallback, plus user-supplied CAs.

use std::sync::OnceLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

fn der_to_pem(der: &[u8]) -> String {
    let b64 = STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}

fn system_roots() -> &'static str {
    static ROOTS: OnceLock<String> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let native = rustls_native_certs::load_native_certs();
        if !native.errors.is_empty() {
            tracing::debug!(errors = ?native.errors, "some native certificates could not be loaded");
        }
        let certs: Vec<String> = if native.certs.is_empty() {
            tracing::warn!("no OS root certificates found, using the bundled Mozilla roots");
            webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().map(|c| der_to_pem(c)).collect()
        } else {
            native.certs.iter().map(|c| der_to_pem(c)).collect()
        };
        certs.concat()
    })
}

/// OS roots plus `extra` PEM certificates.
pub fn bundle(extra: &[String]) -> String {
    let mut pem = system_roots().to_string();
    for cert in extra {
        pem.push_str(cert.trim());
        pem.push('\n');
    }
    pem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_wraps_at_64_columns() {
        let pem = der_to_pem(&[0u8; 100]);
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(pem.lines().all(|l| l.len() <= 64 || l.starts_with("-----")));
    }

    #[test]
    fn bundle_contains_roots_and_extra() {
        let b = bundle(&["-----BEGIN CERTIFICATE-----\nX\n-----END CERTIFICATE-----".into()]);
        assert!(b.matches("BEGIN CERTIFICATE").count() > 10);
        assert!(b.ends_with("-----END CERTIFICATE-----\n"));
    }
}
