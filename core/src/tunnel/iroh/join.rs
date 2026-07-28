//! join 侧连接、自动重连与本地 TCP 代理。

use super::*;

use super::auth::auth_send;
use super::monitor::spawn_path_monitor;
use super::transport::bridge;
use crate::types::{AccessToken, ServiceId};

const RECONNECT_PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

/// Join 重连 supervisor 的运行时上下文。
pub(super) struct JoinContext {
    pub(super) listener: Arc<TcpListener>,
    pub(super) conns: Arc<Mutex<Vec<TrackedConnection>>>,
    pub(super) config: JoinConfig,
    pub(super) service_id: ServiceId,
    pub(super) token: AccessToken,
    /// 关闭信号：为 true 或 sender 被丢弃时，supervisor 应立即退出。
    pub(super) shutdown: tokio::sync::watch::Receiver<bool>,
}

/// Join 侧重连 supervisor。
pub(super) async fn reconnect_supervisor(
    endpoint: Endpoint,
    endpoint_addr: iroh::EndpointAddr,
    mut conn: Connection,
    tx: mpsc::Sender<TunnelEvent>,
    mut ctx: JoinContext,
) {
    loop {
        let remote_id = PeerId::new(conn.remote_id().fmt_short().to_string());
        spawn_path_monitor(conn.clone(), remote_id, tx.clone(), ctx.config.event_delay);
        let accept_handle = spawn_join_accept_loop(
            conn.clone(),
            ctx.listener.clone(),
            tx.clone(),
            ctx.config.local_sessions_max.get(),
        );
        let conn_handle = conn.weak_handle();

        // 等待连接关闭，或提前收到关闭信号
        let permanent_reject = tokio::select! {
            result = conn_handle.closed() => {
                accept_handle.abort();
                if let Some(closed) = result {
                    let rejected = is_permanent_rejection(&closed.reason);
                    super::emit_event(
                        &tx,
                        TunnelEvent::Disconnected {
                            reason: closed.reason.to_string(),
                        },
                    );
                    rejected
                } else {
                    false
                }
            }
            _ = super::wait_for_shutdown(&mut ctx.shutdown) => {
                accept_handle.abort();
                return;
            }
        };

        if permanent_reject {
            return;
        }

        if ctx.config.reconnect_timeout == Some(Duration::ZERO) {
            return;
        }

        let mut progress = tokio::time::interval_at(
            tokio::time::Instant::now() + RECONNECT_PROGRESS_INTERVAL,
            RECONNECT_PROGRESS_INTERVAL,
        );
        progress.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let reconnect_timeout = async {
            match ctx.config.reconnect_timeout {
                Some(timeout) => tokio::time::sleep(timeout).await,
                None => std::future::pending().await,
            }
        };
        tokio::pin!(reconnect_timeout);
        let mut attempt: u32 = 1;
        super::emit_event(&tx, TunnelEvent::Reconnecting { attempt });
        tracing::info!(attempt, "reconnecting...");

        let reconnected = 'reconnect: loop {
            let reconnect = reconnect_once(
                &endpoint,
                &endpoint_addr,
                ctx.service_id,
                &ctx.token,
                attempt,
            );
            tokio::pin!(reconnect);

            loop {
                tokio::select! {
                    biased;
                    _ = super::wait_for_shutdown(&mut ctx.shutdown) => return,
                    _ = &mut reconnect_timeout => {
                        publish_reconnect_timeout(&tx, ctx.config.reconnect_timeout);
                        return;
                    }
                    outcome = &mut reconnect => match outcome {
                        ReconnectOutcome::Connected(new_conn) => break 'reconnect new_conn,
                        ReconnectOutcome::Rejected(message) => {
                            super::emit_event(
                                &tx,
                                TunnelEvent::Error {
                                    category: crate::ErrorCategory::AuthorizationDenied,
                                    message,
                                },
                            );
                            return;
                        }
                        ReconnectOutcome::Retry => break,
                    },
                    _ = progress.tick() => {
                        advance_reconnect_progress(&mut attempt, &tx);
                    }
                }
            }

            tokio::select! {
                _ = super::wait_for_shutdown(&mut ctx.shutdown) => return,
                _ = &mut reconnect_timeout => {
                    publish_reconnect_timeout(&tx, ctx.config.reconnect_timeout);
                    return;
                }
                _ = progress.tick() => {
                    advance_reconnect_progress(&mut attempt, &tx);
                }
            }
        };

        conn = reconnected;

        let lock_error = {
            match super::lock_mutex(&ctx.conns, "join connections") {
                Ok(mut guard) => {
                    guard.retain(|c| c.is_alive());
                    guard.push(TrackedConnection::new(&conn));
                    None
                }
                Err(e) => Some(e),
            }
        };
        if let Some(e) = lock_error {
            super::emit_event(
                &tx,
                TunnelEvent::Error {
                    category: crate::ErrorCategory::Internal,
                    message: e.to_string(),
                },
            );
            return;
        }

        super::emit_event(&tx, TunnelEvent::Reconnected);
        tracing::info!("reconnected successfully");
    }
}

enum ReconnectOutcome {
    Connected(Connection),
    Retry,
    Rejected(String),
}

async fn reconnect_once(
    endpoint: &Endpoint,
    endpoint_addr: &iroh::EndpointAddr,
    service_id: ServiceId,
    token: &AccessToken,
    attempt: u32,
) -> ReconnectOutcome {
    let new_conn = match endpoint.connect(endpoint_addr.clone(), ALPN).await {
        Ok(new_conn) => new_conn,
        Err(error) => {
            tracing::warn!(attempt, "reconnect failed: {error}");
            return ReconnectOutcome::Retry;
        }
    };
    if let Err(error) = auth_send(&new_conn, service_id, token).await {
        tracing::warn!(attempt, "reconnect auth failed: {error}");
        if is_permanent_auth_error(&error, &new_conn) {
            return ReconnectOutcome::Rejected(format!("reconnect rejected: {error}"));
        }
        return ReconnectOutcome::Retry;
    }
    ReconnectOutcome::Connected(new_conn)
}

fn advance_reconnect_progress(attempt: &mut u32, tx: &mpsc::Sender<TunnelEvent>) {
    *attempt = attempt.saturating_add(1);
    super::emit_event(tx, TunnelEvent::Reconnecting { attempt: *attempt });
    tracing::info!(attempt = *attempt, "reconnect pending...");
}

fn publish_reconnect_timeout(tx: &mpsc::Sender<TunnelEvent>, reconnect_timeout: Option<Duration>) {
    let timeout = reconnect_timeout.unwrap_or_default();
    super::emit_event(
        tx,
        TunnelEvent::Error {
            category: crate::ErrorCategory::HostUnreachable,
            message: format!("reconnect timed out after {} seconds", timeout.as_secs()),
        },
    );
}

/// 启动 join accept loop。
fn spawn_join_accept_loop(
    conn: Connection,
    listener: Arc<TcpListener>,
    tx: mpsc::Sender<TunnelEvent>,
    local_sessions_max: usize,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = join_accept_loop(conn, listener, local_sessions_max).await {
            super::emit_event(
                &tx,
                TunnelEvent::Error {
                    category: crate::ErrorCategory::Internal,
                    message: format!("join loop ended: {e}"),
                },
            );
        }
    })
}

/// 判断是否为不应重连的拒绝类型。
fn is_permanent_rejection(err: &ConnectionError) -> bool {
    if let ConnectionError::ApplicationClosed(ApplicationClose { error_code, .. }) = err {
        is_auth_failure_close(err) || *error_code == CLOSE_SERVER_FULL
    } else {
        false
    }
}

fn is_permanent_auth_error(err: &crate::error::SculkError, conn: &Connection) -> bool {
    is_auth_rejected(err)
        || conn
            .close_reason()
            .as_ref()
            .is_some_and(is_permanent_rejection)
}

fn is_auth_rejected(err: &crate::error::SculkError) -> bool {
    matches!(
        err,
        crate::error::SculkError::Tunnel(crate::error::TunnelError::AuthRejectedByHost)
    )
}

/// 含 auth的重试连接流程。
pub(super) async fn connect_with_retry(
    endpoint: &Endpoint,
    endpoint_addr: &iroh::EndpointAddr,
    service_id: ServiceId,
    token: &AccessToken,
    config: &JoinConfig,
    tx: &mpsc::Sender<TunnelEvent>,
) -> crate::Result<Connection> {
    let max = config.initial_retries;
    let mut last_err = None;

    for attempt in 0..=max {
        if attempt > 0 {
            let backoff = std::cmp::min(
                config
                    .base_backoff
                    .saturating_mul(2u32.saturating_pow(attempt - 1)),
                config.max_backoff,
            );
            tracing::info!(attempt, ?backoff, "retrying initial connection...");
            super::emit_event(tx, TunnelEvent::Reconnecting { attempt });
            tokio::time::sleep(backoff).await;
        } else {
            tracing::info!("connecting to host...");
        }

        match endpoint.connect(endpoint_addr.clone(), ALPN).await {
            Ok(conn) => {
                auth_send(&conn, service_id, token).await?;
                tracing::info!("connected to host");
                return Ok(conn);
            }
            Err(e) => {
                tracing::warn!(attempt, "connection failed: {e}");
                last_err = Some(e);
            }
        }
    }

    if let Some(err) = last_err {
        Err(crate::error::TunnelError::ConnectHostEndpoint(err.into()).into())
    } else {
        Err(crate::error::TunnelError::InitialConnectionExhausted {
            attempts: max.saturating_add(1),
        }
        .into())
    }
}

/// Join 侧本地监听并转发到 QUIC 双向流。
async fn join_accept_loop(
    conn: Connection,
    listener: Arc<TcpListener>,
    local_sessions_max: usize,
) -> crate::Result<()> {
    let slots = Arc::new(tokio::sync::Semaphore::new(local_sessions_max));
    loop {
        let permit = slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| crate::error::TunnelError::AcceptLocalTcpClient(e.into()))?;
        let (tcp, peer) = listener
            .accept()
            .await
            .map_err(|e| crate::error::TunnelError::AcceptLocalTcpClient(e.into()))?;
        tracing::info!(%peer, "MC client connected");

        let conn = conn.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let (send, recv) = match conn.open_bi().await {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::error!("failed to open QUIC stream: {e}");
                    return;
                }
            };

            if let Err(e) = bridge(send, recv, tcp).await {
                tracing::debug!(%peer, "stream closed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn application_close(error_code: VarInt, reason: &'static [u8]) -> ConnectionError {
        ConnectionError::ApplicationClosed(ApplicationClose {
            error_code,
            reason: reason.into(),
        })
    }

    #[test]
    fn service_stop_is_retryable() {
        assert!(!is_permanent_rejection(&application_close(
            CLOSE_HOST_STOPPED,
            b"service stopped",
        )));
        assert!(!is_permanent_rejection(&application_close(
            CLOSE_HOST_STOPPED,
            b"node stopped",
        )));
    }

    #[test]
    fn authentication_and_capacity_rejections_remain_permanent() {
        assert!(is_permanent_rejection(&application_close(
            CLOSE_AUTH_FAILED,
            CLOSE_AUTH_FAILED_REASON,
        )));
        assert!(is_permanent_rejection(&application_close(
            CLOSE_SERVER_FULL,
            b"server full",
        )));
    }

    #[test]
    fn auth_rejection_stops_retry() {
        let err = crate::error::SculkError::Tunnel(crate::error::TunnelError::AuthRejectedByHost);
        assert!(is_auth_rejected(&err));
    }

    #[test]
    fn auth_timeout_can_retry() {
        let err = crate::error::SculkError::Tunnel(crate::error::TunnelError::AuthTimedOut);
        assert!(!is_auth_rejected(&err));
    }
}
