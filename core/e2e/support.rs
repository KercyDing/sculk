use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use sculk::tunnel::RelayUrl;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub async fn spawn_relay() -> Option<(RelayUrl, iroh_relay::server::Server)> {
    let mut config = iroh_relay::server::ServerConfig::default();
    config.relay = Some(iroh_relay::server::RelayConfig::new((
        Ipv4Addr::LOCALHOST,
        0,
    )));
    let server = iroh_relay::server::Server::spawn(config).await.ok()?;
    let url = format!("http://{}", server.http_addr()?).parse().ok()?;
    Some((url, server))
}

/// 启动一个会持续接受连接的本地 TCP 回显服务。
pub async fn start_echo_target() -> TestResult<(u16, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0_u8; 5];
                if stream.read_exact(&mut buf).await.is_err() {
                    return;
                }
                let _ = stream.write_all(&buf).await;
            });
        }
    });
    Ok((port, task))
}

pub async fn echo_once(addr: SocketAddr) -> TestResult {
    let mut client = TcpStream::connect(addr).await?;
    client.write_all(b"hello").await?;
    let mut echoed = [0_u8; 5];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed)).await??;
    assert_eq!(&echoed, b"hello");
    Ok(())
}
