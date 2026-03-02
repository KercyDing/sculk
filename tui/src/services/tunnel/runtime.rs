//! 隧道服务运行时：启动、关闭与事件转发。

use std::sync::Arc;
use std::time::Duration;

use sculk::tunnel::{IrohTunnel, Ticket, TunnelConfig, TunnelEvent};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::events::AppEvent;

/// 异步启动 host 隧道，返回 `JoinHandle` 供外部 abort。
pub fn spawn_host(
    port: u16,
    secret_key: sculk::tunnel::SecretKey,
    relay_url: Option<sculk::tunnel::RelayUrl>,
    password: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let config = TunnelConfig {
            event_delay: Duration::ZERO,
            password,
            ..Default::default()
        };
        match IrohTunnel::host(port, Some(secret_key), relay_url, config).await {
            Ok((tunnel, ticket, events)) => {
                let _ = tx.send(AppEvent::HostStarted {
                    tunnel: Arc::new(tunnel),
                    ticket: ticket.to_string(),
                    events,
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::StartFailed(format!("host 启动失败: {e}")));
            }
        }
    })
}

/// 异步启动 join 隧道，返回 `JoinHandle` 供外部 abort。
///
/// 票据解析失败时直接发送 `StartFailed`，返回已完成的 handle。
pub fn spawn_join(
    ticket_str: &str,
    port: u16,
    password: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> JoinHandle<()> {
    let ticket_str = ticket_str.trim().trim_matches('"').to_owned();
    tokio::spawn(async move {
        let ticket: Ticket = match ticket_str.parse() {
            Ok(t) => t,
            Err(e) => {
                let _ = tx.send(AppEvent::StartFailed(format!("票据解析失败: {e}")));
                return;
            }
        };
        let config = TunnelConfig {
            event_delay: Duration::ZERO,
            password,
            ..Default::default()
        };
        match IrohTunnel::join(&ticket, port, config).await {
            Ok((tunnel, events)) => {
                let _ = tx.send(AppEvent::JoinConnected {
                    tunnel: Arc::new(tunnel),
                    events,
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::StartFailed(format!("join 失败: {e}")));
            }
        }
    })
}

/// 异步关闭隧道。
pub fn spawn_close(tunnel: Arc<IrohTunnel>, tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(5), tunnel.close()).await;
        let _ = tx.send(AppEvent::Closed);
    });
}

/// 转发事件到 `AppEvent` 通道。
pub fn spawn_event_forwarder(
    mut events: mpsc::Receiver<TunnelEvent>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if tx.send(AppEvent::Tunnel(event)).is_err() {
                break;
            }
        }
    })
}
