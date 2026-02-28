//! 密钥 + 中继配置管理（移植自 cli/key.rs + cli/relay.rs）。

use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use sculk_core::tunnel::{RelayUrl, SecretKey};

const KEY_LEN: usize = 32;

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine system data directory")
        .join("sculk")
}

pub fn default_key_path() -> PathBuf {
    data_dir().join("secret.key")
}

pub fn default_relay_conf_path() -> PathBuf {
    data_dir().join("relay.conf")
}

// ---- 密钥管理 ----

pub fn load_or_generate_key(path: &Path) -> anyhow::Result<SecretKey> {
    if path.exists() {
        load_key(path)
    } else {
        generate_new_key(path)
    }
}

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

// ---- 中继配置 ----

pub fn load_relay_url(path: &Path) -> anyhow::Result<Option<RelayUrl>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read relay config: {}", path.display()))?;
    let url = content
        .trim()
        .parse::<RelayUrl>()
        .with_context(|| format!("invalid relay URL: {}", content.trim()))?;
    Ok(Some(url))
}

pub fn save_relay_url(path: &Path, url: &str) -> anyhow::Result<()> {
    url.parse::<RelayUrl>()
        .with_context(|| format!("invalid relay URL: {url}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory: {}", parent.display()))?;
    }
    std::fs::write(path, url.trim())
        .with_context(|| format!("failed to write relay config: {}", path.display()))?;
    Ok(())
}

pub fn remove_relay_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove relay config: {}", path.display()))?;
    }
    Ok(())
}

/// 解析 relay 地址，优先级：自定义 URL > 配置文件 > None（默认 n0）
pub fn resolve_relay_url(custom: Option<&str>) -> anyhow::Result<Option<RelayUrl>> {
    if let Some(url_str) = custom {
        let url: RelayUrl = url_str
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid relay URL: {e}"))?;
        return Ok(Some(url));
    }
    load_relay_url(&default_relay_conf_path())
}
