//! Secret key file management: load, generate, and persist iroh SecretKey.

use std::path::Path;

use anyhow::{Context, ensure};
use sculk_core::tunnel::SecretKey;

const KEY_LEN: usize = 32;

/// Load key from file; generate and save a new one if file does not exist.
pub fn load_or_generate_key(path: &Path) -> anyhow::Result<SecretKey> {
    if path.exists() {
        load_key(path)
    } else {
        generate_new_key(path)
    }
}

/// Force-generate a new key and save (overwrites existing file).
pub fn generate_new_key(path: &Path) -> anyhow::Result<SecretKey> {
    let key = SecretKey::generate(&mut rand::rng());
    save_key(path, &key)?;
    Ok(key)
}

fn load_key(path: &Path) -> anyhow::Result<SecretKey> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read key file: {}", path.display()))?;
    ensure!(
        bytes.len() == KEY_LEN,
        "invalid key file length: expected {KEY_LEN} bytes, got {} bytes",
        bytes.len()
    );
    let arr: [u8; KEY_LEN] = bytes.try_into().unwrap();
    Ok(SecretKey::from_bytes(&arr))
}

fn save_key(path: &Path, key: &SecretKey) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create key directory: {}", parent.display()))?;
    }
    std::fs::write(path, key.to_bytes())
        .with_context(|| format!("failed to write key file: {}", path.display()))?;
    Ok(())
}
