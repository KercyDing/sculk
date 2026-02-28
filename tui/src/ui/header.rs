//! 顶部状态栏渲染。

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::theme::{ACCENT, INFO, PANEL, WARN};
use crate::state::AppState;

pub fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let status = if state.hosting {
        Span::styled(
            "托管中",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )
    } else if state.joined {
        Span::styled(
            "已加入",
            Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "空闲",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )
    };
    let route = Span::styled(
        format!("路由-{}", state.route_idx + 1),
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
                .fg(ACCENT)
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
