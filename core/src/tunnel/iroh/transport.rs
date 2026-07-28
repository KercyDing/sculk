//! 双向字节流桥接：iroh QUIC 双向流与 TCP 连接互转。

use std::time::Duration;

use super::*;

/// 半关闭后等待另一方向排空的超时。
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// 判断桥接错误是否仅表示任一端已经正常或主动断开。
pub(super) fn is_connection_closed(error: &crate::error::SculkError) -> bool {
    let crate::error::SculkError::Tunnel(error) = error else {
        return false;
    };
    let source = match error {
        crate::error::TunnelError::BridgeTcpToQuic(source)
        | crate::error::TunnelError::BridgeQuicToTcp(source) => source,
        _ => return false,
    };
    let Some(error) = source.downcast_ref::<std::io::Error>() else {
        return false;
    };
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
    )
}

/// 在 QUIC 双向流与 TCP 连接之间桥接数据。
///
/// 一方向结束后，等待另一方向剩余数据排空（带超时），避免截断。
pub(super) async fn bridge(
    mut send: SendStream,
    mut recv: RecvStream,
    tcp: TcpStream,
) -> crate::Result<()> {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    tokio::select! {
        r = tokio::io::copy(&mut tcp_read, &mut send) => {
            let _ = send.finish();
            r.map_err(|e| crate::error::TunnelError::BridgeTcpToQuic(e.into()))?;
            // TCP->QUIC 方向结束，等待 QUIC->TCP 方向排空
            match tokio::time::timeout(
                DRAIN_TIMEOUT,
                tokio::io::copy(&mut recv, &mut tcp_write),
            ).await {
                Ok(result) => {
                    result.map_err(|e| {
                        crate::error::TunnelError::BridgeQuicToTcp(e.into())
                    })?;
                }
                Err(_) => tracing::debug!("quic->tcp drain timed out"),
            }
        }
        r = tokio::io::copy(&mut recv, &mut tcp_write) => {
            r.map_err(|e| crate::error::TunnelError::BridgeQuicToTcp(e.into()))?;
            // QUIC->TCP 方向结束，等待 TCP->QUIC 方向排空
            let drain = tokio::time::timeout(
                DRAIN_TIMEOUT,
                tokio::io::copy(&mut tcp_read, &mut send),
            ).await;
            let _ = send.finish();
            match drain {
                Ok(result) => {
                    result.map_err(|e| {
                        crate::error::TunnelError::BridgeTcpToQuic(e.into())
                    })?;
                }
                Err(_) => tracing::debug!("tcp->quic drain timed out"),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge_error(kind: std::io::ErrorKind) -> crate::error::SculkError {
        crate::error::TunnelError::BridgeTcpToQuic(Box::new(std::io::Error::from(kind))).into()
    }

    #[test]
    fn treats_connection_shutdown_as_normal_bridge_end() {
        for kind in [
            std::io::ErrorKind::BrokenPipe,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::NotConnected,
            std::io::ErrorKind::UnexpectedEof,
        ] {
            assert!(is_connection_closed(&bridge_error(kind)));
        }
    }

    #[test]
    fn preserves_unexpected_bridge_errors() {
        assert!(!is_connection_closed(&bridge_error(
            std::io::ErrorKind::Other
        )));
    }
}
