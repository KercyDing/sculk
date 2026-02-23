//! 隧道抽象层
//!
//! 基于 iroh QUIC 连接实现 TCP 端口转发隧道。

mod iroh;

pub use iroh::IrohTunnel;
