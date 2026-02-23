//! sculk-core: 零特权 P2P 安全隧道引擎
//!
//! 基于 iroh 实现端到端加密的 P2P 隧道，
//! 通过 QUIC + NAT 打洞为 Minecraft 联机提供透明的网络隧道能力。

pub mod tunnel;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 默认 MC 服务端端口
pub const DEFAULT_MC_PORT: u16 = 25565;

/// 默认本地监听端口（玩家端 Inlet）
pub const DEFAULT_INLET_PORT: u16 = 30000;
