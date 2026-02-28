//! 应用状态机、键盘处理与日志管理。

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use sculk_core::tunnel::{IrohTunnel, TunnelEvent};
use tokio::sync::mpsc;

use crate::clipboard;
use crate::config;
use crate::input::InputField;
use crate::tunnel::{self, AppEvent};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub enum Step {
    Continue,
    Exit,
}

/// 隧道生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelPhase {
    Idle,
    Starting,
    Active,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostField {
    Port,
    Password,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinField {
    Ticket,
    Port,
    Password,
}

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
    pub relay_idx: usize,

    // 隧道状态
    pub phase: TunnelPhase,
    pub active_mode: Option<ActiveTab>,
    tunnel: Option<Arc<IrohTunnel>>,
    pub ticket: Option<String>,
    pub app_tx: mpsc::UnboundedSender<AppEvent>,

    // 连接快照（Active 时 on_tick 刷新）
    pub connections: Vec<sculk_core::tunnel::ConnectionSnapshot>,

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

impl AppState {
    pub fn new(app_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
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
            relay_idx: 0,

            phase: TunnelPhase::Idle,
            active_mode: None,
            tunnel: None,
            ticket: None,
            app_tx,

            connections: Vec::new(),

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

    pub fn is_active(&self) -> bool {
        self.phase == TunnelPhase::Active
    }

    // ---- 键盘处理 ----

    pub fn handle_key(&mut self, key: KeyEvent) -> Step {
        if self.input_mode == InputMode::Editing {
            self.quit_pending = false;
            return self.handle_editing_key(key);
        }

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
            KeyCode::Char('c') => {
                self.clear_logs();
                Step::Continue
            }
            _ => Step::Continue,
        }
    }

    // ---- 主要操作 ----

    pub fn primary_action(&mut self) {
        match self.tab {
            ActiveTab::Host => self.toggle_host(),
            ActiveTab::Join => self.toggle_join(),
            ActiveTab::Relay => self.apply_relay(),
        }
    }

    fn toggle_host(&mut self) {
        match self.phase {
            TunnelPhase::Idle => {
                let port: u16 = match self.host_port.value.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        self.add_log("端口格式错误");
                        return;
                    }
                };
                let password = if self.host_password.value.is_empty() {
                    None
                } else {
                    Some(self.host_password.value.clone())
                };

                let key_path = config::default_key_path();
                let secret_key = match config::load_or_generate_key(&key_path) {
                    Ok(k) => k,
                    Err(e) => {
                        self.add_log(&format!("密钥加载失败: {e}"));
                        return;
                    }
                };

                let relay_url = match config::resolve_relay_url(None) {
                    Ok(r) => r,
                    Err(e) => {
                        self.add_log(&format!("中继配置错误: {e}"));
                        return;
                    }
                };

                self.phase = TunnelPhase::Starting;
                self.active_mode = Some(ActiveTab::Host);
                self.add_log(&format!("正在启动 host 隧道 (端口 {port})..."));
                tunnel::spawn_host(port, secret_key, relay_url, password, self.app_tx.clone());
            }
            TunnelPhase::Active if self.active_mode == Some(ActiveTab::Host) => {
                self.stop_tunnel();
            }
            _ => {
                self.add_log("当前状态无法执行此操作");
            }
        }
    }

    fn toggle_join(&mut self) {
        match self.phase {
            TunnelPhase::Idle => {
                if self.join_ticket.value.is_empty() {
                    self.add_log("请先输入票据");
                    return;
                }
                let port: u16 = match self.join_port.value.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        self.add_log("端口格式错误");
                        return;
                    }
                };
                let password = if self.join_password.value.is_empty() {
                    None
                } else {
                    Some(self.join_password.value.clone())
                };

                self.phase = TunnelPhase::Starting;
                self.active_mode = Some(ActiveTab::Join);
                self.add_log("正在连接...");
                tunnel::spawn_join(
                    &self.join_ticket.value,
                    port,
                    password,
                    self.app_tx.clone(),
                );
            }
            TunnelPhase::Active if self.active_mode == Some(ActiveTab::Join) => {
                self.stop_tunnel();
            }
            _ => {
                self.add_log("当前状态无法执行此操作");
            }
        }
    }

    fn apply_relay(&mut self) {
        let selected = self.relay_state.selected().unwrap_or(self.relay_idx);
        if selected != self.relay_idx {
            self.relay_idx = selected;
            self.add_log(&format!("中继已切换到 {}", RELAYS[self.relay_idx]));
        } else {
            self.add_log(&format!("中继保持不变: {}", RELAYS[self.relay_idx]));
        }
    }

    fn stop_tunnel(&mut self) {
        if let Some(t) = self.tunnel.take() {
            self.phase = TunnelPhase::Stopping;
            self.add_log("正在关闭隧道...");
            tunnel::spawn_close(t, self.app_tx.clone());
        }
    }

    // ---- 隧道事件处理 ----

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::HostStarted {
                tunnel,
                ticket,
                events,
            } => {
                self.phase = TunnelPhase::Active;
                self.tunnel = Some(tunnel);

                let quoted = format!("\"{ticket}\"");
                if clipboard::clipboard_copy(&quoted) {
                    self.add_log("票据已复制到剪贴板");
                }
                self.ticket = Some(ticket);
                self.add_log("host 隧道已启动");

                tunnel::spawn_event_forwarder(events, self.app_tx.clone());
            }
            AppEvent::JoinConnected { tunnel, events } => {
                self.phase = TunnelPhase::Active;
                self.tunnel = Some(tunnel);
                self.add_log("已成功连入隧道");

                tunnel::spawn_event_forwarder(events, self.app_tx.clone());
            }
            AppEvent::StartFailed(msg) => {
                self.phase = TunnelPhase::Idle;
                self.active_mode = None;
                self.add_log(&msg);
            }
            AppEvent::Closed => {
                self.phase = TunnelPhase::Idle;
                self.active_mode = None;
                self.tunnel = None;
                self.ticket = None;
                self.connections.clear();
                self.add_log("隧道已关闭");
            }
            AppEvent::Tunnel(te) => self.handle_tunnel_event(te),
        }
    }

    fn handle_tunnel_event(&mut self, event: TunnelEvent) {
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
            TunnelEvent::PlayerRejected { id, reason } => {
                format!("玩家被拒: {id} ({reason})")
            }
            TunnelEvent::Error { message } => format!("错误: {message}"),
        };
        self.add_log(&msg);
    }

    // ---- tick ----

    pub fn on_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        if self.phase == TunnelPhase::Active {
            if let Some(ref tunnel) = self.tunnel {
                self.connections = tunnel.connections();
            }
        }
    }

    // ---- 状态展示辅助 ----

    pub fn status_label(&self) -> (&str, crate::ui::theme::StatusColor) {
        use crate::ui::theme::StatusColor;
        match self.phase {
            TunnelPhase::Idle => ("空闲", StatusColor::Warn),
            TunnelPhase::Starting => ("连接中...", StatusColor::Info),
            TunnelPhase::Active => match self.active_mode {
                Some(ActiveTab::Host) => ("托管中", StatusColor::Accent),
                Some(ActiveTab::Join) => ("已加入", StatusColor::Info),
                _ => ("活跃", StatusColor::Accent),
            },
            TunnelPhase::Stopping => ("关闭中...", StatusColor::Warn),
        }
    }

    pub fn route_strength(&self) -> u8 {
        if !self.connections.is_empty() {
            let avg_rtt: u64 =
                self.connections.iter().map(|c| c.rtt_ms).sum::<u64>() / self.connections.len() as u64;
            // RTT -> 质量百分比：0ms=98%, 500ms+=10%
            ((100_u64.saturating_sub(avg_rtt / 5)).clamp(10, 98)) as u8
        } else if self.phase == TunnelPhase::Active {
            50
        } else {
            0
        }
    }

    pub fn route_info(&self) -> &str {
        if let Some(conn) = self.connections.first() {
            if conn.is_relay {
                "中继"
            } else {
                "直连"
            }
        } else {
            "无"
        }
    }

    pub fn relay_label(&self) -> &str {
        RELAYS[self.relay_idx]
    }

    // ---- 输入字段 ----

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

    // ---- 日志与列表 ----

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
    use tokio::sync::mpsc;

    use super::{ActiveTab, AppState, FocusPane, InputMode, RELAYS, Step};

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
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
        assert!(state.quit_pending);
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));
        // 编辑模式下 Esc 无效
        let mut state = test_state();
        state.input_mode = InputMode::Editing;
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Continue));
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
        state.handle_key(key(KeyCode::Char('q')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[tokio::test]
    async fn enter_editing_with_e_key() {
        let mut state = test_state();
        state.focus = FocusPane::Profile;
        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        state.handle_key(key(KeyCode::Char('e')));
        assert_eq!(state.input_mode, InputMode::Editing);
    }
}
