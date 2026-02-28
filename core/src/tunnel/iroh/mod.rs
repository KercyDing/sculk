//! 基于 [iroh](https://iroh.computer) 的 P2P 隧道实现。
//! 对外暴露 [`IrohTunnel`]，内部负责 host/join 的连接与转发流程。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::{
    ApplicationClose, Connection, ConnectionError, ConnectionInfo, PathInfoList, RecvStream,
    SendStream, VarInt,
};
use iroh::{Endpoint, EndpointId, RelayMap, RelayMode, RelayUrl, SecretKey, Watcher};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::event::{ConnectionSnapshot, TunnelConfig, TunnelEvent};
use super::ticket::Ticket;

mod auth;
mod endpoint;
mod host;
mod join;
mod monitor;
mod session;
mod transport;

use endpoint::build_endpoint;
use host::host_accept_loop;
use join::{connect_with_retry, reconnect_supervisor};
use session::HostSessions;

const ALPN: &[u8] = b"/sculk/tunnel/1";
const EVENT_CHANNEL_SIZE: usize = 64;
const CLOSE_AUTH_FAILED: VarInt = VarInt::from_u32(1);
const CLOSE_SERVER_FULL: VarInt = VarInt::from_u32(2);
const CLOSE_REPLACED_BY_RECONNECT: VarInt = VarInt::from_u32(3);
const REJECT_DRAIN_TIMEOUT: Duration = Duration::from_secs(3);
const FULL_RECHECK_DELAY: Duration = Duration::from_millis(1500);

/// 基于 iroh 的 P2P 隧道。
pub struct IrohTunnel {
    endpoint: Endpoint,
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
}

impl IrohTunnel {
    /// 创建 host 隧道，返回票据和事件接收端。
    pub async fn host(
        mc_port: u16,
        secret_key: Option<SecretKey>,
        relay_url: Option<RelayUrl>,
        config: TunnelConfig,
    ) -> anyhow::Result<(Self, Ticket, mpsc::Receiver<TunnelEvent>)> {
        let mut builder = build_endpoint(secret_key, relay_url.as_ref());
        builder = builder.alpns(vec![ALPN.to_vec()]);
        let endpoint = builder.bind().await?;
        endpoint.online().await;

        let ticket = Ticket::new(endpoint.id(), relay_url);
        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<ConnectionInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let sessions: Arc<Mutex<HostSessions>> = Arc::new(Mutex::new(HostSessions::default()));

        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        let sessions_clone = sessions.clone();
        tokio::spawn(async move {
            if let Err(e) = host_accept_loop(
                ep,
                mc_port,
                tx.clone(),
                conns_clone,
                sessions_clone,
                config.event_delay,
                config.password,
                config.max_players,
            )
            .await
            {
                let _ = tx
                    .send(TunnelEvent::Error {
                        message: format!("host loop ended: {e}"),
                    })
                    .await;
            }
        });

        Ok((Self { endpoint, conns }, ticket, rx))
    }

    /// 通过票据加入 host，返回事件接收端。
    pub async fn join(
        ticket: &Ticket,
        local_port: u16,
        config: TunnelConfig,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TunnelEvent>)> {
        let endpoint = build_endpoint(None, ticket.relay_url.as_ref())
            .bind()
            .await?;

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<ConnectionInfo>>> = Arc::new(Mutex::new(Vec::new()));

        let conn = connect_with_retry(&endpoint, ticket.endpoint_id, &config, &tx).await?;

        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());
        let _ = tx.send(TunnelEvent::Connected).await;

        let listener = Arc::new(TcpListener::bind(("127.0.0.1", local_port)).await?);
        tracing::info!(local_port, "listening for MC clients");

        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        let endpoint_id = ticket.endpoint_id;
        tokio::spawn(async move {
            reconnect_supervisor(
                ep,
                endpoint_id,
                conn,
                conn_info,
                listener,
                tx,
                conns_clone,
                config,
            )
            .await;
        });

        Ok((Self { endpoint, conns }, rx))
    }

    /// 返回当前活跃连接快照。
    pub fn connections(&self) -> Vec<ConnectionSnapshot> {
        let mut guard = self.conns.lock().unwrap();
        guard.retain(|c| c.is_alive());

        guard
            .iter()
            .map(|info| {
                let path = info.selected_path();
                let (is_relay, rtt_ms, tx_bytes, rx_bytes) = match &path {
                    Some(p) => {
                        let stats = p.stats();
                        (
                            p.is_relay(),
                            stats.rtt.as_millis() as u64,
                            stats.udp_tx.bytes,
                            stats.udp_rx.bytes,
                        )
                    }
                    None => (false, 0, 0, 0),
                };
                ConnectionSnapshot {
                    remote_id: info.remote_id().fmt_short().to_string(),
                    is_relay,
                    rtt_ms,
                    tx_bytes,
                    rx_bytes,
                    alive: info.is_alive(),
                    timestamp: Instant::now(),
                }
            })
            .collect()
    }

    /// 返回本机 EndpointId。
    pub fn local_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// 关闭隧道。
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}
