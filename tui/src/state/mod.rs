//! 应用状态门面：采用分层状态机模块化实现。

use std::time::Instant;

use ratatui::widgets::ListState;
use tokio::sync::mpsc;

mod actions;
mod context;
mod dispatch;
mod events;
mod footer_spec;
mod input_field;
mod logs;
mod machine;
mod types;
mod ui_specs;
mod view;

pub use footer_spec::{FooterSpec, FooterTone};
pub use types::{
    ActiveTab, FocusPane, HostField, InputMode, JoinField, LOG_CAP, RELAYS, Step, TAB_TITLES,
    TunnelPhase,
};
pub use ui_specs::{FieldSpec, HelpLineSpec, PanelSpec, RelayOptionSpec};

pub(crate) use context::AppContext;
use input_field::InputField;

/// TUI 应用全量状态，公开渲染相关字段，内部副作用通过 `AppContext` 管理。
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

    // 生命周期状态
    pub phase: TunnelPhase,
    pub active_mode: Option<ActiveTab>,
    pub ticket: Option<String>,

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

    // 内部上下文
    pub(crate) ctx: AppContext,
}

impl AppState {
    /// 从持久化 Profile 初始化状态，加载失败时回退到默认值。
    ///
    /// Purpose: 建立应用启动时的完整状态快照。
    /// Args: `app_tx` 为应用事件发送端。
    /// Returns: 初始化后的 `AppState`。
    /// Edge Cases: 配置加载失败时回退默认值并写日志。
    pub fn new(app_tx: mpsc::UnboundedSender<crate::services::tunnel::AppEvent>) -> Self {
        let (profile, profile_err) = crate::services::persist::load_profile();

        let relay_idx = if profile.relay.custom { 1 } else { 0 };
        let relay_url_value = match profile.relay.url.clone() {
            Some(url) => url,
            None => String::new(),
        };

        let host_port_str = profile.host.port.to_string();
        let join_port_str = profile.join.port.to_string();
        let last_ticket = match profile.join.last_ticket.clone() {
            Some(ticket) => ticket,
            None => String::new(),
        };

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
            ticket: None,

            connections: Vec::new(),

            host_port: InputField::with_value("端口", &host_port_str),
            host_password: InputField::new("密码"),
            host_field: HostField::Port,

            join_ticket: InputField::with_value("票据", &last_ticket),
            join_port: InputField::with_value("端口", &join_port_str),
            join_password: InputField::new("密码"),
            join_field: JoinField::Ticket,

            relay_url: InputField::with_value("URL", &relay_url_value),

            ctx: AppContext::new(app_tx, profile),
        };
        state.relay_state.select(Some(relay_idx));
        state.add_log("已就绪，按 Enter 执行当前模式");
        if let Some(err) = profile_err {
            state.add_log(&err);
        }
        state
    }

    /// 处理键盘事件，返回 [`Step::Exit`] 时退出事件循环。
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Step {
        dispatch::handle_key(self, key)
    }

    /// 根据当前标签页执行主操作：启动/停止隧道或应用中继配置。
    pub fn primary_action(&mut self) {
        actions::primary_action(self);
    }

    /// 处理来自隧道任务的内部事件。
    pub fn handle_app_event(&mut self, event: crate::services::tunnel::AppEvent) {
        events::handle_app_event(self, event);
    }

    /// 定时刷新：递增 tick、清除超时退出提示、更新连接快照。
    pub fn on_tick(&mut self) {
        events::on_tick(self);
    }

    /// 返回当前状态标签文字与配色。
    pub fn status_label(&self) -> (&str, crate::ui::theme::StatusColor) {
        view::status_label(self)
    }

    /// 连接质量百分比。
    pub fn route_strength(&self) -> u8 {
        view::route_strength(self)
    }

    /// 当前链路类型。
    pub fn route_info(&self) -> &str {
        view::route_info(self)
    }

    /// 链路仪表盘标签。
    pub fn gauge_label(&self) -> String {
        view::gauge_label(self)
    }

    /// 连接数标签。
    pub fn connection_label(&self) -> String {
        view::connection_label(self)
    }

    /// 当前中继标签。
    pub fn relay_label(&self) -> &str {
        view::relay_label(self)
    }

    /// Esc 动作文案。
    pub fn esc_action_label(&self) -> &'static str {
        view::esc_action_label(self)
    }

    /// 当前是否允许 Esc 双击退出。
    pub fn esc_can_exit(&self) -> bool {
        view::esc_can_exit(self)
    }

    /// 生成当前状态对应的 Footer 规格。
    pub fn footer_spec(&self) -> FooterSpec {
        footer_spec::footer_spec(self)
    }

    /// 生成 Header 渲染规格。
    pub(crate) fn header_spec(&self) -> ui_specs::HeaderSpec {
        ui_specs::header_spec(self)
    }

    /// 生成 Logs 渲染规格。
    pub(crate) fn logs_spec(
        &self,
        visible_height: usize,
        message_width: usize,
    ) -> ui_specs::LogsSpec {
        ui_specs::logs_spec(self, visible_height, message_width)
    }

    /// 生成 Tabs 渲染规格。
    pub(crate) fn tabs_spec(&self) -> ui_specs::TabsSpec {
        ui_specs::tabs_spec(self)
    }

    /// 生成帮助弹窗规格。
    pub(crate) fn help_popup_spec(&self) -> ui_specs::HelpPopupSpec {
        ui_specs::help_popup_spec(self)
    }

    /// 生成中止确认弹窗规格。
    pub(crate) fn confirm_stop_popup_spec(&self) -> ui_specs::ConfirmStopPopupSpec {
        ui_specs::confirm_stop_popup_spec(self)
    }

    /// 生成编辑弹窗规格。
    pub(crate) fn edit_popup_spec(&self) -> ui_specs::EditPopupSpec {
        ui_specs::edit_popup_spec(self)
    }

    /// 清空日志。
    pub fn clear_logs(&mut self) {
        logs::clear_logs(self);
    }

    /// 追加日志。
    pub fn add_log(&mut self, msg: &str) {
        logs::add_log(self, msg);
    }

    /// 日志选择向后移动。
    pub fn next_log(&mut self) {
        logs::next_log(self);
    }

    /// 日志选择向前移动。
    pub fn prev_log(&mut self) {
        logs::prev_log(self);
    }

    /// 中继选择向后移动。
    pub fn next_relay_selection(&mut self) {
        logs::next_relay_selection(self);
    }

    /// 中继选择向前移动。
    pub fn prev_relay_selection(&mut self) {
        logs::prev_relay_selection(self);
    }
}

#[cfg(test)]
mod tests;
