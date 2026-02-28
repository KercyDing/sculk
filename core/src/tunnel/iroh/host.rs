use super::*;

use super::auth::auth_verify;
use super::monitor::spawn_path_monitor;
use super::session::HostSessions;
use super::transport::bridge;

/// Host 侧连接循环。
#[allow(clippy::too_many_arguments)]
pub(super) async fn host_accept_loop(
    endpoint: Endpoint,
    mc_port: u16,
    tx: mpsc::Sender<TunnelEvent>,
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
    sessions: Arc<Mutex<HostSessions>>,
    event_delay: Duration,
    password: Option<String>,
    max_players: Option<u32>,
) -> anyhow::Result<()> {
    loop {
        let conn = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?
            .await?;

        let remote_endpoint_id = conn.remote_id();
        let remote_id = remote_endpoint_id.fmt_short().to_string();
        tracing::info!(remote = %remote_id, "player connected");

        if !capacity_check_with_grace(sessions.clone(), remote_endpoint_id, max_players).await {
            tracing::info!(remote = %remote_id, "server full, rejecting");
            let _ = tx
                .send(TunnelEvent::PlayerRejected {
                    id: remote_id.clone(),
                    reason: "server full".into(),
                })
                .await;
            spawn_rejected_conn_cleanup(conn, CLOSE_SERVER_FULL, b"server full", remote_id);
            continue;
        }

        if let Some(ref pwd) = password {
            match auth_verify(&conn, pwd).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::info!(remote = %remote_id, "auth failed");
                    let _ = tx
                        .send(TunnelEvent::AuthFailed {
                            id: remote_id.clone(),
                        })
                        .await;
                    spawn_rejected_conn_cleanup(conn, CLOSE_AUTH_FAILED, b"auth failed", remote_id);
                    continue;
                }
                Err(e) => {
                    tracing::warn!(remote = %remote_id, "auth error: {e}");
                    let _ = tx
                        .send(TunnelEvent::AuthFailed {
                            id: remote_id.clone(),
                        })
                        .await;
                    spawn_rejected_conn_cleanup(conn, CLOSE_AUTH_FAILED, b"auth failed", remote_id);
                    continue;
                }
            }
        }

        let (generation, is_reconnect, old_conn) = {
            let mut guard = sessions.lock().unwrap();
            guard.upsert(remote_endpoint_id, conn.clone())
        };
        if let Some(old_conn) = old_conn {
            old_conn.close(CLOSE_REPLACED_BY_RECONNECT, b"replaced by reconnect");
        }

        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());

        if is_reconnect {
            tracing::info!(remote = %remote_id, "player reconnected");
        } else {
            let _ = tx
                .send(TunnelEvent::PlayerJoined {
                    id: remote_id.clone(),
                })
                .await;
        }

        spawn_path_monitor(conn.clone(), remote_id.clone(), tx.clone(), event_delay);

        let tx_left = tx.clone();
        let left_id = remote_id.clone();
        let sessions_on_close = sessions.clone();
        tokio::spawn(async move {
            let reason = match conn_info.closed().await {
                Some((err, _stats)) => err.to_string(),
                None => "connection closed".to_string(),
            };
            let should_emit_left = {
                let mut guard = sessions_on_close.lock().unwrap();
                guard.remove_if_current(&remote_endpoint_id, generation)
            };
            if should_emit_left {
                let _ = tx_left
                    .send(TunnelEvent::PlayerLeft {
                        id: left_id,
                        reason,
                    })
                    .await;
            } else {
                tracing::debug!(remote = %left_id, "stale connection closed, ignored");
            }
        });

        tokio::spawn(async move {
            if let Err(e) = host_handle_conn(conn, mc_port).await {
                tracing::debug!("connection ended: {e}");
            }
        });
    }
}

/// 满员时短暂复核，避免重连误拒绝。
async fn capacity_check_with_grace(
    sessions: Arc<Mutex<HostSessions>>,
    incoming_id: EndpointId,
    max_players: Option<u32>,
) -> bool {
    capacity_check_with_grace_delay(sessions, incoming_id, max_players, FULL_RECHECK_DELAY).await
}

#[cfg(test)]
async fn capacity_check_with_grace_delay(
    sessions: Arc<Mutex<HostSessions>>,
    incoming_id: EndpointId,
    max_players: Option<u32>,
    recheck_delay: Duration,
) -> bool {
    capacity_check_with_grace_delay_impl(sessions, incoming_id, max_players, recheck_delay).await
}

#[cfg(not(test))]
async fn capacity_check_with_grace_delay(
    sessions: Arc<Mutex<HostSessions>>,
    incoming_id: EndpointId,
    max_players: Option<u32>,
    recheck_delay: Duration,
) -> bool {
    capacity_check_with_grace_delay_impl(sessions, incoming_id, max_players, recheck_delay).await
}

async fn capacity_check_with_grace_delay_impl(
    sessions: Arc<Mutex<HostSessions>>,
    incoming_id: EndpointId,
    max_players: Option<u32>,
    recheck_delay: Duration,
) -> bool {
    let Some(max) = max_players else {
        return true;
    };

    let has_capacity_or_reconnect = |guard: &HostSessions| {
        guard.contains(&incoming_id) || (guard.active_players() as u32) < max
    };

    {
        let guard = sessions.lock().unwrap();
        if has_capacity_or_reconnect(&guard) {
            return true;
        }
    }

    tokio::time::sleep(recheck_delay).await;

    let guard = sessions.lock().unwrap();
    has_capacity_or_reconnect(&guard)
}

/// 拒绝连接后异步 close 并等待 closed() 收敛。
fn spawn_rejected_conn_cleanup(
    conn: Connection,
    code: VarInt,
    reason: &'static [u8],
    remote_id: String,
) {
    tokio::spawn(async move {
        let info = conn.to_info();
        conn.close(code, reason);
        let _ = tokio::time::timeout(REJECT_DRAIN_TIMEOUT, info.closed()).await;
        tracing::debug!(remote = %remote_id, "rejected connection cleanup finished");
    });
}

/// 处理单个连接内的双向流转发。
async fn host_handle_conn(conn: Connection, mc_port: u16) -> anyhow::Result<()> {
    loop {
        let (send, recv) = conn.accept_bi().await?;

        tokio::spawn(async move {
            let tcp = match TcpStream::connect(("127.0.0.1", mc_port)).await {
                Ok(tcp) => tcp,
                Err(e) => {
                    tracing::error!(mc_port, "failed to connect MC server: {e}");
                    return;
                }
            };

            if let Err(e) = bridge(send, recv, tcp).await {
                tracing::debug!("stream closed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_endpoint_id() -> EndpointId {
        SecretKey::generate(&mut rand::rng()).public().into()
    }

    #[tokio::test]
    async fn capacity_allows_reconnect_when_full() {
        let endpoint_id = test_endpoint_id();
        let sessions = Arc::new(Mutex::new(HostSessions::default()));
        sessions.lock().unwrap().insert_for_test(endpoint_id, 1);

        let allowed =
            capacity_check_with_grace_delay(sessions, endpoint_id, Some(1), Duration::ZERO).await;
        assert!(allowed);
    }

    #[tokio::test]
    async fn capacity_rejects_new_player_when_full() {
        let existing = test_endpoint_id();
        let incoming = test_endpoint_id();
        let sessions = Arc::new(Mutex::new(HostSessions::default()));
        sessions.lock().unwrap().insert_for_test(existing, 1);

        let allowed =
            capacity_check_with_grace_delay(sessions, incoming, Some(1), Duration::ZERO).await;
        assert!(!allowed);
    }
}
