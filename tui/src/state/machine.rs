//! 分层状态机：生命周期与 UI 叠加态。

use std::time::Instant;

use crate::state::{AppState, InputMode, Step, TunnelPhase};

/// UI 叠加态
/// 优先级从高到低：ConfirmStop > Help > Editing > Normal。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiOverlayState {
    ConfirmStop,
    Help,
    Editing,
    Normal,
}

/// 读取当前 UI 叠加态。
pub(crate) fn overlay_state(state: &AppState) -> UiOverlayState {
    if state.confirm_stop {
        UiOverlayState::ConfirmStop
    } else if state.show_help {
        UiOverlayState::Help
    } else if state.input_mode == InputMode::Editing {
        UiOverlayState::Editing
    } else {
        UiOverlayState::Normal
    }
}

/// 处理 Normal 态下 Esc 与生命周期状态机迁移。
pub(crate) fn handle_lifecycle_esc(state: &mut AppState) -> Step {
    match state.phase {
        TunnelPhase::Starting => {
            if let Some(handle) = state.ctx.startup_handle.take() {
                handle.abort();
            }
            state.phase = TunnelPhase::Idle;
            state.active_mode = None;
            state.quit_pressed_at = None;
            state.add_log("已取消启动");
            Step::Continue
        }
        TunnelPhase::Active => {
            state.quit_pressed_at = None;
            state.confirm_stop = true;
            Step::Continue
        }
        TunnelPhase::Stopping => {
            state.quit_pressed_at = None;
            Step::Continue
        }
        TunnelPhase::Idle => {
            let now = Instant::now();
            if let Some(prev) = state.quit_pressed_at
                && now.duration_since(prev).as_secs() < 1
            {
                return Step::Exit;
            }
            state.quit_pressed_at = Some(now);
            Step::Continue
        }
    }
}
