//! 左侧面板：Tab 选择 + Host/Join/Relay 输入字段。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};

use super::theme::{ACCENT, INFO, LEFT_PANEL_BG, WARN, border_style};
use crate::state::{AppState, PanelSpec};

pub fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    frame.render_widget(
        Block::default().style(Style::default().bg(LEFT_PANEL_BG)),
        area,
    );

    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);
    let spec = state.tabs_spec();

    let tabs = Tabs::new(*spec.titles)
        .select(spec.selected_tab)
        .style(Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG))
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .bg(LEFT_PANEL_BG)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" • ")
        .block(
            Block::default()
                .title("模式")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(spec.profile_focused))
                .style(Style::default().bg(LEFT_PANEL_BG)),
        );
    frame.render_widget(tabs, sections[0]);

    let panel_block = Block::default()
        .title(spec.panel_title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(spec.profile_focused))
        .style(Style::default().bg(LEFT_PANEL_BG));
    let inner = panel_block.inner(sections[1]);
    frame.render_widget(panel_block, sections[1]);

    match spec.panel {
        PanelSpec::Host { fields, hint } => render_host_fields(frame, inner, fields, hint),
        PanelSpec::Join { fields, hint } => render_join_fields(frame, inner, fields, hint),
        PanelSpec::Relay {
            options,
            relay_url,
            hint,
        } => render_relay_fields(frame, inner, options, relay_url, hint),
    }
}

fn render_host_fields(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    fields: Vec<crate::state::FieldSpec>,
    hint: &'static str,
) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "建房",
                Style::default()
                    .fg(ACCENT)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    if let Some(field) = fields.first() {
        render_field_line(frame, rows[2], field.label, &field.value, field.selected);
    }
    if let Some(field) = fields.get(1) {
        render_field_line(frame, rows[3], field.label, &field.value, field.selected);
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[5],
    );
}

fn render_join_fields(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    fields: Vec<crate::state::FieldSpec>,
    hint: &'static str,
) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "加入",
                Style::default()
                    .fg(INFO)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    if let Some(field) = fields.first() {
        render_field_line(frame, rows[2], field.label, &field.value, field.selected);
    }
    if let Some(field) = fields.get(1) {
        render_field_line(frame, rows[3], field.label, &field.value, field.selected);
    }
    if let Some(field) = fields.get(2) {
        render_field_line(frame, rows[4], field.label, &field.value, field.selected);
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[6],
    );
}

fn render_relay_fields(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    options: Vec<crate::state::RelayOptionSpec>,
    relay_url: Option<crate::state::FieldSpec>,
    hint: &'static str,
) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "中继",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    for (i, option) in options.iter().enumerate() {
        let marker = if option.selected { "▶ " } else { "  " };
        let suffix = if option.applied { " (已应用)" } else { "" };

        let style = if option.selected {
            Style::default().fg(ACCENT).bg(LEFT_PANEL_BG)
        } else if option.applied {
            Style::default().fg(Color::White).bg(LEFT_PANEL_BG)
        } else {
            Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG)
        };

        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{marker}{}{suffix}", option.label),
                style,
            )),
            rows[2 + i],
        );
    }

    if let Some(field) = relay_url {
        render_field_line(frame, rows[4], field.label, &field.value, field.selected);
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[6],
    );
}

fn render_field_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    label: &'static str,
    value: &str,
    selected: bool,
) {
    let label_width = 8u16;
    let cols =
        Layout::horizontal([Constraint::Length(label_width), Constraint::Min(4)]).split(area);

    let label_style = if selected {
        Style::default().fg(ACCENT).bg(LEFT_PANEL_BG)
    } else {
        Style::default().fg(WARN).bg(LEFT_PANEL_BG)
    };

    let marker = if selected { "▶ " } else { "  " };
    frame.render_widget(
        Paragraph::new(Span::styled(format!("{}{}: ", marker, label), label_style)),
        cols[0],
    );

    let max_w = cols[1].width as usize;
    let chars: Vec<char> = value.chars().collect();
    let char_count = chars.len();

    let display = if value.is_empty() {
        "(空)".to_string()
    } else if char_count <= max_w {
        value.to_string()
    } else {
        let mut s: String = chars[..max_w.saturating_sub(1)].iter().collect();
        s.push('…');
        s
    };

    let value_style = if selected {
        Style::default().fg(Color::White).bg(LEFT_PANEL_BG)
    } else {
        Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG)
    };
    frame.render_widget(Paragraph::new(Span::styled(display, value_style)), cols[1]);
}
