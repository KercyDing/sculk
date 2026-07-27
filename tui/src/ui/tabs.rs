//! 建房、加入和中继标签页渲染。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};

use super::{ACCENT, BG, INFO, WARN, border_style};
use crate::model::{ActiveTab, FocusPane, HostField, JoinField, Model, RELAYS, TAB_TITLES};

pub fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model) {
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);
    let focused = model.focus == FocusPane::Profile;

    let tabs = Tabs::new(TAB_TITLES)
        .select(model.tab.index())
        .style(Style::default().fg(Color::Gray).bg(BG))
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" • ")
        .block(
            Block::default()
                .title("模式")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(focused))
                .style(Style::default().bg(BG)),
        );
    frame.render_widget(tabs, sections[0]);

    let title = match model.tab {
        ActiveTab::Host => "建房配置",
        ActiveTab::Join => "加入配置",
        ActiveTab::Relay => "中继列表",
    };
    let panel_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(focused))
        .style(Style::default().bg(BG));
    let inner = panel_block.inner(sections[1]);
    frame.render_widget(panel_block, sections[1]);

    match model.tab {
        ActiveTab::Host => render_host_fields(frame, inner, model, focused),
        ActiveTab::Join => render_join_fields(frame, inner, model, focused),
        ActiveTab::Relay => render_relay_fields(frame, inner, model, focused),
    }
}

fn render_host_fields(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model, focused: bool) {
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
            Span::styled("角色: ", Style::default().fg(Color::DarkGray).bg(BG)),
            Span::styled(
                "建房",
                Style::default()
                    .fg(ACCENT)
                    .bg(BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    render_field_line(
        frame,
        rows[2],
        model.host_port.label,
        &model.host_port.value,
        focused && model.host_field == HostField::Port,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "i 编辑 | ↑/↓ 切换字段",
            Style::default().fg(Color::DarkGray).bg(BG),
        )),
        rows[3],
    );
}

fn render_join_fields(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model, focused: bool) {
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
            Span::styled("角色: ", Style::default().fg(Color::DarkGray).bg(BG)),
            Span::styled(
                "加入",
                Style::default()
                    .fg(INFO)
                    .bg(BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    render_field_line(
        frame,
        rows[2],
        model.join_uri.label,
        &model.join_uri.value,
        focused && model.join_field == JoinField::Uri,
    );
    render_field_line(
        frame,
        rows[3],
        model.join_port.label,
        &model.join_port.value,
        focused && model.join_field == JoinField::Port,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "i 编辑 | ↑/↓ 切换字段",
            Style::default().fg(Color::DarkGray).bg(BG),
        )),
        rows[5],
    );
}

fn render_relay_fields(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model, focused: bool) {
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
            Span::styled("角色: ", Style::default().fg(Color::DarkGray).bg(BG)),
            Span::styled(
                "中继",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let selected = model.relay_state.selected().unwrap_or_default();
    for (i, label) in RELAYS.iter().enumerate() {
        let is_selected = focused && i == selected;
        let marker = if is_selected { "▶ " } else { "  " };
        let suffix = if i == model.relay_idx {
            " (已应用)"
        } else {
            ""
        };

        let style = if is_selected {
            Style::default().fg(ACCENT).bg(BG)
        } else if i == model.relay_idx {
            Style::default().fg(Color::White).bg(BG)
        } else {
            Style::default().fg(Color::Gray).bg(BG)
        };

        frame.render_widget(
            Paragraph::new(Span::styled(format!("{marker}{label}{suffix}"), style)),
            rows[2 + i],
        );
    }

    if selected == 1 {
        render_field_line(
            frame,
            rows[4],
            model.relay_url.label,
            &model.relay_url.value,
            false,
        );
    }

    let hint = if selected == 1 {
        "Enter 应用 | ↑/↓ 选择 | i 编辑URL"
    } else {
        "Enter 应用 | ↑/↓ 选择"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray).bg(BG),
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
        Style::default().fg(ACCENT).bg(BG)
    } else {
        Style::default().fg(WARN).bg(BG)
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
        Style::default().fg(Color::White).bg(BG)
    } else {
        Style::default().fg(Color::Gray).bg(BG)
    };
    frame.render_widget(Paragraph::new(Span::styled(display, value_style)), cols[1]);
}
