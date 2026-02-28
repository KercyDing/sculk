//! `sculk-tui` 主界面与事件循环。

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};

const TICK: Duration = Duration::from_millis(200);
const LOG_CAP: usize = 200;
const TAB_TITLES: [&str; 3] = ["建房", "加入", "中继"];
const RELAYS: [&str; 3] = ["n0 默认中继", "亚洲中继池", "自建中继"];

const BG: Color = Color::Rgb(12, 15, 26);
const PANEL: Color = Color::Rgb(20, 26, 42);
const PANEL_ALT: Color = Color::Rgb(18, 32, 40);
const ACCENT: Color = Color::Rgb(74, 222, 128);
const INFO: Color = Color::Rgb(59, 130, 246);
const WARN: Color = Color::Rgb(245, 158, 11);
const ERROR: Color = Color::Rgb(248, 113, 113);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Host,
    Join,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusPane {
    Profile,
    Logs,
}

enum Step {
    Continue,
    Exit,
}

struct AppState {
    show_help: bool,
    tick: u64,
    tab: ActiveTab,
    focus: FocusPane,
    quit_pending: bool,
    logs: Vec<String>,
    log_state: ListState,
    relay_state: ListState,
    hosting: bool,
    joined: bool,
    relay_idx: usize,
    route_idx: usize,
}

impl Default for AppState {
    fn default() -> Self {
        let mut state = Self {
            show_help: false,
            tick: 0,
            tab: ActiveTab::Host,
            focus: FocusPane::Profile,
            quit_pending: false,
            logs: Vec::new(),
            log_state: ListState::default(),
            relay_state: ListState::default(),
            hosting: false,
            joined: false,
            relay_idx: 0,
            route_idx: 0,
        };
        state.relay_state.select(Some(0));
        state.add_log("sculk-tui 已就绪，按 Enter 执行当前模式");
        state
    }
}

/// 启动 TUI 并处理按键事件，按 `Esc` 连按两次退出。
pub fn run_tui() -> anyhow::Result<()> {
    let mut terminal = init_terminal()?;
    let mut state = AppState::default();
    let run_result = (|| -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| render(frame, &mut state))?;

            if !event::poll(TICK)? {
                state.on_tick();
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }

            if matches!(state.handle_key(key), Step::Exit) {
                break;
            }
        }
        Ok(())
    })();

    restore_terminal(&mut terminal)?;
    run_result
}

fn init_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    terminal.show_cursor()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

fn render(frame: &mut ratatui::Frame<'_>, state: &mut AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .margin(1)
    .split(area);

    render_header(frame, layout[0], state);
    render_main(frame, layout[1], state);
    render_footer(frame, layout[2], state);
    render_help_popup(frame, area, state);
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let status = if state.hosting {
        Span::styled(
            "托管中",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )
    } else if state.joined {
        Span::styled(
            "已加入",
            Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "空闲",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )
    };
    let route = Span::styled(
        format!("路由-{}", state.route_idx + 1),
        Style::default().fg(Color::Cyan),
    );
    let relay = Span::styled(RELAYS[state.relay_idx], Style::default().fg(Color::Magenta));

    let line = Line::from(vec![
        Span::styled(
            "  SCULK 控制台  ",
            Style::default()
                .bg(Color::Rgb(8, 42, 35))
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("状态:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        status,
        Span::raw("    "),
        Span::styled("路由:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        route,
        Span::raw("    "),
        Span::styled("中继:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        relay,
    ]);

    let header = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(PANEL)),
    );
    frame.render_widget(header, area);
}

fn render_main(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let main =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    render_left(frame, main[0], state);
    render_right(frame, main[1], state);
}

fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);

    let tabs = Tabs::new(TAB_TITLES)
        .select(state.tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(" • ")
        .block(
            Block::default()
                .title("模式")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(state.focus == FocusPane::Profile))
                .style(Style::default().bg(PANEL_ALT)),
        );
    frame.render_widget(tabs, sections[0]);

    let panel_block = Block::default()
        .title("概要")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(state.focus == FocusPane::Profile))
        .style(Style::default().bg(PANEL_ALT));

    if state.tab == ActiveTab::Relay {
        let items: Vec<ListItem<'_>> = RELAYS
            .iter()
            .enumerate()
            .map(|(i, relay)| {
                let marker = if i == state.relay_idx {
                    "已应用"
                } else {
                    "待选"
                };
                ListItem::new(format!("{relay}  ({marker})"))
            })
            .collect();
        let relay_list = List::new(items)
            .block(panel_block.title("中继列表"))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ")
            .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);
        frame.render_stateful_widget(relay_list, sections[1], &mut state.relay_state);
    } else {
        let content = Paragraph::new(state.mode_profile())
            .block(panel_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(content, sections[1]);
    }
}

fn render_right(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(5), Constraint::Min(8)]).split(area);
    let strength = state.route_strength();
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title("链路质量")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(PANEL))
                .border_style(border_style(false)),
        )
        .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(12, 40, 30)))
        .label(format!("{strength}%"))
        .percent(strength as u16);
    frame.render_widget(gauge, sections[0]);

    let log_items: Vec<ListItem<'_>> = state
        .logs
        .iter()
        .enumerate()
        .map(|(i, msg)| ListItem::new(format!("[{:03}] {msg}", i + 1)))
        .collect();
    let logs = List::new(log_items)
        .block(
            Block::default()
                .title("会话日志")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(state.focus == FocusPane::Logs))
                .style(Style::default().bg(PANEL)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(INFO)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);
    frame.render_stateful_widget(logs, sections[1], &mut state.log_state);
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let focus = match state.focus {
        FocusPane::Profile => "概要",
        FocusPane::Logs => "日志",
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(ACCENT)),
        Span::raw(" 执行  "),
        Span::styled("←/→", Style::default().fg(ACCENT)),
        Span::raw(" 切模式  "),
        Span::styled("Tab", Style::default().fg(ACCENT)),
        Span::raw(" 焦点  "),
        Span::styled("↑/↓", Style::default().fg(ACCENT)),
        Span::raw(" 列表/日志  "),
        Span::styled("h", Style::default().fg(ACCENT)),
        Span::raw(" 帮助  "),
        Span::styled("双击Esc", Style::default().fg(ERROR)),
        Span::raw(" 退出  "),
        Span::raw(format!("  [焦点: {focus}]")),
    ]))
    .alignment(Alignment::Left)
    .style(Style::default().bg(PANEL));
    frame.render_widget(footer, area);

    if state.quit_pending {
        let hint = Paragraph::new(Line::from(vec![Span::styled(
            "再次按 Esc 退出",
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(PANEL));
        frame.render_widget(hint, area);
    }
}

fn render_help_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    if !state.show_help {
        return;
    }

    let popup = centered_rect(64, 52, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title("帮助")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(PANEL_ALT))
        .border_style(Style::default().fg(INFO));
    frame.render_widget(block, popup);

    let help = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            "SCULK TUI 快捷键",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::raw("Enter / Space : 执行当前模式"),
        Line::raw("Left / Right   : 切换 建房 / 加入 / 中继 模式"),
        Line::raw("Tab            : 在 概要 与 日志 间切换焦点"),
        Line::raw("Up / Down      : 中继页选中列表（其他页浏览日志）"),
        Line::raw("r              : 轮换模拟路由"),
        Line::raw("c              : 清空日志"),
        Line::raw("h / ?          : 显示或关闭帮助"),
        Line::raw("Esc (连按两次) : 退出"),
        Line::raw(""),
        Line::raw("该界面是高保真交互骨架，后续可直接接入真实 tunnel 事件流。"),
    ]))
    .wrap(Wrap { trim: true });
    frame.render_widget(help, popup.inner(Margin::new(1, 1)));
}

fn border_style(active: bool) -> Style {
    if active {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vertical[1])[1]
}

impl AppState {
    /// 处理单个键盘事件并返回循环控制信号。
    fn handle_key(&mut self, key: KeyEvent) -> Step {
        if !matches!(key.code, KeyCode::Esc) {
            self.quit_pending = false;
        }
        match key.code {
            KeyCode::Esc => {
                if self.quit_pending {
                    Step::Exit
                } else {
                    self.quit_pending = true;
                    Step::Continue
                }
            }
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
                } else {
                    self.prev_log();
                }
                Step::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.tab == ActiveTab::Relay && self.focus == FocusPane::Profile {
                    self.next_relay_selection();
                } else {
                    self.next_log();
                }
                Step::Continue
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.primary_action();
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
    fn on_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        if self.tick.is_multiple_of(25) && (self.hosting || self.joined) {
            self.add_log("心跳正常，链路稳定");
        }
    }

    fn primary_action(&mut self) {
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

    fn rotate_route(&mut self) {
        self.route_idx = (self.route_idx + 1) % 3;
        self.add_log(&format!("路由已切换到方案-{}", self.route_idx + 1));
    }

    fn clear_logs(&mut self) {
        self.logs.clear();
        self.log_state.select(None);
        self.add_log("日志已清空");
    }

    fn add_log(&mut self, msg: &str) {
        self.logs.push(msg.to_string());
        if self.logs.len() > LOG_CAP {
            let to_drop = self.logs.len() - LOG_CAP;
            self.logs.drain(0..to_drop);
        }
        self.log_state
            .select(Some(self.logs.len().saturating_sub(1)));
    }

    fn next_log(&mut self) {
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

    fn prev_log(&mut self) {
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

    fn next_relay_selection(&mut self) {
        let next = match self.relay_state.selected() {
            Some(i) if i + 1 < RELAYS.len() => i + 1,
            _ => 0,
        };
        self.relay_state.select(Some(next));
    }

    fn prev_relay_selection(&mut self) {
        let prev = match self.relay_state.selected() {
            Some(0) | None => RELAYS.len() - 1,
            Some(i) => i - 1,
        };
        self.relay_state.select(Some(prev));
    }

    fn mode_profile(&self) -> Text<'_> {
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

    fn route_strength(&self) -> u8 {
        let base = match self.route_idx {
            0 => 84_i16,
            1 => 66_i16,
            _ => 74_i16,
        };
        let pulse = ((self.tick % 9) as i16 - 4) * 2;
        let value = (base + pulse).clamp(35, 98);
        value as u8
    }
}

impl ActiveTab {
    fn index(self) -> usize {
        match self {
            ActiveTab::Host => 0,
            ActiveTab::Join => 1,
            ActiveTab::Relay => 2,
        }
    }

    fn next(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Join,
            ActiveTab::Join => ActiveTab::Relay,
            ActiveTab::Relay => ActiveTab::Relay,
        }
    }

    fn prev(self) -> Self {
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

    use super::{ActiveTab, AppState, RELAYS, Step};

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
        let mut state = AppState::default();
        assert!(matches!(
            state.handle_key(key(KeyCode::Esc)),
            Step::Continue
        ));
        assert!(state.quit_pending);
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));
        let mut state = AppState::default();
        assert!(matches!(
            state.handle_key(key(KeyCode::Char('q'))),
            Step::Continue
        ));
        assert!(!state.quit_pending);
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
        state.focus = super::FocusPane::Profile;
        state.relay_state.select(Some(0));

        assert!(matches!(
            state.handle_key(key(KeyCode::Down)),
            Step::Continue
        ));
        assert_eq!(state.relay_state.selected(), Some(1));

        assert!(matches!(state.handle_key(key(KeyCode::Up)), Step::Continue));
        assert_eq!(state.relay_state.selected(), Some(0));
    }
}
