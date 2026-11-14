use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use std::path::Path;

pub fn load_key(data: &Path) -> std::io::Result<[u8; 32]> {
    let path = data.join("key.bin");
    if let Ok(bytes) = std::fs::read(&path)
        && bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    let key: [u8; 32] = rand::random();
    std::fs::create_dir_all(data)?;
    std::fs::write(&path, key)?;
    Ok(key)
}

pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce: [u8; 12] = rand::random();
    let mut out = Vec::with_capacity(12 + plaintext.len() + 16);
    out.extend_from_slice(&nonce);
    if let Ok(ct) = cipher.encrypt(Nonce::from_slice(&nonce), plaintext) {
        out.extend_from_slice(&ct);
    }
    out
}

pub fn open(key: &[u8; 32], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(&data[..12]), &data[12..])
        .ok()
}
