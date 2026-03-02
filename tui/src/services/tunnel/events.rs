//! 隧道服务事件定义。

use std::sync::Arc;

use sculk::tunnel::{IrohTunnel, TunnelEvent};
use tokio::sync::mpsc;

/// TUI 内部事件。
pub enum AppEvent {
    /// 隧道事件。
    Tunnel(TunnelEvent),
    /// Host 启动成功。
    HostStarted {
        tunnel: Arc<IrohTunnel>,
        ticket: String,
        events: mpsc::Receiver<TunnelEvent>,
    },
    /// Join 连接成功。
    JoinConnected {
        tunnel: Arc<IrohTunnel>,
        events: mpsc::Receiver<TunnelEvent>,
    },
    /// 启动失败。
    StartFailed(String),
    /// 关闭失败。
    CloseFailed(String),
    /// 关闭完成。
    Closed,
}
