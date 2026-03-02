//! 应用状态机、键盘处理与日志管理。

use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use sculk::tunnel::{IrohTunnel, TunnelEvent};
use tokio::sync::mpsc;

use crate::input::InputField;
use crate::tunnel::{self, AppEvent};

pub const LOG_CAP: usize = 200;
pub const TAB_TITLES: [&str; 3] = ["建房", "加入", "中继"];
pub const RELAYS: [&str; 2] = ["n0 默认中继", "自建中继"];

/// 当前激活的顶栏标签页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Host,
    Join,
    Relay,
}

/// 当前焦点所在的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Profile,
    Logs,
}

/// 输入模式：Normal 为导航，Editing 为文本输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// 事件循环单步结果。
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

/// Host 标签页中当前聚焦的输入字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostField {
    Port,
    Password,
}

/// Join 标签页中当前聚焦的输入字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinField {
    Ticket,
    Port,
    Password,
}

/// TUI 应用的全量状态，持有隧道句柄、输入字段与持久化 Profile。
pub struct AppState {
    pub show_help: bool,
    pub confirm_stop: bool,
    pub tick: u64,
    pub tab: ActiveTab,
    pub focus: FocusPane,
    pub input_mode: InputMode,
    pub quit_pressed_at: Option<Instant>,
    pub logs: Vec<String>,
    pub log_state: ListState,
    pub relay_state: ListState,
    pub relay_idx: usize,

    // 隧道状态
    pub phase: TunnelPhase,
    pub active_mode: Option<ActiveTab>,
    tunnel: Option<Arc<IrohTunnel>>,
    event_forwarder: Option<tokio::task::JoinHandle<()>>,
    startup_handle: Option<tokio::task::JoinHandle<()>>,
    pub ticket: Option<String>,
    pub app_tx: mpsc::UnboundedSender<AppEvent>,

    // 持久化配置
    profile: sculk::persist::Profile,

    // 连接快照
    pub connections: Vec<sculk::tunnel::ConnectionSnapshot>,

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
}

impl AppState {
    /// 从持久化 Profile 初始化状态，加载失败时回退到默认值。
    pub fn new(app_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let (profile, profile_err) = match sculk::persist::Profile::load() {
            Ok(p) => (p, None),
            Err(e) => (Default::default(), Some(format!("配置加载失败: {e}"))),
        };

        let relay_idx = if profile.relay.custom { 1 } else { 0 };
        let relay_url_value = profile.relay.url.clone().unwrap_or_default();

        let host_port_str = profile.host.port.to_string();
        let join_port_str = profile.join.port.to_string();
        let last_ticket = profile.join.last_ticket.clone().unwrap_or_default();

        let mut state = Self {
            show_help: false,
            confirm_stop: false,
            tick: 0,
            tab: ActiveTab::Host,
            focus: FocusPane::Profile,
            input_mode: InputMode::Normal,
            quit_pressed_at: None,
            logs: Vec::new(),
            log_state: ListState::default(),
            relay_state: ListState::default(),
            relay_idx,

            phase: TunnelPhase::Idle,
            active_mode: None,
            tunnel: None,
            event_forwarder: None,
            startup_handle: None,
            ticket: None,
            app_tx,

            profile,

            connections: Vec::new(),

            host_port: InputField::with_value("端口", &host_port_str),
            host_password: InputField::new("密码"),
            host_field: HostField::Port,

            join_ticket: InputField::with_value("票据", &last_ticket),
            join_port: InputField::with_value("端口", &join_port_str),
            join_password: InputField::new("密码"),
            join_field: JoinField::Ticket,

            relay_url: InputField::with_value("URL", &relay_url_value),
        };
        state.relay_state.select(Some(relay_idx));
        state.add_log("已就绪，按 Enter 执行当前模式");
        if let Some(err) = profile_err {
            state.add_log(&err);
        }
        state
    }

    /// 处理键盘事件，返回 [`Step::Exit`] 时退出事件循环。
    pub fn handle_key(&mut self, key: KeyEvent) -> Step {
        if self.input_mode == InputMode::Editing {
            self.quit_pressed_at = None;
            return self.handle_editing_key(key);
        }

        // 中止确认弹窗优先处理
        if self.confirm_stop {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_stop = false;
                    self.stop_tunnel();
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm_stop = false;
                }
                _ => {}
            }
            return Step::Continue;
        }

        if self.show_help {
            // 帮助模式下 h 或 Esc 关闭帮助
            if key.code == KeyCode::Char('h') || key.code == KeyCode::Esc {
                self.show_help = false;
            }
            return Step::Continue;
        }

        if key.code == KeyCode::Esc {
            // 启动中：直接中止，无需确认
            if self.phase == TunnelPhase::Starting {
                if let Some(handle) = self.startup_handle.take() {
                    handle.abort();
                }
                self.phase = TunnelPhase::Idle;
                self.active_mode = None;
                self.add_log("已取消启动");
                return Step::Continue;
            }

            // 隧道运行中：Esc 弹出中止确认，而非计入退出计数
            if self.phase == TunnelPhase::Active {
                self.confirm_stop = true;
                return Step::Continue;
            }

            let now = Instant::now();
            if let Some(prev) = self.quit_pressed_at
                && now.duration_since(prev).as_secs() < 3
            {
                return Step::Exit;
            }
            self.quit_pressed_at = Some(now);
            return Step::Continue;
        }
        self.quit_pressed_at = None;

        self.handle_normal_key(key)
    }

    fn handle_editing_key(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.persist_profile();
            }
            KeyCode::Up => {
                self.prev_field_clamped();
            }
            KeyCode::Down => {
                self.next_field_clamped();
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
            KeyCode::Char('h') => {
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
            KeyCode::Char('i') => {
                let can_edit = match self.tab {
                    ActiveTab::Relay => self.relay_state.selected() == Some(1),
                    _ => true,
                };
                if can_edit {
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

    /// 根据当前标签页执行主操作：启动/停止隧道或应用中继配置。
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

                let key_path = sculk::persist::default_key_path();
                let secret_key = match sculk::persist::load_or_generate_key(&key_path) {
                    Ok(k) => k,
                    Err(e) => {
                        self.add_log(&format!("密钥加载失败: {e}"));
                        return;
                    }
                };

                let custom_relay = if self.relay_idx == 1 {
                    Some(self.relay_url.value.as_str())
                } else {
                    None
                };
                let relay_url = match self.profile.resolve_relay_url(custom_relay) {
                    Ok(r) => r,
                    Err(e) => {
                        self.add_log(&format!("中继配置错误: {e}"));
                        return;
                    }
                };

                self.phase = TunnelPhase::Starting;
                self.active_mode = Some(ActiveTab::Host);
                self.add_log(&format!("正在启动 host 隧道 (端口 {port})..."));
                self.startup_handle = Some(tunnel::spawn_host(
                    port,
                    secret_key,
                    relay_url,
                    password,
                    self.app_tx.clone(),
                ));
            }
            TunnelPhase::Active if self.active_mode == Some(ActiveTab::Host) => {
                self.stop_tunnel();
            }
            _ => {
                self.add_log("隧道运行中，请先停止当前隧道");
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
                self.startup_handle = Some(tunnel::spawn_join(
                    &self.join_ticket.value,
                    port,
                    password,
                    self.app_tx.clone(),
                ));
            }
            TunnelPhase::Active if self.active_mode == Some(ActiveTab::Join) => {
                self.stop_tunnel();
            }
            _ => {
                self.add_log("隧道运行中，请先停止当前隧道");
            }
        }
    }

    fn apply_relay(&mut self) {
        if self.phase != TunnelPhase::Idle {
            self.add_log("隧道运行中，无法切换中继");
            return;
        }
        let selected = self.relay_state.selected().unwrap_or(self.relay_idx);

        match selected {
            0 => {
                if selected == self.relay_idx {
                    self.add_log(&format!("中继保持不变: {}", RELAYS[self.relay_idx]));
                    return;
                }
                self.profile.relay.custom = false;
                if let Err(e) = self.profile.save() {
                    self.add_log(&format!("重置中继失败: {e}"));
                    return;
                }
            }
            1 => {
                let url = self.relay_url.value.trim().to_string();
                if url.is_empty() {
                    self.add_log("请先输入自建中继 URL");
                    return;
                }
                if let Err(e) = self.profile.resolve_relay_url(Some(&url)) {
                    self.add_log(&format!("保存失败: {e}"));
                    return;
                }
                self.profile.relay.custom = true;
                self.profile.relay.url = Some(url);
                if let Err(e) = self.profile.save() {
                    self.add_log(&format!("保存失败: {e}"));
                    return;
                }
            }
            _ => {
                if selected == self.relay_idx {
                    self.add_log(&format!("中继保持不变: {}", RELAYS[self.relay_idx]));
                    return;
                }
            }
        }

        self.relay_idx = selected;
        self.add_log(&format!("中继已切换到 {}", RELAYS[self.relay_idx]));
    }

    fn stop_tunnel(&mut self) {
        if let Some(handle) = self.event_forwarder.take() {
            handle.abort();
        }
        if let Some(t) = self.tunnel.take() {
            self.phase = TunnelPhase::Stopping;
            self.add_log("正在关闭隧道...");
            tunnel::spawn_close(t, self.app_tx.clone());
        }
    }

    // ---- 隧道事件处理 ----

    /// 处理来自隧道任务的内部事件。
    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::HostStarted {
                tunnel,
                ticket,
                events,
            } => {
                self.startup_handle = None;
                self.phase = TunnelPhase::Active;
                self.tunnel = Some(tunnel);

                let quoted = format!("\"{ticket}\"");
                if sculk::clipboard::clipboard_copy(&quoted) {
                    self.add_log("票据已复制到剪贴板");
                }
                self.ticket = Some(ticket);

                self.add_log("host 隧道已启动");

                self.event_forwarder =
                    Some(tunnel::spawn_event_forwarder(events, self.app_tx.clone()));
            }
            AppEvent::JoinConnected { tunnel, events } => {
                self.startup_handle = None;
                self.phase = TunnelPhase::Active;
                self.tunnel = Some(tunnel);
                self.add_log("已成功连入隧道");

                // 持久化本次使用的票据
                self.profile.join.last_ticket = Some(self.join_ticket.value.clone());
                let _ = self.profile.save();

                self.event_forwarder =
                    Some(tunnel::spawn_event_forwarder(events, self.app_tx.clone()));
            }
            AppEvent::StartFailed(msg) => {
                self.startup_handle = None;
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
                self.event_forwarder = None;
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

    /// 定时刷新：递增 tick、清除超时退出提示、更新连接快照。
    pub fn on_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        // 3 秒超时自动清除 Esc 退出提示
        if let Some(prev) = self.quit_pressed_at
            && Instant::now().duration_since(prev).as_secs() >= 3
        {
            self.quit_pressed_at = None;
        }
        if self.phase == TunnelPhase::Active
            && let Some(ref tunnel) = self.tunnel
        {
            self.connections = tunnel.connections();
        }
    }

    // ---- 状态展示辅助 ----

    /// 返回当前状态标签文字与配色。
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

    /// 连接质量百分比：0ms ≈ 98%，≥500ms → 10%，无连接且隧道活跃时返回 50。
    pub fn route_strength(&self) -> u8 {
        if !self.connections.is_empty() {
            let avg_rtt: u64 = self.connections.iter().map(|c| c.rtt_ms).sum::<u64>()
                / self.connections.len() as u64;
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
            if conn.is_relay { "中继" } else { "直连" }
        } else {
            "无"
        }
    }

    pub fn gauge_label(&self) -> String {
        if self.connections.is_empty() {
            if self.phase == TunnelPhase::Active {
                "等待连接...".to_string()
            } else {
                "离线".to_string()
            }
        } else {
            let avg_rtt: u64 = self.connections.iter().map(|c| c.rtt_ms).sum::<u64>()
                / self.connections.len() as u64;
            let mode = self.route_info();
            format!(
                "{}% | {}ms | {} | {}人",
                self.route_strength(),
                avg_rtt,
                mode,
                self.connections.len()
            )
        }
    }

    pub fn connection_label(&self) -> String {
        if self.connections.is_empty() {
            "0".to_string()
        } else {
            format!("{}", self.connections.len())
        }
    }

    pub fn relay_label(&self) -> &str {
        RELAYS[self.relay_idx]
    }

    // ---- 输入字段 ----

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
                    HostField::Password => HostField::Password,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Port,
                    JoinField::Port => JoinField::Password,
                    JoinField::Password => JoinField::Password,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    fn prev_field(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.host_field = match self.host_field {
                    HostField::Port => HostField::Port,
                    HostField::Password => HostField::Port,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Ticket,
                    JoinField::Port => JoinField::Ticket,
                    JoinField::Password => JoinField::Port,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    /// 编辑模式下向后切换字段，到末尾停止。
    fn next_field_clamped(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.host_field = match self.host_field {
                    HostField::Port => HostField::Password,
                    HostField::Password => HostField::Password,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Port,
                    JoinField::Port => JoinField::Password,
                    JoinField::Password => JoinField::Password,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    /// 编辑模式下向前切换字段，到首位停止。
    fn prev_field_clamped(&mut self) {
        match self.tab {
            ActiveTab::Host => {
                self.host_field = match self.host_field {
                    HostField::Port => HostField::Port,
                    HostField::Password => HostField::Port,
                };
            }
            ActiveTab::Join => {
                self.join_field = match self.join_field {
                    JoinField::Ticket => JoinField::Ticket,
                    JoinField::Port => JoinField::Ticket,
                    JoinField::Password => JoinField::Port,
                };
            }
            ActiveTab::Relay => {}
        }
    }

    /// 编辑退出时同步 UI 字段到 Profile 并持久化。
    fn persist_profile(&mut self) {
        if let Ok(port) = self.host_port.value.parse::<u16>() {
            self.profile.host.port = port;
        }
        if let Ok(port) = self.join_port.value.parse::<u16>() {
            self.profile.join.port = port;
        }
        let relay_url = self.relay_url.value.trim().to_string();
        if relay_url.is_empty() {
            self.profile.relay.url = None;
        } else {
            self.profile.relay.url = Some(relay_url);
        }
        if let Err(e) = self.profile.save() {
            self.add_log(&format!("配置保存失败: {e}"));
        }
    }

    // ---- 日志与列表 ----

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.log_state.select(None);
        self.add_log("日志已清空");
    }

    /// 将日志追加到队列，超出 `LOG_CAP` 时丢弃最早的条目。
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
            Some(i) => i,
            None => 0,
        };
        self.relay_state.select(Some(next));
    }

    pub fn prev_relay_selection(&mut self) {
        let prev = match self.relay_state.selected() {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.relay_state.select(Some(prev));
    }
}

impl ActiveTab {
    /// 返回标签页对应的数组下标，与 `TAB_TITLES` 对齐。
    pub fn index(self) -> usize {
        match self {
            ActiveTab::Host => 0,
            ActiveTab::Join => 1,
            ActiveTab::Relay => 2,
        }
    }

    /// 向右切换，到末尾时停止。
    pub fn next(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Join,
            ActiveTab::Join => ActiveTab::Relay,
            ActiveTab::Relay => ActiveTab::Relay,
        }
    }

    /// 向左切换，到首位时停止。
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
        assert!(matches!(
            state.handle_key(key(KeyCode::Esc)),
            Step::Continue
        ));
        assert!(state.quit_pressed_at.is_some());
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));
        // 编辑模式下 Esc 无效
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
        // 无连接 + Idle → 0
        assert_eq!(state.route_strength(), 0);
        assert_eq!(state.route_info(), "无");

        // Active 无连接 → 50
        state.phase = super::TunnelPhase::Active;
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
        state.phase = super::TunnelPhase::Active;
        assert_eq!(state.gauge_label(), "等待连接...");
    }

    #[test]
    fn status_label_phases() {
        let mut state = test_state();
        let (label, _) = state.status_label();
        assert_eq!(label, "空闲");

        state.phase = super::TunnelPhase::Starting;
        let (label, _) = state.status_label();
        assert_eq!(label, "连接中...");

        state.phase = super::TunnelPhase::Active;
        state.active_mode = Some(ActiveTab::Host);
        let (label, _) = state.status_label();
        assert_eq!(label, "托管中");

        state.active_mode = Some(ActiveTab::Join);
        let (label, _) = state.status_label();
        assert_eq!(label, "已加入");
    }

    #[test]
    fn handle_app_event_closed() {
        use super::TunnelPhase;
        use crate::tunnel::AppEvent;

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
        use super::TunnelPhase;
        use crate::tunnel::AppEvent;

        let mut state = test_state();
        state.phase = TunnelPhase::Starting;
        state.active_mode = Some(ActiveTab::Host);

        state.handle_app_event(AppEvent::StartFailed("test error".into()));

        assert_eq!(state.phase, TunnelPhase::Idle);
        assert!(state.active_mode.is_none());
        assert!(state.logs.last().unwrap().contains("test error"));
    }
}
