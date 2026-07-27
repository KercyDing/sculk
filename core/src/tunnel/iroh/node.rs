//! 共享 iroh Endpoint 的多服务 Host 实现。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc, watch};

use super::auth::{auth_accept, auth_respond};
use super::endpoint::build_endpoint;
use super::host::{HostContext, start_authenticated_host_connection};
use super::session::HostSessions;
use super::*;
use crate::types::{AccessToken, RelayUrl, SecretKey, ServiceId};

const NODE_UNAUTHENTICATED_CONNECTIONS_MAX: NonZeroUsize = NonZeroUsize::new(128).unwrap();

/// 多服务 Node 的创建参数。
#[derive(Clone, Debug)]
pub struct NodeOptions {
    /// 稳定 Node 密钥；`None` 时创建临时身份。
    pub secret_key: Option<SecretKey>,
    /// Node 级自定义 Relay；所有服务共享此配置。
    pub relay_url: Option<RelayUrl>,
    /// 同时等待控制流认证的连接上限。
    pub unauthenticated_connections_max: NonZeroUsize,
}

impl Default for NodeOptions {
    fn default() -> Self {
        Self {
            secret_key: None,
            relay_url: None,
            unauthenticated_connections_max: NODE_UNAUTHENTICATED_CONNECTIONS_MAX,
        }
    }
}

/// 发布单个 Host 服务的参数。
#[derive(Clone, Debug)]
pub struct HostedServiceOptions {
    /// 稳定的服务路由标识。
    pub service_id: ServiceId,
    /// 仅允许本机 loopback 的目标 Minecraft 地址。
    pub target_addr: SocketAddr,
    /// 当前会话授权令牌。
    pub token: AccessToken,
    /// 自动轮换周期；`None` 表示直到服务停止才失效。
    pub token_refresh: Option<Duration>,
    /// 服务级连接策略。
    pub config: HostConfig,
}

/// Node 或服务操作失败。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SculkNodeError {
    #[error("target address must be a loopback address")]
    TargetNotLoopback,
    #[error("target port must be non-zero")]
    InvalidTargetPort,
    #[error("a service with this ServiceId is already published")]
    DuplicateService,
    #[error("service is not published")]
    ServiceNotFound,
    #[error("token refresh period must be greater than zero")]
    InvalidRefreshPeriod,
    #[error("failed to bind Node endpoint")]
    BindEndpoint(#[source] crate::error::BoxError),
}

/// 单个 iroh Endpoint 上发布多个隔离 Host 服务的 Node。
#[derive(Clone)]
pub struct SculkNode {
    inner: Arc<NodeInner>,
}

struct NodeInner {
    endpoint: Endpoint,
    relay_url: Option<RelayUrl>,
    services: RwLock<HashMap<ServiceId, Arc<HostedService>>>,
    shutdown: watch::Sender<bool>,
}

struct HostedService {
    target_addr: SocketAddr,
    token: RwLock<AccessToken>,
    service_id: ServiceId,
    context: Arc<HostContext>,
    events_tx: mpsc::Sender<TunnelEvent>,
    updates_tx: broadcast::Sender<TunnelEvent>,
    rotation_tx: watch::Sender<u64>,
}

/// 已发布服务的轻量操作句柄。
#[derive(Clone)]
pub struct HostedServiceHandle {
    node: SculkNode,
    service_id: ServiceId,
}

/// Node 的稳定状态快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SculkNodeStatus {
    /// 当前已发布服务数。
    pub service_count: usize,
}

/// 单个已发布服务的稳定状态快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedServiceStatus {
    /// 服务标识。
    pub service_id: ServiceId,
    /// 当前已认证且仍存活的连接数量。
    pub connection_count: usize,
}

impl SculkNode {
    /// 创建 Node 并立即开始接受新连接。
    pub async fn bind(options: NodeOptions) -> std::result::Result<Self, SculkNodeError> {
        let mut builder = build_endpoint(options.secret_key, options.relay_url.as_ref());
        builder = builder.alpns(vec![ALPN.to_vec()]);
        let endpoint = builder
            .bind()
            .await
            .map_err(|error| SculkNodeError::BindEndpoint(error.into()))?;
        endpoint.online().await;

        let (shutdown, shutdown_rx) = watch::channel(false);
        let node = Self {
            inner: Arc::new(NodeInner {
                endpoint: endpoint.clone(),
                relay_url: options.relay_url,
                services: RwLock::new(HashMap::new()),
                shutdown,
            }),
        };
        tokio::spawn(node_accept_loop(
            node.clone(),
            endpoint,
            options.unauthenticated_connections_max.get(),
            shutdown_rx,
        ));
        Ok(node)
    }

    /// 发布服务并返回其操作句柄。
    pub async fn start_service(
        &self,
        options: HostedServiceOptions,
    ) -> std::result::Result<HostedServiceHandle, SculkNodeError> {
        validate_target(options.target_addr)?;
        if options.token_refresh == Some(Duration::ZERO) {
            return Err(SculkNodeError::InvalidRefreshPeriod);
        }
        let (events_tx, mut events_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (updates_tx, _) = broadcast::channel(EVENT_CHANNEL_SIZE);
        let (rotation_tx, rotation_rx) = watch::channel(0_u64);
        let updates_tx_for_task = updates_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                let _ = updates_tx_for_task.send(event);
            }
        });
        let service = Arc::new(HostedService {
            target_addr: options.target_addr,
            token: RwLock::new(options.token.clone()),
            service_id: options.service_id,
            context: Arc::new(HostContext {
                conns: Arc::new(Mutex::new(Vec::new())),
                sessions: Arc::new(Mutex::new(HostSessions::default())),
                event_delay: options.config.event_delay,
                service_id: options.service_id,
                token: options.token,
                max_players: options.config.max_players,
            }),
            events_tx,
            updates_tx,
            rotation_tx,
        });
        let mut services = self.inner.services.write().await;
        if services.contains_key(&options.service_id) {
            return Err(SculkNodeError::DuplicateService);
        }
        services.insert(options.service_id, service);
        if let Some(period) = options.token_refresh {
            tokio::spawn(rotation_loop(
                Arc::downgrade(&self.inner),
                options.service_id,
                period,
                rotation_rx,
            ));
        }
        Ok(HostedServiceHandle {
            node: self.clone(),
            service_id: options.service_id,
        })
    }

    /// 停止一个服务；不会关闭 Node 或影响其他服务。
    pub async fn stop_service(
        &self,
        service_id: ServiceId,
    ) -> std::result::Result<(), SculkNodeError> {
        let service = self
            .inner
            .services
            .write()
            .await
            .remove(&service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let mut conns = service
            .context
            .conns
            .lock()
            .map_err(|_| SculkNodeError::ServiceNotFound)?;
        for tracked in conns.drain(..) {
            if let Some(conn) = tracked.handle.upgrade() {
                conn.close(CLOSE_AUTH_FAILED, b"service stopped");
            }
        }
        Ok(())
    }

    /// 返回当前已发布的服务数量。
    pub async fn service_count(&self) -> usize {
        self.inner.services.read().await.len()
    }

    /// 返回 Node 的稳定状态快照。
    pub async fn status(&self) -> SculkNodeStatus {
        SculkNodeStatus {
            service_count: self.service_count().await,
        }
    }

    /// 为指定服务生成新 Token，并返回对应的新 Join URI。
    ///
    /// 已认证的 QUIC 连接不受影响；后续新连接必须使用返回 URI 中的新 Token。
    pub async fn rotate_token(
        &self,
        service_id: ServiceId,
    ) -> std::result::Result<JoinUri, SculkNodeError> {
        self.rotate_token_inner(service_id, true).await
    }

    async fn rotate_token_inner(
        &self,
        service_id: ServiceId,
        reset_timer: bool,
    ) -> std::result::Result<JoinUri, SculkNodeError> {
        let service = self
            .inner
            .services
            .read()
            .await
            .get(&service_id)
            .cloned()
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let new_token = AccessToken::generate();
        *service.token.write().await = new_token.clone();
        if reset_timer {
            service
                .rotation_tx
                .send_modify(|generation| *generation = generation.wrapping_add(1));
        }
        super::emit_event(&service.events_tx, TunnelEvent::TokenRotated);
        Ok(JoinUri::new(
            self.inner.endpoint.id(),
            service.service_id,
            new_token,
            self.inner.relay_url.clone(),
        ))
    }

    /// 返回 Node 的稳定 EndpointId。
    pub fn endpoint_id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    /// 停止所有服务并关闭 Node Endpoint。
    pub async fn close(&self) {
        let _ = self.inner.shutdown.send(true);
        self.inner.services.write().await.clear();
        self.inner.endpoint.close().await;
    }
}

async fn rotation_loop(
    inner: std::sync::Weak<NodeInner>,
    service_id: ServiceId,
    period: Duration,
    mut reset_rx: watch::Receiver<u64>,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(period) => {
                let Some(inner) = inner.upgrade() else { return };
                let node = SculkNode { inner };
                if node.rotate_token_inner(service_id, false).await.is_err() {
                    return;
                }
            }
            changed = reset_rx.changed() => {
                if changed.is_err() {
                    return;
                }
            }
        }
    }
}

fn validate_target(target_addr: SocketAddr) -> std::result::Result<(), SculkNodeError> {
    if !target_addr.ip().is_loopback() {
        return Err(SculkNodeError::TargetNotLoopback);
    }
    if target_addr.port() == 0 {
        return Err(SculkNodeError::InvalidTargetPort);
    }
    Ok(())
}

impl HostedServiceHandle {
    /// 返回服务标识。
    pub fn service_id(&self) -> ServiceId {
        self.service_id
    }

    /// 返回当前可分享 Join URI。
    pub async fn join_uri(&self) -> std::result::Result<JoinUri, SculkNodeError> {
        let service = self
            .node
            .inner
            .services
            .read()
            .await
            .get(&self.service_id)
            .cloned()
            .ok_or(SculkNodeError::ServiceNotFound)?;
        Ok(JoinUri::new(
            self.node.inner.endpoint.id(),
            service.service_id,
            service.token.read().await.clone(),
            self.node.inner.relay_url.clone(),
        ))
    }

    /// 返回服务的稳定状态快照。
    pub async fn status(&self) -> std::result::Result<HostedServiceStatus, SculkNodeError> {
        let service = self
            .node
            .inner
            .services
            .read()
            .await
            .get(&self.service_id)
            .cloned()
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let connections = service
            .context
            .conns
            .lock()
            .map_err(|_| SculkNodeError::ServiceNotFound)?;
        let connection_count = connections.iter().filter(|conn| conn.is_alive()).count();
        Ok(HostedServiceStatus {
            service_id: service.service_id,
            connection_count,
        })
    }

    /// 独立停止此服务。
    pub async fn stop(&self) -> std::result::Result<(), SculkNodeError> {
        self.node.stop_service(self.service_id).await
    }

    /// 立即轮换服务的访问令牌，并返回新的 Join URI。
    pub async fn rotate_token(&self) -> std::result::Result<JoinUri, SculkNodeError> {
        self.node.rotate_token(self.service_id).await
    }

    /// 订阅此服务的过程事件。
    pub async fn subscribe(
        &self,
    ) -> std::result::Result<broadcast::Receiver<TunnelEvent>, SculkNodeError> {
        self.node
            .inner
            .services
            .read()
            .await
            .get(&self.service_id)
            .map(|service| service.updates_tx.subscribe())
            .ok_or(SculkNodeError::ServiceNotFound)
    }
}

async fn node_accept_loop(
    node: SculkNode,
    endpoint: Endpoint,
    unauthenticated_connections_max: usize,
    mut shutdown: watch::Receiver<bool>,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(unauthenticated_connections_max));
    loop {
        let accepting = tokio::select! {
            _ = super::wait_for_shutdown(&mut shutdown) => return,
            accepting = endpoint.accept() => match accepting { Some(value) => value, None => return },
        };
        let Ok(permit) = slots.clone().acquire_owned().await else {
            return;
        };
        let node = node.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let Ok(Ok(conn)) = tokio::time::timeout(Duration::from_secs(10), accepting).await
            else {
                return;
            };
            let _ = route_connection(&node, conn).await;
        });
    }
}

async fn route_connection(node: &SculkNode, conn: Connection) -> crate::Result<()> {
    let (mut send, request) = match auth_accept(&conn).await {
        Ok(value) => value,
        Err(_) => {
            conn.close(CLOSE_AUTH_FAILED, b"auth failed");
            return Ok(());
        }
    };
    let service = node
        .inner
        .services
        .read()
        .await
        .get(&request.service_id)
        .cloned();
    let valid = match &service {
        Some(service) => {
            let token = service.token.read().await;
            request.token.matches(&token)
        }
        None => false,
    };
    auth_respond(&mut send, valid).await?;
    let Some(service) = service.filter(|_| valid) else {
        conn.close(CLOSE_AUTH_FAILED, b"auth failed");
        return Ok(());
    };
    start_authenticated_host_connection(
        conn,
        service.target_addr.port(),
        &service.events_tx,
        &service.context,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn rejects_non_loopback_targets() {
        let target = "192.168.1.2:25565".parse();
        assert!(target.is_ok());
        let result = validate_target(target.unwrap_or_else(|_| unreachable!()));
        assert!(matches!(result, Err(SculkNodeError::TargetNotLoopback)));
    }

    #[tokio::test]
    async fn stopping_one_service_keeps_node_and_other_service() {
        let node_result = SculkNode::bind(NodeOptions::default()).await;
        assert!(node_result.is_ok());
        let Ok(node) = node_result else {
            return;
        };
        let first_id = ServiceId::generate();
        let second_id = ServiceId::generate();
        let first = node
            .start_service(HostedServiceOptions {
                service_id: first_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(first.is_ok());
        let second = node
            .start_service(HostedServiceOptions {
                service_id: second_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
                token: AccessToken::generate(),
                token_refresh: Some(Duration::from_millis(20)),
                config: HostConfig::default(),
            })
            .await;
        assert!(second.is_ok());
        let Ok(second) = second else {
            return;
        };
        assert_eq!(node.status().await.service_count, 2);
        let second_status = second.status().await;
        assert!(second_status.is_ok());
        assert_eq!(
            second_status.ok().map(|status| status.connection_count),
            Some(0)
        );
        let updates = second.subscribe().await;
        assert!(updates.is_ok());
        let Ok(mut updates) = updates else {
            return;
        };
        let first_uri = second.join_uri().await;
        assert!(first_uri.is_ok());
        let rotated_uri = second.rotate_token().await;
        assert!(rotated_uri.is_ok());
        let (Ok(first_uri), Ok(rotated_uri)) = (first_uri, rotated_uri) else {
            return;
        };
        assert_eq!(first_uri.endpoint_id(), rotated_uri.endpoint_id());
        assert_eq!(first_uri.service_id(), rotated_uri.service_id());
        assert!(!first_uri.token().matches(rotated_uri.token()));
        let rotated = tokio::time::timeout(Duration::from_secs(1), updates.recv()).await;
        assert!(matches!(rotated, Ok(Ok(TunnelEvent::TokenRotated))));
        assert_eq!(node.service_count().await, 2);

        let stopped = node.stop_service(first_id).await;
        assert!(stopped.is_ok());
        assert_eq!(node.service_count().await, 1);
        assert!(second.join_uri().await.is_ok());
        node.close().await;
    }

    #[tokio::test]
    async fn joins_and_forwards_to_the_selected_service() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await;
        assert!(listener.is_ok());
        let Ok(listener) = listener else {
            return;
        };
        let target_addr = listener.local_addr();
        assert!(target_addr.is_ok());
        let Ok(target_addr) = target_addr else {
            return;
        };
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut data = [0_u8; 4];
            if stream.read_exact(&mut data).await.is_ok() {
                let _ = stream.write_all(&data).await;
            }
        });

        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr,
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(service.is_ok());
        let Ok(service) = service else {
            node.close().await;
            return;
        };
        let uri = service.join_uri().await;
        assert!(uri.is_ok());
        let Ok(uri) = uri else {
            node.close().await;
            return;
        };
        let join = IrohTunnel::join(&uri, 0, JoinConfig::default()).await;
        assert!(join.is_ok());
        let Ok((join, _)) = join else {
            node.close().await;
            return;
        };
        let local_addr = join.local_addr();
        assert!(local_addr.is_some());
        let Some(local_addr) = local_addr else {
            join.close().await;
            node.close().await;
            return;
        };
        let client = tokio::net::TcpStream::connect(local_addr).await;
        assert!(client.is_ok());
        let Ok(mut client) = client else {
            join.close().await;
            node.close().await;
            return;
        };
        let write = client.write_all(b"ping").await;
        assert!(write.is_ok());
        let mut echoed = [0_u8; 4];
        let read =
            tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed)).await;
        assert!(matches!(read, Ok(Ok(_))));
        assert_eq!(&echoed, b"ping");
        join.close().await;
        node.close().await;
    }
}
