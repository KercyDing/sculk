//! 状态只读派生：状态标签、链路指标与 Esc 能力。

use crate::state::{ActiveTab, AppState, TunnelPhase};

/// 返回当前状态标签与配色。
pub(crate) fn status_label(state: &AppState) -> (&str, crate::ui::theme::StatusColor) {
    use crate::ui::theme::StatusColor;
    match state.phase {
        TunnelPhase::Idle => ("空闲", StatusColor::Warn),
        TunnelPhase::Starting => ("连接中...", StatusColor::Info),
        TunnelPhase::Active => match state.active_mode {
            Some(ActiveTab::Host) => ("托管中", StatusColor::Accent),
            Some(ActiveTab::Join) => ("已加入", StatusColor::Info),
            _ => ("活跃", StatusColor::Accent),
        },
        TunnelPhase::Stopping => ("关闭中...", StatusColor::Warn),
    }
}

/// 连接质量百分比：0ms≈98%，≥500ms→10%，无连接且隧道活跃时返回50。
pub(crate) fn route_strength(state: &AppState) -> u8 {
    if !state.connections.is_empty() {
        let avg_rtt: u64 = state.connections.iter().map(|c| c.rtt_ms).sum::<u64>()
            / state.connections.len() as u64;
        ((100_u64.saturating_sub(avg_rtt / 5)).clamp(10, 98)) as u8
    } else if state.phase == TunnelPhase::Active {
        50
    } else {
        0
    }
}

/// 当前链路类型。
pub(crate) fn route_info(state: &AppState) -> &str {
    if let Some(conn) = state.connections.first() {
        if conn.is_relay { "中继" } else { "直连" }
    } else {
        "无"
    }
}

/// 链路 Gauge 标签。
pub(crate) fn gauge_label(state: &AppState) -> String {
    if state.connections.is_empty() {
        if state.phase == TunnelPhase::Active {
            "等待连接...".to_string()
        } else {
            "离线".to_string()
        }
    } else {
        let avg_rtt: u64 = state.connections.iter().map(|c| c.rtt_ms).sum::<u64>()
            / state.connections.len() as u64;
        let mode = state.route_info();
        format!(
            "{}% | {}ms | {} | {}人",
            route_strength(state),
            avg_rtt,
            mode,
            state.connections.len()
        )
    }
}

/// 连接数标签。
pub(crate) fn connection_label(state: &AppState) -> String {
    if state.connections.is_empty() {
        "0".to_string()
    } else {
        format!("{}", state.connections.len())
    }
}

/// 当前中继标签。
pub(crate) fn relay_label(state: &AppState) -> &str {
    crate::state::RELAYS.get(state.relay_idx).unwrap_or(&"未知")
}

/// Esc 在当前生命周期对应动作文案。
pub(crate) fn esc_action_label(state: &AppState) -> &'static str {
    if state.phase == TunnelPhase::Idle {
        "退出"
    } else {
        "断开"
    }
}

/// 当前是否允许 Esc 双击退出。
pub(crate) fn esc_can_exit(state: &AppState) -> bool {
    state.phase == TunnelPhase::Idle
}
