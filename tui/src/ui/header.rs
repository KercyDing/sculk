//! 顶部状态栏渲染。

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::theme::PANEL;
use crate::state::AppState;

pub fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let (label, color) = state.status_label();
    let status = Span::styled(
        label,
        Style::default()
            .fg(color.color())
            .add_modifier(Modifier::BOLD),
    );

    let route = Span::styled(
        state.route_info(),
        Style::default().fg(Color::Cyan),
    );
    let relay = Span::styled(
        state.relay_label(),
        Style::default().fg(Color::Magenta),
    );

    let line = Line::from(vec![
        Span::styled(
            "  SCULK 控制台  ",
            Style::default()
                .bg(Color::Rgb(8, 42, 35))
                .fg(super::theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("状态:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        status,
        Span::raw("    "),
        Span::styled("路由:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        route,
        Span::raw("    "),
        Span::styled("中继:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        relay,
    ]);

    let header = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(PANEL)),
    );
    frame.render_widget(header, area);
}
