//! UI 视图规格（ViewModel）：由状态层生成，UI 层只负责渲染。

use crate::state::{
    ActiveTab, AppState, FocusPane, HostField, InputMode, JoinField, RELAYS, TAB_TITLES,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 头部栏规格。
#[derive(Debug, Clone)]
pub struct HeaderSpec {
    pub status_label: String,
    pub status_color: crate::ui::theme::StatusColor,
    pub connection_label: String,
    pub relay_label: String,
}

/// 链路仪表规格。
#[derive(Debug, Clone)]
pub struct GaugeSpec {
    pub strength: u8,
    pub label: String,
}

/// 日志行规格。
#[derive(Debug, Clone)]
pub struct LogRowSpec {
    pub index: usize,
    pub text: String,
    pub selected: bool,
}

/// 右侧日志面板规格。
#[derive(Debug, Clone)]
pub struct LogsSpec {
    pub gauge: GaugeSpec,
    pub focus_logs: bool,
    pub rows: Vec<LogRowSpec>,
}

/// 字段预览规格。
#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub label: &'static str,
    pub value: String,
    pub selected: bool,
}

/// 中继条目规格。
#[derive(Debug, Clone)]
pub struct RelayOptionSpec {
    pub label: &'static str,
    pub selected: bool,
    pub applied: bool,
}

/// 左侧面板规格。
#[derive(Debug, Clone)]
pub enum PanelSpec {
    Host {
        fields: Vec<FieldSpec>,
        hint: &'static str,
    },
    Join {
        fields: Vec<FieldSpec>,
        hint: &'static str,
    },
    Relay {
        options: Vec<RelayOptionSpec>,
        relay_url: Option<FieldSpec>,
        hint: &'static str,
    },
}

/// Tabs 区域规格。
#[derive(Debug, Clone)]
pub struct TabsSpec {
    pub titles: &'static [&'static str; 3],
    pub selected_tab: usize,
    pub profile_focused: bool,
    pub panel_title: &'static str,
    pub panel: PanelSpec,
}

/// 帮助弹窗的行类型。
#[derive(Debug, Clone, Copy)]
pub enum HelpLineSpec {
    Title(&'static str),
    Empty,
    Shortcut {
        key: &'static str,
        description: &'static str,
    },
    Raw(&'static str),
}

/// 帮助弹窗规格。
#[derive(Debug, Clone)]
pub struct HelpPopupSpec {
    pub show: bool,
    pub lines: Vec<HelpLineSpec>,
}

/// 中止确认弹窗规格。
#[derive(Debug, Clone)]
pub struct ConfirmStopPopupSpec {
    pub show: bool,
    pub prompt: &'static str,
}

/// 编辑字段规格。
#[derive(Debug, Clone)]
pub struct EditFieldSpec {
    pub label: &'static str,
    pub value: String,
    pub cursor: usize,
    pub active: bool,
}

/// 编辑弹窗规格。
#[derive(Debug, Clone)]
pub struct EditPopupSpec {
    pub show: bool,
    pub title: &'static str,
    pub fields: Vec<EditFieldSpec>,
    pub hint: &'static str,
}

/// 构建 Header 规格。
///
/// Purpose: 让 header 渲染仅依赖单一数据对象。
/// Args: `state` 为应用状态。
/// Returns: Header 所需文字与颜色数据。
/// Edge Cases: 状态标签来自生命周期派生函数，保持与既有行为一致。
pub(crate) fn header_spec(state: &AppState) -> HeaderSpec {
    let (status_label, status_color) = state.status_label();
    HeaderSpec {
        status_label: status_label.to_string(),
        status_color,
        connection_label: state.connection_label(),
        relay_label: state.relay_label().to_string(),
    }
}

/// 构建 Logs 规格。
///
/// Purpose: 将日志滚动与选中逻辑从 UI 组件中剥离。
/// Args: `state` 为应用状态；`visible_height` 为日志可见行数；`message_width` 为日志正文宽度。
/// Returns: Gauge 与日志行渲染所需数据。
/// Edge Cases: `visible_height=0` 时返回空日志行，避免越界。
pub(crate) fn logs_spec(state: &AppState, visible_height: usize, message_width: usize) -> LogsSpec {
    let gauge = GaugeSpec {
        strength: state.route_strength(),
        label: state.gauge_label(),
    };
    let selected = state.log_state.selected();

    let safe_height = visible_height.max(1);
    let scroll = if let Some(sel) = selected {
        if sel >= safe_height {
            sel - safe_height + 1
        } else {
            0
        }
    } else {
        state.logs.len().saturating_sub(safe_height)
    };

    let mut rows = Vec::new();
    for idx in scroll..state.logs.len() {
        if rows.len() >= visible_height {
            break;
        }
        rows.push(LogRowSpec {
            index: idx,
            text: render_log_text(
                &state.logs[idx],
                message_width,
                selected == Some(idx),
                state.tick,
            ),
            selected: selected == Some(idx),
        });
    }

    LogsSpec {
        gauge,
        focus_logs: state.focus == FocusPane::Logs,
        rows,
    }
}

fn render_log_text(text: &str, width: usize, selected: bool, tick: u64) -> String {
    if width == 0 || text.is_empty() {
        return String::new();
    }

    let text_width = UnicodeWidthStr::width(text);
    if text_width <= width {
        return text.to_string();
    }

    if !selected {
        if width <= 3 {
            return ".".repeat(width);
        }
        let mut compact = take_display_width_prefix(text, width - 3);
        compact.push_str("...");
        return compact;
    }

    let wrapped = format!("{text}   ");
    let chars: Vec<char> = wrapped.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let start = (tick as usize) % chars.len();
    take_display_width_window(&chars, start, width)
}

fn take_display_width_prefix(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let Some(w) = UnicodeWidthChar::width(ch) else {
            continue;
        };
        if w > 0 && used + w > max_width {
            break;
        }
        out.push(ch);
        used += w;
        if used >= max_width {
            break;
        }
    }
    out
}

fn take_display_width_window(chars: &[char], start: usize, width: usize) -> String {
    if chars.is_empty() || width == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut used = 0usize;
    let mut steps = 0usize;
    let step_limit = chars.len().saturating_mul(2).max(width);
    while used < width && steps < step_limit {
        let ch = chars[(start + steps) % chars.len()];
        steps += 1;
        let Some(w) = UnicodeWidthChar::width(ch) else {
            continue;
        };
        if w == 0 {
            out.push(ch);
            continue;
        }
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    while used < width {
        out.push(' ');
        used += 1;
    }
    out
}

/// 构建 Tabs 规格。
///
/// Purpose: 聚合左侧面板展示数据，减少 UI 层状态判断。
/// Args: `state` 为应用状态。
/// Returns: Tabs 与当前面板完整规格。
/// Edge Cases: Relay 仅在“自建中继”选中时返回 URL 预览字段。
pub(crate) fn tabs_spec(state: &AppState) -> TabsSpec {
    let profile_focused = state.focus == FocusPane::Profile;
    let panel = match state.tab {
        ActiveTab::Host => PanelSpec::Host {
            fields: vec![
                FieldSpec {
                    label: state.host_port.label,
                    value: state.host_port.value.clone(),
                    selected: profile_focused && state.host_field == HostField::Port,
                },
                FieldSpec {
                    label: state.host_password.label,
                    value: state.host_password.value.clone(),
                    selected: profile_focused && state.host_field == HostField::Password,
                },
            ],
            hint: "i 编辑 | ↑/↓ 切换字段",
        },
        ActiveTab::Join => PanelSpec::Join {
            fields: vec![
                FieldSpec {
                    label: state.join_ticket.label,
                    value: state.join_ticket.value.clone(),
                    selected: profile_focused && state.join_field == JoinField::Ticket,
                },
                FieldSpec {
                    label: state.join_port.label,
                    value: state.join_port.value.clone(),
                    selected: profile_focused && state.join_field == JoinField::Port,
                },
                FieldSpec {
                    label: state.join_password.label,
                    value: state.join_password.value.clone(),
                    selected: profile_focused && state.join_field == JoinField::Password,
                },
            ],
            hint: "i 编辑 | ↑/↓ 切换字段",
        },
        ActiveTab::Relay => {
            let selected_idx = state.relay_state.selected().unwrap_or_default();
            let options = RELAYS
                .iter()
                .enumerate()
                .map(|(i, label)| RelayOptionSpec {
                    label,
                    selected: profile_focused && i == selected_idx,
                    applied: i == state.relay_idx,
                })
                .collect();

            let relay_url = if selected_idx == 1 {
                Some(FieldSpec {
                    label: state.relay_url.label,
                    value: state.relay_url.value.clone(),
                    selected: false,
                })
            } else {
                None
            };

            let hint = if selected_idx == 1 {
                "Enter 应用 | ↑/↓ 选择 | i 编辑URL"
            } else {
                "Enter 应用 | ↑/↓ 选择"
            };

            PanelSpec::Relay {
                options,
                relay_url,
                hint,
            }
        }
    };

    let panel_title = match state.tab {
        ActiveTab::Host => "建房配置",
        ActiveTab::Join => "加入配置",
        ActiveTab::Relay => "中继列表",
    };

    TabsSpec {
        titles: &TAB_TITLES,
        selected_tab: state.tab.index(),
        profile_focused,
        panel_title,
        panel,
    }
}

/// 构建帮助弹窗规格。
///
/// Purpose: 把帮助内容常量化并集中在状态层。
/// Args: `state` 为应用状态。
/// Returns: 帮助弹窗可见性与内容行。
/// Edge Cases: 隐藏时仍返回完整内容，UI 可直接复用。
pub(crate) fn help_popup_spec(state: &AppState) -> HelpPopupSpec {
    HelpPopupSpec {
        show: state.show_help,
        lines: vec![
            HelpLineSpec::Title("SCULK TUI 快捷键"),
            HelpLineSpec::Empty,
            HelpLineSpec::Shortcut {
                key: "Enter",
                description: "执行当前模式",
            },
            HelpLineSpec::Shortcut {
                key: "←/→",
                description: "切换模式",
            },
            HelpLineSpec::Shortcut {
                key: "Tab",
                description: "切换焦点",
            },
            HelpLineSpec::Shortcut {
                key: "↑/↓",
                description: "字段/中继/日志",
            },
            HelpLineSpec::Shortcut {
                key: "i",
                description: "进入编辑",
            },
            HelpLineSpec::Shortcut {
                key: "Esc",
                description: "退出编辑",
            },
            HelpLineSpec::Shortcut {
                key: "c",
                description: "清空日志",
            },
            HelpLineSpec::Shortcut {
                key: "h",
                description: "开关帮助",
            },
            HelpLineSpec::Shortcut {
                key: "Esc×2",
                description: "退出程序",
            },
            HelpLineSpec::Empty,
            HelpLineSpec::Raw("建房 Enter 启动/停止隧道，"),
            HelpLineSpec::Raw("票据自动复制到剪贴板。"),
        ],
    }
}

/// 构建中止确认弹窗规格。
///
/// Purpose: 将确认弹窗是否显示及文案由状态层统一提供。
/// Args: `state` 为应用状态。
/// Returns: 中止确认弹窗规格。
/// Edge Cases: 仅 `confirm_stop=true` 时展示。
pub(crate) fn confirm_stop_popup_spec(state: &AppState) -> ConfirmStopPopupSpec {
    ConfirmStopPopupSpec {
        show: state.confirm_stop,
        prompt: "确认停止当前隧道？",
    }
}

/// 构建编辑弹窗规格。
///
/// Purpose: 汇总编辑弹窗标题、字段与光标位置数据。
/// Args: `state` 为应用状态。
/// Returns: 编辑弹窗规格。
/// Edge Cases: 非编辑模式时 `show=false` 且字段为空。
pub(crate) fn edit_popup_spec(state: &AppState) -> EditPopupSpec {
    if state.input_mode != InputMode::Editing {
        return EditPopupSpec {
            show: false,
            title: "",
            fields: Vec::new(),
            hint: "[↑/↓] 切换字段  [Esc] 保存",
        };
    }

    let (title, fields) = match state.tab {
        ActiveTab::Host => (
            "编辑 · 建房配置",
            vec![
                EditFieldSpec {
                    label: "端口",
                    value: state.host_port.value.clone(),
                    cursor: state.host_port.cursor,
                    active: state.host_field == HostField::Port,
                },
                EditFieldSpec {
                    label: "密码",
                    value: state.host_password.value.clone(),
                    cursor: state.host_password.cursor,
                    active: state.host_field == HostField::Password,
                },
            ],
        ),
        ActiveTab::Join => (
            "编辑 · 加入配置",
            vec![
                EditFieldSpec {
                    label: "票据",
                    value: state.join_ticket.value.clone(),
                    cursor: state.join_ticket.cursor,
                    active: state.join_field == JoinField::Ticket,
                },
                EditFieldSpec {
                    label: "端口",
                    value: state.join_port.value.clone(),
                    cursor: state.join_port.cursor,
                    active: state.join_field == JoinField::Port,
                },
                EditFieldSpec {
                    label: "密码",
                    value: state.join_password.value.clone(),
                    cursor: state.join_password.cursor,
                    active: state.join_field == JoinField::Password,
                },
            ],
        ),
        ActiveTab::Relay => (
            "编辑 · 中继 URL",
            vec![EditFieldSpec {
                label: "中继 URL",
                value: state.relay_url.value.clone(),
                cursor: state.relay_url.cursor,
                active: true,
            }],
        ),
    };

    EditPopupSpec {
        show: true,
        title,
        fields,
        hint: "[↑/↓] 切换字段  [Esc] 保存",
    }
}
