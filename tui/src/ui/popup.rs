//! 帮助弹窗、编辑弹窗与 centered_rect 工具。

use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::theme::{ACCENT, INFO, PANEL_ALT};
use crate::input::InputField;
use crate::state::{ActiveTab, AppState, HostField, InputMode, JoinField};

pub fn render_help_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
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

    let key_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let sep = Style::default().fg(ratatui::style::Color::DarkGray);

    let help = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            "SCULK TUI 快捷键",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Enter", key_style),
            Span::styled(" — ", sep),
            Span::raw("执行当前模式"),
        ]),
        Line::from(vec![
            Span::styled("←/→", key_style),
            Span::styled(" — ", sep),
            Span::raw("切换模式"),
        ]),
        Line::from(vec![
            Span::styled("Tab", key_style),
            Span::styled(" — ", sep),
            Span::raw("切换焦点"),
        ]),
        Line::from(vec![
            Span::styled("↑/↓", key_style),
            Span::styled(" — ", sep),
            Span::raw("字段/中继/日志"),
        ]),
        Line::from(vec![
            Span::styled("i", key_style),
            Span::styled(" — ", sep),
            Span::raw("进入编辑"),
        ]),
        Line::from(vec![
            Span::styled("Esc", key_style),
            Span::styled(" — ", sep),
            Span::raw("退出编辑"),
        ]),
        Line::from(vec![
            Span::styled("c", key_style),
            Span::styled(" — ", sep),
            Span::raw("清空日志"),
        ]),
        Line::from(vec![
            Span::styled("h", key_style),
            Span::styled(" — ", sep),
            Span::raw("开关帮助"),
        ]),
        Line::from(vec![
            Span::styled("Esc×2", key_style),
            Span::styled(" — ", sep),
            Span::raw("退出程序"),
        ]),
        Line::raw(""),
        Line::raw("建房 Enter 启动/停止隧道，"),
        Line::raw("票据自动复制到剪贴板。"),
    ]));
    frame.render_widget(help, popup.inner(Margin::new(1, 1)));
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

/// 编辑弹窗
pub fn render_edit_popup(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    if state.input_mode != InputMode::Editing {
        return;
    }

    let fields = edit_fields(state);
    let popup_h = (fields.len() * 3 + 4) as u16;

    // 垂直居中
    let vert = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(popup_h),
        Constraint::Fill(1),
    ])
    .split(area);

    // 水平居中
    let horiz = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(60),
        Constraint::Fill(1),
    ])
    .split(vert[1]);

    let popup = horiz[1];
    frame.render_widget(Clear, popup);

    let title = match state.tab {
        ActiveTab::Host => "编辑 · 建房配置",
        ActiveTab::Join => "编辑 · 加入配置",
        ActiveTab::Relay => "编辑 · 中继 URL",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(PANEL_ALT))
        .border_style(Style::default().fg(ACCENT));
    frame.render_widget(block, popup);

    let inner = popup.inner(Margin::new(2, 1));

    let mut constraints = vec![Constraint::Length(1)];
    for _ in &fields {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let rows = Layout::vertical(constraints).split(inner);

    for (i, (label, field, is_active)) in fields.iter().enumerate() {
        let base = 1 + i * 3;
        let label_row = rows[base];
        let value_row = rows[base + 1];

        let label_style = if *is_active {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(*label, label_style)),
            label_row,
        );

        let max_w = value_row.width as usize;
        let chars: Vec<char> = field.value.chars().collect();
        let char_count = chars.len();

        let (display, cursor_offset) = if *is_active {
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

        let value_style = if *is_active {
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

        if *is_active {
            frame.set_cursor_position((value_row.x + cursor_offset as u16, value_row.y));
        }
    }

    let hint_row = rows[1 + fields.len() * 3];
    frame.render_widget(
        Paragraph::new(Span::styled(
            "[↑/↓] 切换字段  [Esc] 保存",
            Style::default().fg(Color::DarkGray),
        )),
        hint_row,
    );
}

/// 返回当前 tab 的可编辑字段列表：(标签, 字段引用, 是否活跃)。
fn edit_fields<'a>(state: &'a AppState) -> Vec<(&'static str, &'a InputField, bool)> {
    match state.tab {
        ActiveTab::Host => vec![
            ("端口", &state.host_port, state.host_field == HostField::Port),
            (
                "密码",
                &state.host_password,
                state.host_field == HostField::Password,
            ),
        ],
        ActiveTab::Join => vec![
            (
                "票据",
                &state.join_ticket,
                state.join_field == JoinField::Ticket,
            ),
            (
                "端口",
                &state.join_port,
                state.join_field == JoinField::Port,
            ),
            (
                "密码",
                &state.join_password,
                state.join_field == JoinField::Password,
            ),
        ],
        ActiveTab::Relay => vec![("中继 URL", &state.relay_url, true)],
    }
}
