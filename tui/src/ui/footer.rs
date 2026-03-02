//! 底部快捷键提示。

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme::{ACCENT, BG, ERROR, INFO};
use crate::state::{AppState, FooterSpec, FooterTone};

pub fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let spec = state.footer_spec();

    let mut spans = Vec::new();
    for item in &spec.left {
        spans.push(Span::styled(item.key.as_ref(), key_style(item.tone)));
        if !item.label.is_empty() {
            spans.push(Span::raw(format!(" {}", item.label)));
        }
        spans.push(Span::raw("  "));
    }

    let footer = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .style(Style::default().bg(BG));
    frame.render_widget(footer, area);

    if let Some(hint) = spec.right_hint {
        let hint = Paragraph::new(Line::from(vec![Span::styled(
            hint,
            Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(BG));
        frame.render_widget(hint, area);
    }
}

fn key_style(tone: FooterTone) -> Style {
    match tone {
        FooterTone::Accent => Style::default().fg(ACCENT),
        FooterTone::Info => Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        FooterTone::Error => Style::default().fg(ERROR),
    }
}

#[allow(dead_code)]
fn _assert_footer_spec_used(_spec: &FooterSpec) {}
