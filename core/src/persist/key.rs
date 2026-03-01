//! 密钥文件管理：加载、生成并持久化 iroh `SecretKey`。

use std::path::Path;

use anyhow::{Context, ensure};

use crate::tunnel::SecretKey;

const KEY_LEN: usize = 32;

/// 从文件加载密钥；若文件不存在则生成新密钥并保存。
pub fn load_or_generate_key(path: &Path) -> anyhow::Result<SecretKey> {
    if path.exists() {
        load_key(path)
    } else {
        generate_new_key(path)
    }
}

/// 强制重新生成新密钥并保存。
pub fn generate_new_key(path: &Path) -> anyhow::Result<SecretKey> {
    let bytes: [u8; KEY_LEN] = rand::random();
    let key = SecretKey::from_bytes(&bytes);
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

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to open key file: {}", path.display()))?;
        file.write_all(&key.to_bytes())
            .with_context(|| format!("failed to write key file: {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, key.to_bytes())
            .with_context(|| format!("failed to write key file: {}", path.display()))?;
    }

    Ok(())
}
