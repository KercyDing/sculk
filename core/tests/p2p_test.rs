//! P2P 隧道集成测试
//!
//! 在同一进程内启动 echo server + host + join，验证数据能通过隧道双向传输。

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use sculk::tunnel::{TunnelConfig, TunnelEvent};

/// 启动一个简单的 TCP echo server，收到什么就回什么
async fn echo_server(listener: TcpListener) {
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = stream.write_all(&buf[..n]).await;
                    }
                }
            }
        });
    }
}

/// 分配一个随机可用端口并返回端口号（释放监听器，端口可被复用）
async fn alloc_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// 从事件流中接收指定类型事件，带超时
async fn recv_event(
    rx: &mut tokio::sync::mpsc::Receiver<TunnelEvent>,
    timeout_secs: u64,
) -> TunnelEvent {
    tokio::time::timeout(Duration::from_secs(timeout_secs), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("channel closed")
}

/// 从事件流中接收匹配的事件，跳过 PathChanged 等无关事件
async fn recv_event_matching(
    rx: &mut tokio::sync::mpsc::Receiver<TunnelEvent>,
    timeout_secs: u64,
    mut pred: impl FnMut(&TunnelEvent) -> bool,
) -> TunnelEvent {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timeout waiting for matching event")
            .expect("channel closed");
        if pred(&event) {
            return event;
        }
    }
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)] // CI 无真实网络，跳过
async fn tunnel_echo_roundtrip() {
    // 1. 启动 echo server（模拟 MC 服务端）
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    // 2. Host: 创建隧道
    let (host_tunnel, ticket, mut host_events) =
        sculk::tunnel::IrohTunnel::host(mc_port, None, None, TunnelConfig::default())
            .await
            .unwrap();

    // 3. Join: 用票据连接，监听随机端口
    let local_port = alloc_port().await;

    let (join_tunnel, mut join_events) =
        sculk::tunnel::IrohTunnel::join(&ticket, local_port, TunnelConfig::default())
            .await
            .unwrap();

    // 验证 join 端收到 Connected 事件
    let event = recv_event(&mut join_events, 5).await;
    assert!(matches!(event, TunnelEvent::Connected));

    // 验证 host 端收到 PlayerJoined 事件
    let event = recv_event(&mut host_events, 5).await;
    assert!(matches!(event, TunnelEvent::PlayerJoined { .. }));

    // 等待隧道就绪
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 4. 模拟 MC 客户端连接
    let mut client = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();

    // 5. 发送数据，验证 echo 回来
    let messages = ["hello", "minecraft", "sculk tunnel works!"];
    for msg in &messages {
        client.write_all(msg.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut buf = vec![0u8; msg.len()];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&buf), *msg);
    }

    // 6. 清理
    drop(client);
    join_tunnel.close().await;
    host_tunnel.close().await;
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)]
async fn tunnel_password_correct() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    let config = TunnelConfig {
        password: Some("secret123".into()),
        max_retries: Some(0),
        ..Default::default()
    };

    let (host_tunnel, ticket, mut host_events) =
        sculk::tunnel::IrohTunnel::host(mc_port, None, None, config.clone())
            .await
            .unwrap();

    let local_port = alloc_port().await;
    let (join_tunnel, mut join_events) =
        sculk::tunnel::IrohTunnel::join(&ticket, local_port, config)
            .await
            .unwrap();

    // 验证连接成功
    let event = recv_event(&mut join_events, 10).await;
    assert!(matches!(event, TunnelEvent::Connected));

    let event = recv_event(&mut host_events, 10).await;
    assert!(matches!(event, TunnelEvent::PlayerJoined { .. }));

    join_tunnel.close().await;
    host_tunnel.close().await;
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)]
async fn tunnel_password_wrong() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    let host_config = TunnelConfig {
        password: Some("correct".into()),
        ..Default::default()
    };

    let (host_tunnel, ticket, mut host_events) =
        sculk::tunnel::IrohTunnel::host(mc_port, None, None, host_config)
            .await
            .unwrap();

    let local_port = alloc_port().await;
    let join_config = TunnelConfig {
        password: Some("wrong".into()),
        max_retries: Some(0),
        ..Default::default()
    };

    // join 应该返回 Err（密码错误）
    let err = sculk::tunnel::IrohTunnel::join(&ticket, local_port, join_config)
        .await
        .err()
        .expect("join with wrong password should fail");

    // DNS/网络抖动导致连接失败时跳过后续断言
    let err_msg = err.to_string();
    if !err_msg.contains("auth") {
        eprintln!("skipping: connection failed before auth: {err_msg}");
        host_tunnel.close().await;
        return;
    }

    // host 端应该收到 AuthFailed
    let event = recv_event_matching(&mut host_events, 10, |e| {
        matches!(e, TunnelEvent::AuthFailed { .. })
    })
    .await;
    assert!(matches!(event, TunnelEvent::AuthFailed { .. }));

    host_tunnel.close().await;
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)]
async fn tunnel_max_players() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    let host_config = TunnelConfig {
        max_players: Some(1),
        ..Default::default()
    };

    let (host_tunnel, ticket, mut host_events) =
        sculk::tunnel::IrohTunnel::host(mc_port, None, None, host_config)
            .await
            .unwrap();

    // 第一个 join 应该成功
    let local_port1 = alloc_port().await;
    let join_config = TunnelConfig {
        max_retries: Some(0),
        ..Default::default()
    };
    let (join1, mut join1_events) =
        sculk::tunnel::IrohTunnel::join(&ticket, local_port1, join_config.clone())
            .await
            .unwrap();

    let event = recv_event(&mut join1_events, 10).await;
    assert!(matches!(event, TunnelEvent::Connected));

    let event = recv_event(&mut host_events, 10).await;
    assert!(matches!(event, TunnelEvent::PlayerJoined { .. }));

    // 第二个 join 应该连接后被关闭
    let local_port2 = alloc_port().await;
    let (join2, mut join2_events) =
        sculk::tunnel::IrohTunnel::join(&ticket, local_port2, join_config)
            .await
            .unwrap();

    // host 端应收到 PlayerRejected（跳过 PathChanged 等事件）
    let event = recv_event_matching(&mut host_events, 10, |e| {
        matches!(e, TunnelEvent::PlayerRejected { .. })
    })
    .await;
    assert!(matches!(event, TunnelEvent::PlayerRejected { .. }));

    // join2 端应收到 Disconnected（连接被 host 关闭）
    let event = recv_event_matching(&mut join2_events, 10, |e| {
        matches!(e, TunnelEvent::Disconnected { .. })
    })
    .await;
    assert!(matches!(event, TunnelEvent::Disconnected { .. }));

    join1.close().await;
    join2.close().await;
    host_tunnel.close().await;
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)]
async fn tunnel_no_password_compat() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    // 双方都不设密码
    let config = TunnelConfig {
        max_retries: Some(0),
        ..Default::default()
    };

    let (host_tunnel, ticket, mut host_events) =
        sculk::tunnel::IrohTunnel::host(mc_port, None, None, config.clone())
            .await
            .unwrap();

    let local_port = alloc_port().await;
    let (join_tunnel, mut join_events) =
        sculk::tunnel::IrohTunnel::join(&ticket, local_port, config)
            .await
            .unwrap();

    // 验证正常连接
    let event = recv_event(&mut join_events, 10).await;
    assert!(matches!(event, TunnelEvent::Connected));

    let event = recv_event(&mut host_events, 10).await;
    assert!(matches!(event, TunnelEvent::PlayerJoined { .. }));

    // 验证数据传输
    tokio::time::sleep(Duration::from_secs(1)).await;
    let mut client = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();
    client.write_all(b"ping").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut buf = [0u8; 4];
    client.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    drop(client);
    join_tunnel.close().await;
    host_tunnel.close().await;
}
