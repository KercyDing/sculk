//! 隧道事件、配置与连接快照类型

use std::time::{Duration, Instant};

/// 隧道配置
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    /// PathChanged 事件策略。
    ///
    /// - `Duration::ZERO`（默认）：仅在状态实际变化时发送（去重模式）
    /// - 其他值：按此间隔定期发送，relay ↔ direct 切换始终立即发送
    pub event_delay: Duration,
    /// 连接密码，None = 无需密码。host 和 join 端需一致
    pub password: Option<String>,
    /// 最大重连次数 (仅 join 端)。None = 无限, Some(0) = 不重连
    pub max_retries: Option<u32>,
    /// 重连初始退避 (仅 join 端，默认 500ms)
    pub base_backoff: Duration,
    /// 重连最大退避 (仅 join 端，默认 30s)
    pub max_backoff: Duration,
    /// 最大玩家数 (仅 host 端)。None = 无限制
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
    /// 正在重连（join 端）
    Reconnecting { attempt: u32 },
    /// 重连成功（join 端）
    Reconnected,
    /// 密码验证失败（host 端收到）
    AuthFailed { id: String },
    /// 玩家被拒绝（host 端，如人数已满）
    PlayerRejected { id: String, reason: String },
    /// 隧道错误
    Error { message: String },
}

/// 连接信息快照，由 `IrohTunnel::connections()` 返回
#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    /// 对端节点短 ID（用于标识玩家或房主）
    pub remote_id: String,
    /// 当前是否走 relay 中继路径（`false` 表示直连）
    pub is_relay: bool,
    /// 当前路径 RTT（毫秒）
    pub rtt_ms: u64,
    /// 已发送字节数（UDP 统计）
    pub tx_bytes: u64,
    /// 已接收字节数（UDP 统计）
    pub rx_bytes: u64,
    /// 连接当前是否存活
    pub alive: bool,
    /// 快照采集时间，供调用方计算流量速率
    pub timestamp: Instant,
}
