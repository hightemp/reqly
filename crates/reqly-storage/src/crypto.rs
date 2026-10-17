//! Encryption of secret values at rest (XChaCha20-Poly1305).
//!
//! The 32-byte key lives in `secret.key` next to the database (created on
//! first open, readable only by the owner). Encrypted values are strings
//! `enc:v1:<base64(nonce || ciphertext)>`.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use reqly_core::{CoreError, CoreResult};

const PREFIX: &str = "enc:v1:";

#[derive(Clone)]
pub struct SecretBox {
    key: [u8; 32],
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBox(..)")
    }
}

fn err(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("storage.secret", e.to_string())
}

impl SecretBox {
    pub fn random() -> Self {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("OS randomness");
        Self { key }
    }

    /// Loads the key from `path`, creating it when missing.
    pub fn load_or_create(path: &Path) -> CoreResult<Self> {
        if let Ok(bytes) = std::fs::read(path) {
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| err("secret.key has a wrong size"))?;
            return Ok(Self { key });
        }
        let this = Self::random();
        std::fs::write(path, this.key).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(this)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&Key::from(self.key))
    }

    pub fn encrypt(&self, plaintext: &str) -> String {
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).expect("OS randomness");
        let ct = self
            .cipher()
            .encrypt(&XNonce::from(nonce), plaintext.as_bytes())
            .expect("encryption cannot fail for valid keys");
        let mut out = nonce.to_vec();
        out.extend(ct);
        format!("{PREFIX}{}", STANDARD.encode(out))
    }

    pub fn is_encrypted(value: &str) -> bool {
        value.starts_with(PREFIX)
    }

    pub fn decrypt(&self, value: &str) -> CoreResult<String> {
        let raw = STANDARD
            .decode(
                value
                    .strip_prefix(PREFIX)
                    .ok_or_else(|| err("not encrypted"))?,
            )
            .map_err(err)?;
        if raw.len() < 24 {
            return Err(err("ciphertext too short"));
        }
        let (nonce, ct) = raw.split_at(24);
        let nonce: [u8; 24] = nonce.try_into().map_err(err)?;
        let pt = self
            .cipher()
            .decrypt(&XNonce::from(nonce), ct)
            .map_err(|_| {
                CoreError::new(
                    "storage.secret_decrypt",
                    "cannot decrypt a secret (was secret.key replaced?)",
                )
            })?;
        String::from_utf8(pt).map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper_detection() {
        let b = SecretBox::random();
        let enc = b.encrypt("hello");
        assert!(SecretBox::is_encrypted(&enc));
        assert_ne!(enc, b.encrypt("hello"), "random nonce");
        assert_eq!(b.decrypt(&enc).unwrap(), "hello");
        assert_eq!(
            SecretBox::random().decrypt(&enc).unwrap_err().code,
            "storage.secret_decrypt"
        );
    }

    #[test]
    fn key_file_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.key");
        let a = SecretBox::load_or_create(&path).unwrap();
        let b = SecretBox::load_or_create(&path).unwrap();
        assert_eq!(b.decrypt(&a.encrypt("x")).unwrap(), "x");
    }
}
