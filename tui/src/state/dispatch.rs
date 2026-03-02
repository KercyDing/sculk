//! 键盘事件分发与输入字段导航。

use crossterm::event::{KeyCode, KeyEvent};

use crate::services::persist;
use crate::state::machine::{self, UiOverlayState};
use crate::state::{ActiveTab, AppState, FocusPane, HostField, InputMode, JoinField, Step};

/// 键盘入口分发。
pub(crate) fn handle_key(state: &mut AppState, key: KeyEvent) -> Step {
    match machine::overlay_state(state) {
        UiOverlayState::Editing => {
            state.quit_pressed_at = None;
            return handle_editing_key(state, key);
        }
        UiOverlayState::ConfirmStop => {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    state.confirm_stop = false;
                    crate::state::actions::stop_tunnel(state);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    state.confirm_stop = false;
                }
                _ => {}
            }
            return Step::Continue;
        }
        UiOverlayState::Help => {
            if key.code == KeyCode::Char('h') || key.code == KeyCode::Esc {
                state.show_help = false;
            }
            return Step::Continue;
        }
        UiOverlayState::Normal => {}
    }

    if key.code == KeyCode::Esc {
        return machine::handle_lifecycle_esc(state);
    }

    state.quit_pressed_at = None;
    handle_normal_key(state, key)
}

fn handle_editing_key(state: &mut AppState, key: KeyEvent) -> Step {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            persist_profile_from_inputs(state);
        }
        KeyCode::Up => {
            prev_field_clamped(state);
        }
        KeyCode::Down => {
            next_field_clamped(state);
        }
        KeyCode::Backspace => {
            active_input_mut(state).backspace();
        }
        KeyCode::Delete => {
            active_input_mut(state).delete();
        }
        KeyCode::Left => {
            active_input_mut(state).move_left();
        }
        KeyCode::Right => {
            active_input_mut(state).move_right();
        }
        KeyCode::Home => {
            active_input_mut(state).move_home();
        }
        KeyCode::End => {
            active_input_mut(state).move_end();
        }
        KeyCode::Char(c) => {
            active_input_mut(state).insert(c);
        }
        _ => {}
    }
    Step::Continue
}

fn handle_normal_key(state: &mut AppState, key: KeyEvent) -> Step {
    match key.code {
        KeyCode::Char('h') => {
            state.show_help = !state.show_help;
            Step::Continue
        }
        KeyCode::Tab => {
            state.focus = if state.focus == FocusPane::Profile {
                FocusPane::Logs
            } else {
                FocusPane::Profile
            };
            Step::Continue
        }
        KeyCode::Left => {
            state.tab = state.tab.prev();
            Step::Continue
        }
        KeyCode::Right => {
            state.tab = state.tab.next();
            Step::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if state.tab == ActiveTab::Relay && state.focus == FocusPane::Profile {
                state.prev_relay_selection();
            } else if state.focus == FocusPane::Profile {
                prev_field(state);
            } else {
                state.prev_log();
            }
            Step::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.tab == ActiveTab::Relay && state.focus == FocusPane::Profile {
                state.next_relay_selection();
            } else if state.focus == FocusPane::Profile {
                next_field(state);
            } else {
                state.next_log();
            }
            Step::Continue
        }
        KeyCode::Enter => {
            state.primary_action();
            Step::Continue
        }
        KeyCode::Char('i') => {
            let can_edit = match state.tab {
                ActiveTab::Relay => state.relay_state.selected() == Some(1),
                _ => true,
            };
            if can_edit {
                state.input_mode = InputMode::Editing;
            }
            Step::Continue
        }
        KeyCode::Char('c') => {
            state.clear_logs();
            Step::Continue
        }
        _ => Step::Continue,
    }
}

fn active_input_mut(state: &mut AppState) -> &mut super::input_field::InputField {
    match state.tab {
        ActiveTab::Host => match state.host_field {
            HostField::Port => &mut state.host_port,
            HostField::Password => &mut state.host_password,
        },
        ActiveTab::Join => match state.join_field {
            JoinField::Ticket => &mut state.join_ticket,
            JoinField::Port => &mut state.join_port,
            JoinField::Password => &mut state.join_password,
        },
        ActiveTab::Relay => &mut state.relay_url,
    }
}

fn next_field(state: &mut AppState) {
    match state.tab {
        ActiveTab::Host => {
            state.host_field = match state.host_field {
                HostField::Port => HostField::Password,
                HostField::Password => HostField::Password,
            };
        }
        ActiveTab::Join => {
            state.join_field = match state.join_field {
                JoinField::Ticket => JoinField::Port,
                JoinField::Port => JoinField::Password,
                JoinField::Password => JoinField::Password,
            };
        }
        ActiveTab::Relay => {}
    }
}

fn prev_field(state: &mut AppState) {
    match state.tab {
        ActiveTab::Host => {
            state.host_field = match state.host_field {
                HostField::Port => HostField::Port,
                HostField::Password => HostField::Port,
            };
        }
        ActiveTab::Join => {
            state.join_field = match state.join_field {
                JoinField::Ticket => JoinField::Ticket,
                JoinField::Port => JoinField::Ticket,
                JoinField::Password => JoinField::Port,
            };
        }
        ActiveTab::Relay => {}
    }
}

fn next_field_clamped(state: &mut AppState) {
    next_field(state);
}

fn prev_field_clamped(state: &mut AppState) {
    prev_field(state);
}

fn persist_profile_from_inputs(state: &mut AppState) {
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
    if let Err(e) = persist::save_profile(&state.ctx.profile) {
        state.add_log(&format!("配置保存失败: {e}"));
    }
}
