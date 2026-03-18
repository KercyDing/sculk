//! 持久化与外部副作用服务。

use std::path::{Path, PathBuf};

use anyhow::Result;

/// 加载 Profile，失败时返回默认配置与错误文本。
///
/// Purpose: 将配置加载失败统一回退为默认值，避免调用方处理重复分支。
/// Args: 无。
/// Returns: `(Profile, Option<String>)`，`Option` 为可显示日志文本。
/// Edge Cases: 配置损坏或目录不可写时回退默认配置。
pub fn load_profile() -> (sculk::persist::Profile, Option<String>) {
    match sculk::persist::Profile::load() {
        Ok(p) => (p, None),
        Err(e) => (Default::default(), Some(format!("配置加载失败: {e}"))),
    }
}

/// 保存 Profile。
///
/// Purpose: 统一持久化保存入口，方便测试环境替换。
/// Args: `profile` 为待保存配置。
/// Returns: 保存成功返回 `Ok(())`。
/// Edge Cases: 测试环境直接返回成功，避免依赖宿主文件系统。
#[cfg(not(test))]
pub fn save_profile(profile: &sculk::persist::Profile) -> Result<()> {
    Ok(profile.save()?)
}

/// 测试环境下跳过落盘写入。
#[cfg(test)]
pub fn save_profile(_profile: &sculk::persist::Profile) -> Result<()> {
    Ok(())
}

/// 解析最终使用的 Relay URL。
///
/// Purpose: 统一 relay URL 解析错误格式。
/// Args: `profile` 为当前配置；`custom` 为可选覆盖值。
/// Returns: 解析后的 relay URL（或 `None` 表示默认中继）。
/// Edge Cases: URL 非法时返回错误。
pub fn resolve_relay_url(
    profile: &sculk::persist::Profile,
    custom: Option<&str>,
) -> Result<Option<sculk::RelayUrl>> {
    Ok(profile.resolve_relay_url(custom)?)
}

/// 返回默认密钥路径。
///
/// Purpose: 解耦状态层与底层 persist API。
/// Args: 无。
/// Returns: 密钥文件路径。
/// Edge Cases: 由底层库决定路径可用性。
pub fn default_key_path() -> Result<PathBuf> {
    sculk::persist::default_key_path().map_err(Into::into)
}

/// 读取或生成密钥。
///
/// Purpose: Host 启动前获取密钥材料。
/// Args: `path` 为密钥文件路径。
/// Returns: 成功时返回密钥。
/// Edge Cases: 文件系统错误时返回失败。
pub fn load_or_generate_key(path: &Path) -> Result<sculk::SecretKey> {
    Ok(sculk::persist::load_or_generate_key(path)?)
}

/// 尝试复制内容到剪贴板。
///
/// Purpose: 将剪贴板副作用集中在服务层。
/// Args: `content` 为待复制文本。
/// Returns: 成功返回 `true`。
/// Edge Cases: 测试环境恒为 `false`。
#[cfg(not(test))]
pub fn clipboard_copy(content: &str) -> bool {
    sculk::clipboard::clipboard_copy(content)
}

/// 测试环境下关闭剪贴板副作用。
#[cfg(test)]
pub fn clipboard_copy(_content: &str) -> bool {
    false
}
