//! Relay config file management: load, save, and remove custom relay URL.
//!
//! Config is a plain text file containing a single URL string.
//! Path: `{data_dir}/sculk/relay.conf`

use std::path::Path;

use anyhow::Context;
use sculk_core::tunnel::RelayUrl;

/// Load relay URL from config file. Returns `None` if file does not exist.
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

/// Save relay URL to config file.
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

/// Remove relay config file, restoring default n0 relay servers.
pub fn remove_relay_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove relay config: {}", path.display()))?;
    }
    Ok(())
}
