//! 底部快捷键提示。

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme::{ACCENT, ERROR, INFO, PANEL};
use crate::state::{AppState, InputMode};

pub fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    if state.input_mode == InputMode::Editing {
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("编辑模式", Style::default().fg(INFO).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("q", Style::default().fg(ACCENT)),
            Span::raw(" 退出编辑  "),
            Span::styled("Tab", Style::default().fg(ACCENT)),
            Span::raw(" 下个字段  "),
        ]))
        .alignment(Alignment::Left)
        .style(Style::default().bg(PANEL));
        frame.render_widget(footer, area);
        return;
    }

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(ACCENT)),
        Span::raw(" 执行  "),
        Span::styled("e", Style::default().fg(ACCENT)),
        Span::raw(" 编辑  "),
        Span::styled("←/→", Style::default().fg(ACCENT)),
        Span::raw(" 模式  "),
        Span::styled("Tab", Style::default().fg(ACCENT)),
        Span::raw(" 焦点  "),
        Span::styled("↑/↓", Style::default().fg(ACCENT)),
        Span::raw(" 字段  "),
        Span::styled("h", Style::default().fg(ACCENT)),
        Span::raw(" 帮助  "),
        Span::styled("Esc", Style::default().fg(ERROR)),
        Span::raw(" 退出"),
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
