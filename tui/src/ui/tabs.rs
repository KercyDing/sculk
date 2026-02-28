//! 左侧面板：Tab 选择 + Host/Join/Relay 内容。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{
    Block, BorderType, Borders, HighlightSpacing, List, ListItem, Paragraph, Tabs, Wrap,
};

use super::theme::{ACCENT, PANEL_ALT, border_style};
use crate::state::{ActiveTab, AppState, FocusPane, RELAYS, TAB_TITLES};

pub fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);

    let tabs = Tabs::new(TAB_TITLES)
        .select(state.tab.index())
        .style(Style::default().fg(ratatui::style::Color::Gray))
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
                    .fg(ratatui::style::Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(relay_list, sections[1], &mut state.relay_state);
    } else {
        let content = Paragraph::new(state.mode_profile())
            .block(panel_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(content, sections[1]);
    }
}
