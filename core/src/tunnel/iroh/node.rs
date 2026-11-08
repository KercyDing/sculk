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
use super::endpoint::{build_endpoint, wait_online};
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
    /// 上次发布保存的令牌状态；首次发布时为 `None`。
    pub token_state: Option<TokenState>,
    /// 跨停止和重启生效的令牌刷新策略。
    pub token_refresh: TokenRefreshPolicy,
    /// 服务级连接策略。
    pub config: HostConfig,
}

/// 跨服务停止和进程重启生效的令牌刷新策略。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenRefreshPolicy {
    /// 每次发布服务时生成新令牌。
    #[default]
    Always,
    /// 一直复用已保存的令牌，直到手动轮换。
    Never,
    /// 从令牌创建时刻开始按自然时间定期轮换。
    After(Duration),
}

/// 需要由实例管理层安全持久化的令牌状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenState {
    token: AccessToken,
    created_at: SystemTime,
}

impl TokenState {
    /// 从令牌及其原始创建时间恢复状态。
    pub fn new(token: AccessToken, created_at: SystemTime) -> Self {
        Self { token, created_at }
    }

    /// 返回访问令牌。
    pub fn token(&self) -> &AccessToken {
        &self.token
    }

    /// 返回令牌的原始创建时间。
    pub fn created_at(&self) -> SystemTime {
        self.created_at
    }

    fn generate(now: SystemTime) -> Self {
        Self::new(AccessToken::generate(), now)
    }
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
    /// 等待 Relay 连接超时。
    #[error(transparent)]
    RelayUnavailable(#[from] crate::error::RelayUnavailable),
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
            Self::RelayUnavailable(_) => ErrorCategory::HostUnreachable,
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

struct ResolvedTokenState {
    state: TokenState,
    refresh: Option<Duration>,
    deadline: Option<SystemTime>,
    deadline_monotonic: Option<Instant>,
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
        if let Err(error) = wait_online(&endpoint, options.relay_url.as_ref()).await {
            endpoint.close().await;
            return Err(error.into());
        }

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
        let now = SystemTime::now();
        let now_monotonic = Instant::now();
        let resolved = resolve_token_state(
            options.token_refresh,
            options.token_state,
            now,
            now_monotonic,
        )?;
        let token_state = resolved.state;
        let token_refresh = resolved.refresh;
        let next_rotation_at = resolved.deadline;
        let next_rotation_instant = resolved.deadline_monotonic;
        JoinUri::new(
            self.inner.endpoint.id(),
            options.service_id,
            token_state.token.clone(),
            self.inner.relay_url.clone(),
        )
        .expose_secret_uri()?;
        let session_generation = loop {
            let generation = self.inner.service_session_next.load(Ordering::Relaxed);
            let next_generation = generation
                .checked_add(1)
                .ok_or(SculkNodeError::SessionGenerationExhausted)?;
            match self.inner.service_session_next.compare_exchange_weak(
                generation,
                next_generation,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break generation,
                Err(_) => continue,
            }
        };
        let (events_tx, mut events_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (updates_tx, _) = broadcast::channel(EVENT_CHANNEL_SIZE);
        let (rotation_tx, rotation_rx) = watch::channel(0_u64);
        let status =
            HostedStatus::new(options.service_id, token_state.created_at, next_rotation_at);
        let updates_tx_for_task = updates_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                let _ = updates_tx_for_task.send(event);
            }
        });
        let service = Arc::new(HostedService {
            target_addr: options.target_addr,
            token: RwLock::new(token_state.token.clone()),
            service_id: options.service_id,
            context: Arc::new(HostContext {
                conns: Arc::new(Mutex::new(Vec::new())),
                sessions: Arc::new(Mutex::new(HostSessions::default())),
                event_delay: options.config.event_delay,
                service_id: options.service_id,
                token: token_state.token,
                max_players: options.config.max_players,
                status: Some(status.clone()),
            }),
            events_tx,
            updates_tx,
            token_rotation: AsyncMutex::new(()),
            rotation_tx,
            session_generation,
            token_refresh,
            status,
        });
        let mut services = self.inner.services.write().await;
        if services.contains_key(&options.service_id) {
            return Err(SculkNodeError::DuplicateService);
        }
        services.insert(options.service_id, service);
        if let (Some(period), Some(deadline), Some(deadline_monotonic)) =
            (token_refresh, next_rotation_at, next_rotation_instant)
        {
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
                conn.close(CLOSE_HOST_STOPPED, b"service stopped");
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
        let service = self
            .inner
            .services
            .read()
            .await
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

        let services = self.inner.services.read().await;
        let current = services
            .get(&service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        if !Arc::ptr_eq(current, &service) {
            return Err(SculkNodeError::ServiceNotFound);
        }
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
                        conn.close(CLOSE_HOST_STOPPED, b"node stopped");
                    }
                }
            }
            service.status.set_phase(HostedServicePhase::Stopped);
        }
        self.inner.endpoint.close().await;
        let _ = self.inner.shutdown.send(true);
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

fn resolve_token_state(
    policy: TokenRefreshPolicy,
    saved: Option<TokenState>,
    now: SystemTime,
    now_monotonic: Instant,
) -> std::result::Result<ResolvedTokenState, SculkNodeError> {
    match policy {
        TokenRefreshPolicy::Always => Ok(ResolvedTokenState {
            state: TokenState::generate(now),
            refresh: None,
            deadline: None,
            deadline_monotonic: None,
        }),
        TokenRefreshPolicy::Never => Ok(ResolvedTokenState {
            state: saved.unwrap_or_else(|| TokenState::generate(now)),
            refresh: None,
            deadline: None,
            deadline_monotonic: None,
        }),
        TokenRefreshPolicy::After(period) => {
            if period.is_zero() {
                return Err(SculkNodeError::InvalidRefreshPeriod);
            }
            let mut state = saved
                .filter(|state| state.created_at <= now)
                .unwrap_or_else(|| TokenState::generate(now));
            let mut deadline = state
                .created_at
                .checked_add(period)
                .ok_or(SculkNodeError::InvalidRefreshPeriod)?;
            if deadline <= now {
                state = TokenState::generate(now);
                deadline = now
                    .checked_add(period)
                    .ok_or(SculkNodeError::InvalidRefreshPeriod)?;
            }
            let remaining = deadline
                .duration_since(now)
                .map_err(|_| SculkNodeError::InvalidRefreshPeriod)?;
            let deadline_monotonic = now_monotonic
                .checked_add(remaining)
                .ok_or(SculkNodeError::InvalidRefreshPeriod)?;
            Ok(ResolvedTokenState {
                state,
                refresh: Some(period),
                deadline: Some(deadline),
                deadline_monotonic: Some(deadline_monotonic),
            })
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

    /// 返回当前令牌及其原始创建时间，供实例管理层安全持久化。
    pub async fn token_state(&self) -> std::result::Result<TokenState, SculkNodeError> {
        let services = self.node.inner.services.read().await;
        let service = services
            .get(&self.service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        let _rotation = service.token_rotation.lock().await;
        let created_at = service.status.snapshot().token_created_at;
        let token = service.token.read().await.clone();
        Ok(TokenState::new(token, created_at))
    }

    /// 返回服务的稳定状态快照。
    pub async fn status(&self) -> std::result::Result<HostedServiceStatus, SculkNodeError> {
        let services = self.node.inner.services.read().await;
        let service = services
            .get(&self.service_id)
            .ok_or(SculkNodeError::ServiceNotFound)?;
        Ok(service.status.snapshot())
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
            conn.close(CLOSE_AUTH_FAILED, CLOSE_AUTH_FAILED_REASON);
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
        conn.close(CLOSE_AUTH_FAILED, CLOSE_AUTH_FAILED_REASON);
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
mod tests;
