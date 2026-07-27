//! 托管隧道服务：集中处理单实例生命周期、状态快照与多调用方事件订阅。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::{net::SocketAddr, num::NonZeroU16};

use thiserror::Error;
use tokio::sync::{Mutex, broadcast, watch};
use tokio::task::JoinHandle;

use super::{
    AccessToken, ConnectionSnapshot, HostConfig, IrohTunnel, JoinConfig, JoinUri, RelayUrl,
    SecretKey, ServiceId, TunnelEvent,
};
use crate::{ErrorCategory, SculkError};

const EVENT_BROADCAST_SIZE: usize = 128;
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_millis(200);

/// 隧道运行角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelMode {
    Host,
    Join,
}

/// 托管隧道生命周期阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelPhase {
    Idle,
    Starting,
    Active,
    Stopping,
}

/// 不包含连接指标的轻量生命周期快照。
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TunnelState {
    /// 当前生命周期阶段。
    pub phase: TunnelPhase,
    /// 正在启动、运行或关闭的角色；空闲时为 `None`。
    pub mode: Option<TunnelMode>,
    /// Host 活动时可分享的 Join URI。
    pub join_uri: Option<JoinUri>,
    /// Join 活动时实际绑定的本地监听地址。
    pub local_addr: Option<SocketAddr>,
}

impl TunnelState {
    fn idle() -> Self {
        Self {
            phase: TunnelPhase::Idle,
            mode: None,
            join_uri: None,
            local_addr: None,
        }
    }

    fn pending(phase: TunnelPhase, mode: TunnelMode) -> Self {
        Self {
            phase,
            mode: Some(mode),
            join_uri: None,
            local_addr: None,
        }
    }
}

/// 包含当前连接指标的完整托管隧道快照。
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TunnelStatus {
    /// 生命周期、角色与 Join URI 快照。
    pub state: TunnelState,
    /// 查询时仍存活的连接快照。
    pub connections: Vec<ConnectionSnapshot>,
}

impl TunnelStatus {
    fn idle() -> Self {
        Self {
            state: TunnelState::idle(),
            connections: Vec::new(),
        }
    }
}

/// Host 托管启动参数。
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct HostOptions {
    /// 本地 Minecraft 服务端端口。
    pub mc_port: u16,
    /// 稳定节点密钥；`None` 表示生成临时密钥。
    pub secret_key: Option<SecretKey>,
    /// 自定义中继；`None` 表示使用 iroh 默认中继。
    pub relay_url: Option<RelayUrl>,
    /// 服务标识。由实例管理层在创建服务时生成并持久化。
    pub service_id: ServiceId,
    /// 当前 Host 会话的访问令牌。
    pub token: AccessToken,
    /// Host 行为配置。
    pub config: HostConfig,
}

impl HostOptions {
    /// 使用指定 Minecraft 服务端端口创建默认 Host 参数。
    pub fn new(mc_port: u16) -> Self {
        Self {
            mc_port,
            secret_key: None,
            relay_url: None,
            service_id: ServiceId::generate(),
            token: AccessToken::generate(),
            config: HostConfig::default(),
        }
    }

    /// 设置稳定节点密钥。
    pub fn secret_key(mut self, secret_key: Option<SecretKey>) -> Self {
        self.secret_key = secret_key;
        self
    }

    /// 设置自定义中继。
    pub fn relay_url(mut self, relay_url: Option<RelayUrl>) -> Self {
        self.relay_url = relay_url;
        self
    }

    /// 设置服务标识。
    pub fn service_id(mut self, service_id: ServiceId) -> Self {
        self.service_id = service_id;
        self
    }

    /// 设置当前会话访问令牌。
    pub fn token(mut self, token: AccessToken) -> Self {
        self.token = token;
        self
    }

    /// 设置 Host 行为配置。
    pub fn config(mut self, config: HostConfig) -> Self {
        self.config = config;
        self
    }
}

/// Join 托管启动参数。
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct JoinOptions {
    /// Host 分享的完整 Join URI。
    pub join_uri: JoinUri,
    /// 本地 Minecraft 客户端连接端口。
    pub local_port: LocalPort,
    /// Join 行为配置。
    pub config: JoinConfig,
}

impl JoinOptions {
    /// 使用 Join URI 创建自动分配本地端口的参数。
    pub fn new(join_uri: JoinUri) -> Self {
        Self {
            join_uri,
            local_port: LocalPort::Auto,
            config: JoinConfig::default(),
        }
    }

    /// 设置本地监听端口；默认自动分配。
    pub fn local_port(mut self, local_port: LocalPort) -> Self {
        self.local_port = local_port;
        self
    }

    /// 设置 Join 行为配置。
    pub fn config(mut self, config: JoinConfig) -> Self {
        self.config = config;
        self
    }
}

/// Join 本地 TCP 监听端口策略。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalPort {
    /// 由操作系统自动分配可用端口。
    Auto,
    /// 绑定指定的非零端口。
    Fixed(NonZeroU16),
}

impl LocalPort {
    fn value(self) -> u16 {
        match self {
            Self::Auto => 0,
            Self::Fixed(port) => port.get(),
        }
    }
}

/// 托管隧道服务错误。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TunnelServiceError {
    #[error("a tunnel is already starting, active, or stopping")]
    Busy,
    #[error("no tunnel is active")]
    NotRunning,
    #[error("port must be non-zero")]
    InvalidPort,
    #[error("max players must be greater than zero")]
    InvalidMaxPlayers,
    #[error(transparent)]
    Tunnel(#[from] SculkError),
}

impl TunnelServiceError {
    /// Returns the stable product-level category for this error.
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::Busy | Self::NotRunning => ErrorCategory::OperationConflict,
            Self::InvalidPort | Self::InvalidMaxPlayers => ErrorCategory::InvalidConfiguration,
            Self::Tunnel(error) => error.category(),
        }
    }
}

/// 托管隧道的统一更新。
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum TunnelUpdate {
    /// 生命周期与连接指标的最新完整快照。
    Status(TunnelStatus),
    /// 连接、重连、路径变化或错误等过程事件。
    Event(TunnelEvent),
}

/// 托管隧道统一订阅端。
///
/// 首次调用 [`Self::recv`] 会返回当时的最新状态，随后同时接收状态变化和过程事件。
/// 状态使用 `watch` 保证可恢复到最新值，过程事件使用有界广播且不会阻塞网络任务。
pub struct TunnelSubscription {
    status: watch::Receiver<TunnelStatus>,
    events: broadcast::Receiver<TunnelEvent>,
    initial_status_pending: bool,
    status_open: bool,
    events_open: bool,
}

impl TunnelSubscription {
    /// 接收下一项状态或过程事件；所有发送端关闭且缓冲区耗尽时返回 `None`。
    pub async fn recv(&mut self) -> Option<TunnelUpdate> {
        if self.initial_status_pending {
            self.initial_status_pending = false;
            return Some(TunnelUpdate::Status(self.status.borrow().clone()));
        }

        loop {
            if !self.status_open && !self.events_open {
                return None;
            }
            tokio::select! {
                result = self.status.changed(), if self.status_open => {
                    match result {
                        Ok(()) => {
                            return Some(TunnelUpdate::Status(self.status.borrow().clone()));
                        }
                        Err(_) => {
                            self.status_open = false;
                        }
                    }
                }
                event = self.events.recv(), if self.events_open => {
                    match event {
                        Ok(event) => return Some(TunnelUpdate::Event(event)),
                        Err(broadcast::error::RecvError::Lagged(count)) => {
                            return Some(TunnelUpdate::Event(TunnelEvent::Error {
                                message: format!(
                                    "event subscriber lagged and lost {count} events"
                                ),
                            }));
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            self.events_open = false;
                        }
                    }
                }
            }
        }
    }
}

/// 单实例托管隧道服务。
///
/// 克隆值共享同一条活动隧道。低层调用方仍可直接使用 [`IrohTunnel`]。
#[derive(Clone)]
pub struct TunnelService {
    inner: Arc<ServiceInner>,
}

struct ServiceInner {
    state: Mutex<ServiceState>,
    status_tx: watch::Sender<TunnelStatus>,
    event_tx: broadcast::Sender<TunnelEvent>,
    operation_next: AtomicU64,
}

enum ServiceState {
    Idle,
    Starting {
        operation_id: u64,
        task: Option<JoinHandle<()>>,
    },
    Active(ActiveTunnel),
    Stopping {
        operation_id: u64,
    },
}

struct ActiveTunnel {
    mode: TunnelMode,
    join_uri: Option<JoinUri>,
    tunnel: Arc<IrohTunnel>,
    events: Option<tokio::sync::mpsc::Receiver<TunnelEvent>>,
    event_task: Option<JoinHandle<()>>,
}

impl Default for TunnelService {
    fn default() -> Self {
        Self::new()
    }
}

impl TunnelService {
    /// 创建空闲的托管隧道服务。
    pub fn new() -> Self {
        let initial_status = TunnelStatus::idle();
        let (status_tx, _) = watch::channel(initial_status);
        let (event_tx, _) = broadcast::channel(EVENT_BROADCAST_SIZE);
        Self {
            inner: Arc::new(ServiceInner {
                state: Mutex::new(ServiceState::Idle),
                status_tx,
                event_tx,
                operation_next: AtomicU64::new(1),
            }),
        }
    }

    /// 在 core 内部启动并托管 Host 任务。
    ///
    /// 返回成功表示任务已被接受；后续失败通过 [`Self::subscribe`] 发布。
    pub async fn start_host(&self, options: HostOptions) -> Result<(), TunnelServiceError> {
        validate_host_options(&options)?;
        let guard = self.begin_start(TunnelMode::Host).await?;
        let operation_id = guard.operation_id;
        let service = self.clone();
        let task = tokio::spawn(async move {
            if let Err(error) = service.clone().complete_host(options, guard).await
                && !matches!(error, TunnelServiceError::Busy)
            {
                service.publish_error("host start failed", &error);
            }
        });
        self.attach_start_task(operation_id, task).await;
        Ok(())
    }

    async fn complete_host(
        &self,
        options: HostOptions,
        guard: OperationGuard,
    ) -> Result<TunnelStatus, TunnelServiceError> {
        let result = IrohTunnel::host(
            options.mc_port,
            options.secret_key,
            options.relay_url,
            options.service_id,
            options.token,
            options.config,
        )
        .await;

        match result {
            Ok((tunnel, join_uri, events)) => {
                let active = ActiveTunnel::new(TunnelMode::Host, Some(join_uri), tunnel, events);
                self.finish_start(guard, active).await
            }
            Err(error) => {
                self.fail_start(guard).await;
                Err(error.into())
            }
        }
    }

    /// 在 core 内部启动并托管 Join 任务。
    ///
    /// 返回成功表示任务已被接受；后续失败通过 [`Self::subscribe`] 发布。
    pub async fn start_join(&self, options: JoinOptions) -> Result<(), TunnelServiceError> {
        validate_join_options(&options)?;
        let guard = self.begin_start(TunnelMode::Join).await?;
        let operation_id = guard.operation_id;
        let service = self.clone();
        let task = tokio::spawn(async move {
            if let Err(error) = service.clone().complete_join(options, guard).await
                && !matches!(error, TunnelServiceError::Busy)
            {
                service.publish_error("join start failed", &error);
            }
        });
        self.attach_start_task(operation_id, task).await;
        Ok(())
    }

    async fn complete_join(
        &self,
        options: JoinOptions,
        guard: OperationGuard,
    ) -> Result<TunnelStatus, TunnelServiceError> {
        let result = IrohTunnel::join(
            &options.join_uri,
            options.local_port.value(),
            options.config,
        )
        .await;

        match result {
            Ok((tunnel, events)) => {
                let active = ActiveTunnel::new(TunnelMode::Join, None, tunnel, events);
                self.finish_start(guard, active).await
            }
            Err(error) => {
                self.fail_start(guard).await;
                Err(error.into())
            }
        }
    }

    /// 关闭活动隧道或取消正在启动的操作。
    ///
    /// 空闲时返回 [`TunnelServiceError::NotRunning`]；取消启动后，即使旧启动 future
    /// 稍后完成，也不能覆盖当前状态，且其新建隧道会立即关闭。
    pub async fn stop(&self) -> Result<TunnelStatus, TunnelServiceError> {
        let Some((active, guard)) = self.begin_stop().await? else {
            return Ok(TunnelStatus::idle());
        };
        active.close().await;
        self.finish_stop(guard).await;
        Ok(TunnelStatus::idle())
    }

    /// 幂等关闭服务，适合应用退出流程。
    pub async fn shutdown(&self) -> Result<(), TunnelServiceError> {
        match self.stop().await {
            Ok(_) | Err(TunnelServiceError::NotRunning) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// 返回无需异步锁的最新完整状态快照。
    pub fn status(&self) -> TunnelStatus {
        self.inner.status_tx.borrow().clone()
    }

    /// 统一订阅状态快照与过程事件。
    pub fn subscribe(&self) -> TunnelSubscription {
        TunnelSubscription {
            status: self.inner.status_tx.subscribe(),
            events: self.inner.event_tx.subscribe(),
            initial_status_pending: true,
            status_open: true,
            events_open: true,
        }
    }

    async fn begin_start(&self, mode: TunnelMode) -> Result<OperationGuard, TunnelServiceError> {
        let mut state = self.inner.state.lock().await;
        if !matches!(*state, ServiceState::Idle) {
            return Err(TunnelServiceError::Busy);
        }
        let operation_id = self.next_operation_id();
        *state = ServiceState::Starting {
            operation_id,
            task: None,
        };
        self.publish(TunnelStatus {
            state: TunnelState::pending(TunnelPhase::Starting, mode),
            connections: Vec::new(),
        });
        Ok(OperationGuard::new(self.clone(), operation_id))
    }

    async fn finish_start(
        &self,
        mut guard: OperationGuard,
        mut active: ActiveTunnel,
    ) -> Result<TunnelStatus, TunnelServiceError> {
        let status = active.status()?;
        let mut state = self.inner.state.lock().await;
        if !state.matches_starting(guard.operation_id) {
            drop(state);
            active.close().await;
            return Err(TunnelServiceError::Busy);
        }
        active.start_events(
            self.inner.event_tx.clone(),
            self.inner.status_tx.clone(),
            status.clone(),
        );
        *state = ServiceState::Active(active);
        self.publish(status.clone());
        guard.disarm();
        Ok(status)
    }

    async fn attach_start_task(&self, operation_id: u64, task: JoinHandle<()>) {
        let mut state = self.inner.state.lock().await;
        match &mut *state {
            ServiceState::Starting {
                operation_id: current,
                task: current_task,
            } if *current == operation_id => {
                assert!(current_task.is_none());
                *current_task = Some(task);
            }
            _ => {
                task.abort();
            }
        }
    }

    async fn fail_start(&self, mut guard: OperationGuard) {
        self.reset_operation(guard.operation_id).await;
        guard.disarm();
    }

    async fn begin_stop(
        &self,
    ) -> Result<Option<(ActiveTunnel, OperationGuard)>, TunnelServiceError> {
        let mut state = self.inner.state.lock().await;
        let operation_id = self.next_operation_id();
        let previous = std::mem::replace(&mut *state, ServiceState::Idle);
        match previous {
            ServiceState::Active(mut active) => {
                let mode = active.mode;
                active.stop_events().await;
                *state = ServiceState::Stopping { operation_id };
                self.publish(TunnelStatus {
                    state: TunnelState::pending(TunnelPhase::Stopping, mode),
                    connections: Vec::new(),
                });
                Ok(Some((
                    active,
                    OperationGuard::new(self.clone(), operation_id),
                )))
            }
            ServiceState::Starting { task, .. } => {
                if let Some(task) = task {
                    task.abort();
                }
                self.publish(TunnelStatus::idle());
                Ok(None)
            }
            other => {
                let error = match other {
                    ServiceState::Idle => TunnelServiceError::NotRunning,
                    ServiceState::Stopping { .. } => TunnelServiceError::Busy,
                    ServiceState::Starting { .. } => unreachable!(),
                    ServiceState::Active(_) => unreachable!(),
                };
                *state = other;
                Err(error)
            }
        }
    }

    async fn finish_stop(&self, mut guard: OperationGuard) {
        self.reset_operation(guard.operation_id).await;
        guard.disarm();
    }

    async fn reset_operation(&self, operation_id: u64) {
        let mut state = self.inner.state.lock().await;
        if state.operation_id() == Some(operation_id) {
            *state = ServiceState::Idle;
            self.publish(TunnelStatus::idle());
        }
    }

    fn next_operation_id(&self) -> u64 {
        self.inner.operation_next.fetch_add(1, Ordering::Relaxed)
    }

    fn publish(&self, status: TunnelStatus) {
        self.inner.status_tx.send_replace(status);
    }

    fn publish_error(&self, context: &str, error: &TunnelServiceError) {
        let _ = self.inner.event_tx.send(TunnelEvent::Error {
            message: format!("{context}: {error}"),
        });
    }
}

impl ServiceState {
    fn operation_id(&self) -> Option<u64> {
        match self {
            Self::Starting { operation_id, .. } | Self::Stopping { operation_id } => {
                Some(*operation_id)
            }
            Self::Idle | Self::Active(_) => None,
        }
    }

    fn matches_starting(&self, operation_id: u64) -> bool {
        matches!(
            self,
            Self::Starting {
                operation_id: current,
                ..
            } if *current == operation_id
        )
    }
}

struct OperationGuard {
    service: TunnelService,
    operation_id: u64,
    armed: bool,
}

impl OperationGuard {
    fn new(service: TunnelService, operation_id: u64) -> Self {
        Self {
            service,
            operation_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let service = self.service.clone();
        let operation_id = self.operation_id;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                service.reset_operation(operation_id).await;
            });
        }
    }
}

impl ActiveTunnel {
    fn new(
        mode: TunnelMode,
        join_uri: Option<JoinUri>,
        tunnel: IrohTunnel,
        events: tokio::sync::mpsc::Receiver<TunnelEvent>,
    ) -> Self {
        Self {
            mode,
            join_uri,
            tunnel: Arc::new(tunnel),
            events: Some(events),
            event_task: None,
        }
    }

    fn start_events(
        &mut self,
        event_tx: broadcast::Sender<TunnelEvent>,
        status_tx: watch::Sender<TunnelStatus>,
        initial_status: TunnelStatus,
    ) {
        assert!(self.event_task.is_none());
        assert!(self.events.is_some());
        let Some(mut events) = self.events.take() else {
            return;
        };
        let tunnel = self.tunnel.clone();
        let forward_tx = event_tx.clone();
        let mut status = initial_status;
        let event_task = tokio::spawn(async move {
            let first_refresh = tokio::time::Instant::now() + STATUS_REFRESH_INTERVAL;
            let mut interval = tokio::time::interval_at(first_refresh, STATUS_REFRESH_INTERVAL);
            let mut events_open = true;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let Ok(connections) = tunnel.connections() else {
                            continue;
                        };
                        if status.connections != connections {
                            status.connections = connections;
                            status_tx.send_replace(status.clone());
                        }
                    }
                    event = events.recv(), if events_open => {
                        match event {
                            Some(event) => {
                                let _ = forward_tx.send(event);
                            }
                            None => {
                                events_open = false;
                            }
                        }
                    }
                }
            }
        });
        self.event_task = Some(event_task);
    }

    fn state(&self) -> TunnelState {
        TunnelState {
            phase: TunnelPhase::Active,
            mode: Some(self.mode),
            join_uri: self.join_uri.clone(),
            local_addr: self.tunnel.local_addr(),
        }
    }

    fn status(&self) -> crate::Result<TunnelStatus> {
        Ok(TunnelStatus {
            state: self.state(),
            connections: self.tunnel.connections()?,
        })
    }

    async fn close(mut self) {
        self.tunnel.close().await;
        self.stop_events().await;
    }

    async fn stop_events(&mut self) {
        if let Some(task) = self.event_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        if let Some(task) = self.event_task.take() {
            task.abort();
        }
    }
}

fn validate_host_options(options: &HostOptions) -> Result<(), TunnelServiceError> {
    if options.mc_port == 0 {
        return Err(TunnelServiceError::InvalidPort);
    }
    if options.config.max_players == Some(0) {
        return Err(TunnelServiceError::InvalidMaxPlayers);
    }
    Ok(())
}

fn validate_join_options(_options: &JoinOptions) -> Result<(), TunnelServiceError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn new_service_is_idle() {
        let service = TunnelService::new();

        let status = service.status();

        assert_eq!(status.state.phase, TunnelPhase::Idle);
        assert!(status.connections.is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_options_before_starting() {
        let service = TunnelService::new();

        let host = service.start_host(HostOptions::new(0)).await;
        assert!(matches!(host, Err(TunnelServiceError::InvalidPort)));
        assert_eq!(service.status().state.phase, TunnelPhase::Idle);
    }

    #[tokio::test]
    async fn dropped_operation_guard_restores_idle() {
        let service = TunnelService::new();
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin start");
        drop(guard);
        tokio::task::yield_now().await;

        assert_eq!(service.status().state.phase, TunnelPhase::Idle);
    }

    #[tokio::test]
    async fn starting_service_rejects_another_start() {
        let service = TunnelService::new();
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin first start");

        let second = service.begin_start(TunnelMode::Join).await;

        assert!(matches!(second, Err(TunnelServiceError::Busy)));
    }

    #[tokio::test]
    async fn stop_cancels_starting_operation() {
        let service = TunnelService::new();
        let mut updates = service.subscribe();
        let initial = updates.recv().await;
        assert!(matches!(
            initial,
            Some(TunnelUpdate::Status(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Idle,
                    ..
                },
                ..
            }))
        ));
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin start");
        let starting = updates.recv().await;
        assert!(matches!(
            starting,
            Some(TunnelUpdate::Status(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Starting,
                    mode: Some(TunnelMode::Host),
                    ..
                },
                ..
            }))
        ));

        let status = service.stop().await;

        assert!(matches!(
            status,
            Ok(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Idle,
                    ..
                },
                ..
            })
        ));
        let idle = updates.recv().await;
        assert!(matches!(
            idle,
            Some(TunnelUpdate::Status(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Idle,
                    ..
                },
                ..
            }))
        ));
        assert_eq!(service.status().state.phase, TunnelPhase::Idle);
        drop(guard);
        tokio::task::yield_now().await;
        assert_eq!(service.status().state.phase, TunnelPhase::Idle);
    }

    #[tokio::test]
    async fn unified_subscription_starts_with_status_and_tracks_changes() {
        let service = TunnelService::new();
        let mut updates = service.subscribe();

        let initial = updates.recv().await;
        assert!(matches!(
            initial,
            Some(TunnelUpdate::Status(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Idle,
                    ..
                },
                ..
            }))
        ));

        let guard = service.begin_start(TunnelMode::Join).await;
        assert!(guard.is_ok(), "begin start");
        let starting = updates.recv().await;
        assert!(matches!(
            starting,
            Some(TunnelUpdate::Status(TunnelStatus {
                state: TunnelState {
                    phase: TunnelPhase::Starting,
                    mode: Some(TunnelMode::Join),
                    ..
                },
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn stop_aborts_managed_start_task() {
        let service = TunnelService::new();
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin start");
        let Ok(guard) = guard else {
            return;
        };
        let operation_id = guard.operation_id;
        let dropped = Arc::new(AtomicBool::new(false));
        let task_flag = DropFlag(dropped.clone());
        let task = tokio::spawn(async move {
            let _guard = guard;
            let _task_flag = task_flag;
            std::future::pending::<()>().await;
        });
        service.attach_start_task(operation_id, task).await;

        let status = service.stop().await;
        assert!(status.is_ok(), "stop managed start");
        let Ok(status) = status else {
            return;
        };
        tokio::task::yield_now().await;

        assert_eq!(status.state.phase, TunnelPhase::Idle);
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn idle_shutdown_is_idempotent() {
        let service = TunnelService::new();

        assert!(service.shutdown().await.is_ok());
        assert_eq!(service.status().state.phase, TunnelPhase::Idle);
    }
}
