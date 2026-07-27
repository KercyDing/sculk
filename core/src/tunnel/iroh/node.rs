//! 共享 iroh Endpoint 的多服务 Host 实现。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, RwLock, broadcast, mpsc, watch};

use super::auth::{auth_accept, auth_respond};
use super::endpoint::build_endpoint;
use super::host::{HostContext, start_authenticated_host_connection};
use super::session::HostSessions;
use super::*;
use crate::ErrorCategory;
use crate::types::{AccessToken, RelayUrl, SecretKey, ServiceId};

const NODE_UNAUTHENTICATED_CONNECTIONS_MAX: NonZeroUsize = NonZeroUsize::new(128).unwrap();
const ROTATION_CLOCK_CHECK_MAX: Duration = Duration::from_secs(60);
const ROTATION_RETRY_MIN: Duration = Duration::from_secs(1);
const ROTATION_RETRY_MAX: Duration = Duration::from_secs(60);

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
    /// Another token rotation currently owns this service's publication state.
    #[error("service token rotation is already in progress")]
    RotationInProgress,
    #[error("token refresh period must be greater than zero")]
    InvalidRefreshPeriod,
    #[error("service session generation space exhausted")]
    SessionGenerationExhausted,
    #[error(transparent)]
    JoinUri(#[from] crate::error::JoinUriError),
    #[error("failed to bind Node endpoint")]
    BindEndpoint(#[source] crate::error::BoxError),
}

impl SculkNodeError {
    /// Returns the stable product-level category for this error.
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::TargetNotLoopback | Self::InvalidTargetPort | Self::InvalidRefreshPeriod => {
                ErrorCategory::InvalidConfiguration
            }
            Self::DuplicateService | Self::ServiceNotFound | Self::RotationInProgress => {
                ErrorCategory::OperationConflict
            }
            Self::SessionGenerationExhausted => ErrorCategory::ResourceLimit,
            Self::JoinUri(error) => error.category(),
            Self::BindEndpoint(_) => ErrorCategory::InvalidEndpoint,
        }
    }
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
    unauthenticated_slots: Arc<tokio::sync::Semaphore>,
    unauthenticated_connections_max: usize,
    service_session_next: AtomicU64,
    shutdown: watch::Sender<bool>,
}

struct HostedService {
    target_addr: SocketAddr,
    token: RwLock<AccessToken>,
    service_id: ServiceId,
    context: Arc<HostContext>,
    events_tx: mpsc::Sender<TunnelEvent>,
    updates_tx: broadcast::Sender<TunnelEvent>,
    token_rotation: AsyncMutex<()>,
    rotation_tx: watch::Sender<u64>,
    session_generation: u64,
    token_refresh: Option<Duration>,
    status: Arc<HostedStatus>,
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
    /// Stable EndpointId owned by this Node.
    pub endpoint_id: EndpointId,
    /// Whether the underlying Endpoint is still open.
    pub online: bool,
    /// Custom Node-level Relay configuration, if one is configured.
    pub relay_url: Option<RelayUrl>,
    /// 当前已发布服务数。
    pub service_count: usize,
    /// Connections currently waiting for authentication.
    pub unauthenticated_connection_count: usize,
    /// Configured global authentication concurrency limit.
    pub unauthenticated_connections_max: usize,
}

/// Lifecycle of a service registered in a [`SculkNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedServicePhase {
    /// Service is registered and accepts new authenticated connections.
    Active,
    /// Service was removed from routing and its existing connections are closing.
    Stopping,
    /// Service and its tracked connections have been stopped.
    Stopped,
}

/// 单个已发布服务的稳定状态快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedServiceStatus {
    /// 服务标识。
    pub service_id: ServiceId,
    /// Current service lifecycle phase.
    pub phase: HostedServicePhase,
    /// Generation of the currently published Join URI.
    pub uri_generation: u64,
    /// Wall-clock time when the current Token was created.
    pub token_created_at: SystemTime,
    /// Wall-clock deadline for the next automatic rotation.
    pub next_rotation_at: Option<SystemTime>,
    /// 当前已认证且仍存活的连接数量。
    pub connection_count: usize,
    /// Currently active TCP-to-QUIC bridges.
    pub bridge_count: usize,
    /// Most recent structured runtime error affecting this service.
    pub last_error: Option<ErrorCategory>,
}

/// Recoverable service status subscription.
pub struct HostedServiceStatusSubscription {
    status: watch::Receiver<HostedServiceStatus>,
    initial_status_pending: bool,
}

impl HostedServiceStatusSubscription {
    /// Receives the current status first and then every subsequent status change.
    pub async fn recv(&mut self) -> Option<HostedServiceStatus> {
        if self.initial_status_pending {
            self.initial_status_pending = false;
            return Some(self.status.borrow().clone());
        }
        self.status.changed().await.ok()?;
        Some(self.status.borrow().clone())
    }
}

pub(super) struct HostedStatus {
    current: Mutex<HostedServiceStatus>,
    status_tx: watch::Sender<HostedServiceStatus>,
}

impl HostedStatus {
    fn new(
        service_id: ServiceId,
        token_created_at: SystemTime,
        next_rotation_at: Option<SystemTime>,
    ) -> Arc<Self> {
        let initial = HostedServiceStatus {
            service_id,
            phase: HostedServicePhase::Active,
            uri_generation: 1,
            token_created_at,
            next_rotation_at,
            connection_count: 0,
            bridge_count: 0,
            last_error: None,
        };
        let (status_tx, _) = watch::channel(initial.clone());
        Arc::new(Self {
            current: Mutex::new(initial),
            status_tx,
        })
    }

    fn snapshot(&self) -> HostedServiceStatus {
        self.lock().clone()
    }

    fn subscribe(&self) -> HostedServiceStatusSubscription {
        HostedServiceStatusSubscription {
            status: self.status_tx.subscribe(),
            initial_status_pending: true,
        }
    }

    fn set_phase(&self, phase: HostedServicePhase) {
        self.update(|status| status.phase = phase);
    }

    pub(super) fn set_connection_count(&self, count: usize) {
        self.update(|status| status.connection_count = count);
    }

    pub(super) fn bridge_started(self: &Arc<Self>) -> HostedBridgeGuard {
        self.update(|status| {
            status.bridge_count = status.bridge_count.saturating_add(1);
        });
        HostedBridgeGuard {
            status: self.clone(),
        }
    }

    pub(super) fn set_error(&self, category: ErrorCategory) {
        self.update(|status| status.last_error = Some(category));
    }

    fn rotate_uri(&self, token_created_at: SystemTime, next_rotation_at: Option<SystemTime>) {
        self.update(|status| {
            status.uri_generation = status.uri_generation.saturating_add(1);
            status.token_created_at = token_created_at;
            status.next_rotation_at = next_rotation_at;
        });
    }

    fn update(&self, change: impl FnOnce(&mut HostedServiceStatus)) {
        let mut status = self.lock();
        change(&mut status);
        self.status_tx.send_replace(status.clone());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HostedServiceStatus> {
        match self.current.lock() {
            Ok(status) => status,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

pub(super) struct HostedBridgeGuard {
    status: Arc<HostedStatus>,
}

impl Drop for HostedBridgeGuard {
    fn drop(&mut self) {
        self.status.update(|status| {
            status.bridge_count = status.bridge_count.saturating_sub(1);
        });
    }
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
        let unauthenticated_connections_max = options.unauthenticated_connections_max.get();
        let unauthenticated_slots =
            Arc::new(tokio::sync::Semaphore::new(unauthenticated_connections_max));
        let node = Self {
            inner: Arc::new(NodeInner {
                endpoint: endpoint.clone(),
                relay_url: options.relay_url,
                services: RwLock::new(HashMap::new()),
                unauthenticated_slots,
                unauthenticated_connections_max,
                service_session_next: AtomicU64::new(1),
                shutdown,
            }),
        };
        tokio::spawn(node_accept_loop(
            Arc::downgrade(&node.inner),
            endpoint,
            node.inner.unauthenticated_slots.clone(),
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
        let token_created_at = SystemTime::now();
        let next_rotation_at = rotation_deadline(token_created_at, options.token_refresh)?;
        let next_rotation_instant =
            monotonic_rotation_deadline(Instant::now(), options.token_refresh)?;
        JoinUri::new(
            self.inner.endpoint.id(),
            options.service_id,
            options.token.clone(),
            self.inner.relay_url.clone(),
        )
        .expose_secret_uri()?;
        let session_generation = self
            .inner
            .service_session_next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |generation| {
                generation.checked_add(1)
            })
            .map_err(|_| SculkNodeError::SessionGenerationExhausted)?;
        let (events_tx, mut events_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (updates_tx, _) = broadcast::channel(EVENT_CHANNEL_SIZE);
        let (rotation_tx, rotation_rx) = watch::channel(0_u64);
        let status = HostedStatus::new(options.service_id, token_created_at, next_rotation_at);
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
                status: Some(status.clone()),
            }),
            events_tx,
            updates_tx,
            token_rotation: AsyncMutex::new(()),
            rotation_tx,
            session_generation,
            token_refresh: options.token_refresh,
            status,
        });
        let mut services = self.inner.services.write().await;
        if services.contains_key(&options.service_id) {
            return Err(SculkNodeError::DuplicateService);
        }
        services.insert(options.service_id, service);
        if let (Some(period), Some(deadline), Some(deadline_monotonic)) = (
            options.token_refresh,
            next_rotation_at,
            next_rotation_instant,
        ) {
            tokio::spawn(rotation_loop(
                Arc::downgrade(&self.inner),
                options.service_id,
                session_generation,
                period,
                deadline,
                deadline_monotonic,
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
        service.status.set_phase(HostedServicePhase::Stopping);
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
        service.status.set_phase(HostedServicePhase::Stopped);
        Ok(())
    }

    /// 返回当前已发布的服务数量。
    pub async fn service_count(&self) -> usize {
        self.inner.services.read().await.len()
    }

    /// 返回 Node 的稳定状态快照。
    pub async fn status(&self) -> SculkNodeStatus {
        let available = self.inner.unauthenticated_slots.available_permits();
        SculkNodeStatus {
            endpoint_id: self.inner.endpoint.id(),
            online: !self.inner.endpoint.is_closed(),
            relay_url: self.inner.relay_url.clone(),
            service_count: self.service_count().await,
            unauthenticated_connection_count: self
                .inner
                .unauthenticated_connections_max
                .saturating_sub(available),
            unauthenticated_connections_max: self.inner.unauthenticated_connections_max,
        }
    }

    /// 为指定服务生成新 Token，并返回对应的新 Join URI。
    ///
    /// 已认证的 QUIC 连接不受影响；后续新连接必须使用返回 URI 中的新 Token。
    pub async fn rotate_token(
        &self,
        service_id: ServiceId,
    ) -> std::result::Result<JoinUri, SculkNodeError> {
        self.rotate_token_inner(service_id, None, true).await
    }

    async fn rotate_token_inner(
        &self,
        service_id: ServiceId,
        expected_session_generation: Option<u64>,
        reset_timer: bool,
    ) -> std::result::Result<JoinUri, SculkNodeError> {
        let services = self.inner.services.read().await;
        let service = services
            .get(&service_id)
            .cloned()
            .ok_or(SculkNodeError::ServiceNotFound)?;
        if expected_session_generation
            .is_some_and(|generation| generation != service.session_generation)
        {
            return Err(SculkNodeError::ServiceNotFound);
        }
        let _rotation = service
            .token_rotation
            .try_lock()
            .map_err(|_| SculkNodeError::RotationInProgress)?;
        let new_token = AccessToken::generate();
        let candidate = JoinUri::new(
            self.inner.endpoint.id(),
            service.service_id,
            new_token.clone(),
            self.inner.relay_url.clone(),
        );
        candidate.expose_secret_uri()?;
        let token_created_at = SystemTime::now();
        let next_rotation_at = rotation_deadline(token_created_at, service.token_refresh)?;

        *service.token.write().await = new_token.clone();
        service
            .status
            .rotate_uri(token_created_at, next_rotation_at);
        if reset_timer {
            service
                .rotation_tx
                .send_modify(|generation| *generation = generation.wrapping_add(1));
        }
        super::emit_event(&service.events_tx, TunnelEvent::TokenRotated);
        drop(services);
        Ok(candidate)
    }

    /// 返回 Node 的稳定 EndpointId。
    pub fn endpoint_id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    async fn record_rotation_failure(
        &self,
        service_id: ServiceId,
        session_generation: u64,
        category: ErrorCategory,
        retry_in: Duration,
    ) {
        let service = self
            .inner
            .services
            .read()
            .await
            .get(&service_id)
            .filter(|service| service.session_generation == session_generation)
            .cloned();
        if let Some(service) = service {
            service.status.set_error(category);
            super::emit_event(
                &service.events_tx,
                TunnelEvent::TokenRotationFailed { retry_in },
            );
        }
    }

    /// 停止所有服务并关闭 Node Endpoint。
    pub async fn close(&self) {
        let _ = self.inner.shutdown.send(true);
        let services = {
            let mut services = self.inner.services.write().await;
            services
                .drain()
                .map(|(_, service)| service)
                .collect::<Vec<_>>()
        };
        for service in services {
            service.status.set_phase(HostedServicePhase::Stopping);
            if let Ok(mut conns) = service.context.conns.lock() {
                for tracked in conns.drain(..) {
                    if let Some(conn) = tracked.handle.upgrade() {
                        conn.close(CLOSE_AUTH_FAILED, b"node stopped");
                    }
                }
            }
            service.status.set_phase(HostedServicePhase::Stopped);
        }
        self.inner.endpoint.close().await;
    }
}

async fn rotation_loop(
    inner: std::sync::Weak<NodeInner>,
    service_id: ServiceId,
    session_generation: u64,
    period: Duration,
    deadline: SystemTime,
    deadline_monotonic: Instant,
    mut reset_rx: watch::Receiver<u64>,
) {
    let mut schedule = RotationSchedule::new(period, deadline, deadline_monotonic);
    loop {
        let delay = schedule.delay(SystemTime::now(), Instant::now());
        tokio::select! {
            _ = tokio::time::sleep(delay) => {
                if !schedule.should_rotate(SystemTime::now(), Instant::now()) {
                    continue;
                }
                let Some(inner) = inner.upgrade() else { return };
                let node = SculkNode { inner };
                match node
                    .rotate_token_inner(service_id, Some(session_generation), false)
                    .await
                {
                    Ok(_) => {
                        if !schedule.reset(SystemTime::now(), Instant::now()) {
                            return;
                        }
                    }
                    Err(SculkNodeError::ServiceNotFound) => return,
                    Err(SculkNodeError::RotationInProgress) => {
                        schedule.record_failure();
                    }
                    Err(error) => {
                        schedule.record_failure();
                        let retry_in = schedule.retry_delay();
                        node.record_rotation_failure(
                            service_id,
                            session_generation,
                            error.category(),
                            retry_in,
                        )
                        .await;
                    }
                }
            }
            changed = reset_rx.changed() => {
                if changed.is_err() {
                    return;
                }
                if !schedule.reset(SystemTime::now(), Instant::now()) {
                    return;
                }
            }
        }
    }
}

struct RotationSchedule {
    period: Duration,
    deadline: SystemTime,
    deadline_monotonic: Instant,
    failure_count: u32,
}

impl RotationSchedule {
    fn new(period: Duration, deadline: SystemTime, deadline_monotonic: Instant) -> Self {
        Self {
            period,
            deadline,
            deadline_monotonic,
            failure_count: 0,
        }
    }

    fn delay(&self, now: SystemTime, now_monotonic: Instant) -> Duration {
        if self.failure_count > 0 {
            return self.retry_delay();
        }
        let wall_delay = self.deadline.duration_since(now).unwrap_or(Duration::ZERO);
        let monotonic_delay = self
            .deadline_monotonic
            .saturating_duration_since(now_monotonic);
        wall_delay
            .min(monotonic_delay)
            .min(ROTATION_CLOCK_CHECK_MAX)
    }

    fn should_rotate(&self, now: SystemTime, now_monotonic: Instant) -> bool {
        self.failure_count > 0 || now >= self.deadline || now_monotonic >= self.deadline_monotonic
    }

    fn reset(&mut self, now: SystemTime, now_monotonic: Instant) -> bool {
        let Some(deadline) = now.checked_add(self.period) else {
            return false;
        };
        let Some(deadline_monotonic) = now_monotonic.checked_add(self.period) else {
            return false;
        };
        self.deadline = deadline;
        self.deadline_monotonic = deadline_monotonic;
        self.failure_count = 0;
        true
    }

    fn record_failure(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
    }

    fn retry_delay(&self) -> Duration {
        let exponent = self.failure_count.saturating_sub(1).min(31);
        ROTATION_RETRY_MIN
            .saturating_mul(2_u32.saturating_pow(exponent))
            .min(ROTATION_RETRY_MAX)
    }
}

fn rotation_deadline(
    now: SystemTime,
    period: Option<Duration>,
) -> std::result::Result<Option<SystemTime>, SculkNodeError> {
    period
        .map(|period| {
            now.checked_add(period)
                .ok_or(SculkNodeError::InvalidRefreshPeriod)
        })
        .transpose()
}

fn monotonic_rotation_deadline(
    now: Instant,
    period: Option<Duration>,
) -> std::result::Result<Option<Instant>, SculkNodeError> {
    period
        .map(|period| {
            now.checked_add(period)
                .ok_or(SculkNodeError::InvalidRefreshPeriod)
        })
        .transpose()
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
        let services = self.node.inner.services.read().await;
        let service = services
            .get(&self.service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let _rotation = service.token_rotation.lock().await;
        let uri = JoinUri::new(
            self.node.inner.endpoint.id(),
            service.service_id,
            service.token.read().await.clone(),
            self.node.inner.relay_url.clone(),
        );
        Ok(uri)
    }

    /// 返回服务的稳定状态快照。
    pub async fn status(&self) -> std::result::Result<HostedServiceStatus, SculkNodeError> {
        let services = self.node.inner.services.read().await;
        let service = services
            .get(&self.service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let _rotation = service.token_rotation.lock().await;
        let status = service.status.snapshot();
        Ok(status)
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

    /// Subscribes to recoverable status snapshots for this service.
    pub async fn subscribe_status(
        &self,
    ) -> std::result::Result<HostedServiceStatusSubscription, SculkNodeError> {
        self.node
            .inner
            .services
            .read()
            .await
            .get(&self.service_id)
            .map(|service| service.status.subscribe())
            .ok_or(SculkNodeError::ServiceNotFound)
    }
}

async fn node_accept_loop(
    inner: std::sync::Weak<NodeInner>,
    endpoint: Endpoint,
    unauthenticated_slots: Arc<tokio::sync::Semaphore>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let accepting = tokio::select! {
            _ = super::wait_for_shutdown(&mut shutdown) => {
                endpoint.close().await;
                return;
            },
            accepting = endpoint.accept() => match accepting { Some(value) => value, None => return },
        };
        let permit = tokio::select! {
            _ = super::wait_for_shutdown(&mut shutdown) => {
                endpoint.close().await;
                return;
            }
            permit = unauthenticated_slots.clone().acquire_owned() => {
                let Ok(permit) = permit else { return };
                permit
            }
        };
        let Some(inner) = inner.upgrade() else {
            endpoint.close().await;
            return;
        };
        let node = SculkNode { inner };
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
        service.target_addr,
        &service.events_tx,
        &service.context,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    async fn recv_status_where(
        updates: &mut HostedServiceStatusSubscription,
        predicate: impl Fn(&HostedServiceStatus) -> bool,
    ) -> Option<HostedServiceStatus> {
        for _ in 0..16 {
            let update = tokio::time::timeout(Duration::from_secs(5), updates.recv()).await;
            let Ok(Some(status)) = update else {
                return None;
            };
            if predicate(&status) {
                return Some(status);
            }
        }
        None
    }

    async fn join_node(
        node: &SculkNode,
        uri: &JoinUri,
    ) -> crate::Result<(IrohTunnel, mpsc::Receiver<TunnelEvent>)> {
        IrohTunnel::join_direct(uri, node.inner.endpoint.addr(), 0, JoinConfig::default()).await
    }

    #[test]
    fn validates_target_address_boundaries() {
        assert!(validate_target(SocketAddr::from(([127, 0, 0, 1], 1))).is_ok());
        assert!(validate_target(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 65535))).is_ok());

        let target = "192.168.1.2:25565".parse();
        assert!(target.is_ok());
        let result = validate_target(target.unwrap_or_else(|_| unreachable!()));
        assert!(matches!(result, Err(SculkNodeError::TargetNotLoopback)));

        let zero_port = SocketAddr::from(([127, 0, 0, 1], 0));
        assert!(matches!(
            validate_target(zero_port),
            Err(SculkNodeError::InvalidTargetPort)
        ));
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
        let Ok(first) = first else {
            node.close().await;
            return;
        };
        let first_status = first.subscribe_status().await;
        assert!(first_status.is_ok());
        let Ok(mut first_status) = first_status else {
            node.close().await;
            return;
        };
        let initial = first_status.recv().await;
        assert!(matches!(
            initial,
            Some(HostedServiceStatus {
                phase: HostedServicePhase::Active,
                uri_generation: 1,
                connection_count: 0,
                bridge_count: 0,
                ..
            })
        ));
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
        let node_status = node.status().await;
        assert_eq!(node_status.endpoint_id, node.endpoint_id());
        assert!(node_status.online);
        assert!(node_status.relay_url.is_none());
        assert_eq!(node_status.service_count, 2);
        assert_eq!(node_status.unauthenticated_connection_count, 0);
        assert_eq!(
            node_status.unauthenticated_connections_max,
            NODE_UNAUTHENTICATED_CONNECTIONS_MAX.get()
        );
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
        let second_status = second.status().await;
        assert_eq!(
            second_status.ok().map(|status| status.uri_generation),
            Some(2)
        );
        let rotated = tokio::time::timeout(Duration::from_secs(1), updates.recv()).await;
        assert!(matches!(rotated, Ok(Ok(TunnelEvent::TokenRotated))));
        assert_eq!(node.service_count().await, 2);

        let stopped = node.stop_service(first_id).await;
        assert!(stopped.is_ok());
        let stopped_status = first_status.recv().await;
        assert!(matches!(
            stopped_status,
            Some(HostedServiceStatus {
                phase: HostedServicePhase::Stopped,
                ..
            })
        ));
        assert!(matches!(
            first.status().await,
            Err(SculkNodeError::ServiceNotFound)
        ));
        assert_eq!(node.service_count().await, 1);
        assert!(second.join_uri().await.is_ok());
        node.close().await;
        let closed = node.status().await;
        assert!(!closed.online);
        assert_eq!(closed.service_count, 0);
    }

    #[tokio::test]
    async fn joins_and_forwards_to_the_selected_service() {
        // IPv6 loopback proves routing preserves the full SocketAddr, not only the target port.
        let listener = tokio::net::TcpListener::bind("[::1]:0").await;
        assert!(listener.is_ok());
        let Ok(listener) = listener else {
            return;
        };
        let target_addr = listener.local_addr();
        assert!(target_addr.is_ok());
        let Ok(target_addr) = target_addr else {
            return;
        };
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut data = [0_u8; 4];
            if stream.read_exact(&mut data).await.is_ok() {
                let _ = stream.write_all(&data).await;
                let _ = release_rx.await;
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
        let status_updates = service.subscribe_status().await;
        assert!(status_updates.is_ok());
        let Ok(mut status_updates) = status_updates else {
            node.close().await;
            return;
        };
        assert!(status_updates.recv().await.is_some());
        let uri = service.join_uri().await;
        assert!(uri.is_ok());
        let Ok(uri) = uri else {
            node.close().await;
            return;
        };
        let join = join_node(&node, &uri).await;
        assert!(join.is_ok());
        let Ok((join, _)) = join else {
            node.close().await;
            return;
        };
        let connected =
            recv_status_where(&mut status_updates, |status| status.connection_count == 1).await;
        assert!(
            connected.is_some(),
            "authenticated connection status missing"
        );
        let rotated = service.rotate_token().await;
        assert!(rotated.is_ok(), "rotation after authentication failed");
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
        let bridging =
            recv_status_where(&mut status_updates, |status| status.bridge_count == 1).await;
        assert!(bridging.is_some(), "active bridge status missing");
        let _ = release_tx.send(());
        drop(client);
        let bridge_closed =
            recv_status_where(&mut status_updates, |status| status.bridge_count == 0).await;
        assert!(bridge_closed.is_some(), "closed bridge status missing");
        join.close().await;
        node.close().await;
    }

    #[tokio::test]
    async fn rejects_unknown_service_and_wrong_token_identically() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let token = AccessToken::generate();
        let started = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: token.clone(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(started.is_ok());

        for (requested_service, requested_token) in [
            (ServiceId::generate(), token.clone()),
            (service_id, AccessToken::generate()),
        ] {
            let uri = JoinUri::new(node.endpoint_id(), requested_service, requested_token, None);
            let rejected = join_node(&node, &uri).await;
            assert!(
                matches!(
                    rejected,
                    Err(crate::SculkError::Tunnel(
                        crate::error::TunnelError::AuthRejectedByHost
                    ))
                ),
                "rejection must use the public authorization error"
            );
        }
        node.close().await;
    }

    #[tokio::test]
    async fn rejects_additional_stream_before_authentication() -> TestResult {
        let node = SculkNode::bind(NodeOptions::default()).await?;
        let client = build_endpoint(None, None).bind().await?;
        let conn = client.connect(node.inner.endpoint.addr(), ALPN).await?;
        let (_control_send, _control_recv) = conn.open_bi().await?;
        let (mut extra_send, _extra_recv) = conn.open_bi().await?;
        extra_send.write_all(b"x").await?;
        extra_send.finish()?;

        let closed = tokio::time::timeout(Duration::from_secs(3), conn.closed()).await?;
        assert!(matches!(
            closed,
            ConnectionError::ApplicationClosed(ApplicationClose {
                error_code,
                ..
            }) if error_code == CLOSE_AUTH_FAILED
        ));

        client.close().await;
        node.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn reports_fixed_local_port_conflict() -> TestResult {
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let occupied_addr = occupied.local_addr()?;
        let node = SculkNode::bind(NodeOptions::default()).await?;
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await?;
        let uri = service.join_uri().await?;

        let join = IrohTunnel::join_direct(
            &uri,
            node.inner.endpoint.addr(),
            occupied_addr.port(),
            JoinConfig::default(),
        )
        .await;
        assert_eq!(
            join.as_ref().err().map(crate::SculkError::category),
            Some(ErrorCategory::LocalPortUnavailable)
        );
        if let Ok((tunnel, _)) = join {
            tunnel.close().await;
        }
        node.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn bounds_local_forwarding_sessions() -> TestResult {
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let target_addr = target.local_addr()?;
        let node = SculkNode::bind(NodeOptions::default()).await?;
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr,
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await?;
        let uri = service.join_uri().await?;
        let (join, _) = IrohTunnel::join_direct(
            &uri,
            node.inner.endpoint.addr(),
            0,
            JoinConfig::new().local_sessions_max(NonZeroUsize::MIN),
        )
        .await?;
        let Some(local_addr) = join.local_addr() else {
            join.close().await;
            node.close().await;
            return Err(std::io::Error::other("Join listener address is missing").into());
        };

        let mut first_local = tokio::net::TcpStream::connect(local_addr).await?;
        first_local.write_all(b"a").await?;
        let (first_target, _) =
            tokio::time::timeout(Duration::from_secs(3), target.accept()).await??;

        let mut second_local = tokio::net::TcpStream::connect(local_addr).await?;
        second_local.write_all(b"b").await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(200), target.accept())
                .await
                .is_err(),
            "second session bypassed the configured limit"
        );

        drop(first_local);
        drop(first_target);
        tokio::time::timeout(Duration::from_secs(3), target.accept()).await??;

        drop(second_local);
        join.close().await;
        node.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn stopping_service_closes_authenticated_connections() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let service = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
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
        let join = join_node(&node, &uri).await;
        assert!(join.is_ok());
        let Ok((join, _)) = join else {
            node.close().await;
            return;
        };

        let connected = async {
            for _ in 0..100 {
                if service
                    .status()
                    .await
                    .is_ok_and(|status| status.connection_count == 1)
                {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            false
        };
        assert!(connected.await);
        assert!(node.stop_service(service_id).await.is_ok());

        let closed = async {
            for _ in 0..100 {
                if join
                    .connections()
                    .is_ok_and(|connections| connections.is_empty())
                {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            false
        };
        assert!(closed.await, "authenticated connection remained after stop");
        assert!(matches!(
            service.status().await,
            Err(SculkNodeError::ServiceNotFound)
        ));

        join.close().await;
        node.close().await;
    }

    #[test]
    fn rotation_schedule_handles_clock_jumps_without_catch_up() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let deadline = start + Duration::from_secs(120);
        let monotonic_start = Instant::now();
        let monotonic_deadline = monotonic_start + Duration::from_secs(120);
        let mut schedule =
            RotationSchedule::new(Duration::from_secs(120), deadline, monotonic_deadline);

        assert_eq!(
            schedule.delay(start, monotonic_start),
            ROTATION_CLOCK_CHECK_MAX,
            "long waits must be split into bounded clock checks"
        );
        let jumped_forward = start + Duration::from_secs(500);
        assert!(schedule.should_rotate(jumped_forward, monotonic_start));
        assert!(schedule.reset(jumped_forward, monotonic_start));
        assert!(!schedule.should_rotate(jumped_forward, monotonic_start));
        assert_eq!(
            schedule.deadline,
            jumped_forward + Duration::from_secs(120),
            "missed periods must collapse into one rotation"
        );

        let jumped_backward = start - Duration::from_secs(500);
        assert_eq!(
            schedule.delay(jumped_backward, monotonic_start),
            ROTATION_CLOCK_CHECK_MAX,
            "backward wall-clock jumps must not delay the monotonic deadline"
        );
        assert!(schedule.should_rotate(jumped_backward, monotonic_deadline));
    }

    #[test]
    fn rotation_retry_backoff_is_bounded() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let monotonic_start = Instant::now();
        let mut schedule = RotationSchedule::new(Duration::from_secs(60), start, monotonic_start);

        schedule.record_failure();
        assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MIN);
        schedule.record_failure();
        assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MIN.saturating_mul(2));
        for _ in 0..64 {
            schedule.record_failure();
        }
        assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MAX);
        assert_eq!(schedule.delay(start, monotonic_start), ROTATION_RETRY_MAX);
        assert!(schedule.should_rotate(start, monotonic_start));
    }

    #[test]
    fn rejects_unrepresentable_rotation_deadline() {
        let deadline = rotation_deadline(SystemTime::UNIX_EPOCH, Some(Duration::MAX));
        assert!(matches!(
            deadline,
            Err(SculkNodeError::InvalidRefreshPeriod)
        ));
    }

    #[tokio::test]
    async fn manual_rotation_resets_automatic_deadline() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let period = Duration::from_millis(300);
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: Some(period),
                config: HostConfig::default(),
            })
            .await;
        assert!(service.is_ok());
        let Ok(service) = service else {
            node.close().await;
            return;
        };
        let initial = service.status().await;
        assert!(initial.is_ok());
        let initial_created_at = initial.as_ref().ok().map(|status| status.token_created_at);
        assert!(
            initial
                .as_ref()
                .ok()
                .and_then(|status| status.next_rotation_at)
                .is_some()
        );

        tokio::time::sleep(Duration::from_millis(200)).await;
        let manual = service.rotate_token().await;
        assert!(manual.is_ok());
        let after_manual = service.status().await;
        assert!(after_manual.is_ok());
        let Some(after_manual) = after_manual.ok() else {
            node.close().await;
            return;
        };
        assert_eq!(after_manual.uri_generation, 2);
        assert!(Some(after_manual.token_created_at) > initial_created_at);

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            service
                .status()
                .await
                .ok()
                .map(|status| status.uri_generation),
            Some(2),
            "the old automatic deadline was not cancelled"
        );

        let rotated = async {
            for _ in 0..40 {
                let status = service.status().await.ok()?;
                if status.uri_generation == 3 {
                    return Some(status);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            None
        };
        let rotated = tokio::time::timeout(Duration::from_secs(1), rotated).await;
        assert!(matches!(rotated, Ok(Some(_))));
        node.close().await;
    }

    #[tokio::test]
    async fn rotation_conflict_preserves_current_uri() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let service = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
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
        let current_uri = service.join_uri().await;
        assert!(current_uri.is_ok());
        let Some(hosted) = node.inner.services.read().await.get(&service_id).cloned() else {
            node.close().await;
            return;
        };
        let rotation = hosted.token_rotation.lock().await;

        assert!(matches!(
            service.rotate_token().await,
            Err(SculkNodeError::RotationInProgress)
        ));
        drop(rotation);

        let after_conflict = service.join_uri().await;
        assert!(after_conflict.is_ok());
        let (Ok(current_uri), Ok(after_conflict)) = (current_uri, after_conflict) else {
            node.close().await;
            return;
        };
        assert!(current_uri.token().matches(after_conflict.token()));
        assert_eq!(
            service
                .status()
                .await
                .ok()
                .map(|status| status.uri_generation),
            Some(1)
        );
        node.close().await;
    }

    #[tokio::test]
    async fn stopped_rotation_task_cannot_modify_republished_service() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let old = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: Some(Duration::from_millis(100)),
                config: HostConfig::default(),
            })
            .await;
        assert!(old.is_ok());
        let stopped = node.stop_service(service_id).await;
        assert!(stopped.is_ok());

        let current = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(current.is_ok());
        let Ok(current) = current else {
            node.close().await;
            return;
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            current
                .status()
                .await
                .ok()
                .map(|status| status.uri_generation),
            Some(1)
        );
        node.close().await;
    }

    #[tokio::test]
    async fn rotation_failure_updates_status_and_warning_event() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let service = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
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
        let events = service.subscribe().await;
        assert!(events.is_ok());
        let Ok(mut events) = events else {
            node.close().await;
            return;
        };
        let session_generation = node
            .inner
            .services
            .read()
            .await
            .get(&service_id)
            .map(|service| service.session_generation);
        assert!(session_generation.is_some());
        let Some(session_generation) = session_generation else {
            node.close().await;
            return;
        };

        node.record_rotation_failure(
            service_id,
            session_generation,
            ErrorCategory::Internal,
            ROTATION_RETRY_MIN,
        )
        .await;
        assert_eq!(
            service
                .status()
                .await
                .ok()
                .and_then(|status| status.last_error),
            Some(ErrorCategory::Internal)
        );
        let warning = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
        assert!(matches!(
            warning,
            Ok(Ok(TunnelEvent::TokenRotationFailed {
                retry_in: ROTATION_RETRY_MIN
            }))
        ));
        node.close().await;
    }

    #[tokio::test]
    async fn rejects_duplicate_service_ids() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service_id = ServiceId::generate();
        let first = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(first.is_ok());
        let duplicate = node
            .start_service(HostedServiceOptions {
                service_id,
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::default(),
            })
            .await;
        assert!(matches!(duplicate, Err(SculkNodeError::DuplicateService)));
        assert_eq!(node.service_count().await, 1);
        node.close().await;
    }

    #[tokio::test]
    async fn dropping_last_node_handle_closes_endpoint() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let observer = node.inner.endpoint.clone();
        drop(node);

        let closed = tokio::time::timeout(Duration::from_secs(3), observer.closed()).await;
        assert!(closed.is_ok(), "Node Endpoint remained open after drop");
    }

    #[tokio::test]
    async fn enforces_service_connection_limit() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
                token: AccessToken::generate(),
                token_refresh: None,
                config: HostConfig::new().max_players(Some(1)),
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
        let status = service.subscribe_status().await;
        assert!(status.is_ok());
        let Ok(mut status) = status else {
            node.close().await;
            return;
        };
        assert!(status.recv().await.is_some());
        let events = service.subscribe().await;
        assert!(events.is_ok());
        let Ok(mut events) = events else {
            node.close().await;
            return;
        };

        let first = join_node(&node, &uri).await;
        assert!(first.is_ok());
        let Ok((first, _)) = first else {
            node.close().await;
            return;
        };
        let first_connected =
            recv_status_where(&mut status, |status| status.connection_count == 1).await;
        assert!(first_connected.is_some());

        let second = join_node(&node, &uri).await;
        let second = second.ok().map(|(tunnel, _)| tunnel);
        let rejected = async {
            for _ in 0..16 {
                let event = events.recv().await.ok()?;
                if matches!(event, TunnelEvent::PlayerRejected { .. }) {
                    return Some(());
                }
            }
            None
        };
        let rejected = tokio::time::timeout(Duration::from_secs(5), rejected).await;
        assert!(matches!(rejected, Ok(Some(()))));
        assert_eq!(
            service
                .status()
                .await
                .ok()
                .map(|status| status.connection_count),
            Some(1)
        );

        if let Some(second) = second {
            second.close().await;
        }
        first.close().await;
        node.close().await;
    }

    #[tokio::test]
    async fn bounds_unauthenticated_connections_globally() {
        let node = SculkNode::bind(NodeOptions {
            unauthenticated_connections_max: NonZeroUsize::MIN,
            ..NodeOptions::default()
        })
        .await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let client_a = build_endpoint(None, None).bind().await;
        assert!(client_a.is_ok());
        let Ok(client_a) = client_a else {
            node.close().await;
            return;
        };
        let node_addr = node.inner.endpoint.addr();
        let conn_a = client_a.connect(node_addr, ALPN).await;
        assert!(conn_a.is_ok());
        let Ok(conn_a) = conn_a else {
            client_a.close().await;
            node.close().await;
            return;
        };

        let bounded = async {
            for _ in 0..100 {
                let status = node.status().await;
                if status.unauthenticated_connection_count == 1 {
                    return Some(status);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            None
        };
        let bounded = tokio::time::timeout(Duration::from_secs(3), bounded).await;
        assert!(matches!(
            bounded,
            Ok(Some(SculkNodeStatus {
                unauthenticated_connection_count: 1,
                unauthenticated_connections_max: 1,
                ..
            }))
        ));

        conn_a.close(CLOSE_AUTH_FAILED, b"test complete");
        client_a.close().await;
        node.close().await;
    }

    #[tokio::test]
    async fn records_target_connection_failures_in_status() {
        let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await;
        assert!(unused.is_ok());
        let Ok(unused) = unused else {
            return;
        };
        let target_addr = unused.local_addr();
        assert!(target_addr.is_ok());
        let Ok(target_addr) = target_addr else {
            return;
        };

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
        let join = join_node(&node, &uri).await;
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
        drop(unused);
        let client = tokio::net::TcpStream::connect(local_addr).await;
        assert!(client.is_ok());
        let Ok(mut client) = client else {
            join.close().await;
            node.close().await;
            return;
        };
        assert!(client.write_all(b"ping").await.is_ok());

        let error_recorded = async {
            for _ in 0..100 {
                let status = service.status().await.ok()?;
                if status.last_error == Some(ErrorCategory::TargetUnavailable) {
                    return Some(());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            None
        };
        let error_recorded = tokio::time::timeout(Duration::from_secs(7), error_recorded).await;
        assert!(matches!(error_recorded, Ok(Some(()))));
        join.close().await;
        node.close().await;
    }

    #[tokio::test]
    async fn rejects_old_uri_after_token_rotation() {
        let node = SculkNode::bind(NodeOptions::default()).await;
        assert!(node.is_ok());
        let Ok(node) = node else {
            return;
        };
        let service = node
            .start_service(HostedServiceOptions {
                service_id: ServiceId::generate(),
                target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
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
        let old_uri = service.join_uri().await;
        assert!(old_uri.is_ok());
        let Ok(old_uri) = old_uri else {
            node.close().await;
            return;
        };
        let rotated_uri = service.rotate_token().await;
        assert!(rotated_uri.is_ok());
        let Ok(rotated_uri) = rotated_uri else {
            node.close().await;
            return;
        };

        let old_join = join_node(&node, &old_uri).await;
        assert!(old_join.is_err());
        let new_join = join_node(&node, &rotated_uri).await;
        assert!(new_join.is_ok());
        if let Ok((join, _)) = new_join {
            join.close().await;
        }
        node.close().await;
    }
}
