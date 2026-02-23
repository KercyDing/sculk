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

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointId};

use tokio::net::{TcpListener, TcpStream};

/// sculk 隧道协议标识
const ALPN: &[u8] = b"/sculk/tunnel/1";

/// 基于 iroh 的 P2P 隧道
pub struct IrohTunnel {
    endpoint: Endpoint,
}

impl IrohTunnel {
    /// 房主: 创建隧道，返回连接票据供玩家使用。
    ///
    /// 票据为 EndpointId 字符串，玩家可通过 n0 DNS 发现房主地址。
    pub async fn host(mc_port: u16) -> anyhow::Result<(Self, String)> {
        let endpoint = Endpoint::builder()
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await?;

        // 等待连上 Relay，确保地址可被发现
        endpoint.online().await;

        let ticket = endpoint.id().to_string();

        let ep = endpoint.clone();
        tokio::spawn(async move {
            if let Err(e) = host_accept_loop(ep, mc_port).await {
                tracing::error!("host loop ended: {e}");
            }
        });

        Ok((Self { endpoint }, ticket))
    }

    /// 玩家: 通过票据连接房主，在本地端口监听。
    pub async fn join(ticket: &str, local_port: u16) -> anyhow::Result<Self> {
        let endpoint_id: EndpointId = ticket
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid ticket: {e}"))?;

        let endpoint = Endpoint::builder().bind().await?;

        tracing::info!("connecting to host...");
        let conn = endpoint.connect(endpoint_id, ALPN).await?;
        tracing::info!("connected to host");

        let listener = TcpListener::bind(("127.0.0.1", local_port)).await?;
        tracing::info!(local_port, "listening for MC clients");

        tokio::spawn(async move {
            if let Err(e) = join_accept_loop(conn, listener).await {
                tracing::error!("join loop ended: {e}");
            }
        });

        Ok(Self { endpoint })
    }

    /// 关闭隧道
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// Host: 持续接受 QUIC 连接
async fn host_accept_loop(endpoint: Endpoint, mc_port: u16) -> anyhow::Result<()> {
    loop {
        let conn = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?
            .await?;

        tracing::info!(remote = %conn.remote_id().fmt_short(), "player connected");

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
