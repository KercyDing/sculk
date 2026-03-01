//! 帮助弹窗与 centered_rect 工具。

use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::theme::{ACCENT, INFO, PANEL_ALT};
use crate::state::AppState;

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
            Span::styled("e", key_style),
            Span::styled(" — ", sep),
            Span::raw("进入编辑"),
        ]),
        Line::from(vec![
            Span::styled("q", key_style),
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
