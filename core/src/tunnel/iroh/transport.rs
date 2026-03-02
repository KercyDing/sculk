//! 双向字节流桥接：iroh QUIC 双向流与 TCP 连接互转。

use super::*;

/// 在 QUIC 双向流与 TCP 连接之间桥接数据，任一方向 EOF 或错误时关闭另一侧。
pub(super) async fn bridge(
    mut send: SendStream,
    mut recv: RecvStream,
    tcp: TcpStream,
) -> crate::Result<()> {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    tokio::select! {
        r = tokio::io::copy(&mut tcp_read, &mut send) => {
            let _ = send.finish();
            r.map_err(|e| crate::error::TunnelError::BridgeTcpToQuic(e.to_string()))?;
        }
        r = tokio::io::copy(&mut recv, &mut tcp_write) => {
            r.map_err(|e| crate::error::TunnelError::BridgeQuicToTcp(e.to_string()))?;
        }
    }

    Ok(())
}
