//! Relay 配置文件管理：加载、保存和删除自定义 relay 地址。
//!
//! 配置文件是纯文本格式，仅包含一个 URL 字符串。
//! 路径：`{data_dir}/sculk/relay.conf`

use std::path::Path;

use anyhow::Context;
use sculk_core::tunnel::RelayUrl;

/// 从配置文件加载 relay 地址；文件不存在时返回 `None`。
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

/// 保存 relay 地址到配置文件。
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

/// 删除 relay 配置文件，恢复默认 n0 relay。
pub fn remove_relay_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove relay config: {}", path.display()))?;
    }
    Ok(())
}
