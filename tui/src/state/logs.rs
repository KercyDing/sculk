//! 日志与列表选择逻辑。

use crate::state::{AppState, LOG_CAP, RELAYS};

/// 清空日志并写入提示。
pub(crate) fn clear_logs(state: &mut AppState) {
    state.logs.clear();
    state.log_state.select(None);
    add_log(state, "日志已清空");
}

/// 将日志追加到队列，超出 `LOG_CAP` 时丢弃最早条目。
pub(crate) fn add_log(state: &mut AppState, msg: &str) {
    state.logs.push(msg.to_string());
    if state.logs.len() > LOG_CAP {
        let to_drop = state.logs.len() - LOG_CAP;
        state.logs.drain(0..to_drop);
    }
    state
        .log_state
        .select(Some(state.logs.len().saturating_sub(1)));
}

/// 日志选择向后移动，到末尾停止。
pub(crate) fn next_log(state: &mut AppState) {
    if state.logs.is_empty() {
        state.log_state.select(None);
        return;
    }
    let next = match state.log_state.selected() {
        Some(i) if i + 1 < state.logs.len() => i + 1,
        Some(i) => i,
        None => 0,
    };
    state.log_state.select(Some(next));
}

/// 日志选择向前移动，到首位停止。
pub(crate) fn prev_log(state: &mut AppState) {
    if state.logs.is_empty() {
        state.log_state.select(None);
        return;
    }
    let prev = match state.log_state.selected() {
        Some(0) => 0,
        Some(i) => i - 1,
        None => 0,
    };
    state.log_state.select(Some(prev));
}

/// 中继选项向后移动，到末尾停止。
pub(crate) fn next_relay_selection(state: &mut AppState) {
    let next = match state.relay_state.selected() {
        Some(i) if i + 1 < RELAYS.len() => i + 1,
        Some(i) => i,
        None => 0,
    };
    state.relay_state.select(Some(next));
}

/// 中继选项向前移动，到首位停止。
pub(crate) fn prev_relay_selection(state: &mut AppState) {
    let prev = match state.relay_state.selected() {
        Some(0) | None => 0,
        Some(i) => i - 1,
    };
    state.relay_state.select(Some(prev));
}
