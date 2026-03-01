//! 颜色常量与边框样式工具。

use ratatui::style::{Color, Style};

pub const BG: Color = Color::Rgb(12, 15, 26);
pub const LEFT_PANEL_BG: Color = Color::Rgb(4, 18, 28);
pub const PANEL: Color = LEFT_PANEL_BG;
pub const PANEL_ALT: Color = LEFT_PANEL_BG;
pub const ACCENT: Color = Color::Rgb(74, 222, 128);
pub const INFO: Color = Color::Rgb(59, 130, 246);
pub const WARN: Color = Color::Rgb(245, 158, 11);
pub const ERROR: Color = Color::Rgb(248, 113, 113);
pub const FOCUS: Color = Color::Rgb(125, 211, 252);

/// 状态栏颜色标识，由 AppState::status_label() 返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    Accent,
    Info,
    Warn,
}

impl StatusColor {
    pub fn color(self) -> Color {
        match self {
            StatusColor::Accent => ACCENT,
            StatusColor::Info => INFO,
            StatusColor::Warn => WARN,
        }
    }
}

/// 根据焦点状态返回边框样式。
pub fn border_style(active: bool) -> Style {
    if active {
        Style::default().fg(FOCUS)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
