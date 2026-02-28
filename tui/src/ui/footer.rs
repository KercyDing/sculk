//! 底部快捷键提示。

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme::{ACCENT, ERROR, PANEL};
use crate::state::{AppState, FocusPane};

pub fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
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
