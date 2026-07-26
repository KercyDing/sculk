//! P2P 隧道：基于 iroh QUIC 的 TCP 端口转发层。
//!
//! [`TunnelService`] 为多数上层应用提供托管生命周期、状态和多调用方事件订阅。
//! [`IrohTunnel`] 保留低层单隧道所有权，适合需要直接控制事件接收端的调用方。

mod event;
mod iroh;
mod service;
mod ticket;

pub use crate::types::{RelayUrl, SecretKey};
pub use ::iroh::EndpointId;
pub use event::{ConnectionSnapshot, HostConfig, JoinConfig, PeerId, TunnelEvent};
pub use iroh::IrohTunnel;
pub use service::{
    HostOptions, JoinOptions, TunnelMode, TunnelPhase, TunnelService, TunnelServiceError,
    TunnelState, TunnelStatus, TunnelSubscription, TunnelUpdate,
};
pub use ticket::Ticket;
