//! Content-addressed file store for large bodies and uploaded files.
//!
//! Blobs live at `<dir>/<hash[0..2]>/<hash>` where `hash` is the SHA-256 of
//! the content. Writes go to a temp file first and are renamed into place,
//! so readers never see partial blobs. Unreferenced blobs are removed by
//! [`BlobStore::gc`] given the set of hashes still in use.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use reqly_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::db::db_err;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct BlobStore {
    dir: PathBuf,
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl BlobStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path_of(&self, hash: &str) -> CoreResult<PathBuf> {
        if !valid_hash(hash) {
            return Err(
                CoreError::new("blob.invalid_hash", "invalid blob hash").with_param("hash", hash)
            );
        }
        Ok(self.dir.join(&hash[..2]).join(hash))
    }

    pub async fn put_bytes(&self, bytes: &[u8]) -> CoreResult<BlobRef> {
        self.put_reader(bytes).await
    }

    /// Streams `reader` into the store without buffering it in memory.
    pub async fn put_reader(&self, mut reader: impl AsyncRead + Unpin) -> CoreResult<BlobRef> {
        let tmp_dir = self.dir.join("tmp");
        tokio::fs::create_dir_all(&tmp_dir).await.map_err(db_err)?;
        let tmp = tmp_dir.join(reqly_core::Id::new().to_string());
        let mut file = tokio::fs::File::create(&tmp).await.map_err(db_err)?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).await.map_err(db_err)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).await.map_err(db_err)?;
            size += n as u64;
        }
        file.flush().await.map_err(db_err)?;
        drop(file);

        let hash = hex::encode(hasher.finalize());
        let target = self.path_of(&hash)?;
        if target.exists() {
            tokio::fs::remove_file(&tmp).await.map_err(db_err)?;
        } else {
            tokio::fs::create_dir_all(target.parent().expect("has parent"))
                .await
                .map_err(db_err)?;
            tokio::fs::rename(&tmp, &target).await.map_err(db_err)?;
        }
        Ok(BlobRef { hash, size })
    }

    pub async fn read(&self, hash: &str) -> CoreResult<Vec<u8>> {
        let path = self.path_of(hash)?;
        tokio::fs::read(&path).await.map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CoreError::not_found("blob", hash),
            _ => db_err(e),
        })
    }

    /// Reads at most `limit` bytes from `offset` (for paging large bodies into the UI).
    pub async fn read_range(&self, hash: &str, offset: u64, limit: usize) -> CoreResult<Vec<u8>> {
        use tokio::io::AsyncSeekExt;
        let path = self.path_of(hash)?;
        let mut file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => CoreError::not_found("blob", hash),
                _ => db_err(e),
            })?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(db_err)?;
        let mut out = Vec::with_capacity(limit.min(1 << 20));
        file.take(limit as u64)
            .read_to_end(&mut out)
            .await
            .map_err(db_err)?;
        Ok(out)
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.path_of(hash).map(|p| p.exists()).unwrap_or(false)
    }

    /// Deletes every blob whose hash is not in `live`; returns how many were removed.
    pub async fn gc(&self, live: &HashSet<String>) -> CoreResult<usize> {
        let mut removed = 0;
        let mut shards = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(db_err(e)),
        };
        while let Some(shard) = shards.next_entry().await.map_err(db_err)? {
            let name = shard.file_name();
            let name = name.to_string_lossy();
            if name.len() != 2 || !shard.path().is_dir() {
                continue;
            }
            let mut entries = tokio::fs::read_dir(shard.path()).await.map_err(db_err)?;
            while let Some(entry) = entries.next_entry().await.map_err(db_err)? {
                let hash = entry.file_name().to_string_lossy().into_owned();
                if valid_hash(&hash) && !live.contains(&hash) {
                    tokio::fs::remove_file(entry.path()).await.map_err(db_err)?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (BlobStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (BlobStore::new(dir.path().join("blobs")), dir)
    }

    #[tokio::test]
    async fn put_is_content_addressed_and_idempotent() {
        let (s, _d) = store();
        let a = s.put_bytes(b"hello").await.unwrap();
        assert_eq!(
            a.hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(a.size, 5);
        let b = s.put_bytes(b"hello").await.unwrap();
        assert_eq!(a, b);
        assert_eq!(s.read(&a.hash).await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn streams_large_input() {
        let (s, _d) = store();
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let r = s.put_reader(&data[..]).await.unwrap();
        assert_eq!(r.size, data.len() as u64);
        assert_eq!(
            s.read_range(&r.hash, 1_000_000, 10).await.unwrap(),
            &data[1_000_000..1_000_010]
        );
        assert_eq!(
            s.read_range(&r.hash, 2_999_995, 100).await.unwrap().len(),
            5
        );
    }

    #[tokio::test]
    async fn rejects_path_traversal_hashes() {
        let (s, _d) = store();
        assert_eq!(
            s.read("../../etc/passwd").await.unwrap_err().code,
            "blob.invalid_hash"
        );
        assert_eq!(
            s.read(&"A".repeat(64)).await.unwrap_err().code,
            "blob.invalid_hash"
        );
    }

    #[tokio::test]
    async fn missing_blob_is_not_found() {
        let (s, _d) = store();
        assert_eq!(s.read(&"0".repeat(64)).await.unwrap_err().code, "not_found");
    }

    #[tokio::test]
    async fn gc_keeps_only_live_blobs() {
        let (s, _d) = store();
        let keep = s.put_bytes(b"keep").await.unwrap();
        let drop = s.put_bytes(b"drop").await.unwrap();
        let removed = s.gc(&HashSet::from([keep.hash.clone()])).await.unwrap();
        assert_eq!(removed, 1);
        assert!(s.exists(&keep.hash));
        assert!(!s.exists(&drop.hash));
    }

    #[tokio::test]
    async fn gc_on_missing_dir_is_noop() {
        let (s, _d) = store();
        assert_eq!(s.gc(&HashSet::new()).await.unwrap(), 0);
    }
}
