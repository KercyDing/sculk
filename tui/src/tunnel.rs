//! 封装 IrohTunnel 异步操作，通过 mpsc 回传事件。

use std::sync::Arc;
use std::time::Duration;

use sculk_core::tunnel::{IrohTunnel, Ticket, TunnelConfig, TunnelEvent};
use tokio::sync::mpsc;

/// TUI 内部事件。
pub enum AppEvent {
    /// 隧道事件
    Tunnel(TunnelEvent),
    /// Host 启动成功
    HostStarted {
        tunnel: Arc<IrohTunnel>,
        ticket: String,
        events: mpsc::Receiver<TunnelEvent>,
    },
    /// Join 连接成功
    JoinConnected {
        tunnel: Arc<IrohTunnel>,
        events: mpsc::Receiver<TunnelEvent>,
    },
    /// 启动失败
    StartFailed(String),
    /// 关闭完成
    Closed,
}

/// 异步启动 host 隧道。
pub fn spawn_host(
    port: u16,
    secret_key: sculk_core::tunnel::SecretKey,
    relay_url: Option<sculk_core::tunnel::RelayUrl>,
    password: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
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
    });
}

/// 异步启动 join 隧道。
pub fn spawn_join(
    ticket_str: &str,
    port: u16,
    password: Option<String>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let ticket_str = ticket_str.trim().trim_matches('"');
    let ticket_result: Result<Ticket, _> = ticket_str.parse();
    match ticket_result {
        Ok(ticket) => {
            tokio::spawn(async move {
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
            });
        }
        Err(e) => {
            let _ = tx.send(AppEvent::StartFailed(format!("票据解析失败: {e}")));
        }
    }
}

/// 异步关闭隧道。
pub fn spawn_close(tunnel: Arc<IrohTunnel>, tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        tunnel.close().await;
        let _ = tx.send(AppEvent::Closed);
    });
}

/// 转发事件到 AppEvent 通道。
pub fn spawn_event_forwarder(
    mut events: mpsc::Receiver<TunnelEvent>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if tx.send(AppEvent::Tunnel(event)).is_err() {
                break;
            }
        }
    });
}
