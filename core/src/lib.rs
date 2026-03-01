//! sculk-core：面向 Minecraft 联机的 P2P 隧道库。
//!
//! 基于 [`iroh`](https://iroh.computer) 提供端到端加密的 QUIC 连接，
//! 封装了 host/join 双端流程、票据编码、事件流与自动重连能力。
//!
//! # Overview
//!
//! - [`tunnel::IrohTunnel`]：创建 host 或 join 隧道。
//! - [`tunnel::Ticket`]：`sculk://` 连接票据（可序列化分享）。
//! - [`tunnel::TunnelConfig`]：密码、重连、人数上限、事件节流等配置。
//! - [`tunnel::TunnelEvent`]：运行时状态与错误事件。
//!
//! # Examples
//!
//! Host 端：
//!
//! ```no_run
//! use sculk_core::tunnel::{IrohTunnel, TunnelConfig};
//!
//! # async fn demo() -> anyhow::Result<()> {
//! let (_tunnel, ticket, mut events) =
//!     IrohTunnel::host(25565, None, None, TunnelConfig::default()).await?;
//! println!("share ticket: {ticket}");
//!
//! while let Some(event) = events.recv().await {
//!     println!("{event:?}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Join 端：
//!
//! ```no_run
//! use sculk_core::tunnel::{IrohTunnel, Ticket, TunnelConfig};
//!
//! # async fn demo() -> anyhow::Result<()> {
//! let ticket: Ticket = "sculk://<endpoint-id>".parse()?;
//! let (_tunnel, mut events) = IrohTunnel::join(&ticket, 30000, TunnelConfig::default()).await?;
//!
//! while let Some(event) = events.recv().await {
//!     println!("{event:?}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Notes
//!
//! - `TunnelConfig::max_players` 按唯一 `EndpointId` 计数。
//! - `TunnelConfig::password` 是应用层校验，不替代传输层加密。
//! - `join` 侧是否自动重连由 `max_retries` 控制。

#[cfg(feature = "clipboard")]
pub mod clipboard;
#[cfg(feature = "persist")]
pub mod persist;
pub mod tunnel;

/// Minecraft 服务端标准端口。
pub const DEFAULT_MC_PORT: u16 = 25565;

/// join 端本地入站监听端口默认值。
pub const DEFAULT_INLET_PORT: u16 = 30000;
