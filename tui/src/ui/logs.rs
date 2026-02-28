//! 右侧面板：链路质量 Gauge + 日志列表。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, HighlightSpacing, List, ListItem,
};

use super::theme::{ACCENT, INFO, PANEL, border_style};
use crate::state::{AppState, FocusPane};

pub fn render_right(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
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
        .highlight_spacing(HighlightSpacing::Always);
    frame.render_stateful_widget(logs, sections[1], &mut state.log_state);
}
