//! 托管隧道服务：集中处理单实例生命周期、状态快照与多调用方事件订阅。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;
use tokio::sync::{Mutex, broadcast, watch};
use tokio::task::JoinHandle;

use super::{
    ConnectionSnapshot, HostConfig, IrohTunnel, JoinConfig, RelayUrl, SecretKey, Ticket,
    TunnelEvent,
};
use crate::SculkError;

const EVENT_BROADCAST_SIZE: usize = 128;

/// 隧道运行角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TunnelMode {
    Host,
    Join,
}

/// 托管隧道生命周期阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Host 活动时可分享的票据。
    pub ticket: Option<Ticket>,
}

impl TunnelState {
    fn idle() -> Self {
        Self {
            phase: TunnelPhase::Idle,
            mode: None,
            ticket: None,
        }
    }

    fn pending(phase: TunnelPhase, mode: TunnelMode) -> Self {
        Self {
            phase,
            mode: Some(mode),
            ticket: None,
        }
    }
}

/// 包含当前连接指标的完整托管隧道快照。
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TunnelStatus {
    /// 生命周期、角色与票据快照。
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
    /// Host 分享的连接票据。
    pub ticket: Ticket,
    /// 本地 Minecraft 客户端连接端口。
    pub local_port: u16,
    /// Join 行为配置。
    pub config: JoinConfig,
}

impl JoinOptions {
    /// 使用指定票据和本地监听端口创建默认 Join 参数。
    pub fn new(ticket: Ticket, local_port: u16) -> Self {
        Self {
            ticket,
            local_port,
            config: JoinConfig::default(),
        }
    }

    /// 设置 Join 行为配置。
    pub fn config(mut self, config: JoinConfig) -> Self {
        self.config = config;
        self
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

/// 单实例托管隧道服务。
///
/// 克隆值共享同一条活动隧道。低层调用方仍可直接使用 [`IrohTunnel`]。
#[derive(Clone)]
pub struct TunnelService {
    inner: Arc<ServiceInner>,
}

struct ServiceInner {
    state: Mutex<ServiceState>,
    state_tx: watch::Sender<TunnelState>,
    event_tx: broadcast::Sender<TunnelEvent>,
    operation_next: AtomicU64,
}

enum ServiceState {
    Idle,
    Starting { operation_id: u64, mode: TunnelMode },
    Active(ActiveTunnel),
    Stopping { operation_id: u64, mode: TunnelMode },
}

struct ActiveTunnel {
    mode: TunnelMode,
    ticket: Option<Ticket>,
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
        let (state_tx, _) = watch::channel(TunnelState::idle());
        let (event_tx, _) = broadcast::channel(EVENT_BROADCAST_SIZE);
        Self {
            inner: Arc::new(ServiceInner {
                state: Mutex::new(ServiceState::Idle),
                state_tx,
                event_tx,
                operation_next: AtomicU64::new(1),
            }),
        }
    }

    /// 启动 Host；调用 future 被取消时，服务会自动恢复到空闲态。
    pub async fn host(&self, options: HostOptions) -> Result<TunnelStatus, TunnelServiceError> {
        validate_host_options(&options)?;
        let guard = self.begin_start(TunnelMode::Host).await?;
        let result = IrohTunnel::host(
            options.mc_port,
            options.secret_key,
            options.relay_url,
            options.config,
        )
        .await;

        match result {
            Ok((tunnel, ticket, events)) => {
                let active = ActiveTunnel::new(TunnelMode::Host, Some(ticket), tunnel, events);
                self.finish_start(guard, active).await
            }
            Err(error) => {
                self.fail_start(guard).await;
                Err(error.into())
            }
        }
    }

    /// 加入 Host；调用 future 被取消时，服务会自动恢复到空闲态。
    pub async fn join(&self, options: JoinOptions) -> Result<TunnelStatus, TunnelServiceError> {
        validate_join_options(&options)?;
        let guard = self.begin_start(TunnelMode::Join).await?;
        let result = IrohTunnel::join(&options.ticket, options.local_port, options.config).await;

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

    /// 返回包含连接指标的当前完整状态。
    pub async fn status(&self) -> Result<TunnelStatus, TunnelServiceError> {
        let state = self.inner.state.lock().await;
        match &*state {
            ServiceState::Idle => Ok(TunnelStatus::idle()),
            ServiceState::Starting { mode, .. } => Ok(TunnelStatus {
                state: TunnelState::pending(TunnelPhase::Starting, *mode),
                connections: Vec::new(),
            }),
            ServiceState::Stopping { mode, .. } => Ok(TunnelStatus {
                state: TunnelState::pending(TunnelPhase::Stopping, *mode),
                connections: Vec::new(),
            }),
            ServiceState::Active(active) => active.status().map_err(Into::into),
        }
    }

    /// 返回无需异步锁的最新生命周期快照。
    pub fn state(&self) -> TunnelState {
        self.inner.state_tx.borrow().clone()
    }

    /// 订阅权威生命周期状态。`watch` 始终保留最新值。
    pub fn subscribe_state(&self) -> watch::Receiver<TunnelState> {
        self.inner.state_tx.subscribe()
    }

    /// 订阅隧道事件；可在启动前订阅，多个调用方可同时接收。
    ///
    /// 消费者落后超过内部容量时会收到
    /// [`broadcast::error::RecvError::Lagged`]，但不会阻塞隧道网络任务。
    pub fn subscribe_events(&self) -> broadcast::Receiver<TunnelEvent> {
        self.inner.event_tx.subscribe()
    }

    async fn begin_start(&self, mode: TunnelMode) -> Result<OperationGuard, TunnelServiceError> {
        let mut state = self.inner.state.lock().await;
        if !matches!(*state, ServiceState::Idle) {
            return Err(TunnelServiceError::Busy);
        }
        let operation_id = self.next_operation_id();
        *state = ServiceState::Starting { operation_id, mode };
        self.publish(TunnelState::pending(TunnelPhase::Starting, mode));
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
        active.start_events(self.inner.event_tx.clone());
        *state = ServiceState::Active(active);
        self.publish(status.state.clone());
        guard.disarm();
        Ok(status)
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
            ServiceState::Active(active) => {
                let mode = active.mode;
                *state = ServiceState::Stopping { operation_id, mode };
                self.publish(TunnelState::pending(TunnelPhase::Stopping, mode));
                Ok(Some((
                    active,
                    OperationGuard::new(self.clone(), operation_id),
                )))
            }
            ServiceState::Starting { .. } => {
                self.publish(TunnelState::idle());
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
            self.publish(TunnelState::idle());
        }
    }

    fn next_operation_id(&self) -> u64 {
        self.inner.operation_next.fetch_add(1, Ordering::Relaxed)
    }

    fn publish(&self, state: TunnelState) {
        self.inner.state_tx.send_replace(state);
    }
}

impl ServiceState {
    fn operation_id(&self) -> Option<u64> {
        match self {
            Self::Starting { operation_id, .. } | Self::Stopping { operation_id, .. } => {
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
        ticket: Option<Ticket>,
        tunnel: IrohTunnel,
        events: tokio::sync::mpsc::Receiver<TunnelEvent>,
    ) -> Self {
        Self {
            mode,
            ticket,
            tunnel: Arc::new(tunnel),
            events: Some(events),
            event_task: None,
        }
    }

    fn start_events(&mut self, event_tx: broadcast::Sender<TunnelEvent>) {
        assert!(self.event_task.is_none());
        assert!(self.events.is_some());
        let Some(mut events) = self.events.take() else {
            return;
        };
        let forward_tx = event_tx.clone();
        let event_task = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let _ = forward_tx.send(event);
            }
        });
        self.event_task = Some(event_task);
    }

    fn state(&self) -> TunnelState {
        TunnelState {
            phase: TunnelPhase::Active,
            mode: Some(self.mode),
            ticket: self.ticket.clone(),
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

fn validate_join_options(options: &JoinOptions) -> Result<(), TunnelServiceError> {
    if options.local_port == 0 {
        return Err(TunnelServiceError::InvalidPort);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_service_is_idle() {
        let service = TunnelService::new();

        let status = service.status().await;

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
    }

    #[tokio::test]
    async fn rejects_invalid_options_before_starting() {
        let service = TunnelService::new();

        let host = service.host(HostOptions::new(0)).await;
        assert!(matches!(host, Err(TunnelServiceError::InvalidPort)));
        assert_eq!(service.state().phase, TunnelPhase::Idle);
    }

    #[tokio::test]
    async fn dropped_operation_guard_restores_idle() {
        let service = TunnelService::new();
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin start");
        drop(guard);
        tokio::task::yield_now().await;

        assert_eq!(service.state().phase, TunnelPhase::Idle);
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
        let mut states = service.subscribe_state();
        let guard = service.begin_start(TunnelMode::Host).await;
        assert!(guard.is_ok(), "begin start");
        assert!(states.changed().await.is_ok(), "receive starting state");
        assert_eq!(states.borrow().phase, TunnelPhase::Starting);

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
        assert!(states.changed().await.is_ok(), "receive idle state");
        assert_eq!(states.borrow().phase, TunnelPhase::Idle);
        assert_eq!(service.state().phase, TunnelPhase::Idle);
        drop(guard);
        tokio::task::yield_now().await;
        assert_eq!(service.state().phase, TunnelPhase::Idle);
    }

    #[tokio::test]
    async fn idle_shutdown_is_idempotent() {
        let service = TunnelService::new();

        assert!(service.shutdown().await.is_ok());
        assert_eq!(service.state().phase, TunnelPhase::Idle);
    }
}
