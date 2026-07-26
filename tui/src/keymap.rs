//! 将键盘输入映射为界面意图。

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};

use crate::input::InputField;
use crate::model::{
    ActiveTab, FocusPane, HostField, InputMode, Intent, JoinField, Model, TunnelPhase,
};

pub fn handle_key(model: &mut Model, key: KeyEvent) -> Intent {
    if model.confirm_stop {
        return handle_confirmation(model, key);
    }
    if model.show_help {
        if matches!(key.code, KeyCode::Char('h') | KeyCode::Esc) {
            model.show_help = false;
        }
        return Intent::None;
    }
    if model.input_mode == InputMode::Editing {
        model.quit_pressed_at = None;
        return handle_editing(model, key);
    }
    if key.code == KeyCode::Esc {
        return handle_escape(model);
    }

    model.quit_pressed_at = None;
    handle_normal(model, key)
}

fn handle_confirmation(model: &mut Model, key: KeyEvent) -> Intent {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            model.confirm_stop = false;
            Intent::Stop
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            model.confirm_stop = false;
            Intent::None
        }
        _ => Intent::None,
    }
}

fn handle_editing(model: &mut Model, key: KeyEvent) -> Intent {
    match key.code {
        KeyCode::Esc => {
            model.input_mode = InputMode::Normal;
            return Intent::SaveInputs;
        }
        KeyCode::Up => previous_field(model),
        KeyCode::Down => next_field(model),
        KeyCode::Backspace => active_input(model).backspace(),
        KeyCode::Delete => active_input(model).delete(),
        KeyCode::Left => active_input(model).move_left(),
        KeyCode::Right => active_input(model).move_right(),
        KeyCode::Home => active_input(model).move_home(),
        KeyCode::End => active_input(model).move_end(),
        KeyCode::Char(ch) => active_input(model).insert(ch),
        _ => {}
    }
    Intent::None
}

fn handle_normal(model: &mut Model, key: KeyEvent) -> Intent {
    match key.code {
        KeyCode::Char('h') => model.show_help = true,
        KeyCode::Tab => {
            model.focus = match model.focus {
                FocusPane::Profile => FocusPane::Logs,
                FocusPane::Logs => FocusPane::Profile,
            };
        }
        KeyCode::Left => model.tab = model.tab.prev(),
        KeyCode::Right => model.tab = model.tab.next(),
        KeyCode::Up | KeyCode::Char('k') => {
            if model.tab == ActiveTab::Relay && model.focus == FocusPane::Profile {
                model.prev_relay();
            } else if model.focus == FocusPane::Profile {
                previous_field(model);
            } else {
                model.prev_log();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if model.tab == ActiveTab::Relay && model.focus == FocusPane::Profile {
                model.next_relay();
            } else if model.focus == FocusPane::Profile {
                next_field(model);
            } else {
                model.next_log();
            }
        }
        KeyCode::Enter => return Intent::Primary,
        KeyCode::Char('i') => {
            let editable = model.tab != ActiveTab::Relay || model.relay_state.selected() == Some(1);
            if editable {
                model.input_mode = InputMode::Editing;
            }
        }
        KeyCode::Char('c') => model.clear_logs(),
        _ => {}
    }
    Intent::None
}

fn handle_escape(model: &mut Model) -> Intent {
    match model.tunnel.state.phase {
        TunnelPhase::Starting => {
            model.add_log("正在取消启动...");
            Intent::Stop
        }
        TunnelPhase::Active => {
            model.confirm_stop = true;
            Intent::None
        }
        TunnelPhase::Stopping => Intent::None,
        TunnelPhase::Idle => {
            let now = Instant::now();
            if model
                .quit_pressed_at
                .is_some_and(|pressed_at| now.duration_since(pressed_at).as_secs() < 1)
            {
                return Intent::Exit;
            }
            model.quit_pressed_at = Some(now);
            Intent::None
        }
    }
}

fn active_input(model: &mut Model) -> &mut InputField {
    match model.tab {
        ActiveTab::Host => match model.host_field {
            HostField::Port => &mut model.host_port,
            HostField::Password => &mut model.host_password,
        },
        ActiveTab::Join => match model.join_field {
            JoinField::Ticket => &mut model.join_ticket,
            JoinField::Port => &mut model.join_port,
            JoinField::Password => &mut model.join_password,
        },
        ActiveTab::Relay => &mut model.relay_url,
    }
}

fn next_field(model: &mut Model) {
    match model.tab {
        ActiveTab::Host => model.host_field = HostField::Password,
        ActiveTab::Join => {
            model.join_field = match model.join_field {
                JoinField::Ticket => JoinField::Port,
                JoinField::Port | JoinField::Password => JoinField::Password,
            };
        }
        ActiveTab::Relay => {}
    }
}

fn previous_field(model: &mut Model) {
    match model.tab {
        ActiveTab::Host => model.host_field = HostField::Port,
        ActiveTab::Join => {
            model.join_field = match model.join_field {
                JoinField::Ticket | JoinField::Port => JoinField::Ticket,
                JoinField::Password => JoinField::Port,
            };
        }
        ActiveTab::Relay => {}
    }
}
