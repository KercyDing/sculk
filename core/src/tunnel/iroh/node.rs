//! 共享 iroh Endpoint 的多服务 Host 实现。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc, watch};

use super::auth::{auth_accept, auth_respond};
use super::endpoint::build_endpoint;
use super::host::{HostContext, start_authenticated_host_connection};
use super::session::HostSessions;
use super::*;
use crate::types::{AccessToken, RelayUrl, SecretKey, ServiceId};

const NODE_CONNECTION_TASKS_MAX: usize = 128;

/// 多服务 Node 的创建参数。
#[derive(Clone, Debug, Default)]
pub struct NodeOptions {
    /// 稳定 Node 密钥；`None` 时创建临时身份。
    pub secret_key: Option<SecretKey>,
    /// Node 级自定义 Relay；所有服务共享此配置。
    pub relay_url: Option<RelayUrl>,
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
    token: AccessToken,
    join_uri: JoinUri,
    context: Arc<HostContext>,
    events_tx: mpsc::Sender<TunnelEvent>,
    updates_tx: broadcast::Sender<TunnelEvent>,
}

/// 已发布服务的轻量操作句柄。
#[derive(Clone)]
pub struct HostedServiceHandle {
    node: SculkNode,
    service_id: ServiceId,
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
        tokio::spawn(node_accept_loop(node.clone(), endpoint, shutdown_rx));
        Ok(node)
    }

    /// 发布服务并返回其操作句柄。
    pub async fn start_service(
        &self,
        options: HostedServiceOptions,
    ) -> std::result::Result<HostedServiceHandle, SculkNodeError> {
        validate_target(options.target_addr)?;
        let join_uri = JoinUri::new(
            self.inner.endpoint.id(),
            options.service_id,
            options.token.clone(),
            self.inner.relay_url.clone(),
        );
        let (events_tx, mut events_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (updates_tx, _) = broadcast::channel(EVENT_CHANNEL_SIZE);
        let updates_tx_for_task = updates_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                let _ = updates_tx_for_task.send(event);
            }
        });
        let service = Arc::new(HostedService {
            target_addr: options.target_addr,
            token: options.token.clone(),
            join_uri,
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
        });
        let mut services = self.inner.services.write().await;
        if services.contains_key(&options.service_id) {
            return Err(SculkNodeError::DuplicateService);
        }
        services.insert(options.service_id, service);
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
        self.node
            .inner
            .services
            .read()
            .await
            .get(&self.service_id)
            .map(|service| service.join_uri.clone())
            .ok_or(SculkNodeError::ServiceNotFound)
    }

    /// 独立停止此服务。
    pub async fn stop(&self) -> std::result::Result<(), SculkNodeError> {
        self.node.stop_service(self.service_id).await
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
    mut shutdown: watch::Receiver<bool>,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(NODE_CONNECTION_TASKS_MAX));
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
    let valid = service
        .as_ref()
        .is_some_and(|service| request.token.matches(&service.token));
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
                config: HostConfig::default(),
            })
            .await;
        assert!(first.is_ok());
        let second = node
            .start_service(HostedServiceOptions {
                service_id: second_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
                token: AccessToken::generate(),
                config: HostConfig::default(),
            })
            .await;
        assert!(second.is_ok());
        let Ok(second) = second else {
            return;
        };
        assert_eq!(node.service_count().await, 2);

        let stopped = node.stop_service(first_id).await;
        assert!(stopped.is_ok());
        assert_eq!(node.service_count().await, 1);
        assert!(second.join_uri().await.is_ok());
        node.close().await;
    }
}
