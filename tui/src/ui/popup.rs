//! 帮助弹窗、编辑弹窗与 centered_rect 工具。

use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::theme::{ACCENT, INFO, PANEL_ALT, WARN};
use crate::state::{AppState, HelpLineSpec};

pub fn render_help_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let spec = state.help_popup_spec();
    if !spec.show {
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

    let key_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let sep = Style::default().fg(ratatui::style::Color::DarkGray);

    let mut lines = Vec::new();
    for line in spec.lines {
        match line {
            HelpLineSpec::Title(text) => lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))),
            HelpLineSpec::Empty => lines.push(Line::raw("")),
            HelpLineSpec::Shortcut { key, description } => lines.push(Line::from(vec![
                Span::styled(key, key_style),
                Span::styled(" — ", sep),
                Span::raw(description),
            ])),
            HelpLineSpec::Raw(text) => lines.push(Line::raw(text)),
        }
    }

    let help = Paragraph::new(Text::from(lines));
    frame.render_widget(help, popup.inner(Margin::new(1, 1)));
}

/// 中止隧道确认弹窗。
pub fn render_confirm_stop_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let spec = state.confirm_stop_popup_spec();
    if !spec.show {
        return;
    }

    let popup = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(7),
        Constraint::Fill(1),
    ])
    .split(area);

    let popup = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(36),
        Constraint::Fill(1),
    ])
    .split(popup[1])[1];

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" 中止隧道 ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(PANEL_ALT))
        .border_style(Style::default().fg(WARN));
    frame.render_widget(block, popup);

    let inner = popup.inner(Margin::new(2, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(spec.prompt, Style::default().fg(Color::White))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 停止    ", Style::default().fg(Color::Gray)),
            Span::styled(
                "[N]",
                Style::default().fg(INFO).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" 取消", Style::default().fg(Color::Gray)),
        ])),
        rows[2],
    );
}

/// 在给定区域内生成居中矩形。
pub fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
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

/// 编辑弹窗。
pub fn render_edit_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let spec = state.edit_popup_spec();
    if !spec.show {
        return;
    }

    let popup_h = (spec.fields.len() * 3 + 4) as u16;

    let vert = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(popup_h),
        Constraint::Fill(1),
    ])
    .split(area);

    let horiz = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(60),
        Constraint::Fill(1),
    ])
    .split(vert[1]);

    let popup = horiz[1];
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(spec.title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(PANEL_ALT))
        .border_style(Style::default().fg(ACCENT));
    frame.render_widget(block, popup);

    let inner = popup.inner(Margin::new(2, 1));

    let mut constraints = vec![Constraint::Length(1)];
    for _ in &spec.fields {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let rows = Layout::vertical(constraints).split(inner);

    for (i, field) in spec.fields.iter().enumerate() {
        let base = 1 + i * 3;
        let label_row = rows[base];
        let value_row = rows[base + 1];

        let label_style = if field.active {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(field.label, label_style)),
            label_row,
        );

        let max_w = value_row.width as usize;
        let chars: Vec<char> = field.value.chars().collect();
        let char_count = chars.len();

        let (display, cursor_offset) = if field.active {
            let cursor_char = field.value[..field.cursor].chars().count();
            if char_count == 0 {
                (" ".to_string(), 0usize)
            } else {
                let start = if cursor_char >= max_w {
                    cursor_char - max_w + 1
                } else {
                    0
                };
                let end = (start + max_w).min(char_count);
                let s: String = chars[start..end].iter().collect();
                (s, cursor_char - start)
            }
        } else if field.value.is_empty() {
            ("(空)".to_string(), 0)
        } else if char_count <= max_w {
            (field.value.clone(), 0)
        } else {
            let mut s: String = chars[..max_w.saturating_sub(1)].iter().collect();
            s.push('…');
            (s, 0)
        };

        let value_style = if field.active {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(display, value_style)),
            value_row,
        );

        if field.active {
            frame.set_cursor_position((value_row.x + cursor_offset as u16, value_row.y));
        }
    }

    let hint_row = rows[1 + spec.fields.len() * 3];
    frame.render_widget(
        Paragraph::new(Span::styled(
            spec.hint,
            Style::default().fg(Color::DarkGray),
        )),
        hint_row,
    );
}
