//! Footer 状态契约：由状态层生成，UI 仅渲染。

use std::borrow::Cow;

use crate::state::{AppState, InputMode};

/// Footer 颜色语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterTone {
    Accent,
    Info,
    Error,
}

/// Footer 键位项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterItem {
    pub key: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub tone: FooterTone,
}

/// Footer 描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterSpec {
    pub left: Vec<FooterItem>,
    pub right_hint: Option<Cow<'static, str>>,
}

impl FooterItem {
    pub(crate) fn new(key: &'static str, label: &'static str, tone: FooterTone) -> Self {
        Self {
            key: Cow::Borrowed(key),
            label: Cow::Borrowed(label),
            tone,
        }
    }
}

/// 构建当前状态对应的 FooterSpec。
pub(crate) fn footer_spec(state: &AppState) -> FooterSpec {
    if state.input_mode == InputMode::Editing {
        return FooterSpec {
            left: vec![
                FooterItem::new("编辑模式", "", FooterTone::Info),
                FooterItem::new("Esc", "退出编辑", FooterTone::Accent),
                FooterItem::new("Tab", "下个字段", FooterTone::Accent),
            ],
            right_hint: None,
        };
    }

    FooterSpec {
        left: vec![
            FooterItem::new("Enter", "执行", FooterTone::Accent),
            FooterItem::new("i", "编辑", FooterTone::Accent),
            FooterItem::new("←/→", "模式", FooterTone::Accent),
            FooterItem::new("Tab", "焦点", FooterTone::Accent),
            FooterItem::new("↑/↓", "字段", FooterTone::Accent),
            FooterItem::new("h", "帮助", FooterTone::Accent),
            FooterItem {
                key: Cow::Borrowed("Esc"),
                label: Cow::Borrowed(state.esc_action_label()),
                tone: FooterTone::Error,
            },
        ],
        right_hint: if state.esc_can_exit() && state.quit_pressed_at.is_some() {
            Some(Cow::Borrowed("再次按 Esc 退出"))
        } else {
            None
        },
    }
}
