//! 基于 [iroh](https://iroh.computer) 的 P2P 隧道实现。
//!
//! ## 名词
//!
//! - **Endpoint**: iroh 节点，负责管理连接和 NAT 打洞
//! - **QUIC 连接**: 两个 Endpoint 之间的加密 P2P 连接
//! - **双向流 (bi-stream)**: 一条 QUIC 连接内可复用的独立数据通道，
//!   类似 TCP 连接但无需额外握手。每条双向流有独立的发送/接收端
//! - **ALPN**: 应用层协议标识，用于区分不同协议的连接
//! - **Relay**: iroh 官方中继服务器，用于 NAT 打洞失败时的回退转发
//!
//! ## 数据流
//!
//! ```text
//! MC客户端 → [本地TCP] → sculk(join端)
//!     ═══ iroh P2P 加密隧道 (QUIC, 自动NAT打洞) ═══
//!                          sculk(host端) → [本地TCP] → MC服务端
//! ```
//!
//! ## 工作方式
//!
//! - **Host**: 启动 Endpoint 等待连接，每收到一条双向流就桥接到本地 MC 服务端
//! - **Client**: 连接 Host，本地开放端口，MC 每次连入就开一条新的双向流转发

use std::sync::{Arc, Mutex};

use iroh::endpoint::{Connection, ConnectionInfo, RecvStream, SendStream};
use iroh::{Endpoint, EndpointId, SecretKey, Watcher};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use super::event::{ConnectionSnapshot, TunnelEvent};

/// sculk 隧道协议标识
const ALPN: &[u8] = b"/sculk/tunnel/1";

/// 事件通道缓冲区大小
const EVENT_CHANNEL_SIZE: usize = 64;

/// 基于 iroh 的 P2P 隧道
pub struct IrohTunnel {
    endpoint: Endpoint,
    /// 活跃连接列表（ConnectionInfo 是弱引用，不阻止连接释放）
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
}

impl IrohTunnel {
    /// 房主: 创建隧道，返回连接票据和事件接收端。
    ///
    /// 票据为 EndpointId 字符串，玩家可通过 n0 DNS 发现房主地址。
    /// 传入 `secret_key` 可使 ticket 跨重启保持稳定；传 `None` 则自动生成新密钥。
    pub async fn host(
        mc_port: u16,
        secret_key: Option<SecretKey>,
    ) -> anyhow::Result<(Self, String, mpsc::Receiver<TunnelEvent>)> {
        let mut builder = Endpoint::builder().alpns(vec![ALPN.to_vec()]);
        if let Some(key) = secret_key {
            builder = builder.secret_key(key);
        }
        let endpoint = builder.bind().await?;

        // 等待连上 Relay，确保地址可被发现
        endpoint.online().await;

        let ticket = endpoint.id().to_string();
        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<ConnectionInfo>>> = Arc::new(Mutex::new(Vec::new()));

        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        tokio::spawn(async move {
            if let Err(e) = host_accept_loop(ep, mc_port, tx.clone(), conns_clone).await {
                let _ = tx
                    .send(TunnelEvent::Error {
                        message: format!("host loop ended: {e}"),
                    })
                    .await;
            }
        });

        Ok((Self { endpoint, conns }, ticket, rx))
    }

    /// 玩家: 通过票据连接房主，返回事件接收端。
    pub async fn join(
        ticket: &str,
        local_port: u16,
    ) -> anyhow::Result<(Self, mpsc::Receiver<TunnelEvent>)> {
        let endpoint_id: EndpointId = ticket
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid ticket: {e}"))?;

        let endpoint = Endpoint::builder().bind().await?;

        tracing::info!("connecting to host...");
        let conn = endpoint.connect(endpoint_id, ALPN).await?;
        tracing::info!("connected to host");

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<ConnectionInfo>>> = Arc::new(Mutex::new(Vec::new()));

        // 保存连接信息
        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());

        let _ = tx.send(TunnelEvent::Connected).await;

        // 监控路径变化
        let remote_id = conn.remote_id().fmt_short().to_string();
        spawn_path_monitor(conn.clone(), remote_id.clone(), tx.clone());

        // 监控断开
        let tx_dc = tx.clone();
        tokio::spawn(async move {
            if let Some((err, _stats)) = conn_info.closed().await {
                let _ = tx_dc
                    .send(TunnelEvent::Disconnected {
                        reason: err.to_string(),
                    })
                    .await;
            }
        });

        let listener = TcpListener::bind(("127.0.0.1", local_port)).await?;
        tracing::info!(local_port, "listening for MC clients");

        tokio::spawn(async move {
            if let Err(e) = join_accept_loop(conn, listener).await {
                let _ = tx
                    .send(TunnelEvent::Error {
                        message: format!("join loop ended: {e}"),
                    })
                    .await;
            }
        });

        Ok((Self { endpoint, conns }, rx))
    }

    /// 获取连接信息快照
    ///
    /// host 端返回所有玩家，join 端返回房主信息。
    /// 已断开的连接会被自动清理。
    pub fn connections(&self) -> Vec<ConnectionSnapshot> {
        let mut guard = self.conns.lock().unwrap();
        // 清理已断开的连接
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
                }
            })
            .collect()
    }

    /// 获取本机 EndpointId
    pub fn local_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// 关闭隧道
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// Host: 持续接受 QUIC 连接，发送事件并管理连接列表
async fn host_accept_loop(
    endpoint: Endpoint,
    mc_port: u16,
    tx: mpsc::Sender<TunnelEvent>,
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
) -> anyhow::Result<()> {
    loop {
        let conn = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?
            .await?;

        let remote_id = conn.remote_id().fmt_short().to_string();
        tracing::info!(remote = %remote_id, "player connected");

        // 保存连接信息
        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());

        let _ = tx
            .send(TunnelEvent::PlayerJoined {
                id: remote_id.clone(),
            })
            .await;

        // 监控路径变化
        spawn_path_monitor(conn.clone(), remote_id.clone(), tx.clone());

        // 监控断开
        let tx_left = tx.clone();
        let left_id = remote_id.clone();
        tokio::spawn(async move {
            if let Some((err, _stats)) = conn_info.closed().await {
                let _ = tx_left
                    .send(TunnelEvent::PlayerLeft {
                        id: left_id,
                        reason: err.to_string(),
                    })
                    .await;
            }
        });

        tokio::spawn(async move {
            if let Err(e) = host_handle_conn(conn, mc_port).await {
                tracing::debug!("connection ended: {e}");
            }
        });
    }
}

/// Host: 处理单个连接，每条双向流桥接到本地 MC
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

/// Client: 接受本地 MC 连接，每个开一条双向流
async fn join_accept_loop(conn: Connection, listener: TcpListener) -> anyhow::Result<()> {
    loop {
        let (tcp, peer) = listener.accept().await?;
        tracing::info!(%peer, "MC client connected");

        let conn = conn.clone();
        tokio::spawn(async move {
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

/// 监控连接路径变化，发送 PathChanged 事件
fn spawn_path_monitor(conn: Connection, remote_id: String, tx: mpsc::Sender<TunnelEvent>) {
    tokio::spawn(async move {
        let mut watcher = conn.paths();
        loop {
            // 等待路径变化
            if watcher.updated().await.is_err() {
                break; // watcher disconnected（连接已关闭）
            }
            let paths = watcher.get();
            // 找到当前选中的路径
            if let Some(selected) = paths.iter().find(|p| p.is_selected()) {
                let _ = tx
                    .send(TunnelEvent::PathChanged {
                        remote_id: remote_id.clone(),
                        is_relay: selected.is_relay(),
                        rtt_ms: selected.rtt().as_millis() as u64,
                    })
                    .await;
            }
        }
    });
}

/// 双向桥接：双向流 <-> TCP，任一方向断开则关闭
async fn bridge(mut send: SendStream, mut recv: RecvStream, tcp: TcpStream) -> anyhow::Result<()> {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    tokio::select! {
        r = tokio::io::copy(&mut tcp_read, &mut send) => {
            let _ = send.finish();
            r?;
        }
        r = tokio::io::copy(&mut recv, &mut tcp_write) => {
            r?;
        }
    }

    Ok(())
}
