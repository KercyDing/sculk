//! 隧道事件处理与定时刷新。

use std::time::Instant;

use sculk::tunnel::{TunnelEvent, TunnelMode, TunnelStatus, TunnelUpdate};

use crate::services::persist;
use crate::state::{AppState, TunnelPhase};

/// 处理 core 统一订阅发布的更新。
pub(crate) fn handle_tunnel_update(state: &mut AppState, update: TunnelUpdate) {
    match update {
        TunnelUpdate::Status(status) => apply_status(state, status),
        TunnelUpdate::Event(event) => handle_tunnel_event(state, event),
        _ => {}
    }
}

fn apply_status(state: &mut AppState, status: TunnelStatus) {
    let previous_phase = state.phase;
    let previous_mode = state.active_mode;
    let next_mode = match status.state.mode {
        Some(TunnelMode::Host) => Some(crate::state::ActiveTab::Host),
        Some(TunnelMode::Join) => Some(crate::state::ActiveTab::Join),
        None => None,
    };
    let next_ticket = status.state.ticket.as_ref().map(ToString::to_string);

    state.phase = status.state.phase;
    state.active_mode = next_mode;
    state.ticket = next_ticket;
    state.connections = status.connections;

    if state.phase != previous_phase {
        state.quit_pressed_at = None;
    }
    if state.phase == TunnelPhase::Active && previous_phase == TunnelPhase::Starting {
        match state.active_mode {
            Some(crate::state::ActiveTab::Host) => {
                state.host_password.clear();
                state.add_log("host 隧道已启动");
            }
            Some(crate::state::ActiveTab::Join) => {
                state.join_password.clear();
                state.add_log("已成功连入隧道");
                state.ctx.profile.join.last_ticket = Some(state.join_ticket.value.clone());
                if let Err(e) = persist::save_profile(&state.ctx.profile) {
                    state.add_log(&format!("配置保存失败: {e}"));
                }
            }
            Some(crate::state::ActiveTab::Relay) | None => {}
        }
    }
    if state.phase == TunnelPhase::Idle && previous_phase != TunnelPhase::Idle {
        state.host_password.clear();
        state.join_password.clear();
        if matches!(previous_phase, TunnelPhase::Active | TunnelPhase::Stopping) {
            state.add_log("隧道已关闭");
        }
    }
    if previous_mode != state.active_mode && state.phase == TunnelPhase::Idle {
        state.active_mode = None;
    }
}

/// 处理隧道细粒度事件。
pub(crate) fn handle_tunnel_event(state: &mut AppState, event: TunnelEvent) {
    let msg = match &event {
        TunnelEvent::PlayerJoined { id } => format!("玩家加入: {id}"),
        TunnelEvent::PlayerLeft { id, reason } => format!("玩家离开: {id} ({reason})"),
        TunnelEvent::Connected => "已连接到 host".to_string(),
        TunnelEvent::Disconnected { reason } => format!("连接断开: {reason}"),
        TunnelEvent::PathChanged {
            remote_id,
            is_relay,
            rtt_ms,
        } => {
            let mode = if *is_relay { "中继" } else { "直连" };
            format!("{remote_id} 路径: {mode}, RTT: {rtt_ms}ms")
        }
        TunnelEvent::Reconnecting { attempt } => format!("正在重连 (第 {attempt} 次)..."),
        TunnelEvent::Reconnected => "重连成功".to_string(),
        TunnelEvent::AuthFailed { id } => format!("认证失败: {id}"),
        TunnelEvent::PlayerRejected { id, reason } => format!("玩家被拒: {id} ({reason})"),
        TunnelEvent::Error { message } => format!("错误: {message}"),
        _ => "未知事件".to_string(),
    };
    state.add_log(&msg);
}

/// 定时刷新：递增 tick 并清理退出提示。
pub(crate) fn on_tick(state: &mut AppState) {
    state.tick = state.tick.saturating_add(1);

    if let Some(prev) = state.quit_pressed_at
        && Instant::now().duration_since(prev).as_secs() >= 1
    {
        state.quit_pressed_at = None;
    }
}
