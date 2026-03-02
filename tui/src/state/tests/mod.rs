use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tokio::sync::mpsc;

use super::{ActiveTab, AppState, FocusPane, FooterTone, InputMode, RELAYS, Step, TunnelPhase};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn test_state() -> AppState {
    let (tx, _rx) = mpsc::unbounded_channel();
    AppState::new(tx)
}

#[test]
fn quit_keys_exit() {
    let mut state = test_state();
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Step::Continue
    ));
    assert!(state.quit_pressed_at.is_some());
    assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));

    let mut state = test_state();
    state.input_mode = InputMode::Editing;
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Step::Continue
    ));
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Step::Continue
    ));
}

#[test]
fn esc_does_not_exit_while_stopping() {
    let mut state = test_state();
    state.phase = TunnelPhase::Stopping;

    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Step::Continue
    ));
    assert!(state.quit_pressed_at.is_none());
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Step::Continue
    ));
}

#[test]
fn switch_tab_and_toggle_help() {
    let mut state = test_state();
    assert_eq!(state.tab, ActiveTab::Host);
    state.handle_key(key(KeyCode::Right));
    assert_eq!(state.tab, ActiveTab::Join);
    state.handle_key(key(KeyCode::Left));
    assert_eq!(state.tab, ActiveTab::Host);
    assert!(!state.show_help);
    state.handle_key(key(KeyCode::Char('h')));
    assert!(state.show_help);
}

#[test]
fn tab_selection_clamps_at_edges() {
    let mut state = test_state();
    state.handle_key(key(KeyCode::Left));
    assert_eq!(state.tab, ActiveTab::Host);
    state.tab = ActiveTab::Relay;
    state.handle_key(key(KeyCode::Right));
    assert_eq!(state.tab, ActiveTab::Relay);
}

#[test]
fn relay_apply() {
    let mut state = test_state();
    state.tab = ActiveTab::Relay;
    state.relay_url.value = "https://relay.example.com".to_string();
    let old = state.relay_idx;
    state.relay_state.select(Some((old + 1) % RELAYS.len()));
    state.primary_action();
    assert_ne!(old, state.relay_idx);
}

#[test]
fn log_selection_clamps_at_edges() {
    let mut state = test_state();
    state.add_log("a");
    state.add_log("b");
    state.add_log("c");
    state.log_state.select(Some(3));
    state.next_log();
    assert_eq!(state.log_state.selected(), Some(3));
    state.prev_log();
    assert_eq!(state.log_state.selected(), Some(2));
    state.log_state.select(Some(0));
    state.prev_log();
    assert_eq!(state.log_state.selected(), Some(0));
}

#[test]
fn relay_tab_up_down_moves_relay_selection() {
    let mut state = test_state();
    state.tab = ActiveTab::Relay;
    state.focus = FocusPane::Profile;
    state.relay_state.select(Some(0));
    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.relay_state.selected(), Some(1));
    state.handle_key(key(KeyCode::Up));
    assert_eq!(state.relay_state.selected(), Some(0));
}

#[test]
fn editing_mode_inserts_chars() {
    let mut state = test_state();
    state.input_mode = InputMode::Editing;
    state.host_port.clear();
    state.handle_key(key(KeyCode::Char('8')));
    state.handle_key(key(KeyCode::Char('0')));
    assert_eq!(state.host_port.value, "80");
    state.handle_key(key(KeyCode::Esc));
    assert_eq!(state.input_mode, InputMode::Normal);
}

#[tokio::test]
async fn enter_editing_with_e_key() {
    let mut state = test_state();
    state.focus = FocusPane::Profile;
    state.handle_key(key(KeyCode::Enter));
    assert_eq!(state.input_mode, InputMode::Normal);
    state.handle_key(key(KeyCode::Char('i')));
    assert_eq!(state.input_mode, InputMode::Editing);
}

#[test]
fn route_strength_mapping() {
    let mut state = test_state();
    assert_eq!(state.route_strength(), 0);
    assert_eq!(state.route_info(), "无");

    state.phase = TunnelPhase::Active;
    assert_eq!(state.route_strength(), 50);
}

#[test]
fn gauge_label_offline() {
    let state = test_state();
    assert_eq!(state.gauge_label(), "离线");
}

#[test]
fn gauge_label_active_waiting() {
    let mut state = test_state();
    state.phase = TunnelPhase::Active;
    assert_eq!(state.gauge_label(), "等待连接...");
}

#[test]
fn status_label_phases() {
    let mut state = test_state();
    let (label, _) = state.status_label();
    assert_eq!(label, "空闲");

    state.phase = TunnelPhase::Starting;
    let (label, _) = state.status_label();
    assert_eq!(label, "连接中...");

    state.phase = TunnelPhase::Active;
    state.active_mode = Some(ActiveTab::Host);
    let (label, _) = state.status_label();
    assert_eq!(label, "托管中");

    state.active_mode = Some(ActiveTab::Join);
    let (label, _) = state.status_label();
    assert_eq!(label, "已加入");
}

#[test]
fn handle_app_event_closed() {
    use crate::services::tunnel::AppEvent;

    let mut state = test_state();
    state.phase = TunnelPhase::Active;
    state.active_mode = Some(ActiveTab::Host);
    state.ticket = Some("test".to_string());

    state.handle_app_event(AppEvent::Closed);

    assert_eq!(state.phase, TunnelPhase::Idle);
    assert!(state.active_mode.is_none());
    assert!(state.ticket.is_none());
    assert!(state.connections.is_empty());
}

#[test]
fn handle_app_event_start_failed() {
    use crate::services::tunnel::AppEvent;

    let mut state = test_state();
    state.phase = TunnelPhase::Starting;
    state.active_mode = Some(ActiveTab::Host);

    state.handle_app_event(AppEvent::StartFailed("test error".into()));

    assert_eq!(state.phase, TunnelPhase::Idle);
    assert!(state.active_mode.is_none());
    assert!(
        state
            .logs
            .last()
            .is_some_and(|msg| msg.contains("test error"))
    );
}

#[test]
fn esc_action_label_changes_with_phase() {
    let mut state = test_state();

    state.phase = TunnelPhase::Idle;
    assert_eq!(state.esc_action_label(), "退出");
    assert!(state.esc_can_exit());

    state.phase = TunnelPhase::Starting;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());

    state.phase = TunnelPhase::Active;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());

    state.phase = TunnelPhase::Stopping;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());
}

#[test]
fn footer_spec_follows_state() {
    let mut state = test_state();

    let spec = state.footer_spec();
    assert!(
        spec.left
            .iter()
            .any(|item| item.key == "Esc" && item.label == "退出")
    );
    assert!(spec.right_hint.is_none());

    state.quit_pressed_at = Some(std::time::Instant::now());
    let spec = state.footer_spec();
    assert_eq!(spec.right_hint.as_deref(), Some("再次按 Esc 退出"));

    state.phase = TunnelPhase::Active;
    let spec = state.footer_spec();
    assert!(
        spec.left
            .iter()
            .any(|item| item.key == "Esc" && item.label == "断开")
    );
    assert!(spec.right_hint.is_none());

    state.input_mode = InputMode::Editing;
    let spec = state.footer_spec();
    assert!(
        spec.left
            .iter()
            .any(|item| item.key == "编辑模式" && item.tone == FooterTone::Info)
    );
}

#[test]
fn logs_spec_ellipsis_for_unselected_row() {
    let mut state = test_state();
    state.logs.clear();
    state.log_state.select(None);
    state.add_log("0123456789");
    state.add_log("tail");
    state.log_state.select(Some(1));

    let spec = state.logs_spec(2, 5);
    assert_eq!(spec.rows.len(), 2);
    assert_eq!(spec.rows[0].text, "01...");
    assert_eq!(spec.rows[1].text, "tail");
}

#[test]
fn logs_spec_marquee_for_selected_row() {
    let mut state = test_state();
    state.logs.clear();
    state.log_state.select(None);
    state.add_log("abcdefghi");
    state.log_state.select(Some(0));

    state.tick = 0;
    let text0 = state.logs_spec(1, 6).rows[0].text.clone();
    state.tick = 1;
    let text1 = state.logs_spec(1, 6).rows[0].text.clone();

    assert_eq!(text0, "abcdef");
    assert_eq!(text1, "bcdefg");
}

#[test]
fn handle_app_event_close_failed_logs_error() {
    use crate::services::tunnel::AppEvent;

    let mut state = test_state();
    state.handle_app_event(AppEvent::CloseFailed("close failed".into()));
    assert!(
        state
            .logs
            .last()
            .is_some_and(|msg| msg.contains("close failed"))
    );
}

#[test]
fn logs_spec_uses_display_width_for_cjk_ellipsis_and_marquee() {
    let mut state = test_state();
    state.logs.clear();
    state.log_state.select(None);
    state.add_log("正在启动 host");
    state.add_log("tail");

    state.log_state.select(Some(1));
    let unselected = state.logs_spec(2, 11).rows[0].text.clone();
    assert_eq!(unselected, "正在启动...");

    state.log_state.select(Some(0));
    state.tick = 0;
    let text0 = state.logs_spec(1, 11).rows[0].text.clone();
    state.tick = 1;
    let text1 = state.logs_spec(1, 11).rows[0].text.clone();
    assert_ne!(text0, text1);
}
