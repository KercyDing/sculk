//! Minecraft Java 版局域网公告的扫描与发布。

use super::probe_server;
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::num::NonZeroU16;
use std::sync::mpsc::{self, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// Minecraft 客户端使用数据报来源 IP 作为服务器地址，因此隧道公告只需发往本机。
const LAN_DESTINATION: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4445);
const LAN_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const LAN_PORT: u16 = 4445;
const LAN_INTERVAL: Duration = Duration::from_millis(1_500);
const SCAN_RECEIVE_TIMEOUT: Duration = Duration::from_millis(500);
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const SCAN_PACKET_SIZE_MAX: usize = 8 * 1024;
const MOTD_CHARS_MAX: usize = 64;
const MOTD_FALLBACK: &str = "sculk";

/// 监听 Minecraft Java 版 LAN 公告，并只报告本机确实可连接的服务。
///
/// 扫描器找到第一个通过 Minecraft 探测的本机端口后会结束。丢弃扫描器或调用
/// [`LanScanner::stop`] 会停止后台线程。
pub struct LanScanner {
    ports: mpsc::Receiver<NonZeroU16>,
    stop_tx: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

impl LanScanner {
    /// 在 Minecraft 固定组播地址 `224.0.2.60:4445` 上开始扫描。
    pub fn start() -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, LAN_PORT));
        socket.bind(&addr.into())?;
        let socket: UdpSocket = socket.into();
        socket.join_multicast_v4(&LAN_GROUP, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_read_timeout(Some(SCAN_RECEIVE_TIMEOUT))?;
        Self::start_thread(socket, PROBE_TIMEOUT)
    }

    fn start_thread(socket: UdpSocket, probe_timeout: Duration) -> io::Result<Self> {
        let (port_tx, ports) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("sculk-minecraft-lan-scan".to_owned())
            .spawn(move || scan_loop(socket, probe_timeout, port_tx, stop_rx))?;
        Ok(Self {
            ports,
            stop_tx: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// 非阻塞地读取已发现的本机 Minecraft 端口。
    pub fn try_recv(&self) -> Result<NonZeroU16, TryRecvError> {
        self.ports.try_recv()
    }

    /// 返回后台扫描线程是否已经结束。
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// 停止扫描并等待后台线程退出。
    pub fn stop(mut self) -> io::Result<()> {
        self.stop_inner()
    }

    fn stop_inner(&mut self) -> io::Result<()> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| io::Error::other("LAN scan thread panicked"))?
    }
}

impl Drop for LanScanner {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}

fn scan_loop(
    socket: UdpSocket,
    probe_timeout: Duration,
    ports: mpsc::Sender<NonZeroU16>,
    stop_rx: mpsc::Receiver<()>,
) -> io::Result<()> {
    let mut packet = [0_u8; SCAN_PACKET_SIZE_MAX];
    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }

        match socket.recv_from(&mut packet) {
            Ok((size, _)) => {
                let Some(port) = parse_lan_port(&packet[..size]) else {
                    continue;
                };
                let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port.get()));
                if probe_server(addr, probe_timeout).is_err() {
                    continue;
                }
                let _ = ports.send(port);
                return Ok(());
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error),
        }
    }
}

fn parse_lan_port(packet: &[u8]) -> Option<NonZeroU16> {
    let packet = std::str::from_utf8(packet).ok()?;
    let motd = packet.strip_prefix("[MOTD]")?;
    let motd_end = motd.find("[/MOTD]")? + "[/MOTD]".len();
    let advertisement = motd.get(motd_end..)?.strip_prefix("[AD]")?;
    let port_end = advertisement.find("[/AD]")?;
    advertisement.get(..port_end)?.parse().ok()
}

#[derive(Clone, Copy)]
enum BroadcastMode {
    #[cfg(test)]
    Always,
    Probe(ProbeConfig),
}

#[derive(Clone, Copy)]
struct ProbeConfig {
    addr: SocketAddr,
    interval: Duration,
    timeout: Duration,
}

/// 定期向本机 Minecraft Java 版客户端发布 LAN 公告。
///
/// 广播器会先探测 `port` 上的本机 Minecraft 服务，并每五秒重新探测一次。服务
/// 不可用时暂停公告，恢复后自动继续。丢弃广播器或调用 [`LanBroadcaster::stop`]
/// 会停止后台线程。
pub struct LanBroadcaster {
    stop_tx: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

impl LanBroadcaster {
    /// 为本机 `port` 上的 Minecraft 服务开始发布 LAN 公告。
    pub fn start(name: &str, port: NonZeroU16) -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        Self::start_thread(
            socket,
            SocketAddr::V4(LAN_DESTINATION),
            LAN_INTERVAL,
            build_packet(name, port),
            BroadcastMode::Probe(ProbeConfig {
                addr: SocketAddr::from((Ipv4Addr::LOCALHOST, port.get())),
                interval: PROBE_INTERVAL,
                timeout: PROBE_TIMEOUT,
            }),
        )
    }

    #[cfg(test)]
    fn start_with_socket(
        socket: UdpSocket,
        destination: SocketAddr,
        interval: Duration,
        packet: Vec<u8>,
    ) -> io::Result<Self> {
        Self::start_thread(socket, destination, interval, packet, BroadcastMode::Always)
    }

    #[cfg(test)]
    fn start_with_probe(
        socket: UdpSocket,
        destination: SocketAddr,
        interval: Duration,
        packet: Vec<u8>,
        probe: ProbeConfig,
    ) -> io::Result<Self> {
        Self::start_thread(
            socket,
            destination,
            interval,
            packet,
            BroadcastMode::Probe(probe),
        )
    }

    fn start_thread(
        socket: UdpSocket,
        destination: SocketAddr,
        interval: Duration,
        packet: Vec<u8>,
        mode: BroadcastMode,
    ) -> io::Result<Self> {
        #[cfg(test)]
        if matches!(mode, BroadcastMode::Always) {
            send_packet(&socket, destination, &packet)?;
        }
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("sculk-minecraft-lan-broadcast".to_owned())
            .spawn(move || broadcast_loop(socket, destination, interval, packet, mode, stop_rx))?;
        Ok(Self {
            stop_tx: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// 返回后台广播线程是否已经结束。
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// 停止广播并等待后台线程退出。
    pub fn stop(mut self) -> io::Result<()> {
        self.stop_inner()
    }

    fn stop_inner(&mut self) -> io::Result<()> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| io::Error::other("LAN broadcast thread panicked"))?
    }
}

impl Drop for LanBroadcaster {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}

fn broadcast_loop(
    socket: UdpSocket,
    destination: SocketAddr,
    interval: Duration,
    packet: Vec<u8>,
    mode: BroadcastMode,
    stop_rx: mpsc::Receiver<()>,
) -> io::Result<()> {
    match mode {
        #[cfg(test)]
        BroadcastMode::Always => {
            repeat_broadcast_loop(socket, destination, interval, packet, stop_rx)
        }
        BroadcastMode::Probe(probe) => {
            checked_broadcast_loop(socket, destination, interval, packet, probe, stop_rx)
        }
    }
}

#[cfg(test)]
fn repeat_broadcast_loop(
    socket: UdpSocket,
    destination: SocketAddr,
    interval: Duration,
    packet: Vec<u8>,
    stop_rx: mpsc::Receiver<()>,
) -> io::Result<()> {
    loop {
        match stop_rx.recv_timeout(interval) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                send_packet(&socket, destination, &packet)?;
            }
        }
    }
}

fn checked_broadcast_loop(
    socket: UdpSocket,
    destination: SocketAddr,
    broadcast_interval: Duration,
    packet: Vec<u8>,
    probe: ProbeConfig,
    stop_rx: mpsc::Receiver<()>,
) -> io::Result<()> {
    loop {
        if probe_server(probe.addr, probe.timeout).is_err() {
            match stop_rx.recv_timeout(probe.interval) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }

        let probe_deadline = Instant::now() + probe.interval;
        send_packet(&socket, destination, &packet)?;
        loop {
            let remaining = probe_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let wait = broadcast_interval.min(remaining);
            match stop_rx.recv_timeout(wait) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= probe_deadline {
                        break;
                    }
                    send_packet(&socket, destination, &packet)?;
                }
            }
        }
    }
}

fn send_packet(socket: &UdpSocket, destination: SocketAddr, packet: &[u8]) -> io::Result<()> {
    let size = socket.send_to(packet, destination)?;
    if size != packet.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "incomplete LAN broadcast datagram",
        ));
    }
    Ok(())
}

fn build_packet(name: &str, port: NonZeroU16) -> Vec<u8> {
    format!("[MOTD]{}[/MOTD][AD]{}[/AD]", sanitize_name(name), port).into_bytes()
}

fn sanitize_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .take(MOTD_CHARS_MAX)
        .map(|character| match character {
            '[' | ']' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        MOTD_FALLBACK.to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    type ProbeServer = (SocketAddr, JoinHandle<io::Result<()>>);

    fn test_port() -> io::Result<NonZeroU16> {
        NonZeroU16::new(25_565).ok_or_else(|| io::Error::other("test port is zero"))
    }

    fn start_probe_server(response: u8) -> io::Result<ProbeServer> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0_u8; 1];
            stream.read_exact(&mut request)?;
            if request != [0xFE] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected test probe request",
                ));
            }
            stream.write_all(&[response])
        });
        Ok((addr, thread))
    }

    fn join_probe_server(thread: JoinHandle<io::Result<()>>) -> io::Result<()> {
        thread
            .join()
            .map_err(|_| io::Error::other("test probe server panicked"))?
    }

    fn recv_timeout_is_expected(result: io::Result<(usize, SocketAddr)>) -> io::Result<()> {
        match result {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
            Ok(_) => Err(io::Error::other("unexpected LAN packet")),
        }
    }

    #[test]
    fn builds_java_edition_lan_packet() -> io::Result<()> {
        assert_eq!(
            build_packet("sculk", test_port()?),
            b"[MOTD]sculk[/MOTD][AD]25565[/AD]"
        );
        Ok(())
    }

    #[test]
    fn sanitizes_lan_display_name() {
        assert_eq!(sanitize_name("  房间[一]\n[/MOTD]  "), "房间 一 /MOTD");
        assert_eq!(sanitize_name("\0\n"), MOTD_FALLBACK);

        let long_name = "界".repeat(MOTD_CHARS_MAX + 1);
        assert_eq!(sanitize_name(&long_name).chars().count(), MOTD_CHARS_MAX);
    }

    #[test]
    fn parses_port_from_minecraft_lan_packet() {
        assert_eq!(
            parse_lan_port(b"[MOTD]Local world[/MOTD][AD]32809[/AD]"),
            NonZeroU16::new(32_809)
        );
        assert_eq!(parse_lan_port(b"[MOTD]Local world[/MOTD][AD]0[/AD]"), None);
        assert_eq!(parse_lan_port(b"[AD]32809[/AD]"), None);
        assert_eq!(
            parse_lan_port(b"[MOTD]Local world[/MOTD][AD]invalid[/AD]"),
            None
        );
        assert_eq!(
            parse_lan_port(b"prefix[MOTD]Local world[/MOTD][AD]32809[/AD]"),
            None
        );
    }

    #[test]
    fn starts_on_minecraft_multicast_socket() -> io::Result<()> {
        LanScanner::start()?.stop()
    }

    #[test]
    fn scanner_reports_local_minecraft_server_once() -> io::Result<()> {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        let destination = socket.local_addr()?;
        let scanner = LanScanner::start_thread(socket, Duration::from_millis(100))?;
        let (probe_addr, probe_thread) = start_probe_server(0xFF)?;
        let packet = format!("[MOTD]Local world[/MOTD][AD]{}[/AD]", probe_addr.port());
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        sender.send_to(packet.as_bytes(), destination)?;

        let port = scanner
            .ports
            .recv_timeout(Duration::from_secs(1))
            .map_err(|error| io::Error::other(error.to_string()))?;
        assert_eq!(port.get(), probe_addr.port());
        scanner.stop()?;
        join_probe_server(probe_thread)
    }

    #[test]
    fn scanner_ignores_non_minecraft_server() -> io::Result<()> {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        let destination = socket.local_addr()?;
        let scanner = LanScanner::start_thread(socket, Duration::from_millis(100))?;
        let (probe_addr, probe_thread) = start_probe_server(0)?;
        let packet = format!("[MOTD]Local world[/MOTD][AD]{}[/AD]", probe_addr.port());
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        sender.send_to(packet.as_bytes(), destination)?;

        let result = scanner.ports.recv_timeout(Duration::from_millis(200));
        assert!(matches!(result, Err(mpsc::RecvTimeoutError::Timeout)));
        scanner.stop()?;
        join_probe_server(probe_thread)
    }

    #[test]
    fn broadcasts_immediately_and_repeatedly() -> io::Result<()> {
        let listener = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        listener.set_read_timeout(Some(Duration::from_secs(1)))?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        let destination = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.local_addr()?.port()));
        let packet = build_packet("sculk", test_port()?);
        let broadcaster = LanBroadcaster::start_with_socket(
            sender,
            destination,
            Duration::from_millis(20),
            packet.clone(),
        )?;

        let mut buffer = [0_u8; 128];
        for _ in 0..2 {
            let (size, source) = listener.recv_from(&mut buffer)?;
            assert_eq!(&buffer[..size], packet);
            assert!(source.ip().is_loopback());
        }
        broadcaster.stop()
    }

    #[test]
    fn broadcasts_after_minecraft_probe_succeeds() -> io::Result<()> {
        let listener = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        listener.set_read_timeout(Some(Duration::from_secs(1)))?;
        let destination = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.local_addr()?.port()));
        let (probe_addr, probe_thread) = start_probe_server(0xFF)?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        let packet = build_packet("sculk", test_port()?);
        let probe = ProbeConfig {
            addr: probe_addr,
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(100),
        };
        let broadcaster = LanBroadcaster::start_with_probe(
            sender,
            destination,
            Duration::from_millis(20),
            packet.clone(),
            probe,
        )?;

        let mut buffer = [0_u8; 128];
        let (size, _) = listener.recv_from(&mut buffer)?;
        assert_eq!(&buffer[..size], packet);
        broadcaster.stop()?;
        join_probe_server(probe_thread)
    }

    #[test]
    fn pauses_broadcast_when_minecraft_probe_fails() -> io::Result<()> {
        let listener = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        listener.set_read_timeout(Some(Duration::from_millis(100)))?;
        let destination = SocketAddr::from((Ipv4Addr::LOCALHOST, listener.local_addr()?.port()));
        let (probe_addr, probe_thread) = start_probe_server(0)?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        let probe = ProbeConfig {
            addr: probe_addr,
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(100),
        };
        let broadcaster = LanBroadcaster::start_with_probe(
            sender,
            destination,
            Duration::from_millis(20),
            build_packet("sculk", test_port()?),
            probe,
        )?;

        let mut buffer = [0_u8; 128];
        recv_timeout_is_expected(listener.recv_from(&mut buffer))?;
        broadcaster.stop()?;
        join_probe_server(probe_thread)
    }

    #[test]
    fn stop_interrupts_broadcast_wait() -> io::Result<()> {
        let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_read_timeout(Some(Duration::from_millis(100)))?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        let broadcaster = LanBroadcaster::start_with_socket(
            sender,
            listener.local_addr()?,
            Duration::from_secs(2),
            build_packet("sculk", test_port()?),
        )?;

        let mut buffer = [0_u8; 128];
        listener.recv_from(&mut buffer)?;
        let started = Instant::now();
        broadcaster.stop()?;
        assert!(started.elapsed() < Duration::from_secs(1));

        recv_timeout_is_expected(listener.recv_from(&mut buffer))
    }

    #[test]
    fn stop_interrupts_scanner_wait() -> io::Result<()> {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(20)))?;
        let scanner = LanScanner::start_thread(socket, Duration::from_millis(100))?;

        let started = Instant::now();
        scanner.stop()?;
        assert!(started.elapsed() < Duration::from_secs(1));
        Ok(())
    }
}
