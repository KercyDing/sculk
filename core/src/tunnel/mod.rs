//! P2P 隧道：基于 iroh QUIC 的 TCP 端口转发层。
//!
//! [`TunnelService`] 为多数上层应用提供托管生命周期、状态和多调用方事件订阅。
//! [`IrohTunnel`] 保留低层单隧道所有权，适合需要直接控制事件接收端的调用方。

mod event;
mod iroh;
mod join_uri;
mod service;

pub use crate::types::{AccessToken, RelayUrl, SecretKey, ServiceId};
pub use ::iroh::EndpointId;
pub use event::{ConnectionSnapshot, HostConfig, JoinConfig, PeerId, TunnelEvent};
pub use iroh::{
    HostedServiceHandle, HostedServiceOptions, HostedServiceStatus, IrohTunnel, NodeOptions,
    SculkNode, SculkNodeError, SculkNodeStatus,
};
pub use join_uri::JoinUri;
pub use service::{
    HostOptions, JoinOptions, LocalPort, TunnelMode, TunnelPhase, TunnelService,
    TunnelServiceError, TunnelState, TunnelStatus, TunnelSubscription, TunnelUpdate,
};
