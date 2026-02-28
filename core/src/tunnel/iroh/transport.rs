use super::*;

/// 双向桥接：双向流 <-> TCP，任一方向断开则关闭
pub(super) async fn bridge(
    mut send: SendStream,
    mut recv: RecvStream,
    tcp: TcpStream,
) -> anyhow::Result<()> {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    tokio::select! {
        r = tokio::io::copy(&mut tcp_read, &mut send) => {
            let _ = send.finish();
            r?;
        }
        r = tokio::io::copy(&mut recv, &mut tcp_write) => {
            r?;
        }
    }

    Ok(())
}
