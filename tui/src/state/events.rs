//! 隧道事件处理与定时刷新。

use std::time::Instant;

use sculk::tunnel::TunnelEvent;

use crate::state::{AppState, TunnelPhase, persist};
use crate::tunnel;
use crate::tunnel::AppEvent;

/// 处理来自隧道任务的内部事件。
pub(crate) fn handle_app_event(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::HostStarted {
            tunnel,
            ticket,
            events,
        } => {
            state.ctx.startup_handle = None;
            state.phase = TunnelPhase::Active;
            state.quit_pressed_at = None;
            state.ctx.tunnel = Some(tunnel);

            let quoted = format!("\"{ticket}\"");
            if persist::clipboard_copy(&quoted) {
                state.add_log("票据已复制到剪贴板");
            }
            state.ticket = Some(ticket);

            state.add_log("host 隧道已启动");
            state.ctx.event_forwarder = Some(tunnel::spawn_event_forwarder(
                events,
                state.ctx.app_tx.clone(),
            ));
        }
        AppEvent::JoinConnected { tunnel, events } => {
            state.ctx.startup_handle = None;
            state.phase = TunnelPhase::Active;
            state.quit_pressed_at = None;
            state.ctx.tunnel = Some(tunnel);
            state.add_log("已成功连入隧道");

            state.ctx.profile.join.last_ticket = Some(state.join_ticket.value.clone());
            let _ = persist::save_profile(&state.ctx.profile);

            state.ctx.event_forwarder = Some(tunnel::spawn_event_forwarder(
                events,
                state.ctx.app_tx.clone(),
            ));
        }
        AppEvent::StartFailed(msg) => {
            state.ctx.startup_handle = None;
            state.phase = TunnelPhase::Idle;
            state.quit_pressed_at = None;
            state.active_mode = None;
            state.add_log(&msg);
        }
        AppEvent::Closed => {
            state.phase = TunnelPhase::Idle;
            state.quit_pressed_at = None;
            state.active_mode = None;
            state.ctx.tunnel = None;
            state.ticket = None;
            state.connections.clear();
            state.ctx.event_forwarder = None;
            state.add_log("隧道已关闭");
        }
        AppEvent::Tunnel(te) => handle_tunnel_event(state, te),
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
    };
    state.add_log(&msg);
}

/// 定时刷新：递增 tick、清理退出提示、更新连接快照。
pub(crate) fn on_tick(state: &mut AppState) {
    state.tick = state.tick.saturating_add(1);

    if let Some(prev) = state.quit_pressed_at
        && Instant::now().duration_since(prev).as_secs() >= 3
    {
        state.quit_pressed_at = None;
    }

    if state.phase == TunnelPhase::Active
        && let Some(ref tunnel) = state.ctx.tunnel
    {
        state.connections = tunnel.connections();
    }
}
