//! 右侧面板：链路质量 Gauge + 日志列表。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::theme::{ACCENT, BG, border_style};
use crate::state::AppState;

pub fn render_right(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);
    let base_spec = state.logs_spec(0, 0);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title("链路质量")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(BG))
                .border_style(border_style(false)),
        )
        .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(12, 40, 30)))
        .label(base_spec.gauge.label)
        .percent(base_spec.gauge.strength as u16);
    frame.render_widget(gauge, sections[0]);

    let block = Block::default()
        .title("会话日志")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(base_spec.focus_logs))
        .style(Style::default().bg(BG));
    let inner = block.inner(sections[1]);
    frame.render_widget(block, sections[1]);

    let message_width = (inner.width as usize).saturating_sub(log_row_prefix_width());
    let spec = state.logs_spec(inner.height as usize, message_width);
    for (vi, row_spec) in spec.rows.iter().enumerate() {
        let is_selected = row_spec.selected;
        let marker = if is_selected { "▶ " } else { "  " };
        let style = if is_selected {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(Color::Gray)
        };
        let row = Rect {
            x: inner.x,
            y: inner.y + vi as u16,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{marker}[{:03}] {}", row_spec.index + 1, row_spec.text),
                style,
            )),
            row,
        );
    }
}

fn log_row_prefix_width() -> usize {
    let selected_prefix = format!("▶ [{:03}] ", 1);
    let normal_prefix = format!("  [{:03}] ", 1);
    UnicodeWidthStr::width(selected_prefix.as_str())
        .max(UnicodeWidthStr::width(normal_prefix.as_str()))
}
