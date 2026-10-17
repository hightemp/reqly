//! Test helpers: a throwaway on-disk database.

use tempfile::TempDir;

use crate::{OpenOptions, Storage};

/// Opens a migrated database in a temp dir. Keep the `TempDir` alive.
pub async fn temp_storage() -> (Storage, TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let storage = Storage::open(&dir.path().join("reqly.db"), OpenOptions::default())
        .await
        .expect("open temp storage");
    (storage, dir)
}
