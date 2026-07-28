//! sculk：面向 Minecraft 联机的 P2P 隧道库。
//!
//! 基于 [`iroh`](https://iroh.computer) 提供端到端加密的 QUIC 连接，
//! 封装了 host/join 双端流程、分享 URI、事件流与自动重连能力。
//!
//! # Overview
//!
//! - [`minecraft::probe_server`]：探测本机 Minecraft Java 版服务。
//! - [`minecraft::lan`]：扫描原版 LAN 公告或向本机客户端发布隧道入口。
//! - [`tunnel::IrohTunnel`]：创建 host 或 join 隧道。
//! - [`tunnel::TunnelService`]：托管单条隧道的生命周期、状态与多调用方事件订阅。
//! - [`tunnel::JoinUri`]：`sculk://join/v1/` 分享 URI。
//! - [`tunnel::HostConfig`] / [`tunnel::JoinConfig`]：分端配置。
//! - [`tunnel::TunnelEvent`]：运行时状态与错误事件。
//!
//! # Examples
//!
//! Host 端：
//!
//! ```no_run
//! use sculk::tunnel::{AccessToken, HostConfig, IrohTunnel, ServiceId};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let (_tunnel, uri, _events) = IrohTunnel::host(
//!     25565,
//!     None,
//!     None,
//!     ServiceId::generate(),
//!     AccessToken::generate(),
//!     HostConfig::default(),
//! ).await?;
//! println!("share URI: {}", uri.expose_secret_uri()?);
//! # Ok(())
//! # }
//! ```
//!
//! Join 端：
//!
//! ```no_run
//! use sculk::tunnel::{JoinOptions, JoinUri, TunnelService};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let uri: JoinUri = "sculk://join/v1/<payload>".parse()?;
//! let service = TunnelService::new();
//! service.start_join(JoinOptions::new(uri)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Notes
//!
//! - `HostConfig::max_players` 按唯一 `EndpointId` 计数。
//! - 分享 URI 包含当前会话访问令牌，不应写入日志或持久化配置。
//! - `join` 侧是否自动重连由 `JoinConfig::max_retries` 控制。
//! - 简单集成优先使用 `TunnelService`；需要直接拥有事件接收端时使用 `IrohTunnel`。

pub mod error;
pub mod minecraft;
#[cfg(feature = "persist")]
pub mod persist;
pub mod tunnel;
pub mod types;

pub use error::{ErrorCategory, Result, SculkError};
pub use types::{RelayUrl, SecretKey};

/// Minecraft 服务端标准端口。
pub const DEFAULT_MC_PORT: u16 = 25565;

/// join 端本地入站监听端口默认值。
pub const DEFAULT_INLET_PORT: u16 = 30000;
