//! 隧道配置、运行时事件与连接快照类型。
//!
//! 调用方通过 [`TunnelConfig`] 输入策略，通过 `mpsc` 接收 [`TunnelEvent`]，
//! 并可按需读取 [`ConnectionSnapshot`] 做状态展示或统计。

use std::time::{Duration, Instant};

/// 隧道配置。
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    /// `PathChanged` 发送策略：`ZERO` 仅变化时发送，非零按间隔发送。
    pub event_delay: Duration,
    /// 连接密码，`None` 表示不校验。
    pub password: Option<String>,
    /// 最大重连次数（仅 join 侧）：`None` 无限，`Some(0)` 关闭重连。
    pub max_retries: Option<u32>,
    /// 重连初始退避（仅 join 侧）。
    pub base_backoff: Duration,
    /// 重连最大退避（仅 join 侧）。
    pub max_backoff: Duration,
    /// 最大玩家数（仅 host 侧，按唯一 `EndpointId` 计）。
    pub max_players: Option<u32>,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            event_delay: Duration::ZERO,
            password: None,
            max_retries: None,
            base_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            max_players: None,
        }
    }
}

/// 隧道运行时事件（通过 `mpsc` 推送）。
#[derive(Debug, Clone)]
pub enum TunnelEvent {
    PlayerJoined {
        id: String,
    },
    PlayerLeft {
        id: String,
        reason: String,
    },
    Connected,
    Disconnected {
        reason: String,
    },
    PathChanged {
        remote_id: String,
        is_relay: bool,
        rtt_ms: u64,
    },
    Reconnecting {
        attempt: u32,
    },
    Reconnected,
    AuthFailed {
        id: String,
    },
    PlayerRejected {
        id: String,
        reason: String,
    },
    Error {
        message: String,
    },
}

/// 连接状态快照，由 `IrohTunnel::connections()` 返回。
#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    pub remote_id: String,
    pub is_relay: bool,
    pub rtt_ms: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub alive: bool,
    pub timestamp: Instant,
}
