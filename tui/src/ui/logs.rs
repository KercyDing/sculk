//! 右侧面板：链路质量 Gauge + 日志列表。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};

use super::theme::{ACCENT, PANEL, border_style};
use crate::state::{AppState, FocusPane};

pub fn render_right(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(5), Constraint::Min(8)]).split(area);
    let strength = state.route_strength();
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title("链路质量")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(PANEL))
                .border_style(border_style(false)),
        )
        .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(12, 40, 30)))
        .label(format!("{strength}%"))
        .percent(strength as u16);
    frame.render_widget(gauge, sections[0]);

    let block = Block::default()
        .title("会话日志")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(state.focus == FocusPane::Logs))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(sections[1]);
    frame.render_widget(block, sections[1]);

    let selected = state.log_state.selected();
    let visible_height = inner.height as usize;

    // 计算滚动偏移，保证选中行可见
    let scroll = if let Some(sel) = selected {
        if sel >= visible_height {
            sel - visible_height + 1
        } else {
            0
        }
    } else {
        state.logs.len().saturating_sub(visible_height)
    };

    for (vi, idx) in (scroll..state.logs.len()).enumerate() {
        if vi >= visible_height {
            break;
        }
        let is_selected = selected == Some(idx);
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
                format!("{marker}[{:03}] {}", idx + 1, state.logs[idx]),
                style,
            )),
            row,
        );
    }
}
