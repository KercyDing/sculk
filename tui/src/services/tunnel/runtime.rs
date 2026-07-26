//! 隧道服务运行时：启动、关闭与事件转发。

use std::time::Duration;

use sculk::tunnel::{HostConfig, HostOptions, JoinConfig, JoinOptions, Ticket, TunnelService};
use tokio::sync::mpsc;

/// 异步启动 host 隧道。
pub fn spawn_host(
    service: TunnelService,
    port: u16,
    secret_key: sculk::SecretKey,
    relay_url: Option<sculk::RelayUrl>,
    password: Option<String>,
    tx: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        let config = HostConfig::new()
            .event_delay(Duration::ZERO)
            .password(password);
        let options = HostOptions::new(port)
            .secret_key(Some(secret_key))
            .relay_url(relay_url)
            .config(config);
        match service.start_host(options).await {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(format!("host 启动失败: {e}"));
            }
        }
    });
}

/// 异步启动 join 隧道。
///
/// 票据解析失败时直接发送错误文本。
pub fn spawn_join(
    service: TunnelService,
    ticket_str: &str,
    port: u16,
    password: Option<String>,
    tx: mpsc::UnboundedSender<String>,
) {
    let ticket_str = ticket_str.trim().to_owned();
    tokio::spawn(async move {
        let ticket: Ticket = match ticket_str.parse() {
            Ok(t) => t,
            Err(e) => {
                let _ = tx.send(format!("票据解析失败: {e}"));
                return;
            }
        };
        let config = JoinConfig::new()
            .event_delay(Duration::ZERO)
            .password(password);
        match service
            .start_join(JoinOptions::new(ticket, port).config(config))
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(format!("join 失败: {e}"));
            }
        }
    });
}

/// 异步关闭隧道。
///
/// 完成状态由 core 快照发布，失败时发送错误文本。
pub fn spawn_close(service: TunnelService, tx: mpsc::UnboundedSender<String>) {
    tokio::spawn(async move {
        match tokio::time::timeout(Duration::from_secs(5), service.shutdown()).await {
            Ok(Ok(())) => {
                // The Idle snapshot is the authoritative completion signal.
            }
            Ok(Err(e)) => {
                let _ = tx.send(format!("关闭隧道失败: {e}"));
            }
            Err(_) => {
                let _ = tx.send("关闭隧道超时 (5s)".to_string());
            }
        }
    });
}
