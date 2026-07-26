use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use sculk::tunnel::{TunnelMode, TunnelService};

use crate::keymap::handle_key;
use crate::model::{ActiveTab, FocusPane, InputMode, Intent, Model, TunnelPhase};
use crate::ui::render_log_text;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn test_state() -> Model {
    let service = TunnelService::new();
    Model::new(&sculk::persist::Profile::default(), service.status(), None)
}

#[test]
fn quit_keys_exit() {
    let mut state = test_state();
    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::None
    ));
    assert!(state.quit_pressed_at.is_some());
    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::Exit
    ));

    let mut state = test_state();
    state.input_mode = InputMode::Editing;
    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::SaveInputs
    ));
    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::None
    ));
}

#[test]
fn esc_does_not_exit_while_stopping() {
    let mut state = test_state();
    state.tunnel.state.phase = TunnelPhase::Stopping;

    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::None
    ));
    assert!(state.quit_pressed_at.is_none());
    assert!(matches!(
        handle_key(&mut state, key(KeyCode::Esc)),
        Intent::None
    ));
}

#[test]
fn switch_tab_and_toggle_help() {
    let mut state = test_state();
    assert_eq!(state.tab, ActiveTab::Host);
    handle_key(&mut state, key(KeyCode::Right));
    assert_eq!(state.tab, ActiveTab::Join);
    handle_key(&mut state, key(KeyCode::Left));
    assert_eq!(state.tab, ActiveTab::Host);
    assert!(!state.show_help);
    handle_key(&mut state, key(KeyCode::Char('h')));
    assert!(state.show_help);
}

#[test]
fn tab_selection_clamps_at_edges() {
    let mut state = test_state();
    handle_key(&mut state, key(KeyCode::Left));
    assert_eq!(state.tab, ActiveTab::Host);
    state.tab = ActiveTab::Relay;
    handle_key(&mut state, key(KeyCode::Right));
    assert_eq!(state.tab, ActiveTab::Relay);
}

#[test]
fn enter_requests_primary_action() {
    let mut state = test_state();
    state.tab = ActiveTab::Relay;
    state.relay_url.value = "https://relay.example.com".to_string();
    state.relay_state.select(Some(1));
    assert_eq!(handle_key(&mut state, key(KeyCode::Enter)), Intent::Primary);
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
    handle_key(&mut state, key(KeyCode::Down));
    assert_eq!(state.relay_state.selected(), Some(1));
    handle_key(&mut state, key(KeyCode::Up));
    assert_eq!(state.relay_state.selected(), Some(0));
}

#[test]
fn editing_mode_inserts_chars() {
    let mut state = test_state();
    state.input_mode = InputMode::Editing;
    state.host_port.clear();
    handle_key(&mut state, key(KeyCode::Char('8')));
    handle_key(&mut state, key(KeyCode::Char('0')));
    assert_eq!(state.host_port.value, "80");
    handle_key(&mut state, key(KeyCode::Esc));
    assert_eq!(state.input_mode, InputMode::Normal);
}

#[tokio::test]
async fn enter_editing_with_e_key() {
    let mut state = test_state();
    state.focus = FocusPane::Profile;
    handle_key(&mut state, key(KeyCode::Enter));
    assert_eq!(state.input_mode, InputMode::Normal);
    handle_key(&mut state, key(KeyCode::Char('i')));
    assert_eq!(state.input_mode, InputMode::Editing);
}

#[test]
fn route_strength_mapping() {
    let mut state = test_state();
    assert_eq!(state.route_strength(), 0);
    assert_eq!(state.route_info(), "无");

    state.tunnel.state.phase = TunnelPhase::Active;
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
    state.tunnel.state.phase = TunnelPhase::Active;
    assert_eq!(state.gauge_label(), "等待连接...");
}

#[test]
fn status_label_phases() {
    let mut state = test_state();
    let (label, _) = state.status_label();
    assert_eq!(label, "空闲");

    state.tunnel.state.phase = TunnelPhase::Starting;
    let (label, _) = state.status_label();
    assert_eq!(label, "连接中...");

    state.tunnel.state.phase = TunnelPhase::Active;
    state.tunnel.state.mode = Some(TunnelMode::Host);
    let (label, _) = state.status_label();
    assert_eq!(label, "托管中");

    state.tunnel.state.mode = Some(TunnelMode::Join);
    let (label, _) = state.status_label();
    assert_eq!(label, "已加入");
}

#[test]
fn handle_tunnel_update_closed() {
    use sculk::tunnel::TunnelUpdate;

    let mut state = test_state();
    state.tunnel.state.phase = TunnelPhase::Stopping;
    state.tunnel.state.mode = Some(TunnelMode::Host);

    state.handle_tunnel_update(TunnelUpdate::Status(TunnelService::new().status()));

    assert_eq!(state.tunnel.state.phase, TunnelPhase::Idle);
    assert!(state.tunnel.state.mode.is_none());
    assert!(state.tunnel.state.ticket.is_none());
    assert!(state.tunnel.connections.is_empty());
}

#[test]
fn esc_action_label_changes_with_phase() {
    let mut state = test_state();

    state.tunnel.state.phase = TunnelPhase::Idle;
    assert_eq!(state.esc_action_label(), "退出");
    assert!(state.esc_can_exit());

    state.tunnel.state.phase = TunnelPhase::Starting;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());

    state.tunnel.state.phase = TunnelPhase::Active;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());

    state.tunnel.state.phase = TunnelPhase::Stopping;
    assert_eq!(state.esc_action_label(), "断开");
    assert!(!state.esc_can_exit());
}

#[test]
fn log_text_ellipsizes_unselected_row() {
    assert_eq!(render_log_text("0123456789", 5, false, 0), "01...");
    assert_eq!(render_log_text("tail", 5, false, 0), "tail");
}

#[test]
fn log_text_scrolls_selected_row() {
    assert_eq!(render_log_text("abcdefghi", 6, true, 0), "abcdef");
    assert_eq!(render_log_text("abcdefghi", 6, true, 1), "bcdefg");
}

#[test]
fn log_text_uses_display_width_for_cjk() {
    assert_eq!(
        render_log_text("正在启动 host", 11, false, 0),
        "正在启动..."
    );
    let text0 = render_log_text("正在启动 host", 11, true, 0);
    let text1 = render_log_text("正在启动 host", 11, true, 1);
    assert_ne!(text0, text1);
}
