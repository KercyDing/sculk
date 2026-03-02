//! 状态层公共类型与常量。

/// 日志最大保留条数。
pub const LOG_CAP: usize = 200;
/// 顶部标签标题。
pub const TAB_TITLES: [&str; 3] = ["建房", "加入", "中继"];
/// 中继选项标题。
pub const RELAYS: [&str; 2] = ["n0 默认中继", "自建中继"];

/// 当前激活的顶栏标签页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Host,
    Join,
    Relay,
}

/// 当前焦点所在的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Profile,
    Logs,
}

/// 输入模式：Normal 为导航，Editing 为文本输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// 事件循环单步结果。
pub enum Step {
    Continue,
    Exit,
}

/// 隧道生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Idle,
    Starting,
    Active,
    Stopping,
}

/// 兼容历史命名。
pub type TunnelPhase = LifecycleState;

/// Host 标签页中当前聚焦的输入字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostField {
    Port,
    Password,
}

/// Join 标签页中当前聚焦的输入字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinField {
    Ticket,
    Port,
    Password,
}

impl ActiveTab {
    /// 返回标签页对应的数组下标，与 `TAB_TITLES` 对齐。
    pub fn index(self) -> usize {
        match self {
            ActiveTab::Host => 0,
            ActiveTab::Join => 1,
            ActiveTab::Relay => 2,
        }
    }

    /// 向右切换，到末尾时停止。
    pub fn next(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Join,
            ActiveTab::Join => ActiveTab::Relay,
            ActiveTab::Relay => ActiveTab::Relay,
        }
    }

    /// 向左切换，到首位时停止。
    pub fn prev(self) -> Self {
        match self {
            ActiveTab::Host => ActiveTab::Host,
            ActiveTab::Join => ActiveTab::Host,
            ActiveTab::Relay => ActiveTab::Join,
        }
    }
}
