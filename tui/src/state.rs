//! 应用状态机、键盘处理与日志管理。

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::ListState;

use crate::input::InputField;
use crate::ui::theme::{ACCENT, INFO};

pub const LOG_CAP: usize = 200;
pub const TAB_TITLES: [&str; 3] = ["建房", "加入", "中继"];
pub const RELAYS: [&str; 3] = ["n0 默认中继", "亚洲中继池", "自建中继"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Host,
    Join,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Profile,
    Logs,
}

/// Normal 模式下快捷键生效；Editing 模式下字符送入输入字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub enum Step {
    Continue,
    Exit,
}

/// Host tab 的可编辑字段索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostField {
    Port,
    Password,
}

/// Join tab 的可编辑字段索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinField {
    Ticket,
    Port,
    Password,
}

/// Relay tab 的可编辑字段索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayField {
    Url,
}

pub struct AppState {
    pub show_help: bool,
    pub tick: u64,
    pub tab: ActiveTab,
    pub focus: FocusPane,
    pub input_mode: InputMode,
    pub quit_pending: bool,
    pub logs: Vec<String>,
    pub log_state: ListState,
    pub relay_state: ListState,
    pub hosting: bool,
    pub joined: bool,
    pub relay_idx: usize,
    pub route_idx: usize,

    // Host tab 输入字段
    pub host_port: InputField,
    pub host_password: InputField,
    pub host_field: HostField,

    // Join tab 输入字段
    pub join_ticket: InputField,
    pub join_port: InputField,
    pub join_password: InputField,
    pub join_field: JoinField,

    // Relay tab 输入字段
    pub relay_url: InputField,
    pub relay_field: RelayField,
}

impl Default for AppState {
    fn default() -> Self {
        let mut state = Self {
            show_help: false,
            tick: 0,
            tab: ActiveTab::Host,
            focus: FocusPane::Profile,
            input_mode: InputMode::Normal,
            quit_pending: false,
            logs: Vec::new(),
            log_state: ListState::default(),
            relay_state: ListState::default(),
            hosting: false,
            joined: false,
            relay_idx: 0,
            route_idx: 0,

            host_port: InputField::with_value("端口", "25565"),
            host_password: InputField::new("密码"),
            host_field: HostField::Port,

            join_ticket: InputField::new("票据"),
            join_port: InputField::with_value("端口", "30000"),
            join_password: InputField::new("密码"),
            join_field: JoinField::Ticket,

            relay_url: InputField::new("URL"),
            relay_field: RelayField::Url,
        };
        state.relay_state.select(Some(0));
        state.add_log("已就绪，按 Enter 执行当前模式");
        state
    }
}

impl AppState {
    /// 获取当前活跃的输入字段。
    pub fn active_input(&self) -> &InputField {
        match self.tab {
            ActiveTab::Host => match self.host_field {
                HostField::Port => &self.host_port,
                HostField::Password => &self.host_password,
            },
            ActiveTab::Join => match self.join_field {
                JoinField::Ticket => &self.join_ticket,
                JoinField::Port => &self.join_port,
                JoinField::Password => &self.join_password,
            },
            ActiveTab::Relay => &self.relay_url,
        }
    }

    fn active_input_mut(&mut self) -> &mut InputField {
        match self.tab {
            ActiveTab::Host => match self.host_field {
                HostField::Port => &mut self.host_port,
                HostField::Password => &mut self.host_password,
            },
            ActiveTab::Join => match self.join_field {
                JoinField::Ticket => &mut self.join_ticket,
                JoinField::Port => &mut self.join_port,
                JoinField::Password => &mut self.join_password,
            },
            ActiveTab::Relay => &mut self.relay_url,
        }
    }

    /// 切换到下一个输入字段。
    fn next_field(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.host_field = match self.host_field {
                    HostField::Port => HostField::Password,
                    HostField::Password => HostField::Port,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Port,
                    JoinField::Port => JoinField::Password,
                    JoinField::Password => JoinField::Ticket,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    /// 切换到上一个输入字段。
    fn prev_field(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.host_field = match self.host_field {
                    HostField::Port => HostField::Password,
                    HostField::Password => HostField::Port,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Password,
                    JoinField::Port => JoinField::Ticket,
                    JoinField::Password => JoinField::Port,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    /// 处理单个键盘事件并返回循环控制信号。
    pub fn handle_key(&mut self, key: KeyEvent) -> Step {
        // 编辑模式下 Esc 无效，只有 q 退出编辑
        if self.input_mode == InputMode::Editing {
            self.quit_pending = false;
            return self.handle_editing_key(key);
        }

        // Normal 模式：Esc 双击退出
        if key.code == KeyCode::Esc {
            if self.quit_pending {
                return Step::Exit;
            } else {
                self.quit_pending = true;
                return Step::Continue;
            }
        }
        self.quit_pending = false;

        self.handle_normal_key(key)
    }

    fn handle_editing_key(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Char('q') => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Tab => {
                self.next_field();
            }
            KeyCode::BackTab => {
                self.prev_field();
            }
            KeyCode::Backspace => {
                self.active_input_mut().backspace();
            }
            KeyCode::Delete => {
                self.active_input_mut().delete();
            }
            KeyCode::Left => {
                self.active_input_mut().move_left();
            }
            KeyCode::Right => {
                self.active_input_mut().move_right();
            }
            KeyCode::Home => {
                self.active_input_mut().move_home();
            }
            KeyCode::End => {
                self.active_input_mut().move_end();
            }
            KeyCode::Char(c) => {
                self.active_input_mut().insert(c);
            }
            _ => {}
        }
        Step::Continue
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Char('h') | KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                Step::Continue
            }
            KeyCode::Tab => {
                self.focus = if self.focus == FocusPane::Profile {
                    FocusPane::Logs
                } else {
                    FocusPane::Profile
                };
                Step::Continue
            }
            KeyCode::Left => {
                self.tab = self.tab.prev();
                Step::Continue
            }
            KeyCode::Right => {
                self.tab = self.tab.next();
                Step::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.tab == ActiveTab::Relay && self.focus == FocusPane::Profile {
                    self.prev_relay_selection();
                } else if self.focus == FocusPane::Profile {
                    self.prev_field();
                } else {
                    self.prev_log();
                }
                Step::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.tab == ActiveTab::Relay && self.focus == FocusPane::Profile {
                    self.next_relay_selection();
                } else if self.focus == FocusPane::Profile {
                    self.next_field();
                } else {
                    self.next_log();
                }
                Step::Continue
            }
            KeyCode::Enter => {
                self.primary_action();
                Step::Continue
            }
            KeyCode::Char('e') => {
                if self.focus == FocusPane::Profile {
                    self.input_mode = InputMode::Editing;
                }
                Step::Continue
            }
            KeyCode::Char('r') => {
                self.rotate_route();
                Step::Continue
            }
            KeyCode::Char('c') => {
                self.clear_logs();
                Step::Continue
            }
            _ => Step::Continue,
        }
    }

    /// 每个 tick 更新运行态并生成心跳日志。
    pub fn on_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        if self.tick.is_multiple_of(25) && (self.hosting || self.joined) {
            self.add_log("心跳正常，链路稳定");
        }
    }

    pub fn primary_action(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.hosting = !self.hosting;
                if self.hosting {
                    self.joined = false;
                    self.add_log("房主隧道已启动，端口 :25565");
                } else {
                    self.add_log("房主隧道已停止");
                }
            }
            ActiveTab::Join => {
                self.joined = !self.joined;
                if self.joined {
                    self.hosting = false;
                    self.add_log("已通过票据 SCULK-XXXX 加入房间");
                } else {
                    self.add_log("已离开房间会话");
                }
            }
            ActiveTab::Relay => {
                let selected = self.relay_state.selected().unwrap_or(self.relay_idx);
                if selected != self.relay_idx {
                    self.relay_idx = selected;
                    self.add_log(&format!("中继已切换到 {}", RELAYS[self.relay_idx]));
                } else {
                    self.add_log(&format!("中继保持不变: {}", RELAYS[self.relay_idx]));
                }
            }
        }
    }

    pub fn rotate_route(&mut self) {
        self.route_idx = (self.route_idx + 1) % 3;
        self.add_log(&format!("路由已切换到方案-{}", self.route_idx + 1));
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.log_state.select(None);
        self.add_log("日志已清空");
    }

    pub fn add_log(&mut self, msg: &str) {
        self.logs.push(msg.to_string());
        if self.logs.len() > LOG_CAP {
            let to_drop = self.logs.len() - LOG_CAP;
            self.logs.drain(0..to_drop);
        }
        self.log_state
            .select(Some(self.logs.len().saturating_sub(1)));
    }

    pub fn next_log(&mut self) {
        if self.logs.is_empty() {
            self.log_state.select(None);
            return;
        }
        let next = match self.log_state.selected() {
            Some(i) if i + 1 < self.logs.len() => i + 1,
            Some(i) => i,
            None => 0,
        };
        self.log_state.select(Some(next));
    }

    pub fn prev_log(&mut self) {
        if self.logs.is_empty() {
            self.log_state.select(None);
            return;
        }
        let prev = match self.log_state.selected() {
            Some(0) => 0,
            Some(i) => i - 1,
            None => 0,
        };
        self.log_state.select(Some(prev));
    }

    pub fn next_relay_selection(&mut self) {
        let next = match self.relay_state.selected() {
            Some(i) if i + 1 < RELAYS.len() => i + 1,
            _ => 0,
        };
        self.relay_state.select(Some(next));
    }

    pub fn prev_relay_selection(&mut self) {
        let prev = match self.relay_state.selected() {
            Some(0) | None => RELAYS.len() - 1,
            Some(i) => i - 1,
        };
        self.relay_state.select(Some(prev));
    }

    pub fn mode_profile(&self) -> Text<'_> {
        match self.tab {
            ActiveTab::Host => Text::from(vec![
                Line::from(vec![
                    Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "建房",
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::raw(""),
                Line::raw("监听端口       : 25565"),
                Line::raw("公开票据       : 自动生成"),
                Line::raw("最大玩家数     : 8"),
                Line::raw(""),
                Line::raw("Enter / Space 启动或停止房主隧道。"),
            ]),
            ActiveTab::Join => Text::from(vec![
                Line::from(vec![
                    Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "加入",
                        Style::default().fg(INFO).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::raw(""),
                Line::raw("本地端口       : 25566"),
                Line::raw("票据来源       : 剪贴板"),
                Line::raw("重试策略       : 指数退避"),
                Line::raw(""),
                Line::raw("Enter / Space 连接或断开会话。"),
            ]),
            ActiveTab::Relay => Text::from(vec![
                Line::from(vec![
                    Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "中继",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::raw(""),
                Line::raw(format!("当前中继       : {}", RELAYS[self.relay_idx])),
                Line::raw("回退策略       : 已启用"),
                Line::raw("健康检查       : 5秒"),
                Line::raw(""),
                Line::raw("Enter / Space 轮换中继节点。"),
            ]),
        }
    }

    pub fn route_strength(&self) -> u8 {
        let base = match self.route_idx {
            0 => 84_i16,
            1 => 66_i16,
            _ => 74_i16,
        };
        let pulse = ((self.tick % 9) as i16 - 4) * 2;
        let value = (base + pulse).clamp(35, 98);
        value as u8
    }

    pub fn relay_label(&self) -> &str {
        RELAYS[self.relay_idx]
    }
}

impl ActiveTab {
    pub fn index(self) -> usize {
        match self {
            ActiveTab::Host => 0,
            ActiveTab::Join => 1,
            ActiveTab::Relay => 2,
        }
    }

    pub fn next(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Join,
            ActiveTab::Join => ActiveTab::Relay,
            ActiveTab::Relay => ActiveTab::Relay,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Host,
            ActiveTab::Join => ActiveTab::Host,
            ActiveTab::Relay => ActiveTab::Join,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{ActiveTab, AppState, FocusPane, InputMode, RELAYS, Step};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn quit_keys_exit() {
        // 双击 Esc 退出（全局）
        let mut state = AppState::default();
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
        assert!(state.quit_pending);
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));
        // 编辑模式下双击 Esc 不退出
        let mut state = AppState::default();
        state.input_mode = InputMode::Editing;
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
    }

    #[test]
    fn switch_tab_and_toggle_help() {
        let mut state = AppState::default();
        assert_eq!(state.tab, ActiveTab::Host);
        assert!(matches!(
            state.handle_key(key(KeyCode::Right)),
            Step::Continue
        ));
        assert_eq!(state.tab, ActiveTab::Join);
        assert!(matches!(
            state.handle_key(key(KeyCode::Left)),
            Step::Continue
        ));
        assert_eq!(state.tab, ActiveTab::Host);
        assert!(!state.show_help);
        assert!(matches!(
            state.handle_key(key(KeyCode::Char('h'))),
            Step::Continue
        ));
        assert!(state.show_help);
    }

    #[test]
    fn tab_selection_clamps_at_edges() {
        let mut state = AppState::default();
        assert_eq!(state.tab, ActiveTab::Host);
        assert!(matches!(
            state.handle_key(key(KeyCode::Left)),
            Step::Continue
        ));
        assert_eq!(state.tab, ActiveTab::Host);

        state.tab = ActiveTab::Relay;
        assert!(matches!(
            state.handle_key(key(KeyCode::Right)),
            Step::Continue
        ));
        assert_eq!(state.tab, ActiveTab::Relay);
    }

    #[test]
    fn primary_action_changes_session_state() {
        let mut state = AppState::default();
        state.primary_action();
        assert!(state.hosting);
        state.tab = ActiveTab::Join;
        state.primary_action();
        assert!(state.joined);
        assert!(!state.hosting);
        state.tab = ActiveTab::Relay;
        let old = state.relay_idx;
        state.relay_state.select(Some((old + 1) % RELAYS.len()));
        state.primary_action();
        assert_ne!(old, state.relay_idx);
    }

    #[test]
    fn log_selection_clamps_at_edges() {
        let mut state = AppState::default();
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
        let mut state = AppState::default();
        state.tab = ActiveTab::Relay;
        state.focus = FocusPane::Profile;
        state.relay_state.select(Some(0));

        assert!(matches!(
            state.handle_key(key(KeyCode::Down)),
            Step::Continue
        ));
        assert_eq!(state.relay_state.selected(), Some(1));

        assert!(matches!(state.handle_key(key(KeyCode::Up)), Step::Continue));
        assert_eq!(state.relay_state.selected(), Some(0));
    }

    #[test]
    fn editing_mode_inserts_chars() {
        let mut state = AppState::default();
        state.input_mode = InputMode::Editing;
        state.host_port.clear();
        state.handle_key(key(KeyCode::Char('8')));
        state.handle_key(key(KeyCode::Char('0')));
        assert_eq!(state.host_port.value, "80");
        state.handle_key(key(KeyCode::Char('q')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn enter_editing_with_e_key() {
        let mut state = AppState::default();
        state.focus = FocusPane::Profile;
        // Enter 执行操作，不进编辑
        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        // e 进编辑
        state.handle_key(key(KeyCode::Char('e')));
        assert_eq!(state.input_mode, InputMode::Editing);
    }
}
