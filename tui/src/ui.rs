//! Ratatui 界面渲染。

mod popups;
pub mod tabs;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::model::{FocusPane, FooterTone, InputMode, Model};

pub const BG: Color = Color::Rgb(4, 18, 28);
pub const ACCENT: Color = Color::Rgb(74, 222, 128);
pub const INFO: Color = Color::Rgb(59, 130, 246);
pub const WARN: Color = Color::Rgb(245, 158, 11);
pub const ERROR: Color = Color::Rgb(248, 113, 113);
const FOCUS: Color = Color::Rgb(125, 211, 252);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    Accent,
    Info,
    Warn,
}

impl StatusColor {
    fn color(self) -> Color {
        match self {
            Self::Accent => ACCENT,
            Self::Info => INFO,
            Self::Warn => WARN,
        }
    }
}

pub fn border_style(active: bool) -> Style {
    Style::default().fg(if active { FOCUS } else { Color::DarkGray })
}

pub fn render(frame: &mut ratatui::Frame<'_>, model: &mut Model) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .margin(1)
    .split(area);

    render_header(frame, layout[0], model);
    let main = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(layout[1]);
    tabs::render_left(frame, main[0], model);
    render_logs(frame, main[1], model);
    render_footer(frame, layout[2], model);
    popups::render_help_popup(frame, area, model);
    popups::render_edit_popup(frame, area, model);
    popups::render_confirm_stop_popup(frame, area, model);
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model) {
    let (status_label, status_color) = model.status_label();
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
        Span::styled(
            status_label,
            Style::default()
                .fg(status_color.color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled("连接:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(
            model.tunnel.connections.len().to_string(),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("    "),
        Span::styled("中继:", Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(
            crate::model::RELAYS
                .get(model.relay_idx)
                .copied()
                .unwrap_or("未知"),
            Style::default().fg(Color::Magenta),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(BG)),
        ),
        area,
    );
}

fn render_logs(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);
    frame.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .title("链路质量")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .style(Style::default().bg(BG))
                    .border_style(border_style(false)),
            )
            .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(12, 40, 30)))
            .label(model.gauge_label())
            .percent(model.route_strength() as u16),
        sections[0],
    );

    let block = Block::default()
        .title("会话日志")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(model.focus == FocusPane::Logs))
        .style(Style::default().bg(BG));
    let inner = block.inner(sections[1]);
    frame.render_widget(block, sections[1]);

    let height = inner.height as usize;
    let selected = model.log_state.selected();
    let scroll = selected
        .map(|index| index.saturating_sub(height.saturating_sub(1)))
        .unwrap_or_else(|| model.logs.len().saturating_sub(height));
    let message_width = (inner.width as usize).saturating_sub(9);

    for (row_index, log_index) in (scroll..model.logs.len()).take(height).enumerate() {
        let is_selected = selected == Some(log_index);
        let marker = if is_selected { "▶ " } else { "  " };
        let text = render_log_text(
            &model.logs[log_index],
            message_width,
            is_selected,
            model.tick,
        );
        let style = Style::default().fg(if is_selected { ACCENT } else { Color::Gray });
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{marker}[{:03}] {text}", log_index + 1),
                style,
            )),
            Rect::new(inner.x, inner.y + row_index as u16, inner.width, 1),
        );
    }
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model) {
    let mut spans = Vec::new();
    if model.input_mode == InputMode::Editing {
        push_hint(&mut spans, "编辑模式", "", FooterTone::Info);
        push_hint(&mut spans, "Esc", "退出编辑", FooterTone::Accent);
        push_hint(&mut spans, "↑/↓", "字段", FooterTone::Accent);
    } else {
        push_hint(&mut spans, "Enter", "执行", FooterTone::Accent);
        push_hint(&mut spans, "i", "编辑", FooterTone::Accent);
        push_hint(&mut spans, "←/→", "模式", FooterTone::Accent);
        push_hint(&mut spans, "Tab", "焦点", FooterTone::Accent);
        push_hint(&mut spans, "↑/↓", "字段", FooterTone::Accent);
        push_hint(&mut spans, "h", "帮助", FooterTone::Accent);
        push_hint(
            &mut spans,
            "Esc",
            model.esc_action_label(),
            FooterTone::Error,
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Left)
            .style(Style::default().bg(BG)),
        area,
    );

    if model.esc_can_exit() && model.quit_pressed_at.is_some() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "再次按 Esc 退出",
                Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right)
            .style(Style::default().bg(BG)),
            area,
        );
    }
}

fn push_hint(
    spans: &mut Vec<Span<'static>>,
    key: &'static str,
    label: &'static str,
    tone: FooterTone,
) {
    let style = match tone {
        FooterTone::Accent => Style::default().fg(ACCENT),
        FooterTone::Info => Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        FooterTone::Error => Style::default().fg(ERROR),
    };
    spans.push(Span::styled(key, style));
    if !label.is_empty() {
        spans.push(Span::raw(format!(" {label}")));
    }
    spans.push(Span::raw("  "));
}

pub(crate) fn render_log_text(text: &str, width: usize, selected: bool, tick: u64) -> String {
    if width == 0 || text.is_empty() {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if !selected {
        if width <= 3 {
            return ".".repeat(width);
        }
        let mut compact = take_prefix(text, width - 3);
        compact.push_str("...");
        return compact;
    }

    let chars: Vec<char> = format!("{text}   ").chars().collect();
    let start = (tick as usize) % chars.len();
    take_window(&chars, start, width)
}

fn take_prefix(text: &str, width_max: usize) -> String {
    let mut output = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let Some(ch_width) = UnicodeWidthChar::width(ch) else {
            continue;
        };
        if ch_width > 0 && width + ch_width > width_max {
            break;
        }
        output.push(ch);
        width += ch_width;
        if width >= width_max {
            break;
        }
    }
    output
}

fn take_window(chars: &[char], start: usize, width_max: usize) -> String {
    let mut output = String::new();
    let mut width = 0;
    let mut count = 0;
    let count_max = chars.len().saturating_mul(2).max(width_max);
    while width < width_max && count < count_max {
        let ch = chars[(start + count) % chars.len()];
        count += 1;
        let Some(ch_width) = UnicodeWidthChar::width(ch) else {
            continue;
        };
        if width + ch_width > width_max {
            break;
        }
        output.push(ch);
        width += ch_width;
    }
    output.extend(std::iter::repeat_n(' ', width_max.saturating_sub(width)));
    output
}
