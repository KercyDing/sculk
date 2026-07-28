//! 基于 [iroh](https://iroh.computer) 的 P2P 隧道实现。
//! 对外暴露 [`IrohTunnel`]，内部负责 host/join 的连接与转发流程。

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::{
    ApplicationClose, Connection, ConnectionError, RecvStream, SendStream, VarInt,
    WeakConnectionHandle,
};
use iroh::{Endpoint, EndpointId};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::event::{ConnectionSnapshot, HostConfig, JoinConfig, PeerId, TunnelEvent};
use super::join_uri::JoinUri;
use crate::Result;
use crate::error::TunnelError;
use crate::types::{AccessToken, RelayUrl, SecretKey, ServiceId};

mod auth;
mod endpoint;
mod host;
mod join;
mod monitor;
mod node;
mod session;
mod transport;

use endpoint::build_endpoint;
use host::{HostContext, host_accept_loop};
use join::{JoinContext, connect_with_retry, reconnect_supervisor};
use session::HostSessions;

pub use node::{
    HostedServiceHandle, HostedServiceOptions, HostedServicePhase, HostedServiceStatus,
    HostedServiceStatusSubscription, NodeOptions, SculkNode, SculkNodeError, SculkNodeStatus,
    TokenRefreshPolicy, TokenState,
};

const ALPN: &[u8] = b"/sculk/node/1";
const EVENT_CHANNEL_SIZE: usize = 64;
const CLOSE_AUTH_FAILED: VarInt = VarInt::from_u32(1);
const CLOSE_SERVER_FULL: VarInt = VarInt::from_u32(2);
const CLOSE_REPLACED_BY_RECONNECT: VarInt = VarInt::from_u32(3);
const CLOSE_HOST_STOPPED: VarInt = VarInt::from_u32(4);
const CLOSE_AUTH_FAILED_REASON: &[u8] = b"auth failed";
const REJECT_DRAIN_TIMEOUT: Duration = Duration::from_secs(3);
const FULL_RECHECK_DELAY: Duration = Duration::from_millis(1500);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

fn is_auth_failure_close(err: &ConnectionError) -> bool {
    matches!(
        err,
        ConnectionError::ApplicationClosed(ApplicationClose { error_code, reason })
            if *error_code == CLOSE_AUTH_FAILED && reason.as_ref() == CLOSE_AUTH_FAILED_REASON
    )
}

#[derive(Debug, Clone)]
pub(super) struct TrackedConnection {
    remote_id: EndpointId,
    handle: WeakConnectionHandle,
}

impl TrackedConnection {
    fn new(conn: &Connection) -> Self {
        Self {
            remote_id: conn.remote_id(),
            handle: conn.weak_handle(),
        }
    }

    fn is_alive(&self) -> bool {
        self.handle
            .upgrade()
            .is_some_and(|conn| conn.close_reason().is_none())
    }

    fn snapshot(&self, elapsed: Duration) -> ConnectionSnapshot {
        let remote_id = PeerId::new(self.remote_id.fmt_short().to_string());

        let Some(conn) = self.handle.upgrade() else {
            return ConnectionSnapshot {
                remote_id,
                is_relay: false,
                rtt_ms: 0,
                tx_bytes: 0,
                rx_bytes: 0,
                alive: false,
                elapsed,
            };
        };

        let paths = conn.paths();
        let path = paths.iter().find(|path| path.is_selected());
        let (is_relay, rtt_ms, tx_bytes, rx_bytes) = match path {
            Some(path) => {
                let stats = path.stats();
                (
                    path.is_relay(),
                    path.rtt().as_millis() as u64,
                    stats.udp_tx.bytes,
                    stats.udp_rx.bytes,
                )
            }
            None => (false, 0, 0, 0),
        };

        ConnectionSnapshot {
            remote_id,
            is_relay,
            rtt_ms,
            tx_bytes,
            rx_bytes,
            alive: conn.close_reason().is_none(),
            elapsed,
        }
    }
}

/// 基于 iroh 的 P2P 隧道。
pub struct IrohTunnel {
    endpoint: Endpoint,
    conns: Arc<Mutex<Vec<TrackedConnection>>>,
    /// 隧道创建时刻，用于计算 `ConnectionSnapshot::elapsed`。
    created_at: Instant,
    /// 关闭信号发送端。
    shutdown: tokio::sync::watch::Sender<bool>,
    local_addr: Option<std::net::SocketAddr>,
}

impl IrohTunnel {
    /// 创建单服务 Host 隧道，返回 Join URI 和事件接收端。
    pub async fn host(
        mc_port: u16,
        secret_key: Option<SecretKey>,
        relay_url: Option<RelayUrl>,
        service_id: ServiceId,
        token: AccessToken,
        config: HostConfig,
    ) -> Result<(Self, JoinUri, mpsc::Receiver<TunnelEvent>)> {
        let mut builder = build_endpoint(secret_key, relay_url.as_ref());
        builder = builder.alpns(vec![ALPN.to_vec()]);
        let endpoint = builder
            .bind()
            .await
            .map_err(|e| TunnelError::BindHostEndpoint(e.into()))?;
        endpoint.online().await;

        let join_uri = JoinUri::new(endpoint.id(), service_id, token.clone(), relay_url);
        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<TrackedConnection>>> = Arc::new(Mutex::new(Vec::new()));
        let sessions: Arc<Mutex<HostSessions>> = Arc::new(Mutex::new(HostSessions::default()));

        let (shutdown, shutdown_rx) = watch::channel(false);
        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        let sessions_clone = sessions.clone();
        tokio::spawn(async move {
            let ctx = Arc::new(HostContext {
                conns: conns_clone,
                sessions: sessions_clone,
                event_delay: config.event_delay,
                service_id,
                token,
                max_players: config.max_players,
                status: None,
            });
            let target_addr = SocketAddr::from(([127, 0, 0, 1], mc_port));
            if let Err(e) = host_accept_loop(ep, target_addr, tx.clone(), ctx, shutdown_rx).await {
                emit_event(
                    &tx,
                    TunnelEvent::Error {
                        category: crate::ErrorCategory::Internal,
                        message: format!("host loop ended: {e}"),
                    },
                );
            }
        });

        Ok((
            Self {
                endpoint,
                conns,
                created_at: Instant::now(),
                shutdown,
                local_addr: None,
            },
            join_uri,
            rx,
        ))
    }

    /// 通过 Join URI 加入 host，返回事件接收端。
    pub async fn join(
        join_uri: &JoinUri,
        local_port: u16,
        config: JoinConfig,
    ) -> Result<(Self, mpsc::Receiver<TunnelEvent>)> {
        Self::join_to(
            join_uri,
            iroh::EndpointAddr::from(join_uri.endpoint_id()),
            local_port,
            config,
        )
        .await
    }

    async fn join_to(
        join_uri: &JoinUri,
        endpoint_addr: iroh::EndpointAddr,
        local_port: u16,
        config: JoinConfig,
    ) -> Result<(Self, mpsc::Receiver<TunnelEvent>)> {
        let endpoint = build_endpoint(None, join_uri.relay_url())
            .bind()
            .await
            .map_err(|e| TunnelError::BindJoinEndpoint(e.into()))?;

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<TrackedConnection>>> = Arc::new(Mutex::new(Vec::new()));

        let conn = connect_with_retry(
            &endpoint,
            &endpoint_addr,
            join_uri.service_id(),
            join_uri.token(),
            &config,
            &tx,
        )
        .await?;

        lock_mutex(&conns, "join connections")?.push(TrackedConnection::new(&conn));
        emit_event(&tx, TunnelEvent::Connected);

        let listener = Arc::new(
            TcpListener::bind(("127.0.0.1", local_port))
                .await
                .map_err(|e| TunnelError::BindLocalListener(e.into()))?,
        );
        let local_addr = listener
            .local_addr()
            .map_err(|e| TunnelError::BindLocalListener(e.into()))?;
        tracing::info!(%local_addr, "listening for MC clients");

        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        let service_id = join_uri.service_id();
        let token = join_uri.token().clone();

        let (shutdown, shutdown_rx) = watch::channel(false);
        tokio::spawn(async move {
            let ctx = JoinContext {
                listener,
                conns: conns_clone,
                config,
                service_id,
                token,
                shutdown: shutdown_rx,
            };
            reconnect_supervisor(ep, endpoint_addr, conn, tx, ctx).await;
        });

        Ok((
            Self {
                endpoint,
                conns,
                created_at: Instant::now(),
                shutdown,
                local_addr: Some(local_addr),
            },
            rx,
        ))
    }

    #[cfg(test)]
    async fn join_direct(
        join_uri: &JoinUri,
        endpoint_addr: iroh::EndpointAddr,
        local_port: u16,
        config: JoinConfig,
    ) -> Result<(Self, mpsc::Receiver<TunnelEvent>)> {
        Self::join_to(join_uri, endpoint_addr, local_port, config).await
    }

    /// 返回当前活跃连接快照。
    pub fn connections(&self) -> Result<Vec<ConnectionSnapshot>> {
        let mut guard = lock_mutex(&self.conns, "tunnel connections")?;
        guard.retain(|c| c.is_alive());

        let elapsed = self.created_at.elapsed();
        let snapshots: Vec<ConnectionSnapshot> =
            guard.iter().map(|conn| conn.snapshot(elapsed)).collect();
        Ok(snapshots)
    }

    /// 返回本机 EndpointId。
    pub fn local_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// 返回 Join 本地监听地址；Host 隧道返回 `None`。
    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.local_addr
    }

    /// 关闭隧道。先通知后台任务退出，再关闭 endpoint。
    pub async fn close(&self) {
        let _ = self.shutdown.send(true);
        self.endpoint.close().await;
    }
}

impl Drop for IrohTunnel {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if self.endpoint.is_closed() {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let endpoint = self.endpoint.clone();
            runtime.spawn(async move {
                endpoint.close().await;
            });
        }
    }
}

pub(super) fn lock_mutex<'a, T>(
    mutex: &'a Arc<Mutex<T>>,
    name: &'static str,
) -> Result<std::sync::MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| TunnelError::mutex_poisoned(name).into())
}

/// 尽力投递运行时事件；慢消费者不得阻塞网络控制流程。
pub(super) fn emit_event(tx: &mpsc::Sender<TunnelEvent>, event: TunnelEvent) {
    match tx.try_send(event) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(event)) => {
            tracing::debug!(?event, "event channel full, dropping event");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

/// value 变为 true 或 sender 被丢弃时返回。
pub(super) async fn wait_for_shutdown(rx: &mut watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_event_when_full() {
        let (tx, mut rx) = mpsc::channel(1);
        emit_event(&tx, TunnelEvent::Connected);
        emit_event(&tx, TunnelEvent::Reconnected);

        assert!(matches!(rx.try_recv(), Ok(TunnelEvent::Connected)));
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn waits_for_shutdown() {
        let (tx, mut rx) = watch::channel(false);
        let sent = tx.send(true);
        assert!(sent.is_ok(), "shutdown send failed");

        wait_for_shutdown(&mut rx).await;
        assert!(*rx.borrow());
    }

    #[tokio::test]
    async fn drop_closes_endpoint() {
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0DisableRelay)
            .clear_address_lookup()
            .bind()
            .await;
        assert!(endpoint.is_ok(), "test endpoint bind failed: {endpoint:?}");
        let endpoint = if let Ok(endpoint) = endpoint {
            endpoint
        } else {
            return;
        };
        let observer = endpoint.clone();
        let (shutdown, _) = watch::channel(false);
        let tunnel = IrohTunnel {
            endpoint,
            conns: Arc::new(Mutex::new(Vec::new())),
            created_at: Instant::now(),
            shutdown,
            local_addr: None,
        };

        drop(tunnel);
        let closed = tokio::time::timeout(Duration::from_secs(2), observer.closed()).await;
        assert!(closed.is_ok(), "endpoint did not close after tunnel drop");
    }
}
