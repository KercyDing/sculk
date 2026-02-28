//! 左侧面板：Tab 选择 + Host/Join/Relay 输入字段。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};

use super::theme::{ACCENT, INFO, PANEL_ALT, WARN, border_style};
use crate::input::InputField;
use crate::state::{
    ActiveTab, AppState, FocusPane, HostField, InputMode, JoinField, RELAYS, TAB_TITLES,
};

pub fn render_left(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut AppState) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(8)]).split(area);

    let tabs = Tabs::new(TAB_TITLES)
        .select(state.tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(" • ")
        .block(
            Block::default()
                .title("模式")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(state.focus == FocusPane::Profile))
                .style(Style::default().bg(PANEL_ALT)),
        );
    frame.render_widget(tabs, sections[0]);

    let panel_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(state.focus == FocusPane::Profile))
        .style(Style::default().bg(PANEL_ALT));

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
            Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "建房",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let editing = state.input_mode == InputMode::Editing;
    let focused = state.focus == FocusPane::Profile;
    render_field_line(
        frame,
        rows[2],
        &state.host_port,
        focused && state.host_field == HostField::Port,
        editing && state.host_field == HostField::Port,
    );
    render_field_line(
        frame,
        rows[3],
        &state.host_password,
        focused && state.host_field == HostField::Password,
        editing && state.host_field == HostField::Password,
    );

    let hint = if editing {
        "q 退出编辑 | Tab 切换字段"
    } else {
        "e 编辑 | ↑/↓ 切换字段"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray))),
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
            Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "加入",
                Style::default().fg(INFO).add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );

    let editing = state.input_mode == InputMode::Editing;
    let focused = state.focus == FocusPane::Profile;
    render_field_line(
        frame,
        rows[2],
        &state.join_ticket,
        focused && state.join_field == JoinField::Ticket,
        editing && state.join_field == JoinField::Ticket,
    );
    render_field_line(
        frame,
        rows[3],
        &state.join_port,
        focused && state.join_field == JoinField::Port,
        editing && state.join_field == JoinField::Port,
    );
    render_field_line(
        frame,
        rows[4],
        &state.join_password,
        focused && state.join_field == JoinField::Password,
        editing && state.join_field == JoinField::Password,
    );

    let hint = if editing {
        "q 退出编辑 | Tab 切换字段"
    } else {
        "e 编辑 | ↑/↓ 切换字段"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray))),
        rows[6],
    );
}

fn render_relay_fields(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 角色标签
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 中继项 0
        Constraint::Length(1), // 中继项 1
        Constraint::Length(1), // 中继项 2
        Constraint::Length(1), // 空行
        Constraint::Length(1), // 提示
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("角色: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "中继",
                Style::default()
                    .fg(Color::Magenta)
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
            Style::default().fg(ACCENT)
        } else if is_applied {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        frame.render_widget(
            Paragraph::new(Span::styled(format!("{marker}{relay}{suffix}"), style)),
            rows[2 + i],
        );
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Enter 应用 | ↑/↓ 选择",
            Style::default().fg(Color::DarkGray),
        )),
        rows[6],
    );
}

/// 渲染单行输入字段：`label: [value]`。
/// `selected`: Normal 模式下当前字段高亮标签。
/// `editing`: Editing 模式下显示光标。
fn render_field_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    field: &InputField,
    selected: bool,
    editing: bool,
) {
    let label_width = 8u16;
    let cols = Layout::horizontal([
        Constraint::Length(label_width),
        Constraint::Min(4),
    ])
    .split(area);

    let label_style = if editing {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(WARN)
    };

    let marker = if selected || editing { "▶ " } else { "  " };
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

    let (display, cursor_offset) = if field.value.is_empty() && !editing {
        ("(空)".to_string(), 0)
    } else if char_count <= max_w {
        (field.value.clone(), field.value[..field.cursor].chars().count())
    } else if editing {
        // 编辑模式：保持光标可见的滑动窗口
        let cursor_char = field.value[..field.cursor].chars().count();
        let start = if cursor_char >= max_w {
            cursor_char - max_w + 1
        } else {
            0
        };
        let end = (start + max_w).min(char_count);
        let s: String = chars[start..end].iter().collect();
        (s, cursor_char - start)
    } else {
        // 普通模式：截断并显示省略号
        let mut s: String = chars[..max_w.saturating_sub(1)].iter().collect();
        s.push('…');
        (s, 0)
    };

    let value_style = if editing {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::UNDERLINED)
    } else if selected {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(display, value_style)),
        cols[1],
    );

    if editing {
        frame.set_cursor_position((cols[1].x + cursor_offset as u16, cols[1].y));
    }
}
