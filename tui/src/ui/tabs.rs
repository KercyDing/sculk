//! 左侧面板：Tab 选择 + Host/Join/Relay 输入字段。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};

use super::theme::{ACCENT, INFO, LEFT_PANEL_BG, WARN, border_style};
use crate::input::InputField;
use crate::state::{ActiveTab, AppState, FocusPane, HostField, JoinField, RELAYS, TAB_TITLES};

pub fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    // 先铺整块左侧背景，保证空白区域也保持深色。
    frame.render_widget(
        Block::default().style(Style::default().bg(LEFT_PANEL_BG)),
        area,
    );

    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);

    let tabs = Tabs::new(TAB_TITLES)
        .select(state.tab.index())
        .style(Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG))
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .bg(LEFT_PANEL_BG)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" • ")
        .block(
            Block::default()
                .title("模式")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(state.focus == FocusPane::Profile))
                .style(Style::default().bg(LEFT_PANEL_BG)),
        );
    frame.render_widget(tabs, sections[0]);

    let panel_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(state.focus == FocusPane::Profile))
        .style(Style::default().bg(LEFT_PANEL_BG));

    match state.tab {
        ActiveTab::Host => {
            let block = panel_block.title("建房配置");
            let inner = block.inner(sections[1]);
            frame.render_widget(block, sections[1]);
            render_host_fields(frame, inner, state);
        }
        ActiveTab::Join => {
            let block = panel_block.title("加入配置");
            let inner = block.inner(sections[1]);
            frame.render_widget(block, sections[1]);
            render_join_fields(frame, inner, state);
        }
        ActiveTab::Relay => {
            let block = panel_block.title("中继列表");
            let inner = block.inner(sections[1]);
            frame.render_widget(block, sections[1]);
            render_relay_fields(frame, inner, state);
        }
    }
}

fn render_host_fields(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 角色标签
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 端口
        Constraint::Length(1), // 密码
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 提示
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "建房",
                Style::default()
                    .fg(ACCENT)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let focused = state.focus == FocusPane::Profile;
    render_field_line(
        frame,
        rows[2],
        &state.host_port,
        focused && state.host_field == HostField::Port,
    );
    render_field_line(
        frame,
        rows[3],
        &state.host_password,
        focused && state.host_field == HostField::Password,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "i 编辑 | ↑/↓ 切换字段",
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[5],
    );
}

fn render_join_fields(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1), // 票据
        Constraint::Length(1), // 端口
        Constraint::Length(1), // 密码
        Constraint::Length(1),
        Constraint::Length(1), // 提示
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "加入",
                Style::default()
                    .fg(INFO)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let focused = state.focus == FocusPane::Profile;
    render_field_line(
        frame,
        rows[2],
        &state.join_ticket,
        focused && state.join_field == JoinField::Ticket,
    );
    render_field_line(
        frame,
        rows[3],
        &state.join_port,
        focused && state.join_field == JoinField::Port,
    );
    render_field_line(
        frame,
        rows[4],
        &state.join_password,
        focused && state.join_field == JoinField::Password,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "i 编辑 | ↑/↓ 切换字段",
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[6],
    );
}

fn render_relay_fields(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 角色标签
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 中继项 0
        Constraint::Length(1), // 中继项 1
        Constraint::Length(1), // 预留行
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 提示
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "角色: ",
                Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
            ),
            Span::styled(
                "中继",
                Style::default()
                    .fg(Color::Magenta)
                    .bg(LEFT_PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let selected_idx = state.relay_state.selected().unwrap_or(0);
    let focused = state.focus == FocusPane::Profile;

    for (i, relay) in RELAYS.iter().enumerate() {
        let is_selected = focused && i == selected_idx;
        let is_applied = i == state.relay_idx;

        let marker = if is_selected { "▶ " } else { "  " };
        let suffix = if is_applied { " (已应用)" } else { "" };

        let style = if is_selected {
            Style::default().fg(ACCENT).bg(LEFT_PANEL_BG)
        } else if is_applied {
            Style::default().fg(Color::White).bg(LEFT_PANEL_BG)
        } else {
            Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG)
        };

        frame.render_widget(
            Paragraph::new(Span::styled(format!("{marker}{relay}{suffix}"), style)),
            rows[2 + i],
        );
    }

    // 悬停在"自建中继"时显示 URL 预览和编辑提示
    if selected_idx == 1 {
        render_field_line(frame, rows[4], &state.relay_url, false);
    }

    let hint = if selected_idx == 1 {
        "Enter 应用 | ↑/↓ 选择 | i 编辑URL"
    } else {
        "Enter 应用 | ↑/↓ 选择"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray).bg(LEFT_PANEL_BG),
        )),
        rows[6],
    );
}

/// 渲染单行输入字段预览：`label: [value]`。
/// `selected`: Normal 模式下当前字段高亮。长值截断并加省略号。
fn render_field_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    field: &InputField,
    selected: bool,
) {
    let label_width = 8u16;
    let cols =
        Layout::horizontal([Constraint::Length(label_width), Constraint::Min(4)]).split(area);

    let label_style = if selected {
        Style::default().fg(ACCENT).bg(LEFT_PANEL_BG)
    } else {
        Style::default().fg(WARN).bg(LEFT_PANEL_BG)
    };

    let marker = if selected { "▶ " } else { "  " };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{}{}: ", marker, field.label),
            label_style,
        )),
        cols[0],
    );

    let max_w = cols[1].width as usize;
    let chars: Vec<char> = field.value.chars().collect();
    let char_count = chars.len();

    let display = if field.value.is_empty() {
        "(空)".to_string()
    } else if char_count <= max_w {
        field.value.clone()
    } else {
        let mut s: String = chars[..max_w.saturating_sub(1)].iter().collect();
        s.push('…');
        s
    };

    let value_style = if selected {
        Style::default().fg(Color::White).bg(LEFT_PANEL_BG)
    } else {
        Style::default().fg(Color::Gray).bg(LEFT_PANEL_BG)
    };
    frame.render_widget(Paragraph::new(Span::styled(display, value_style)), cols[1]);
}
