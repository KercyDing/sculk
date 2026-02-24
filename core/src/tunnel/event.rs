//! 隧道事件、配置与连接快照类型

use std::time::Duration;

/// 隧道配置
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    /// PathChanged 事件策略。
    ///
    /// - `Duration::ZERO`（默认）：仅在状态实际变化时发送（去重模式）
    /// - 其他值：按此间隔定期发送，relay ↔ direct 切换始终立即发送
    pub event_delay: Duration,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            event_delay: Duration::ZERO,
        }
    }
}

/// 隧道生命周期事件，通过 mpsc channel 推送给调用方
#[derive(Debug, Clone)]
pub enum TunnelEvent {
    /// 玩家连接（host 端收到新 QUIC 连接）
    PlayerJoined { id: String },
    /// 玩家断开（host 端连接关闭）
    PlayerLeft { id: String, reason: String },
    /// 已连接到房主（join 端）
    Connected,
    /// 与房主断开（join 端）
    Disconnected { reason: String },
    /// 网络路径变化（直连/中继切换、延迟变化）
    PathChanged {
        remote_id: String,
        is_relay: bool,
        rtt_ms: u64,
    },
    /// 隧道错误
    Error { message: String },
}

/// 连接信息快照，由 `IrohTunnel::connections()` 返回
#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    pub remote_id: String,
    pub is_relay: bool,
    pub rtt_ms: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub alive: bool,
}
