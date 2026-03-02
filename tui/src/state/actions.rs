//! 主要动作：启动/加入/停止隧道与中继应用。

use crate::services::{persist, tunnel};
use crate::state::RELAYS;
use crate::state::{ActiveTab, AppState, TunnelPhase};

/// 根据当前标签页执行主操作。
pub(crate) fn primary_action(state: &mut AppState) {
    match state.tab {
        ActiveTab::Host => toggle_host(state),
        ActiveTab::Join => toggle_join(state),
        ActiveTab::Relay => apply_relay(state),
    }
}

/// Host 标签页动作：启动或停止 Host 隧道。
pub(crate) fn toggle_host(state: &mut AppState) {
    match state.phase {
        TunnelPhase::Idle => {
            let port: u16 = match state.host_port.value.parse() {
                Ok(p) => p,
                Err(_) => {
                    state.add_log("端口格式错误");
                    return;
                }
            };
            let password = if state.host_password.value.is_empty() {
                None
            } else {
                Some(state.host_password.value.clone())
            };

            let key_path = match persist::default_key_path() {
                Ok(path) => path,
                Err(e) => {
                    state.add_log(&format!("密钥路径获取失败: {e}"));
                    return;
                }
            };
            let secret_key = match persist::load_or_generate_key(&key_path) {
                Ok(k) => k,
                Err(e) => {
                    state.add_log(&format!("密钥加载失败: {e}"));
                    return;
                }
            };

            let custom_relay = if state.relay_idx == 1 {
                Some(state.relay_url.value.as_str())
            } else {
                None
            };
            let relay_url = match persist::resolve_relay_url(&state.ctx.profile, custom_relay) {
                Ok(r) => r,
                Err(e) => {
                    state.add_log(&format!("中继配置错误: {e}"));
                    return;
                }
            };

            state.phase = TunnelPhase::Starting;
            state.active_mode = Some(ActiveTab::Host);
            state.quit_pressed_at = None;
            state.add_log(&format!("正在启动 host 隧道 (端口 {port})..."));
            state.ctx.startup_handle = Some(tunnel::spawn_host(
                port,
                secret_key,
                relay_url,
                password,
                state.ctx.app_tx.clone(),
            ));
        }
        TunnelPhase::Active if state.active_mode == Some(ActiveTab::Host) => {
            stop_tunnel(state);
        }
        _ => {
            state.add_log("隧道运行中，请先停止当前隧道");
        }
    }
}

/// Join 标签页动作：连接或停止 Join 隧道。
pub(crate) fn toggle_join(state: &mut AppState) {
    match state.phase {
        TunnelPhase::Idle => {
            if state.join_ticket.value.is_empty() {
                state.add_log("请先输入票据");
                return;
            }
            let port: u16 = match state.join_port.value.parse() {
                Ok(p) => p,
                Err(_) => {
                    state.add_log("端口格式错误");
                    return;
                }
            };
            let password = if state.join_password.value.is_empty() {
                None
            } else {
                Some(state.join_password.value.clone())
            };

            state.phase = TunnelPhase::Starting;
            state.active_mode = Some(ActiveTab::Join);
            state.quit_pressed_at = None;
            state.add_log("正在连接...");
            state.ctx.startup_handle = Some(tunnel::spawn_join(
                &state.join_ticket.value,
                port,
                password,
                state.ctx.app_tx.clone(),
            ));
        }
        TunnelPhase::Active if state.active_mode == Some(ActiveTab::Join) => {
            stop_tunnel(state);
        }
        _ => {
            state.add_log("隧道运行中，请先停止当前隧道");
        }
    }
}

/// 应用中继配置。
pub(crate) fn apply_relay(state: &mut AppState) {
    if state.phase != TunnelPhase::Idle {
        state.add_log("隧道运行中，无法切换中继");
        return;
    }
    let selected = match state.relay_state.selected() {
        Some(idx) => idx,
        None => state.relay_idx,
    };

    match selected {
        0 => {
            if selected == state.relay_idx {
                state.add_log(&format!(
                    "中继保持不变: {}",
                    RELAYS.get(state.relay_idx).unwrap_or(&"未知")
                ));
                return;
            }
            state.ctx.profile.relay.custom = false;
            if let Err(e) = persist::save_profile(&state.ctx.profile) {
                state.add_log(&format!("重置中继失败: {e}"));
                return;
            }
        }
        1 => {
            let url = state.relay_url.value.trim().to_string();
            if url.is_empty() {
                state.add_log("请先输入自建中继 URL");
                return;
            }
            if let Err(e) = persist::resolve_relay_url(&state.ctx.profile, Some(&url)) {
                state.add_log(&format!("保存失败: {e}"));
                return;
            }
            state.ctx.profile.relay.custom = true;
            state.ctx.profile.relay.url = Some(url);
            if let Err(e) = persist::save_profile(&state.ctx.profile) {
                state.add_log(&format!("保存失败: {e}"));
                return;
            }
        }
        _ => {
            if selected == state.relay_idx {
                state.add_log(&format!(
                    "中继保持不变: {}",
                    RELAYS.get(state.relay_idx).unwrap_or(&"未知")
                ));
                return;
            }
        }
    }

    let selected = selected.min(RELAYS.len().saturating_sub(1));
    state.relay_idx = selected;
    state.add_log(&format!(
        "中继已切换到 {}",
        RELAYS.get(state.relay_idx).unwrap_or(&"未知")
    ));
}

/// 停止当前隧道。
pub(crate) fn stop_tunnel(state: &mut AppState) {
    if let Some(handle) = state.ctx.event_forwarder.take() {
        handle.abort();
    }
    if let Some(t) = state.ctx.tunnel.take() {
        state.phase = TunnelPhase::Stopping;
        state.quit_pressed_at = None;
        state.add_log("正在关闭隧道...");
        tunnel::spawn_close(t, state.ctx.app_tx.clone());
    }
}
