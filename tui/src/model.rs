//! 纯界面状态及其派生显示值。

use std::time::Instant;

use ratatui::widgets::ListState;
use sculk::tunnel::{TunnelEvent, TunnelMode, TunnelStatus, TunnelUpdate};

use crate::input::InputField;

pub use sculk::tunnel::TunnelPhase;

pub const LOG_CAP: usize = 200;
pub const TAB_TITLES: [&str; 3] = ["建房", "加入", "中继"];
pub const RELAYS: [&str; 2] = ["n0 默认中继", "自建中继"];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    None,
    Primary,
    Stop,
    SaveInputs,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostField {
    Port,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinField {
    Uri,
    Port,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterTone {
    Accent,
    Info,
    Error,
}

/// Ratatui 界面持有的状态。
pub struct Model {
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
    pub tunnel: TunnelStatus,
    pub host_port: InputField,
    pub host_field: HostField,
    pub join_uri: InputField,
    pub join_port: InputField,
    pub join_field: JoinField,
    pub relay_url: InputField,
}

impl Model {
    pub fn new(
        profile: &sculk::persist::Profile,
        tunnel: TunnelStatus,
        profile_err: Option<String>,
    ) -> Self {
        let relay_idx = usize::from(profile.relay.custom);
        let relay_url = profile.relay.url.clone().unwrap_or_default();
        let host_port = profile.host.port.to_string();
        let join_port = profile.join.port.to_string();

        let mut model = Self {
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
            tunnel,
            host_port: InputField::with_value("端口", &host_port),
            host_field: HostField::Port,
            join_uri: InputField::new("分享 URI"),
            join_port: InputField::with_value("端口", &join_port),
            join_field: JoinField::Uri,
            relay_url: InputField::with_value("URL", &relay_url),
        };
        model.relay_state.select(Some(relay_idx));
        model.add_log("已就绪，按 Enter 执行当前模式");
        if let Some(err) = profile_err {
            model.add_log(&err);
        }
        model
    }

    pub fn handle_tunnel_update(&mut self, update: TunnelUpdate) {
        match update {
            TunnelUpdate::Status(status) => self.apply_status(status),
            TunnelUpdate::Event(event) => self.apply_event(event),
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        if let Some(pressed_at) = self.quit_pressed_at
            && Instant::now().duration_since(pressed_at).as_secs() >= 1
        {
            self.quit_pressed_at = None;
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.log_state.select(None);
        self.add_log("日志已清空");
    }

    pub fn add_log(&mut self, message: &str) {
        self.logs.push(message.to_owned());
        if self.logs.len() > LOG_CAP {
            let count = self.logs.len() - LOG_CAP;
            self.logs.drain(0..count);
        }
        self.log_state
            .select(Some(self.logs.len().saturating_sub(1)));
    }

    pub fn next_log(&mut self) {
        if self.logs.is_empty() {
            self.log_state.select(None);
            return;
        }
        let index = match self.log_state.selected() {
            Some(index) if index + 1 < self.logs.len() => index + 1,
            Some(index) => index,
            None => 0,
        };
        self.log_state.select(Some(index));
    }

    pub fn prev_log(&mut self) {
        if self.logs.is_empty() {
            self.log_state.select(None);
            return;
        }
        let index = self
            .log_state
            .selected()
            .unwrap_or_default()
            .saturating_sub(1);
        self.log_state.select(Some(index));
    }

    pub fn next_relay(&mut self) {
        let index = match self.relay_state.selected() {
            Some(index) if index + 1 < RELAYS.len() => index + 1,
            Some(index) => index,
            None => 0,
        };
        self.relay_state.select(Some(index));
    }

    pub fn prev_relay(&mut self) {
        let index = self
            .relay_state
            .selected()
            .unwrap_or_default()
            .saturating_sub(1);
        self.relay_state.select(Some(index));
    }

    pub fn status_label(&self) -> (&str, crate::ui::StatusColor) {
        use crate::ui::StatusColor;
        match self.tunnel.state.phase {
            TunnelPhase::Idle => ("空闲", StatusColor::Warn),
            TunnelPhase::Starting => ("连接中...", StatusColor::Info),
            TunnelPhase::Active => match self.tunnel.state.mode {
                Some(TunnelMode::Host) => ("托管中", StatusColor::Accent),
                Some(TunnelMode::Join) => ("已加入", StatusColor::Info),
                None => ("活跃", StatusColor::Accent),
            },
            TunnelPhase::Stopping => ("关闭中...", StatusColor::Warn),
        }
    }

    pub fn route_strength(&self) -> u8 {
        if self.tunnel.connections.is_empty() {
            return if self.tunnel.state.phase == TunnelPhase::Active {
                50
            } else {
                0
            };
        }
        let rtt_sum: u64 = self.tunnel.connections.iter().map(|item| item.rtt_ms).sum();
        let rtt_avg = rtt_sum / self.tunnel.connections.len() as u64;
        ((100_u64.saturating_sub(rtt_avg / 5)).clamp(10, 98)) as u8
    }

    pub fn route_info(&self) -> &str {
        match self.tunnel.connections.first() {
            Some(connection) if connection.is_relay => "中继",
            Some(_) => "直连",
            None => "无",
        }
    }

    pub fn gauge_label(&self) -> String {
        if self.tunnel.connections.is_empty() {
            return if self.tunnel.state.phase == TunnelPhase::Active {
                "等待连接...".to_string()
            } else {
                "离线".to_string()
            };
        }
        let rtt_sum: u64 = self.tunnel.connections.iter().map(|item| item.rtt_ms).sum();
        let rtt_avg = rtt_sum / self.tunnel.connections.len() as u64;
        format!(
            "{}% | {}ms | {} | {}人",
            self.route_strength(),
            rtt_avg,
            self.route_info(),
            self.tunnel.connections.len()
        )
    }

    pub fn esc_action_label(&self) -> &'static str {
        if self.tunnel.state.phase == TunnelPhase::Idle {
            "退出"
        } else {
            "断开"
        }
    }

    pub fn esc_can_exit(&self) -> bool {
        self.tunnel.state.phase == TunnelPhase::Idle
    }

    fn apply_status(&mut self, status: TunnelStatus) {
        let previous_phase = self.tunnel.state.phase;
        self.tunnel = status;

        if self.tunnel.state.phase != previous_phase {
            self.quit_pressed_at = None;
        }
        if self.tunnel.state.phase == TunnelPhase::Active && previous_phase == TunnelPhase::Starting
        {
            match self.tunnel.state.mode {
                Some(TunnelMode::Host) => {
                    self.add_log("host 隧道已启动");
                }
                Some(TunnelMode::Join) => {
                    self.add_log("已成功连入隧道");
                }
                None => {}
            }
        }
        if self.tunnel.state.phase == TunnelPhase::Idle
            && previous_phase != TunnelPhase::Idle
            && matches!(previous_phase, TunnelPhase::Active | TunnelPhase::Stopping)
        {
            self.add_log("隧道已关闭");
        }
    }

    fn apply_event(&mut self, event: TunnelEvent) {
        let message = match event {
            TunnelEvent::PlayerJoined { id } => format!("玩家加入: {id}"),
            TunnelEvent::PlayerLeft { id, reason } => format!("玩家离开: {id} ({reason})"),
            TunnelEvent::Connected => "已连接到 host".to_string(),
            TunnelEvent::Disconnected { reason } => format!("连接断开: {reason}"),
            TunnelEvent::PathChanged {
                remote_id,
                is_relay,
                rtt_ms,
            } => {
                let route = if is_relay { "中继" } else { "直连" };
                format!("{remote_id} 路径: {route}, RTT: {rtt_ms}ms")
            }
            TunnelEvent::Reconnecting { attempt } => format!("正在重连 (第 {attempt} 次)..."),
            TunnelEvent::Reconnected => "重连成功".to_string(),
            TunnelEvent::TokenRotationFailed { retry_in } => {
                format!("令牌轮换失败，将在 {retry_in:?} 后重试")
            }
            TunnelEvent::AuthFailed { id } => format!("认证失败: {id}"),
            TunnelEvent::PlayerRejected { id, reason } => {
                format!("玩家被拒: {id} ({reason})")
            }
            TunnelEvent::Error { message } => format!("错误: {message}"),
            _ => "未知事件".to_string(),
        };
        self.add_log(&message);
    }
}

impl ActiveTab {
    pub fn index(self) -> usize {
        match self {
            Self::Host => 0,
            Self::Join => 1,
            Self::Relay => 2,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Host => Self::Join,
            Self::Join | Self::Relay => Self::Relay,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Host | Self::Join => Self::Host,
            Self::Relay => Self::Join,
        }
    }
}
