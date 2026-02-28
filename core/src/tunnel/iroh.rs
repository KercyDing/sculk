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
use std::time::{Duration, Instant};

use iroh::endpoint::{
    ApplicationClose, Connection, ConnectionError, ConnectionInfo, PathInfoList, RecvStream,
    SendStream, VarInt,
};
use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl, SecretKey, Watcher};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::event::{ConnectionSnapshot, TunnelConfig, TunnelEvent};
use super::ticket::Ticket;

/// sculk 隧道协议标识
const ALPN: &[u8] = b"/sculk/tunnel/1";

/// 事件通道缓冲区大小
const EVENT_CHANNEL_SIZE: usize = 64;

/// Auth 协议版本
const AUTH_VERSION: u8 = 0x01;
/// Auth 结果: 通过
const AUTH_OK: u8 = 0x00;
/// Auth 结果: 拒绝
const AUTH_REJECTED: u8 = 0x01;

/// QUIC close code: auth 失败
const CLOSE_AUTH_FAILED: VarInt = VarInt::from_u32(1);
/// QUIC close code: 人数已满
const CLOSE_SERVER_FULL: VarInt = VarInt::from_u32(2);

/// 基于 iroh 的 P2P 隧道
pub struct IrohTunnel {
    endpoint: Endpoint,
    /// 活跃连接列表（ConnectionInfo 是弱引用，不阻止连接释放）
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
}

impl IrohTunnel {
    /// 房主: 创建隧道，返回连接票据和事件接收端。
    ///
    /// 票据为 `sculk://` URL，包含 EndpointId 和可选的 relay 地址。
    /// 传入 `secret_key` 可使 ticket 跨重启保持稳定；传 `None` 则自动生成新密钥。
    /// 传入 `relay_url` 可使用自定义 relay 服务器；传 `None` 则使用默认 n0 节点。
    pub async fn host(
        mc_port: u16,
        secret_key: Option<SecretKey>,
        relay_url: Option<RelayUrl>,
        config: TunnelConfig,
    ) -> anyhow::Result<(Self, Ticket, mpsc::Receiver<TunnelEvent>)> {
        let mut builder = build_endpoint(secret_key, relay_url.as_ref());
        builder = builder.alpns(vec![ALPN.to_vec()]);
        let endpoint = builder.bind().await?;

        // 等待连上 Relay，确保地址可被发现
        endpoint.online().await;

        let ticket = Ticket::new(endpoint.id(), relay_url);
        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let conns: Arc<Mutex<Vec<ConnectionInfo>>> = Arc::new(Mutex::new(Vec::new()));

        let ep = endpoint.clone();
        let conns_clone = conns.clone();
        tokio::spawn(async move {
            if let Err(e) = host_accept_loop(
                ep,
                mc_port,
                tx.clone(),
                conns_clone,
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

    /// 玩家: 通过票据连接房主，返回事件接收端。
    ///
    /// 支持自动重连（由 `config.max_retries` 控制）。
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

        // 首次连接（含重试 + auth）
        let conn = connect_with_retry(&endpoint, ticket.endpoint_id, &config, &tx).await?;

        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());
        let _ = tx.send(TunnelEvent::Connected).await;

        let listener = Arc::new(TcpListener::bind(("127.0.0.1", local_port)).await?);
        tracing::info!(local_port, "listening for MC clients");

        // 启动重连 supervisor
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
                    timestamp: Instant::now(),
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

// ── Auth 握手 ──────────────────────────────────────────────

/// Join 端发送密码，等待 Host 验证结果
async fn auth_send(conn: &Connection, password: &str) -> anyhow::Result<()> {
    let (mut send, mut recv) = conn.open_bi().await?;

    // 发送: [version][password]
    let mut buf = Vec::with_capacity(1 + password.len());
    buf.push(AUTH_VERSION);
    buf.extend_from_slice(password.as_bytes());
    send.write_all(&buf).await?;
    send.finish()?;

    // 读取结果
    let result = recv
        .read_to_end(1)
        .await
        .map_err(|e| anyhow::anyhow!("auth read failed: {e}"))?;

    if result.first() == Some(&AUTH_OK) {
        Ok(())
    } else {
        anyhow::bail!("auth rejected by host")
    }
}

/// Host 端验证密码，回写结果。返回 true 表示通过
async fn auth_verify(conn: &Connection, expected: &str) -> anyhow::Result<bool> {
    let (mut send, mut recv) = conn.accept_bi().await?;

    // 读取: [version][password]
    let data = recv
        .read_to_end(1 + expected.len() + 256) // 留足余量
        .await
        .map_err(|e| anyhow::anyhow!("auth read failed: {e}"))?;

    if data.is_empty() {
        send.write_all(&[AUTH_REJECTED]).await?;
        send.finish()?;
        return Ok(false);
    }

    let version = data[0];
    if version != AUTH_VERSION {
        send.write_all(&[AUTH_REJECTED]).await?;
        send.finish()?;
        return Ok(false);
    }

    let password = &data[1..];
    let ok = password == expected.as_bytes();

    send.write_all(&[if ok { AUTH_OK } else { AUTH_REJECTED }])
        .await?;
    send.finish()?;

    Ok(ok)
}

// ── Host ───────────────────────────────────────────────────

/// Host: 持续接受 QUIC 连接，发送事件并管理连接列表
async fn host_accept_loop(
    endpoint: Endpoint,
    mc_port: u16,
    tx: mpsc::Sender<TunnelEvent>,
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
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

        let remote_id = conn.remote_id().fmt_short().to_string();
        tracing::info!(remote = %remote_id, "player connected");

        // 检查人数上限（在 auth 之前，避免满员时浪费握手开销）
        if let Some(max) = max_players {
            let active = {
                let mut g = conns.lock().unwrap();
                g.retain(|c| c.is_alive());
                g.len() as u32
            };
            if active >= max {
                tracing::info!(remote = %remote_id, "server full, rejecting");
                let _ = tx
                    .send(TunnelEvent::PlayerRejected {
                        id: remote_id,
                        reason: "server full".into(),
                    })
                    .await;
                conn.close(CLOSE_SERVER_FULL, b"server full");
                continue;
            }
        }

        // 密码验证
        if let Some(ref pwd) = password {
            match auth_verify(&conn, pwd).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::info!(remote = %remote_id, "auth failed");
                    let _ = tx
                        .send(TunnelEvent::AuthFailed {
                            id: remote_id,
                        })
                        .await;
                    conn.close(CLOSE_AUTH_FAILED, b"auth failed");
                    continue;
                }
                Err(e) => {
                    tracing::warn!(remote = %remote_id, "auth error: {e}");
                    let _ = tx
                        .send(TunnelEvent::AuthFailed {
                            id: remote_id,
                        })
                        .await;
                    conn.close(CLOSE_AUTH_FAILED, b"auth failed");
                    continue;
                }
            }
        }

        // 保存连接信息
        let conn_info = conn.to_info();
        conns.lock().unwrap().push(conn_info.clone());

        let _ = tx
            .send(TunnelEvent::PlayerJoined {
                id: remote_id.clone(),
            })
            .await;

        // 监控路径变化
        spawn_path_monitor(conn.clone(), remote_id.clone(), tx.clone(), event_delay);

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

// ── Join ───────────────────────────────────────────────────

/// Join 端重连 supervisor：管理连接生命周期和自动重连
#[allow(clippy::too_many_arguments)]
async fn reconnect_supervisor(
    endpoint: Endpoint,
    endpoint_id: iroh::EndpointId,
    mut conn: Connection,
    mut conn_info: ConnectionInfo,
    listener: Arc<TcpListener>,
    tx: mpsc::Sender<TunnelEvent>,
    conns: Arc<Mutex<Vec<ConnectionInfo>>>,
    config: TunnelConfig,
) {
    loop {
        // 启动 path monitor + accept loop
        let remote_id = conn.remote_id().fmt_short().to_string();
        spawn_path_monitor(conn.clone(), remote_id, tx.clone(), config.event_delay);
        let accept_handle = spawn_join_accept_loop(conn.clone(), listener.clone(), tx.clone());

        // 等待断开
        let permanent_reject = if let Some((err, _stats)) = conn_info.closed().await {
            let rejected = is_permanent_rejection(&err);
            let _ = tx
                .send(TunnelEvent::Disconnected {
                    reason: err.to_string(),
                })
                .await;
            rejected
        } else {
            false
        };

        // abort accept loop
        accept_handle.abort();

        // 永久拒绝（auth 失败、人数已满）不重连
        if permanent_reject {
            return;
        }

        // 检查是否需要重连
        if config.max_retries == Some(0) {
            // 不重连
            return;
        }

        let mut attempt: u32 = 0;
        let reconnected = loop {
            attempt += 1;

            // 检查是否超过最大重试次数
            if let Some(max) = config.max_retries
                && attempt > max
            {
                let _ = tx
                    .send(TunnelEvent::Error {
                        message: format!(
                            "max retries ({max}) exceeded, giving up"
                        ),
                    })
                    .await;
                return;
            }

            // 计算指数退避
            let backoff = std::cmp::min(
                config
                    .base_backoff
                    .saturating_mul(2u32.saturating_pow(attempt - 1)),
                config.max_backoff,
            );

            let _ = tx
                .send(TunnelEvent::Reconnecting { attempt })
                .await;

            tracing::info!(attempt, ?backoff, "reconnecting...");
            tokio::time::sleep(backoff).await;

            // 尝试重连
            match endpoint.connect(endpoint_id, ALPN).await {
                Ok(new_conn) => {
                    // Auth 握手
                    if let Some(ref password) = config.password
                        && let Err(e) = auth_send(&new_conn, password).await
                    {
                        tracing::warn!(attempt, "reconnect auth failed: {e}");
                        continue;
                    }
                    break new_conn;
                }
                Err(e) => {
                    tracing::warn!(attempt, "reconnect failed: {e}");
                    continue;
                }
            }
        };

        // 重连成功
        conn = reconnected;
        conn_info = conn.to_info();

        // 更新连接列表
        {
            let mut g = conns.lock().unwrap();
            g.retain(|c| c.is_alive());
            g.push(conn_info.clone());
        }

        let _ = tx.send(TunnelEvent::Reconnected).await;
        tracing::info!("reconnected successfully");
        // 回到 loop 顶部，重新启动 path monitor + accept loop
    }
}

/// 启动 join accept loop，返回可用于 abort 的 JoinHandle
fn spawn_join_accept_loop(
    conn: Connection,
    listener: Arc<TcpListener>,
    tx: mpsc::Sender<TunnelEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = join_accept_loop(conn, listener).await {
            let _ = tx
                .send(TunnelEvent::Error {
                    message: format!("join loop ended: {e}"),
                })
                .await;
        }
    })
}

/// 检查连接关闭是否为永久拒绝（不应重连）
fn is_permanent_rejection(err: &ConnectionError) -> bool {
    if let ConnectionError::ApplicationClosed(ApplicationClose { error_code, .. }) = err {
        *error_code == CLOSE_AUTH_FAILED || *error_code == CLOSE_SERVER_FULL
    } else {
        false
    }
}

/// 默认首次连接重试次数
const DEFAULT_INITIAL_RETRIES: u32 = 3;

/// 带重试的连接（含 auth），用于首次连接和重连
async fn connect_with_retry(
    endpoint: &Endpoint,
    endpoint_id: iroh::EndpointId,
    config: &TunnelConfig,
    tx: &mpsc::Sender<TunnelEvent>,
) -> anyhow::Result<Connection> {
    let max = DEFAULT_INITIAL_RETRIES;
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
            let _ = tx.send(TunnelEvent::Reconnecting { attempt }).await;
            tokio::time::sleep(backoff).await;
        } else {
            tracing::info!("connecting to host...");
        }

        match endpoint.connect(endpoint_id, ALPN).await {
            Ok(conn) => {
                // Auth 握手
                if let Some(ref password) = config.password {
                    auth_send(&conn, password).await?;
                }
                tracing::info!("connected to host");
                return Ok(conn);
            }
            Err(e) => {
                tracing::warn!(attempt, "connection failed: {e}");
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap().into())
}

/// Client: 接受本地 MC 连接，每个开一条双向流
async fn join_accept_loop(conn: Connection, listener: Arc<TcpListener>) -> anyhow::Result<()> {
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

// ── 路径监控 ───────────────────────────────────────────────

/// 监控连接路径变化，发送 PathChanged 事件
///
/// 两种模式：
/// - `event_delay == ZERO`（去重模式）：仅在 (is_relay, rtt_ms) 实际变化时发送
/// - `event_delay > 0`（定期模式）：按间隔定期发送，relay ↔ direct 切换立即发送
///
/// 首条事件始终立即发送。
fn spawn_path_monitor(
    conn: Connection,
    remote_id: String,
    tx: mpsc::Sender<TunnelEvent>,
    event_delay: Duration,
) {
    tokio::spawn(async move {
        let mut watcher = conn.paths();
        let mut last_is_relay: Option<bool> = None;
        let mut last_rtt_ms: Option<u64> = None;

        // 始终立即发送初始路径状态
        if let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) {
            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
            last_is_relay = Some(is_relay);
            last_rtt_ms = Some(rtt_ms);
        }

        if event_delay.is_zero() {
            // 去重模式：仅在状态实际变化时发送
            loop {
                if watcher.updated().await.is_err() {
                    break;
                }
                let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) else {
                    continue;
                };
                if last_is_relay != Some(is_relay) || last_rtt_ms != Some(rtt_ms) {
                    send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                    last_is_relay = Some(is_relay);
                    last_rtt_ms = Some(rtt_ms);
                }
            }
        } else {
            // 定期模式：按间隔发送 + relay ↔ direct 立即发送
            let mut timer = tokio::time::interval(event_delay);
            timer.tick().await; // 跳过首次立即触发（初始状态已发送）

            loop {
                tokio::select! {
                    result = watcher.updated() => {
                        if result.is_err() { break; }
                        let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) else {
                            continue;
                        };
                        // relay ↔ direct 切换立即发送
                        if last_is_relay != Some(is_relay) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                            last_is_relay = Some(is_relay);
                            timer.reset();
                        }
                    }
                    _ = timer.tick() => {
                        // 定期发送当前状态
                        if let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                            last_is_relay = Some(is_relay);
                        }
                    }
                }
            }
        }
    });
}

/// 从路径列表中提取当前选中路径的 (is_relay, rtt_ms)
fn extract_selected_path(paths: &PathInfoList) -> Option<(bool, u64)> {
    paths
        .iter()
        .find(|p| p.is_selected())
        .map(|p| (p.is_relay(), p.rtt().as_millis() as u64))
}

async fn send_path_event(
    remote_id: &str,
    is_relay: bool,
    rtt_ms: u64,
    tx: &mpsc::Sender<TunnelEvent>,
) {
    let _ = tx
        .send(TunnelEvent::PathChanged {
            remote_id: remote_id.to_string(),
            is_relay,
            rtt_ms,
        })
        .await;
}

// ── 工具函数 ──────────────────────────────────────────────

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

/// 构建 Endpoint builder，根据参数配置 secret key 和 relay 模式
fn build_endpoint(
    secret_key: Option<SecretKey>,
    relay_url: Option<&RelayUrl>,
) -> iroh::endpoint::Builder {
    let mut builder = Endpoint::builder();
    if let Some(key) = secret_key {
        builder = builder.secret_key(key);
    }
    if let Some(url) = relay_url {
        let relay_map = RelayMap::from(url.clone());
        builder = builder.relay_mode(RelayMode::Custom(relay_map));
    }
    builder
}
