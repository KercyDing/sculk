//! host 侧连接接受循环与玩家会话管理。

use std::net::SocketAddr;

use super::*;

use super::auth::auth_verify;
use super::monitor::spawn_path_monitor;
use super::node::HostedStatus;
use super::session::HostSessions;
use super::transport::{bridge, is_connection_closed};
use crate::types::{AccessToken, ServiceId};

const CONNECTION_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECTION_TASKS_MAX: usize = 128;

/// Host 接受循环的运行时上下文。
pub(super) struct HostContext {
    pub(super) conns: Arc<Mutex<Vec<TrackedConnection>>>,
    pub(super) sessions: Arc<Mutex<HostSessions>>,
    pub(super) event_delay: Duration,
    pub(super) service_id: ServiceId,
    pub(super) token: AccessToken,
    pub(super) max_players: Option<u32>,
    pub(super) status: Option<Arc<HostedStatus>>,
}

/// Host 侧连接循环。
pub(super) async fn host_accept_loop(
    endpoint: Endpoint,
    target_addr: SocketAddr,
    tx: mpsc::Sender<TunnelEvent>,
    ctx: Arc<HostContext>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> crate::Result<()> {
    let slots = Arc::new(tokio::sync::Semaphore::new(CONNECTION_TASKS_MAX));

    loop {
        let accepting = tokio::select! {
            _ = super::wait_for_shutdown(&mut shutdown) => return Ok(()),
            accepting = endpoint.accept() => {
                let Some(accepting) = accepting else {
                    return Ok(());
                };
                accepting
            }
        };
        let permit = tokio::select! {
            _ = super::wait_for_shutdown(&mut shutdown) => return Ok(()),
            permit = slots.clone().acquire_owned() => {
                permit.map_err(|e| {
                    crate::error::TunnelError::AcceptHostConnection(e.into())
                })?
            }
        };
        let tx = tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let conn = match tokio::time::timeout(CONNECTION_ACCEPT_TIMEOUT, accepting).await {
                Ok(Ok(conn)) => conn,
                Ok(Err(e)) => {
                    super::emit_event(
                        &tx,
                        TunnelEvent::Error {
                            category: crate::ErrorCategory::Internal,
                            message: format!("accept host connection failed: {e}"),
                        },
                    );
                    return;
                }
                Err(_) => {
                    super::emit_event(
                        &tx,
                        TunnelEvent::Error {
                            category: crate::ErrorCategory::HostUnreachable,
                            message: "accept host connection timed out".to_string(),
                        },
                    );
                    return;
                }
            };
            if let Err(e) = start_host_connection(conn, target_addr, &tx, &ctx).await {
                super::emit_event(
                    &tx,
                    TunnelEvent::Error {
                        category: crate::ErrorCategory::Internal,
                        message: format!("start host connection failed: {e}"),
                    },
                );
            }
        });
    }
}

async fn start_host_connection(
    conn: Connection,
    target_addr: SocketAddr,
    tx: &mpsc::Sender<TunnelEvent>,
    ctx: &HostContext,
) -> crate::Result<()> {
    match auth_verify(&conn, ctx.service_id, &ctx.token).await {
        Ok(true) => start_authenticated_host_connection(conn, target_addr, tx, ctx).await,
        Ok(false) | Err(_) => {
            let remote_id = PeerId::new(conn.remote_id().fmt_short().to_string());
            tracing::info!(remote = %remote_id, "access token rejected");
            super::emit_event(
                tx,
                TunnelEvent::AuthFailed {
                    id: remote_id.clone(),
                },
            );
            spawn_rejected_conn_cleanup(conn, CLOSE_AUTH_FAILED, b"auth failed", remote_id);
            Ok(())
        }
    }
}

/// 在 Node 已完成服务路由与认证后，继续处理该服务的连接。
pub(super) async fn start_authenticated_host_connection(
    conn: Connection,
    target_addr: SocketAddr,
    tx: &mpsc::Sender<TunnelEvent>,
    ctx: &HostContext,
) -> crate::Result<()> {
    let remote_endpoint_id = conn.remote_id();
    let remote_id = PeerId::new(remote_endpoint_id.fmt_short().to_string());
    tracing::info!(remote = %remote_id, "player connected");

    let Some((generation, is_reconnect, old_conn, connection_count)) = register_session_with_grace(
        ctx.sessions.clone(),
        remote_endpoint_id,
        conn.clone(),
        ctx.max_players,
    )
    .await?
    else {
        tracing::info!(remote = %remote_id, "server full, rejecting");
        super::emit_event(
            tx,
            TunnelEvent::PlayerRejected {
                id: remote_id.clone(),
                reason: "server full".into(),
            },
        );
        spawn_rejected_conn_cleanup(conn, CLOSE_SERVER_FULL, b"server full", remote_id);
        return Ok(());
    };
    if let Some(status) = &ctx.status {
        status.set_connection_count(connection_count);
    }
    if let Some(old_conn) = old_conn {
        old_conn.close(CLOSE_REPLACED_BY_RECONNECT, b"replaced by reconnect");
    }

    let conn_handle = conn.weak_handle();
    super::lock_mutex(&ctx.conns, "host connections")?.push(TrackedConnection::new(&conn));

    if is_reconnect {
        tracing::info!(remote = %remote_id, "player reconnected");
    } else {
        super::emit_event(
            tx,
            TunnelEvent::PlayerJoined {
                id: remote_id.clone(),
            },
        );
    }

    spawn_path_monitor(conn.clone(), remote_id.clone(), tx.clone(), ctx.event_delay);

    let tx_left = tx.clone();
    let left_id = remote_id.clone();
    let sessions_on_close = ctx.sessions.clone();
    let status_on_close = ctx.status.clone();
    tokio::spawn(async move {
        let reason = match conn_handle.closed().await {
            Some(closed) => closed.reason.to_string(),
            None => "connection closed".to_string(),
        };
        let mut lock_error = None;
        let (should_emit_left, connection_count) =
            match super::lock_mutex(&sessions_on_close, "host sessions") {
                Ok(mut guard) => {
                    let removed = guard.remove_if_current(&remote_endpoint_id, generation);
                    (removed, Some(guard.active_players()))
                }
                Err(e) => {
                    lock_error = Some(e);
                    (false, None)
                }
            };
        if let (Some(status), Some(connection_count)) = (status_on_close, connection_count) {
            status.set_connection_count(connection_count);
        }
        if let Some(e) = lock_error {
            super::emit_event(
                &tx_left,
                TunnelEvent::Error {
                    category: crate::ErrorCategory::Internal,
                    message: e.to_string(),
                },
            );
        }
        if should_emit_left {
            super::emit_event(
                &tx_left,
                TunnelEvent::PlayerLeft {
                    id: left_id,
                    reason,
                },
            );
        } else {
            tracing::debug!(remote = %left_id, "stale connection closed, ignored");
        }
    });

    let status = ctx.status.clone();
    let bridge_events = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = host_handle_conn(conn, target_addr, status, bridge_events).await {
            tracing::debug!("connection ended: {e}");
        }
    });
    Ok(())
}

/// 原子检查容量并注册会话；满员时短暂复核一次。
async fn register_session_with_grace(
    sessions: Arc<Mutex<HostSessions>>,
    incoming_id: EndpointId,
    conn: Connection,
    max_players: Option<u32>,
) -> crate::Result<Option<(u64, bool, Option<Connection>, usize)>> {
    {
        let mut guard = super::lock_mutex(&sessions, "host sessions")?;
        if guard.has_capacity_for(&incoming_id, max_players) {
            let (generation, is_reconnect, old_conn) = guard.upsert(incoming_id, conn);
            let connection_count = guard.active_players();
            return Ok(Some((generation, is_reconnect, old_conn, connection_count)));
        }
    }

    tokio::time::sleep(FULL_RECHECK_DELAY).await;

    let mut guard = super::lock_mutex(&sessions, "host sessions")?;
    if !guard.has_capacity_for(&incoming_id, max_players) {
        return Ok(None);
    }
    let (generation, is_reconnect, old_conn) = guard.upsert(incoming_id, conn);
    let connection_count = guard.active_players();
    Ok(Some((generation, is_reconnect, old_conn, connection_count)))
}

/// 拒绝连接后异步 close 并等待收敛。
fn spawn_rejected_conn_cleanup(
    conn: Connection,
    code: VarInt,
    reason: &'static [u8],
    remote_id: PeerId,
) {
    tokio::spawn(async move {
        let handle = conn.weak_handle();
        conn.close(code, reason);
        let _ = tokio::time::timeout(REJECT_DRAIN_TIMEOUT, handle.closed()).await;
        tracing::debug!(remote = %remote_id, "rejected connection cleanup finished");
    });
}

/// 处理单个连接内的双向流转发。
async fn host_handle_conn(
    conn: Connection,
    target_addr: SocketAddr,
    status: Option<Arc<HostedStatus>>,
    events: mpsc::Sender<TunnelEvent>,
) -> crate::Result<()> {
    loop {
        let (send, recv) = conn
            .accept_bi()
            .await
            .map_err(|e| crate::error::TunnelError::AcceptQuicBiStream(e.into()))?;

        let status = status.clone();
        let events = events.clone();
        tokio::spawn(async move {
            let tcp = match TcpStream::connect(target_addr).await {
                Ok(tcp) => tcp,
                Err(e) => {
                    if let Some(status) = &status {
                        status.set_error(crate::ErrorCategory::TargetUnavailable);
                    }
                    super::emit_event(
                        &events,
                        TunnelEvent::Error {
                            category: crate::ErrorCategory::TargetUnavailable,
                            message: format!("connect target server failed: {e}"),
                        },
                    );
                    tracing::error!(%target_addr, "failed to connect target server: {e}");
                    return;
                }
            };
            let _bridge = status.as_ref().map(|status| status.bridge_started());

            if let Err(e) = bridge(send, recv, tcp).await {
                if is_connection_closed(&e) {
                    tracing::debug!("stream closed: {e}");
                    return;
                }
                if let Some(status) = &status {
                    status.set_error(crate::ErrorCategory::Internal);
                }
                super::emit_event(
                    &events,
                    TunnelEvent::Error {
                        category: crate::ErrorCategory::Internal,
                        message: format!("bridge ended with an error: {e}"),
                    },
                );
                tracing::debug!("stream closed: {e}");
            }
        });
    }
}
