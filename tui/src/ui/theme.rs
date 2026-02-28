//! 颜色常量与边框样式工具。

use ratatui::style::{Color, Style};

pub const BG: Color = Color::Rgb(12, 15, 26);
pub const PANEL: Color = Color::Rgb(20, 26, 42);
pub const PANEL_ALT: Color = Color::Rgb(18, 32, 40);
pub const ACCENT: Color = Color::Rgb(74, 222, 128);
pub const INFO: Color = Color::Rgb(59, 130, 246);
pub const WARN: Color = Color::Rgb(245, 158, 11);
pub const ERROR: Color = Color::Rgb(248, 113, 113);

/// 根据焦点状态返回边框样式。
pub fn border_style(active: bool) -> Style {
    if active {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
