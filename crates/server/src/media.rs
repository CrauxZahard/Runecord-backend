//! Encrypted media storage.
//!
//! Content-addressed CDN files are sealed with the database key and written
//! under the server's `data` directory. Keeping this in one place prevents each
//! feature handler from reimplementing hashing, extension handling and
//! encryption-on-write.

use crate::crypto;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct StoredAsset {
    pub hash: String,
    pub stored: String,
    pub url: String,
    pub size: u64,
}

pub struct MediaStore {
    data_dir: PathBuf,
    key: [u8; 32],
}

impl MediaStore {
    pub fn new(data_dir: PathBuf, key: [u8; 32]) -> Self {
        Self { data_dir, key }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cdn_path(&self, file: &str) -> PathBuf {
        self.data_dir.join("cdn").join(file)
    }

    pub fn thumb_path(&self, hash: &str, width: u32) -> PathBuf {
        self.data_dir
            .join("thumbs")
            .join(format!("{hash}-{width}.enc"))
    }

    /// Stores an asset under a filename-derived extension.
    pub fn save(&self, bytes: &[u8], filename: &str) -> std::io::Result<StoredAsset> {
        let ext = filename.rsplit('.').next().unwrap_or("bin").to_lowercase();
        self.save_with_ext(bytes, &ext)
    }

    /// Stores an image or other asset when the caller already knows the extension.
    pub fn save_with_ext(&self, bytes: &[u8], ext: &str) -> std::io::Result<StoredAsset> {
        let hash = hash_bytes(bytes);
        let stored = format!("{hash}.{ext}");
        let path = self.cdn_path(&stored);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if !path.exists() {
            std::fs::write(&path, crypto::seal(&self.key, bytes))?;
        }
        Ok(StoredAsset {
            url: format!("/cdn/{stored}"),
            hash,
            stored,
            size: bytes.len() as u64,
        })
    }

    /// Reads and decrypts a CDN file.
    pub fn read(&self, file: &str) -> Option<Vec<u8>> {
        let sealed = std::fs::read(self.cdn_path(file)).ok()?;
        crypto::open(&self.key, &sealed)
    }

    pub fn read_thumb(&self, hash: &str, width: u32) -> Option<Vec<u8>> {
        let sealed = std::fs::read(self.thumb_path(hash, width)).ok()?;
        crypto::open(&self.key, &sealed)
    }

    pub fn write_thumb(&self, hash: &str, width: u32, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.thumb_path(hash, width);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, crypto::seal(&self.key, bytes))
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
