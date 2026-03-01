//! UI 渲染入口分发。

pub mod footer;
pub mod header;
pub mod logs;
pub mod popup;
pub mod tabs;
pub mod theme;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::state::AppState;
use theme::BG;

/// 顶层渲染入口。
pub fn render(frame: &mut ratatui::Frame<'_>, state: &mut AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .margin(1)
    .split(area);

    header::render_header(frame, layout[0], state);
    render_main(frame, layout[1], state);
    footer::render_footer(frame, layout[2], state);
    popup::render_help_popup(frame, area, state);
    popup::render_edit_popup(frame, area, state);
}

fn render_main(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect, state: &mut AppState) {
    let main =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    tabs::render_left(frame, main[0], state);
    logs::render_right(frame, main[1], state);
}
