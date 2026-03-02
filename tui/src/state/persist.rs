//! 状态层持久化与外部副作用封装。

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::state::AppState;

/// 加载 Profile，失败时返回默认配置与错误文本。
///
/// Purpose: 将配置加载失败从状态机逻辑中剥离，统一错误回退策略。
/// Args: 无。
/// Returns: `(Profile, Option<String>)`，`Option` 为可显示给用户的错误日志。
/// Edge Cases: 系统目录不可写或配置损坏时，回退到 `Default`。
pub(crate) fn load_profile() -> (sculk::persist::Profile, Option<String>) {
    match sculk::persist::Profile::load() {
        Ok(p) => (p, None),
        Err(e) => (Default::default(), Some(format!("配置加载失败: {e}"))),
    }
}

/// 保存 Profile。
///
/// Purpose: 将持久化写入统一封装，便于测试环境替换。
/// Args: `profile` 为待保存配置。
/// Returns: 保存成功返回 `Ok(())`。
/// Edge Cases: 测试环境下短路为成功，避免依赖系统目录权限。
#[cfg(not(test))]
pub(crate) fn save_profile(profile: &sculk::persist::Profile) -> Result<()> {
    profile.save()
}

/// 测试环境下跳过落盘写入。
#[cfg(test)]
pub(crate) fn save_profile(_profile: &sculk::persist::Profile) -> Result<()> {
    Ok(())
}

/// 解析最终使用的 Relay URL。
///
/// Purpose: 统一 relay URL 解析错误格式。
/// Args: `profile` 为当前配置；`custom` 为可选覆盖值。
/// Returns: 解析后的 relay URL（或 `None` 表示默认中继）。
/// Edge Cases: URL 非法时返回错误，不触发 panic。
pub(crate) fn resolve_relay_url(
    profile: &sculk::persist::Profile,
    custom: Option<&str>,
) -> Result<Option<sculk::tunnel::RelayUrl>> {
    profile.resolve_relay_url(custom)
}

/// 返回默认密钥路径。
///
/// Purpose: 解耦状态机与底层 persist API。
/// Args: 无。
/// Returns: 密钥文件路径。
/// Edge Cases: 由底层库决定路径可用性。
pub(crate) fn default_key_path() -> PathBuf {
    sculk::persist::default_key_path()
}

/// 读取或生成密钥。
///
/// Purpose: 启动 host 前获取密钥材料。
/// Args: `path` 为密钥文件路径。
/// Returns: 成功时返回密钥。
/// Edge Cases: 文件系统失败时返回错误。
pub(crate) fn load_or_generate_key(path: &Path) -> Result<sculk::tunnel::SecretKey> {
    sculk::persist::load_or_generate_key(path)
}

/// 尝试复制票据到剪贴板。
///
/// Purpose: 将外部副作用统一封装，便于测试替换。
/// Args: `content` 为待复制文本。
/// Returns: 成功返回 `true`。
/// Edge Cases: 测试环境恒为 `false`，不依赖系统剪贴板。
#[cfg(not(test))]
pub(crate) fn clipboard_copy(content: &str) -> bool {
    sculk::clipboard::clipboard_copy(content)
}

/// 测试环境下关闭剪贴板副作用。
#[cfg(test)]
pub(crate) fn clipboard_copy(_content: &str) -> bool {
    false
}

/// 编辑退出时同步 UI 字段到 Profile 并持久化。
///
/// Purpose: 将输入字段与持久化配置对齐。
/// Args: `state` 为应用状态。
/// Returns: 无。
/// Edge Cases: 端口解析失败时保留原配置；保存失败写入日志。
pub(crate) fn persist_profile_from_inputs(state: &mut AppState) {
    if let Ok(port) = state.host_port.value.parse::<u16>() {
        state.ctx.profile.host.port = port;
    }
    if let Ok(port) = state.join_port.value.parse::<u16>() {
        state.ctx.profile.join.port = port;
    }
    let relay_url = state.relay_url.value.trim().to_string();
    if relay_url.is_empty() {
        state.ctx.profile.relay.url = None;
    } else {
        state.ctx.profile.relay.url = Some(relay_url);
    }
    if let Err(e) = save_profile(&state.ctx.profile) {
        state.add_log(&format!("配置保存失败: {e}"));
    }
}
