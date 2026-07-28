//! Minecraft Java 版服务探测与局域网发现支持。

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

pub mod lan;

const LEGACY_PING: u8 = 0xFE;
const LEGACY_RESPONSE: u8 = 0xFF;

/// 使用旧版服务器列表探测确认目标端口由 Minecraft Java 版服务监听。
///
/// TCP 连接、读和写均受 `timeout` 限制。目标返回旧版状态响应标记后即视为可用，
/// 不会继续解析服务端描述内容。
pub fn probe_server(addr: SocketAddr, timeout: Duration) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(&[LEGACY_PING])?;

    let mut response = [0_u8; 1];
    stream.read_exact(&mut response)?;
    let _ = stream.shutdown(Shutdown::Both);
    if response[0] != LEGACY_RESPONSE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected Minecraft status response",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};
    use std::thread;

    fn start_server(response: u8) -> io::Result<(SocketAddr, thread::JoinHandle<io::Result<()>>)> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0_u8; 1];
            stream.read_exact(&mut request)?;
            if request != [LEGACY_PING] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected test probe request",
                ));
            }
            stream.write_all(&[response])
        });
        Ok((addr, server))
    }

    fn join_server(server: thread::JoinHandle<io::Result<()>>) -> io::Result<()> {
        server
            .join()
            .map_err(|_| io::Error::other("test server panicked"))?
    }

    #[test]
    fn accepts_minecraft_legacy_status_response() -> io::Result<()> {
        let (addr, server) = start_server(LEGACY_RESPONSE)?;

        probe_server(addr, Duration::from_secs(1))?;
        join_server(server)
    }

    #[test]
    fn rejects_non_minecraft_response() -> io::Result<()> {
        let (addr, server) = start_server(0)?;

        match probe_server(addr, Duration::from_secs(1)) {
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::InvalidData),
            Ok(()) => return Err(io::Error::other("non-Minecraft response was accepted")),
        }
        join_server(server)
    }
}
