//! 帮助弹窗与 centered_rect 工具。

use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

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
