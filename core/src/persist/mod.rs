//! 应用数据持久化：iroh 密钥管理与用户偏好 TOML Profile。
//!
//! 需在 `sculk-core` 中启用 `persist` feature（默认关闭）。
//! 数据目录为 `{dirs::data_dir()}/sculk/`：
//! - macOS：`~/Library/Application Support/sculk/`
//! - Linux：`~/.local/share/sculk/`
//! - Windows：`%APPDATA%\sculk\`

mod key;
mod profile;

use std::path::PathBuf;

pub use key::{generate_new_key, load_or_generate_key};
pub use profile::{HostProfile, JoinProfile, Profile, RelayProfile};

/// 应用数据目录。
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine system data directory")
        .join("sculk")
}

/// 默认密钥文件路径。
pub fn default_key_path() -> PathBuf {
    data_dir().join("secret.key")
}
