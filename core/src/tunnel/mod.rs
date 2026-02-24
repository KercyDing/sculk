//! 隧道抽象层
//!
//! 基于 iroh QUIC 连接实现 TCP 端口转发隧道。

mod event;
mod iroh;
mod ticket;

pub use ::iroh::{RelayUrl, SecretKey};
pub use event::{ConnectionSnapshot, TunnelEvent};
pub use iroh::IrohTunnel;
pub use ticket::Ticket;
