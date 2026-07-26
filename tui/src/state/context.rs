//! 状态机运行所需的内部上下文。

use sculk::persist::Profile;
use sculk::tunnel::TunnelService;
use tokio::sync::mpsc;

/// 应用内部上下文，封装非 UI 直接渲染字段。
pub(crate) struct AppContext {
    pub(crate) app_tx: mpsc::UnboundedSender<String>,
    pub(crate) profile: Profile,
    pub(crate) tunnel: TunnelService,
}

impl AppContext {
    /// 构建上下文。
    ///
    /// Purpose: 初始化状态机运行需要的通道、配置与异步句柄容器。
    /// Args: `app_tx` 为应用事件发送端；`profile` 为持久化配置快照。
    /// Returns: 初始化后的上下文实例。
    /// Edge Cases: 句柄字段初始为 `None`，由运行时按需填充。
    pub(crate) fn new(app_tx: mpsc::UnboundedSender<String>, profile: Profile) -> Self {
        Self {
            app_tx,
            profile,
            tunnel: TunnelService::new(),
        }
    }
}
